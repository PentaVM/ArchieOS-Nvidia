# Native World exploration

GeoFabric plans bounded geographic work in Rust; `archie-worldd` fetches the data,
keeps checkpoints and exports, and publishes normalized previews. World draws the
real terrain/building samples through the existing native renderer and the shared
Xavi/PentaVM shell theme. Aleph source/port provenance is in
[`third_party/aleph`](../third_party/aleph/README.md).

## Local owner session

### Continuous globe navigation

World opens at the whole Earth. Dragging changes geographic position; the wheel
changes altitude continuously. The native camera sends its current viewport to
Rust, which selects visible quadtree tiles and loads coarse coverage before fine
detail. Existing coverage remains while children arrive. No capture/AOI job or
model call is created per movement. Mercator wraps at the date line; the existing
global Earth supplies the polar caps.

Authorize one bounded session in the owner runtime:

```sh
archie world navigate --domain personal --json
archie world navigation-status SESSION_ID --domain personal --json
archie world navigation-stop SESSION_ID --domain personal --json
```

Defaults are 1 hour, 1800 HTTP requests, 256 MiB received application payload,
64 visible leaves and maximum zoom 19, further capped by the selected source.
`--no-signals`, `--imagery-sources`, `--imagery-date`, `--max-visible-tiles`,
`--max-zoom`, `--max-requests`, `--max-bytes` and `--duration-seconds` narrow this
admission. Closing the standalone launcher stops its daemons. An independently
hosted session continues until explicit stop, expiry or budget exhaustion.

The desktop shortcut opts its owner launcher into `--renew-navigation`: while
that window remains open, an expired admission is replaced with another bounded
hour using the same request and pinned NASA date. Budgets therefore apply per
hour; they are never reset early. Explicit stop, a replaced session or failed
owner verification disables renewal. Core/voice/CLI sessions retain their
explicit expiry; the renderer cannot renew authority. App focus has no effect.

The private `navigation.json` capability permits typed camera/source changes
and tile delivery only. It differs from owner control and read-only projection
credentials. Core/voice admission retains exact personal/work context bindings,
Policy/profile checks and leases. Model-facing acknowledgements contain metadata,
never this capability or imagery. Viewer restarts use newer generations; obsolete
camera downloads are canceled. Client cache inventories prevent repeated transfers
and downloads of tiles the viewer already owns.

Daemon imagery RAM holds at most 192 tiles / 32 MiB compressed data; native RAM
holds at most 256 tiles / 128 MiB decoded data. Geometry has a separate 64 MiB
cache. These bounds cover their named caches, not total process/GPU memory.
Google remains RAM-only through the shared fetcher; NASA may use the existing
bounded open-data disk cache. Local navigation/capture provider caps are shared:
Google 180 requests/minute and NASA GIBS 120/minute, each with four in flight.
These are local policy, not published provider quotas. Provider Retry-After,
local-window waits, usage and remaining session budgets appear in the HUD.

Global USGS/EONET observations accompany the globe. Near the ground, bounded
local observations also include adsb.lol aircraft. Markers use real coordinates,
reported altitude where present, timestamps and source properties. Missing
contacts, source failures and unavailable Core metrics remain explicit.

### Textured city and street geometry

With Google Satellite selected, newly admitted navigation enables
`photogrammetry` by default. Below 250 km altitude, Rust selects visible Earth
Web octree nodes by the actual camera, progressively downloads original JPEG
textures and indexed 3D surfaces, and replaces ready parent octants. A bounded
ancestor chain preserves coverage as detail arrives; obsolete camera requests
are canceled. This is Aleph's textured mesh protocol ported to Rust, not an
extrusion of raster imagery or OSM footprints. The undocumented public endpoint
uses no API key/billing account. Official billed Google 3D Tiles stays disabled.

Near the ground the camera tilts progressively to show building height. Hold the
right mouse button and drag vertically to adjust tilt; left drag pans and the
wheel zooms. Clicking a surface preserves the camera. Geometry appears only
where the provider supplies it; elsewhere satellite context remains with an
explicit coverage state. NASA spectral sources remain their labelled raster
products, not synthetic photographic buildings.

Use `--no-photogrammetry` on `world navigate`, or `photogrammetry:false` in the
voice tool, for raster-only navigation. Older serialized requests default false,
so an existing admission is never silently widened. Core adds `kh.google.com`
only when this flag and Google imagery are both admitted. Private viewer
messages include a separate bounded mesh inventory; model replies strip mesh,
texture and raw-protobuf fields. Google 3D capture/export is refused.

Earth Web source data is volatile: at most 48 raw nodes / 32 MiB, 64 MiB encoded
packets and 16 MiB metadata, plus temporary decode/transfer buffers. Active
packet groups are additionally bounded by decoded image/geometry cost. Native
3D caching is separately capped at 64 meshes / 128 MiB decoded data; original
rectangular JPEGs up to 4096×4096 are dimension-checked before allocation. The
shared `google_earth` local policy is 180 requests/minute, four maximum in flight,
with Retry-After handling and the same admitted session byte/request budgets.
Foreground apps never control this scheduling. Source quality, dates and
coverage remain external; no certified height/accuracy is implied.

### Explicit area captures

Exploration is explicitly enabled. The stock isolated `DynamicUser` world-feed
service remains unchanged; it is not an exploration deployment recipe. Core,
worldd and displayd need an intentionally configured owner session/private runtime
for the commands below. Do not start a second process over an occupied 7667 port.

```sh
cd core
cargo build --locked -p archie-worldd -p archie-cli -p archie-displayd
export ARCHIE_WORLD_RUNTIME_DIR="$XDG_RUNTIME_DIR/archie-worldd"
target/debug/archie-worldd --enable-exploration --domain personal
# In another terminal with the same runtime environment:
target/debug/archie world plan --latitude 41.8902 --longitude 12.4922 \
  --radius-m 50 --zoom 14 --layers terrain --domain personal --json
target/debug/archie world explore --latitude 41.8902 --longitude 12.4922 \
  --radius-m 50 --zoom 14 --layers terrain --domain personal --json
target/debug/archie world status --domain personal --json
target/debug/archie world resolve 'Colosseum, Rome' --domain personal --json
target/debug/archie world pause CAPTURE_ID --domain personal
target/debug/archie world resume CAPTURE_ID --domain personal
target/debug/archie world cancel CAPTURE_ID --domain personal
target/debug/archie visual scene world
```

`--max-tiles` defaults to 256; `--max-bytes` defaults to 64 MiB and bounds both
downloaded application payload and stored capture data, including cache copies
and exports. Terrain supports source zoom 0..14. `--layers` accepts terrain,
buildings, imagery and signals. The CLI requires an explicit
personal/work domain and acknowledges acceptance separately from completion.

The private command descriptor is **not** passed to displayd or Qt. A separate
owner-private `projection.json` grants only GET access to the exploration view;
both `/v1/exploration` and `/api/exploration` require its bearer credential. The
native client reads this file without exposing it to QML. A Windows native view
over WSL needs a matching explicit runtime path accessible to that owner; no
automatic cross-identity credential sharing is installed by this feature.

## Voice authority

The existing `live` voice mode accepts `--world-domain personal/owner` or a full
`work/organization/project` scope. This trusted host setting and the existing
session privacy grade accompany proposals to Core. The model cannot widen them.
The tools include `world_navigate`, `world_navigation_status`,
`world_navigation_stop`, `world_plan`, `world_explore`, `world_resolve`,
`world_status`, `world_pause`, `world_resume` and `world_cancel`. Controls require a specific
32-hex-digit job ID already bound by Core to that exact context. Pause/cancel
use local write authority; resume reads the owned stored request, derives its
fixed provider hosts and obtains new network authority. It cannot select a
different provider or globally pause other jobs.
Core verifies the kernel peer owner, operator profile, exact context scope,
privacy ceiling, emergency Policy state and one-operation leases for fixed
provider hosts. External providers accept explicitly Public data only. Core
rechecks authority after the reply; scoped status excludes other projects,
unbound owner-CLI jobs and global provider aggregates. Windows cannot authenticate
the Unix operator peer and refuses this voice gateway there.

## Concurrency and limits

Foreground app, keyboard/mouse activity outside World, window exposure and visual
render tiers are not download scheduler inputs. Manual World navigation only
cedes its camera. Jobs continue while another app is used or the viewer closes.
Explicit job pause/cancel, configured byte/tile limits, provider admission and
provider failures can stop/wait a job. There is no OS packet shaper or global
human-input suspension added by this feature.

Provider admission is shared across jobs in the daemon. Defaults are **local
caps**, not claims about external quota: terrain 120 requests/minute and 4 in
flight; OSM 6/minute and 1; Photon 30/minute and 1. HTTP `Retry-After` supports
seconds and dates; reported quota remains distinct from local policy. Concurrent
requests for the same resource share a cache lock. The source cache has a 128 MiB
and 2048-file bound with eviction restricted to generated SHA256 names.

Manifests/checkpoints survive interruption. A daemon restart leaves unfinished
jobs paused for explicit resume; preserved source files are reused before making
new requests. Empty coverage completes with empty GeoJSON, without inventing a
GLB. UI projections retain one selected preview and bound current geometry; omitted
buildings/truncated metadata are reported while complete files remain on disk.

## Imagery and observations

```sh
target/debug/archie world explore --latitude 37.62 --longitude=-122.38 \
  --radius-m 15000 --zoom 11 --layers terrain,imagery,signals \
  --imagery-sources sentinel2,blue-marble,google-satellite \
  --imagery-date 2026-10-20 --signal-updates 2 --signal-interval-seconds 60 \
  --domain personal --json
```

NASA GIBS sources are `blue-marble`, `sentinel2`, `landsat`, `natural-color`,
`infrared`, `night-lights` and `thermal`. They require no key or billing account.
Their source resolution, dates and gaps differ: HLS Sentinel/Landsat are dated
swaths, MODIS infrared is false colour rather than a thermal camera, thermal is
a coloured daytime land-surface-temperature visualization, and night lights is
explicitly the 2012 VIIRS composite. No numerical temperature is inferred from
a coloured raster. Daily products default to UTC minus three days; an explicit
date is pinned into the job. A date with no coverage is reported as no data.

`google-satellite` uses an experimental undocumented public endpoint without an
API key. It is proprietary online display imagery, not open data. Its pixels
are held only in RAM and excluded from disk cache, manifests and data exports;
after daemon restart its preview expires. Google/Cesium metered 3D extraction
is not enabled. On-screen source credits remain visible. Automated render-check
and UI-walk exports refuse selected proprietary imagery; use a NASA source for
saved examples. External provider access can change independently of this code.

For explicit AOI captures, no source gets more than 16 preview tiles; the selected sources share 32 tiles
and a 384 KiB encoded-pixel budget. Actual zoom/dimensions and preview omissions
are reported. Original open images remain available under the capture budget.
The UI drapes imagery to the terrain triangle surface with correct Mercator UVs.

Signals use public USGS earthquakes, NASA EONET reported natural events and
adsb.lol aircraft reports. Each point carries actual source/time/measurements;
unknown altitude/time stays unknown, quake depth is not airborne altitude and
aircraft barometric height is not certified geometric or ground-relative height.
The cache TTLs are 60/600/15 seconds respectively. `--signal-updates` is 1..60
and `--signal-interval-seconds` is 60..3600. Refresh schedules survive explicit
pause/restart/resume. Failed sources retain last-good records as stale; sources
that never succeeded remain errors. These requests never call an AI model.
ADS-B accepts up to five seconds of provider clock lead, retaining the original
observation time and disclosing the lead in point properties; stale or larger
future frames remain errors. EONET responses at its 200-event server cap are
explicit lower bounds, including when filtering finds no match in the AOI.

At most 128 observations are projected. Counts retain known valid totals and
omissions separately from rejected/filtered records, with lower-bound flags
when the bounded input scan cannot establish a full total. Core's text model
reply keeps 16 points with truthful counts and strips all pixels and dense
geometry, avoiding base64 token costs. The owner CLI/native view remains fuller.

Small themed point nuclei, halos and beacons mark actual observations and saved
AOIs. Selecting an older AOI performs only an authenticated read-only load of
its normalized preview; it does not dispatch a capture or contact providers.
`/api/exploration/jobs/<32hexid>` proxies `/v1/exploration/jobs/<32hexid>` with
the same projection credential. Unknown IDs fail closed. The selected inspector
updates as observations change, and marks a disappeared contact unavailable.

For an isolated owner-session demo, `ARCHIE_WORLDD_PORT=17667` makes displayd's
exploration proxy use that numeric loopback port; native `--displayd-port 17666`
and `--control-port 17668` avoid the installed OS ports. Both processes need the
same private `ARCHIE_WORLD_RUNTIME_DIR`. The reproducible helper is
`_dev/verification/world-globe/globe_demo.py`; it saves startup metadata and logs,
starts with zero AOI captures, and starts only its own demo daemons. The older
`world-open-data/live_demo.py` remains an explicit area-capture verification tool.
`scripts/world/Start-WorldDemo.ps1` opens that standalone development demo in WSL
using the compiled CLI/daemons and a native executable installed at
`/home/archie/.local/share/archie-world-demo/archie-display` (override with
`-NativeBinary`). It creates a separate retained run directory and closes its
own daemons when the window closes. It does not replace the installed OS startup.
For the standalone desktop build, configure native CMake with
`-DCMAKE_BUILD_TYPE=Release`; the development worldd crate also uses optimization.
Streaming uses up to four existing-governor node requests concurrently and reuses
immutable packet/JPEG allocations. It does not pause for focus/input in other apps.
Measured load timings and graphics limitations are recorded in
`_dev/WORLD-PERFORMANCE-2026-10-25.md`.

The desktop demo initially selects Google's online RAM globe; use
`-ImagerySource sentinel2` or the themed source selector for NASA. The Python
helper also defaults to Google; pass `--source blue_marble` for reproducible
non-proprietary capture checks.

## Truth and supported scope

- Native geometry sources: public AWS/Tilezen terrain GeoTIFF, closed OSM building
  ways and Photon lookup, plus the imagery/observation adapters above. Data source
  credits/permissions remain separate from the code license (OSM/adsb.lol ODbL,
  NASA/USGS/Copernicus product-specific credits; EONET includes third-party reports).
- Original TIFF/OSM captures and GeoJSON/GLB exports are kept. The displayed/exported
  terrain mosaic is a bounded 33×33 Web Mercator preview, not a full-resolution DEM.
- Source nodata stays missing. Building height is an explicit OSM tag or unknown;
  building bases use a labeled 0 m preview assumption. Vertical datum is unverified.
- Pure planning handles the antimeridian, but capture/export currently asks for
  two non-wrapping areas before any requests. Polar Mercator regions are refused.
- Multipolygon/hole buildings, Google Street View and offline exports of
  Google photogrammetry are not advertised as implemented. Existing
  GeoFabric storage/analysis restrictions are preserved.
- HUD effective TPS is the latest measured Core `AiInfer` output-token count over
  its full execution time, not generation-only throughput or all voice traffic.
  Unreported/stale measurements remain unavailable; no benchmark is displayed as live.
- Tests, real network smoke and limitations are recorded in
  [`_dev/WORLD-GEOFABRIC-RUST-2026-10-24.md`](../_dev/WORLD-GEOFABRIC-RUST-2026-10-24.md).
  The extension is recorded in `_dev/WORLD-OPEN-DATA-2026-10-24.md`.
  Continuous navigation and real mouse-input checks are recorded in
  `_dev/WORLD-GLOBE-STREAMING-2026-10-25.md`.
  Headless tests do not verify GPU behavior, live voice hardware or packaged deployment.
