//! Bounded observation adapters, without network or command authority.
//! Formats and source terms verified 2026-10-24 against primary sources:
//! USGS: https://earthquake.usgs.gov/earthquakes/feed/v1.0/geojson.php
//! USGS government material: https://www.usgs.gov/information-policies-and-instructions/copyrights-and-credits
//! NASA EONET: https://eonet.gsfc.nasa.gov/docs/v3 and /what-is-eonet
//! (curated approximate extents for visualization/general information).
//! ADS-B: https://api.adsb.lol/api/openapi.json (public API data ODbL 1.0),
//! https://www.adsb.lol/docs/privacy-license . Receiver coverage is incomplete;
//! positions/altitudes are reports, not verified aircraft identity or safety advice.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalRequest {
    pub provider: String,
    pub url: String,
    pub label: String,
    pub ttl_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalBatch {
    pub points: Vec<WorldSignal>,
    pub seen: usize,
    /// Unique valid observations in the requested area, before the output cap.
    #[serde(default)]
    pub matched_total: usize,
    #[serde(default)]
    pub omitted_points: usize,
    /// The bounded input scan cannot establish a complete total beyond its cap.
    #[serde(default)]
    pub total_is_lower_bound: bool,
    pub rejected: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldSignal {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub latitude: f64,
    pub longitude: f64,
    pub altitude_m: Option<f64>,
    pub observed_at_ms: Option<i64>,
    pub source_id: String,
    pub source_url: String,
    /// Zero denotes unassessed display severity; no hazard/threat score is inferred.
    pub severity: f64,
    pub properties: BTreeMap<String, Value>,
}

const MAX_INPUT_BYTES: usize = 2 * 1024 * 1024;
const MAX_INPUT_ITEMS: usize = 4096;
const MAX_POINTS: usize = 128;
const EONET_RESPONSE_LIMIT: usize = 200;
const DAY: i64 = 86_400_000;
// Independent feed/receiver clocks may differ slightly. Keep the original time
// and disclose a positive lead; larger future frames still fail closed.
const AIRCRAFT_CLOCK_TOLERANCE_MS: i64 = 5_000;

fn valid_aoi(lat: f64, lon: f64, radius: f64) -> bool {
    lat.is_finite()
        && lon.is_finite()
        && radius.is_finite()
        && lat.abs() <= 90.0
        && lon.abs() <= 180.0
        && radius > 0.0
        && radius <= 100_000.0
}

pub fn requests(latitude: f64, longitude: f64, radius_m: f64) -> Vec<SignalRequest> {
    if !valid_aoi(latitude, longitude, radius_m) {
        return vec![];
    }
    let angular = radius_m / 6_371_008.8;
    let south = (latitude.to_radians() - angular).to_degrees();
    let north = (latitude.to_radians() + angular).to_degrees();
    let mut eonet = format!(
        "https://eonet.gsfc.nasa.gov/api/v3/events?status=open&days=30&limit={EONET_RESPONSE_LIMIT}"
    );
    if south > -90.0 && north < 90.0 {
        let delta = (angular.sin() / latitude.to_radians().cos())
            .clamp(-1.0, 1.0)
            .asin()
            .to_degrees();
        let west = longitude - delta;
        let east = longitude + delta;
        if west >= -180.0 && east <= 180.0 {
            eonet.push_str(&format!("&bbox={west:.7},{north:.7},{east:.7},{south:.7}"));
        }
    }
    [
        (
            "usgs_earthquakes",
            "USGS · last 24 hours",
            "https://earthquake.usgs.gov/earthquakes/feed/v1.0/summary/all_day.geojson".to_owned(),
            60,
        ),
        (
            "nasa_eonet",
            "NASA EONET · reported natural events",
            eonet,
            600,
        ),
        (
            "adsb_lol",
            "adsb.lol · reported aircraft",
            format!(
                "https://api.adsb.lol/v2/lat/{latitude:.6}/lon/{longitude:.6}/dist/{}",
                (radius_m / 1852.0).ceil().clamp(1.0, 54.0) as u32
            ),
            15,
        ),
    ]
    .into_iter()
    .map(|(provider, label, url, ttl_seconds)| SignalRequest {
        provider: provider.into(),
        label: label.into(),
        url,
        ttl_seconds,
    })
    .collect()
}

/// Fixed, bounded public event feeds for globe-scale display. Aircraft feeds
/// remain local because receiver queries require an explicit bounded area.
pub fn global_requests() -> Vec<SignalRequest> {
    [
        (
            "usgs_earthquakes",
            "USGS · last 24 hours",
            "https://earthquake.usgs.gov/earthquakes/feed/v1.0/summary/all_day.geojson".to_owned(),
            60,
        ),
        (
            "nasa_eonet",
            "NASA EONET · reported natural events",
            format!("https://eonet.gsfc.nasa.gov/api/v3/events?status=open&days=30&limit={EONET_RESPONSE_LIMIT}"),
            600,
        ),
    ]
    .into_iter()
    .map(|(provider, label, url, ttl_seconds)| SignalRequest {
        provider: provider.into(),
        label: label.into(),
        url,
        ttl_seconds,
    })
    .collect()
}

/// Decode global natural events with the same validation, deduplication and
/// caps as local observations, without an AOI filter. Performs no I/O.
pub fn decode_global(provider: &str, bytes: &[u8]) -> Result<SignalBatch, String> {
    decode_at(provider, bytes, None, now_ms()?)
}

pub fn decode(
    provider: &str,
    bytes: &[u8],
    lat: f64,
    lon: f64,
    radius_m: f64,
) -> Result<SignalBatch, String> {
    decode_at(provider, bytes, Some((lat, lon, radius_m)), now_ms()?)
}

fn now_ms() -> Result<i64, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "system time unavailable")?;
    i64::try_from(now.as_millis()).map_err(|_| "system time out of range".into())
}

fn decode_at(
    provider: &str,
    bytes: &[u8],
    area: Option<(f64, f64, f64)>,
    now: i64,
) -> Result<SignalBatch, String> {
    if area.is_none() && !matches!(provider, "usgs_earthquakes" | "nasa_eonet") {
        return Err("global signals support only USGS earthquakes and NASA EONET; aircraft require a bounded local area".into());
    }
    if area.is_some_and(|(lat, lon, radius)| !valid_aoi(lat, lon, radius))
        || now < 0
        || bytes.len() > MAX_INPUT_BYTES
    {
        return Err("invalid signal area, clock or response size".into());
    }
    let root: Value = serde_json::from_slice(bytes).map_err(|_| "invalid signal JSON")?;
    let (key, frame) = match provider {
        "usgs_earthquakes" => {
            if root["type"] != "FeatureCollection" {
                return Err("invalid USGS envelope".into());
            }
            ("features", None)
        }
        "nasa_eonet" => ("events", None),
        "adsb_lol" => {
            let frame = root["now"]
                .as_f64()
                .filter(|v| {
                    v.is_finite()
                        && *v >= 0.0
                        && *v <= now.saturating_add(AIRCRAFT_CLOCK_TOLERANCE_MS) as f64
                        && *v >= now.saturating_sub(120_000) as f64
                })
                .ok_or("aircraft feed timestamp is missing, stale or in the future")?;
            ("ac", Some(frame.floor() as i64))
        }
        _ => return Err("unknown signal provider".into()),
    };
    let items = root[key].as_array().ok_or("invalid signal items")?;
    let mut unique: BTreeMap<String, WorldSignal> = BTreeMap::new();
    for item in items.iter().take(MAX_INPUT_ITEMS) {
        let candidate = match provider {
            "usgs_earthquakes" => quake_point(item, now),
            "nasa_eonet" => event_point(item, now),
            "adsb_lol" => aircraft_point(item, frame.unwrap(), now),
            _ => None,
        };
        let Some(point) = candidate else {
            continue;
        };
        if area.is_some_and(|(lat, lon, radius)| {
            !in_area(point.latitude, point.longitude, lat, lon, radius)
        }) {
            continue;
        }
        match unique.entry(point.id.clone()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(point);
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                if point.observed_at_ms > entry.get().observed_at_ms {
                    entry.insert(point);
                }
            }
        }
    }
    let matched_total = unique.len();
    let total_is_lower_bound = items.len() > MAX_INPUT_ITEMS
        || (provider == "nasa_eonet" && items.len() >= EONET_RESPONSE_LIMIT);
    let truncated = matched_total > MAX_POINTS || total_is_lower_bound;
    let points: Vec<_> = unique.into_values().take(MAX_POINTS).collect();
    Ok(SignalBatch {
        seen: items.len(),
        rejected: items.len().saturating_sub(matched_total),
        matched_total,
        omitted_points: matched_total.saturating_sub(points.len()),
        total_is_lower_bound,
        points,
        truncated,
    })
}

fn text(value: &str, max: usize) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .take(max)
        .collect::<String>()
        .trim()
        .to_owned()
}
fn identity(value: &Value) -> Option<&str> {
    let id = value.as_str()?;
    (!id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-'))
    .then_some(id)
}
fn coordinates(value: &Value) -> Option<(f64, f64)> {
    let values = value.as_array()?;
    let lon = values.first()?.as_f64()?;
    let lat = values.get(1)?.as_f64()?;
    (lon.is_finite() && lat.is_finite() && lon.abs() <= 180.0 && lat.abs() <= 90.0)
        .then_some((lat, lon))
}
fn in_area(lat: f64, lon: f64, center_lat: f64, center_lon: f64, radius: f64) -> bool {
    let dlat = (lat - center_lat).to_radians();
    let dlon = (lon - center_lon).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat.to_radians().cos() * center_lat.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * 6_371_008.8 * a.clamp(0.0, 1.0).sqrt().asin() <= radius
}
fn epoch(value: Option<&Value>, now: i64, max_age: i64) -> Result<Option<i64>, ()> {
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let timestamp = value.as_i64().ok_or(())?;
    if timestamp < 0 || timestamp > now || timestamp < now.saturating_sub(max_age) {
        return Err(());
    }
    Ok(Some(timestamp))
}
fn date(value: Option<&Value>, now: i64) -> Result<Option<i64>, ()> {
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let date = time::OffsetDateTime::parse(
        value.as_str().ok_or(())?,
        &time::format_description::well_known::Rfc3339,
    )
    .map_err(|_| ())?;
    let timestamp = i64::try_from(date.unix_timestamp_nanos() / 1_000_000).map_err(|_| ())?;
    epoch(Some(&Value::from(timestamp)), now, 30 * DAY)
}
fn metric(
    properties: &mut BTreeMap<String, Value>,
    key: &str,
    value: Option<f64>,
    min: f64,
    max: f64,
) {
    if let Some(value) = value.filter(|v| v.is_finite() && *v >= min && *v <= max) {
        properties.insert(key.into(), Value::from(value));
    }
}
// Keep the source identity, coordinates and timestamp explicit at each adapter.
#[allow(clippy::too_many_arguments)]
fn point(
    provider: &str,
    id: &str,
    kind: &str,
    label: String,
    lat: f64,
    lon: f64,
    observed_at_ms: Option<i64>,
    source_url: String,
    mut properties: BTreeMap<String, Value>,
) -> WorldSignal {
    properties.insert("severity_basis".into(), Value::from("not_assessed"));
    WorldSignal {
        id: format!("{provider}:{id}"),
        kind: kind.into(),
        label,
        latitude: lat,
        longitude: lon,
        altitude_m: None,
        observed_at_ms,
        source_id: provider.into(),
        source_url,
        severity: 0.0,
        properties,
    }
}
fn quake_point(value: &Value, now: i64) -> Option<WorldSignal> {
    let id = identity(value.get("id")?)?;
    let props = value.get("properties")?.as_object()?;
    if value["geometry"]["type"] != "Point" {
        return None;
    }
    let (lat, lon) = coordinates(&value["geometry"]["coordinates"])?;
    let observed = epoch(props.get("time"), now, DAY).ok()?;
    let place = props
        .get("place")
        .and_then(Value::as_str)
        .map(|v| text(v, 120))
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| id.into());
    let mut properties = BTreeMap::new();
    metric(
        &mut properties,
        "magnitude",
        props.get("mag").and_then(Value::as_f64),
        -5.0,
        10.0,
    );
    metric(
        &mut properties,
        "depth_km",
        value["geometry"]["coordinates"]
            .get(2)
            .and_then(Value::as_f64),
        -10.0,
        1000.0,
    );
    if let Some(status) = props.get("status").and_then(Value::as_str) {
        properties.insert("review_status".into(), Value::from(text(status, 32)));
    }
    Some(point(
        "usgs_earthquakes",
        id,
        "earthquake",
        place,
        lat,
        lon,
        observed,
        format!("https://earthquake.usgs.gov/earthquakes/eventpage/{id}"),
        properties,
    ))
}
fn event_point(value: &Value, now: i64) -> Option<WorldSignal> {
    let id = identity(value.get("id")?)?;
    let geometries = value["geometry"].as_array()?;
    if geometries.len() > 512 {
        return None;
    }
    let mut selected: Option<(Option<i64>, f64, f64, &Value)> = None;
    for geometry in geometries {
        if geometry["type"] != "Point" {
            continue;
        }
        let Some((lat, lon)) = coordinates(&geometry["coordinates"]) else {
            continue;
        };
        let Ok(timestamp) = date(geometry.get("date"), now) else {
            continue;
        };
        if selected.as_ref().is_none_or(|old| timestamp > old.0) {
            selected = Some((timestamp, lat, lon, geometry));
        }
    }
    let (timestamp, lat, lon, geometry) = selected?;
    let label = value["title"]
        .as_str()
        .map(|s| text(s, 120))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| id.into());
    let mut properties = BTreeMap::new();
    let categories: Vec<_> = value["categories"]
        .as_array()
        .into_iter()
        .flatten()
        .take(4)
        .filter_map(|v| v["title"].as_str())
        .map(|s| text(s, 48))
        .collect();
    properties.insert("categories".into(), serde_json::json!(categories));
    properties.insert(
        "extent".into(),
        Value::from("reported approximate point, not official event extent"),
    );
    metric(
        &mut properties,
        "magnitude_value",
        geometry["magnitudeValue"].as_f64(),
        -1.0e12,
        1.0e12,
    );
    if let Some(unit) = geometry["magnitudeUnit"].as_str() {
        properties.insert("magnitude_unit".into(), Value::from(text(unit, 32)));
    }
    Some(point(
        "nasa_eonet",
        id,
        "natural_event",
        label,
        lat,
        lon,
        timestamp,
        format!("https://eonet.gsfc.nasa.gov/api/v3/events/{id}"),
        properties,
    ))
}
fn aircraft_point(value: &Value, frame: i64, now: i64) -> Option<WorldSignal> {
    let raw_id = value["hex"].as_str()?;
    if raw_id.len() != 6 || !raw_id.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let id = raw_id.to_ascii_lowercase();
    let lat = value["lat"].as_f64()?;
    let lon = value["lon"].as_f64()?;
    if !lat.is_finite() || !lon.is_finite() || lat.abs() > 90.0 || lon.abs() > 180.0 {
        return None;
    }
    let observed = match value.get("seen_pos").filter(|v| !v.is_null()) {
        Some(age) => {
            let age = age
                .as_f64()
                .filter(|v| v.is_finite() && *v >= 0.0 && *v <= 60.0)?;
            let timestamp = frame.checked_sub((age * 1000.0).round() as i64)?;
            if timestamp < now.saturating_sub(60_000)
                || timestamp > now.saturating_add(AIRCRAFT_CLOCK_TOLERANCE_MS)
            {
                return None;
            }
            Some(timestamp)
        }
        None => None,
    };
    let label = value["flight"]
        .as_str()
        .map(|s| text(s, 32))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("Aircraft {}", id.to_ascii_uppercase()));
    let mut properties = BTreeMap::new();
    properties.insert("icao_hex".into(), Value::from(id.clone()));
    if frame > now {
        properties.insert("provider_clock_lead_ms".into(), Value::from(frame - now));
    }
    metric(
        &mut properties,
        "ground_speed_knots",
        value["gs"].as_f64(),
        0.0,
        2500.0,
    );
    metric(
        &mut properties,
        "track_degrees",
        value["track"].as_f64(),
        0.0,
        360.0,
    );
    metric(
        &mut properties,
        "vertical_rate_feet_per_minute",
        value["baro_rate"].as_f64(),
        -30_000.0,
        30_000.0,
    );
    if let Some(category) = value["category"].as_str() {
        properties.insert("aircraft_category".into(), Value::from(text(category, 8)));
    }
    let altitude = value["alt_baro"]
        .as_f64()
        .filter(|v| v.is_finite() && *v >= -2000.0 && *v <= 100_000.0)
        .map(|v| v * 0.3048);
    if altitude.is_some() {
        properties.insert(
            "altitude_reference".into(),
            Value::from("barometric pressure altitude; not geometric or AGL"),
        );
    }
    if value["alt_baro"] == "ground" {
        properties.insert("ground_reported".into(), Value::Bool(true));
    }
    let mut point = point(
        "adsb_lol",
        &id,
        "aircraft",
        label,
        lat,
        lon,
        observed,
        format!("https://adsb.lol/?icao={id}"),
        properties,
    );
    point.altitude_m = altitude;
    Some(point)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const NOW: i64 = 1_700_000_000_000;
    fn parse(provider: &str, value: Value, lat: f64, lon: f64, radius: f64) -> SignalBatch {
        decode_at(
            provider,
            &serde_json::to_vec(&value).unwrap(),
            Some((lat, lon, radius)),
            NOW,
        )
        .unwrap()
    }
    fn quake(id: &str, lon: f64, lat: f64, time: Value) -> Value {
        json!({"type":"Feature","id":id,"properties":{"place":"A\u{0} safe place\n","mag":4.2,"time":time,"url":"https://evil.invalid/"},"geometry":{"type":"Point","coordinates":[lon,lat,12.5]}})
    }
    #[test]
    fn requests_have_fixed_hosts_bounded_radius_and_documented_bbox_order() {
        let list = requests(-33.45, -70.66, 100_000.);
        assert_eq!(list.len(), 3);
        assert_eq!(
            list[0].url,
            "https://earthquake.usgs.gov/earthquakes/feed/v1.0/summary/all_day.geojson"
        );
        assert!(list[1].url.starts_with(
            "https://eonet.gsfc.nasa.gov/api/v3/events?status=open&days=30&limit=200&bbox="
        ));
        let bbox: Vec<f64> = list[1]
            .url
            .split("bbox=")
            .nth(1)
            .unwrap()
            .split(',')
            .map(|v| v.parse().unwrap())
            .collect();
        assert!(bbox[0] < -70.66 && bbox[2] > -70.66 && bbox[1] > -33.45 && bbox[3] < -33.45);
        assert!(list[2].url.ends_with("/dist/54"));
        assert_eq!(
            [
                list[0].ttl_seconds,
                list[1].ttl_seconds,
                list[2].ttl_seconds
            ],
            [60, 600, 15]
        );
        assert!(!requests(0., 179.99, 10_000.)[1].url.contains("bbox="));
    }
    #[test]
    fn quake_depth_is_not_altitude_and_provider_urls_are_not_forwarded() {
        let batch = parse(
            "usgs_earthquakes",
            json!({"type":"FeatureCollection","features":[quake("us123",179.95,0.,json!(NOW-1000))]}),
            0.,
            -179.95,
            30_000.,
        );
        assert_eq!(batch.points.len(), 1);
        let p = &batch.points[0];
        assert_eq!(p.altitude_m, None);
        assert_eq!(p.observed_at_ms, Some(NOW - 1000));
        assert_eq!(p.properties["depth_km"], 12.5);
        assert_eq!(p.properties["magnitude"], 4.2);
        assert_eq!(
            p.source_url,
            "https://earthquake.usgs.gov/earthquakes/eventpage/us123"
        );
        assert!(!p.label.chars().any(char::is_control));
    }
    #[test]
    fn invalid_outside_future_stale_and_duplicate_quakes_are_omitted() {
        let input = vec![
            quake("a", 0., 0., json!(NOW - 100)),
            quake("a", 0., 0., json!(NOW - 100)),
            quake("far", 1., 1., json!(NOW)),
            quake("bad", 0., 91., json!(NOW)),
            quake("future", 0., 0., json!(NOW + 1)),
            quake("stale", 0., 0., json!(NOW - 2 * 86400000)),
            quake("badtime", 0., 0., json!("bad")),
            quake("../evil", 0., 0., json!(NOW)),
        ];
        let batch = parse(
            "usgs_earthquakes",
            json!({"type":"FeatureCollection","features":input}),
            0.,
            0.,
            1000.,
        );
        assert_eq!((batch.seen, batch.rejected, batch.points.len()), (8, 7, 1));
    }
    #[test]
    fn eonet_chooses_newest_valid_point_and_does_not_invent_polygon_centroids() {
        let batch = parse(
            "nasa_eonet",
            json!({"events":[{"id":"EONET_1","title":"Fire","categories":[{"id":"wildfires","title":"Wildfires"}],"geometry":[
            {"type":"Point","date":"2023-11-13T00:00:00Z","coordinates":[0.001,0.]},
            {"type":"Point","date":"2023-11-14T00:00:00Z","coordinates":[0.002,0.]},
            {"type":"Point","date":"2023-11-15T00:00:00Z","coordinates":[0.003,0.]}
        ]},{"id":"EONET_2","title":"Polygon","geometry":[{"type":"Polygon","date":"2023-11-14T00:00:00Z","coordinates":[[[0.,0.],[1.,0.],[0.,1.],[0.,0.]]]}]}]}),
            0.,
            0.,
            1000.,
        );
        assert_eq!((batch.points.len(), batch.rejected), (1, 1));
        assert_eq!(batch.points[0].longitude, 0.002);
        assert_eq!(
            batch.points[0].source_url,
            "https://eonet.gsfc.nasa.gov/api/v3/events/EONET_1"
        );
        assert_eq!(batch.points[0].altitude_m, None);
    }
    #[test]
    fn eonet_provider_limit_marks_filtered_totals_as_lower_bounds() {
        for matching in [false, true] {
            let events: Vec<_> = (0..200)
                .map(|i| {
                    json!({"id":format!("EONET_{i}"),"geometry":[{
                        "type":"Point","date":"2023-11-14T00:00:00Z",
                        "coordinates":[if matching && i == 0 { 0. } else { 10. },0.]
                    }]})
                })
                .collect();
            let batch = parse("nasa_eonet", json!({"events":events}), 0., 0., 1000.);
            assert_eq!(batch.matched_total, usize::from(matching));
            assert_eq!(batch.omitted_points, 0);
            assert!(batch.total_is_lower_bound);
            assert!(batch.truncated);
        }
    }
    #[test]
    fn aircraft_epoch_is_milliseconds_and_baro_altitude_has_explicit_units() {
        let batch = parse(
            "adsb_lol",
            json!({"now":NOW,"ac":[{"hex":"ABC123","lat":0.,"lon":0.,"alt_baro":10000,"seen_pos":2.5,"flight":"TEST\n  "},{"hex":"abc124","lat":0.,"lon":0.,"alt_baro":"ground"}]}),
            0.,
            0.,
            1000.,
        );
        assert_eq!(batch.points.len(), 2);
        assert_eq!(batch.points[0].altitude_m, Some(3048.));
        assert_eq!(batch.points[0].observed_at_ms, Some(NOW - 2500));
        assert_eq!(
            batch.points[0].properties["altitude_reference"],
            "barometric pressure altitude; not geometric or AGL"
        );
        assert_eq!(batch.points[1].altitude_m, None);
        assert_eq!(batch.points[1].observed_at_ms, None);
    }
    #[test]
    fn aircraft_stale_future_invalid_and_duplicate_reports_are_not_current_contacts() {
        let batch = parse(
            "adsb_lol",
            json!({"now":NOW,"ac":[{"hex":"abc123","lat":0.,"lon":0.,"seen_pos":1.},{"hex":"ABC123","lat":0.,"lon":0.,"seen_pos":1.},{"hex":"abc124","lat":0.,"lon":0.,"seen_pos":120.},{"hex":"abc125","lat":0.,"lon":0.,"seen_pos":-1.},{"hex":"abc126","lat":0.,"lon":181.},{"hex":"not-hex","lat":0.,"lon":0.}]}),
            0.,
            0.,
            1000.,
        );
        assert_eq!((batch.seen, batch.rejected, batch.points.len()), (6, 5, 1));
        assert!(decode_at(
            "adsb_lol",
            br#"{"now":1700000005001,"ac":[]}"#,
            Some((0., 0., 1000.)),
            NOW
        )
        .is_err());
    }
    #[test]
    fn aircraft_small_provider_clock_lead_is_explicit_and_preserves_reported_time() {
        // The real feed was 81 ms ahead of the receiving machine on refresh.
        // A bounded clock difference must not discard the entire observation feed.
        let batch = parse(
            "adsb_lol",
            json!({"now":NOW+81,"ac":[{"hex":"abc123","lat":0.,"lon":0.,"seen_pos":0.}]}),
            0.,
            0.,
            1000.,
        );
        assert_eq!(batch.points.len(), 1);
        assert_eq!(batch.points[0].observed_at_ms, Some(NOW + 81));
        assert_eq!(batch.points[0].properties["provider_clock_lead_ms"], 81);
    }
    #[test]
    fn bounded_output_counts_omissions_and_keeps_unknown_time_unknown() {
        let items: Vec<_> = (0..140)
            .map(|i| quake(&format!("q{i}"), 0., 0., Value::Null))
            .collect();
        let batch = parse(
            "usgs_earthquakes",
            json!({"type":"FeatureCollection","features":items}),
            0.,
            0.,
            1000.,
        );
        assert_eq!(
            (
                batch.points.len(),
                batch.seen,
                batch.rejected,
                batch.truncated
            ),
            (128, 140, 0, true)
        );
        let counts = serde_json::to_value(&batch).unwrap();
        assert_eq!(counts["matched_total"], 140);
        assert_eq!(counts["omitted_points"], 12);
        assert_eq!(counts["total_is_lower_bound"], false);
        assert!(batch.points.iter().all(|p| p.observed_at_ms.is_none()));
    }
    #[test]
    fn bad_envelopes_and_invalid_aoi_fail_closed() {
        for (provider, bytes) in [
            ("unknown", b"{}".as_slice()),
            ("usgs_earthquakes", b"{}"),
            ("nasa_eonet", b"{\"events\":false}"),
            ("adsb_lol", b"{\"ac\":[]}"),
            ("usgs_earthquakes", b"{bad"),
        ] {
            assert!(decode_at(provider, bytes, Some((0., 0., 1000.)), NOW).is_err());
        }
        assert!(decode_at(
            "nasa_eonet",
            br#"{"events":[]}"#,
            Some((f64::NAN, 0., 1000.)),
            NOW
        )
        .is_err());
        assert!(requests(0., 0., 0.).is_empty());
    }

    #[test]
    fn duplicate_ids_keep_the_newest_actual_observation() {
        let batch = parse(
            "usgs_earthquakes",
            json!({"type":"FeatureCollection","features":[
                quake("same",0.,0.,json!(NOW-2000)),quake("same",0.,0.,json!(NOW-1000))
            ]}),
            0.,
            0.,
            1000.,
        );
        assert_eq!((batch.points.len(), batch.rejected), (1, 1));
        assert_eq!(batch.points[0].observed_at_ms, Some(NOW - 1000));
    }

    #[test]
    fn eonet_stale_or_invalid_dated_points_are_not_undated_observations() {
        let batch = parse(
            "nasa_eonet",
            json!({"events":[
                {"id":"old","geometry":[{"type":"Point","date":"2022-01-01T00:00:00Z","coordinates":[0.,0.]}]},
                {"id":"bad","geometry":[{"type":"Point","date":"not-date","coordinates":[0.,0.]}]},
                {"id":"unknown","geometry":[{"type":"Point","coordinates":[0.,0.]}]}
            ]}),
            0.,
            0.,
            1000.,
        );
        assert_eq!((batch.points.len(), batch.rejected), (1, 2));
        assert_eq!(batch.points[0].observed_at_ms, None);
    }

    #[test]
    fn globe_requests_are_fixed_global_bounded_natural_event_feeds() {
        let list = global_requests();
        assert_eq!(list.len(), 2);
        assert_eq!(
            (list[0].provider.as_str(), list[0].ttl_seconds),
            ("usgs_earthquakes", 60)
        );
        assert_eq!(
            list[0].url,
            "https://earthquake.usgs.gov/earthquakes/feed/v1.0/summary/all_day.geojson"
        );
        assert_eq!(
            (list[1].provider.as_str(), list[1].ttl_seconds),
            ("nasa_eonet", 600)
        );
        assert_eq!(
            list[1].url,
            "https://eonet.gsfc.nasa.gov/api/v3/events?status=open&days=30&limit=200"
        );
    }

    #[test]
    fn globe_keeps_continents_and_both_sides_of_seam_but_rejects_invalid_quakes() {
        let input = json!({"type":"FeatureCollection","features":[
            quake("chile",-70.66,-33.45,Value::Null),
            quake("japan",139.69,35.68,Value::Null),
            quake("seam_west",-179.99,0.,Value::Null),
            quake("seam_east",179.99,0.,Value::Null),
            quake("chile",-70.66,-33.45,Value::Null),
            quake("invalid_lat",0.,91.,Value::Null),
            quake("invalid_lon",181.,0.,Value::Null),
            quake("stale",0.,0.,json!(0)),
            quake("future",0.,0.,json!(i64::MAX))
        ]});
        let batch =
            decode_global("usgs_earthquakes", &serde_json::to_vec(&input).unwrap()).unwrap();
        assert_eq!(
            (
                batch.seen,
                batch.matched_total,
                batch.rejected,
                batch.omitted_points
            ),
            (9, 4, 5, 0)
        );
        let ids: Vec<_> = batch.points.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "usgs_earthquakes:chile",
                "usgs_earthquakes:japan",
                "usgs_earthquakes:seam_east",
                "usgs_earthquakes:seam_west"
            ]
        );
        assert!(!batch.total_is_lower_bound);
        assert!(batch.points.iter().all(|p| p.observed_at_ms.is_none()));
    }

    #[test]
    fn globe_eonet_provider_limit_and_output_omissions_remain_explicit() {
        let events:Vec<_>=(0..200).map(|i|json!({"id":format!("EONET_{i}"),"geometry":[{"type":"Point","coordinates":[if i%2==0 {179.99} else {-179.99},if i%3==0 {35.} else {-33.}]}]})).collect();
        let batch = decode_global(
            "nasa_eonet",
            &serde_json::to_vec(&json!({"events":events})).unwrap(),
        )
        .unwrap();
        assert_eq!(
            (
                batch.seen,
                batch.matched_total,
                batch.points.len(),
                batch.omitted_points,
                batch.rejected
            ),
            (200, 200, 128, 72, 0)
        );
        assert!(batch.total_is_lower_bound && batch.truncated);
        assert!(batch.points.iter().any(|p| p.longitude < 0.));
        assert!(batch.points.iter().any(|p| p.longitude > 0.));
    }

    #[test]
    fn globe_refuses_aircraft_unknown_and_oversized_feeds() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        assert!(decode_global(
            "adsb_lol",
            &serde_json::to_vec(&json!({"now":now,"ac":[]})).unwrap()
        )
        .is_err());
        assert!(decode_global("other", b"{}").is_err());
        assert!(decode_global("nasa_eonet", &vec![b' '; 2 * 1024 * 1024 + 1]).is_err());
    }

    #[test]
    fn globe_retains_input_scan_ceiling_and_discloses_incomplete_total() {
        let features: Vec<_> = (0..4097)
            .map(|i| quake(&format!("q{i}"), 139., 35., Value::Null))
            .collect();
        let batch = decode_global(
            "usgs_earthquakes",
            &serde_json::to_vec(&json!({"type":"FeatureCollection","features":features})).unwrap(),
        )
        .unwrap();
        assert_eq!(
            (
                batch.seen,
                batch.matched_total,
                batch.points.len(),
                batch.omitted_points
            ),
            (4097, 4096, 128, 3968)
        );
        assert!(batch.total_is_lower_bound && batch.truncated);
    }
}
