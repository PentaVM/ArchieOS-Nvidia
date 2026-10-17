# Router administration SDK (Rust)

`archie_sentry::adapters::router::admin` provides an async, owner-enrolled SSH
session, firmware profiles, privilege evidence and typed telemetry reads. It
reuses Claude's process runner, SSH configuration, router capability probes and
typed Core adapter. This is an SDK and inspection example; adopting its constructor
in a particular daemon/config loader remains that consumer's integration step.

The architecture takes the driver/connection separation from
[Netmiko](https://github.com/ktbyers/netmiko), whose
[MIT-licensed reference revision](https://github.com/ktbyers/netmiko/tree/3e6861cac666d22cf7a137b56ded56af2fc292cb)
was inspected. This is original Rust code, not a full Netmiko port, and it does
not inherit Netmiko's device-support or performance claims. No Python runtime
or new crate dependency is added. Network/SSH latency has not been benchmarked.

## Initial compatibility catalogue

| Firmware/dialect | Identity/privilege | Read templates | Typed changes |
|---|---|---|---|
| OpenWrt | os-release/release markers; numeric UID | ubus board/system/WAN, Linux interfaces/routes/neighbours, dnsmasq leases, sampled conntrack | transient Wi-Fi up/down via existing WifiSet/ubus, with settled multi-radio readback |
| Asuswrt / Merlin | explicit banner marker or owner hint; numeric UID | Linux network reads, dnsmasq lease candidate | existing Broadcom wl/nvram strategy when independently probed |
| FreshTomato | explicit banner marker or owner hint; numeric UID | Linux network reads, dnsmasq lease candidate | existing Broadcom strategy when independently probed |
| DD-WRT | explicit banner marker or owner hint; numeric UID | Linux network reads, dnsmasq lease candidate | existing Broadcom strategy when independently probed |
| VyOS | os-release ID; numeric UID; optional existing sudo | Linux network reads, uptime, sampled conntrack when present | no new VyOS configuration writer |
| OPNsense | explicit marker or owner hint; numeric UID | version, ifconfig, routes, ARP, pf states, uptime | no new configd/REST writer |
| pfSense | platform marker or owner hint; numeric UID | version, ifconfig, routes, ARP, pf states, uptime | no new configuration writer |
| MikroTik RouterOS | native resource output and observed user/group policies | resource/interface/route/ARP/DHCP lease/connection tables | no new RouterOS writer |
| Generic Linux | Linux kernel response; numeric UID | Linux interfaces/routes/neighbours, uptime, sampled conntrack | only independently probed existing strategies |

These are **fixture-tested profiles, not hardware-certified routers**. A template
can fail when its tool/file/version is absent or permission is denied. An owner
hint selects a dialect but is reported unverified unless authenticated markers
agree; conflicting detected firmware fails. Unsupported read kinds fail before
a process is started. OpenWrt across different hardware brands still requires
hardware/version testing: brand, Dropbear and MAC manufacturer are not firmware
or administrator proofs.

## Minimal setup and use

1. Enable SSH through the router's supported LAN administration UI and enroll
   an owner public key. The library does not discover passwords, exploit a locked
   ISP firmware or install another firmware to obtain root.
2. Supply host, account and existing key once. The example defaults its separate
   known-hosts file beside that key. Host identity must be pinned; optional
   `--trust-first-use` explicitly accepts the first observed key and still refuses
   changed keys. First-use trust is not out-of-band identity verification.

From `core/`, first enrollment/inspection:

```sh
cargo run --locked -p archie-sentry --example router-admin -- \
  --host 192.168.1.1 --user root --identity /home/archie/.ssh/router_admin \
  --trust-first-use --read interfaces
```

Subsequent calls omit `--trust-first-use`. A version needing a configured dialect
can add `--firmware openwrt` (or another catalogue identifier). For a dedicated
known-hosts store or custom port use `--known-hosts /private/router_hosts --port 22`.
`--profiles` lists supported profile templates and enrollment guidance **offline**.
No password field or generic remote-command option exists.

```rust,ignore
use archie_sentry::adapters::{process::ProcessRunner, router::{
    admin::{AdminConfig, RouterAdmin, ReadKind}, ssh::SshConfig,
}};
use std::sync::Arc;

let ssh = SshConfig::new("/usr/bin/ssh", "root", "192.168.1.1".parse()?,
    Some("/home/archie/.ssh/router_admin".into()))?;
let cfg = AdminConfig::new(ssh, "/home/archie/.ssh/router_hosts")?;
let session = RouterAdmin::connect(cfg, Arc::new(ProcessRunner::default())).await?;
let access = session.report();
let interfaces = session.read(ReadKind::Interfaces).await?;
// Optional adapter handoff to Core's existing Intent/Policy/lease pipeline:
let adapter = session.core_adapter("local", "router", "gateway",
    "192.168.1.107".parse()?).await?;
```

The library does not persist passwords or read private-key contents into reports.
Keep credentials and session configuration within the caller's personal/work
domain. No service or image is silently replaced, and no active host capture
or Wanda worker is controlled by this SDK.

## Privilege and evidence

- `root` requires an exact successful `id -u` response of `0`, not a username.
- `sudo-root` is available for reads only, and only when the caller explicitly
  enables `allow_sudo_read`/`--sudo-read` and `sudo -n -- id -u` returns `0`.
  This checks pre-existing rights and never edits sudoers or prompts. Other
  commands can still be denied by a narrower sudo rule.
- RouterOS has no POSIX root. `network-admin` means the queried account's group
  explicitly includes `ssh`, `read` and `write` and does not revoke them.
  Other policies remain visible; this is not a claim of unrestricted access.
- `unknown` remains unknown. Profile read lists describe available templates;
  they do not prove each command's availability or authorize an action.
- Each read returns bounded output plus an evidence digest binding endpoint,
  account, dialect, operation, output and observation time. It is not a router
  signature or independently verified complete telemetry.

SSH disables ambient client config, agent authentication, password/keyboard
prompts, local commands and forwarding. Known-host checks default to strict.
Probe budget defaults to 30 seconds; individual commands and output are bounded.
Blocking subprocess waits run outside the async executor. Raw configuration,
secrets, `nvram show`, arbitrary shell execution and firmware flashing are not
part of the SDK. Connection tables can be capped/rejected and conntrack reads
sample at most 128 lines. None is full gateway WAN capture.

Mutations remain the existing typed Sentry/Core surface. Wi-Fi has a measurement
readback; empty, mixed, pending, malformed or oversized OpenWrt radio state is
unknown. `sudo-read` is not propagated to writes. Core pairing, Intent approval,
Policy, leases and verification remain required. No general `execute_root` API
is added.

## Adding firmware and validating real devices

Add a Firmware profile, explicit authenticated markers, named bounded reads,
enrollment guidance and sanitized fixtures. A new writer additionally needs an
existing or explicitly designed typed contract, parameter validation, authority
gating and measurable postconditions; adding a catalogue entry cannot grant it.
Record the hardware model, firmware version, SSH mode, exact outputs, trust mode,
permissions and live test results separately before calling that combination
hardware-tested. Flashing requires a separate device-specific plan and rollback.

Primary sources inspected:

- [Netmiko platforms](https://github.com/ktbyers/netmiko/blob/develop/PLATFORMS.md)
- [OpenWrt ubus](https://openwrt.org/docs/techref/ubus) and
  [official wifi/ubus implementation](https://github.com/openwrt/openwrt/blob/main/package/network/config/wifi-scripts/files/sbin/wifi)
- [RouterOS SSH](https://manual.mikrotik.com/docs/management-tools/ssh/) and
  [user/group policy](https://manual.mikrotik.com/docs/authentication-authorization-accounting/user/)
- [VyOS SSH service](https://docs.vyos.io/en/rolling/configuration/service/ssh.html)
- [OPNsense SSH settings](https://docs.opnsense.org/manual/settingsmenu.html) and
  [API authorization](https://docs.opnsense.org/development/api.html)
- [pfSense SSH access](https://docs.netgate.com/pfsense/en/latest/recipes/ssh-access.html)
- [FreshTomato SSH enrollment](https://wiki.freshtomato.org/doku.php/admin-access)
