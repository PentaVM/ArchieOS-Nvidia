//! Router adapter tests: a mock WANIPConnection SOAP server on loopback for
//! the IGD channel, and a FakeRunner (CommandRunner) recording exact argv for
//! the SSH channel. Nothing here touches a real router.
use archie_sentry::{
    adapters::{
        process::{CommandOutput, CommandRunner},
        router::{
            clients::{parse_dnsmasq_leases, parse_ip_neigh, router_clients},
            igd::{IgdClient, SpecificEntry},
            probe::{probe_router, RouterCapabilities},
            ssh::{SshChannel, SshConfig},
            RouterAdapter, RouterConfig,
        },
    },
    contracts::*,
    engine::*,
    registry::ResourceDescriptor,
};
use std::{
    collections::BTreeMap,
    net::Ipv4Addr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

// ---------- shared fixtures ----------

fn resource(caps: &[CommandKind]) -> ResourceDescriptor {
    ResourceDescriptor {
        resource_id: "router".into(),
        edge_id: "edge".into(),
        adapter_id: "router".into(),
        paired: true,
        capabilities: caps.to_vec(),
    }
}

// `Path::is_absolute` is per-platform: `C:/…` is relative on Unix.
fn ssh_path() -> &'static str {
    if cfg!(windows) {
        "C:/tools/ssh.exe"
    } else {
        "/usr/bin/ssh"
    }
}
fn key_path() -> &'static str {
    if cfg!(windows) {
        "C:/keys/router_ed25519"
    } else {
        "/etc/archie/keys/router_ed25519"
    }
}
fn ssh_config() -> SshConfig {
    SshConfig::new(
        ssh_path(),
        "admin",
        Ipv4Addr::new(192, 168, 1, 1),
        Some(key_path().into()),
    )
    .unwrap()
}

const LAN_CLIENT: Ipv4Addr = Ipv4Addr::new(192, 168, 1, 10);

fn caps(igd: bool, ssh: bool, radio_tools: &[&str]) -> RouterCapabilities {
    RouterCapabilities {
        igd,
        ssh,
        radio_tools: radio_tools.iter().map(|t| t.to_string()).collect(),
        dhcp_leases_path: None,
        firmware_hint: None,
        firewall_tools: vec![],
    }
}

fn igd_adapter(url: &str, igd_ok: bool) -> RouterAdapter {
    RouterAdapter::new(
        "edge",
        "router",
        "router",
        RouterConfig {
            control_url: Some(format!("{url}/upnp/control/WANIPConnection1")),
            lan_client: LAN_CLIENT,
            ssh: None,
            protected_macs: vec![],
        },
        caps(igd_ok, false, &[]),
        FakeRunner::new(vec![]),
    )
    .unwrap()
}

// ---------- FakeRunner ----------

/// A CommandRunner that records every argv and answers by exact argv-suffix
/// match against a script. Unscripted commands exit 255, like a router that
/// does not have the tool at all.
struct FakeRunner {
    calls: Mutex<Vec<Vec<String>>>,
    scripted: Vec<(Vec<String>, i32, Vec<u8>)>,
}
impl FakeRunner {
    fn new(scripted: Vec<(&[&str], i32, &str)>) -> Arc<Self> {
        Arc::new(Self {
            calls: Mutex::new(vec![]),
            scripted: scripted
                .into_iter()
                .map(|(tail, status, stdout)| {
                    (
                        tail.iter().map(|s| s.to_string()).collect(),
                        status,
                        stdout.as_bytes().to_vec(),
                    )
                })
                .collect(),
        })
    }
    fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }
}
impl CommandRunner for FakeRunner {
    fn run(&self, argv: &[String], _timeout: Duration) -> Result<CommandOutput, AdapterError> {
        self.calls.lock().unwrap().push(argv.to_vec());
        for (tail, status, stdout) in &self.scripted {
            if argv.len() >= tail.len() && argv[argv.len() - tail.len()..] == tail[..] {
                return Ok(CommandOutput {
                    status: *status,
                    stdout: stdout.clone(),
                    stderr: vec![],
                });
            }
        }
        Ok(CommandOutput {
            status: 255,
            stdout: vec![],
            stderr: b"command not found".to_vec(),
        })
    }
}

// ---------- mock IGD server ----------

/// (internal port, internal client, enabled) per (external port, protocol).
#[derive(Default)]
struct IgdState {
    mappings: BTreeMap<(u16, String), (u16, Ipv4Addr, bool)>,
}

fn soap_ok(response_el: &str, fields: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
         <s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
         <u:{response_el} xmlns:u=\"urn:schemas-upnp-org:service:WANIPConnection:1\">\
         {fields}</u:{response_el}></s:Body></s:Envelope>"
    )
}
fn soap_fault(code: u16) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
         <s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body>\
         <s:Fault><faultcode>s:Client</faultcode><faultstring>UPnPError</faultstring>\
         <detail><UPnPError xmlns=\"urn:schemas-upnp-org:control-1-0\">\
         <errorCode>{code}</errorCode><errorDescription>fixture</errorDescription>\
         </UPnPError></detail></s:Fault></s:Body></s:Envelope>"
    )
}

/// Loose test-side field extractor (fixtures are controlled).
fn field(body: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let start = body.find(&open)? + open.len();
    let end = body[start..].find(&format!("</{name}>"))? + start;
    Some(body[start..end].to_string())
}

fn specific_fields(state: &IgdState, port: u16, proto: &str) -> String {
    let (internal_port, client, enabled) = state.mappings[&(port, proto.to_string())];
    format!(
        "<NewInternalPort>{internal_port}</NewInternalPort>\
         <NewInternalClient>{client}</NewInternalClient>\
         <NewEnabled>{}</NewEnabled>\
         <NewPortMappingDescription>archie-sentry</NewPortMappingDescription>\
         <NewLeaseDuration>0</NewLeaseDuration>",
        if enabled { 1 } else { 0 }
    )
}

fn igd_handle(state: &Arc<Mutex<IgdState>>, request: &[u8]) -> (&'static str, String) {
    let text = String::from_utf8_lossy(request).into_owned();
    let lower = text.to_lowercase();
    let body_start = text.find("\r\n\r\n").map(|i| i + 4).unwrap_or(0);
    let body = &text[body_start..];
    let mut state = state.lock().unwrap();
    if lower.contains("#addportmapping") {
        let port: u16 = field(body, "NewExternalPort").unwrap().parse().unwrap();
        let proto = field(body, "NewProtocol").unwrap();
        let internal: u16 = field(body, "NewInternalPort").unwrap().parse().unwrap();
        let client: Ipv4Addr = field(body, "NewInternalClient").unwrap().parse().unwrap();
        state
            .mappings
            .insert((port, proto.clone()), (internal, client, true));
        ("200 OK", soap_ok("AddPortMappingResponse", ""))
    } else if lower.contains("#deleteportmapping") {
        let port: u16 = field(body, "NewExternalPort").unwrap().parse().unwrap();
        let proto = field(body, "NewProtocol").unwrap();
        state.mappings.remove(&(port, proto));
        ("200 OK", soap_ok("DeletePortMappingResponse", ""))
    } else if lower.contains("#getspecificportmappingentry") {
        let port: u16 = field(body, "NewExternalPort").unwrap().parse().unwrap();
        let proto = field(body, "NewProtocol").unwrap();
        if state.mappings.contains_key(&(port, proto.clone())) {
            let fields = specific_fields(&state, port, &proto);
            (
                "200 OK",
                soap_ok("GetSpecificPortMappingEntryResponse", &fields),
            )
        } else {
            ("500 Internal Server Error", soap_fault(714))
        }
    } else if lower.contains("#getgenericportmappingentry") {
        let index: usize = field(body, "NewPortMappingIndex").unwrap().parse().unwrap();
        if let Some(((port, proto), (internal, client, enabled))) = state.mappings.iter().nth(index)
        {
            let fields = format!(
                "<NewRemoteHost></NewRemoteHost>\
                 <NewExternalPort>{port}</NewExternalPort>\
                 <NewProtocol>{proto}</NewProtocol>\
                 <NewInternalPort>{internal}</NewInternalPort>\
                 <NewInternalClient>{client}</NewInternalClient>\
                 <NewEnabled>{}</NewEnabled>\
                 <NewPortMappingDescription>archie-sentry</NewPortMappingDescription>\
                 <NewLeaseDuration>0</NewLeaseDuration>",
                if *enabled { 1 } else { 0 }
            );
            (
                "200 OK",
                soap_ok("GetGenericPortMappingEntryResponse", &fields),
            )
        } else {
            ("500 Internal Server Error", soap_fault(713))
        }
    } else if lower.contains("#getstatusinfo") {
        (
            "200 OK",
            soap_ok(
                "GetStatusInfoResponse",
                "<NewConnectionStatus>Connected</NewConnectionStatus>\
                 <NewUptime>3600</NewUptime>\
                 <NewExternalIPAddress>100.64.0.2</NewExternalIPAddress>",
            ),
        )
    } else {
        ("500 Internal Server Error", soap_fault(401))
    }
}

async fn read_request(stream: &mut TcpStream) -> Vec<u8> {
    let mut request = vec![];
    let mut chunk = [0; 4096];
    loop {
        let size = stream.read(&mut chunk).await.unwrap();
        if size == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..size]);
        if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
            let text = String::from_utf8_lossy(&request[..end]);
            let length = text
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|s| s.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            if request.len() >= end + 4 + length {
                break;
            }
        }
    }
    request
}

async fn write_response(stream: &mut TcpStream, status: &str, body: &str) {
    let headers = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/xml; charset=\"utf-8\"\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(headers.as_bytes()).await.unwrap();
    stream.write_all(body.as_bytes()).await.unwrap();
}

/// Stateful mock IGD: answers `count` requests, returns every raw request.
async fn igd_server(
    count: usize,
    state: Arc<Mutex<IgdState>>,
) -> (String, tokio::task::JoinHandle<Vec<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let mut requests = vec![];
        for _ in 0..count {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_request(&mut stream).await;
            let (status, body) = igd_handle(&state, &request);
            requests.push(request);
            write_response(&mut stream, status, &body).await;
        }
        requests
    });
    (format!("http://{address}"), task)
}

/// Canned server: every request gets the same status and body, regardless.
async fn canned_server(
    count: usize,
    status: &'static str,
    body: String,
) -> (String, tokio::task::JoinHandle<Vec<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let mut requests = vec![];
        for _ in 0..count {
            let (mut stream, _) = listener.accept().await.unwrap();
            requests.push(read_request(&mut stream).await);
            write_response(&mut stream, status, &body).await;
        }
        requests
    });
    (format!("http://{address}"), task)
}

// ---------- IGD flow through the adapter ----------

#[tokio::test]
async fn add_port_mapping_request_body_is_correct_and_mapping_readback_verifies() {
    let (url, task) = igd_server(2, Arc::new(Mutex::new(IgdState::default()))).await;
    let mut adapter = igd_adapter(&url, true);
    let command = Command::PortMapping {
        present: true,
        external_port: 8080,
        protocol: PortProtocol::Tcp,
    };
    let resource = resource(&[CommandKind::PortMapping]);
    assert!(adapter.apply(&resource, &command).await.unwrap().accepted);
    let observation = adapter.observe(&resource).await.unwrap();
    assert_eq!(
        observation.state,
        ResourceState::Router {
            mapping_present: Some(true),
            wifi_enabled: None,
            device_blocked: None,
        }
    );
    assert_eq!(observation.origin, ObservationOrigin::Measurement);
    assert!(command.matches(&observation.state));
    let requests = task.await.unwrap();
    assert_eq!(requests.len(), 2);
    let add = String::from_utf8_lossy(&requests[0]).into_owned();
    assert!(add.contains("<NewExternalPort>8080</NewExternalPort>"));
    assert!(add.contains("<NewProtocol>TCP</NewProtocol>"));
    assert!(add.contains("<NewInternalPort>8080</NewInternalPort>"));
    assert!(add.contains("<NewInternalClient>192.168.1.10</NewInternalClient>"));
    assert!(add.contains("<NewEnabled>1</NewEnabled>"));
    let lower = add.to_lowercase();
    assert!(
        lower.contains(
            "soapaction: \"urn:schemas-upnp-org:service:wanipconnection:1#addportmapping\""
        ),
        "missing SOAPAction header: {lower}"
    );
    let query = String::from_utf8_lossy(&requests[1]).into_owned();
    assert!(query.contains("GetSpecificPortMappingEntry"));
    assert!(query.contains("<NewExternalPort>8080</NewExternalPort>"));
    assert!(query.contains("<NewProtocol>TCP</NewProtocol>"));
}

#[tokio::test]
async fn delete_port_mapping_readback_reports_absent() {
    let state = Arc::new(Mutex::new(IgdState::default()));
    state
        .lock()
        .unwrap()
        .mappings
        .insert((9090, "UDP".to_string()), (9090, LAN_CLIENT, true));
    let (url, task) = igd_server(2, state).await;
    let mut adapter = igd_adapter(&url, true);
    let command = Command::PortMapping {
        present: false,
        external_port: 9090,
        protocol: PortProtocol::Udp,
    };
    let resource = resource(&[CommandKind::PortMapping]);
    assert!(adapter.apply(&resource, &command).await.unwrap().accepted);
    let observation = adapter.observe(&resource).await.unwrap();
    assert_eq!(
        observation.state,
        ResourceState::Router {
            mapping_present: Some(false),
            wifi_enabled: None,
            device_blocked: None,
        }
    );
    assert!(command.matches(&observation.state));
    let requests = task.await.unwrap();
    assert_eq!(requests.len(), 2);
    let delete = String::from_utf8_lossy(&requests[0]).into_owned();
    assert!(delete.contains("DeletePortMapping"));
    assert!(delete.contains("<NewProtocol>UDP</NewProtocol>"));
}

#[tokio::test]
async fn fresh_observe_reports_unknown_and_sends_nothing() {
    let (url, task) = igd_server(0, Arc::new(Mutex::new(IgdState::default()))).await;
    let mut adapter = igd_adapter(&url, true);
    let observation = adapter
        .observe(&resource(&[CommandKind::PortMapping]))
        .await
        .unwrap();
    assert_eq!(
        observation.state,
        ResourceState::Router {
            mapping_present: None,
            wifi_enabled: None,
            device_blocked: None,
        }
    );
    assert!(
        task.await.unwrap().is_empty(),
        "no watch target, no traffic"
    );
}

#[tokio::test]
async fn soap_fault_maps_to_protocol() {
    let (url, task) = canned_server(1, "200 OK", soap_fault(500)).await;
    let mut adapter = igd_adapter(&url, true);
    assert_eq!(
        adapter
            .apply(
                &resource(&[CommandKind::PortMapping]),
                &Command::PortMapping {
                    present: true,
                    external_port: 8080,
                    protocol: PortProtocol::Tcp,
                },
            )
            .await,
        Err(AdapterError::Protocol)
    );
    assert_eq!(task.await.unwrap().len(), 1);
}

#[tokio::test]
async fn soap_fault_over_http_500_is_still_protocol_not_unavailable() {
    // Real gateways answer UPnP faults with HTTP 500; the fault must parse.
    let (url, task) = canned_server(1, "500 Internal Server Error", soap_fault(500)).await;
    let mut adapter = igd_adapter(&url, true);
    assert_eq!(
        adapter
            .apply(
                &resource(&[CommandKind::PortMapping]),
                &Command::PortMapping {
                    present: false,
                    external_port: 8080,
                    protocol: PortProtocol::Tcp,
                },
            )
            .await,
        Err(AdapterError::Protocol)
    );
    assert_eq!(task.await.unwrap().len(), 1);
}

#[tokio::test]
async fn http_401_maps_to_authentication() {
    let (url, task) = canned_server(1, "401 Unauthorized", String::new()).await;
    let mut adapter = igd_adapter(&url, true);
    assert_eq!(
        adapter
            .apply(
                &resource(&[CommandKind::PortMapping]),
                &Command::PortMapping {
                    present: true,
                    external_port: 8080,
                    protocol: PortProtocol::Tcp,
                },
            )
            .await,
        Err(AdapterError::Authentication)
    );
    assert_eq!(task.await.unwrap().len(), 1);
}

#[tokio::test]
async fn oversized_response_is_bounded() {
    // A declared body beyond the shared 1 MiB cap is refused before reading.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let _ = read_request(&mut stream).await;
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/xml\r\n\
                  Content-Length: 1048577\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
    });
    let mut adapter = igd_adapter(&format!("http://{address}"), true);
    assert_eq!(
        adapter
            .apply(
                &resource(&[CommandKind::PortMapping]),
                &Command::PortMapping {
                    present: true,
                    external_port: 8080,
                    protocol: PortProtocol::Tcp,
                },
            )
            .await,
        Err(AdapterError::Protocol)
    );
    task.await.unwrap();
}

#[tokio::test]
async fn unprobed_igd_is_unsupported_without_traffic() {
    let (url, task) = igd_server(0, Arc::new(Mutex::new(IgdState::default()))).await;
    let mut adapter = igd_adapter(&url, false); // probe said: no IGD
    assert_eq!(
        adapter
            .apply(
                &resource(&[CommandKind::PortMapping]),
                &Command::PortMapping {
                    present: true,
                    external_port: 8080,
                    protocol: PortProtocol::Tcp,
                },
            )
            .await,
        Err(AdapterError::Unsupported)
    );
    assert!(task.await.unwrap().is_empty());
}

#[tokio::test]
async fn non_router_commands_are_unsupported_without_traffic() {
    let (url, task) = igd_server(0, Arc::new(Mutex::new(IgdState::default()))).await;
    let mut adapter = igd_adapter(&url, true);
    for command in [
        Command::MediaPause,
        Command::SwitchSet { on: true },
        Command::WakeHost,
    ] {
        assert_eq!(
            adapter
                .apply(&resource(&[CommandKind::PortMapping]), &command)
                .await,
            Err(AdapterError::Unsupported)
        );
    }
    assert!(task.await.unwrap().is_empty());
}

#[tokio::test]
async fn unpaired_or_foreign_resource_is_rejected() {
    let (url, task) = igd_server(0, Arc::new(Mutex::new(IgdState::default()))).await;
    let mut adapter = igd_adapter(&url, true);
    let mut foreign = resource(&[CommandKind::PortMapping]);
    foreign.paired = false;
    assert_eq!(
        adapter.observe(&foreign).await,
        Err(AdapterError::Unsupported)
    );
    let mut other = resource(&[CommandKind::PortMapping]);
    other.resource_id = "other-router".into();
    assert_eq!(
        adapter.observe(&other).await,
        Err(AdapterError::Unsupported)
    );
    assert!(task.await.unwrap().is_empty());
}

#[tokio::test]
async fn specific_entry_fields_parse_and_714_means_absent() {
    let state = Arc::new(Mutex::new(IgdState::default()));
    state
        .lock()
        .unwrap()
        .mappings
        .insert((8080, "TCP".to_string()), (8080, LAN_CLIENT, true));
    let (url, task) = igd_server(2, state).await;
    let igd = IgdClient::new(&format!("{url}/upnp/control/WANIPConnection1")).unwrap();
    let entry = igd
        .get_specific_port_mapping_entry(8080, PortProtocol::Tcp)
        .await
        .unwrap();
    assert_eq!(
        entry,
        Some(SpecificEntry {
            internal_port: 8080,
            internal_client: LAN_CLIENT,
            enabled: true,
            description: Some("archie-sentry".into()),
        })
    );
    assert_eq!(
        igd.get_specific_port_mapping_entry(8081, PortProtocol::Tcp)
            .await,
        Ok(None)
    );
    assert_eq!(task.await.unwrap().len(), 2);
}

#[tokio::test]
async fn generic_enumeration_is_bounded_and_ends_on_713() {
    let state = Arc::new(Mutex::new(IgdState::default()));
    {
        let mut state = state.lock().unwrap();
        state
            .mappings
            .insert((8080, "TCP".to_string()), (8080, LAN_CLIENT, true));
        state
            .mappings
            .insert((9090, "UDP".to_string()), (9090, LAN_CLIENT, true));
    }
    let (url, task) = igd_server(3, state).await;
    let igd = IgdClient::new(&format!("{url}/upnp/control/WANIPConnection1")).unwrap();
    let entries = igd.list_port_mappings().await.unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].external_port, 8080);
    assert_eq!(entries[0].protocol, PortProtocol::Tcp);
    assert_eq!(entries[1].external_port, 9090);
    assert_eq!(entries[1].protocol, PortProtocol::Udp);
    assert!(entries
        .iter()
        .all(|e| e.enabled && e.internal_client == LAN_CLIENT));
    // Two entries plus the terminating 713: exactly three requests.
    assert_eq!(task.await.unwrap().len(), 3);
}

// ---------- SSH channel ----------

fn ssh_adapter(runner: Arc<FakeRunner>, capabilities: RouterCapabilities) -> RouterAdapter {
    RouterAdapter::new(
        "edge",
        "router",
        "router",
        RouterConfig {
            control_url: None,
            lan_client: LAN_CLIENT,
            ssh: Some(ssh_config()),
            protected_macs: vec![],
        },
        capabilities,
        runner,
    )
    .unwrap()
}

#[tokio::test]
async fn wifi_on_runs_exact_wl_argv_and_radio_state_verifies() {
    let runner = FakeRunner::new(vec![
        (&["wl", "-i", "wl0", "radio", "on"], 0, ""),
        (&["wl", "-i", "wl0", "radio"], 0, "0x0001\n"),
    ]);
    let mut adapter = ssh_adapter(runner.clone(), caps(false, true, &["wl"]));
    let command = Command::WifiSet { on: true };
    let resource = resource(&[CommandKind::WifiSet]);
    assert!(adapter.apply(&resource, &command).await.unwrap().accepted);
    let observation = adapter.observe(&resource).await.unwrap();
    assert_eq!(
        observation.state,
        ResourceState::Router {
            mapping_present: None,
            wifi_enabled: Some(true),
            device_blocked: None,
        }
    );
    assert!(command.matches(&observation.state));
    let calls = runner.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[0],
        vec![
            ssh_path(),
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=5",
            "-o",
            "StrictHostKeyChecking=accept-new",
            "-p",
            "22",
            "-i",
            key_path(),
            "admin@192.168.1.1",
            "wl",
            "-i",
            "wl0",
            "radio",
            "on",
        ]
    );
    assert!(calls[1].ends_with(&["wl".to_string(), "-i".into(), "wl0".into(), "radio".into()]));
}

#[tokio::test]
async fn wifi_off_runs_exact_wl_argv_and_reports_disabled() {
    let runner = FakeRunner::new(vec![
        (&["wl", "-i", "wl0", "radio", "off"], 0, ""),
        (&["wl", "-i", "wl0", "radio"], 0, "0x0000\n"),
    ]);
    let mut adapter = ssh_adapter(runner.clone(), caps(false, true, &["wl"]));
    let command = Command::WifiSet { on: false };
    let resource = resource(&[CommandKind::WifiSet]);
    assert!(adapter.apply(&resource, &command).await.unwrap().accepted);
    let observation = adapter.observe(&resource).await.unwrap();
    assert_eq!(
        observation.state,
        ResourceState::Router {
            mapping_present: None,
            wifi_enabled: Some(false),
            device_blocked: None,
        }
    );
    assert!(command.matches(&observation.state));
    let calls = runner.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].last().unwrap(), "off");
}

#[tokio::test]
async fn wifi_without_probed_radio_tool_is_unsupported_and_spawns_nothing() {
    let runner = FakeRunner::new(vec![]);
    let mut adapter = ssh_adapter(runner.clone(), caps(false, true, &[]));
    assert_eq!(
        adapter
            .apply(
                &resource(&[CommandKind::WifiSet]),
                &Command::WifiSet { on: true }
            )
            .await,
        Err(AdapterError::Unsupported)
    );
    let observation = adapter
        .observe(&resource(&[CommandKind::WifiSet]))
        .await
        .unwrap();
    assert_eq!(
        observation.state,
        ResourceState::Router {
            mapping_present: None,
            wifi_enabled: None,
            device_blocked: None,
        }
    );
    assert!(runner.calls().is_empty(), "no radio tool, no process");
}

#[tokio::test]
async fn nonzero_ssh_exit_is_unavailable() {
    let runner = FakeRunner::new(vec![]); // everything exits 255
    let channel = SshChannel::new(ssh_config(), runner);
    assert_eq!(
        channel.run_named("ip_neigh"),
        Err(AdapterError::Unavailable)
    );
}

#[tokio::test]
async fn command_outside_the_table_is_unsupported_without_spawning() {
    let runner = FakeRunner::new(vec![]);
    let channel = SshChannel::new(ssh_config(), runner.clone());
    for name in ["factory_reset", "reboot", "rm_rf", ""] {
        assert_eq!(channel.run_named(name), Err(AdapterError::Unsupported));
    }
    assert!(
        runner.calls().is_empty(),
        "nothing outside the table may spawn"
    );
}

// ---------- client list parsers ----------

#[test]
fn dnsmasq_leases_parse_and_malformed_lines_are_skipped() {
    let text = "\
        1730000000 AA:BB:CC:11:22:33 192.168.1.20 laptop 01:aa:bb:cc:11:22:33\n\
        1730000001 aa:bb:cc:44:55:66 192.168.1.21 * *\n\
        0 aa:bb:cc:77:88:99 192.168.1.22 static-host 00\n\
        garbage line without enough fields\n\
        1730000002 not-a-mac 192.168.1.23 bad 00\n\
        1730000003 aa:bb:cc:00:11:22 not-an-ip bad 00\n\
        1730000004 aa:bb:cc:00:11:23 8.8.8.8 public-ip 00\n\
        notanumber aa:bb:cc:00:11:24 192.168.1.24 bad 00\n";
    let clients = parse_dnsmasq_leases(text);
    assert_eq!(clients.len(), 3);
    assert_eq!(clients[0].mac, "aa:bb:cc:11:22:33");
    assert_eq!(clients[0].ip, Ipv4Addr::new(192, 168, 1, 20));
    assert_eq!(clients[0].hostname.as_deref(), Some("laptop"));
    assert_eq!(clients[1].hostname, None, "`*` is no hostname");
    assert_eq!(clients[2].hostname.as_deref(), Some("static-host"));
    // bad mac, bad ip, public ip and bad expiry were all skipped
    assert!(!clients.iter().any(|c| c.ip == Ipv4Addr::new(8, 8, 8, 8)));
}

#[test]
fn ip_neigh_parse_skips_failed_and_lladdr_less_lines() {
    let text = "\
        192.168.1.20 dev br0 lladdr aa:bb:cc:11:22:33 REACHABLE\n\
        192.168.1.21 dev br0 lladdr AA:BB:CC:44:55:66 STALE\n\
        192.168.1.22 dev br0 FAILED\n\
        192.168.1.23 dev br0 lladdr aa:bb:cc:77:88:99 FAILED\n\
        192.168.1.24 dev br0 router\n\
        garbage\n\
        8.8.8.8 dev br0 lladdr aa:bb:cc:00:11:22 REACHABLE\n";
    let clients = parse_ip_neigh(text);
    assert_eq!(clients.len(), 2);
    assert_eq!(clients[0].mac, "aa:bb:cc:11:22:33");
    assert_eq!(clients[1].mac, "aa:bb:cc:44:55:66");
    assert!(clients.iter().all(|c| c.hostname.is_none()));
}

#[test]
fn router_clients_prefers_the_probed_leases_path() {
    let runner = FakeRunner::new(vec![
        (
            &["cat", "/tmp/dhcp.leases"],
            0,
            "1730000000 aa:bb:cc:11:22:33 192.168.1.20 laptop 01:x\n",
        ),
        (&["ip", "neigh"], 0, ""),
    ]);
    let channel = SshChannel::new(ssh_config(), runner.clone());
    let mut capabilities = caps(false, true, &[]);
    capabilities.dhcp_leases_path = Some("/tmp/dhcp.leases".into());
    let clients = router_clients(&channel, &capabilities).unwrap();
    assert_eq!(clients.len(), 1);
    assert_eq!(clients[0].hostname.as_deref(), Some("laptop"));
    let calls = runner.calls();
    assert_eq!(calls.len(), 1);
    assert!(calls[0].ends_with(&["cat".to_string(), "/tmp/dhcp.leases".into()]));
}

#[test]
fn router_clients_falls_back_to_ip_neigh_without_a_leases_path() {
    let runner = FakeRunner::new(vec![(
        &["ip", "neigh"],
        0,
        "192.168.1.20 dev br0 lladdr aa:bb:cc:11:22:33 REACHABLE\n",
    )]);
    let channel = SshChannel::new(ssh_config(), runner.clone());
    let clients = router_clients(&channel, &caps(false, true, &[])).unwrap();
    assert_eq!(clients.len(), 1);
    assert_eq!(clients[0].ip, Ipv4Addr::new(192, 168, 1, 20));
    let calls = runner.calls();
    assert_eq!(calls.len(), 1);
    assert!(calls[0].ends_with(&["ip".to_string(), "neigh".into()]));
}

// ---------- capability probe ----------

#[tokio::test]
async fn probe_igd_only() {
    let (url, task) = igd_server(1, Arc::new(Mutex::new(IgdState::default()))).await;
    let igd = IgdClient::new(&format!("{url}/upnp/control/WANIPConnection1")).unwrap();
    let capabilities = probe_router(Some(&igd), None).await;
    assert_eq!(
        capabilities,
        RouterCapabilities {
            igd: true,
            ssh: false,
            radio_tools: vec![],
            dhcp_leases_path: None,
            firmware_hint: None,
            firewall_tools: vec![],
        }
    );
    assert_eq!(task.await.unwrap().len(), 1);
}

#[tokio::test]
async fn probe_ssh_only() {
    let runner = FakeRunner::new(vec![
        (
            &["ip", "link"],
            0,
            "1: lo: <LOOPBACK>\n2: br0: <BROADCAST>\n",
        ),
        (
            &["cat", "/tmp/dhcp.leases"],
            0,
            "1730000000 aa:bb:cc:11:22:33 192.168.1.20 laptop 01:x\n",
        ),
        (&["wl", "-i", "wl0", "radio"], 0, "0x0001\n"),
        (&["nvram", "get", "productid"], 0, "GPT-2741GNAC\n"),
        // nvram get wl0_radio deliberately unscripted: exit 255, not found.
    ]);
    let channel = SshChannel::new(ssh_config(), runner.clone());
    let capabilities = probe_router(None, Some(&channel)).await;
    assert_eq!(
        capabilities,
        RouterCapabilities {
            igd: false,
            ssh: true,
            radio_tools: vec!["wl".to_string()],
            dhcp_leases_path: Some("/tmp/dhcp.leases".into()),
            firmware_hint: Some("GPT-2741GNAC".into()),
            firewall_tools: vec![],
        }
    );
    // Probing is read-only: no write argv ever ran.
    for call in runner.calls() {
        let remote = call.last().unwrap();
        assert_ne!(remote, "on");
        assert_ne!(remote, "off");
        assert_ne!(remote, "set");
    }
}

#[tokio::test]
async fn probe_both_channels() {
    let (url, task) = igd_server(1, Arc::new(Mutex::new(IgdState::default()))).await;
    let igd = IgdClient::new(&format!("{url}/upnp/control/WANIPConnection1")).unwrap();
    let runner = FakeRunner::new(vec![
        (&["ip", "link"], 0, "1: lo: <LOOPBACK>\n"),
        (&["wl", "-i", "wl0", "radio"], 0, "0x0000\n"),
    ]);
    let channel = SshChannel::new(ssh_config(), runner);
    let capabilities = probe_router(Some(&igd), Some(&channel)).await;
    assert!(capabilities.igd && capabilities.ssh);
    assert_eq!(capabilities.radio_tools, vec!["wl".to_string()]);
    assert_eq!(
        capabilities.dhcp_leases_path, None,
        "no leases file answered"
    );
    assert_eq!(capabilities.firmware_hint, None);
    assert_eq!(task.await.unwrap().len(), 1);
}

#[tokio::test]
async fn probe_neither_channel_reports_nothing() {
    let runner = FakeRunner::new(vec![]); // every command exits 255
    let channel = SshChannel::new(ssh_config(), runner);
    let capabilities = probe_router(None, Some(&channel)).await;
    assert_eq!(capabilities, RouterCapabilities::default());
}

/// A failing command whose stderr is not ASCII: Spanish firmware, or bytes that are
/// not UTF-8 and come back as U+FFFD (three bytes each).
struct GarbledRunner;
impl CommandRunner for GarbledRunner {
    fn run(&self, _argv: &[String], _timeout: Duration) -> Result<CommandOutput, AdapterError> {
        // 100 three-byte characters: byte 200 falls inside the 67th.
        let mut stderr = "€".repeat(100).into_bytes();
        stderr.extend(std::iter::repeat_n(0xff, 100));
        Ok(CommandOutput {
            status: 1,
            stdout: vec![],
            stderr,
        })
    }
}

#[tokio::test]
async fn a_non_ascii_error_from_the_router_is_unavailable_not_a_crash() {
    // Logging cut it at byte 200, which can fall inside a character; under
    // panic = "abort" that ended the whole process.
    let channel = SshChannel::new(ssh_config(), Arc::new(GarbledRunner));
    assert_eq!(
        channel.run_named("ip_neigh"),
        Err(AdapterError::Unavailable)
    );
}
