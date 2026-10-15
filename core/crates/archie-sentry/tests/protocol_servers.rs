//! Every adapter against an in-process server that implements its protocol
//! with state (see `tests/servers`). Each action goes Intent → policy →
//! approval → observation → dispatch → observation, and the postcondition is
//! read back from the server's own state. These are protocol tests on
//! loopback: no physical device was involved.
mod servers;

use archie_sentry::{
    adapters::{
        home_assistant::HomeAssistant,
        ipp::IppAdapter,
        matter::{Matter, MatterKind, MatterTarget},
        mqtt::{Mqtt, MqttBinding},
        onvif::OnvifProbe,
        roku::Roku,
        router::{probe_router, IgdClient, RouterAdapter, RouterConfig},
    },
    contracts::*,
    engine::*,
    registry::ResourceDescriptor,
};
use serde_json::json;
use servers::{execute, home_assistant::HomeAssistantServer, igd, ipp, matter, mqtt, onvif, roku};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, net::Ipv4Addr, sync::Arc};

fn resource(id: &str, adapter: &str, capabilities: &[CommandKind]) -> ResourceDescriptor {
    ResourceDescriptor {
        resource_id: id.into(),
        edge_id: "edge".into(),
        adapter_id: adapter.into(),
        paired: true,
        capabilities: capabilities.to_vec(),
    }
}

// ---------- Roku ECP ----------

#[tokio::test]
async fn roku_power_launch_home_and_pause_change_the_tv_and_are_verified() {
    let tv = roku::RokuTv::start(&[("12", "Netflix"), ("837", "YouTube")]).await;
    let mut adapter = Roku::new(
        "edge",
        "roku",
        BTreeMap::from([("tv".into(), tv.base.clone())]),
    )
    .unwrap();
    let r = resource(
        "tv",
        "roku",
        &[CommandKind::RemoteKey, CommandKind::AppLaunch],
    );
    let key = |key| Command::RemoteKey { key };

    let on = execute(&r, &mut adapter, key(RemoteKeyName::PowerOn)).await;
    assert_eq!(on.status, ExecutionStatus::Verified, "{on:?}");
    assert!(tv.state.lock().unwrap().power_on);

    let launched = execute(
        &r,
        &mut adapter,
        Command::AppLaunch {
            app_id: "837".into(),
        },
    )
    .await;
    assert_eq!(launched.status, ExecutionStatus::Verified, "{launched:?}");
    let paused = execute(&r, &mut adapter, key(RemoteKeyName::Pause)).await;
    assert_eq!(paused.status, ExecutionStatus::Verified, "{paused:?}");
    assert_eq!(tv.state.lock().unwrap().playing, Some(false));

    let home = execute(&r, &mut adapter, key(RemoteKeyName::Home)).await;
    assert_eq!(home.status, ExecutionStatus::Verified, "{home:?}");
    assert_eq!(tv.state.lock().unwrap().active_app, None);

    let off = execute(&r, &mut adapter, key(RemoteKeyName::PowerOff)).await;
    assert_eq!(off.status, ExecutionStatus::Verified, "{off:?}");
    assert!(tv.log().iter().any(|l| l == "POST /keypress/PowerOff"));
}

#[tokio::test]
async fn roku_launching_a_channel_that_is_not_installed_is_never_verified() {
    let tv = roku::RokuTv::start(&[("12", "Netflix")]).await;
    let mut adapter = Roku::new(
        "edge",
        "roku",
        BTreeMap::from([("tv".into(), tv.base.clone())]),
    )
    .unwrap();
    let r = resource("tv", "roku", &[CommandKind::AppLaunch]);
    let result = execute(
        &r,
        &mut adapter,
        Command::AppLaunch {
            app_id: "999".into(),
        },
    )
    .await;
    assert_eq!(result.status, ExecutionStatus::Indeterminate);
    assert!(!tv.state.lock().unwrap().power_on);
}

// ---------- Home Assistant REST ----------

#[tokio::test]
async fn home_assistant_switch_light_and_climate_are_verified_against_entity_state() {
    let ha = HomeAssistantServer::start(
        "loopback-token",
        &[
            ("switch.office", "off", json!({})),
            ("light.desk", "off", json!({"brightness": null})),
            ("climate.office", "heat", json!({"temperature": 20.0})),
        ],
    )
    .await;
    let mut adapter = HomeAssistant::new(
        "edge",
        "ha",
        &ha.base,
        "loopback-token",
        BTreeMap::from([
            ("office".into(), "switch.office".into()),
            ("desk".into(), "light.desk".into()),
            ("thermo".into(), "climate.office".into()),
        ]),
    )
    .unwrap();
    let switch = execute(
        &resource("office", "ha", &[CommandKind::SwitchSet]),
        &mut adapter,
        Command::SwitchSet { on: true },
    )
    .await;
    assert_eq!(switch.status, ExecutionStatus::Verified, "{switch:?}");
    assert_eq!(
        ha.state.lock().unwrap().entities["switch.office"].state,
        "on"
    );

    let light = execute(
        &resource("desk", "ha", &[CommandKind::LightSetBrightness]),
        &mut adapter,
        Command::LightSetBrightness { percent: 40 },
    )
    .await;
    assert_eq!(light.status, ExecutionStatus::Verified, "{light:?}");
    assert_eq!(
        ha.state.lock().unwrap().entities["light.desk"].attributes["brightness"],
        102
    );

    let climate = execute(
        &resource("thermo", "ha", &[CommandKind::ClimateSetTemperature]),
        &mut adapter,
        Command::ClimateSetTemperature {
            milli_celsius: 22_500,
        },
    )
    .await;
    assert_eq!(climate.status, ExecutionStatus::Verified, "{climate:?}");
    assert_eq!(
        ha.state.lock().unwrap().entities["climate.office"].attributes["temperature"],
        22.5
    );
    assert!(ha
        .state
        .lock()
        .unwrap()
        .calls
        .contains(&"climate/set_temperature".to_string()));
}

#[tokio::test]
async fn home_assistant_refuses_a_wrong_token_and_fahrenheit_never_reaches_the_thermostat() {
    let ha = HomeAssistantServer::start(
        "right-token",
        &[("climate.office", "heat", json!({"temperature": 20.0}))],
    )
    .await;
    let entities = BTreeMap::from([("thermo".into(), "climate.office".to_string())]);
    let r = resource("thermo", "ha", &[CommandKind::ClimateSetTemperature]);
    let mut wrong =
        HomeAssistant::new("edge", "ha", &ha.base, "wrong-token", entities.clone()).unwrap();
    assert_eq!(wrong.observe(&r).await, Err(AdapterError::Authentication));

    ha.state.lock().unwrap().temperature_unit = "°F".into();
    let mut adapter = HomeAssistant::new("edge", "ha", &ha.base, "right-token", entities).unwrap();
    let result = execute(
        &r,
        &mut adapter,
        Command::ClimateSetTemperature {
            milli_celsius: 22_000,
        },
    )
    .await;
    assert_ne!(result.status, ExecutionStatus::Verified);
    assert!(result.dispatched_at.is_none());
    assert!(!ha
        .state
        .lock()
        .unwrap()
        .calls
        .contains(&"climate/set_temperature".to_string()));
}

// ---------- MQTT ----------

async fn mqtt_home(credentials: Option<(&'static str, &'static str)>) -> (mqtt::Broker, Mqtt) {
    let broker = mqtt::Broker::start(credentials).await;
    mqtt::Device::attach(
        &broker.address,
        "plug",
        "home/plug/set",
        "home/plug/state",
        b"OFF",
        credentials,
        mqtt::plug(),
    )
    .await;
    mqtt::Device::attach(
        &broker.address,
        "dimmer",
        "home/dim/set",
        "home/dim/state",
        b"0",
        credentials,
        mqtt::dimmer(),
    )
    .await;
    mqtt::Device::attach(
        &broker.address,
        "thermo",
        "home/heat/set",
        "home/heat/state",
        b"20.0",
        credentials,
        mqtt::thermostat(),
    )
    .await;
    let adapter = Mqtt::new(
        "edge",
        "mqtt",
        &broker.address,
        credentials.map(|(u, p)| (u.to_owned(), p.to_owned())),
        BTreeMap::from([
            (
                "plug".into(),
                MqttBinding::switch("home/plug/set", "home/plug/state", "ON", "OFF"),
            ),
            (
                "dim".into(),
                MqttBinding::light("home/dim/set", "home/dim/state", "{percent}"),
            ),
            (
                "heat".into(),
                MqttBinding::climate("home/heat/set", "home/heat/state", "{celsius}"),
            ),
        ]),
    )
    .unwrap();
    (broker, adapter)
}

#[tokio::test]
async fn mqtt_devices_behind_a_broker_report_their_own_state_and_rounding_diverges() {
    let (broker, mut adapter) = mqtt_home(Some(("archie", "loopback-pass"))).await;
    let plug = execute(
        &resource("plug", "mqtt", &[CommandKind::SwitchSet]),
        &mut adapter,
        Command::SwitchSet { on: true },
    )
    .await;
    assert_eq!(plug.status, ExecutionStatus::Verified, "{plug:?}");
    assert_eq!(
        broker.retained("home/plug/state").as_deref(),
        Some(b"ON".as_slice())
    );

    let dim = execute(
        &resource("dim", "mqtt", &[CommandKind::LightSetBrightness]),
        &mut adapter,
        Command::LightSetBrightness { percent: 55 },
    )
    .await;
    assert_eq!(dim.status, ExecutionStatus::Verified, "{dim:?}");

    let heat = resource("heat", "mqtt", &[CommandKind::ClimateSetTemperature]);
    let exact = execute(
        &heat,
        &mut adapter,
        Command::ClimateSetTemperature {
            milli_celsius: 21_500,
        },
    )
    .await;
    assert_eq!(exact.status, ExecutionStatus::Verified, "{exact:?}");
    // The thermostat only takes half degrees: 22.3 settles on 22.5, and the
    // receipt says so instead of claiming 22.3.
    let rounded = execute(
        &heat,
        &mut adapter,
        Command::ClimateSetTemperature {
            milli_celsius: 22_300,
        },
    )
    .await;
    assert_eq!(rounded.status, ExecutionStatus::Diverged, "{rounded:?}");
    assert_eq!(
        broker.retained("home/heat/state").as_deref(),
        Some(b"22.5".as_slice())
    );
    // The command went to the command topic; the adapter listened on the state topic first.
    let log = broker.log();
    let sub = log
        .iter()
        .position(|l| l.ends_with("SUBSCRIBE home/plug/state"))
        .unwrap();
    let publish = log
        .iter()
        .position(|l| l.ends_with("PUBLISH home/plug/set"))
        .unwrap();
    assert!(sub < publish, "{log:?}");
}

#[tokio::test]
async fn mqtt_wrong_credentials_are_refused_by_the_broker() {
    let (_broker, _) = mqtt_home(Some(("archie", "loopback-pass"))).await;
    let broker = mqtt::Broker::start(Some(("archie", "loopback-pass"))).await;
    let mut adapter = Mqtt::new(
        "edge",
        "mqtt",
        &broker.address,
        Some(("archie".into(), "wrong".into())),
        BTreeMap::from([(
            "plug".into(),
            MqttBinding::switch("p/set", "p/state", "ON", "OFF"),
        )]),
    )
    .unwrap();
    let r = resource("plug", "mqtt", &[CommandKind::SwitchSet]);
    assert_eq!(adapter.observe(&r).await, Err(AdapterError::Authentication));
    assert_eq!(
        adapter.apply(&r, &Command::SwitchSet { on: true }).await,
        Err(AdapterError::Authentication)
    );
}

#[test]
fn mqtt_broker_filters_follow_the_spec() {
    assert!(mqtt::filter_matches("home/+/state", "home/plug/state"));
    assert!(mqtt::filter_matches("home/#", "home/plug/state"));
    assert!(!mqtt::filter_matches("home/+", "home/plug/state"));
    assert!(!mqtt::filter_matches("home/plug/set", "home/plug/state"));
}

// ---------- IPP ----------

fn spool(name: &str, document: &[u8]) -> (std::path::PathBuf, String) {
    let dir = std::env::temp_dir().join(format!("archie-ipp-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let sha = format!("{:x}", Sha256::digest(document));
    std::fs::write(dir.join(&sha), document).unwrap();
    (dir, sha)
}
const UUID: &str = "urn:uuid:e3248000-80ce-11db-8000-0000000000aa";

#[tokio::test]
async fn ipp_print_submit_lands_in_the_printer_queue_with_the_bytes_that_were_verified() {
    let printer = ipp::Printer::start(UUID, &["application/pdf", "image/jpeg"]).await;
    let document = [&[0xff, 0xd8, 0xff, 0xe0][..], b"loopback page"].concat();
    let (dir, sha) = spool("ok", &document);
    let mut adapter = IppAdapter::new("edge", "ipp", &printer.uri, "printer", dir.clone()).unwrap();
    let r = resource("printer", "ipp", &[CommandKind::PrintSubmit]);
    let result = execute(
        &r,
        &mut adapter,
        Command::PrintSubmit {
            sha256: sha.clone(),
            format: PrintFormat::Jpeg,
            pages: 1,
        },
    )
    .await;
    assert_eq!(result.status, ExecutionStatus::Verified, "{result:?}");
    {
        let state = printer.state.lock().unwrap();
        assert_eq!(state.jobs.len(), 1);
        assert_eq!(state.jobs[0].name, Command::print_job_name(&sha).unwrap());
        assert_eq!(
            state.jobs[0].sha256, sha,
            "the printer holds exactly the verified bytes"
        );
    }
    // Once the printer finishes, the not-completed queue no longer lists it.
    printer.finish_all();
    let after = adapter.observe(&r).await.unwrap();
    assert_eq!(
        after.state,
        ResourceState::Printer {
            idle: true,
            job_present: None
        }
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn ipp_a_format_the_printer_does_not_declare_is_never_sent() {
    let printer = ipp::Printer::start(UUID, &["image/pwg-raster"]).await;
    let (dir, sha) = spool("fmt", b"%PDF-1.7 loopback");
    let mut adapter = IppAdapter::new("edge", "ipp", &printer.uri, "printer", dir.clone()).unwrap();
    let result = execute(
        &resource("printer", "ipp", &[CommandKind::PrintSubmit]),
        &mut adapter,
        Command::PrintSubmit {
            sha256: sha,
            format: PrintFormat::Pdf,
            pages: 1,
        },
    )
    .await;
    assert_ne!(result.status, ExecutionStatus::Verified);
    assert!(printer.state.lock().unwrap().jobs.is_empty());
    assert!(!printer.log().is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn ipp_print_job_is_followed_until_the_printer_reports_it_completed() {
    use archie_sentry::adapters::ipp::{print::JobState, IppClient};
    let printer = ipp::Printer::start(UUID, &["application/pdf"]).await;
    let client = IppClient::new(&printer.uri).unwrap();
    let job = client
        .print_job(1, "archie-loopback", "application/pdf", b"%PDF-1.7 x")
        .await
        .unwrap();
    assert_eq!(job.state, JobState::Pending);
    let mut states = vec![];
    for request in 2..10 {
        let snapshot = client.job(request, job.job_id).await.unwrap();
        states.push(snapshot.state);
        if snapshot.state.ended() {
            break;
        }
    }
    assert_eq!(
        states,
        [JobState::Pending, JobState::Processing, JobState::Completed]
    );
    // Bytes that are not what they claim to be are aborted by the printer.
    let bad = client
        .print_job(20, "archie-bad", "application/pdf", b"not a pdf")
        .await
        .unwrap();
    assert_eq!(bad.state, JobState::Aborted);
    assert_eq!(bad.reasons, ["document-format-error"]);
}

// ---------- ONVIF ----------

#[tokio::test]
async fn onvif_probe_authenticates_with_a_verified_digest_and_reads_stream_uris() {
    let camera = onvif::Camera::start(
        Some(("viewer", "loopback-secret")),
        &[
            ("main", "rtsp://192.168.1.20:554/main"),
            ("sub", "rtsp://192.168.1.20:554/sub"),
        ],
    )
    .await;
    let probe = OnvifProbe::new(
        &camera.url,
        Some(("viewer".into(), "loopback-secret".into())),
    )
    .unwrap();
    let info = probe.probe().await.unwrap();
    assert_eq!(
        (info.manufacturer.as_str(), info.model.as_str()),
        ("Loopback", "Cam-1")
    );
    assert_eq!(
        info.stream_uris,
        [
            "rtsp://192.168.1.20:554/main",
            "rtsp://192.168.1.20:554/sub"
        ]
    );
    // Four requests, four distinct nonces, none refused.
    let state = camera.state.lock().unwrap();
    assert_eq!((state.nonces.len(), state.refused), (4, 0));
}

#[tokio::test]
async fn onvif_a_wrong_password_is_refused_by_the_camera() {
    let camera = onvif::Camera::start(
        Some(("viewer", "loopback-secret")),
        &[("main", "rtsp://192.168.1.20/m")],
    )
    .await;
    let probe = OnvifProbe::new(&camera.url, Some(("viewer".into(), "guess".into()))).unwrap();
    assert!(probe.probe().await.is_err());
    assert_eq!(camera.state.lock().unwrap().refused, 1);
}

// ---------- UPnP IGD ----------

const LAN_CLIENT: Ipv4Addr = Ipv4Addr::new(192, 168, 1, 10);

#[tokio::test]
async fn igd_port_mappings_are_added_and_removed_in_the_gateway_table_and_verified() {
    let gateway = igd::Gateway::start().await;
    let capabilities =
        probe_router(Some(&IgdClient::new(&gateway.control_url).unwrap()), None).await;
    assert!(
        capabilities.igd,
        "the probe found IGD by asking the gateway"
    );
    let mut adapter = RouterAdapter::new(
        "edge",
        "router",
        "router",
        RouterConfig {
            control_url: Some(gateway.control_url.clone()),
            lan_client: LAN_CLIENT,
            ssh: None,
            protected_macs: vec![],
        },
        capabilities,
        Arc::new(NoProcesses),
    )
    .unwrap();
    let r = resource("router", "router", &[CommandKind::PortMapping]);
    let add = Command::PortMapping {
        present: true,
        external_port: 8443,
        protocol: PortProtocol::Tcp,
    };
    let added = execute(&r, &mut adapter, add).await;
    assert_eq!(added.status, ExecutionStatus::Verified, "{added:?}");
    let entry = gateway.state.lock().unwrap().table[&(8443, "TCP".to_string())].clone();
    assert_eq!(
        (entry.internal_client, entry.internal_port),
        (LAN_CLIENT, 8443)
    );

    let listed = IgdClient::new(&gateway.control_url)
        .unwrap()
        .list_port_mappings()
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);

    let remove = Command::PortMapping {
        present: false,
        external_port: 8443,
        protocol: PortProtocol::Tcp,
    };
    let removed = execute(&r, &mut adapter, remove).await;
    assert_eq!(removed.status, ExecutionStatus::Verified, "{removed:?}");
    assert!(gateway.state.lock().unwrap().table.is_empty());
}

#[tokio::test]
async fn igd_a_port_held_by_another_client_is_a_conflict_not_a_success() {
    let gateway = igd::Gateway::start().await;
    gateway.state.lock().unwrap().table.insert(
        (8443, "TCP".into()),
        igd::Mapping {
            internal_port: 443,
            internal_client: Ipv4Addr::new(192, 168, 1, 77),
            enabled: true,
            description: "someone else".into(),
        },
    );
    let capabilities =
        probe_router(Some(&IgdClient::new(&gateway.control_url).unwrap()), None).await;
    let mut adapter = RouterAdapter::new(
        "edge",
        "router",
        "router",
        RouterConfig {
            control_url: Some(gateway.control_url.clone()),
            lan_client: LAN_CLIENT,
            ssh: None,
            protected_macs: vec![],
        },
        capabilities,
        Arc::new(NoProcesses),
    )
    .unwrap();
    let result = execute(
        &resource("router", "router", &[CommandKind::PortMapping]),
        &mut adapter,
        Command::PortMapping {
            present: true,
            external_port: 8443,
            protocol: PortProtocol::Tcp,
        },
    )
    .await;
    assert_ne!(result.status, ExecutionStatus::Verified);
    let table = gateway.state.lock().unwrap();
    assert_eq!(
        table.table[&(8443, "TCP".to_string())].internal_client,
        Ipv4Addr::new(192, 168, 1, 77)
    );
}

struct NoProcesses;
impl archie_sentry::adapters::process::CommandRunner for NoProcesses {
    fn run(
        &self,
        _: &[String],
        _: std::time::Duration,
    ) -> Result<archie_sentry::adapters::process::CommandOutput, AdapterError> {
        panic!("IGD needs no process")
    }
}

// ---------- Matter (through chip-tool) ----------

fn chip_path() -> &'static str {
    if cfg!(windows) {
        "C:/tools/chip-tool.exe"
    } else {
        "/usr/bin/chip-tool"
    }
}

#[tokio::test]
async fn matter_plug_light_and_thermostat_change_node_state_and_are_verified() {
    let start = matter::Endpoint {
        on: false,
        level: 0,
        heating_setpoint: 2000,
    };
    let chip = matter::ChipTool::new(chip_path())
        .commissioned(11, 1, start.clone())
        .commissioned(12, 1, start.clone())
        .commissioned(13, 1, start);
    let targets = BTreeMap::from([
        (
            "plug".into(),
            MatterTarget {
                node_id: 11,
                endpoint: 1,
                kind: MatterKind::Switch,
            },
        ),
        (
            "lamp".into(),
            MatterTarget {
                node_id: 12,
                endpoint: 1,
                kind: MatterKind::Light,
            },
        ),
        (
            "heat".into(),
            MatterTarget {
                node_id: 13,
                endpoint: 1,
                kind: MatterKind::Climate,
            },
        ),
    ]);
    let mut adapter = Matter::new(
        "edge",
        "matter",
        chip_path(),
        targets,
        Arc::new(chip.clone()),
    )
    .unwrap();

    let plug = execute(
        &resource("plug", "matter", &[CommandKind::SwitchSet]),
        &mut adapter,
        Command::SwitchSet { on: true },
    )
    .await;
    assert_eq!(plug.status, ExecutionStatus::Verified, "{plug:?}");
    let lamp = execute(
        &resource("lamp", "matter", &[CommandKind::LightSetBrightness]),
        &mut adapter,
        Command::LightSetBrightness { percent: 50 },
    )
    .await;
    assert_eq!(lamp.status, ExecutionStatus::Verified, "{lamp:?}");
    let heat = resource("heat", "matter", &[CommandKind::ClimateSetTemperature]);
    let set = execute(
        &heat,
        &mut adapter,
        Command::ClimateSetTemperature {
            milli_celsius: 21_500,
        },
    )
    .await;
    assert_eq!(set.status, ExecutionStatus::Verified, "{set:?}");
    {
        let fabric = chip.fabric.lock().unwrap();
        assert!(fabric.nodes[&(11, 1)].on);
        assert_eq!(fabric.nodes[&(12, 1)].level, 127);
        assert_eq!(fabric.nodes[&(13, 1)].heating_setpoint, 2150);
    }
    // 32 °C is outside what this thermostat accepts: CONSTRAINT_ERROR, never verified.
    let refused = execute(
        &heat,
        &mut adapter,
        Command::ClimateSetTemperature {
            milli_celsius: 32_000,
        },
    )
    .await;
    assert_ne!(refused.status, ExecutionStatus::Verified);
    assert_eq!(
        chip.fabric.lock().unwrap().nodes[&(13, 1)].heating_setpoint,
        2150
    );
}

#[tokio::test]
async fn matter_a_node_must_be_commissioned_before_it_answers() {
    let chip = matter::ChipTool::new(chip_path());
    chip.fabric.lock().unwrap().uncommissioned.insert(
        20202021,
        matter::Endpoint {
            on: false,
            level: 0,
            heating_setpoint: 2000,
        },
    );
    let targets = BTreeMap::from([(
        "plug".into(),
        MatterTarget {
            node_id: 31,
            endpoint: 1,
            kind: MatterKind::Switch,
        },
    )]);
    let mut adapter = Matter::new(
        "edge",
        "matter",
        chip_path(),
        targets,
        Arc::new(chip.clone()),
    )
    .unwrap();
    let r = resource("plug", "matter", &[CommandKind::SwitchSet]);
    assert_eq!(adapter.observe(&r).await, Err(AdapterError::Unavailable));
    assert_eq!(
        adapter.pair_onnetwork(31, 11111111),
        Err(AdapterError::Unavailable)
    );
    adapter.pair_onnetwork(31, 20202021).unwrap();
    let on = execute(&r, &mut adapter, Command::SwitchSet { on: true }).await;
    assert_eq!(on.status, ExecutionStatus::Verified, "{on:?}");
}
