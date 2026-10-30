//! `archie-worldd` — the only ArchieOS display component allowed Internet egress.

use archie_observability::{init_logging, LogFormat};
use archie_worldd::{
    exploration_control,
    exploration_runtime::{ExplorationConfig, ExplorationService},
};
use archie_worldd::{http, WorldService};
use clap::Parser;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tracing::{error, info, warn};

#[derive(Debug, Parser)]
#[command(
    name = "archie-worldd",
    version,
    about = "Fetch and normalize authoritative world data."
)]
struct Args {
    /// Loopback-only API consumed by archie-displayd.
    #[arg(long, default_value = "127.0.0.1:7667")]
    bind: SocketAddr,

    /// Provider refresh cadence. The provider service independently enforces backoff.
    #[arg(long, default_value_t = 60_000)]
    poll_ms: u64,

    #[arg(long)]
    text_logs: bool,

    /// Unified LAN device view (`archie-sentry-devices-v1`), as written by
    /// `archie sentry-devices --json`. Read at startup and on every poll tick;
    /// a rejected or unreadable file keeps the last known device graph.
    #[arg(long)]
    lan_view: Option<std::path::PathBuf>,
    /// Enable owner-requested GeoFabric captures (provider traffic independent of UI focus).
    #[arg(long)]
    enable_exploration: bool,
    /// Isolated capture domain. A daemon never serves both domains together.
    #[arg(long, default_value="personal", value_parser=["personal", "work"])]
    domain: String,
    /// Private cache/manifests, separate from Core state and keys.
    #[arg(long)]
    exploration_cache: Option<std::path::PathBuf>,
    /// Private owner command descriptor. Never served by the display HTTP API.
    #[arg(long)]
    exploration_runtime: Option<std::path::PathBuf>,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let args = Args::parse();
    init_logging(
        if args.text_logs {
            LogFormat::Text
        } else {
            LogFormat::Json
        },
        "info",
    );

    let listener = match http::bind(args.bind).await {
        Ok(listener) => listener,
        Err(error) => {
            error!(%error, address = %args.bind, "cannot bind world endpoint");
            return std::process::ExitCode::FAILURE;
        }
    };
    let service = Arc::new(WorldService::live_usgs());
    if let Some(path) = &args.lan_view {
        refresh_lan_view(&service, path).await;
    }
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let exploration = if args.enable_exploration {
        let domain = if args.domain == "work" {
            archie_geofabric::exploration::ExplorationDomain::Work
        } else {
            archie_geofabric::exploration::ExplorationDomain::Personal
        };
        let cache = args.exploration_cache.unwrap_or_else(|| {
            std::env::var_os("XDG_CACHE_HOME")
                .or_else(|| std::env::var_os("LOCALAPPDATA"))
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| {
                    std::env::var_os("HOME")
                        .map(std::path::PathBuf::from)
                        .unwrap_or_else(std::env::temp_dir)
                        .join(".cache")
                })
                .join("archie-world")
        });
        match ExplorationService::new(ExplorationConfig {
            cache_dir: cache,
            domain,
        }) {
            Ok(service) => Some(service),
            Err(error) => {
                error!(%error,"cannot start exploration");
                return std::process::ExitCode::FAILURE;
            }
        }
    } else {
        None
    };
    let exploration_runtime = args
        .exploration_runtime
        .unwrap_or_else(exploration_control::default_runtime_dir);
    let control_failed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let exploration_control_task = exploration.as_ref().map(|service| {
        let service = service.clone();
        let runtime = exploration_runtime.clone();
        let shutdown = shutdown_rx.clone();
        let stop = shutdown_tx.clone();
        let failed = control_failed.clone();
        tokio::spawn(async move {
            let result = exploration_control::run(service, runtime, shutdown).await;
            if let Err(error) = &result {
                error!(%error,"private exploration control stopped");
                failed.store(true, std::sync::atomic::Ordering::Relaxed);
                let _ = stop.send(true);
            }
            result
        })
    });

    let refresher = {
        let service = service.clone();
        let lan_view = args.lan_view.clone();
        let mut shutdown = shutdown_rx.clone();
        let interval = Duration::from_millis(args.poll_ms.max(10_000));
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    _ = shutdown.changed() => if *shutdown.borrow() { return; },
                    _ = ticker.tick() => {
                        service.refresh().await;
                        if let Some(path) = &lan_view {
                            refresh_lan_view(&service, path).await;
                        }
                    },
                }
            }
        })
    };

    let serving = {
        let service = service.clone();
        let exploration = exploration.clone();
        let exploration_runtime = exploration_runtime.clone();
        tokio::spawn(http::serve(
            listener,
            move |request| {
                let service = service.clone();
                let exploration = exploration.clone();
                let exploration_runtime = exploration_runtime.clone();
                Box::pin(async move {
                    match request.path.as_str() {
                        path if path == "/v1/exploration"
                            || path.starts_with("/v1/exploration/jobs/") =>
                        {
                            exploration_response(
                                exploration.as_ref(),
                                &exploration_runtime,
                                request.authorization.as_deref(),
                                path,
                            )
                            .await
                        }
                        "/v1/world" => match serde_json::to_vec(&service.current().await) {
                            Ok(body) => http::Response::json(body),
                            Err(_) => http::Response::error(500, b"cannot encode world snapshot\n"),
                        },
                        "/healthz" => http::Response::json(b"{\"ok\":true}"),
                        _ => http::Response::error(404, b"not found\n"),
                    }
                })
            },
            shutdown_rx,
        ))
    };

    info!(address = %args.bind, "world adapter listening");
    let mut requested_shutdown = shutdown_tx.subscribe();
    if !*requested_shutdown.borrow() {
        tokio::select! {_=wait_for_shutdown()=>{},_=requested_shutdown.changed()=>{}}
    }
    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(3), serving).await;
    let _ = tokio::time::timeout(Duration::from_secs(3), refresher).await;
    if let Some(task) = exploration_control_task {
        let _ = tokio::time::timeout(Duration::from_secs(3), task).await;
    }
    if control_failed.load(std::sync::atomic::Ordering::Relaxed) {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    }
}

async fn exploration_response(
    service: Option<&ExplorationService>,
    runtime: &std::path::Path,
    authorization: Option<&str>,
    path: &str,
) -> http::Response {
    let selected = if path == "/v1/exploration" {
        None
    } else if let Some(id) = path
        .strip_prefix("/v1/exploration/jobs/")
        .filter(|id| id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        Some(id)
    } else {
        return http::Response::error(404, b"not found\n");
    };
    if service.is_some()
        && !archie_worldd::exploration_projection::authorized(runtime, authorization)
    {
        return http::Response::error(
            403,
            b"private World projection requires its read-only credential\n",
        );
    }
    let view = match (service, selected) {
        (Some(service), Some(id)) => match service.snapshot_for(id).await {
            Some(view) => view,
            None => return http::Response::error(404, b"capture not found\n"),
        },
        (Some(service), None) => service.snapshot().await,
        (None, Some(_)) => return http::Response::error(404, b"capture not found\n"),
        (None, None) => archie_worldd::exploration::ExplorationSnapshot::unavailable("disabled"),
    };
    match serde_json::to_vec(&view) {
        Ok(body) => http::Response::json(body),
        Err(_) => http::Response::error(500, b"cannot encode exploration\n"),
    }
}

/// Read the unified device view and fold it into the world graph. A missing or
/// rejected file keeps the last known graph; the log says which happened.
async fn refresh_lan_view(service: &WorldService, path: &std::path::Path) {
    let size = match tokio::fs::metadata(path).await {
        Ok(metadata) => metadata.len(),
        Err(error) => {
            warn!(%error, "lan view unreadable; keeping the last known device graph");
            return;
        }
    };
    if size > archie_worldd::lan::MAX_VIEW_BYTES as u64 {
        warn!("lan view exceeds the size limit; keeping the last known device graph");
        return;
    }
    match tokio::fs::read(path).await {
        Ok(bytes) => match service.ingest_lan_view(&bytes).await {
            Ok(report) => info!(
                devices = report.devices,
                "lan view ingested into the world graph"
            ),
            Err(error) => warn!(%error, "lan view rejected; keeping the last known device graph"),
        },
        Err(error) => warn!(%error, "lan view unreadable; keeping the last known device graph"),
    }
}

#[cfg(unix)]
async fn wait_for_shutdown() {
    use tokio::signal::unix::{signal, SignalKind};
    let Ok(mut term) = signal(SignalKind::terminate()) else {
        let _ = tokio::signal::ctrl_c().await;
        return;
    };
    tokio::select! { _ = term.recv() => {}, _ = tokio::signal::ctrl_c() => {} }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn selected_projection_transport_requires_auth_and_remains_get_only() {
        let root = std::env::temp_dir().join(format!(
            "archie-selected-route-{:032x}",
            rand::random::<u128>()
        ));
        let runtime = root.join("runtime");
        archie_worldd::exploration_runtime::private_directory(&runtime).unwrap();
        archie_worldd::exploration_projection::ensure(&runtime).unwrap();
        let token = archie_worldd::exploration_projection::read_token(&runtime).unwrap();
        let service = ExplorationService::new(ExplorationConfig {
            cache_dir: root.join("cache"),
            domain: archie_geofabric::exploration::ExplorationDomain::Personal,
        })
        .unwrap();
        let listener = http::bind("127.0.0.1:0".parse().unwrap()).await.unwrap();
        let address = listener.local_addr().unwrap();
        let (stop, shutdown) = watch::channel(false);
        let projected = service.clone();
        let server = tokio::spawn(http::serve(
            listener,
            move |request| {
                let service = projected.clone();
                let runtime = runtime.clone();
                Box::pin(async move {
                    exploration_response(
                        Some(&service),
                        &runtime,
                        request.authorization.as_deref(),
                        &request.path,
                    )
                    .await
                })
            },
            shutdown,
        ));
        let path = format!("/v1/exploration/jobs/{}", "a".repeat(32));
        for (method, authorization, status) in [
            ("GET", String::new(), 403),
            ("GET", format!("Authorization: Bearer {token}\r\n"), 404),
            ("POST", format!("Authorization: Bearer {token}\r\n"), 405),
        ] {
            let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
            socket.write_all(format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\n{authorization}Connection: close\r\n\r\n").as_bytes()).await.unwrap();
            let mut response = String::new();
            socket.read_to_string(&mut response).await.unwrap();
            assert!(
                response.starts_with(&format!("HTTP/1.1 {status} ")),
                "{response}"
            );
        }
        stop.send(true).unwrap();
        server.await.unwrap();
        assert!(service
            .snapshot()
            .await
            .providers
            .iter()
            .all(|provider| provider.requests_sent == 0));
        assert!(!root.join("runtime/control.json").exists());
    }
}

#[cfg(not(unix))]
async fn wait_for_shutdown() {
    let _ = tokio::signal::ctrl_c().await;
}
