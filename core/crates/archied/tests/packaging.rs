//! Packaging invariants for the ArchieOS ISO.
//!
//! # Why these are Rust tests
//!
//! The 0.1.0 ISO built cleanly, booted cleanly, and still failed: `archied` started as
//! `User=archie`, could not read its own root-owned config, exited 78 (EX_CONFIG), and
//! `RestartPreventExitStatus=78` correctly kept it down — so the image never reached
//! ARCHIE READY. Nothing in the Rust test suite could see that, because the fault lived
//! in the seam between the systemd unit, sysusers.d and tmpfiles.d.
//!
//! Catching it needed a 45-minute ISO build and a QEMU boot. These tests check the same
//! invariants by reading the packaging files directly, so the seam is verified in
//! seconds on every `cargo test`, on any platform.
//!
//! They assert *relationships between files*, never the contents of one file alone.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    // core/crates/archied -> core/crates -> core -> repo root
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("archied crate must sit three levels below the repository root")
        .to_path_buf()
}

/// Counts files with the given extension anywhere under `dir`, matching the
/// recursive `find ... -type f` the release builder actually uses to stage
/// `providers/data` (which groups manifests into category subdirectories).
fn count_files_with_extension_recursive(dir: &Path, extension: &str) -> usize {
    let mut count = 0;
    for entry in std::fs::read_dir(dir).unwrap().filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            count += count_files_with_extension_recursive(&path, extension);
        } else if path.extension().is_some_and(|ext| ext == extension) {
            count += 1;
        }
    }
    count
}

#[test]
fn core_waits_for_a_dedicated_non_superuser_database() {
    let core = unit_directives(&read("airootfs/etc/systemd/system/archied.service"));
    assert!(core["Requires"]
        .split_whitespace()
        .any(|u| u == "archie-database.service"));
    let database =
        std::fs::read_to_string(repo_root().join("distro/database/archie-database.service"))
            .unwrap();
    assert_eq!(
        unit_directives(&database)["Requires"],
        "archie-postgres.service"
    );
    let pg = std::fs::read_to_string(repo_root().join("distro/database/archie-postgres.service"))
        .unwrap();
    let pg = unit_directives(&pg);
    assert_eq!(pg["User"], "postgres");
    assert_eq!(pg["StateDirectory"], "archie-postgres");
    assert!(pg["Requires"].contains("archie-postgres-init.service"));
    let build = read("build.sh");
    assert!(build.contains("-p archie-state"));
    assert!(build.contains("release/archie-db"));
    for package in ["postgresql", "timescaledb", "pgvector"] {
        assert!(read("packages.x86_64").lines().any(|p| p == package));
    }
    let provision =
        std::fs::read_to_string(repo_root().join("distro/database/provision-node.sh")).unwrap();
    assert!(provision.contains("NOSUPERUSER NOCREATEDB NOCREATEROLE"));
    assert!(provision.contains("chmod 0640"));
    assert!(!provision.contains("DROP DATABASE"));
}

fn distro(relative: &str) -> PathBuf {
    repo_root().join("distro/archiso").join(relative)
}

#[test]
fn iso_extension_lock_matches_the_runtime_compatibility_contract() {
    let root = repo_root().join("distro/database");
    let contract: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("compatibility.json")).unwrap())
            .unwrap();
    let lock = std::fs::read_to_string(root.join("arch-packages.lock")).unwrap();
    let entries: Vec<_> = lock
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .collect();
    assert_eq!(entries.len(), 2);
    for package in ["timescaledb", "pgvector"] {
        let version = contract[format!("{package}_version")].as_str().unwrap();
        let prefix = format!("{package}-{version}-");
        let fields: Vec<_> = entries
            .iter()
            .find_map(|line| {
                let fields: Vec<_> = line.split_whitespace().collect();
                (fields.len() == 3 && fields[1].starts_with(&prefix)).then_some(fields)
            })
            .expect("reviewed runtime version must have an exact package lock");
        assert_eq!(fields[0].len(), 64);
        assert!(fields[0].bytes().all(|b| b.is_ascii_hexdigit()));
        assert!(fields[2].starts_with("https://archive.archlinux.org/packages/"));
        assert!(fields[2].ends_with(fields[1]));
    }
    let runtime = std::fs::read_to_string(root.join("pacman.conf")).unwrap();
    assert!(runtime
        .lines()
        .any(|line| line == "IgnorePkg = timescaledb pgvector"));
    assert!(read("build.sh").contains("distro/database/stage-reviewed-packages.sh"));
}

fn read(relative: &str) -> String {
    let path = distro(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read packaging file {}: {e}", path.display()))
}

/// `Key=Value` pairs from a systemd unit, last assignment winning.
fn unit_directives(text: &str) -> BTreeMap<String, String> {
    let mut directives = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            directives.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    directives
}

/// One parsed tmpfiles.d entry.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TmpfilesEntry {
    kind: String,
    path: String,
    mode: String,
    user: String,
    group: String,
}

fn tmpfiles_entries() -> Vec<TmpfilesEntry> {
    read("airootfs/usr/lib/tmpfiles.d/archie.conf")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            assert!(
                fields.len() >= 5,
                "malformed tmpfiles line (need at least 5 fields): {line:?}"
            );
            TmpfilesEntry {
                kind: fields[0].to_string(),
                path: fields[1].to_string(),
                mode: fields[2].to_string(),
                user: fields[3].to_string(),
                group: fields[4].to_string(),
            }
        })
        .collect()
}

fn entry_for(path: &str) -> Option<TmpfilesEntry> {
    tmpfiles_entries().into_iter().find(|e| e.path == path)
}

#[test]
fn cortex_can_traverse_its_graph_parent_without_core_group_membership() {
    let cortex =
        std::fs::read_to_string(repo_root().join("distro/cortex/archie-cortex.service")).unwrap();
    let unit = unit_directives(&cortex);
    let user = unit.get("User").unwrap();
    let parent = entry_for("/var/lib/archie").unwrap();
    assert_ne!(
        unit.get("Group").unwrap(),
        &parent.group,
        "Cortex must not gain the Core socket group"
    );
    assert_eq!(
        parent.mode, "0750",
        "Core state must not become globally traversable/readable"
    );
    assert!(unit
        .get("ExecStart")
        .unwrap()
        .contains("/var/lib/archie/knowledge/"));
    let tmpfiles =
        std::fs::read_to_string(repo_root().join("distro/cortex/archie-cortex.tmpfiles")).unwrap();
    let acl = format!("u:{user}:--x");
    assert!(
        tmpfiles.lines().any(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            fields.len() == 7 && fields[0] == "a+" && fields[1] == parent.path && fields[6] == acl
        }),
        "nested graph/state need a traverse-only ACL on the private Core parent"
    );
    let sysusers =
        std::fs::read_to_string(repo_root().join("distro/cortex/archie-cortex.sysusers")).unwrap();
    assert!(!sysusers.lines().any(|line| {
        let fields: Vec<_> = line.split_whitespace().collect();
        fields.len() >= 3 && fields[0] == "m" && fields[1] == user && fields[2] == parent.group
    }));
}

/// The account name sysusers.d allocates.
fn sysusers_account() -> String {
    read("airootfs/usr/lib/sysusers.d/archie.conf")
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("u "))
        .and_then(|line| line.split_whitespace().nth(1).map(str::to_string))
        .expect("sysusers.d must declare a `u` user line")
}

// --------------------------------------------------------------------- the regression

#[test]
fn the_daemon_can_read_its_own_configuration() {
    // THE 0.1.0 BOOT REGRESSION.
    //
    // `z` adjusts exactly one path and is NOT recursive, so an entry for the directory
    // does not reach the file inside it. Without an explicit entry the config keeps its
    // image ownership (root:root 0640), group `archie` falls through to "other", and the
    // daemon dies with EACCES before it can announce anything.
    let unit = unit_directives(&read("airootfs/etc/systemd/system/archied.service"));
    let group = unit
        .get("Group")
        .expect("archied.service must pin an explicit Group");

    let config = entry_for("/etc/archie/archie.toml").expect(
        "/etc/archie/archie.toml needs its OWN tmpfiles.d entry. An entry for /etc/archie \
         is not enough: `z` is not recursive, so the file keeps root:root and the daemon \
         cannot read it.",
    );

    assert_eq!(
        &config.group, group,
        "the config file's group must match archied.service Group={group}, or the daemon \
         gets EACCES on its own configuration"
    );

    // Group must actually have the read bit. 0640 grants it; 0600 would not.
    let group_digit = config
        .mode
        .chars()
        .nth(2)
        .and_then(|c| c.to_digit(8))
        .unwrap_or_else(|| panic!("unreadable tmpfiles mode {:?}", config.mode));
    assert!(
        group_digit & 0b100 != 0,
        "mode {} does not grant group read on the config file",
        config.mode
    );
}

#[test]
fn the_config_directory_is_traversable_by_the_daemon_group() {
    // Group read on the file is useless if the group cannot traverse the directory.
    let directory = entry_for("/etc/archie").expect("/etc/archie needs a tmpfiles.d entry");
    assert_eq!(directory.group, "archie");

    let group_digit = directory
        .mode
        .chars()
        .nth(2)
        .and_then(|c| c.to_digit(8))
        .expect("unreadable mode");
    assert!(
        group_digit & 0b001 != 0,
        "mode {} does not grant group execute (traverse) on /etc/archie",
        directory.mode
    );
}

// ------------------------------------------------------------------ unit/account seam

#[test]
fn the_service_account_is_the_one_sysusers_creates() {
    // A typo here produces a unit that can never start, and systemd reports it only as
    // a generic failure to change user.
    let account = sysusers_account();
    let unit = unit_directives(&read("airootfs/etc/systemd/system/archied.service"));
    assert_eq!(unit.get("User").map(String::as_str), Some(account.as_str()));
    assert_eq!(
        unit.get("Group").map(String::as_str),
        Some(account.as_str())
    );
}

#[test]
fn the_daemon_never_runs_as_root() {
    let unit = unit_directives(&read("airootfs/etc/systemd/system/archied.service"));
    let user = unit.get("User").map(String::as_str).unwrap_or("root");
    assert_ne!(
        user, "root",
        "the control plane must not run with blanket root"
    );
    assert_eq!(
        unit.get("CapabilityBoundingSet").map(String::as_str),
        Some(""),
        "archied needs no capabilities; the bounding set must be empty"
    );
    assert_eq!(unit.get("NoNewPrivileges").map(String::as_str), Some("yes"));
}

#[test]
fn every_writable_runtime_path_is_provisioned_for_the_daemon() {
    // These are the paths `StatePaths` uses. If one is missing from tmpfiles.d the
    // daemon fails at a different, later, and much more confusing point.
    let account = sysusers_account();
    for path in ["/var/lib/archie", "/var/log/archie", "/run/archie"] {
        let entry = entry_for(path)
            .unwrap_or_else(|| panic!("{path} is written by archied but has no tmpfiles.d entry"));
        assert_eq!(
            entry.user, account,
            "{path} must be owned by the daemon account"
        );
        assert_eq!(entry.kind, "d", "{path} must be created as a directory");
    }
}

#[test]
fn the_unit_refuses_to_restart_loop_on_a_bad_configuration() {
    // archied exits 78 (EX_CONFIG) for unusable configuration. Restarting on that would
    // hammer the journal forever instead of surfacing one clear failure.
    let unit = unit_directives(&read("airootfs/etc/systemd/system/archied.service"));
    assert_eq!(
        unit.get("RestartPreventExitStatus").map(String::as_str),
        Some("78"),
        "EX_CONFIG must not be retried"
    );
}

#[test]
fn the_unit_grants_write_access_to_the_directories_it_needs() {
    // ProtectSystem=strict makes /etc, /usr and /boot read-only. Without the matching
    // *Directory= directives the daemon cannot create its own state.
    let unit = unit_directives(&read("airootfs/etc/systemd/system/archied.service"));
    assert_eq!(
        unit.get("ProtectSystem").map(String::as_str),
        Some("strict")
    );
    for directive in [
        "RuntimeDirectory",
        "StateDirectory",
        "LogsDirectory",
        "ConfigurationDirectory",
    ] {
        assert_eq!(
            unit.get(directive).map(String::as_str),
            Some("archie"),
            "{directive} must be granted, or ProtectSystem=strict blocks the daemon"
        );
    }
}

#[test]
fn the_fabric_address_family_is_permitted_by_the_sandbox() {
    // The fabric binds UDP and the control endpoint binds a Unix socket. A sandbox that
    // omits either makes the daemon fail only once it reaches that boot stage.
    let unit = unit_directives(&read("airootfs/etc/systemd/system/archied.service"));
    let families = unit
        .get("RestrictAddressFamilies")
        .expect("the sandbox must pin address families");
    for family in ["AF_UNIX", "AF_INET"] {
        assert!(
            families.contains(family),
            "RestrictAddressFamilies is missing {family}; the fabric or control socket \
             would fail at runtime"
        );
    }
}

// --------------------------------------------------------------------- image contents

#[test]
fn the_shipped_configuration_parses_and_is_safe_by_default() {
    // The ISO config is what every fresh install boots with, so it must both parse and
    // be conservative: a live image must not assume it has a physical safety boundary.
    let text = std::fs::read_to_string(distro("airootfs/etc/archie/archie.toml"))
        .expect("the ISO must ship a default configuration");
    let config =
        archie_core::config::ArchieConfig::from_toml(&text).expect("shipped config must parse");

    assert!(
        !config.policy.safety_boundary_present,
        "a live image must never claim a verified safety boundary"
    );
    assert!(
        !config.node.profile.may_hold_canonical_state(),
        "the live ISO profile must not be trusted with canonical state"
    );
}

#[test]
fn image_file_permissions_never_grant_world_write() {
    let profiledef = read("profiledef.sh");
    for line in profiledef.lines() {
        let line = line.trim();
        if !line.starts_with("[\"/") {
            continue;
        }
        let mode = line
            .rsplit(':')
            .next()
            .and_then(|m| m.trim_matches(['"', ']', ')', ' ']).parse::<u32>().ok())
            .unwrap_or_else(|| panic!("unparsable file_permissions line: {line}"));

        // The "other" digit is the last one; bit 0b010 is write. Mode 0755 is r-x for
        // other and is correct for a binary — only the write bit is disqualifying.
        let other = mode % 10;
        assert_eq!(
            other & 0b010,
            0,
            "world-writable mode in file_permissions: {line}"
        );
    }
}

#[test]
fn the_archie_binaries_are_installed_executable() {
    let profiledef = read("profiledef.sh");
    for binary in [
        "/usr/bin/archie",
        "/usr/bin/archied",
        "/usr/bin/archie-displayd",
        "/usr/bin/archie-worldd",
        "/usr/bin/archie-display",
    ] {
        assert!(
            profiledef.contains(binary),
            "{binary} must have an explicit file_permissions entry"
        );
    }
}

#[test]
fn the_service_is_enabled_in_the_image() {
    // A unit that is shipped but never enabled produces an ISO that boots to a login
    // prompt with no Archie running — which is exactly what a smoke test would miss if
    // it only checked that the binary exists.
    let preset = read("airootfs/usr/lib/systemd/system-preset/50-archie-os.preset");
    assert!(
        preset.contains("enable archied.service"),
        "archied.service must be enabled by preset"
    );

    let build = std::fs::read_to_string(repo_root().join("distro/archiso/build.sh"))
        .expect("build.sh must exist");
    assert!(
        build.contains("multi-user.target.wants/archied.service"),
        "build.sh must link archied.service into multi-user.target.wants"
    );
}

#[test]
fn the_build_script_refuses_to_ship_development_content() {
    // The operator asked that bitácora, legacy sources and test material never reach the
    // exportable image.
    let build = std::fs::read_to_string(repo_root().join("distro/archiso/build.sh"))
        .expect("build.sh must exist");
    for forbidden in ["_legacy", "secrets", "target", ".git"] {
        assert!(
            build.contains(forbidden),
            "build.sh must reject {forbidden} from the release image"
        );
    }
}

#[test]
fn the_configuration_directory_is_not_writable_by_the_daemon() {
    // The daemon reads its policy; it must not be able to rewrite it. This is also the
    // constraint that `StatePaths::ensure_directories` has to respect: it may not
    // require write access to /etc/archie, or a correctly-hardened node fails to boot.
    let directory = entry_for("/etc/archie").expect("/etc/archie needs a tmpfiles.d entry");
    let group_digit = directory
        .mode
        .chars()
        .nth(2)
        .and_then(|c| c.to_digit(8))
        .expect("unreadable mode");
    assert_eq!(
        group_digit & 0b010,
        0,
        "mode {} makes the config directory writable by the daemon group; the daemon \
         must not be able to rewrite its own policy",
        directory.mode
    );
    assert_eq!(
        directory.user, "root",
        "configuration is operator-owned, not daemon-owned"
    );
}

#[test]
fn the_image_ships_the_module_manifests() {
    // Without these the ISO boots with an empty cognitive topology, which looks
    // identical to a broken module system.
    let build = std::fs::read_to_string(repo_root().join("distro/archiso/build.sh"))
        .expect("build.sh must exist");
    assert!(
        build.contains("etc/archie/modules"),
        "build.sh must stage the module manifests into the image"
    );

    let modules = repo_root().join("modules");
    let count = walk_toml(&modules);
    assert!(
        count >= 10,
        "expected the canonical module set in modules/, found {count}"
    );
}

/// Count `*.toml` files under a directory tree.
fn walk_toml(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                walk_toml(&path)
            } else if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                1
            } else {
                0
            }
        })
        .sum()
}

// ─────────────────────────────────────────────────── the observatory session

fn display_unit(name: &str) -> BTreeMap<String, String> {
    let path = repo_root().join("distro/display").join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    unit_directives(&text)
}

#[test]
fn the_renderer_cannot_reach_the_control_socket() {
    // The whole isolation argument in RFC-006 rests on this. archie-displayd is a member
    // of the `archie` group so it can open archied's group-gated control socket. The
    // native renderer must NOT be, or the GPU process inherits the reach of the CLI.
    let bridge = display_unit("archie-displayd.service");
    let renderer = display_unit("archie-display.service");

    assert_eq!(
        bridge.get("SupplementaryGroups").map(String::as_str),
        Some("archie"),
        "the bridge needs the archie group to open the control socket"
    );
    assert!(
        !renderer.contains_key("SupplementaryGroups"),
        "the renderer must not be granted any supplementary group; it reaches Archie \
         only through the bridge's loopback endpoint"
    );
}

#[test]
fn neither_display_unit_runs_as_root_or_as_the_daemon_account() {
    for name in ["archie-displayd.service", "archie-display.service"] {
        let unit = display_unit(name);
        let user = unit.get("User").map(String::as_str).unwrap_or("root");
        assert_eq!(
            user, "archie-display",
            "{name} must run as the unprivileged observatory account"
        );
        assert_ne!(user, "root");
        assert_ne!(
            user, "archie",
            "{name} must not run as the control-plane account"
        );
    }
}

#[test]
fn the_observatory_is_denied_the_network() {
    // Everything the renderer draws comes from the loopback bridge. Denying egress means
    // a compromised graphics client has nowhere to send what it can see.
    for name in ["archie-displayd.service", "archie-display.service"] {
        let unit = display_unit(name);
        assert_eq!(
            unit.get("IPAddressDeny").map(String::as_str),
            Some("any"),
            "{name} must deny outbound network"
        );
        assert_eq!(
            unit.get("IPAddressAllow").map(String::as_str),
            Some("localhost"),
            "{name} must allow only loopback"
        );
    }
}

#[test]
fn only_the_world_adapter_has_egress_and_it_cannot_reach_archie_state() {
    let adapter = display_unit("archie-worldd.service");
    assert_eq!(adapter.get("DynamicUser").map(String::as_str), Some("yes"));
    assert!(
        !adapter.contains_key("User") && !adapter.contains_key("SupplementaryGroups"),
        "worldd must not inherit the display or archie accounts"
    );
    let families = adapter
        .get("RestrictAddressFamilies")
        .expect("worldd must pin address families");
    assert!(families.contains("AF_INET"));
    assert!(
        !families.contains("AF_UNIX"),
        "worldd must be structurally unable to open archied's Unix control socket"
    );
    let hidden = adapter
        .get("InaccessiblePaths")
        .expect("Archie paths must be invisible to worldd");
    for path in [
        "/etc/archie",
        "/run/archie",
        "/var/lib/archie",
        "/var/log/archie",
    ] {
        assert!(hidden.contains(path), "worldd can still see {path}");
    }
    assert_eq!(
        adapter.get("CapabilityBoundingSet").map(String::as_str),
        Some("")
    );

    let display_manifest =
        std::fs::read_to_string(repo_root().join("core/crates/archie-displayd/Cargo.toml"))
            .expect("display manifest must exist");
    assert!(
        !display_manifest
            .lines()
            .any(|line| line.trim_start().starts_with("reqwest")),
        "archie-displayd must not contain an Internet HTTP client"
    );
}

#[test]
fn the_bridge_sandbox_permits_exactly_what_it_needs() {
    let bridge = display_unit("archie-displayd.service");
    let families = bridge
        .get("RestrictAddressFamilies")
        .expect("the bridge must pin address families");
    // AF_UNIX reaches archied; AF_INET is the loopback listener.
    assert!(families.contains("AF_UNIX"));
    assert!(families.contains("AF_INET"));
    assert_eq!(
        bridge.get("CapabilityBoundingSet").map(String::as_str),
        Some("")
    );
    assert_eq!(
        bridge.get("ProtectSystem").map(String::as_str),
        Some("strict")
    );
}

#[test]
fn the_display_does_not_restart_loop_on_a_headless_node() {
    // Restart=always plus no GPU is an infinite loop that fills the journal. The
    // condition makes the unit simply not start where there is nothing to draw on.
    let renderer = display_unit("archie-display.service");
    assert_eq!(renderer.get("Restart").map(String::as_str), Some("always"));
    assert!(
        renderer
            .get("ConditionPathExists")
            .is_some_and(|c| c.contains("/dev/dri/")),
        "the display unit needs a GPU condition, or it restart-loops on a headless node"
    );
}

#[test]
fn a_renderer_that_keeps_dying_stops_instead_of_looping_silently() {
    // The GPU condition covers a headless node. It does nothing about a renderer that
    // starts, has a GPU and dies anyway — an unusable driver, a shader that will not
    // compile, a memory ceiling it cannot fit under.
    //
    // Restart=always with RestartSec=3s retries that forever unless a limit stops it, and
    // systemd's default (5 starts / 10s) never fires here: five attempts three seconds
    // apart span fifteen seconds. The failure would stay invisible in the journal instead
    // of becoming a state `systemctl status` reports.
    let renderer = display_unit("archie-display.service");

    let burst: u32 = renderer
        .get("StartLimitBurst")
        .expect("the display needs a start limit, or a crash loop is silent")
        .parse()
        .expect("StartLimitBurst must be a number");
    let interval: u32 = renderer
        .get("StartLimitIntervalSec")
        .expect("a burst without an interval is not a limit")
        .parse()
        .expect("StartLimitIntervalSec must be seconds");
    let restart_sec: u32 = renderer
        .get("RestartSec")
        .map(|value| value.trim_end_matches('s').parse().unwrap_or(0))
        .unwrap_or(0);

    // The limit only means something if the retries actually fit inside the window.
    assert!(
        burst * restart_sec < interval,
        "{burst} restarts {restart_sec}s apart span {}s, which exceeds the {interval}s          window — the limit can never trigger and the loop is unbounded",
        burst * restart_sec
    );
    assert!(
        burst >= 2,
        "one attempt makes a transient failure permanent"
    );
}

#[test]
fn the_display_memory_ceiling_clears_a_4k_framebuffer() {
    // The globe texture does not size this: 2048x1024 RGBA is 8 MB, ~11 MB with mipmaps.
    // The framebuffers do, and they scale with the panel. A triple-buffered 4K surface is
    // ~100 MB before Quick3D's own depth and render targets, and a proprietary NVIDIA
    // userspace maps considerably more than Mesa. A ceiling that fits 1080p-on-Mesa and
    // not 4K-on-NVIDIA fails on exactly the machine this is for.
    let renderer = display_unit("archie-display.service");
    let mebibytes = |key: &str| -> u64 {
        renderer
            .get(key)
            .unwrap_or_else(|| panic!("{key} must be declared"))
            .trim_end_matches('M')
            .parse()
            .unwrap_or_else(|_| panic!("{key} must be plain mebibytes"))
    };

    let high = mebibytes("MemoryHigh");
    let max = mebibytes("MemoryMax");

    const TRIPLE_BUFFERED_4K_MIB: u64 = 3840 * 2160 * 4 * 3 / (1024 * 1024);
    assert!(
        max >= TRIPLE_BUFFERED_4K_MIB * 4,
        "MemoryMax={max}M leaves no room around a {TRIPLE_BUFFERED_4K_MIB}M 4K          framebuffer set plus driver userspace"
    );
    assert!(
        high < max,
        "MemoryHigh must throttle before MemoryMax kills, or reclaim never gets a chance"
    );
}

#[test]
fn the_native_drm_client_gets_a_dedicated_controlling_vt() {
    let renderer = display_unit("archie-display.service");
    assert_eq!(
        renderer.get("TTYPath").map(String::as_str),
        Some("/dev/tty2")
    );
    assert_eq!(
        renderer.get("StandardInput").map(String::as_str),
        Some("tty-force"),
        "the EGLFS/KMS client needs a controlling VT for logind/seat ownership"
    );
    assert!(
        renderer
            .get("Conflicts")
            .is_some_and(|value| value.contains("getty@tty2.service")),
        "the kiosk and a getty must never race for tty2"
    );
}

#[test]
fn the_display_stack_is_built_staged_and_enabled_in_the_image() {
    let build = std::fs::read_to_string(repo_root().join("distro/archiso/build.sh"))
        .expect("build.sh must exist");
    for required in [
        "-p archie-displayd",
        "-p archie-worldd",
        "display/native",
        "cmake --build",
        "release/archie-displayd",
        "release/archie-worldd",
        "usr/bin/archie-display",
        "distro/display/archie-displayd.service",
        "distro/display/archie-worldd.service",
        "distro/display/archie-display.service",
        "distro/display/archie-display.sysusers",
        "multi-user.target.wants/archie-displayd.service",
        "multi-user.target.wants/archie-worldd.service",
        "graphical.target.wants/archie-display.service",
        "systemd/system/default.target",
    ] {
        assert!(
            build.contains(required),
            "build.sh does not stage or enable required display component {required}"
        );
    }

    let preset = read("airootfs/usr/lib/systemd/system-preset/50-archie-os.preset");
    for service in [
        "archie-worldd.service",
        "archie-displayd.service",
        "archie-display.service",
    ] {
        assert!(
            preset.contains(&format!("enable {service}")),
            "the image preset must enable {service}"
        );
    }
}

#[test]
fn the_display_package_allowlist_is_minimal_and_complete() {
    let packages: BTreeSet<_> = read("packages.x86_64")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect();

    for package in ["mesa", "qt6-base", "qt6-declarative", "qt6-quick3d"] {
        assert!(
            packages.contains(package),
            "the observatory needs the allowlisted Arch package {package}"
        );
    }
    for forbidden in ["cage", "chromium", "electron"] {
        assert!(
            !packages.contains(forbidden),
            "the native observatory must not ship the web runtime {forbidden}"
        );
    }
    for desktop in [
        "gnome",
        "gnome-shell",
        "plasma-meta",
        "sway",
        "weston",
        "xorg-server",
    ] {
        assert!(
            !packages.contains(desktop),
            "{desktop} expands the kiosk into a desktop environment"
        );
    }
}

#[test]
fn the_live_installer_is_explicit_and_does_not_replace_the_observatory() {
    let root = repo_root();
    let entry = std::fs::read_to_string(
        root.join("distro/archiso/efiboot/loader/entries/02-archieos-install.conf"),
    )
    .unwrap();
    assert!(entry.contains("archie.install=1"));
    let service =
        std::fs::read_to_string(root.join("distro/installer/archie-installer.service")).unwrap();
    assert!(service.contains("ConditionPathExists=/run/archiso"));
    assert!(service.contains("ConditionKernelCommandLine=archie.install=1"));
    let display =
        std::fs::read_to_string(root.join("distro/display/archie-display.service")).unwrap();
    assert!(display.contains("ConditionKernelCommandLine=!archie.install=1"));
    let finalizer =
        std::fs::read_to_string(root.join("distro/installer/finalize-target.sh")).unwrap();
    assert!(finalizer.contains("/etc/archie/install-target-device"));
    assert!(finalizer.contains("pacman -R --noconfirm"));
    assert!(finalizer.contains("root=PARTUUID="));
    assert!(finalizer.contains("usermod --lock root"));
    let build = std::fs::read_to_string(root.join("distro/archiso/build.sh")).unwrap();
    for required in [
        "distro/installer/packages.x86_64",
        "release/archie-map-fetch",
        "installer/archie-wizard",
        "distro/installer/finalize-target.sh",
        "distro/installer/archie-installer.service",
    ] {
        assert!(
            build.contains(required),
            "installer image missing {required}"
        );
    }
}

#[test]
fn the_renderer_is_a_native_compiled_gpu_client_not_a_web_app() {
    let renderer = display_unit("archie-display.service");
    let exec = renderer
        .get("ExecStart")
        .map(String::as_str)
        .unwrap_or_default();
    let mut words = exec.split_whitespace();
    assert_eq!(words.next(), Some("/usr/bin/archie-display"));
    // The native shell, fed by the read-only bridge the image ships; no other argument.
    assert_eq!(
        words.collect::<Vec<_>>(),
        ["--shell", "--shell-bridge", "/usr/bin/archie-shell-bridge"]
    );

    let native = repo_root().join("display/native");
    assert!(native.join("CMakeLists.txt").is_file());
    assert!(native.join("src/main.cpp").is_file());
    assert!(native.join("qml/Main.qml").is_file());
    assert!(native.join("assets/earth-blue-marble.png").is_file());
    assert!(!repo_root().join("display/index.html").exists());

    let unit_text =
        std::fs::read_to_string(repo_root().join("distro/display/archie-display.service"))
            .expect("native display unit exists");
    for required in [
        "Environment=QT_QPA_PLATFORM=eglfs",
        "Environment=QT_QPA_EGLFS_INTEGRATION=eglfs_kms",
        "Environment=QML_DISABLE_DISK_CACHE=1",
    ] {
        assert!(
            unit_text.contains(required),
            "native display unit is missing {required}"
        );
    }
    for forbidden in ["chromium", "cage", "http://", "https://"] {
        assert!(
            !unit_text.to_ascii_lowercase().contains(forbidden),
            "native display unit still references web/compositor component {forbidden}"
        );
    }
}

/// The Observatory's view: the window `Main.qml` opens and the content it hosts, which the
/// desktop embeds as well.
fn observatory_qml() -> String {
    ["Main.qml", "ObservatoryContent.qml"]
        .iter()
        .map(|name| {
            std::fs::read_to_string(repo_root().join("display/native/qml").join(name))
                .expect("native QML exists")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn native_motion_has_a_real_trigger_and_no_synthetic_loading_loop() {
    let qml = observatory_qml();
    assert!(qml.contains("onSnapshotRevisionChanged() { realEventPulse.restart() }"));
    assert!(qml.contains("visible: archieBridge.busy"));
    assert!(qml.contains("height: archieBridge.busy ? 2 : 0"));
    for forbidden in [
        "BusyIndicator",
        "running: true",
        "loops: Animation.Infinite",
    ] {
        assert!(
            !qml.contains(forbidden),
            "native display contains synthetic activity: {forbidden}"
        );
    }

    // The machine's own condition is the other real trigger. `kernelRevision` moves only
    // when a verdict changes, never when a pressure number merely drifts, so a pulse on
    // it marks the machine actually changing state rather than the clock advancing.
    assert!(
        qml.contains("onKernelRevisionChanged() { realEventPulse.restart() }"),
        "a change of machine condition is a real event and should read as one"
    );
}

#[test]
fn the_draw_budget_changes_the_drawing_rather_than_describing_it() {
    // Reading pressure and then rendering identically would make the budget a caption.
    // The observatory is the largest discretionary consumer on the node, so a saturated
    // machine has to actually get its GPU back.
    let qml = observatory_qml();

    assert!(
        qml.contains("antialiasingMode: archieBridge.frame.msaaSamples"),
        "the budget must reach the scene environment, not just a label"
    );
    assert!(
        qml.contains("antialiasingQuality: archieBridge.frame.msaaSamples"),
        "quality must follow the budget"
    );

    // It reaches the scene as the model's own sample count, in the renderer's units.
    // It used to arrive as a budget word the bridge derived from the kernel verdicts —
    // a second reading of the telemetry the governor had already read, free to disagree
    // with the tier actually in force, and mapping to a `SceneEnvironment.Low` that does
    // not exist in Qt. A display that re-decides this is the failure, not the wiring.
    assert!(
        !qml.contains("displayBudget"),
        "the display must not derive a budget of its own beside the governor's tier"
    );

    // And a scene that has quietly got worse has to say why, or its operator learns to
    // distrust it.
    assert!(
        qml.contains("archieBridge.frame.tierReason"),
        "a degraded tier must carry its reason to the screen"
    );
}

#[test]
fn unmeasured_and_idle_never_render_the_same() {
    // A gauge reading zero and a gauge that cannot read must look different. One says the
    // machine is quiet; the other says nothing is being measured, and an operator acts
    // differently on each.
    let qml = observatory_qml();

    assert!(
        qml.contains("pressureKnown") && qml.contains("thermalKnown"),
        "availability must be carried into the view, not collapsed into a number"
    );
    assert!(
        qml.contains(r#"value: root.pressureKnown"#) && qml.contains(r#""not measured""#),
        "unmeasured pressure needs its own wording, not a zero"
    );
    assert!(
        qml.contains("known: modelData.headroom_fraction !== null"),
        "a zone with no critical trip has no scale, and its bar must refuse to draw"
    );
}

#[test]
fn every_binary_the_build_installs_is_one_the_build_allows() {
    // build.sh keeps two independent lists: what it installs into /usr/bin, and what it
    // permits to be there. They exist for different reasons — the second is the guard
    // that stops a binary nobody decided to ship arriving by accident — and nothing
    // except this test keeps them in step.
    //
    // Updating one and not the other is exactly the mistake that reached CI: the ISO
    // build ran for six minutes, compiled everything, and then refused at the very last
    // check with "unexpected staged Archie binary".
    let script = std::fs::read_to_string(repo_root().join("distro/archiso/build.sh"))
        .expect("the archiso build script exists");

    let installed: BTreeSet<String> = script
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            // Read from the destination, not the source. The first version of this looked
            // only at `${cargo_target}/release/...`, so every CMake-built binary — the
            // observatory, cortex, and the person graph — reached /usr/bin without the
            // allowlist ever being consulted. What matters is what ends up staged.
            let (_, rest) = line.split_once("\"${stage}/airootfs/usr/bin/")?;
            let name = rest.split('"').next()?;
            Some(name.to_string())
        })
        .collect();
    assert!(
        !installed.is_empty(),
        "the parse found nothing, so this test is asserting about an empty set"
    );

    let allowed: BTreeSet<String> = script
        .lines()
        .filter(|line| line.contains("! -name"))
        .flat_map(|line| {
            line.split("! -name")
                .skip(1)
                .filter_map(|piece| piece.split_whitespace().next())
                .map(|name| name.to_string())
                .collect::<Vec<_>>()
        })
        .collect();

    for binary in &installed {
        assert!(
            allowed.contains(binary),
            "build.sh installs {binary} but its staged-binary guard does not allow it; \
             the ISO build would refuse after compiling everything"
        );
    }
}

#[test]
fn release_builder_stages_model_runtime_and_provider_registry() {
    let root = repo_root();
    let script = std::fs::read_to_string(root.join("distro/archiso/build.sh")).unwrap();
    for required in [
        "-p archie-model-runtime",
        "release/archie-infer",
        "providers/data",
        "airootfs/etc/archie/providers",
    ] {
        assert!(script.contains(required), "release omits {required}");
    }

    let provider_count = count_files_with_extension_recursive(&root.join("providers/data"), "toml");
    assert!(provider_count > 0, "the staged provider registry is empty");
}

/// The router chooses among the models whose manifests the image carries. An image that
/// staged none would boot with nothing able to see a screen.
#[test]
fn release_builder_stages_the_model_manifests() {
    let root = repo_root();
    let script = std::fs::read_to_string(root.join("distro/archiso/build.sh")).unwrap();
    assert!(
        script.contains("airootfs/etc/archie/models"),
        "release omits the model manifests"
    );
    let model_count = count_files_with_extension_recursive(&root.join("models"), "toml");
    assert!(model_count > 0, "the staged model registry is empty");
}

#[test]
fn nvidia_iso_profile_is_explicit_and_uses_repository_packages() {
    let root = repo_root();
    let script = std::fs::read_to_string(root.join("distro/archiso/build.sh")).unwrap();
    assert!(script.contains("ARCHIE_NVIDIA_PROFILE"));
    assert!(script.contains("distro/nvidia/packages.x86_64"));

    let packages = std::fs::read_to_string(root.join("distro/nvidia/packages.x86_64")).unwrap();
    for required in ["nvidia-open", "nvidia-utils", "nvidia-container-toolkit"] {
        assert!(packages.lines().any(|line| line == required));
    }
    for forbidden in ["cuda", "cudnn", "model-weights", "nim-container"] {
        assert!(
            !packages.lines().any(|line| line == forbidden),
            "the image must not silently embed {forbidden}"
        );
    }
}

#[test]
fn the_executor_is_delegated_a_subtree_rather_than_the_whole_hierarchy() {
    // The entire reason the executor is a separate unit. archied keeps
    // ProtectControlGroups= and an empty capability bounding set; the access a worker
    // supervisor needs is confined here, and confined further to one subtree.
    let unit = unit_directives(&read(
        "airootfs/etc/systemd/system/archie-executord.service",
    ));

    let delegate = unit
        .get("Delegate")
        .expect("the executor needs a delegated cgroup subtree, or budgets are advisory");
    for controller in ["cpu", "io", "memory", "pids"] {
        assert!(
            delegate.contains(controller),
            "Delegate= must name {controller}; an unnamed set arrives with a systemd              upgrade rather than a deliberate edit"
        );
    }

    // The blanket denial would defeat the narrow grant: with both, the unit cannot write
    // the subtree it was given, which is the one thing it exists to do.
    assert!(
        !unit.contains_key("ProtectControlGroups"),
        "ProtectControlGroups= would make Delegate= useless"
    );
}

#[test]
fn the_executor_cannot_reach_the_network_at_all() {
    // This process can create cgroups and spawn workers. Admitting AF_INET would turn a
    // local privilege boundary into a network-reachable one.
    let unit = unit_directives(&read(
        "airootfs/etc/systemd/system/archie-executord.service",
    ));

    assert_eq!(
        unit.get("RestrictAddressFamilies").map(String::as_str),
        Some("AF_UNIX"),
        "the executor talks to archied over a local socket and has no other correspondent"
    );
    assert_eq!(
        unit.get("CapabilityBoundingSet").map(String::as_str),
        Some("")
    );
    assert_eq!(unit.get("NoNewPrivileges").map(String::as_str), Some("yes"));
}

#[test]
fn the_executor_does_not_impose_its_own_memory_protections_on_workers() {
    // MemoryDenyWriteExecute is right for archied and wrong here: it is inherited by
    // every worker this process execs, and a worker running a JIT — any inference runtime
    // generating kernels — would die on its first compilation with an error pointing
    // nowhere near the cause. Worker confinement is per task, not inherited wholesale.
    let unit = unit_directives(&read(
        "airootfs/etc/systemd/system/archie-executord.service",
    ));
    assert!(
        !unit.contains_key("MemoryDenyWriteExecute"),
        "this would be inherited by every worker and break any JIT among them"
    );
}

#[test]
fn the_executor_runs_as_its_own_account_and_shares_only_a_group() {
    // It must not be able to read the node's private key. The shared group is what lets
    // the two processes share a socket by filesystem permission, which is the whole of
    // the authentication between them.
    let unit = unit_directives(&read(
        "airootfs/etc/systemd/system/archie-executord.service",
    ));
    assert_eq!(
        unit.get("User").map(String::as_str),
        Some("archie-executor")
    );
    assert_ne!(
        unit.get("User").map(String::as_str),
        Some("root"),
        "nothing here needs root"
    );
    assert_eq!(unit.get("Group").map(String::as_str), Some("archie"));
}

#[test]
fn the_executor_does_not_require_archied_to_run() {
    // A supervisor that refused to start without its control plane would drop every
    // surviving worker during an archied restart — exactly the case its reconciliation
    // exists to handle.
    let unit = unit_directives(&read(
        "airootfs/etc/systemd/system/archie-executord.service",
    ));
    assert!(
        !unit.contains_key("Requires"),
        "Wants=, not Requires=: the executor outlives its control plane on purpose"
    );
    assert!(unit
        .get("Wants")
        .is_some_and(|value| value.contains("archied.service")));
}

#[test]
fn the_display_account_is_dedicated_and_has_no_daemon_membership() {
    let path = repo_root().join("distro/display/archie-display.sysusers");
    let sysusers = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let records: Vec<_> = sysusers
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();

    assert!(
        records.iter().any(|line| {
            line.starts_with("u archie-display ") && line.ends_with("/usr/bin/nologin")
        }),
        "sysusers must create a non-login archie-display account"
    );
    for device_group in ["video", "render", "input"] {
        assert!(
            records
                .iter()
                .any(|line| *line == format!("m archie-display {device_group}")),
            "the kiosk account needs the {device_group} device group"
        );
    }
    assert!(
        !records.contains(&"m archie-display archie"),
        "the account itself must not inherit control-plane access; only the bridge \
         process receives SupplementaryGroups=archie"
    );
}

#[test]
fn display_failure_cannot_block_archied_or_the_boot_target() {
    let daemon_text = read("airootfs/etc/systemd/system/archied.service");
    let daemon = unit_directives(&daemon_text);
    assert_eq!(
        daemon.get("WantedBy").map(String::as_str),
        Some("multi-user.target")
    );
    for forbidden in [
        "archie-display.service",
        "archie-displayd.service",
        "archie-worldd.service",
        "graphical.target",
    ] {
        assert!(
            !daemon_text.contains(forbidden),
            "archied must not depend on the optional display stack ({forbidden})"
        );
    }

    let bridge = display_unit("archie-displayd.service");
    let renderer = display_unit("archie-display.service");
    assert!(
        bridge
            .get("Wants")
            .is_some_and(|value| value.contains("archied.service")),
        "the bridge may start archied, but must not require it"
    );
    assert!(
        renderer
            .get("Wants")
            .is_some_and(|value| value.contains("archie-displayd.service")),
        "the renderer should request its bridge"
    );
    assert_eq!(
        renderer.get("WantedBy").map(String::as_str),
        Some("graphical.target")
    );
    for (name, unit) in [("bridge", bridge), ("renderer", renderer)] {
        for hard_edge in ["Requires", "Requisite", "BindsTo", "PartOf"] {
            assert!(
                !unit.contains_key(hard_edge),
                "the display {name} must not use {hard_edge}; its failure is fail-open"
            );
        }
    }

    let readme = std::fs::read_to_string(repo_root().join("distro/display/README.md"))
        .expect("the session must document its hardware verification status");
    assert!(
        readme.to_ascii_lowercase().contains("hardware-unverified"),
        "distro/display/README.md must not pretend the native display was tested on hardware"
    );
}

/// The traffic audit's unit, its account and the ISO staging agree, and the audit's
/// files can reach the alert follower (group `archie`) and never anyone else.
#[test]
fn the_netwatch_unit_runs_as_its_static_account_with_the_capture_group() {
    let root = repo_root();
    let unit_text = std::fs::read_to_string(root.join("distro/sentry/archie-netwatch.service"))
        .expect("the netwatch unit exists");
    let unit = unit_directives(&unit_text);
    // A DynamicUser's state lives under /var/lib/private (0700 root): the follower,
    // running as `archie`, could never read alerts.jsonl there.
    assert!(
        !unit.contains_key("DynamicUser"),
        "netwatch must not run as a DynamicUser"
    );
    let user = unit.get("User").expect("netwatch names its account");
    assert_eq!(unit.get("Group").map(String::as_str), Some("archie"));
    // dumpcap is root:wireshark 0754 on Arch: no group, no capture.
    assert!(unit
        .get("SupplementaryGroups")
        .is_some_and(|groups| groups.split_whitespace().any(|g| g == "wireshark")));
    assert_eq!(unit.get("UMask").map(String::as_str), Some("0027"));
    assert_eq!(
        unit.get("StateDirectoryMode").map(String::as_str),
        Some("0750")
    );
    assert_eq!(
        unit.get("CapabilityBoundingSet").map(String::as_str),
        Some("CAP_NET_RAW CAP_NET_ADMIN")
    );

    let sysusers = std::fs::read_to_string(root.join("distro/sentry/archie-netwatch.sysusers"))
        .expect("the netwatch account is declared");
    let records: Vec<Vec<&str>> = sysusers
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.split_whitespace().collect())
        .collect();
    assert!(
        records.iter().any(|r| r.first() == Some(&"u")
            && r.get(1) == Some(&user.as_str())
            && r.last() == Some(&"/usr/bin/nologin")),
        "sysusers must create the non-login {user} account"
    );

    // The follower reads what netwatch writes, as the control plane's account, with
    // no network and no capabilities.
    let follow = unit_directives(
        &std::fs::read_to_string(root.join("distro/sentry/archie-sentry-follow.service"))
            .expect("the follower unit exists"),
    );
    assert_eq!(follow.get("User").map(String::as_str), Some("archie"));
    assert_eq!(follow.get("Group").map(String::as_str), Some("archie"));
    let state_dir = unit
        .get("ExecStart")
        .and_then(|e| e.split("--state-dir ").nth(1))
        .expect("netwatch names its state directory");
    assert_eq!(
        follow.get("ExecStart").map(String::as_str),
        Some(format!("/usr/bin/archie sentry-follow --state-dir {state_dir}").as_str()),
        "the follower must read the directory netwatch writes"
    );
    assert_eq!(
        follow.get("CapabilityBoundingSet").map(String::as_str),
        Some("")
    );
    assert_eq!(
        follow.get("RestrictAddressFamilies").map(String::as_str),
        Some("AF_UNIX")
    );

    let build = std::fs::read_to_string(root.join("distro/archiso/build.sh")).unwrap();
    assert!(
        build.contains("distro/sentry/archie-sentry-follow.service"),
        "the ISO must stage the follower unit"
    );
    assert!(
        build.contains("distro/sentry/archie-netwatch.sysusers")
            && build.contains("sysusers.d/archie-netwatch.conf"),
        "the ISO must stage the netwatch account with its unit"
    );
}
