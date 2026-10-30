use archie_memory::inference::InferenceStatus;
use std::time::Duration;

/// Record one new Core AiInfer execution, without inspecting activity or Policy.
/// The caller measures the full `service.execute` wall time (context, provider
/// prefill/generation/transport, and result persistence), not generation-only
/// time. Tokens must come from provider-reported usage; absent usage stays absent.
///
/// TPS is scaled by 1000 because Metrics gauges are integers. An unavailable or
/// unrepresentable measurement has gauge -1 and availability 0; the registry's
/// convenience `gauge()` accessor defaults missing keys to zero, so consumers
/// must check key presence and the corresponding availability flag. Recall and
/// Harness receipt timers must not be fed into this execution-duration series.
pub(super) fn record_inference_metrics(
    metrics: &archie_observability::Metrics,
    receipt_model: Option<(&str, InferenceStatus)>,
    output_tokens: Option<u64>,
    elapsed: Option<Duration>,
    observed_at_ms: i64,
) {
    let tokens = output_tokens.and_then(|value| i64::try_from(value).ok());
    let elapsed_ms = elapsed.and_then(|value| i64::try_from(value.as_millis()).ok());
    // Use the measured nanoseconds for division, so a genuine sub-millisecond
    // response does not acquire a fake zero duration in the throughput formula.
    // Integer division rounds down to the nearest milli-token per second.
    let effective_tps_x1000 =
        tokens
            .zip(elapsed)
            .zip(elapsed_ms)
            .and_then(|((tokens, duration), _)| {
                u128::try_from(tokens)
                    .ok()?
                    .checked_mul(1_000_000_000_000)?
                    .checked_div(duration.as_nanos())
                    .and_then(|value| i64::try_from(value).ok())
            });
    for (value_name, availability_name, value) in [
        ("output_tokens", "output_tokens_available", tokens),
        (
            "request_elapsed_ms",
            "request_elapsed_ms_available",
            elapsed_ms,
        ),
        (
            "effective_output_tokens_per_second_x1000",
            "effective_output_tps_available",
            effective_tps_x1000,
        ),
    ] {
        metrics.set_gauge(&format!("inference.last.{value_name}"), value.unwrap_or(-1));
        metrics.set_gauge(
            &format!("inference.last.{availability_name}"),
            i64::from(value.is_some()),
        );
    }
    metrics.set_gauge("inference.last.observed_at_ms", observed_at_ms);
    metrics.count("inference.executions.observed");
    // Runtime Ok can contain Failed or Cancelled receipts. Only Completed may
    // create/refresh a recent_completed row; all attempts keep the legacy globals.
    if let Some((model_id, InferenceStatus::Completed)) = receipt_model {
        metrics.observe_model_inference(
            model_id,
            observed_at_ms,
            tokens.and_then(|value| u64::try_from(value).ok()),
            elapsed_ms.and_then(|value| u64::try_from(value).ok()),
            effective_tps_x1000.and_then(|value| u64::try_from(value).ok()),
        );
    }
}

#[cfg(test)]
mod inference_metrics_tests {
    type TestObservation = (Option<u64>, Option<Duration>);
    use archie_memory::inference::InferenceStatus;
    use archie_observability::Metrics;
    use std::time::Duration;

    fn record_inference_metrics(metrics: &Metrics, usage: &TestObservation, observed_at_ms: i64) {
        super::record_inference_metrics(
            metrics,
            Some(("receipt/test-model", InferenceStatus::Completed)),
            usage.0,
            usage.1,
            observed_at_ms,
        );
    }

    fn observed(output_tokens: Option<u64>, duration_ms: Option<u64>) -> TestObservation {
        (output_tokens, duration_ms.map(Duration::from_millis))
    }

    #[test]
    fn records_provider_output_tokens_and_measured_effective_execution_rate() {
        let metrics = Metrics::new();
        record_inference_metrics(&metrics, &observed(Some(120), Some(2500)), 1700000000000);
        let gauges = metrics.snapshot().gauges;
        assert_eq!(gauges.get("inference.last.output_tokens"), Some(&120));
        assert_eq!(gauges.get("inference.last.request_elapsed_ms"), Some(&2500));
        assert_eq!(
            gauges.get("inference.last.effective_output_tokens_per_second_x1000"),
            Some(&48000)
        );
        assert_eq!(
            gauges.get("inference.last.output_tokens_available"),
            Some(&1)
        );
        assert_eq!(
            gauges.get("inference.last.request_elapsed_ms_available"),
            Some(&1)
        );
        assert_eq!(
            gauges.get("inference.last.effective_output_tps_available"),
            Some(&1)
        );
        assert_eq!(
            gauges.get("inference.last.observed_at_ms"),
            Some(&1700000000000)
        );
        assert!(!gauges.contains_key("inference.last.generation_tokens_per_second"));
    }

    #[test]
    fn missing_usage_invalidates_old_measurement_without_inventing_zero() {
        let metrics = Metrics::new();
        assert!(!metrics
            .snapshot()
            .gauges
            .contains_key("inference.last.output_tokens"));
        record_inference_metrics(&metrics, &observed(Some(120), Some(2500)), 1700000000000);
        record_inference_metrics(&metrics, &(None, None), 1700000000001);
        let gauges = metrics.snapshot().gauges;
        for name in [
            "output_tokens",
            "request_elapsed_ms",
            "effective_output_tokens_per_second_x1000",
        ] {
            assert_eq!(gauges.get(&format!("inference.last.{name}")), Some(&-1));
        }
        for name in [
            "output_tokens_available",
            "request_elapsed_ms_available",
            "effective_output_tps_available",
        ] {
            assert_eq!(gauges.get(&format!("inference.last.{name}")), Some(&0));
        }
    }

    #[test]
    fn zero_elapsed_or_missing_tokens_makes_throughput_unavailable() {
        for usage in [observed(Some(12), Some(0)), observed(None, Some(250))] {
            let metrics = Metrics::new();
            record_inference_metrics(&metrics, &usage, 1700000000000);
            let gauges = metrics.snapshot().gauges;
            assert_eq!(
                gauges.get("inference.last.effective_output_tokens_per_second_x1000"),
                Some(&-1)
            );
            assert_eq!(
                gauges.get("inference.last.effective_output_tps_available"),
                Some(&0)
            );
            assert_eq!(
                gauges.get("inference.last.request_elapsed_ms_available"),
                Some(&1)
            );
        }
    }

    #[test]
    fn reported_zero_tokens_is_distinct_from_missing_usage() {
        let metrics = Metrics::new();
        record_inference_metrics(&metrics, &observed(Some(0), Some(250)), 1700000000000);
        let gauges = metrics.snapshot().gauges;
        assert_eq!(gauges.get("inference.last.output_tokens"), Some(&0));
        assert_eq!(
            gauges.get("inference.last.output_tokens_available"),
            Some(&1)
        );
        assert_eq!(
            gauges.get("inference.last.effective_output_tokens_per_second_x1000"),
            Some(&0)
        );
        assert_eq!(
            gauges.get("inference.last.effective_output_tps_available"),
            Some(&1)
        );
    }

    #[test]
    fn inference_observations_are_recorded_while_other_applications_are_active() {
        let metrics = Metrics::new();
        metrics.set_gauge("activity.user_active", 1);
        metrics.set_gauge("activity.application_count", 6);
        record_inference_metrics(&metrics, &observed(Some(120), Some(2500)), 1700000000000);
        let gauges = metrics.snapshot().gauges;
        assert_eq!(gauges.get("inference.last.output_tokens"), Some(&120));
        assert_eq!(
            gauges.get("inference.last.effective_output_tps_available"),
            Some(&1)
        );
        assert_eq!(gauges.get("activity.user_active"), Some(&1));
        assert_eq!(gauges.get("activity.application_count"), Some(&6));
    }

    #[test]
    fn out_of_range_observations_stay_unavailable_instead_of_wrapping() {
        let metrics = Metrics::new();
        record_inference_metrics(&metrics, &observed(Some(u64::MAX), Some(1)), 1700000000000);
        let gauges = metrics.snapshot().gauges;
        assert_eq!(gauges.get("inference.last.output_tokens"), Some(&-1));
        assert_eq!(
            gauges.get("inference.last.output_tokens_available"),
            Some(&0)
        );
        assert_eq!(
            gauges.get("inference.last.effective_output_tps_available"),
            Some(&0)
        );
    }

    #[test]
    fn per_model_rows_use_only_an_available_receipt_identity() {
        let metrics = Metrics::new();
        super::record_inference_metrics(
            &metrics,
            Some(("receipt/model-a", InferenceStatus::Completed)),
            Some(120),
            Some(Duration::from_millis(2500)),
            1700000000000,
        );
        super::record_inference_metrics(
            &metrics,
            Some(("receipt/model-b", InferenceStatus::Completed)),
            None,
            Some(Duration::from_millis(900)),
            1700000000001,
        );
        super::record_inference_metrics(
            &metrics,
            None,
            Some(999),
            Some(Duration::from_millis(1)),
            1700000000002,
        );
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.model_inference.len(), 2);
        let a = &snapshot.model_inference[0];
        assert_eq!(a.model_id, "receipt/model-a");
        assert_eq!(a.output_tokens, Some(120));
        assert_eq!(a.effective_output_tps_x1000, Some(48000));
        assert_eq!(snapshot.model_inference[1].output_tokens, None);
        assert_eq!(snapshot.model_inference[1].effective_output_tps_x1000, None);
        assert_eq!(snapshot.gauge("inference.executions.observed"), 0);
        assert_eq!(snapshot.counter("inference.executions.observed"), 3);
        assert_eq!(snapshot.gauge("inference.last.output_tokens"), 999);
    }

    #[test]
    fn only_completed_receipts_create_recent_completed_rows_but_all_attempts_update_globals() {
        for status in [
            InferenceStatus::Completed,
            InferenceStatus::Failed,
            InferenceStatus::Cancelled,
        ] {
            let metrics = Metrics::new();
            super::record_inference_metrics(
                &metrics,
                Some(("receipt/model", status)),
                Some(12),
                Some(Duration::from_millis(250)),
                1700000000000,
            );
            let snapshot = metrics.snapshot();
            assert_eq!(
                snapshot.model_inference.len(),
                usize::from(status == InferenceStatus::Completed)
            );
            assert_eq!(snapshot.counter("inference.executions.observed"), 1);
            assert_eq!(snapshot.gauge("inference.last.output_tokens"), 12);
            assert_eq!(snapshot.gauge("inference.last.request_elapsed_ms"), 250);
            assert_eq!(
                snapshot.gauge("inference.last.effective_output_tokens_per_second_x1000"),
                48000
            );
        }
    }

    #[test]
    fn failed_and_cancelled_receipts_cannot_refresh_an_old_completion() {
        let metrics = Metrics::new();
        super::record_inference_metrics(
            &metrics,
            Some(("receipt/model", InferenceStatus::Completed)),
            Some(120),
            Some(Duration::from_millis(2500)),
            1700000000000,
        );
        let completed = metrics.snapshot().model_inference;
        for status in [InferenceStatus::Failed, InferenceStatus::Cancelled] {
            super::record_inference_metrics(
                &metrics,
                Some(("receipt/model", status)),
                None,
                Some(Duration::from_millis(900)),
                1700000001000,
            );
        }
        assert_eq!(metrics.snapshot().model_inference, completed);
        assert_eq!(
            metrics.snapshot().counter("inference.executions.observed"),
            3
        );
        assert_eq!(
            metrics
                .snapshot()
                .gauge("inference.last.output_tokens_available"),
            0
        );
    }
}
