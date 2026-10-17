//! Owner-enrolled SDK inspection tool. It exposes reads, never arbitrary writes.
use archie_sentry::adapters::{
    process::ProcessRunner,
    router::{
        admin::{AdminConfig, Firmware, ReadKind, RouterAdmin},
        ssh::SshConfig,
    },
};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

fn usage() -> &'static str {
    "router-admin --profiles\nrouter-admin --host PRIVATE_IP --user USER --identity ABSOLUTE_KEY [--known-hosts ABSOLUTE_FILE] [--firmware NAME] [--trust-first-use] [--sudo-read] [--port N] [--ssh-binary ABSOLUTE_PATH] [--read interfaces|routes|neighbors|leases|connections|system|wan|identity]\nFirst-use trust is explicit. Default trust is strict. SSH must already be enabled and the owner key enrolled."
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("router-admin: {error}");
        std::process::exit(1);
    }
}
async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1).peekable();
    let mut values = BTreeMap::new();
    let mut trust = false;
    let mut sudo = false;
    let mut profiles = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("{}", usage());
                return Ok(());
            }
            "--profiles" => profiles = true,
            "--trust-first-use" => trust = true,
            "--sudo-read" => sudo = true,
            "--host" | "--user" | "--identity" | "--known-hosts" | "--firmware" | "--port"
            | "--ssh-binary" | "--read" => {
                let value = args.next().ok_or("missing option value")?;
                if values.insert(arg, value).is_some() {
                    return Err("duplicate option".into());
                }
            }
            _ => return Err("unknown option; use --help".into()),
        }
    }
    if profiles {
        if !values.is_empty() || trust || sudo {
            return Err("--profiles is an offline-only command".into());
        }
        let records:Vec<_>=Firmware::ALL.iter().map(|family|serde_json::json!({"firmware":family,"validation":"fixture-tested; no hardware certification","enrollment":family.enrollment_help()})).collect();
        println!("{}", serde_json::to_string_pretty(&records)?);
        return Ok(());
    }
    let host = values.get("--host").ok_or(usage())?.parse()?;
    let user = values.get("--user").ok_or(usage())?;
    let key = PathBuf::from(values.get("--identity").ok_or(usage())?);
    if !key.is_absolute() || !key.is_file() {
        return Err("identity must name an existing absolute key file".into());
    }
    let default_hosts = key
        .parent()
        .ok_or("key has no parent")?
        .join("archie_router_known_hosts");
    let hosts = values
        .get("--known-hosts")
        .map(PathBuf::from)
        .unwrap_or(default_hosts);
    let default_binary = if cfg!(windows) {
        PathBuf::from(std::env::var_os("WINDIR").ok_or("WINDIR unavailable")?)
            .join("System32/OpenSSH/ssh.exe")
    } else {
        PathBuf::from("/usr/bin/ssh")
    };
    let binary = values
        .get("--ssh-binary")
        .map(PathBuf::from)
        .unwrap_or(default_binary);
    ProcessRunner::tool(binary.to_str().ok_or("invalid SSH path")?)?;
    let ssh = SshConfig::new(
        binary.to_str().ok_or("invalid SSH path")?,
        user,
        host,
        Some(key.to_str().ok_or("invalid key path")?.into()),
    )?
    .with_port(
        values
            .get("--port")
            .map(|s| s.parse())
            .transpose()?
            .unwrap_or(22),
    )?;
    let mut config = AdminConfig::new(ssh, hosts.to_str().ok_or("invalid known-hosts path")?)?;
    config.firmware = values.get("--firmware").map(|s| s.parse()).transpose()?;
    config.trust_first_use = trust;
    config.allow_sudo_read = sudo;
    let read = values
        .get("--read")
        .map(|s| s.parse::<ReadKind>())
        .transpose()?;
    let admin = RouterAdmin::connect(config, Arc::new(ProcessRunner::default())).await?;
    let evidence = match read {
        Some(kind) => Some(admin.read(kind).await?),
        None => None,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"access":admin.report(),"read":evidence})
        )?
    );
    Ok(())
}
