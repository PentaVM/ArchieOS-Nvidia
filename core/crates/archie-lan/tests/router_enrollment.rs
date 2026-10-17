//! CLI lifecycle tests with a local SSH protocol fixture, never a router login.
#![cfg(unix)]

use archie_lan::router::{self, Config, Kind};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};

struct Fixture(PathBuf);

impl Fixture {
    fn new(name: &str) -> Self {
        let directory =
            std::env::temp_dir().join(format!("archie-router-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let fixture = Self(directory);
        fixture.save(&fixture.original());
        std::fs::write(fixture.0.join(router::KEY), "fixture private key").unwrap();
        std::fs::write(
            fixture.0.join(format!("{}.pub", router::KEY)),
            "ssh-ed25519 AAAAC3Nza fixture",
        )
        .unwrap();
        let ssh = fixture.0.join("ssh");
        std::fs::write(
            &ssh,
            r#"#!/bin/sh
case "$*" in
  *BatchMode=yes*)
    printf '%s\n' "$@" > "$FIXTURE_ARGUMENTS"
    if [ "$FIXTURE_MODE" = fail-pull ]; then
      echo 'fixture read failed' >&2
      exit 1
    fi
    printf '0 54:4e:f0:07:3d:e8 192.168.1.84 fixture *\n@@ARCHIE-NEIGHBOURS@@\n@@ARCHIE-FLOWS@@\n'
    ;;
  *) /bin/cat > /dev/null ;;
esac
"#,
        )
        .unwrap();
        std::fs::set_permissions(ssh, std::fs::Permissions::from_mode(0o700)).unwrap();
        fixture
    }

    fn original(&self) -> Config {
        Config {
            address: "192.0.2.1".parse().unwrap(),
            port: 2222,
            user: "owner".into(),
            kind: Kind::Linux,
        }
    }

    fn save(&self, config: &Config) {
        std::fs::write(
            self.0.join(router::CONFIG),
            serde_json::to_vec(config).unwrap(),
        )
        .unwrap();
    }

    fn config(&self) -> Config {
        serde_json::from_slice(&std::fs::read(self.0.join(router::CONFIG)).unwrap()).unwrap()
    }

    fn run(&self, action: &str, options: &[&str], mode: &str) -> Output {
        Command::new(env!("CARGO_BIN_EXE_archie-netwatch"))
            .args(["router", action, "--state-dir"])
            .arg(&self.0)
            .args(options)
            .env("PATH", &self.0)
            .env("FIXTURE_ARGUMENTS", self.0.join("arguments"))
            .env("FIXTURE_MODE", mode)
            .output()
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn retry_without_options_preserves_the_router_and_says_access_is_unverified() {
    let fixture = Fixture::new("resume");
    let output = fixture.run("enroll", &[], "unused");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fixture.config(), fixture.original());
    assert!(String::from_utf8_lossy(&output.stderr).contains("SSH access is unverified"));
    assert!(
        !fixture.0.join("arguments").exists(),
        "manual enrollment never logs in"
    );
}

#[test]
fn failed_verification_after_install_keeps_the_previous_configuration() {
    let fixture = Fixture::new("failed-verification");
    let output = fixture.run(
        "enroll",
        &[
            "--gateway",
            "192.0.2.2",
            "--user",
            "replacement",
            "--port",
            "2200",
            "--install",
        ],
        "fail-pull",
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("fixture read failed"));
    assert_eq!(fixture.config(), fixture.original());
}

#[test]
fn successful_verification_persists_explicit_connection_overrides() {
    let fixture = Fixture::new("verified-overrides");
    let output = fixture.run(
        "enroll",
        &[
            "--gateway",
            "192.0.2.2",
            "--user",
            "replacement",
            "--port",
            "2200",
            "--kind",
            "openwrt",
            "--install",
        ],
        "success",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fixture.config(),
        Config {
            address: "192.0.2.2".parse().unwrap(),
            user: "replacement".into(),
            port: 2200,
            kind: Kind::Openwrt
        }
    );
}

#[test]
fn pull_uses_the_persisted_target_even_with_an_unrelated_gateway_option() {
    let fixture = Fixture::new("pull");
    let output = fixture.run("pull", &["--gateway", "192.0.2.99"], "success");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let arguments = std::fs::read_to_string(fixture.0.join("arguments")).unwrap();
    assert!(arguments
        .lines()
        .any(|argument| argument == "owner@192.0.2.1"));
    assert!(fixture.0.join("state.json").exists());
}
