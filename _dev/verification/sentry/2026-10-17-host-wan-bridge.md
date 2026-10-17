# Host/WAN bridge: implementation and actual Windows verification

Owner request: extend Sentry beyond WSL/LAN visibility, reuse Claude's SSH work,
and require as little ongoing configuration as possible. Repository origin was
verified as `PentaVM/ArchieOS-Nvidia`. Work is isolated in
`codex/sentry-host-wan-bridge`; no GUI, WSL image generation, Wanda worker or
Core/Cortex authority code was replaced. No GPU operation or router login ran.

## Design and implementation

- Windows built-in PktMon captures all host NICs with a 768-byte snaplen, a
  64-MiB circular ETL and finite windows. No Npcap installation or new network
  listener is needed. One elevated registration installs an owner-scoped,
  highest-privilege task at sign-in, with duplicate-instance protection and
  three bounded restart attempts. It runs independently of Codex credits.
- A separately built Rust binary publishes its actual capture field catalogue
  and ingests bounded TSV metadata through stdin. Existing audit, learning and
  durable state are reused. TShark and analysis run in the existing ArchieOS
  distro, on CPU. The deployed binary is separate from OS/Claude services.
- Actual Windows testing found that combined `etl2pcap` output labels the single
  interface `unknown`, losing NIC provenance. The corrected implementation maps
  PktMon component `ifGuid` properties to Windows adapters, converts each known
  component separately, and carries its authoritative GUID into audit identity.
  Empty components receive explicit receipts. Duplicate observations on virtual
  and physical NIC paths are never presented as a unique host total.
- Host public-peer counters include IPv4/IPv6 and both directions, exclude
  multicast and inventoried on-link prefixes, and explicitly state that the WAN
  route itself is not verified. VPN routing and inventory changes remain limits.
- Raw files have a private ACL. Cleanup intent remains durable through analysis,
  and raw deletion precedes journal deletion. Stopped journals can be recovered
  without touching any active session. Every global PktMon stop first verifies
  the exact owned ETL path; an unknown/replacement session is left alone.
- A task installation identifier prevents overwriting an unrelated named task.
  A busy capture mutex exits without overwriting another worker's health.
  Existing global filters are inspected and never cleared. Tests include actual
  Spanish empty-filter output from this machine.

## Evidence (initial source snapshot PR39 `9902014`)

- The complete `archie-lan` Cargo test suite passed; its two TShark tests were
  initially ignored by default and then explicitly run: **2/2 passed** against
  the actual installed TShark and packet fixtures. These are parser/rule proofs,
  not evidence of attacks on this network.
- `cargo clippy -p archie-lan --all-targets --locked -- -D warnings` passed.
- **15/15** Windows PowerShell/Pester tests passed, including literal argv
  round-trip through real Python, Windows ACL setup, replacement-session refusal,
  cleanup-journal recovery and task ownership. Session/recovery tests use mocks;
  they do not prove real crash recovery or real competing PktMon sessions.
- **11/11** Python tests passed, covering public IPv6/on-link separation,
  multicast, bounded peer tracking, single-interface classic PCAP and filtered
  PktMon provenance. The real TShark → Python → Rust metadata pipeline also
  processed the repository packet fixture successfully.
- Actual standard-UAC Windows captures completed: an initial 12-second combined
  capture observed 100 public-peer records; the corrected 12-second capture
  observed 442 on physical Ethernet and no public-peer traffic on VirtualBox.
  Both reported zero alerts in their audit state. Raw ETL/PCAPNG files and the
  ownership journal were absent after finite completion. No claim of a clean or
  attack-free network follows from such short windows.
- The installed task `ArchieOS-SentryHostBridge` was observed Running with its
  own worker; a subsequent autonomous window produced receipts for **7** mapped
  components, with packets on **2**, and 1,284 public-peer observations on
  physical Ethernet (33,139 bytes outbound / 868,105 inbound). Counts belong to
  that window/observation point. Other components were empty, not failed or
  assumed absent. During capture one active raw ETL is expected.

Private receipts remain under the owner's local SentryBridge state directory;
raw payload files, keys, LAN identifiers and destination lists are not published
here. Runtime requirements and stop/status commands are in
`tools/sentry-bridge/README.md`.

## Router and authority limits

The unauthenticated router homepage identifies itself as Movistar; its Dropbear
banner does not establish firmware or authorized administration. The owner has
only confirmed web-panel access. No SSH password/key was guessed, installed or
verified. Complete gateway WAN capture remains unavailable until the owner's
one-time supported router enrollment/capture access exists.

The router lifecycle fixes preserve saved target/user/port/kind across retries,
do not overwrite configuration after failed install verification, and keep valid
DHCP/neighbour data when optional conntrack is unavailable. See the companion
router-autonomy report for fixture evidence and remaining SSH trust limitations.
Conntrack remains a bounded IPv4 snapshot, not total WAN traffic.

The existing typed HostSsh adapter already probes capabilities and gates host
administration through Core/Policy/leases. This collector does not grant an
unrestricted sysadmin shell or new authority. No PR merge was performed.

## Adoption of subsequent Claude changes

Before publication, PR39 advanced to `0c483da901fd66dad941fe7d780759191577eb3c`
(login-burst, SMBv1 and DNS-bypass). The isolated bridge commit was rebased onto
that head without conflicts; no shared worktree or Claude file was overwritten.
The dynamic catalogue now contains **47 fields** without a bridge-side schema
fork. The complete current suite, explicitly including ignored TShark tests,
passed **153/153**, and Clippy with warnings denied passed again.

The first autonomous worker then received its own documented stop request. It
completed **4 windows**, exited normally, left its scheduled task Ready, and
left no raw captures or ownership journal. Only after that did preparation
replace its separate pinned binary with the current PR39 build and the same
owned task was restarted. This verifies graceful owned stop/update, not arbitrary
crash recovery. Routine raw/journal recovery failure paths remain mocked tests.

Live re-registration exposed a Windows normalization detail: Task Scheduler
returns the local account as a short name rather than `MACHINE\user`. The
ownership guard initially refused its own existing task. A regression test
failed first; the fix resolves a local short name to its qualified Windows SID
and still requires the exact installation marker and owner SID. The expanded
Windows suite passes **16/16**. The replacement task remains scoped to the same
installation/account; no unrelated task is adopted by a matching short name.

## Current live notice requiring DNS-policy context

After the current 47-field build resumed, an actual autonomous window reported
1,067 public-peer observations on physical Ethernet and **one `dns-bypass`
notice**, for plain DNS to `200.28.4.129`. `Get-DnsClientServerAddress` on that
physical interface reports **200.28.4.129 and 200.28.4.130 as its configured DNS
servers**. This demonstrates public DNS use, not intentional filter bypass or a
compromise. The newly added rule treats public resolvers as outside the network's
DNS without consulting the configured resolver policy; a network that legitimately
hands out public ISP resolvers needs that context before concluding bypass.

Recommendation for Claude: extend the responsible rule/settings path with
explicit, provenance-bearing network DNS policy or resolver context, retaining
the raw observation and avoiding silent allowlisting based only on the current
OS setting (which DHCP itself could have changed). Do not rewrite labels or
suppress other rules in the bridge. No device block or administrative action was
taken in response to this notice. The task is running with source-matching
deployed scripts; the repository worktree is clean after publication.
