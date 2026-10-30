use archie_geofabric::exploration::{
    plan, ExplorationDomain, ExplorationError, ExplorationLayer, ExplorationRequest, TileCoordinate,
};
use std::collections::BTreeSet;

#[test]
fn imagery_sources_dates_and_signal_refresh_are_typed_and_bounded() {
    use archie_geofabric::exploration::ImagerySource;
    let mut request = ExplorationRequest {
        layers: vec![ExplorationLayer::Imagery, ExplorationLayer::Signals],
        imagery_sources: vec![ImagerySource::GoogleSatellite, ImagerySource::BlueMarble],
        imagery_date: Some("2024-02-29".into()),
        signal_updates: 60,
        signal_interval_seconds: 60,
        ..Default::default()
    };
    assert!(plan(&request).is_ok());
    for date in ["2023-02-29", "2026-13-01", "2026-01-32", "../escape"] {
        request.imagery_date = Some(date.into());
        assert!(plan(&request).is_err());
    }
    request.imagery_date = None;
    request.signal_updates = 61;
    assert!(plan(&request).is_err());
    request.signal_updates = 1;
    request.signal_interval_seconds = 59;
    assert!(plan(&request).is_err());
    request.signal_interval_seconds = 3601;
    assert!(plan(&request).is_err());
}

#[test]
fn older_requests_receive_conservative_imagery_and_refresh_defaults() {
    let mut raw = serde_json::to_value(ExplorationRequest::default()).unwrap();
    for name in [
        "imagery_sources",
        "imagery_date",
        "signal_updates",
        "signal_interval_seconds",
    ] {
        raw.as_object_mut().unwrap().remove(name);
    }
    let request: ExplorationRequest = serde_json::from_value(raw).unwrap();
    assert!(request.imagery_sources.is_empty());
    assert_eq!(request.signal_updates, 1);
    assert_eq!(request.signal_interval_seconds, 60);
}

fn at(latitude: f64, longitude: f64) -> ExplorationRequest {
    ExplorationRequest {
        latitude,
        longitude,
        ..ExplorationRequest::default()
    }
}

#[test]
fn default_region_has_deterministic_unique_bounded_tiles() {
    let request = at(-33.4489, -70.6693);
    let first = plan(&request).unwrap();
    assert_eq!(first, plan(&request).unwrap());
    assert!(!first.tiles.is_empty());
    assert!(first.tiles.len() <= request.max_tiles as usize);
    assert_eq!(
        first.tiles.iter().collect::<BTreeSet<_>>().len(),
        first.tiles.len()
    );
    assert_eq!(first.estimated_requests, first.tiles.len() as u64);
    assert_eq!(first.camera.radius_m, 500.0);
    assert_eq!(first.camera.latitude, request.latitude);
    assert_eq!(first.request, request);
    assert!(first.bounds.south < request.latitude && first.bounds.north > request.latitude);
}

#[test]
fn equator_origin_small_circle_intersects_four_tiles() {
    // The prime meridian and equator are tile boundaries at zoom 1.
    let result = plan(&ExplorationRequest {
        zoom: 1,
        radius_m: 1.0,
        ..at(0.0, 0.0)
    })
    .unwrap();
    assert_eq!(
        result.tiles,
        vec![
            TileCoordinate {
                zoom: 1,
                x: 0,
                y: 0
            },
            TileCoordinate {
                zoom: 1,
                x: 0,
                y: 1
            },
            TileCoordinate {
                zoom: 1,
                x: 1,
                y: 0
            },
            TileCoordinate {
                zoom: 1,
                x: 1,
                y: 1
            },
        ]
    );
}

#[test]
fn ground_radius_bounds_use_meters_at_different_latitudes() {
    let equator = plan(&at(0.0, 20.0)).unwrap();
    let high = plan(&at(60.0, 20.0)).unwrap();
    let equator_span = equator.bounds.east - equator.bounds.west;
    let high_span = high.bounds.east - high.bounds.west;
    assert!((equator.bounds.north - 0.0044966).abs() < 0.000001);
    assert!((high_span / equator_span - 2.0).abs() < 0.001);
    assert_eq!(high.camera.radius_m, equator.camera.radius_m);
}

#[test]
fn circle_excludes_bounding_box_corner_tiles() {
    // At zoom 16 the equatorial tile width is about 611 m. Center this
    // circle on a tile's middle: 700 m reaches all nine surrounding tiles.
    let result = plan(&ExplorationRequest {
        latitude: 0.0027465820301980836,
        longitude: 0.00274658203125,
        radius_m: 700.0,
        zoom: 16,
        ..ExplorationRequest::default()
    })
    .unwrap();
    assert_eq!(result.tiles.len(), 9);
    // A tighter circle only touches the middle tile and its four neighbors.
    let tight = plan(&ExplorationRequest {
        radius_m: 400.0,
        ..result.request
    })
    .unwrap();
    assert_eq!(tight.tiles.len(), 5);
}

#[test]
fn dateline_regions_wrap_with_no_world_spanning_tile_selection() {
    for longitude in [179.999, -179.999, 180.0, -180.0] {
        let result = plan(&at(0.0, longitude)).unwrap();
        assert!(result.bounds.west > result.bounds.east);
        assert!(result.tiles.iter().any(|tile| tile.x == 0));
        assert!(result.tiles.iter().any(|tile| tile.x == (1 << 14) - 1));
        assert!(result.tiles.len() <= 4);
    }
    let world_tile = plan(&ExplorationRequest {
        zoom: 0,
        ..at(0.0, 180.0)
    })
    .unwrap();
    assert_eq!(
        world_tile.tiles,
        vec![TileCoordinate {
            zoom: 0,
            x: 0,
            y: 0
        }]
    );
}

#[test]
fn poles_and_regions_crossing_projection_limit_are_explicitly_refused() {
    for latitude in [-90.0, 90.0, -85.05, 85.05] {
        assert!(matches!(
            plan(&at(latitude, 0.0)),
            Err(ExplorationError::ProjectionLimit)
        ));
    }
    assert!(plan(&at(84.0, 0.0)).is_ok());
}

#[test]
fn invalid_coordinates_and_budgets_are_rejected() {
    for latitude in [f64::NAN, f64::INFINITY, -90.1, 90.1] {
        assert!(plan(&at(latitude, 0.0)).is_err());
    }
    for longitude in [f64::NAN, f64::NEG_INFINITY, -180.1, 180.1] {
        assert!(plan(&at(0.0, longitude)).is_err());
    }
    for radius_m in [f64::NAN, f64::INFINITY, 0.0, -1.0, 100_001.0] {
        assert!(plan(&ExplorationRequest {
            radius_m,
            ..at(0.0, 0.0)
        })
        .is_err());
    }
    for request in [
        ExplorationRequest {
            zoom: 23,
            ..at(0.0, 0.0)
        },
        ExplorationRequest {
            max_tiles: 0,
            ..at(0.0, 0.0)
        },
        ExplorationRequest {
            max_tiles: 4097,
            ..at(0.0, 0.0)
        },
        ExplorationRequest {
            max_bytes: 0,
            ..at(0.0, 0.0)
        },
        ExplorationRequest {
            max_bytes: 1_073_741_825,
            ..at(0.0, 0.0)
        },
        ExplorationRequest {
            layers: vec![],
            ..at(0.0, 0.0)
        },
    ] {
        assert!(plan(&request).is_err());
    }
}

#[test]
fn enormous_fine_zoom_regions_refuse_before_materializing_tiles() {
    let request = ExplorationRequest {
        radius_m: 100_000.0,
        zoom: 22,
        ..at(0.0, 0.0)
    };
    assert!(matches!(
        plan(&request),
        Err(ExplorationError::TileBudgetExceeded { .. })
    ));
    assert!(matches!(
        plan(&ExplorationRequest {
            max_tiles: 1,
            ..at(0.0, 0.0)
        }),
        Err(ExplorationError::TileBudgetExceeded { .. })
    ));
}

#[test]
fn repeated_layers_do_not_amplify_requests_and_domains_stay_explicit() {
    let request = ExplorationRequest {
        layers: vec![
            ExplorationLayer::Terrain,
            ExplorationLayer::Terrain,
            ExplorationLayer::Buildings,
        ],
        ..at(0.0, 0.0)
    };
    let personal = plan(&request).unwrap();
    assert_eq!(personal.estimated_requests, personal.tiles.len() as u64 * 2);
    let work = plan(&ExplorationRequest {
        domain: ExplorationDomain::Work,
        ..request
    })
    .unwrap();
    assert_eq!(work.tiles, personal.tiles);
    assert_eq!(work.request.domain, ExplorationDomain::Work);
    assert_eq!(personal.request.domain, ExplorationDomain::Personal);
}

#[test]
fn foreground_applications_have_no_planning_role() {
    // Attempted activity metadata has no representation in geographic input.
    // There is no focus-triggered enlargement or alternate scheduling signal.
    let mut from_maps = serde_json::to_value(at(-33.0, -70.0)).unwrap();
    from_maps["application"] = serde_json::json!("maps");
    from_maps["focused"] = serde_json::json!(true);
    let mut from_editor = from_maps.clone();
    from_editor["application"] = serde_json::json!("editor");
    from_editor["focused"] = serde_json::json!(false);
    let maps: ExplorationRequest = serde_json::from_value(from_maps).unwrap();
    let editor: ExplorationRequest = serde_json::from_value(from_editor).unwrap();
    assert_eq!(plan(&maps).unwrap(), plan(&editor).unwrap());
}

#[test]
fn requests_round_trip_with_snake_case_layer_and_domain_names() {
    let json = serde_json::to_value(ExplorationRequest::default()).unwrap();
    assert_eq!(json["layers"], serde_json::json!(["terrain"]));
    assert_eq!(json["domain"], "personal");
    assert_eq!(
        plan(&serde_json::from_value(json).unwrap()).unwrap(),
        plan(&ExplorationRequest::default()).unwrap()
    );
}
