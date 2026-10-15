use archie_sentry::contracts::*;
use archie_sentry::registry::*;
use archie_types::{Timestamp, TraceId};

fn intent() -> Intent {
    Intent {
        schema_version: 1,
        intent_id: "intent-1".into(),
        operation_id: 1,
        trace_id: TraceId::new(),
        resource_id: "tv-living".into(),
        command: Command::MediaPause,
        created_at: Timestamp(1000),
        deadline_at: Timestamp(1400),
        max_observation_age_ms: 100,
        verify: true,
    }
}
fn resource() -> ResourceDescriptor {
    ResourceDescriptor {
        resource_id: "tv-living".into(),
        edge_id: "edge-home".into(),
        adapter_id: "ha-home".into(),
        paired: true,
        capabilities: vec![CommandKind::MediaPause],
    }
}
fn observation() -> Observation {
    Observation {
        resource_id: "tv-living".into(),
        adapter_id: "ha-home".into(),
        evidence_id: "measurement-1".into(),
        observed_at: Timestamp(1200),
        origin: ObservationOrigin::Measurement,
        state: ResourceState::Media { playing: false },
    }
}

#[test]
fn intent_bounds_reject_unsafe_or_unversioned_input() {
    let valid = intent();
    assert!(valid.validate().is_ok());
    let mut cases = vec![];
    let mut bad = valid.clone();
    bad.schema_version = 2;
    cases.push(bad);
    let mut bad = valid.clone();
    bad.operation_id = 0;
    cases.push(bad);
    let mut bad = valid.clone();
    bad.resource_id = "http://other".into();
    cases.push(bad);
    let mut bad = valid.clone();
    bad.deadline_at = Timestamp(1000);
    cases.push(bad);
    let mut bad = valid.clone();
    bad.deadline_at = Timestamp(31001);
    cases.push(bad);
    let mut bad = valid.clone();
    bad.max_observation_age_ms = 0;
    cases.push(bad);
    let mut bad = valid.clone();
    bad.verify = false;
    cases.push(bad);
    let mut bad = valid.clone();
    bad.command = Command::LightSetBrightness { percent: 101 };
    cases.push(bad);
    let mut bad = valid.clone();
    bad.command = Command::ClimateSetTemperature {
        milli_celsius: 35001,
    };
    cases.push(bad);
    for bad in cases {
        assert!(bad.validate().is_err(), "{bad:?}");
    }
}

#[test]
fn unknown_wire_fields_are_rejected() {
    let mut value = serde_json::to_value(intent()).unwrap();
    value["authority"] = serde_json::json!("operator-approved");
    assert!(serde_json::from_value::<Intent>(value).is_err());
}

#[test]
fn digest_binds_target_payload_deadline_and_operation_id() {
    let first = intent();
    let hash = first.digest().unwrap();
    assert_eq!(hash.len(), 64);
    for mut second in [first.clone(), first.clone(), first.clone(), first.clone()] {
        second.command = Command::MediaPlay;
        assert_ne!(hash, second.digest().unwrap());
    }
    let mut second = first.clone();
    second.resource_id = "another-tv".into();
    assert_ne!(hash, second.digest().unwrap());
    let mut second = first.clone();
    second.operation_id = 2;
    assert_ne!(hash, second.digest().unwrap());
    let mut second = first;
    second.deadline_at = Timestamp(1401);
    assert_ne!(hash, second.digest().unwrap());
}

#[test]
fn resolver_refuses_unpaired_undeclared_and_duplicate_resources() {
    let mut registry = Registry::default();
    registry.register(resource()).unwrap();
    assert_eq!(
        registry
            .resolve(&intent(), "edge-home", "ha-home")
            .unwrap()
            .resource_id,
        "tv-living"
    );
    assert!(registry.register(resource()).is_err());
    assert!(registry
        .resolve(&intent(), "other-edge", "ha-home")
        .is_err());
    let mut other = intent();
    other.command = Command::MediaPlay;
    assert!(registry.resolve(&other, "edge-home", "ha-home").is_err());
    let mut registry = Registry::default();
    let mut unpaired = resource();
    unpaired.paired = false;
    registry.register(unpaired).unwrap();
    assert!(registry.resolve(&intent(), "edge-home", "ha-home").is_err());
}

#[test]
fn verification_requires_fresh_measurement_from_exact_target_after_dispatch() {
    let request = intent();
    let value = observation();
    assert!(value
        .validate_after(&request, "ha-home", Timestamp(1100), Timestamp(1250))
        .is_ok());
    assert!(request.command.matches(&value.state));
    let mut bad = value.clone();
    bad.origin = ObservationOrigin::ModelHypothesis;
    assert!(bad
        .validate_after(&request, "ha-home", Timestamp(1100), Timestamp(1250))
        .is_err());
    let mut bad = value.clone();
    bad.resource_id = "another-tv".into();
    assert!(bad
        .validate_after(&request, "ha-home", Timestamp(1100), Timestamp(1250))
        .is_err());
    assert!(value
        .validate_after(&request, "other-adapter", Timestamp(1100), Timestamp(1250))
        .is_err());
    assert!(value
        .validate_after(&request, "ha-home", Timestamp(1201), Timestamp(1250))
        .is_err());
    assert!(value
        .validate_after(&request, "ha-home", Timestamp(1100), Timestamp(1199))
        .is_err());
    assert!(value
        .validate_after(&request, "ha-home", Timestamp(1100), Timestamp(1301))
        .is_err());
    assert!(!request.command.matches(&ResourceState::Unknown));
    assert!(!Command::MediaPlay.matches(&value.state));
}

fn tv(
    power_on: bool,
    showing_home: bool,
    active_app: Option<&str>,
    playing: Option<bool>,
) -> ResourceState {
    ResourceState::Tv {
        power_on,
        showing_home,
        active_app: active_app.map(str::to_string),
        playing,
    }
}

#[test]
fn remote_keys_match_only_their_measurable_postcondition() {
    assert!(Command::RemoteKey {
        key: RemoteKeyName::PowerOn
    }
    .matches(&tv(true, false, None, None)));
    assert!(!Command::RemoteKey {
        key: RemoteKeyName::PowerOn
    }
    .matches(&tv(false, false, None, None)));
    assert!(Command::RemoteKey {
        key: RemoteKeyName::PowerOff
    }
    .matches(&tv(false, false, None, None)));
    assert!(Command::RemoteKey {
        key: RemoteKeyName::Home
    }
    .matches(&tv(true, true, None, None)));
    assert!(!Command::RemoteKey {
        key: RemoteKeyName::Home
    }
    .matches(&tv(true, false, Some("12"), Some(true))));
    assert!(Command::RemoteKey {
        key: RemoteKeyName::Pause
    }
    .matches(&tv(true, false, None, Some(false))));
    // A paused TV that is off does not prove the key landed.
    assert!(!Command::RemoteKey {
        key: RemoteKeyName::Pause
    }
    .matches(&tv(false, false, None, Some(false))));
    // Unknown playback never verifies Play.
    assert!(!Command::RemoteKey {
        key: RemoteKeyName::Play
    }
    .matches(&tv(true, false, None, None)));
}

#[test]
fn app_launch_matches_only_the_requested_app_on_a_powered_tv() {
    let launch = Command::AppLaunch {
        app_id: "12".into(),
    };
    assert!(launch.matches(&tv(true, false, Some("12"), None)));
    assert!(!launch.matches(&tv(true, false, Some("2285"), None)));
    assert!(!launch.matches(&tv(false, false, Some("12"), None)));
    let mut bad = intent();
    bad.command = Command::AppLaunch {
        app_id: "https://evil.example".into(),
    };
    assert!(bad.validate().is_err());
}

#[test]
fn print_submit_matches_the_content_named_queue_entry() {
    let sha256 = "a".repeat(64);
    let print = Command::PrintSubmit {
        sha256: sha256.clone(),
        format: PrintFormat::Jpeg,
        pages: 2,
    };
    assert_eq!(
        Command::print_job_name(&sha256).unwrap(),
        "archie-aaaaaaaaaaaaaaaa"
    );
    assert!(print.matches(&ResourceState::Printer {
        idle: false,
        job_present: Some("archie-aaaaaaaaaaaaaaaa".into()),
    }));
    assert!(!print.matches(&ResourceState::Printer {
        idle: true,
        job_present: None,
    }));
    for bad_sha in ["A".repeat(64), "g".repeat(64), "a".repeat(63)] {
        let bad = Command::PrintSubmit {
            sha256: bad_sha,
            format: PrintFormat::Pdf,
            pages: 1,
        };
        assert!(bad.validate().is_err());
    }
    let bad_pages = Command::PrintSubmit {
        sha256,
        format: PrintFormat::PwgRaster,
        pages: 0,
    };
    assert!(bad_pages.validate().is_err());
}

#[test]
fn measured_tv_and_printer_observations_are_bounded() {
    let request = intent();
    let good = Observation {
        state: tv(true, false, Some("12"), Some(true)),
        ..observation()
    };
    assert!(good
        .validate_after(&request, "ha-home", Timestamp(1100), Timestamp(1250))
        .is_ok());
    let bad_app = Observation {
        state: tv(true, false, Some("http://other"), None),
        ..observation()
    };
    assert!(bad_app
        .validate_after(&request, "ha-home", Timestamp(1100), Timestamp(1250))
        .is_err());
    let bad_job = Observation {
        state: ResourceState::Printer {
            idle: false,
            job_present: Some("job\nwith-control".into()),
        },
        ..observation()
    };
    assert!(bad_job
        .validate_after(&request, "ha-home", Timestamp(1100), Timestamp(1250))
        .is_err());
}

fn host(
    reachable: bool,
    awake: Option<bool>,
    session_locked: Option<bool>,
    volume_percent: Option<u8>,
    foreground_app: Option<&str>,
) -> ResourceState {
    ResourceState::Host {
        reachable,
        awake,
        session_locked,
        volume_percent,
        foreground_app: foreground_app.map(str::to_string),
    }
}

#[test]
fn host_and_router_commands_match_only_their_measurable_postcondition() {
    // Wake-on-LAN: the packet proves nothing; the host answering does.
    assert!(Command::WakeHost.matches(&host(true, None, None, None, None)));
    assert!(!Command::WakeHost.matches(&host(false, None, None, None, None)));

    // Port mappings verify against the router's own list, never the request.
    let add = Command::PortMapping {
        present: true,
        external_port: 25565,
        protocol: PortProtocol::Tcp,
    };
    assert!(add.matches(&ResourceState::Router {
        mapping_present: Some(true),
        wifi_enabled: None,
        device_blocked: None,
    }));
    assert!(!add.matches(&ResourceState::Router {
        mapping_present: Some(false),
        wifi_enabled: None,
        device_blocked: None,
    }));
    // A router that cannot say never verifies.
    assert!(!add.matches(&ResourceState::Router {
        mapping_present: None,
        wifi_enabled: None,
        device_blocked: None,
    }));
    let mut bad = intent();
    bad.command = Command::PortMapping {
        present: true,
        external_port: 0,
        protocol: PortProtocol::Udp,
    };
    assert!(bad.validate().is_err());

    // Blocking one device: the address must name a single device, and only the
    // router's own firewall answer verifies it.
    let block = Command::DeviceBlock {
        mac: "de:ad:be:ef:00:02".into(),
        blocked: true,
    };
    assert!(block.validate().is_ok());
    assert_eq!(block.capability(), CommandKind::DeviceBlock);
    let router = |device_blocked| ResourceState::Router {
        mapping_present: None,
        wifi_enabled: None,
        device_blocked,
    };
    assert!(block.matches(&router(Some(true))));
    assert!(!block.matches(&router(Some(false))));
    assert!(
        !block.matches(&router(None)),
        "a router that cannot say never verifies"
    );
    for mac in [
        "DE:AD:BE:EF:00:02",
        "de:ad:be:ef:00",
        "de-ad-be-ef-00-02",
        "ff:ff:ff:ff:ff:ff",
        "01:00:5e:00:00:01",
        "00:00:00:00:00:00",
        "de:ad:be:ef:00:02; reboot",
    ] {
        let bad = Command::DeviceBlock {
            mac: mac.into(),
            blocked: true,
        };
        assert!(bad.validate().is_err(), "{mac} must be refused");
    }
    // A state recorded before the field existed still reads, as unknown.
    let old: ResourceState =
        serde_json::from_str(r#"{"kind":"router","mapping_present":true,"wifi_enabled":null}"#)
            .unwrap();
    assert_eq!(old, router_with_mapping());

    assert!(
        Command::WifiSet { on: false }.matches(&ResourceState::Router {
            mapping_present: None,
            wifi_enabled: Some(false),
            device_blocked: None,
        })
    );
    assert!(
        !Command::WifiSet { on: true }.matches(&ResourceState::Router {
            mapping_present: None,
            wifi_enabled: Some(false),
            device_blocked: None,
        })
    );

    assert!(Command::SessionLock { locked: true }.matches(&host(
        true,
        None,
        Some(true),
        None,
        None
    )));
    assert!(!Command::SessionLock { locked: true }.matches(&host(
        true,
        None,
        Some(false),
        None,
        None
    )));
    assert!(!Command::SessionLock { locked: true }.matches(&host(true, None, None, None, None)));

    assert!(Command::VolumeSet { percent: 40 }.matches(&host(true, None, None, Some(40), None)));
    assert!(!Command::VolumeSet { percent: 40 }.matches(&host(true, None, None, Some(41), None)));
    let mut bad = intent();
    bad.command = Command::VolumeSet { percent: 101 };
    assert!(bad.validate().is_err());

    // Android app launch: foreground app is the proof, on an awake device.
    let launch = Command::AppLaunch {
        app_id: "com.whatsapp".into(),
    };
    assert!(launch.matches(&host(true, Some(true), None, None, Some("com.whatsapp"))));
    assert!(!launch.matches(&host(true, Some(false), None, None, Some("com.whatsapp"))));
    assert!(!launch.matches(&host(true, Some(true), None, None, Some("com.other"))));
}

#[test]
fn measured_host_observations_are_bounded() {
    let request = intent();
    let good = Observation {
        state: host(
            true,
            Some(true),
            Some(false),
            Some(65),
            Some("com.whatsapp"),
        ),
        ..observation()
    };
    assert!(good
        .validate_after(&request, "ha-home", Timestamp(1100), Timestamp(1250))
        .is_ok());
    let bad_volume = Observation {
        state: host(true, None, None, Some(101), None),
        ..observation()
    };
    assert!(bad_volume
        .validate_after(&request, "ha-home", Timestamp(1100), Timestamp(1250))
        .is_err());
    let bad_app = Observation {
        state: host(true, None, None, None, Some("http://other")),
        ..observation()
    };
    assert!(bad_app
        .validate_after(&request, "ha-home", Timestamp(1100), Timestamp(1250))
        .is_err());
}

fn router_with_mapping() -> ResourceState {
    ResourceState::Router {
        mapping_present: Some(true),
        wifi_enabled: None,
        device_blocked: None,
    }
}
