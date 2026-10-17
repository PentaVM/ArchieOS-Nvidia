# Extensible router administration SDK

Owner request: a Rust library Archie can use to connect to many router firmware
families, establish the administrator access the owner supplied, and grow the
compatibility catalogue. Preserve Core, Policy, leases and existing authority.

Reference: Netmiko driver/connection separation, MIT, develop revision
`3e6861cac666d22cf7a137b56ded56af2fc292cb`. This is an original Rust
implementation, not a wholesale port or a claim of Netmiko platform parity.

Implement under `archie_sentry::adapters::router::admin`, reusing SshConfig,
CommandRunner, ProcessRunner and RouterAdapter. Separate firmware profiles,
session/trust and typed read operations. No replacement GUI, OS image or service.

Initial profiles: OpenWrt, Asuswrt, FreshTomato, DD-WRT, VyOS, OPNsense, pfSense,
RouterOS, and generic Linux fallback. A profile is fixture-tested compatibility,
not hardware certification. Firmware identity needs authenticated evidence;
Dropbear, an SSID, a manufacturer MAC or a configured username proves no root.

Key-only SSH requires an explicit identity and private known-hosts file. Default
is strict pinned trust; first-use trust is an explicit enrollment mode. Disable
ambient SSH configuration, agent, forwarding and local commands. Revalidate
mutable SshConfig fields, bound outputs/deadlines and keep all remote command
templates static except validated RouterOS user/group identifiers.

Direct POSIX root is verified by exact `id -u` == 0. Optional owner-requested
`sudo -n -- id -u` checks existing noninteractive sudo rights; it never acquires
new rights, prompts, guesses passwords or changes sudoers. RouterOS reports
observed user/group policies, not POSIX root. Unknown identity/permissions remain
unknown. Reads are typed: identity, interfaces, routes, neighbours, leases,
connections, system and WAN status. Unsupported reads fail explicitly.

Expose an adapter bridge using the existing typed Core execution surface and
fresh capability probes. Add OpenWrt transient Wi-Fi control/readback to that
surface only if its status is parseable; mixed/unknown radios cannot verify a
postcondition. No generic shell/config upload/flash API is introduced.

CLI example: inspect profiles offline; connect with host, user and key, with
optional firmware hint, first-use trust and sudo-read. JSON evidence distinguishes
configuration hints from detected firmware and root from network policy.

Implementation plan (inline execution):

1. Write failing integration tests for detection, trust argv, no inferred root,
   profile mismatch, privilege checks, typed reads and bounds; observe failure.
2. Implement the profiles/session SDK and CLI example. Test fixture transports;
   no router credential guessing or live login as part of tests.
3. Add OpenWrt radio parsing/strategy with regression tests on mixed radios,
   unknown states and typed adapter application; retain existing adapter tests.
4. Run affected Sentry tests, Clippy and formatting. Run offline catalogue CLI;
   record verified versus unverified support and publish a reviewable draft PR.

Primary protocol sources: OpenWrt ubus documentation and netifd wifi source;
MikroTik RouterOS user-policy and SSH manuals; VyOS SSH docs; OPNsense API and
SSH settings docs; FreshTomato SSH/admin documentation. Detailed links and
expansion criteria travel with the SDK documentation.
