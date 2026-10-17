# Rust router SDK: verification and handoff

Repository origin verified: `PentaVM/ArchieOS-Nvidia`. Branch
`codex/router-admin-library`, based on the completed PR43 branch at `060d14a`.
No shared Claude/Xavi worktree, GUI, OS image, Core contracts, Policy/leases,
personal/work domain code or Wanda/GPU worker was edited. No router login,
credential guessing, root exploit, firmware flash or live Wi-Fi change occurred.

## Implemented

- Public async Rust SDK under `archie_sentry::adapters::router::admin`, original
  implementation informed by Netmiko driver/connection separation (MIT reference
  revision recorded in the design). No Python runtime or new dependencies.
- Nine initial dialect profiles: OpenWrt, Asuswrt, FreshTomato, DD-WRT, VyOS,
  OPNsense, pfSense, RouterOS and generic Linux. Authenticated markers are
  separated from configured hints. Conflicting markers fail closed.
- Explicit owner key and isolated known-hosts file, strict default trust and
  explicit first-use enrollment. Ambient SSH config/agent, passwords, keyboard
  prompts, local commands and forwarding are disabled. Config is revalidated
  before process creation. A known-hosts/private-key path collision is rejected.
- Exact POSIX UID proof; optional existing noninteractive sudo for reads only;
  RouterOS group-policy evidence including wrapped values and revocations.
  `privilege_scope` says exactly what was checked. No generic root command API.
- Eight typed read kinds with firmware-specific static templates, bounded
  output/deadlines, endpoint/account/timestamp provenance and reproducible digest.
  Declared profile reads remain distinct from measured capability availability.
- Core adapter handoff uses strict trust and fresh existing capability probes;
  sudo-read is not inherited by writes. OpenWrt transient Wi-Fi up/down uses the
  existing typed WifiSet contract and ubus readback. Mixed/pending/malformed radio
  states are unknown rather than an invented verified postcondition.
- Offline profile catalogue and owner-enrolled inspection CLI example. Setup,
  support levels, sources and expansion guide are in
  `docs/SENTRY-ROUTER-ADMIN-SDK.md`.

## Proof

CPU only, existing ArchieOS WSL distro, system Rust/Cargo 1.99.0, locked
dependencies, two build jobs and isolated target cache. This is not a claim that
MSRV 1.88 or another hardware/OS combination was tested here.

TDD observed failures before implementation: missing SDK module; ambiguous
firmware incorrectly falling back to a different dialect; noncanonical `+0`
being accepted as root; missing OpenWrt radio support; missing evidence provenance;
wrapped RouterOS policies being lost; private-key/known-hosts collision accepted.
Each corresponding final test passed.

- `cargo test --locked -p archie-sentry`: **250 passed, 9 ignored, 0 failed**.
  Ignored tests require isolated PostgreSQL/cluster/crash-recovery environments;
  they were not run and do not certify deployment. This includes **15 SDK tests**
  and **28 router adapter tests**.
- `cargo clippy --locked -p archie-sentry --all-targets -- -D warnings`: passed.
- `cargo fmt -p archie-sentry -- --check` and `git diff --check`: passed.
- Built and executed `router-admin --profiles`: valid offline JSON for all nine
  profiles with explicit fixture-only validation labels. No connection attempted.
- Actual installed OpenSSH `ssh -G` parsed the hardening options offline:
  strict host checking true, IdentityAgent none, forwarding cleared, and a
  known-hosts path containing spaces retained correctly. This proves client
  configuration parsing, not server authentication or live router access.
- Independent same-chat read-only review found no blocking defects. Its two
  suggestions were implemented: explicit sudo verification scope and returned
  endpoint/account provenance. A final scoped review found both addressed.

## Limits and adoption

Profiles and OpenWrt actuation have fixture/loopback evidence only. There is no
hardware certification, speed benchmark, live router root verification, complete
WAN capture, REST/NETCONF backend or arbitrary configuration/flash writer. The
initial SSH endpoint type reuses Sentry's private/loopback IPv4 restriction.
Command-specific sudo rights can still deny every other read. Firmware tools,
lease paths and CLI output can vary by version; failed reads remain failures.

This SDK exports an adapter construction hook. A daemon wishing to use the new
connection/profile lifecycle must explicitly adopt it; the existing Core config
loader was not silently replaced. Additional management operations need typed
contracts, authority checks and readback, not just another string in a registry.

The actual owner router has only web-panel access confirmed so far. No existing
Windows/HP password was reused as a router credential. Owner-supported SSH/key
enrollment is still required before a real-device validation can proceed.

Separate operational observation: the earlier Windows host bridge was found
stopped after a Windows control-exit code (`0xC000013A`), with stale analysis
health. It was not stopped by this SDK. A restart attempt awaited Windows UAC
and was cancelled without an approval; capture was not claimed resumed. This
does not affect SDK/fixture test outcomes or authorize an SSH login.
