# World open imagery, signals and point beacons

User-directed extension of PR #52, 2026-10-24. Repository verified as
PentaVM/ArchieOS-Nvidia; reuse the attached clean feature worktree. The user
delegated provider choice and requested a working, tested result.

## Design

Use the existing Rust job/HTTP governor and native Qt World renderer. Add imagery
as a typed layer and sources selected through CLI/voice. Google public satellite
tiles responded HTTP 200 without a key during reconnaissance; expose this as an
experimental online preview, with attribution, no disk cache/export and no pixels
in model-facing Core replies. Google imagery is proprietary, not open data.
NASA GIBS is the open, keyless stack: Blue Marble, HLS Sentinel/Landsat, MODIS
natural/false colour, night lights and land-surface temperature. Source dates,
native resolution, gaps and failures stay visible. No paid API or billing setup.

Point signals come from bounded USGS, NASA EONET and adsb.lol requests. Their
observations carry source, time, coordinates and measured properties. Unknown
altitude/time remains unknown. Explicit bounded refresh jobs can update them;
other applications and viewer activity never affect job admission or scheduling.

Reuse Lumenfield's visual language for small World markers: luminous nucleus,
thin halo and tapered beacon, with restrained screen size and physical elevation.
Keep ordinary system nodes out of geographic space when they have no coordinates.
AOI nodes identify actual requested areas and can reframe their existing capture.
Data points identify actual observations. No fabricated contacts or demo metrics.

Alternatives considered: Google-only scraping is fragile and lacks reusable data
rights; account/billing-backed stacks do not meet the user's requested startup.
The chosen mixed stack keeps a working online Google preview and open imagery
and observations that can be retained and inspected.

## Implementation plan

Goal: satellite-backed World areas and useful sourced data, presented as compact
orb-like points and beacons, with a reproducible real-network demonstration.

Architecture: pure typed planning -> owner/Core admission -> bounded Rust HTTP
jobs -> authenticated read-only projection -> native textured AOI and markers.

Tech stack: existing Rust/Tokio/reqwest/serde and Qt Quick 3D/C++/QML. Preserve
Core, Policy, leases, exact work context, personal/work separation and GUI theme.

- [x] Add tests for typed sources, dates, bounded refresh, fixed host admission,
  Google no-persistence/model-pixel exclusion and independent source failures.
- [x] Implement imagery adapters under the existing budgets, provider governor,
  explicit pause/cancel and persistent job semantics. Keep preview bytes bounded.
- [x] Implement bounded USGS/EONET/ADS-B parsers, malformed-input/coordinate tests,
  AOI filtering, provenance and explicit data freshness.
- [x] Render georeferenced textures and small Lumenfield-inspired beacons; test
  UV/coordinates, realistic sizes, selection, missing data and existing camera.
- [x] Exercise actual providers, CLI jobs, refresh and private views; pause/resume
  scheduling is covered by runtime tests (not represented as a live operator test).
- [x] Build/run the native World and inspect an actual rendered example, fixing
  visible defects. Separate rendering evidence from hardware/deployment claims.
- [x] Run relevant Rust/native tests, review authority and concurrency, update
  operational docs/results and the existing PR.

Review focus: no hidden paid source, no arbitrary renderer egress/URLs, no Google
pixels saved or returned to AI, no false live timestamps or fabricated altitude,
bounded snapshots/refresh/cache, exact context on resume, no app-focus throttle.

## Research

- https://github.com/TheLoki2020/aleph — pinned upstream source inspected locally.
- https://nasa-gibs.github.io/gibs-api-docs/access-basics/
- https://gibs.earthdata.nasa.gov/wmts/epsg3857/best/1.0.0/WMTSCapabilities.xml
- https://earthquake.usgs.gov/earthquakes/feed/v1.0/geojson.php
- https://eonet.gsfc.nasa.gov/docs/v3
- https://github.com/imzyrix/Gods-Eye/blob/main/DATA_SOURCES.md — source-stack
  reference; source licenses remain separate from its MIT code.

## Verification updates, 2026-10-25

Source remains on `codex/world-geofabric-rust` in PentaVM/ArchieOS-Nvidia, PR #52.
The independent GUI owner's `668ade2` Clippy corrections are preserved. Only the
explicitly authorized test distro was restarted after the disk-full incident.
Other distributions, GUI checkouts and installed OS services were not restarted.

The actual SFO capture used 37.62,-122.38, radius 15 km, date 2026-10-20. NASA
Sentinel-2 and Blue Marble delivered pixels; Google's keyless endpoint delivered
eight RAM-only tiles. The job received 2,009,165 payload bytes. Two scheduled
observation refreshes completed: 19 then 18 reported aircraft. USGS and EONET
returned no matching records in this particular area; no synthetic contacts were
substituted. Retained JSON is redacted of every imagery pixel payload.

The real ADS-B refresh initially exposed a provider clock lead of 80.5 ms. A new
RED test reproduced the discarded feed; the fix accepts a bounded five-second
lead, preserves source times and publishes `provider_clock_lead_ms`. A separate
review caught the EONET server-side limit; another RED/GREEN regression now marks
200-event responses as lower bounds even for zero local matches. Backend review
then found no remaining blockers.

Authenticated selection was exercised through displayd: anonymous GET 403, POST
405, missing ID 404. Repeated selected GETs changed neither downloaded bytes nor
cache-hit counts. Actual persistent manifests retain Google metadata with empty
tiles while the live projection has eight tiles. These checks supplement the
unit tests forcing Google cache bypass and model pixel removal.

Rust verification: the five affected packages passed 396 tests with 33 ignored;
the subsequent EONET regression and entire worldd suite passed (65 library and
21 binary/integration tests), giving 397 passing tests across the final affected
set. Six voice-tool tests passed with the live feature; no microphone, model
accuracy, voice latency or hardware GPU performance is inferred from them.
Results and RED/GREEN history are under `verification/world-open-data/`.

Native visual verification is complete: actual captures exposed and reproduced
texture-provider wiring, inverted V coordinates, coincident-surface depth
fighting, missing-pixel alpha, incorrect midtone output and overlapping pointer
targets. The final renderer uses a single measured surface, neutral no-data
material and faithful sRGB output. Eight native CTests, 19 visual CTests and Node
contracts pass. Independent native re-review found no remaining blockers.
Actual pointer clicks changed selection from the newer capture to the older
capture with both NASA Sentinel-2 and Blue Marble; the screenshots and reports
are retained. Cloud cover and the approximately 242 m/pixel bounded Sentinel
preview remain visible limitations. See `verification/world-open-data/native.md`.

The final executable was copied to the owner-private demo directory and launched
from that copy; `startup-copy.png` verifies its embedded QML/resources load.
The separate desktop shortcut `C:\Users\Penta\Desktop\ArchieOS World - datos reales.lnk`
launches `scripts/world/Start-WorldDemo.ps1`. A real Windows PowerShell 5.1 run
caught shell interpretation of Windows backslashes; `--exec` plus slash-form
input fixed path conversion. The script carries a UTF-8 BOM for that host. Closing
the test window stopped only its own daemons; the desktop shortcut was then
opened again to leave an interactive World window with Google's RAM-only preview.
No proprietary screenshot was taken. This is the standalone World development
demo, not a claim that Core/voice or the installed full OS were deployed.
