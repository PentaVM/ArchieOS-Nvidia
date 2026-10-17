# Sentry: Windows host bridge

Reuses `archie-netwatch` and its actual field catalogue. Windows PktMon captures
all host NICs, including physical Ethernet/Wi-Fi and WSL/VM virtual adapters;
WSL TShark extracts metadata and Rust applies the existing Sentry audit rules.
It does not install a network listener or give observations execution authority.

From this repository, in ordinary PowerShell:

```powershell
.\tools\sentry-bridge\Start-SentryHostBridge.ps1 -PrepareOnly
```

The ArchieOS distro is discovered automatically when there is exactly one. If
there are several, specify `-Distro NAME` once. Preparation uses the distro's
system Cargo, TShark and Python 3.11+, builds on two CPU jobs, and deploys a
separate pinned binary. It does not load models or CUDA or replace an OS service.

Then run the deployed launcher **once as administrator** to enable autonomous
capture at sign-in (the owner remains the Windows user, not SYSTEM):

```powershell
& "$env:LOCALAPPDATA\ArchieOS\SentryBridge\app\Start-SentryHostBridge.ps1" -InstallStartup
```

For a finite verification instead of startup registration, use
`-WindowSeconds 12 -MaxWindows 1`. Normal capture uses 30-second windows and a
64-MiB circular ETL. There are gaps during conversion and analysis; this is not
lossless packet recording. Loopback coverage is not guaranteed. Existing global
PktMon filters or another capture cause a refusal rather than being reset.

Status and graceful stop need no elevation:

```powershell
& "$env:LOCALAPPDATA\ArchieOS\SentryBridge\app\Start-SentryHostBridge.ps1" -Status
& "$env:LOCALAPPDATA\ArchieOS\SentryBridge\app\Start-SentryHostBridge.ps1" -Stop
```

`-Stop` leaves a persistent stop request; `-InstallStartup` explicitly clears it.
The scheduled task retries failures three times, with one-minute spacing. It
does not depend on a Codex session or credits. Its installation identity prevents
overwriting an unrelated task with the same name. A mutex prevents duplicate
bridge workers. Raw captures have a private Windows ACL, are deleted after each
window, and retain a cleanup journal through conversion and analysis. A restarted
worker recovers its own journal only; an uncertain global capture is left alone.
Truncating packets to 768 bytes can still temporarily include application data:
raw ETL/PCAPNG files must never be published. Persistent results retain metadata,
not packet payloads or password arguments.

Results are private under `%LOCALAPPDATA%\ArchieOS\SentryBridge\latest.json` and
`health.json`; Linux audit state is under
`~/.local/state/archie-sentry-bridge/interfaces/`. Interface provenance is kept
separate; counters are never presented as unique totals across duplicate NIC
paths. Unnamed interfaces do not acquire invented stable identities. Storage
fails closed at 64 interface states rather than growing without a bound.

## Host Internet traffic versus the gateway WAN

The host summary counts traffic between a configured host address and a public
peer, in both directions, excluding multicast and known on-link prefixes
(including globally addressed IPv6 LAN neighbours). This is public-address
traffic at an observation point, not proof of the route taken: VPNs and changes
after the inventory snapshot can affect interpretation. TLS remains encrypted;
Sentry observes available headers, DNS and handshake metadata.

Capturing the host does **not** capture all other devices' Internet traffic.
Claude's router enrollment is reused for DHCP/neighbour tables and bounded
IPv4 conntrack snapshots. It needs the owner's one-time router SSH setup; a
Dropbear banner alone does not prove OpenWrt. See
[`docs/SENTRY-NETWATCH.md`](../../docs/SENTRY-NETWATCH.md).
An enrolled `archie-netwatch live` already polls the router autonomously.
Conntrack is sampled, may lack byte accounting, and is not full WAN capture.

Router telemetry uses a forced read-only SSH key. Host administration remains
the existing typed `archie-sentry` HostSsh adapter under Core/Policy/leases;
this bridge does not install a general root shell or bypass those contracts.
