//! Bounded navigation authorization, distinct from captures and read-only views.
use archie_geofabric::exploration::{ExplorationDomain, ImagerySource};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NavigationRequest {
    pub domain: ExplorationDomain,
    pub sources: Vec<ImagerySource>,
    pub imagery_date: Option<String>,
    pub max_zoom: u8,
    pub max_visible_tiles: u32,
    pub max_requests: u32,
    pub max_bytes: u64,
    pub duration_seconds: u32,
    pub signals: bool,
    /// Public Earth Web textured meshes are admitted only for preview in RAM.
    /// Existing serialized sessions cannot acquire this additional provider.
    #[serde(default)]
    pub photogrammetry: bool,
}

impl Default for NavigationRequest {
    fn default() -> Self {
        Self {
            domain: ExplorationDomain::Personal,
            sources: vec![
                ImagerySource::GoogleSatellite,
                ImagerySource::BlueMarble,
                ImagerySource::Sentinel2,
                ImagerySource::Landsat,
                ImagerySource::NaturalColor,
                ImagerySource::Infrared,
                ImagerySource::NightLights,
                ImagerySource::Thermal,
            ],
            imagery_date: None,
            max_zoom: 19,
            max_visible_tiles: 64,
            max_requests: 1800,
            max_bytes: 256 * 1024 * 1024,
            duration_seconds: 3600,
            signals: true,
            photogrammetry: true,
        }
    }
}

impl NavigationRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.sources.is_empty()
            || self.sources.len() > 8
            || self
                .sources
                .iter()
                .enumerate()
                .any(|(i, s)| self.sources[..i].contains(s))
            || self.max_zoom > 19
            || !(1..=96).contains(&self.max_visible_tiles)
            || !(1..=3600).contains(&self.duration_seconds)
            || !(1..=10000).contains(&self.max_requests)
            || !(1024..=1024 * 1024 * 1024).contains(&self.max_bytes)
            || self
                .imagery_date
                .as_deref()
                .is_some_and(|s| !archie_geofabric::exploration::valid_imagery_date(s))
        {
            return Err("invalid navigation sources, date or bounded session limits".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn photogrammetry_requires_new_admission_for_legacy_serialized_requests() {
        let mut value = serde_json::to_value(NavigationRequest::default()).unwrap();
        assert_eq!(value["photogrammetry"], true);
        value.as_object_mut().unwrap().remove("photogrammetry");
        let legacy: NavigationRequest = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(legacy).unwrap()["photogrammetry"],
            false
        );
        value["photogrammetry"] = serde_json::json!(true);
        assert!(serde_json::from_value::<NavigationRequest>(value.clone()).is_ok());
        value["photogrammetry"] = serde_json::json!("true");
        assert!(serde_json::from_value::<NavigationRequest>(value).is_err());
    }
    #[test]
    fn navigation_limits_are_explicit_typed_and_bounded() {
        let mut request = NavigationRequest::default();
        assert!(request.validate().is_ok());
        request.max_visible_tiles = 97;
        assert!(request.validate().is_err());
        request.max_visible_tiles = 64;
        request.duration_seconds = 3601;
        assert!(request.validate().is_err());
        request.duration_seconds = 3600;
        request.sources.push(request.sources[0]);
        assert!(request.validate().is_err());
    }
    #[test]
    fn arbitrary_endpoint_and_unknown_authority_are_not_navigation_fields() {
        let mut value = serde_json::to_value(NavigationRequest::default()).unwrap();
        value["endpoint"] = serde_json::json!("https://example.com");
        assert!(serde_json::from_value::<NavigationRequest>(value).is_err());
    }
}
