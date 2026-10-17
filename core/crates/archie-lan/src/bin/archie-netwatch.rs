//! `archie-netwatch` — Sentry's continuous audit of the traffic on the owner's network.
//!
//! ```text
//! archie-netwatch interfaces
//! archie-netwatch live --interface eth0 --state-dir DIR [--learn 600] [--gateway 192.168.1.1] [--filter BPF] [--seconds 30]
//! archie-netwatch read FILE.pcapng --state-dir DIR [--gateway 192.168.1.1]
//! archie-netwatch report --state-dir DIR [--alerts 50]
//! archie-netwatch leases DHCP.LEASES --state-dir DIR [--learn 600]
//! archie-netwatch router detect [--gateway 192.168.1.1]
//! archie-netwatch router enroll --state-dir DIR [--gateway IP] [--kind openwrt|asuswrt|bsd-firewall|linux] [--user root] [--port 22] [--install]
//! archie-netwatch router pull --state-dir DIR
//! ```
//!
//! `live` runs until stopped, or until its optional capture duration expires: TShark
//! captures, each frame is audited, alerts are appended
//! to `DIR/alerts.jsonl` as they are raised, what is known of each device is kept in
//! `DIR/state.json`, and which alerts were already raised in `DIR/told.json`, so a
//! restart does not repeat them. `read` audits a capture file the same way. `leases`
//! adds what the router's DHCP server knows, from a dnsmasq lease file the owner copied
//! (see `archie_lan::leases`). `router` enrols the owner's router once and reads its
//! leases and neighbour table over SSH with a read-only key (see `archie_lan::router`);
//! once enrolled, `live` reads it every five minutes by itself. `report` prints both. Everything is written for its owner only: which names a device asks for
//! is private.

use archie_lan::audit::{Alert, Audit, Settings, State, Told};
use archie_lan::capture::{field_arguments, parse, SNAPLEN};
use archie_lan::router;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

const MAX_ALERTS_BYTES: u64 = 4 * 1024 * 1024;
const SAVE_EVERY: Duration = Duration::from_secs(30);
/// How often `live` reads an enrolled router.
const ROUTER_EVERY: Duration = Duration::from_secs(300);

fn take(arguments: &mut Vec<String>, flag: &str) -> Option<String> {
    let at = arguments.iter().position(|a| a == flag)?;
    arguments
        .get(at + 1)
        .is_some_and(|value| !value.starts_with("--"))
        .then(|| {
            arguments.remove(at);
            arguments.remove(at)
        })
}

/// Let TShark stop the capture even when stdout receives no packets. Its normal
/// EOF path then saves the audit; Core still owns process/startup timeouts.
fn live_source(arguments: &mut Vec<String>) -> Result<Vec<String>, String> {
    let interface = take(arguments, "--interface").ok_or_else(|| {
        "live needs --interface NAME; `archie-netwatch interfaces` lists them".to_string()
    })?;
    let mut source = vec!["-i".to_string(), interface, "-s".into(), SNAPLEN.into()];
    if let Some(filter) = take(arguments, "--filter") {
        source.extend(["-f".to_string(), filter]);
    }
    if let Some(seconds) = take(arguments, "--seconds") {
        let seconds = seconds
            .parse::<u64>()
            .ok()
            .filter(|value| (1..=86_400).contains(value))
            .ok_or_else(|| "--seconds must be an integer from 1 to 86400".to_string())?;
        source.extend(["-a".to_string(), format!("duration:{seconds}")]);
    }
    // Never silently turn a misspelled/missing limit into a continuous capture.
    if arguments.len() != 1 {
        return Err(format!(
            "unknown or incomplete live options: {}",
            arguments[1..].join(" ")
        ));
    }
    Ok(source)
}

/// Opens a file for the audit's own account and its group, never for others. Under the
/// unit the group is `archie`, so the alert follower (the control plane) can read the
/// alerts; the frames' DNS questions in state.json reach nobody outside it.
fn private(path: &Path, append: bool) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o640);
    options.open(path)
}

/// Writes a file in one move: a partial write is never mistaken for the whole.
fn save(directory: &Path, name: &str, value: &impl serde::Serialize) -> std::io::Result<()> {
    let partial = directory.join(format!("{name}.partial"));
    private(&partial, false)?.write_all(&serde_json::to_vec(value).expect("it serializes"))?;
    std::fs::rename(partial, directory.join(name))
}

fn load(directory: &Path) -> Result<State, String> {
    match std::fs::read(directory.join("state.json")) {
        Ok(bytes) => State::parse(&bytes)
            .map_err(|e| format!("state.json is not a state this program wrote: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(State::default()),
        Err(e) => Err(e.to_string()),
    }
}

/// What was already told. A told.json that will not load is set aside rather than
/// fatal: the worst that follows is an alert told once more, and that is said.
fn load_told(directory: &Path) -> Told {
    match std::fs::read(directory.join("told.json")) {
        Ok(bytes) => match Told::parse(&bytes) {
            Ok(told) => told,
            Err(error) => {
                eprintln!("archie-netwatch: told.json is set aside ({error}); some alerts may be told once more");
                Told::default()
            }
        },
        Err(_) => Told::default(),
    }
}

fn append(directory: &Path, alerts: &[Alert]) -> std::io::Result<()> {
    let path = directory.join("alerts.jsonl");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_ALERTS_BYTES) {
        std::fs::rename(&path, directory.join("alerts.1.jsonl"))?;
    }
    let mut file = private(&path, true)?;
    for alert in alerts {
        writeln!(
            file,
            "{}",
            serde_json::to_string(alert).expect("an alert serializes")
        )?;
    }
    Ok(())
}

/// The router, from the kernel's own routing table.
fn default_gateway() -> Option<Ipv4Addr> {
    let table = std::fs::read_to_string("/proc/net/route").ok()?;
    table.lines().skip(1).find_map(|line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        (fields.len() > 2 && fields[1] == "00000000").then(|| {
            u32::from_str_radix(fields[2], 16)
                .ok()
                .map(|raw| Ipv4Addr::from(raw.swap_bytes()))
        })?
    })
}

/// The router's IPv6 next hop for the default route, from the kernel's own table.
fn default_gateway_v6() -> Option<Ipv6Addr> {
    let table = std::fs::read_to_string("/proc/net/ipv6_route").ok()?;
    table.lines().find_map(parse_ipv6_default_route)
}

/// One `/proc/net/ipv6_route` line: destination, its prefix length, source, its prefix
/// length, next hop, all in hex. The default route is `::/0` with a next hop that is set.
fn parse_ipv6_default_route(line: &str) -> Option<Ipv6Addr> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    let hex = |text: &str| u128::from_str_radix(text, 16).ok().map(Ipv6Addr::from);
    if fields.len() < 5 || fields[1] != "00" || hex(fields[0])? != Ipv6Addr::UNSPECIFIED {
        return None;
    }
    hex(fields[4]).filter(|hop| !hop.is_unspecified())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Writes alerts as they are raised, with what was told, and says them.
fn record(directory: &Path, audit: &Audit, alerts: &[Alert]) -> Result<(), String> {
    if alerts.is_empty() {
        return Ok(());
    }
    append(directory, alerts).map_err(|e| e.to_string())?;
    // What was told is written with the alerts themselves, so even a crash between saves
    // does not repeat them.
    save(directory, "told.json", &audit.told).map_err(|e| e.to_string())?;
    for alert in alerts {
        eprintln!(
            "archie-netwatch: {:?} {} {}: {}",
            alert.severity, alert.rule, alert.device, alert.detail
        );
    }
    Ok(())
}

/// Feeds one read of the router to the audit.
fn apply_router(audit: &mut Audit, tables: &router::Tables, at_ms: u64) -> Vec<Alert> {
    let mut alerts = Vec::new();
    for lease in &tables.leases {
        alerts.extend(audit.leased(lease, at_ms));
    }
    for (mac, address) in &tables.neighbours {
        alerts.extend(audit.router_saw(mac, *address, at_ms));
    }
    // Connections are by address; the same read's neighbour and lease tables say whose.
    let owners: std::collections::HashMap<std::net::Ipv4Addr, &str> = tables
        .leases
        .iter()
        .map(|lease| (lease.address, lease.mac.as_str()))
        .chain(tables.neighbours.iter().map(|(mac, a)| (*a, mac.as_str())))
        .collect();
    for (address, outbound) in &tables.flows {
        if let Some(mac) = owners.get(address) {
            audit.router_flows(mac, outbound, at_ms);
        }
    }
    alerts
}

/// The enrolled router, if there is one in the state directory.
fn enrolled(directory: &Path) -> Option<router::Config> {
    let bytes = std::fs::read(directory.join(router::CONFIG)).ok()?;
    match serde_json::from_slice(&bytes) {
        Ok(config) => Some(config),
        Err(error) => {
            eprintln!("archie-netwatch: {} is set aside ({error})", router::CONFIG);
            None
        }
    }
}

fn audit(
    source: &[String],
    directory: &Path,
    settings: Settings,
    router: Option<router::Config>,
) -> Result<(), String> {
    std::fs::create_dir_all(directory).map_err(|e| format!("{}: {e}", directory.display()))?;
    let mut audit = Audit::new(load(directory)?, settings);
    audit.told = load_told(directory);
    // The router is read on its own thread, so a slow SSH never holds up the capture.
    let (sender, received) = std::sync::mpsc::channel();
    let mut router_asked: Option<Instant> = None;
    let mut router_reading = false;
    let mut tshark = Command::new("tshark")
        .args(source)
        .args(field_arguments())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| {
            format!("tshark could not be started ({e}); it comes with the wireshark-cli package")
        })?;
    let lines = BufReader::new(tshark.stdout.take().expect("piped"));
    let (mut frames, mut raised, mut saved) = (0u64, 0u64, Instant::now());
    for line in lines.lines() {
        let line = line.map_err(|e| e.to_string())?;
        let Some(packet) = parse(&line) else { continue };
        frames += 1;
        let alerts = audit.see(&packet);
        raised += alerts.len() as u64;
        record(directory, &audit, &alerts)?;
        if let Some(config) = &router {
            let due = router_asked.is_none_or(|asked| asked.elapsed() >= ROUTER_EVERY);
            if due && !router_reading {
                let (config, folder, sender) =
                    (config.clone(), directory.to_path_buf(), sender.clone());
                std::thread::spawn(move || {
                    let at_ms = now_ms();
                    let _ = sender.send((at_ms, router::pull(&config, &folder, at_ms / 1000)));
                });
                router_asked = Some(Instant::now());
                router_reading = true;
            }
        }
        while let Ok((at_ms, result)) = received.try_recv() {
            router_reading = false;
            match result {
                Ok(tables) => {
                    let alerts = apply_router(&mut audit, &tables, at_ms);
                    raised += alerts.len() as u64;
                    record(directory, &audit, &alerts)?;
                }
                Err(error) => eprintln!("archie-netwatch: router: {error}"),
            }
        }
        if saved.elapsed() >= SAVE_EVERY {
            save(directory, "state.json", &audit.state).map_err(|e| e.to_string())?;
            save(directory, "told.json", &audit.told).map_err(|e| e.to_string())?;
            saved = Instant::now();
        }
    }
    save(directory, "state.json", &audit.state).map_err(|e| e.to_string())?;
    save(directory, "told.json", &audit.told).map_err(|e| e.to_string())?;
    let status = tshark.wait().map_err(|e| e.to_string())?;
    eprintln!(
        "archie-netwatch: {frames} frames, {raised} alerts, {} devices known",
        audit.state.devices.len()
    );
    if status.success() {
        Ok(())
    } else {
        Err(format!("tshark ended with {status}"))
    }
}

/// The most of a lease file that is read: a home router's table is a few kilobytes.
const MAX_LEASES_BYTES: u64 = 1024 * 1024;

fn leases(file: &Path, directory: &Path, settings: Settings) -> Result<(), String> {
    let size = std::fs::metadata(file)
        .map_err(|e| format!("{}: {e}", file.display()))?
        .len();
    if size > MAX_LEASES_BYTES {
        return Err(format!(
            "{}: {size} bytes is not a lease file (at most {MAX_LEASES_BYTES})",
            file.display()
        ));
    }
    let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    std::fs::create_dir_all(directory).map_err(|e| format!("{}: {e}", directory.display()))?;
    let mut audit = Audit::new(load(directory)?, settings);
    audit.told = load_told(directory);
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    let leases = archie_lan::leases::parse(&text, now_ms / 1000);
    let alerts: Vec<Alert> = leases
        .iter()
        .flat_map(|lease| audit.leased(lease, now_ms))
        .collect();
    if !alerts.is_empty() {
        append(directory, &alerts).map_err(|e| e.to_string())?;
        for alert in &alerts {
            eprintln!(
                "archie-netwatch: {:?} {} {}: {}",
                alert.severity, alert.rule, alert.device, alert.detail
            );
        }
    }
    save(directory, "state.json", &audit.state).map_err(|e| e.to_string())?;
    save(directory, "told.json", &audit.told).map_err(|e| e.to_string())?;
    eprintln!(
        "archie-netwatch: {} leases, {} alerts, {} devices known",
        leases.len(),
        alerts.len(),
        audit.state.devices.len()
    );
    Ok(())
}

fn router_command(
    arguments: &mut Vec<String>,
    directory: Option<PathBuf>,
    gateway: Option<IpAddr>,
    learn: Option<u64>,
) -> Result<(), String> {
    let install = arguments
        .iter()
        .position(|a| a == "--install")
        .map(|at| arguments.remove(at))
        .is_some();
    let kind = take(arguments, "--kind");
    let user = take(arguments, "--user");
    let port = take(arguments, "--port");
    let action = arguments.get(1).cloned();
    if arguments.len() != 2 {
        return Err(format!(
            "unknown router options: {}",
            arguments[1..].join(" ")
        ));
    }
    match (action.as_deref(), directory) {
        (Some("detect"), _) => {
            let address = gateway
                .or_else(|| default_gateway().map(IpAddr::V4))
                .ok_or("no default gateway; name the router with --gateway")?;
            let detected = router::detect(address);
            println!(
                "{}",
                serde_json::to_string_pretty(&detected).expect("it serializes")
            );
            Ok(())
        }
        (Some("enroll"), Some(directory)) => {
            let previous = enrolled(&directory);
            // A retry keeps its target even when the host's default route changes.
            let address = gateway
                .or_else(|| previous.as_ref().map(|config| config.address))
                .or_else(|| default_gateway().map(IpAddr::V4))
                .ok_or("no default gateway; name the router with --gateway")?;
            let kind = match kind {
                Some(kind) => router::Kind::parse(&kind)
                    .ok_or("--kind is one of openwrt, asuswrt, bsd-firewall, linux")?,
                None => previous.as_ref().map(|config| config.kind)
                    .or_else(|| router::detect(address).kind).ok_or(
                    "the router did not say what it runs; name it with --kind openwrt|asuswrt|bsd-firewall|linux",
                )?,
            };
            let user = user
                .or_else(|| previous.as_ref().map(|config| config.user.clone()))
                .unwrap_or_else(|| {
                    match kind {
                        router::Kind::Asuswrt | router::Kind::BsdFirewall => "admin",
                        _ => "root",
                    }
                    .to_string()
                });
            let port = match port {
                Some(port) => port.parse().map_err(|_| "--port is a port number")?,
                None => previous.as_ref().map_or(22, |config| config.port),
            };
            std::fs::create_dir_all(&directory)
                .map_err(|e| format!("{}: {e}", directory.display()))?;
            let public = router::ensure_key(&directory)?;
            let config = router::Config {
                address,
                port,
                user,
                kind,
            };
            let line = router::authorized_line(kind, &public);
            println!("{line}");
            if install {
                if !kind.installs_over_ssh() {
                    return Err(format!(
                        "this router keeps its keys in its own settings; {}",
                        kind.where_the_key_goes()
                    ));
                }
                eprintln!(
                    "archie-netwatch: adding Sentry's read-only key on {}@{address}; ssh asks for the router's password once",
                    config.user
                );
                router::install(&config, &directory, &line)?;
                let tables = router::pull(&config, &directory, now_ms() / 1000)?;
                save(&directory, router::CONFIG, &config).map_err(|e| e.to_string())?;
                eprintln!(
                    "archie-netwatch: router enrolled: {} leases and {} neighbours read; `live` reads it every {} minutes",
                    tables.leases.len(),
                    tables.neighbours.len(),
                    ROUTER_EVERY.as_secs() / 60
                );
            } else {
                save(&directory, router::CONFIG, &config).map_err(|e| e.to_string())?;
                eprintln!(
                    "archie-netwatch: enrollment saved; SSH access is unverified. Add the line above on the router. {}. It lets Sentry read the lease and neighbour tables and nothing else. Then `archie-netwatch router pull --state-dir {}` checks it.",
                    kind.where_the_key_goes(),
                    directory.display()
                );
            }
            Ok(())
        }
        (Some("pull"), Some(directory)) => {
            let config = enrolled(&directory)
                .ok_or("no router enrolled here; run `archie-netwatch router enroll` first")?;
            let mut audit = Audit::new(
                load(&directory)?,
                Settings {
                    learn_ms: learn.unwrap_or(600) * 1000,
                    gateway: match config.address {
                        IpAddr::V4(v4) => Some(v4),
                        IpAddr::V6(_) => None,
                    },
                    gateway_v6: None,
                },
            );
            audit.told = load_told(&directory);
            let at_ms = now_ms();
            let tables = router::pull(&config, &directory, at_ms / 1000)?;
            let alerts = apply_router(&mut audit, &tables, at_ms);
            record(&directory, &audit, &alerts)?;
            save(&directory, "state.json", &audit.state).map_err(|e| e.to_string())?;
            save(&directory, "told.json", &audit.told).map_err(|e| e.to_string())?;
            eprintln!(
                "archie-netwatch: router: {} leases, {} neighbours, {} alerts, {} devices known",
                tables.leases.len(),
                tables.neighbours.len(),
                alerts.len(),
                audit.state.devices.len()
            );
            Ok(())
        }
        _ => Err("router detect | enroll --state-dir DIR | pull --state-dir DIR".into()),
    }
}

fn report(directory: &Path, last: usize) -> Result<(), String> {
    let state = load(directory)?;
    let text = std::fs::read_to_string(directory.join("alerts.jsonl")).unwrap_or_default();
    let alerts: Vec<serde_json::Value> = text
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    let recent = &alerts[alerts.len().saturating_sub(last)..];
    // What each device's own traffic says it is: maker, random address, hints with the
    // evidence they rest on.
    let profiles: Vec<_> = state
        .devices
        .iter()
        .map(|(mac, device)| archie_lan::fingerprint::profile(mac, device))
        .collect();
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({ "state": state, "profiles": profiles, "alerts": recent })
        )
        .expect("a report serializes")
    );
    Ok(())
}

/// Finite, trusted-local metadata windows from a host bridge. No payload field is
/// accepted outside the catalogue returned by `capture-fields`.
fn ingest_fields(directory: &Path, settings: Settings) -> Result<(), String> {
    const MAX_ROW: usize = 64 * 1024;
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let mut audit = Audit::new(load(directory)?, settings);
    audit.told = load_told(directory);
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let (mut frames, mut alerts_count) = (0u64, 0u64);
    let mut last_save = Instant::now();
    loop {
        let mut bytes = Vec::new();
        let size = (&mut input)
            .take((MAX_ROW + 1) as u64)
            .read_until(b'\n', &mut bytes)
            .map_err(|e| e.to_string())?;
        if size == 0 {
            break;
        }
        if size > MAX_ROW {
            return Err("metadata row exceeds bounded size".into());
        }
        let line = std::str::from_utf8(&bytes)
            .map_err(|_| "metadata row is not UTF-8")?
            .trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            continue;
        }
        if line.split('\t').count() != archie_lan::capture::FIELDS.len() {
            return Err("metadata row does not match capture-fields catalogue".into());
        }
        let packet = parse(line).ok_or("metadata row is not a valid packet")?;
        frames += 1;
        let alerts = audit.see(&packet);
        alerts_count += alerts.len() as u64;
        record(directory, &audit, &alerts)?;
        if last_save.elapsed() >= SAVE_EVERY {
            save(directory, "state.json", &audit.state).map_err(|e| e.to_string())?;
            save(directory, "told.json", &audit.told).map_err(|e| e.to_string())?;
            last_save = Instant::now();
        }
    }
    save(directory, "state.json", &audit.state).map_err(|e| e.to_string())?;
    save(directory, "told.json", &audit.told).map_err(|e| e.to_string())?;
    println!(
        "{}",
        serde_json::json!({"format":"archie-sentry-ingest-v1",
        "frames":frames,"alerts":alerts_count})
    );
    Ok(())
}

fn main() -> ExitCode {
    let mut arguments: Vec<String> = std::env::args().skip(1).collect();
    let directory = take(&mut arguments, "--state-dir").map(PathBuf::from);
    // `--gateway` may name the router by either address; each family keeps its own.
    let gateway_ip: Option<IpAddr> = take(&mut arguments, "--gateway").and_then(|g| g.parse().ok());
    let gateway = gateway_ip.and_then(|ip| match ip {
        IpAddr::V4(v4) => Some(v4),
        IpAddr::V6(_) => None,
    });
    let gateway_v6 = gateway_ip.and_then(|ip| match ip {
        IpAddr::V6(v6) => Some(v6),
        IpAddr::V4(_) => None,
    });
    let learn = take(&mut arguments, "--learn").and_then(|s| s.parse::<u64>().ok());
    let outcome = match (arguments.first().map(String::as_str), directory) {
        (Some("capture-fields"), None) if arguments.len() == 1 => {
            println!(
                "{}",
                serde_json::json!({"format":"archie-sentry-fields-v1",
                "fields":archie_lan::capture::FIELDS.as_slice(),
                "snaplen":archie_lan::capture::SNAPLEN.parse::<u32>().expect("fixed snaplen")})
            );
            Ok(())
        }
        (Some("ingest-fields"), Some(directory)) if arguments.len() == 1 => ingest_fields(
            &directory,
            Settings {
                learn_ms: learn.unwrap_or(600) * 1000,
                gateway,
                gateway_v6,
            },
        ),
        (Some("interfaces"), _) => Command::new("tshark")
            .arg("-D")
            .status()
            .map(|_| ())
            .map_err(|e| format!("tshark could not be started: {e}")),
        (Some("live"), Some(directory)) => {
            let source = match live_source(&mut arguments) {
                Ok(source) => source,
                Err(error) => {
                    eprintln!("archie-netwatch: {error}");
                    return ExitCode::from(2);
                }
            };
            let settings = Settings {
                learn_ms: learn.unwrap_or(600) * 1000,
                gateway: gateway.or_else(default_gateway),
                gateway_v6: gateway_v6.or_else(default_gateway_v6),
            };
            let router = enrolled(&directory);
            audit(&source, &directory, settings, router)
        }
        (Some("read"), Some(directory)) if arguments.len() == 2 => {
            // A file is audited from its first frame: nothing in it is news to be learned first.
            audit(
                &["-r".to_string(), arguments[1].clone()],
                &directory,
                Settings {
                    learn_ms: learn.unwrap_or(0) * 1000,
                    gateway,
                    gateway_v6,
                },
                None,
            )
        }
        (Some("router"), directory) => router_command(&mut arguments, directory, gateway_ip, learn),
        (Some("leases"), Some(directory)) if arguments.len() == 2 => leases(
            Path::new(&arguments[1]),
            &directory,
            Settings {
                learn_ms: learn.unwrap_or(600) * 1000,
                gateway,
                gateway_v6,
            },
        ),
        (Some("report"), Some(directory)) => report(
            &directory,
            take(&mut arguments, "--alerts")
                .and_then(|n| n.parse().ok())
                .unwrap_or(50),
        ),
        _ => {
            eprintln!(
                "usage: archie-netwatch interfaces\n       archie-netwatch live --interface NAME --state-dir DIR [--learn SECONDS] [--gateway IP] [--filter BPF] [--seconds 1..86400]\n       archie-netwatch read FILE --state-dir DIR [--gateway IP]\n       archie-netwatch report --state-dir DIR [--alerts N]\n       archie-netwatch leases DHCP.LEASES --state-dir DIR [--learn SECONDS]\n       archie-netwatch router detect [--gateway IP]\n       archie-netwatch router enroll --state-dir DIR [--gateway IP] [--kind openwrt|asuswrt|bsd-firewall|linux] [--user NAME] [--port N] [--install]\n       archie-netwatch router pull --state-dir DIR"
            );
            return ExitCode::from(2);
        }
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("archie-netwatch: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use archie_lan::audit::Limits;
    use archie_lan::capture::Packet;

    const TV: &str = "54:4e:f0:07:3d:e8";

    fn saved_router(directory: &Path) -> router::Config {
        let config = router::Config {
            address: "192.0.2.1".parse().unwrap(),
            port: 2222,
            user: "owner".into(),
            kind: router::Kind::Linux,
        };
        save(directory, router::CONFIG, &config).unwrap();
        // Existing key fixture: enrollment must reuse it, never invoke ssh-keygen.
        std::fs::write(directory.join(router::KEY), "test-only-private-key").unwrap();
        std::fs::write(
            directory.join(format!("{}.pub", router::KEY)),
            "ssh-ed25519 AAAAC3Nza fixture",
        )
        .unwrap();
        config
    }

    #[test]
    fn enrollment_retry_preserves_the_saved_connection() {
        let directory = dir("router-retry");
        let original = saved_router(&directory);
        let mut arguments = vec![
            "router".into(),
            "enroll".into(),
            "--kind".into(),
            "linux".into(),
        ];
        router_command(&mut arguments, Some(directory.clone()), None, None).unwrap();
        assert_eq!(enrolled(&directory), Some(original));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn unsupported_install_does_not_replace_the_saved_connection() {
        let directory = dir("router-failed-install");
        let original = saved_router(&directory);
        let mut arguments = vec![
            "router".into(),
            "enroll".into(),
            "--kind".into(),
            "asuswrt".into(),
            "--install".into(),
        ];
        let error = router_command(
            &mut arguments,
            Some(directory.clone()),
            Some("192.0.2.2".parse().unwrap()),
            None,
        )
        .unwrap_err();
        assert!(error.contains("keeps its keys"), "{error}");
        assert_eq!(enrolled(&directory), Some(original));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn the_ipv6_default_route_names_the_router_by_its_next_hop() {
        let default = "00000000000000000000000000000000 00 00000000000000000000000000000000 00 fe800000000000000000000000000001 00000400 00000001 00000000 00450003 eth0";
        assert_eq!(parse_ipv6_default_route(default), "fe80::1".parse().ok());
        // A link route has no next hop; a /64 is not the default route.
        let onlink = "00000000000000000000000000000000 00 00000000000000000000000000000000 00 00000000000000000000000000000000 00000400 00000001 00000000 00200200 lo";
        assert_eq!(parse_ipv6_default_route(onlink), None);
        let prefix = "fd000000000000000000000000000000 40 00000000000000000000000000000000 00 00000000000000000000000000000000 00000100 00000001 00000000 00000001 eth0";
        assert_eq!(parse_ipv6_default_route(prefix), None);
    }

    /// A directory of one's own, emptied before and after.
    fn dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("archie-netwatch-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn from(mac: &str, source: &str, at_ms: u64) -> Packet {
        Packet {
            at_ms,
            length: 100,
            source_mac: Some(mac.into()),
            source: Some(source.parse().unwrap()),
            ..Packet::default()
        }
    }

    #[test]
    fn a_restart_repeats_nothing_that_was_told() {
        let dir = dir("restart");
        let settings = Settings {
            learn_ms: 0,
            gateway: None,
            gateway_v6: None,
        };
        let telnet = Packet {
            destination: Some("192.168.1.1".parse().unwrap()),
            tcp: Some((50_000, 23)),
            opening: true,
            ..from(TV, "192.168.1.84", 2_000)
        };
        let mut first = Audit::new(load(&dir).unwrap(), settings.clone());
        first.told = load_told(&dir);
        assert_eq!(
            first.see(&from(TV, "192.168.1.84", 1_000)).len(),
            1,
            "new-device is told"
        );
        assert_eq!(first.see(&telnet).len(), 1, "cleartext-login is told");
        save(&dir, "state.json", &first.state).unwrap();
        save(&dir, "told.json", &first.told).unwrap();

        // The process stops and starts. The same frames arrive again.
        let mut second = Audit::new(load(&dir).unwrap(), settings);
        second.told = load_told(&dir);
        assert_eq!(second.state, first.state, "the state survives the restart");
        assert!(
            second.see(&from(TV, "192.168.1.84", 3_000)).is_empty(),
            "the device is known"
        );
        assert!(
            second.see(&telnet).is_empty(),
            "the alert was already told before the restart"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_broken_state_file_is_an_error_naming_why_never_a_panic() {
        let dir = dir("broken");
        std::fs::write(dir.join("state.json"), b"{ not json").unwrap();
        let error = load(&dir).unwrap_err();
        assert!(
            error.contains("not the JSON this program writes"),
            "{error}"
        );

        // One device past the cap of devices: refused, naming the cap.
        let device = serde_json::json!({
            "first_seen_ms": 1, "last_seen_ms": 1, "addresses": ["192.168.1.84"],
            "hostname": null, "frames": 1, "bytes": 100, "asked": [], "sent_to": {}
        });
        let cap = Limits::default().devices;
        let devices: serde_json::Map<String, serde_json::Value> = (0..=cap)
            .map(|i| {
                (
                    format!(
                        "aa:bb:cc:{:02x}:{:02x}:{:02x}",
                        i / 65536,
                        i / 256 % 256,
                        i % 256
                    ),
                    device.clone(),
                )
            })
            .collect();
        let state = serde_json::json!({ "format": "archie-netwatch-state-v1", "started_ms": 1, "devices": devices, "owners": {} });
        std::fs::write(dir.join("state.json"), serde_json::to_vec(&state).unwrap()).unwrap();
        let error = load(&dir).unwrap_err();
        assert!(
            error.contains(&format!("is more than the {cap}")),
            "{error}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_told_file_over_the_cap_is_set_aside_and_a_sound_one_kept() {
        let dir = dir("told-cap");
        // Over the cap of remembered alerts: set aside, so the run goes on.
        let cap = Limits::default().told;
        let records: serde_json::Map<String, serde_json::Value> = (0..=cap)
            .map(|i| {
                (
                    format!("alert {i}"),
                    serde_json::json!({ "at_ms": i, "count": 1 }),
                )
            })
            .collect();
        let told = serde_json::json!({ "format": "archie-netwatch-told-v1", "records": records });
        std::fs::write(dir.join("told.json"), serde_json::to_vec(&told).unwrap()).unwrap();
        assert!(
            load_told(&dir).records.is_empty(),
            "over the cap it starts fresh"
        );

        // A sound one comes back whole.
        let mut audit = Audit::new(
            State::default(),
            Settings {
                learn_ms: 0,
                gateway: None,
                gateway_v6: None,
            },
        );
        audit.see(&from(TV, "192.168.1.84", 1_000));
        save(&dir, "told.json", &audit.told).unwrap();
        assert_eq!(load_told(&dir), audit.told);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn the_files_written_are_the_owners_own() {
        use std::os::unix::fs::PermissionsExt;
        let dir = dir("private");
        let audit = Audit::new(State::default(), Settings::default());
        save(&dir, "state.json", &audit.state).unwrap();
        save(&dir, "told.json", &audit.told).unwrap();
        append(
            &dir,
            &[Alert {
                at_ms: 1,
                severity: archie_lan::audit::Severity::Notice,
                rule: "new-device".into(),
                device: TV.into(),
                detail: "first seen".into(),
            }],
        )
        .unwrap();
        for name in ["state.json", "told.json", "alerts.jsonl"] {
            let mode = std::fs::metadata(dir.join(name))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode & 0o007, 0, "{name} is never for others");
            assert_eq!(mode & 0o020, 0, "{name} is never group-writable");
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
