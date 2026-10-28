# Continuous World globe streaming — implementation plan

The user rejected the earlier isolated AOI capture as the default World. Their
requested behavior is an existing Earth globe that can be dragged and zoomed
from orbit to streets, automatically requesting visible tiles and retaining
coarse imagery while finer imagery arrives. Aleph-style explicit capture/export
remains useful but must not gate ordinary geographic navigation.

Use the existing native Qt globe and Rust GeoFabric/worldd. A browser globe would
duplicate the renderer and bypass the requested daemon integration. Repeated AOI
jobs per mouse movement reproduce the rejected behavior and waste admission and
model calls. The chosen native continuous viewport stream reuses fixed providers,
the shared governor and private domain caches, with no model calls per movement.
The user has already authorized implementation and delegated provider choices;
this correction follows the clarified acceptance criteria without another design
approval round.

## Interfaces and ownership

- Pure GeoFabric `globe::select(GlobeViewport,max_zoom,max_tiles)` selects bounded
  disjoint visible Mercator quadtree leaves using the actual Earth-centered
  camera, frustum, horizon and projected error. Ancestors load coarse first.
- Owner CLI/Core/voice `Navigate(NavigationRequest)` authorizes a bounded session.
  Navigation has a separate ephemeral capability in private `navigation.json`,
  usable only for viewport/source updates and tile status. The existing projection
  credential stays read-only and the navigation capability cannot issue captures.
  Full Core ContextDomain/Policy/leases and exact session ledger bindings remain.
- Private newline JSON `{token,session_id,command:{action:viewport,...}}` reuses
  the owner loopback transport. Status returns original encoded tiles missing
  from the renderer's bounded `known_keys`, not repeatedly downsampled AOI pixels.
  Generation numbers discard stale camera replies. Only fixed typed providers
  are admitted; navigation data never reaches model-facing responses.
- worldd keeps a bounded volatile LRU, coalesces changing viewports, prioritizes
  current ancestors/leaves, and dispatches at most four downloads. It shares the
  capture governor, respects Retry-After and explicit stop/expiry/budgets. App
  focus and other PC use never alter either capture or navigation scheduling.
- Native `GlobeStreamBridge` + tile visuals render curved Earth tiles, clip ready
  children out of parent coverage, retain fallback while loading, and keep the
  global/polar context. Camera pans geographically at ground scale and zooms
  continuously. Small real observations and existing capture beacons remain.

## Tasks and verification

- [x] Pure selector RED/GREEN: planet/ground LOD, pan, seam, poles, caps, invalid
  camera, frustum/horizon and independent ray coverage.
- [x] Bounded session/capability/meter RED/GREEN, shared HTTP governor, Google
  RAM-only policy, delta replies, obsolete queue replacement and expiry/stop.
- [x] CLI/Core/voice authorization and strict schemas, cross-project isolation,
  durable context binding and rollback, model credential/pixel exclusion.
- [x] Native curved tile/fallback/camera/transport tests and themed streaming HUD.
- [x] Actual startup at whole Earth; actual wheel zoom requests higher detail;
  pan loads different geographic tiles; inspect NASA captures at multiple scales.
  Verify Google works interactively without saving its proprietary pixels.
- [x] Review, targeted Rust/native checks, replace standalone static-zone startup,
  update PR #52 and record measured results and remaining limitations.

Source dates/resolution and missing data remain explicit. No claim of Google 3D
photogrammetry, certified elevation, hardware acceleration or model accuracy is
implied by a textured globe or headless tests.

## Results — 2026-10-25

Remote verified as `PentaVM/ArchieOS-Nvidia`; work remains on
`codex/world-geofabric-rust`, PR #52. No edits to other agents' worktrees or the
installed GUI/WSL generation. The independent desktop World launcher is updated.

### Real input, network and rendering

`verification/world-globe/` contains the actual input scripts, filtered metadata,
NASA-only screenshots and logs. Inputs are Qt mouse-wheel and drag events, not
camera test setters or repeated capture jobs. Fresh owner navigation sessions
start at whole Earth with **zero AOI jobs**. All observations and HTTP data in
these runs came from the real providers.

- Google whole Earth: LOD 2..4, 32 selected leaves loaded, no pending tiles.
  Actual drag moved the center from 5.7391 N / 0 E to 40.7128 N / 74.006 W.
  Wheel navigation selected progressively higher LOD. The first gradual run
  reached the local 180/minute admission window; its real `rate_limited` state
  was displayed, rather than claiming completion or disabling the limit.
- The same admitted session recovered. Rapid wheel input coalesced to a final
  altitude of **148.94 m**, **LOD 19**, **12 loaded leaves**, **0 pending**,
  `ready`. A subsequent real drag changed the desired coordinates, loaded
  **16 leaves**, **0 pending**, and increased actual HTTP requests **366 → 376**.
  This check saved metadata only: `google-street-metadata.json`. No Google
  screenshots, exports, model pixels or per-movement model calls were used.
- NASA Blue Marble whole Earth is visually inspected in `planet-nasa.png`;
  real wheel zoom to ~1208 km loaded 40/48 requested LOD-8 leaves and retained
  parent imagery for the rest (`country-nasa.png`). A pan changed the desired
  tiles and retained coarse coverage while waiting on NASA's local 120/minute
  window. Its pending state is preserved in `nasa-input-metadata.json`, not
  reported as fully loaded. Escape returned to whole Earth, 32 leaves ready.
  Blue Marble is a static ~500 m source, not street-resolution imagery.
- These combined real v2 runs received **6,382,894 application bytes** over
  **497 HTTP requests**, including live public signals. The session was then
  explicitly stopped; its cache cleared and only its QA daemons terminated.

### Desktop startup

Opened the actual **ArchieOS World - datos reales.lnk**. It runs the updated
PowerShell/WSL helper and final native binary, with its own ports 17666/17667/17668.
`desktop-startup.json` confirms native control responding, Google `ready`,
LOD 2..4, zero pending, original **256×256** tiles, and zero startup AOI jobs.
The sampled returned Google pixel hashes were absent from every file in that
run's disk cache. RAM-only Google fetch policy also has unit coverage. The
interactive window remains open; closing it stops only its own helper daemons.

Installed standalone executable SHA256:
`2422059d398fed41148ab6e6d4b076f7fb79d09c649ddc33f8c1523b74f9d2a6`.

### Targeted verification and review

- Five-package Rust suite: **421 passed, 33 ignored**; final worldd rerun adds
  three new regressions (**78 lib + 1 bin + 3 integration + 17 formats passed**),
  for **424 distinct passing cases** across the affected suite.
- Live-feature voice tool parsing/scope tests: **8 passed**. This is schema and
  dispatch evidence, not live microphone/Nemo/model testing.
- Native CTests: **10/10 passed**; visual CTests: **19/19 passed**. Production
  native executable built with the project's Qt no-ASCII-cast constraints.
- Five-package Clippy, all targets, locked dependencies, `-D warnings`: passed.
- Independent read-only authority/budget/cancellation/transport review: no
  blocking findings. Exact work-context bindings, private capability separation,
  bounded client inventories, disclosure rechecks and model redaction reviewed.
- Regressions proved before fixing: global event parsing, new voice-job camera
  ownership, and hidden historical tile geometry inflation. Actual first render
  also exposed image-provider publication ordering; fixed by publishing images
  before model rows, with a rows-inserted regression. Reopened viewers seed a
  newer epoch generation so an existing session accepts their camera.

All logs are under `verification/world-globe/logs/`. Native execution used
Qt/WSLg **software rendering**, not verified RTX acceleration. The standalone
run has no connected Core service, so the GUI truthfully reports it unavailable;
no live inference TPS, voice hardware or complete OS deployment is asserted.
Byte budgets meter received application chunks; a final in-flight chunk can
cross the remaining budget. This is bounded application admission, not a hard
packet-level traffic cap. Focus/input in other apps never controls downloads.

GitHub CI on preceding head `d2e132f` did not start due to account payments/spend
limit (run 40176773472). That external execution restriction is separate from
the successful local checks; no billing configuration was changed.

## Zoom follow-up — 2026-10-25

The user reported that zoom did not show detail. Rechecked the live desktop run
instead of treating the earlier loaded-tile counters as visual proof. Two
independent causes were found:

1. At 18:23 UTC, the desktop session had expired at 15:26 UTC. Its capability
   correctly refused more downloads while the viewer retained old coarse tiles.
   The old launcher admitted only one hour and never renewed it. Re-admission
   through the owner CLI restored downloads in that existing window.
2. `worldNearClip` used 5% of the distance to the camera target. For globe
   navigation that target is Earth's centre: at 149 m altitude the near plane
   was approximately 318 km away, clipping the ground despite loaded LOD-19
   imagery. This affects altitudes below roughly 335 km. It now uses the lesser
   of target distance and altitude above the existing radius-20 Earth, with the
   existing positive floor. AOI and below-sea viewing remain supported.

Evidence before fixing:

- `logs/zoom-nearclip-red.log`: surface fails the numeric near-plane constraint
  at 10 m. Regression covers 10 m through 24,000 km, along all three axes.
- `logs/zoom-ground-render-red.log`: an actual Qt perspective camera 149 m above
  the globe renders a black pixel where the production image material should be.
  This uses synthetic labelled colour imagery, not Google pixels or a mock
  declaration that rendering worked.
- `logs/zoom-lifetime-red.log`: the launcher had no renewal lifecycle.

After fixing:

- JS camera/projection contracts pass. All **10 native CTests pass**, including
  the actual perspective ground render using production `Exploration.js`,
  material, image provider and geometry. Its red image detail is visible now.
- **5 launcher lifecycle tests pass**. An opted-in owner desktop launcher only
  renews its verified expired admission, preserving budgets per hour and the
  pinned NASA date. It never renews early, overrides explicit stop/replacement,
  or treats a control error as permission. Core/voice/ordinary CLI keep bounded
  authority; the renderer gains no renewal capability.
- `launcher-lifetime.json`: actual CLI, daemon and native viewer with 5-second
  admissions renewed after expiry with an identical request/date. Explicit stop
  survived the next expiry boundary without closing the viewer. Only that
  fixture's processes were closed.
- `zoom-google-metadata.json`: fresh actual wheel/drag run after the camera fix
  reaches 148.94 m, LOD 19, 12 leaves ready / zero pending; pan changes coverage
  to 16 leaves ready / zero pending, HTTP count **89 → 108**. Saved metadata only.
  Google pixels still stay out of disk/model/export paths. This network proof
  supplements the actual perspective render regression; counters alone had
  missed the clipping bug previously.
- Rebuilt and installed the native executable, restarted only the previous
  standalone World window, and opened the actual desktop shortcut. New run
  `run-1792953322977` enables owner renewal. `zoom-desktop-startup.json` records
  responsive native control, Google ready, no pending tiles, original 256×256
  pixels, and sampled pixel hashes absent from disk cache.

Current standalone executable SHA256:
`4b5ce61f8e0f4caf2670217d2c37510c24a238c8d87c89b2d989fa97c8fa8d58`.

No Rust/Policy/lease or provider-limit changes were needed. Other app focus does
not affect downloads or owner renewal. Tests and rendering remain WSLg/software
evidence, not a claim of RTX acceleration or complete OS deployment.
