//! Strict host-scoped World console gateway. No shell or worldd credential access.
use archie_core::control::{ControlRequest, ControlResponse, WorldJobAction};
use archie_geofabric::exploration::{self, ExplorationDomain, ExplorationRequest};
use archie_types::PrivacyGrade;
use archie_vault::ContextDomain;
use archie_worldd::navigation::NavigationRequest;
use serde_json::{json, Value};

fn scoped_body(raw: &Value, defaults: Value, domain: ExplorationDomain) -> Result<Value, String> {
    let fields = raw.as_object().ok_or("request must be an object")?;
    let mut body = defaults
        .as_object()
        .ok_or("invalid request defaults")?
        .clone();
    for (key, value) in fields {
        if key == "domain" || !body.contains_key(key) {
            return Err(format!("unsupported request field: {key}"));
        }
        body.insert(key.clone(), value.clone());
    }
    body.insert("domain".into(), json!(domain));
    Ok(Value::Object(body))
}

fn parse(context: &str, raw: &str) -> Result<ControlRequest, String> {
    if context.len() > 256 || raw.len() > 16_384 {
        return Err("operator request exceeds input bound".into());
    }
    let domain: ContextDomain =
        serde_json::from_str(context).map_err(|_| "invalid host context JSON")?;
    let privacy = PrivacyGrade::Public;
    let geographic = if domain.is_work() {
        ExplorationDomain::Work
    } else {
        ExplorationDomain::Personal
    };
    let value: Value = serde_json::from_str(raw).map_err(|_| "invalid operation JSON")?;
    let fields = value.as_object().ok_or("operation must be an object")?;
    let op = value["op"].as_str().ok_or("operation requires op")?;
    let argument = match op {
        "status" | "os_status" | "os_topology" | "os_modules" | "os_providers" | "os_peers"
        | "os_models" | "os_metrics" | "os_boot" | "os_ping" => None,
        "resolve" => Some("query"),
        "plan" | "explore" | "navigate" => Some("request"),
        "navigation_status" | "navigation_stop" | "pause" | "resume" | "cancel" => Some("id"),
        _ => return Err("unsupported operator operation".into()),
    };
    if fields
        .keys()
        .any(|key| key != "op" && Some(key.as_str()) != argument)
    {
        return Err("unsupported operation field; scope belongs to the host".into());
    }
    let id = || -> Result<String, String> {
        let id = value["id"].as_str().ok_or("operation requires id")?;
        if id.len() != 32 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("invalid operation identifier".into());
        }
        Ok(id.into())
    };
    Ok(match op {
        "status" => ControlRequest::WorldStatus { domain, privacy },
        "resolve" => {
            let query = value["query"].as_str().ok_or("operation requires query")?;
            if query.trim().is_empty() || query.chars().count() > 256 {
                return Err("place query must contain 1..256 characters".into());
            }
            ControlRequest::WorldResolve {
                query: query.into(),
                domain,
                privacy,
            }
        }
        "navigate" => {
            let body = scoped_body(
                &value["request"],
                json!(NavigationRequest::default()),
                geographic,
            )?;
            let request: NavigationRequest =
                serde_json::from_value(body).map_err(|_| "invalid navigation request")?;
            request.validate()?;
            ControlRequest::WorldNavigate {
                request,
                domain,
                privacy,
            }
        }
        "plan" | "explore" => {
            let fields = value["request"]
                .as_object()
                .ok_or("request must be an object")?;
            if ["latitude", "longitude", "radius_m"]
                .iter()
                .any(|key| !fields.contains_key(*key))
            {
                return Err("region requires latitude, longitude and radius_m".into());
            }
            let body = scoped_body(
                &value["request"],
                json!(ExplorationRequest::default()),
                geographic,
            )?;
            let request: ExplorationRequest =
                serde_json::from_value(body).map_err(|_| "invalid region request")?;
            exploration::plan(&request).map_err(|error| error.to_string())?;
            if op == "plan" {
                ControlRequest::WorldPlan {
                    request,
                    domain,
                    privacy,
                }
            } else {
                ControlRequest::WorldExplore {
                    request,
                    domain,
                    privacy,
                }
            }
        }
        "navigation_status" => ControlRequest::WorldNavigationStatus {
            id: id()?,
            domain,
            privacy,
        },
        "navigation_stop" => ControlRequest::WorldNavigationStop {
            id: id()?,
            domain,
            privacy,
        },
        "pause" | "resume" | "cancel" => ControlRequest::WorldJob {
            action: match op {
                "pause" => WorldJobAction::Pause,
                "resume" => WorldJobAction::Resume,
                _ => WorldJobAction::Cancel,
            },
            id: id()?,
            domain,
            privacy,
        },
        "os_status" => ControlRequest::Status,
        "os_topology" => ControlRequest::Topology,
        "os_modules" => ControlRequest::Modules,
        "os_providers" => ControlRequest::Providers,
        "os_peers" => ControlRequest::Peers,
        "os_models" => ControlRequest::Models,
        "os_metrics" => ControlRequest::Metrics,
        "os_boot" => ControlRequest::Boot,
        "os_ping" => ControlRequest::Ping,
        _ => unreachable!("validated operation"),
    })
}

async fn execute(context: &str, raw: &str) -> Result<Value, String> {
    let request = parse(context, raw)?;
    if request.is_world() {
        return archie_core::world_control::send(request).await;
    }
    // The explicit OS catalog is read-only. Never retry or fall back to a shell,
    // standalone runtime, or worldd when Core is unavailable.
    let paths = archie_state::StatePaths::from_env();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let mut stream = archie_core::endpoint::connect(&paths)
            .await
            .map_err(|_| "Core operator endpoint unavailable")?;
        let response = archie_core::control::request_over(&mut stream, &request)
            .await
            .map_err(|_| "Core response unavailable; request was not replayed")?;
        match response {
            ControlResponse::Error { message } => Err(message),
            value => serde_json::to_value(value).map_err(|_| "invalid Core response".into()),
        }
    })
    .await
    .map_err(|_| "Core response deadline expired; request was not replayed")?
}

pub async fn run(context: &str, raw: &str) -> std::process::ExitCode {
    match execute(context, raw).await {
        Ok(result) => {
            println!("{}", json!({"ok":true,"result":result}));
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            println!("{}", json!({"ok":false,"error":error}));
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use archie_types::PrivacyGrade;
    use archie_vault::ContextDomain;
    use serde_json::json;

    const CONTEXT: &str = r#""work/acme/maps""#;

    #[test]
    fn fixed_argv_console_command_parses_without_a_shell() {
        use clap::Parser;
        let cli = crate::Cli::try_parse_from([
            "archie",
            "--json",
            "world",
            "operator",
            "--context",
            CONTEXT,
            "--request",
            r#"{"op":"status"}"#,
        ])
        .unwrap();
        let crate::Command::World {
            command: crate::world::WorldCommand::Operator { context, request },
        } = cli.command
        else {
            panic!("operator route")
        };
        assert_eq!(parse(&context, &request).unwrap().name(), "world-status");
    }

    #[test]
    fn host_context_and_public_privacy_bind_navigation_and_capture() {
        let navigation = parse(
            CONTEXT,
            r#"{"op":"navigate","request":{"sources":["blue_marble"],"photogrammetry":false}}"#,
        )
        .unwrap();
        let ControlRequest::WorldNavigate {
            domain,
            privacy,
            request,
        } = navigation
        else {
            panic!("navigation")
        };
        assert_eq!(domain, ContextDomain::work("acme", "maps").unwrap());
        assert_eq!(privacy, PrivacyGrade::Public);
        assert_eq!(
            request.domain,
            archie_geofabric::exploration::ExplorationDomain::Work
        );
        assert!(!request.photogrammetry);
        for op in ["plan", "explore"] {
            let request = parse(CONTEXT, &json!({"op":op,"request":{"latitude":-33.4,"longitude":-70.6,"radius_m":750,"layers":["terrain","buildings"]}}).to_string()).unwrap();
            assert_eq!(
                serde_json::to_value(&request).unwrap()["domain"],
                "work/acme/maps"
            );
            assert_eq!(
                serde_json::to_value(&request).unwrap()["request_body"]["radius_m"],
                750.0
            );
        }
    }

    #[test]
    fn operations_cannot_inject_scope_paths_endpoints_or_unknown_fields() {
        for raw in [
            r#"{"op":"status","domain":"personal"}"#,
            r#"{"op":"status","privacy":"public"}"#,
            r#"{"op":"status","command":"rm -rf /"}"#,
            r#"{"op":"navigate","request":{"domain":"personal"}}"#,
            r#"{"op":"navigate","request":{"url":"https://example.com"}}"#,
            r#"{"op":"explore","request":{"latitude":0,"longitude":0,"radius_m":1,"path":"/tmp/a"}}"#,
            r#"{"op":"shell","command":"echo hi"}"#,
            r#"{"op":"cancel","id":"../escape"}"#,
            r#"{"op":"plan","request":{"latitude":91,"longitude":0,"radius_m":100}}"#,
            r#"{"op":"plan","request":{"latitude":0,"longitude":0}}"#,
            r#"{"op":"resolve","query":" "}"#,
        ] {
            assert!(parse(CONTEXT, raw).is_err(), "accepted {raw}");
        }
        assert!(parse(r#""work""#, r#"{"op":"status"}"#).is_err());
    }

    #[test]
    fn exact_job_identifiers_reach_each_core_operation() {
        for (op, expected) in [
            ("pause", "world-job"),
            ("resume", "world-job"),
            ("cancel", "world-job"),
            ("navigation_status", "world-navigation-status"),
            ("navigation_stop", "world-navigation-stop"),
        ] {
            let request = parse(
                CONTEXT,
                &json!({"op":op,"id":"0123456789abcdef0123456789abcdef"}).to_string(),
            )
            .unwrap();
            assert_eq!(request.name(), expected);
            assert_eq!(
                serde_json::to_value(&request).unwrap()["id"],
                "0123456789abcdef0123456789abcdef"
            );
        }
        assert_eq!(
            parse(CONTEXT, r#"{"op":"resolve","query":"Santiago"}"#)
                .unwrap()
                .name(),
            "world-resolve"
        );
        assert_eq!(
            parse(CONTEXT, r#"{"op":"status"}"#).unwrap().name(),
            "world-status"
        );
    }

    #[test]
    fn os_catalog_only_routes_existing_read_only_core_requests() {
        for (op, expected) in [
            ("os_status", "status"),
            ("os_topology", "topology"),
            ("os_modules", "modules"),
            ("os_providers", "providers"),
            ("os_peers", "peers"),
            ("os_models", "models"),
            ("os_metrics", "metrics"),
            ("os_boot", "boot"),
            ("os_ping", "ping"),
        ] {
            let request = parse(CONTEXT, &json!({"op":op}).to_string()).unwrap();
            assert_eq!(request.name(), expected);
            assert!(!request.is_mutating());
            assert!(!request.is_world());
        }
    }
}
