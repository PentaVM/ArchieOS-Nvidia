use std::io::Write;
use std::process::{Command, Stdio};

fn netwatch() -> Command {
    Command::new(env!("CARGO_BIN_EXE_archie-netwatch"))
}

#[test]
fn capture_schema_is_the_actual_parser_catalogue() {
    let result = netwatch().arg("capture-fields").output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["format"], "archie-sentry-fields-v1");
    assert_eq!(
        value["fields"],
        serde_json::json!(archie_lan::capture::FIELDS.as_slice())
    );
    assert_eq!(value["snaplen"], 768);
    assert!(!value["fields"].as_array().unwrap().iter().any(|v| {
        v.as_str().unwrap().contains("password") || v.as_str().unwrap().contains("request.arg")
    }));
}

fn root() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("sentry-metadata-{}", uuid::Uuid::new_v4()))
}

#[test]
fn finite_metadata_window_keeps_wan_destination_and_durable_state() {
    let directory = root();
    let mut row = vec![String::new(); archie_lan::capture::FIELDS.len()];
    row[0] = "1792200000.0".into();
    row[1] = "100".into();
    row[2] = "02:00:00:00:00:01".into();
    row[3] = "02:00:00:00:00:02".into();
    row[4] = "192.168.1.107".into();
    row[5] = "1.1.1.1".into();
    row[10] = "50001".into();
    row[11] = "443".into();
    row[12] = "1".into();
    let mut process = netwatch()
        .args(["ingest-fields", "--learn", "0", "--state-dir"])
        .arg(&directory)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(process.stdin.take().unwrap(), "{}", row.join("\t")).unwrap();
    let result = process.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report = netwatch()
        .args(["report", "--state-dir"])
        .arg(&directory)
        .output()
        .unwrap();
    assert!(report.status.success());
    let value: serde_json::Value = serde_json::from_slice(&report.stdout).unwrap();
    let devices = value["state"]["devices"].as_object().unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices["02:00:00:00:00:01"]["sent_to"]["1.1.1.1"], 100);
    assert!(directory.join("told.json").exists());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn malformed_metadata_is_not_silently_admitted() {
    let directory = root();
    let mut process = netwatch()
        .args(["ingest-fields", "--state-dir"])
        .arg(&directory)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(process.stdin.take().unwrap(), "not\ta\tpacket").unwrap();
    let result = process.wait_with_output().unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("metadata"));
    assert!(!directory.join("state.json").exists());
    if directory.exists() {
        std::fs::remove_dir_all(directory).unwrap();
    }
}
