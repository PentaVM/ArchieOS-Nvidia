//! Metrics.
//!
//! A small counter/gauge registry with no external dependency. v0.1 exposes metrics
//! through the control socket; a Prometheus exporter can be layered on later without
//! changing how call sites record values.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

const MAX_MODEL_OBSERVATIONS: usize = 64;
const MODEL_OBSERVATION_TTL_MS: i64 = 30_000;

/// A completed request observation, never evidence of a request currently generating.
/// Only the trusted execution receipt's model identity belongs in this record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInferenceObservation {
    pub model_id: String,
    pub state: ModelInferenceState,
    pub observed_at_ms: i64,
    pub expires_at_ms: i64,
    pub output_tokens: Option<u64>,
    pub request_elapsed_ms: Option<u64>,
    /// Provider output usage divided by full request execution time, scaled by 1000.
    pub effective_output_tps_x1000: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelInferenceState {
    RecentCompleted,
}

/// A point-in-time view of every recorded metric.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricSnapshot {
    /// Monotonically increasing totals.
    pub counters: BTreeMap<String, u64>,
    /// Values that can go up and down.
    pub gauges: BTreeMap<String, i64>,
    /// Bounded last completion for each observed model. Consumers expire each row
    /// against `expires_at_ms`; absence does not mean idle or zero throughput.
    #[serde(default)]
    pub model_inference: Vec<ModelInferenceObservation>,
}

impl MetricSnapshot {
    pub fn counter(&self, name: &str) -> u64 {
        self.counters.get(name).copied().unwrap_or(0)
    }

    pub fn gauge(&self, name: &str) -> i64 {
        self.gauges.get(name).copied().unwrap_or(0)
    }
}

/// A cheap, cloneable metrics registry.
///
/// Cloning shares the same underlying storage, so the registry can be handed to every
/// subsystem without threading a reference through every signature.
#[derive(Debug, Clone, Default)]
pub struct Metrics {
    counters: Arc<RwLock<BTreeMap<String, Arc<AtomicU64>>>>,
    gauges: Arc<RwLock<BTreeMap<String, Arc<AtomicI64>>>>,
    model_inference: Arc<RwLock<BTreeMap<String, ModelInferenceObservation>>>,
}

impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add `value` to a counter, creating it at zero if needed.
    pub fn increment(&self, name: &str, value: u64) {
        if let Some(counter) = self.counters.read().ok().and_then(|m| m.get(name).cloned()) {
            counter.fetch_add(value, Ordering::Relaxed);
            return;
        }
        // Slow path: take the write lock and re-check, since another thread may have
        // created the counter between our read and write.
        if let Ok(mut map) = self.counters.write() {
            map.entry(name.to_string())
                .or_insert_with(|| Arc::new(AtomicU64::new(0)))
                .fetch_add(value, Ordering::Relaxed);
        }
    }

    pub fn count(&self, name: &str) {
        self.increment(name, 1);
    }

    /// Set a gauge to an absolute value.
    pub fn set_gauge(&self, name: &str, value: i64) {
        if let Some(gauge) = self.gauges.read().ok().and_then(|m| m.get(name).cloned()) {
            gauge.store(value, Ordering::Relaxed);
            return;
        }
        if let Ok(mut map) = self.gauges.write() {
            map.entry(name.to_string())
                .or_insert_with(|| Arc::new(AtomicI64::new(0)))
                .store(value, Ordering::Relaxed);
        }
    }

    /// Store sanitized measurements attributed to an actual completed receipt.
    /// No prompt, task/domain/account identifier, command or provider secret is accepted.
    /// Out-of-order completions cannot replace a newer observation of the same model.
    pub fn observe_model_inference(
        &self,
        model_id: &str,
        observed_at_ms: i64,
        output_tokens: Option<u64>,
        request_elapsed_ms: Option<u64>,
        effective_output_tps_x1000: Option<u64>,
    ) -> bool {
        if model_id.is_empty()
            || model_id.len() > 128
            || model_id
                .chars()
                .any(|c| c.is_control() || c.is_whitespace())
            || observed_at_ms <= 0
        {
            return false;
        }
        let Some(expires_at_ms) = observed_at_ms.checked_add(MODEL_OBSERVATION_TTL_MS) else {
            return false;
        };
        let Ok(mut rows) = self.model_inference.write() else {
            return false;
        };
        if rows
            .get(model_id)
            .is_some_and(|row| row.observed_at_ms > observed_at_ms)
        {
            return false;
        }
        if !rows.contains_key(model_id) && rows.len() >= MAX_MODEL_OBSERVATIONS {
            let Some(oldest) = rows
                .values()
                .min_by_key(|row| (row.observed_at_ms, &row.model_id))
            else {
                return false;
            };
            if observed_at_ms < oldest.observed_at_ms {
                return false;
            }
            let oldest_id = oldest.model_id.clone();
            rows.remove(&oldest_id);
        }
        // Keep values representable in the existing signed-gauge contract. Overflow
        // is missing measurement, not a saturated or wrapped throughput claim.
        let available = |value: Option<u64>| value.filter(|value| i64::try_from(*value).is_ok());
        let output_tokens = available(output_tokens);
        let request_elapsed_ms = available(request_elapsed_ms);
        let effective_output_tps_x1000 = output_tokens
            .zip(request_elapsed_ms)
            .and_then(|_| available(effective_output_tps_x1000));
        rows.insert(
            model_id.into(),
            ModelInferenceObservation {
                model_id: model_id.into(),
                state: ModelInferenceState::RecentCompleted,
                observed_at_ms,
                expires_at_ms,
                output_tokens,
                request_elapsed_ms,
                effective_output_tps_x1000,
            },
        );
        true
    }

    pub fn snapshot(&self) -> MetricSnapshot {
        let counters = self
            .counters
            .read()
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.load(Ordering::Relaxed)))
                    .collect()
            })
            .unwrap_or_default();
        let gauges = self
            .gauges
            .read()
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.load(Ordering::Relaxed)))
                    .collect()
            })
            .unwrap_or_default();
        let model_inference = self
            .model_inference
            .read()
            .map(|rows| rows.values().cloned().collect())
            .unwrap_or_default();
        MetricSnapshot {
            counters,
            gauges,
            model_inference,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_accumulate() {
        let m = Metrics::new();
        m.count("boot.attempts");
        m.count("boot.attempts");
        m.increment("boot.attempts", 3);
        assert_eq!(m.snapshot().counter("boot.attempts"), 5);
    }

    #[test]
    fn unknown_metrics_read_as_zero() {
        let snapshot = Metrics::new().snapshot();
        assert_eq!(snapshot.counter("nope"), 0);
        assert_eq!(snapshot.gauge("nope"), 0);
    }

    #[test]
    fn gauges_move_in_both_directions() {
        let m = Metrics::new();
        m.set_gauge("fabric.peers", 3);
        assert_eq!(m.snapshot().gauge("fabric.peers"), 3);
        m.set_gauge("fabric.peers", 1);
        assert_eq!(m.snapshot().gauge("fabric.peers"), 1);
        m.set_gauge("fabric.peers", -1);
        assert_eq!(m.snapshot().gauge("fabric.peers"), -1);
    }

    #[test]
    fn clones_share_storage() {
        let a = Metrics::new();
        let b = a.clone();
        a.count("shared");
        b.count("shared");
        assert_eq!(a.snapshot().counter("shared"), 2);
    }

    #[test]
    fn concurrent_increments_do_not_lose_counts() {
        let m = Metrics::new();
        let mut handles = Vec::new();
        for _ in 0..8 {
            let m = m.clone();
            handles.push(std::thread::spawn(move || {
                for _ in 0..1_000 {
                    m.count("hot");
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(m.snapshot().counter("hot"), 8_000);
    }

    #[test]
    fn snapshot_round_trips_through_json() {
        let m = Metrics::new();
        m.count("a");
        m.set_gauge("b", 7);
        let snapshot = m.snapshot();
        let json = serde_json::to_string(&snapshot).unwrap();
        assert_eq!(
            serde_json::from_str::<MetricSnapshot>(&json).unwrap(),
            snapshot
        );
    }

    #[test]
    fn old_snapshots_deserialize_without_per_model_observations() {
        let snapshot: MetricSnapshot =
            serde_json::from_str(r#"{"counters":{},"gauges":{}}"#).unwrap();
        assert!(snapshot.model_inference.is_empty());
    }

    #[test]
    fn per_model_completions_keep_attribution_and_missing_usage() {
        let metrics = Metrics::new();
        assert!(metrics.observe_model_inference(
            "model/a",
            100,
            Some(120),
            Some(2500),
            Some(48000)
        ));
        assert!(metrics.observe_model_inference("model/b", 101, None, Some(200), None));
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.model_inference.len(), 2);
        let a = &snapshot.model_inference[0];
        assert_eq!(a.model_id, "model/a");
        assert_eq!(a.output_tokens, Some(120));
        assert_eq!(a.effective_output_tps_x1000, Some(48000));
        assert_eq!(a.expires_at_ms, 30100);
        assert_eq!(
            serde_json::to_value(a).unwrap()["state"],
            "recent_completed"
        );
        let b = &snapshot.model_inference[1];
        assert_eq!(b.output_tokens, None);
        assert_eq!(b.effective_output_tps_x1000, None);
        assert_eq!(b.request_elapsed_ms, Some(200));
    }

    #[test]
    fn model_observations_reject_invalid_identity_and_time_without_growth() {
        let metrics = Metrics::new();
        for id in ["", "model\nsecret", "model account", &"a".repeat(129)] {
            assert!(!metrics.observe_model_inference(id, 1, None, None, None));
        }
        for at in [0, -1, i64::MAX] {
            assert!(!metrics.observe_model_inference("model/a", at, None, None, None));
        }
        assert!(metrics.snapshot().model_inference.is_empty());
    }

    #[test]
    fn bounded_model_observations_evict_oldest_and_ignore_delayed_completions() {
        let metrics = Metrics::new();
        for i in 0..65 {
            metrics.observe_model_inference(
                &format!("model/{i:02}"),
                i + 1,
                Some(1),
                Some(1),
                Some(1),
            );
        }
        let rows = metrics.snapshot().model_inference;
        assert_eq!(rows.len(), 64);
        assert!(!rows.iter().any(|row| row.model_id == "model/00"));
        assert!(!metrics.observe_model_inference("model/64", 2, Some(9), None, None));
        assert!(!metrics.observe_model_inference("too-old", 1, None, None, None));
        assert_eq!(
            metrics
                .snapshot()
                .model_inference
                .last()
                .unwrap()
                .output_tokens,
            Some(1)
        );
    }

    #[test]
    fn parallel_models_do_not_overwrite_each_others_usage_and_overflow_is_absent() {
        let metrics = Metrics::new();
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let metrics = metrics.clone();
                std::thread::spawn(move || {
                    metrics.observe_model_inference(
                        &format!("model/{i}"),
                        100 + i,
                        Some(i as u64),
                        Some(2),
                        Some(u64::MAX),
                    );
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        let rows = metrics.snapshot().model_inference;
        assert_eq!(rows.len(), 8);
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(row.output_tokens, Some(i as u64));
            assert_eq!(row.effective_output_tps_x1000, None);
        }
        let json = serde_json::to_string(&metrics.snapshot()).unwrap();
        assert_eq!(
            serde_json::from_str::<MetricSnapshot>(&json).unwrap(),
            metrics.snapshot()
        );
    }
}
