//! Fixed keyless imagery adapters. Google is experimental, volatile display-only.
use archie_geofabric::exploration::{
    self, ExplorationRequest, GeoBounds, ImagerySource, TileCoordinate,
};
use base64::Engine;
use serde_json::Value;
use std::io::Cursor;

pub const INLINE_BUDGET: usize = 384 * 1024;

pub struct ImagerySpec {
    pub id: &'static str,
    pub label: &'static str,
    pub attribution: &'static str,
    pub layer: &'static str,
    pub max_zoom: u8,
    pub extension: &'static str,
    pub daily: bool,
    pub online_only: bool,
}
pub fn spec(source: ImagerySource) -> ImagerySpec {
    let (id, label, layer, max_zoom, extension, daily, online_only) = match source {
        ImagerySource::GoogleSatellite => (
            "google_satellite",
            "Google satellite · experimental online preview",
            "",
            19,
            "jpg",
            false,
            true,
        ),
        ImagerySource::BlueMarble => (
            "blue_marble",
            "NASA Blue Marble shaded relief",
            "BlueMarble_ShadedRelief_Bathymetry",
            8,
            "jpeg",
            false,
            false,
        ),
        ImagerySource::Sentinel2 => (
            "sentinel2",
            "NASA HLS Sentinel-2",
            "HLS_S30_Nadir_BRDF_Adjusted_Reflectance",
            12,
            "png",
            true,
            false,
        ),
        ImagerySource::Landsat => (
            "landsat",
            "NASA HLS Landsat",
            "HLS_L30_Nadir_BRDF_Adjusted_Reflectance",
            12,
            "png",
            true,
            false,
        ),
        ImagerySource::NaturalColor => (
            "natural_color",
            "NASA MODIS natural colour",
            "MODIS_Terra_CorrectedReflectance_TrueColor",
            9,
            "jpeg",
            true,
            false,
        ),
        ImagerySource::Infrared => (
            "infrared",
            "NASA MODIS false colour · bands 7/2/1",
            "MODIS_Terra_CorrectedReflectance_Bands721",
            9,
            "jpeg",
            true,
            false,
        ),
        ImagerySource::NightLights => (
            "night_lights",
            "NASA VIIRS city lights · 2012 composite",
            "VIIRS_CityLights_2012",
            8,
            "jpeg",
            false,
            false,
        ),
        ImagerySource::Thermal => (
            "thermal",
            "NASA MODIS daily daytime land-surface temperature",
            "MODIS_Terra_L3_Land_Surface_Temp_Daily_Day_TES",
            7,
            "png",
            true,
            false,
        ),
    };
    ImagerySpec {
        id,
        label,
        layer,
        max_zoom,
        extension,
        daily,
        online_only,
        attribution: if online_only {
            "© Google · experimental online display only; imagery dates vary"
        } else {
            "NASA EOSDIS GIBS · NASA/USGS/ESA source imagery; dates and gaps vary"
        },
    }
}

pub fn selected_sources(request: &ExplorationRequest) -> Vec<ImagerySource> {
    let selected = if request.imagery_sources.is_empty() {
        vec![ImagerySource::GoogleSatellite, ImagerySource::BlueMarble]
    } else {
        request.imagery_sources.clone()
    };
    let mut unique = Vec::new();
    for source in selected {
        if !unique.contains(&source) {
            unique.push(source);
        }
    }
    unique
}

pub fn effective_date(requested: Option<&str>, now_ms: i64) -> Result<String, String> {
    if let Some(date) = requested {
        if !exploration::valid_imagery_date(date) {
            return Err("invalid imagery date".into());
        }
        return Ok(date.into());
    }
    let date = time::OffsetDateTime::from_unix_timestamp(now_ms / 1000)
        .map_err(|_| "imagery clock unavailable")?
        .checked_sub(time::Duration::days(3))
        .ok_or("imagery date overflow")?;
    date.format(time::macros::format_description!("[year]-[month]-[day]"))
        .map_err(|e| e.to_string())
}

pub struct ImageryTileRequest {
    pub tile: TileCoordinate,
    pub bounds: GeoBounds,
    pub url: String,
}
pub fn source_tiles(
    request: &ExplorationRequest,
    source: ImagerySource,
    date: &str,
) -> Result<Vec<ImageryTileRequest>, String> {
    if !exploration::valid_imagery_date(date) {
        return Err("invalid imagery date".into());
    }
    let source = spec(source);
    let mut bounded = request.clone();
    // Match the native projection's 32-tile aggregate limit while giving a
    // small selection enough tiles to preserve local detail across the AOI.
    bounded.max_tiles = request
        .max_tiles
        .min(16)
        .min(32 / selected_sources(request).len().max(1) as u32);
    for zoom in (0..=request.zoom.min(source.max_zoom)).rev() {
        bounded.zoom = zoom;
        let Ok(plan) = exploration::plan(&bounded) else {
            continue;
        };
        return Ok(plan.tiles.into_iter().map(|tile| {
            let n = 2f64.powi(i32::from(tile.zoom));
            let lat = |y:f64| (std::f64::consts::PI * (1.0 - 2.0*y/n)).sinh().atan().to_degrees();
            let bounds = GeoBounds {west:f64::from(tile.x)/n*360.0-180.0,east:f64::from(tile.x+1)/n*360.0-180.0,
                north:lat(f64::from(tile.y)),south:lat(f64::from(tile.y+1))};
            let url = if source.online_only {
                format!("https://mt1.google.com/vt/lyrs=s&x={}&y={}&z={}",tile.x,tile.y,tile.zoom)
            } else {
                let time = if source.daily {format!("{date}/")} else {String::new()};
                format!("https://gibs.earthdata.nasa.gov/wmts/epsg3857/best/{}/default/{}GoogleMapsCompatible_Level{}/{}/{}/{}.{}",
                    source.layer,time,source.max_zoom,tile.zoom,tile.y,tile.x,source.extension)
            };
            ImageryTileRequest {tile,bounds,url}
        }).collect());
    }
    Err("imagery AOI cannot be represented within its bounded tile allocation".into())
}

/// Removes only volatile display imagery, retaining open data and geometry.
pub fn strip_online_pixels(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            if fields.get("mode").and_then(Value::as_str) == Some("online_preview_only")
                || fields.get("source_id").and_then(Value::as_str) == Some("google_satellite")
            {
                fields.insert("tiles".into(), serde_json::json!([]));
                fields.insert("state".into(), serde_json::json!("preview_expired"));
                fields.insert(
                    "error".into(),
                    serde_json::json!("online imagery is not retained or supplied to models"),
                );
            }
            for child in fields.values_mut() {
                strip_online_pixels(child);
            }
        }
        Value::Array(items) => {
            for child in items {
                strip_online_pixels(child);
            }
        }
        _ => {}
    }
}

pub struct EncodedPreview {
    pub mime: &'static str,
    pub data_base64: String,
    pub width: u32,
    pub height: u32,
    pub source_width: u32,
}
pub fn encode_preview(bytes: &[u8], budget: usize) -> Result<EncodedPreview, String> {
    let (width, height, channels, pixels, mime) = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let mut decoder = png::Decoder::new(Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().map_err(|_| "invalid imagery PNG")?;
        let info = reader.info();
        if info.width == 0 || info.height == 0 || info.width > 512 || info.height > 512 {
            return Err("imagery tile dimensions exceed 512px".into());
        }
        let mut pixels = vec![0; reader.output_buffer_size().ok_or("invalid PNG buffer")?];
        let output = reader
            .next_frame(&mut pixels)
            .map_err(|_| "invalid imagery PNG pixels")?;
        pixels.truncate(output.buffer_size());
        let channels = match output.color_type {
            png::ColorType::Grayscale => 1,
            png::ColorType::GrayscaleAlpha => 2,
            png::ColorType::Rgb => 3,
            png::ColorType::Rgba => 4,
            _ => return Err("unsupported imagery PNG colour".into()),
        };
        if (channels == 2 || channels == 4)
            && pixels
                .chunks_exact(channels)
                .all(|pixel| pixel[channels - 1] == 0)
        {
            return Err("source returned transparent no-data tile".into());
        }
        (output.width, output.height, channels, pixels, "image/png")
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        let mut decoder = jpeg_decoder::Decoder::new(Cursor::new(bytes));
        decoder.read_info().map_err(|_| "invalid imagery JPEG")?;
        let info = decoder.info().ok_or("imagery JPEG has no dimensions")?;
        if info.width == 0 || info.height == 0 || info.width > 512 || info.height > 512 {
            return Err("imagery tile dimensions exceed 512px".into());
        }
        let channels = match info.pixel_format {
            jpeg_decoder::PixelFormat::L8 => 1,
            jpeg_decoder::PixelFormat::RGB24 => 3,
            _ => return Err("unsupported imagery JPEG colour".into()),
        };
        let pixels = decoder
            .decode()
            .map_err(|_| "invalid imagery JPEG pixels")?;
        (
            u32::from(info.width),
            u32::from(info.height),
            channels,
            pixels,
            "image/jpeg",
        )
    } else {
        return Err("provider returned no supported imagery pixels".into());
    };
    if bytes.len().div_ceil(3) * 4 <= budget {
        return Ok(EncodedPreview {
            mime,
            data_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
            width,
            height,
            source_width: width,
        });
    }
    for side in [256, 128, 96, 64] {
        let w = width.min(side);
        let h = height.min(side);
        let mut reduced = Vec::with_capacity((w * h) as usize * channels);
        for y in 0..h {
            for x in 0..w {
                let index = ((y * height / h * width + x * width / w) as usize) * channels;
                reduced.extend_from_slice(&pixels[index..index + channels]);
            }
        }
        let mut encoded = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut encoded, w, h);
            encoder.set_color(match channels {
                1 => png::ColorType::Grayscale,
                2 => png::ColorType::GrayscaleAlpha,
                3 => png::ColorType::Rgb,
                _ => png::ColorType::Rgba,
            });
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder
                .write_header()
                .map_err(|_| "imagery preview encoding failed")?;
            writer
                .write_image_data(&reduced)
                .map_err(|_| "imagery preview encoding failed")?;
        }
        if encoded.len().div_ceil(3) * 4 <= budget {
            return Ok(EncodedPreview {
                mime: "image/png",
                data_base64: base64::engine::general_purpose::STANDARD.encode(encoded),
                width: w,
                height: h,
                source_width: width,
            });
        }
    }
    Err("preview omitted: aggregate inline byte cap".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use archie_geofabric::exploration::{ExplorationRequest, ImagerySource};

    fn png_tile(side: u32, transparent: bool) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, side, side);
            encoder.set_color(png::ColorType::Rgba);
            let mut writer = encoder.write_header().unwrap();
            let mut pixels = vec![255; (side * side * 4) as usize];
            if transparent {
                for pixel in pixels.chunks_exact_mut(4) {
                    pixel[3] = 0;
                }
            }
            writer.write_image_data(&pixels).unwrap();
        }
        bytes
    }

    #[test]
    fn native_pixels_survive_when_the_aggregate_preview_budget_fits() {
        let bytes = png_tile(256, false);
        let result = encode_preview(&bytes, INLINE_BUDGET).unwrap();
        assert_eq!((result.width, result.height), (256, 256));
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(result.data_base64)
                .unwrap(),
            bytes
        );
        assert!(encode_preview(&bytes, 1)
            .err()
            .unwrap()
            .contains("inline byte cap"));
    }

    #[test]
    fn oversized_and_transparent_tiles_report_omissions_without_fabricated_pixels() {
        assert!(encode_preview(&png_tile(513, false), INLINE_BUDGET)
            .err()
            .unwrap()
            .contains("512px"));
        assert!(encode_preview(&png_tile(256, true), INLINE_BUDGET)
            .err()
            .unwrap()
            .contains("no-data"));
    }

    #[test]
    fn local_imagery_keeps_detail_with_sixteen_tiles_and_thirty_two_across_sources() {
        let request = ExplorationRequest {
            latitude: 37.62,
            longitude: -122.38,
            radius_m: 15_000.0,
            zoom: 14,
            ..Default::default()
        };
        let tiles = source_tiles(&request, ImagerySource::GoogleSatellite, "2026-10-20").unwrap();
        assert!(tiles.len() <= 16);
        assert!(
            tiles[0].tile.zoom >= 11,
            "a 30km area must keep available local detail"
        );
        let all = ExplorationRequest {
            imagery_sources: vec![
                ImagerySource::GoogleSatellite,
                ImagerySource::BlueMarble,
                ImagerySource::Sentinel2,
                ImagerySource::Landsat,
                ImagerySource::NaturalColor,
                ImagerySource::Infrared,
                ImagerySource::NightLights,
                ImagerySource::Thermal,
            ],
            ..request
        };
        let mut total = 0;
        for source in selected_sources(&all) {
            let tiles = source_tiles(&all, source, "2026-10-20").unwrap();
            assert!(tiles.len() <= 4);
            total += tiles.len();
        }
        assert!(total <= 32);
    }

    #[test]
    fn sources_have_fixed_urls_and_complete_bounded_aoi_tiles() {
        let request = ExplorationRequest {
            latitude: 37.62,
            longitude: -122.38,
            radius_m: 15_000.0,
            zoom: 11,
            ..ExplorationRequest::default()
        };
        for source in [
            ImagerySource::GoogleSatellite,
            ImagerySource::BlueMarble,
            ImagerySource::Thermal,
        ] {
            let tiles = source_tiles(&request, source, "2026-10-21").unwrap();
            assert!(!tiles.is_empty() && tiles.len() <= 16);
            assert!(tiles.iter().all(|tile| tile.url.starts_with(
                if source == ImagerySource::GoogleSatellite {
                    "https://mt1.google.com/"
                } else {
                    "https://gibs.earthdata.nasa.gov/"
                }
            )));
            assert!(tiles.iter().all(|tile| tile.tile.zoom <= request.zoom));
        }
    }

    #[test]
    fn daily_default_is_utc_minus_three_days_and_input_date_is_pinned() {
        assert_eq!(
            effective_date(None, 1_761_264_000_000).unwrap(),
            "2025-10-21"
        );
        assert_eq!(effective_date(Some("2024-02-29"), 0).unwrap(), "2024-02-29");
        assert!(effective_date(Some("2023-02-29"), 0).is_err());
    }

    #[test]
    fn online_pixels_are_removed_from_persistence_and_model_views() {
        let mut value = serde_json::json!({"preview":{"terrain":{"width":2},"imagery":[
            {"source_id":"google_satellite","mode":"online_preview_only","tiles":[{"data_base64":"SECRET_PIXELS"}]},
            {"source_id":"blue_marble","mode":"open_data","tiles":[{"data_base64":"OPEN_PIXELS"}]}
        ]}});
        strip_online_pixels(&mut value);
        assert!(!value.to_string().contains("SECRET_PIXELS"));
        assert!(value.to_string().contains("OPEN_PIXELS"));
        assert_eq!(value["preview"]["terrain"]["width"], 2);
    }
}
