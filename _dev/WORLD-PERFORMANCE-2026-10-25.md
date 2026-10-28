# World streaming / native graphics performance â€” 2026-10-25

Scope: PentaVM/ArchieOS-Nvidia, existing `codex/world-geofabric-rust` / PR #52.
Only World rendering, its private streaming worker and its owner demo launcher.
No installed GUI/Core/WSL-generation replacement or global desktop driver change.

## Evidence and causes

A real paired private-protocol test at NYC 40.7128/-74.006, 200 m altitude,
1550x1016 / 45 degree FOV, then +0.0004 longitude, measured these results:

| Operational measurement | Before | After |
| --- | ---: | ---: |
| Cold first mesh | 4.444 s | 3.732 s |
| Cold complete render plan | 17.966 s | 9.120 s |
| Nearby pan complete render plan | 15.046 s | 1.259 s |
| Cold median private response | 60.18 ms | 18.01 ms |
| Pan median private response | 311.97 ms | 42.59 ms |
| Pan maximum private response | 606.92 ms | 67.82 ms |

Both runs reached ready, LOD 20, 60 cold / 62 pan mesh parts, with exactly
140 / 169 cumulative admitted requests. No quality/SSE/node-budget reduction.
The endpoint is real, not fixtures; timings exclude process/window startup and
vary with network/provider/PC load. A single before/after pair is operational
evidence, not a statistical latency guarantee. Saved files are counts/timings,
never imagery, geometry or capabilities.

The old worker downloaded each node sequentially, rebuilt the full cached plan
once per cached node on a pan, and deep-copied/re-serialized encoded buffers on
each publication. It also decoded identical JPEGs again in the native viewer.
The viewer's default OpenGL context was Mesa llvmpipe (CPU).

## Changes

- At most four node downloads/decodes in flight, through the existing provider
  governor and shared session meter. Coarse nodes remain first in the queue.
- Publish the cached plan once; publish the first incoming mesh promptly, then
  batches of four or after 120 ms of elapsed work. No frame-rate/focus/input or
  other-application condition controls HTTP. Provider rate/retry, explicit stop,
  session bounds and obsolete-camera cancellation continue to apply.
- Bounded full-node validation runs in blocking workers, outside the navigation
  mutex. Re-check exact session/generation, active meter and source before any
  asset write and publication. JoinSet cancellation prevents late asset writes.
- Immutable encoded packet buffers use shared ownership; serialized/payload byte
  costs are computed once. Only unacknowledged live deltas clone wire values.
  Clear/revocation drops this RAM state. Existing 32 MiB raw / 64 MiB packet
  admission, 48 nodes / 64 parts and frame bounds remain.
- Native immutable mesh deltas keep their original geometry/image allocation;
  changed octant masks reuse unchanged JPEG images by content identity. Validation,
  rectangular originals, active draw plans and conservative byte eviction remain.
- Optimize the worldd crate in development; compile the installed standalone
  native executable with CMake Release. Other Rust workspace profiles stay intact.

A separate acceleration experiment created a D3D12 RTX 3070 OpenGL context,
but full native street rendering stalled in the tested WSL Qt configurations.
An allocated hardware context alone is insufficient render/performance proof.
Those launcher/driver changes were removed. The installed viewer keeps its
existing stable graphics selection; no NVIDIA acceleration claim is made.
`perf-native-renderer.log` / `perf-native-basic-renderer.log` retain diagnostic
text only, with no images. No driver, global environment or other app was changed.

## Fresh verification

- Worldd: 93 lib tests pass, including new shared-buffer/budget/inventory/clear
  coverage and existing stop/replacement race. All-target Clippy with locked
  dependencies and `-D warnings` passes. worldd binary rebuild succeeds.
- Native CMake Release build succeeds; 11/11 CTests pass, including new duplicate
  JPEG/allocation regression and original malformed mesh/bounds/3D wall tests.
- Real baseline thresholds cold<10s / pan<5s failed before optimization and pass
  after it with unchanged plan level/counts. Existing owner renewal is unchanged.
- Full final native camera verification uses isolated Xvfb with the stable Mesa
  software renderer, explicit XCB and no screenshots. GPU context experiments
  are not counted as completed scene/hardware verification.

Reproduction: `benchmark_stream.py --bin-dir <compiled> --run-dir <new-private-dir>
--output <metadata.json>` and `benchmark_native.py --bin-dir <compiled>
--native-bin <viewer> --run-dir <new-private-dir> --output <metadata.json>`, both
under `_dev/verification/world-globe`. Native QA uses isolated Xvfb; its scripted
walk contains no screenshots and proprietary capture guards remain enabled.

Evidence: `perf-before.json`, `perf-after.json`, `logs/perf-*`. Prior functional,
source provenance and authority checks remain in `WORLD-PHOTOGRAMMETRY-2026-10-25.md`.

## Final native / desktop result

Actual final native execution exited 0. The log-vendor assertion in the QA helper
initially failed because this software run emitted no renderer log; it was removed
rather than inferring hardware from environment. The existing complete report
was independently summarized and checked: NYC street LOD 21 / 64 parts / 113,205
triangles; after pan LOD 21 / 64 / 105,261. Both ready, 93 draw calls, approximately
10 FPS with software requested. Render vertices were 439,614 / 420,177. This is
not a certified GPU performance result; data load timings above are separate.
`perf-native-stable.json` contains only these operational values.

The standalone executable was installed with Release optimization:
SHA256 `70e8f4b85ec4aa3aa854b2541eb789b4f9ed9deb1db72859b4bf989a906c8e47`.
The existing desktop shortcut was reopened, with the newly built worldd and owner
renewal. Only the owned standalone World process/daemons were restarted.
`perf-desktop-startup.json` records the live desktop status. Its verification
allows ongoing camera/source navigation, instead of resetting the user's current
view or requiring a changing render plan to be fully downloaded.
