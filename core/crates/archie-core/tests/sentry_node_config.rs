//! Configuration validation for the node/router/live-wiring sections added with
//! the alert follower and Wanda watcher: `[sentry.router]`, `[sentry.wol]`,
//! `[sentry.android]`, `[sentry.netwatch]`, `[sentry.chat_exports]`.
//! Validation-only: no network, no devices, no credential files are touched —
//! adapter constructors are probed with the configured values, and every
//! address here is a fixture.
use archie_core::config::ArchieConfig;
use archie_core::control::SentryCommand;
use archie_sentry::contracts::CommandKind;

fn abs(name: &str) -> String {
    if cfg!(windows) {
        format!("C:/archie/{name}")
    } else {
        format!("/etc/archie/{name}")
    }
}

fn config_text() -> String {
    format!(
        "[sentry]\n\
         [sentry.router]\n\
         resource_id='home-router'\n\
         host='192.168.1.1'\n\
         lan_client='192.168.1.2'\n\
         control_url='http://192.168.1.1/igd/control'\n\
         protected_macs=['7c:10:c9:55:aa:02']\n\
         [sentry.router.ssh]\n\
         user='admin'\n\
         identity_file='{key}'\n\
         [sentry.wol]\n\
         broadcast='192.168.1.255'\n\
         arp_tool='{ip_tool}'\n\
         [[sentry.wol.nodes]]\n\
         resource_id='living-pc'\n\
         mac='7c:10:c9:55:aa:01'\n\
         ip='192.168.1.50'\n\
         [sentry.android]\n\
         adb='{adb}'\n\
         [[sentry.android.nodes]]\n\
         resource_id='pixel'\n\
         serial='192.168.1.60:42351'\n\
         network=true\n\
         [sentry.netwatch]\n\
         state_dir='{netwatch}'\n\
         [sentry.response]\n\
         min_severity='high'\n\
         [sentry.chat_exports]\n\
         watch_dir='{exports}'\n\
         salt='fixture-salt-not-a-secret'\n",
        key = abs("router.key"),
        ip_tool = abs("bin/ip"),
        adb = abs("bin/adb"),
        netwatch = abs("netwatch"),
        exports = abs("exports"),
    )
}

#[test]
fn the_node_router_and_live_wiring_sections_are_accepted() {
    let config = ArchieConfig::from_toml(&config_text()).unwrap();
    let sentry = &config.sentry;

    let router = sentry.router.as_ref().unwrap();
    assert_eq!(router.resource_id, "home-router");
    assert_eq!(router.ssh.as_ref().unwrap().user, "admin");

    assert_eq!(
        sentry.actuable_route("living-pc"),
        Some(("wol", vec![CommandKind::WakeHost]))
    );
    assert_eq!(
        sentry.actuable_route("pixel"),
        Some((
            "adb",
            vec![
                CommandKind::AppLaunch,
                CommandKind::SessionLock,
                CommandKind::VolumeSet,
            ]
        ))
    );
    let route = sentry.actuable_route("home-router").unwrap();
    assert_eq!(route.0, "router");
    assert!(route.1.contains(&CommandKind::PortMapping));
    assert!(route.1.contains(&CommandKind::WifiSet));
    assert!(route.1.contains(&CommandKind::DeviceBlock));

    assert_eq!(sentry.netwatch.as_ref().unwrap().state_dir, abs("netwatch"));
    assert_eq!(sentry.response.as_ref().unwrap().min_severity, "high");
    assert_eq!(
        sentry.chat_exports.as_ref().unwrap().watch_dir,
        abs("exports")
    );

    // The router config the adapter would be built from is constructible.
    let (router_config, capabilities) = router.to_router_config().unwrap();
    assert!(capabilities.igd);
    assert!(capabilities.ssh);
    assert!(router_config.control_url.is_some());
    assert!(router_config.ssh.is_some());
    assert_eq!(router_config.protected_macs, vec!["7c:10:c9:55:aa:02"]);
    // Firewalls are asked on the router at the first block, never assumed.
    assert!(capabilities.firewall_tools.is_empty());
}

#[test]
fn malformed_node_sections_are_refused() {
    // A WoL node whose MAC is not six lowercase hex octets.
    let bad_mac = config_text().replace("7c:10:c9:55:aa:01", "7C:10:C9:55:AA:01");
    assert!(ArchieConfig::from_toml(&bad_mac).is_err());

    // An Android node claiming wireless debugging without an <ip>:<port> serial.
    let bad_serial = config_text().replace("192.168.1.60:42351", "usb-serial-1");
    assert!(ArchieConfig::from_toml(&bad_serial).is_err());

    // A router with neither an IGD control URL nor SSH administers nothing.
    let minimal = "[sentry]\n[sentry.router]\nresource_id='r'\nhost='192.168.1.1'\nlan_client='192.168.1.2'\n";
    assert!(ArchieConfig::from_toml(minimal).is_err());

    // A protected address that is not a single lowercase device address.
    for bad in ["7C:10:C9:55:AA:02", "ff:ff:ff:ff:ff:ff", "router"] {
        let text = config_text().replace("7c:10:c9:55:aa:02", bad);
        assert!(ArchieConfig::from_toml(&text).is_err(), "{bad}");
    }

    // An IGD-only router cannot block: DeviceBlock needs the SSH channel.
    let igd_only = config_text().replace(
        &format!(
            "[sentry.router.ssh]\nuser='admin'\nidentity_file='{}'\n",
            abs("router.key")
        ),
        "",
    );
    let igd_only = ArchieConfig::from_toml(&igd_only).unwrap();
    let route = igd_only.sentry.actuable_route("home-router").unwrap();
    assert!(!route.1.contains(&CommandKind::DeviceBlock));

    // A notice floor that is not one of the audit's severities is refused.
    let floor = config_text().replace("min_severity='high'", "min_severity='critical'");
    assert!(ArchieConfig::from_toml(&floor).is_err());

    // A relative watch directory or an empty salt is refused.
    let rel = config_text().replace(&abs("exports"), "relative/exports");
    assert!(ArchieConfig::from_toml(&rel).is_err());
    let empty_salt = config_text().replace("salt='fixture-salt-not-a-secret'", "salt=''");
    assert!(ArchieConfig::from_toml(&empty_salt).is_err());
}

#[test]
fn the_cli_commands_match_the_contract_bounds() {
    // The contracts the CLI verbs map to accept what the CLI accepts.
    assert!(SentryCommand::WakeHost.validate().is_ok());
    assert!(SentryCommand::SessionLock { locked: true }
        .validate()
        .is_ok());
    assert!(SentryCommand::VolumeSet { percent: 100 }.validate().is_ok());
    assert!(SentryCommand::VolumeSet { percent: 101 }
        .validate()
        .is_err());
    assert!(SentryCommand::AppLaunch {
        app_id: "com.example.app".into()
    }
    .validate()
    .is_ok());
    assert!(SentryCommand::DeviceBlock {
        mac: "7c:10:c9:55:aa:03".into(),
        blocked: true
    }
    .validate()
    .is_ok());
    assert!(SentryCommand::DeviceBlock {
        mac: "7C:10:C9:55:AA:03".into(),
        blocked: true
    }
    .validate()
    .is_err());
}
