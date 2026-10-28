# Native GeoFabric exploration implementation

Approved by PentaVM in chat on 2026-10-24, in a separate managed worktree.
Initial integration base: 9091658. The integration branch subsequently merged
into main in PR #51; final base is origin/main at 51db334. The feature PR targets
main so unrelated integration changes do not enter its diff. Remote verified:
PentaVM/ArchieOS-Nvidia.

## Contract

- Rust geospatial planner in archie-geofabric; network and export workers in
  archie-worldd; displayd remains a read-only loopback projection.
- Explore intent carries WGS84 center, ground radius, layers, zoom, domain and
  explicit tile/byte budgets. It never derives permission from camera position.
- CLI and existing gated voice path issue explicit commands over a private local
  control channel. The display gets normalized read-only status and bounded previews.
- World camera movement/manual takeover only changes the camera. App focus,
  unrelated input, render tiers and hidden windows never pause or throttle jobs.
- Provider quotas, configured budgets, errors and explicit pause/cancel are the
  scheduling inputs. No global packet throttling, input gate or Stage changes.
- Cache/manifests are scoped by personal/work domain. No keys/Core state are
  passed to worldd or the renderer. Source URLs fixed by provider adapters.
- World HUD reuses the Xavi/PentaVM GUI theme, fonts, states and reduced motion.
  Missing telemetry is unavailable, not zero. Tokens/sec needs a measured source.
- Aleph reference ad68fc4a3c60fd7af3969597ef0ba886c63b0a40 (MIT, Belluxx).
  Preserve notices and document supported providers and formats honestly.

## Tasks and proof

1. [x] Bounded spherical ROI, antimeridian handling, tile planning and tests.
2. [x] Bounded TIFF/OSM decoders, GeoJSON/GLB exports and real format fixtures.
3. [x] Persistent jobs/cache, shared provider admission/retry state and explicit
       owner command channel; cancellation/resume and local HTTP integration tests.
4. [x] CLI and voice route through the same commands; read-only display projection.
5. [x] Native World HUD, geographic layer preview and camera target with isolated
       manual camera control; same shell theme and genuine source attribution.
6. [ ] Focused Cargo/Qt checks, review, results/limitations here, commit and PR.

## Environment

Windows cargo/rustc 1.99.1 available. Installed WSL distro returned
Wsl/Service/CreateInstance/E_FAIL during initial toolchain probe. Do not restart or
replace other agents' WSL installations. Linux/hardware tests remain unverified
unless actually exercised later. No recording is authorized in this task.

## Evidence

- Planner RED: unresolved archie_geofabric::exploration (test added before code).
- Format RED: 8 expected decoder/exporter failures before implementation; then
  12 format tests passed with generated real signed/float TIFF and OSM fixtures.
- Transport/planner RED: missing modules/functions; shared quota and restart
  tests added before implementation. Core gateway helpers were also observed
  missing before production implementation; telemetry recorder had 6 RED tests.
- Windows focused Cargo suite: **351 passed, 19 explicitly ignored**, 0 failed,
  covering archie-core/cli/displayd/geofabric/worldd. See cargo-tests.log.
- Linux/WSL Core owner gateway: **8 passed**, including the Unix owner boundary.
- Linux/WSL voice tool tests with installed NeMo SDK: **4 passed**; no audio,
  inference quality, microphone or synthesis runtime was exercised.
- Windows `cargo check -p archie-voice --features live --tests --locked`: passed
  using the installed SDK prefix for type checking only; no Windows SDK link,
  microphone, synthesis, model accuracy or live conversation was verified.
- Native headless checks: 3 CTests (preview, credential, actual QML HUD), 2 visual
  CTests (camera/observatory, 875 assertions), Node contracts, strict C++ syntax.
  Shared light/dark theme, reduced motion, manual-only camera takeover, late
  source elevation and omitted/truncated previews are covered. No GUI was shown
  and no recording was started. Windows Qt credential branch/full executable
  link, GPU/depth behavior and deployed scene transitions remain unverified.
- Real provider smoke: one public Rome 50m AOI, **594,915 downloaded payload
  bytes**, **840,254 stored bytes**, 1/1 terrain tile, GeoTIFF + GeoJSON + GLB.
  Photon lookup also succeeded. Anonymous private projection returned 403;
  separate read-only credential returned 200. See live-smoke-summary.json and
  the reproducible smoke.ps1. Original public source artifacts/logs remain in
  D:/ArchieOS-Geofabric-Build-20261024/live-smoke-1 (no credentials committed).

## Review decisions and corrections

- Protected private capture views at BOTH worldd and displayd, using a separate
  read-only credential. Qt never reads the command credential and never falls
  back to an anonymous exploration request. Core-scoped status also removes all
  unbound/foreign work-project jobs and provider aggregates.
- Added kernel-owner/Core profile/Policy admission to voice proposals rather
  than letting live model tool output invoke the owner worldd channel directly.
  Host-selected exact ContextDomain/privacy cannot be widened by tool JSON;
  final authorization recheck and bounded private binding ledger cover reply
  races. Failed accepted-job binding is followed by explicit cancellation.
- Saved source artifacts replay on resume even at an exhausted network budget;
  empty coverage completes without a fabricated GLB. Capture storage is bounded
  too; source-cache eviction has its own 128MiB/2048-object bound.
- Projection geometry and history are byte-bounded; full source files remain
  separate. Preview omissions/truncation are visible. Antimeridian export is
  explicitly refused before network work (pure planning still supports wrap).
- Mosaic, export and renderer now use one Web Mercator row convention. Terrain
  nodata stays missing, negative elevations remain source values, and a first
  measured elevation reframes only an untouched automatic camera.
- Fixed both admission and completion persistence-error concurrency leaks.
- Final voice controls cover plan/explore/resolve/status/pause/resume/cancel.
  Job controls require an exact persisted ContextDomain binding. Resume reads
  the owned stored request under a separate local lease, derives only its fixed
  provider hosts and obtains new Network leases. Pause/cancel use local Write
  leases. Focused independent review found no actionable blockers.
- Review's cache-eviction race was treated as a correctness fix: an eviction
  after metadata becomes a cache miss, rather than failing a valid capture.
  No unrelated cleanup or changes to Stage's human authority were made.

## Final validation after job controls

- Linux/WSL focused Cargo suite: **362 passed, 33 ignored, 0 failed** across
  archie-core/cli/displayd/geofabric/worldd, including the three new Core job
  control tests. See linux-final-suite.log. Ignored tests remain explicit;
  this does not certify hardware or deployment.
- Native post-rebase checks: all 3 HUD/preview/credential CTests and both
  camera/observatory CTests passed; see native-final.log.
- Final cargo fmt and git diff --check passed.
- Final live-feature voice tool tests with installed Linux NeMo SDK: **5 passed**,
  including all seven tools, strict job IDs and host scope/privacy preservation.
  See linux-voice-final.log; no audio hardware or model execution was tested.
- The final Windows broad-suite retry failed while linking with MSVC PDB
  error LNK1318. It is not recorded as a passing Windows final run; the prior
  351-test Windows result above predates the final job controls. See
  windows-linker-limitation.log. Final Rust behavior was validated on Linux.

## Limits and deployment

This is the native capture/planning subset documented in docs/WORLD-EXPLORATION.md,
not full Aleph provider/file-format parity. Google extraction remains gated out;
terrain/closed OSM ways/Photon are implemented. Existing local C++ PBF ingestion
is preserved. Terrain mosaic is a bounded preview; vertical datum, building base
alignment and GPU rendering fidelity are not certified by these tests.

The stock system world-feed unit retains DynamicUser/ProtectHome isolation and
does not automatically share owner credentials. The opt-in owner-session path
was exercised; a cross-identity packaged deployment still needs explicit
integration. No WSL image generation or installed release was overwritten.

Disk exhaustion interrupted initial Windows debug builds. Automatic review
rejected deletion of build cache/symbols, so all artifacts were preserved and
the task-owned build directories were losslessly NTFS-compressed. Compact Cargo
profiles disable debug/incremental compilation. No user data or sibling worktree
was removed. Attribution retains Belluxx's Aleph MIT notice and Xavi/PentaVM's GUI
ownership.
