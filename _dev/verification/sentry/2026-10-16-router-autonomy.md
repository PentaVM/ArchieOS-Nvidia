# Router enrollment retry — 2026-10-16

Repository verified before edits: origin is PentaVM/ArchieOS-Nvidia; isolated
sentry-host-wan-bridge worktree. No router login, administrative action, GUI work,
hardware capture, model/GPU work, commit or push performed.

## Changes

- Bare Dropbear/OpenSSH banners no longer establish OpenWrt/Linux firmware.
  Explicit web markers and a FreeBSD banner remain evidence; an unknown router
  needs an explicit kind instead of invented lease/key paths.
- Enrollment retry preserves saved address, kind, user and port unless the owner
  supplies that option. A changed host default route cannot silently retarget it.
- `router pull` reads the persisted target directly; gateway discovery now belongs
  only to detect/new enrollment, avoiding its former Linux routing-table
  prerequisite on Windows.
- `--install` saves replacement configuration only after installation and the
  subsequent read succeed. Manual enrollment explicitly says SSH access remains
  unverified. The public Config serialization remains compatible.
- Missing optional nf_conntrack no longer turns successful DHCP/neighbour output
  into an SSH failure. This remains a bounded IPv4 snapshot, not total WAN bytes.

## Validation

TDD red observed: generic SSH falsely classified Linux; retry reset the saved
connection; unsupported install replaced the previous configuration; missing
conntrack failed the full local shell read. All four mechanisms failed before
their fixes and passed afterward.

WSL distro ArchieOS-Operator-RTX3070-Test, /usr/sbin/cargo and rustc 1.99.0,
locked dependencies, CARGO_BUILD_JOBS=2,
CARGO_TARGET_DIR=/home/archie/.cache/sentry-bridge-build,
RUSTFLAGS=-Cdebuginfo=0:

```
cargo test -p archie-lan --locked --lib --bin archie-netwatch --test router_enrollment
```

105 passed: 94 library tests (6 router tests), 7 netwatch unit tests and 4 new
router CLI lifecycle tests. No failures/ignored in this selection. The CLI tests
use an isolated local SSH executable fixture with per-process PATH, never a
router connection. They verify saved-kind retry, explicit override persistence
after success, previous configuration retained after post-install read failure,
and pull targeting the saved router. The conntrack test executes the actual
generated command through /bin/sh against local fixture tables.

Other integration targets and the concurrent Windows metadata changes are for
the parent task's final package verification. Real SSH authentication, firmware,
host trust, router capabilities and complete WAN visibility remain unverified.

## Remaining risks

- Both install and reads still use StrictHostKeyChecking=accept-new. Changed keys
  fail against an existing entry, but missing pin state can establish fresh trust.
- Exact-line append is idempotent for unchanged key commands only. Upgrading the
  forced read command can leave an older duplicate of the same key active;
  existing authorized_keys/.ssh modes are not repaired. Existing installed keys
  retain their old command, including its conntrack failure behavior.
- Missing public half of an existing private key is not recovered automatically.
- A saved config has no durable verified/pending field; live attempts it, and
  manual enrollment remains pending until a successful read. No privilege is
  inferred from configuration or SSH banners.
- live router polling/result handling still depends on receiving parsed capture
  packets. An idle interface can delay reads/results. This slice does not alter
  that scheduler or Core/Policy/lease authority.
- Optional conntrack failure now preserves base tables but is not separately
  reported as a capability result. IPv6, capped entries and missing accounting
  prevent interpreting snapshots as total WAN traffic.
