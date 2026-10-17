//! SDK fixtures: these are not hardware/firmware certification.
use archie_sentry::{
    adapters::{
        process::{CommandOutput, CommandRunner},
        router::{
            admin::{AdminConfig, Firmware, Privilege, ReadKind, RouterAdmin},
            ssh::SshConfig,
        },
    },
    engine::AdapterError,
};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Default)]
struct Fixture {
    calls: Mutex<Vec<Vec<String>>>,
    marker: String,
    uid: String,
    sudo: bool,
    oversized: bool,
    group_output: Option<String>,
    ssh_failure: bool,
}
impl CommandRunner for Fixture {
    fn run(&self, argv: &[String], timeout: Duration) -> Result<CommandOutput, AdapterError> {
        assert!(timeout <= Duration::from_secs(30));
        self.calls.lock().unwrap().push(argv.to_vec());
        if self.ssh_failure {
            return Ok(CommandOutput {
                status: 255,
                stdout: vec![],
                stderr: vec![],
            });
        }
        let command = argv.last().unwrap().as_str();
        let (status, output) = if command.contains("/etc/os-release") {
            (0, self.marker.clone())
        } else if command == "id -u" {
            (0, self.uid.clone())
        } else if command == "sudo -n -- id -u" && self.sudo {
            (0, "0\n".into())
        } else if command == "/system resource print" {
            (
                0,
                "version: 7.20 (stable)\nboard-name: hAP ax2\narchitecture-name: arm64\n".into(),
            )
        } else if command == "/user print detail without-paging where name=admin" {
            (0, "0 name=admin group=operators disabled=no\n".into())
        } else if command == "/user group print detail without-paging where name=operators" {
            (
                0,
                self.group_output
                    .clone()
                    .unwrap_or_else(|| "0 name=operators policy=ssh,read,write,test\n".into()),
            )
        } else if command == "ip -s link" {
            (0, "1: lo: <LOOPBACK>\n".into())
        } else {
            (1, String::new())
        };
        let stdout = if self.oversized {
            vec![b'x'; 65537]
        } else {
            output.into_bytes()
        };
        Ok(CommandOutput {
            status,
            stdout,
            stderr: Vec::new(),
        })
    }
}
fn config() -> AdminConfig {
    let (binary, key, hosts) = if cfg!(windows) {
        ("C:/tools/ssh.exe", "C:/keys/admin", "C:/keys/hosts")
    } else {
        ("/usr/bin/ssh", "/keys/admin", "/keys/hosts")
    };
    AdminConfig::new(
        SshConfig::new(
            binary,
            "admin",
            "192.168.1.1".parse().unwrap(),
            Some(key.into()),
        )
        .unwrap(),
        hosts,
    )
    .unwrap()
}
fn openwrt(uid: &str) -> Arc<Fixture> {
    Arc::new(Fixture {
        marker: "ID=openwrt\nLinux\n".into(),
        uid: uid.into(),
        ..Default::default()
    })
}

#[tokio::test]
async fn authenticated_openwrt_root_and_typed_telemetry() {
    let runner = openwrt("0\n");
    let admin = RouterAdmin::connect(config(), runner.clone())
        .await
        .unwrap();
    assert_eq!(admin.report().firmware, Firmware::Openwrt);
    assert!(admin.report().firmware_verified);
    assert_eq!(admin.report().privilege, Privilege::Root);
    let evidence = admin.read(ReadKind::Interfaces).await.unwrap();
    assert!(evidence.output.contains("LOOPBACK"));
    assert_eq!(evidence.sha256.len(), 64);
    assert_eq!(
        evidence.host,
        "192.168.1.1".parse::<std::net::Ipv4Addr>().unwrap()
    );
    assert_eq!(evidence.port, 22);
    assert_eq!(evidence.user, "admin");
    use sha2::{Digest, Sha256};
    let digest = format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&(
                evidence.host,
                evidence.port,
                &evidence.user,
                evidence.firmware,
                evidence.kind,
                &evidence.output,
                evidence.observed_at_ms
            ))
            .unwrap()
        )
    );
    assert_eq!(evidence.sha256, digest);
    let calls = runner.calls.lock().unwrap();
    let argv = &calls[0];
    for option in [
        "StrictHostKeyChecking=yes",
        "IdentityAgent=none",
        "IdentitiesOnly=yes",
        "PasswordAuthentication=no",
        "ClearAllForwardings=yes",
        "PermitLocalCommand=no",
    ] {
        assert!(argv.iter().any(|a| a == option), "{option}");
    }
    assert!(argv.windows(2).any(|a| a == ["-F", "none"]));
    assert!(!format!("{admin:?}").contains("/keys/admin"));
}
#[tokio::test]
async fn username_or_banner_never_proves_root() {
    let mut cfg = config();
    cfg.ssh.user = "root".into();
    let admin = RouterAdmin::connect(cfg, openwrt("1000\n")).await.unwrap();
    assert_eq!(admin.report().privilege, Privilege::User);
}
#[tokio::test]
async fn existing_noninteractive_sudo_requires_explicit_opt_in() {
    let runner = Arc::new(Fixture {
        marker: "ID=vyos\nLinux\n".into(),
        uid: "1000\n".into(),
        sudo: true,
        ..Default::default()
    });
    let normal = RouterAdmin::connect(config(), runner.clone())
        .await
        .unwrap();
    assert_eq!(normal.report().privilege, Privilege::User);
    assert!(!runner
        .calls
        .lock()
        .unwrap()
        .iter()
        .any(|a| a.last().unwrap().contains("sudo")));
    let mut cfg = config();
    cfg.allow_sudo_read = true;
    let elevated = RouterAdmin::connect(cfg, runner).await.unwrap();
    assert_eq!(elevated.report().privilege, Privilege::SudoRoot);
    assert!(elevated.report().privilege_scope.contains("id -u only"));
}
#[tokio::test]
async fn routeros_checks_effective_policies_instead_of_claiming_posix_root() {
    let mut cfg = config();
    cfg.firmware = Some(Firmware::Routeros);
    // Explicit RouterOS uses its own CLI, not a POSIX detector.
    let admin = RouterAdmin::connect(cfg, openwrt("0")).await.unwrap();
    assert_eq!(admin.report().firmware, Firmware::Routeros);
    assert_eq!(admin.report().privilege, Privilege::NetworkAdmin);
    assert!(admin.report().policies.contains(&"write".into()));
    assert!(admin.read(ReadKind::Leases).await.is_err()); // fixture never supplied leases
}
#[tokio::test]
async fn ambiguous_identity_is_rejected_instead_of_switching_dialects() {
    let runner = Arc::new(Fixture {
        marker: "ID=openwrt\nASUSWRT Merlin\nLinux\n".into(),
        uid: "0\n".into(),
        ..Default::default()
    });
    assert!(RouterAdmin::connect(config(), runner).await.is_err());
}
#[tokio::test]
async fn root_requires_the_exact_canonical_uid_response() {
    let admin = RouterAdmin::connect(config(), openwrt("+0\n"))
        .await
        .unwrap();
    assert_eq!(admin.report().privilege, Privilege::Unknown);
}
#[tokio::test]
async fn catalogue_families_are_detected_from_authenticated_firmware_markers() {
    for (marker, family) in [
        ("DISTRIB_ID='OpenWrt'\nLinux", Firmware::Openwrt),
        ("ASUSWRT-Merlin\nLinux", Firmware::Asuswrt),
        ("FreshTomato 2025.3\nLinux", Firmware::Freshtomato),
        ("DD-WRT v3.0\nLinux", Firmware::Ddwrt),
        ("ID=vyos\nLinux", Firmware::Vyos),
        ("OPNsense 25.1\nFreeBSD", Firmware::Opnsense),
        ("pfSense\nFreeBSD", Firmware::Pfsense),
        ("ID=debian\nLinux", Firmware::Linux),
    ] {
        let runner = Arc::new(Fixture {
            marker: marker.into(),
            uid: "0\n".into(),
            ..Default::default()
        });
        let admin = RouterAdmin::connect(config(), runner).await.unwrap();
        assert_eq!(admin.report().firmware, family, "{marker}");
        assert!(admin.report().firmware_verified);
    }
}
#[tokio::test]
async fn configured_profile_cannot_replace_conflicting_authenticated_identity() {
    let mut cfg = config();
    cfg.firmware = Some(Firmware::Asuswrt);
    assert!(RouterAdmin::connect(cfg, openwrt("0")).await.is_err());
}
#[tokio::test]
async fn mutated_configuration_is_revalidated_before_any_process() {
    let runner = openwrt("0");
    let mut cfg = config();
    cfg.ssh.user = "root; reboot".into();
    assert!(RouterAdmin::connect(cfg, runner.clone()).await.is_err());
    assert!(runner.calls.lock().unwrap().is_empty());
}
#[tokio::test]
async fn oversized_output_does_not_establish_identity_or_root() {
    let runner = Arc::new(Fixture {
        oversized: true,
        ..Default::default()
    });
    assert!(RouterAdmin::connect(config(), runner).await.is_err());
}
#[test]
fn driver_catalogue_is_extensible_without_claiming_hardware_support() {
    assert!(Firmware::ALL.len() >= 8);
    for firmware in Firmware::ALL {
        assert_eq!(firmware.as_str().parse::<Firmware>().unwrap(), *firmware);
    }
    assert!("dropbear".parse::<Firmware>().is_err());
}

#[tokio::test]
async fn routeros_wrapped_group_policies_keep_explicit_revocations() {
    for (policies, expected) in [
        (
            "0 name=operators policy=ssh,read,\n  write,test skin=default",
            Privilege::NetworkAdmin,
        ),
        (
            "0 name=operators policy=ssh,read,write,\n  !write,test skin=default",
            Privilege::Unknown,
        ),
    ] {
        let mut cfg = config();
        cfg.firmware = Some(Firmware::Routeros);
        let runner = Arc::new(Fixture {
            group_output: Some(policies.into()),
            ..Default::default()
        });
        assert_eq!(
            RouterAdmin::connect(cfg, runner)
                .await
                .unwrap()
                .report()
                .privilege,
            expected
        );
    }
}
#[tokio::test]
async fn failed_authentication_does_not_cycle_through_dialects_or_users() {
    let runner = Arc::new(Fixture {
        ssh_failure: true,
        ..Default::default()
    });
    assert!(RouterAdmin::connect(config(), runner.clone())
        .await
        .is_err());
    assert_eq!(runner.calls.lock().unwrap().len(), 1);
}
#[test]
fn known_hosts_cannot_be_the_private_identity_file() {
    let cfg = config();
    assert!(AdminConfig::new(cfg.ssh.clone(), cfg.ssh.identity_file.as_deref().unwrap()).is_err());
}
#[tokio::test]
async fn enrollment_trust_is_explicit_and_core_handoff_uses_strict_trust() {
    let runner = openwrt("0");
    let mut cfg = config();
    cfg.trust_first_use = true;
    let admin = RouterAdmin::connect(cfg, runner.clone()).await.unwrap();
    let before = runner.calls.lock().unwrap().len();
    admin
        .core_adapter(
            "edge",
            "router",
            "gateway",
            "192.168.1.107".parse().unwrap(),
        )
        .await
        .unwrap();
    let calls = runner.calls.lock().unwrap();
    let first_policy = |a: &Vec<String>| {
        a.iter()
            .find(|s| s.starts_with("StrictHostKeyChecking="))
            .unwrap()
            .clone()
    };
    assert_eq!(first_policy(&calls[0]), "StrictHostKeyChecking=accept-new");
    assert!(calls[before..]
        .iter()
        .all(|a| first_policy(a) == "StrictHostKeyChecking=yes"));
}
