//! Metadata summaries of Core's bounded World observations.
pub(crate) fn plan_tile_count(value: &serde_json::Value) -> u64 {
    value["plan"]["tiles_total"]
        .as_u64()
        .unwrap_or_else(|| value["plan"]["tiles"].as_array().map_or(0, Vec::len) as u64)
}

#[cfg(test)]
mod tests {
    #[test]
    fn compact_core_plans_report_the_actual_tile_count() {
        let value = serde_json::json!({"plan":{"tiles":[],"tiles_total":12,"omitted_tiles":12}});
        assert_eq!(super::plan_tile_count(&value), 12);
        assert_eq!(
            super::plan_tile_count(&serde_json::json!({"plan":{"tiles":[{},{}]}})),
            2
        );
    }
}
