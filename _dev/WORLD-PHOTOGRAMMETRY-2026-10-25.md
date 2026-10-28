# Continuous textured 3D World — correction to the raster-only implementation

User acceptance is globe -> city -> street with real textured 3D surfaces like
Aleph, automatically streamed by the camera. A raster globe, extruded OSM and
loaded raster counters do not satisfy this. Existing authority and no-billing
requirements remain. This document records the chosen implementation within the
already authorized task, not a request to repeat design approval.

## Chosen path

Port the pinned Aleph `src/mesh.py` public Earth Web protocol and bounded protobuf
decoder to Rust. The endpoint is `https://kh.google.com/rt/earth/`, distinct from
the disabled, billed official Google Map Tiles API. No API key/billing account.
It is proprietary experimental online viewing, not open data: mesh/JPEG buffers
stay in RAM and never enter exports, disk caches, model input or semantic analysis.
Keep the existing GeoFabric official-provider permissions and free-only plan.
The public upstream README documents the undocumented transport and its fragility:
https://github.com/TheLoki2020/aleph . Retain Aleph MIT source credit.

Open imagery/terrain remain globe context and explicit data sources, with honest
limits and coverage. They are not substituted for photographic city meshes.

## Boundaries and implementation tasks

1. `archie-worldd/src/earth.rs`: independent bounded parser for PlanetoidMetadata,
   BulkMetadata octree/OBBs, NodeData transform, byte delta coordinates, UV planes,
   high-water-mark triangle strips, layers/skirts and original JPEGs. No network
   or persistence. Convert original ECEF (X Greenwich, Z north) to native
   (+Z Greenwich, +Y north), with local origins. Reject malformed/oversized input.
   Synthetic protocol fixtures prove transforms, masks, winding and bounds.
2. `navigation.rs`, Core/CLI/voice: explicit `photogrammetry` admission flag;
   old serialized requests default false. New desktop navigation defaults on.
   A Google-selected photogrammetry request adds `kh.google.com` to exact Public
   provider leases. Preserve exact ContextDomain binding and strip model pixels.
3. `earth_runtime.rs` + navigation runtime: camera-selected octree metadata,
   progressive textured meshes, separate bounded in-memory metadata/mesh caches,
   delta `known_mesh_keys`, generation cancellation and provider usage. Reuse
   the existing session meter and shared HTTP admission, including Retry-After;
   never disk-cache Google mesh/metadata/textures. The owner ACK stays immediate.
4. Native: bounded decoded mesh/image buffers, stable model rows, actual local
   geometry/UV texture rendering in the existing Earth frame and GUI theme.
   Refine parents only when children are available, retain fallback across pan,
   clear obsolete source/session data, guard proprietary captures. Support
   camera orientation that makes city height visible, with no automatic fixed AOI.
5. Real validation: inspect camera-driven city and street geometry (nonzero real
   triangles, original texture dimensions, geometric height variation), pan and
   provider waits/expiry. Synthetic render proof alone is insufficient to claim
   real city delivery. No Google imagery/geometry screenshots or exports saved.
   Rebuild/open the existing standalone desktop World, update PR #52 and record
   remaining coverage/performance constraints. Do not claim official Earth 3D
   parity or RTX deployment based on tests.

## Private response contract

Navigation viewport/status envelopes accept optional `known_mesh_keys: [string]`
(max 64) alongside existing raster keys. Replies add `earth` metadata and
`meshes` deltas. Each mesh has `key`, `node_path`, `origin:[x,y,z]` in native units,
`vertices_base64` (8 LE f32: local xyz, normal xyz, uv), `indices_base64` (LE u32),
`texture_base64` (original JPEG), `vertex_count`, `triangle_count`,
`replace_mask` (ready child octants), and `attribution`. `earth.active_keys` is
the render plan, not the entire historical cache. Parent masks must be applied
by the backend decoder only for children actually ready, so pending children
cannot erase coverage. Responses/model/cache limits apply before allocation.

Sources/requests are typed and fixed-host; no viewer-supplied URL or source path
can initiate arbitrary HTTP. Focus in other applications is never a scheduler
input. A locally absent 3D coverage area keeps honest satellite fallback.

## Completed validation

The old raster-only PR did not meet the requested 3D outcome; this is a new
textured geometry path. It uses Aleph's original octree/protobuf/UV/triangle
algorithms ported to Rust, with bounded native mesh/JPEG delivery, not fabricated
extrusions. Original rectangular textures are supported (real samples included
256×512 and 512×1024). An initial square-only texture restriction was found with
real data, reproduced with a failing rectangular-JPEG test and removed.

Actual camera-driven results, with zero AOI jobs:

| Camera | Earth octree LOD | Loaded mesh parts | Triangles | State |
| --- | ---: | ---: | ---: | --- |
| NYC city | 15 | 53 | 95,753 | ready |
| NYC street | 21 | 64 | 113,205 | ready |
| NYC after pan | 21 | 64 | 105,261 | ready |
| San Francisco street | 21 | 62 | 56,680 | ready |

`earth-city-street-proof.json`, `earth-render-proof.json` and
`earth-sfo-proof.json` retain only operational metadata, coordinates, resource
counts and native renderer counters. NYC is 40.7128/-74.0060; the independent
SF camera is 37.7749/-122.4294. No Google pixels, mesh buffers or capabilities
were saved. Original provider HTTP is real, not fixtures. The SF stage reported
413 aggregate Earth requests / 26,732,103 Earth payload bytes across the live
QA session, within its admitted 1800 requests / 256 MiB total session bounds.

The final tests used isolated Xvfb + Qt/OpenGL software rendering so physical
mouse input on the user's PC could not redirect the scripted camera. Earlier
visible-window scripts were affected by concurrent manual navigation; the first
one correctly reported no coverage over the Pacific. The generic offscreen Qt
platform selected its 2D software scenegraph and was not counted as 3D render
evidence. Xvfb explicitly created a real OpenGL 4.6 QRhi pipeline.

With actual meshes, NYC submitted 93 draw calls / 420,177 vertices after pan;
SF submitted 95 / 269,376. World frustum culling avoids drawing geometry outside
the camera. Observed software rendering was approximately 5–8 FPS at these
views; this is not an RTX benchmark or a performance guarantee.

The renderer's built-in custom-geometry picking did not provide reliable hit
telemetry. An independent double-sided triangle ray check consumed all 64
current mesh parts **in RAM only** and found two center-ray intersections, the
nearest 177.13 m away. This proves real non-flat geometry intersects the camera
ray; no imagery/geometry analysis is routed to Cerebra. The saved result contains
only counts/distance and render metrics, not source buffers. The diagnostic
picking fields were removed from the final UI helper to avoid misleading data.

Focused proof:

- Worldd: **92 lib tests pass**, including 10 bounded decoder cases, protocol
  selection, legacy admission, and a real queued-worker stop/replacement race.
- Core: **154 lib tests pass**, including explicit host leases and model buffer
  removal. CLI navigation tests: **5 pass**. Live-feature voice schemas: **9 pass**
  against the installed SDK; no live audio/model accuracy claim.
- Native: **11/11 CTests pass**, including original JPEG/geometry bounds, active
  cache plans, private transport/delta/provider ordering, source/session clearing,
  and an actual inclined textured 3D wall render (synthetic known texture).
- Visual: **19/19 CTests pass**, including geographic pan/zoom and tilt validity.
- Five-package all-target Clippy with locked dependencies and `-D warnings`:
  passed. Production Rust binaries and native executable rebuilt successfully.
- Independent read-only reviews caught depth/imagery flags, ancestor-plan gaps,
  parent readiness, raw-cache bounds, stale worker/session adoption and post-stop
  buffer repopulation. These were corrected; a stop/replacement regression was
  observed failing before its fix. Google Earth is forcibly RAM-only even if a
  caller requests disk caching; the billed GeoFabric provider plan is unchanged.

Desktop shortcut opened with the final executable and rebuilt daemons:
`earth-desktop-startup.json` confirms responsive native control, Google ready,
photogrammetry admitted, zero AOI jobs and hourly owner renewal. Only the old
standalone World window was restarted; the installed GUI/Core/WSL generation
and other apps were not replaced. Current native SHA256:
`46269aff478883b9fcee85e5416d053ccd3605208da4219bc19e749eea1e0110`.

Remaining boundaries: proprietary experimental online provider and variable
geographic coverage; no official paid 3D API, offline Google mesh export,
Google Street View, certified elevations, full OS deployment or verified RTX
acceleration. NASA spectral products remain honest raster layers. The physical
desktop window remains open with default Google and photogrammetry enabled.
