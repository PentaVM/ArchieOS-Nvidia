# Aleph source provenance

Source: https://github.com/TheLoki2020/aleph
Pinned commit: ad68fc4a3c60fd7af3969597ef0ba886c63b0a40
Original author/copyright: Belluxx, 2026. MIT license retained in LICENSE.

The pinned source was cloned and inspected during implementation. This is a native
Rust adaptation of area/tile geometry, capture planning/checkpoints, atomic files,
paced HTTP/caching, OSM building extraction and terrain/export utilities. The Python
dashboard, its design, and Python runtime are not dependencies of ArchieOS.

Mapping: src/geo.py -> archie-geofabric::exploration; src/common.py and
src/capture.py -> archie-worldd::exploration_runtime; src/terrain.py and the GLB
export convention -> archie-worldd::geo_formats. Rust format readers use maintained
TIFF/earcutr libraries and existing ArchieOS types rather than copying parsers.

Implemented source adapters: public AWS/Tilezen terrain GeoTIFF, OpenStreetMap
closed building ways via bounded Overpass queries, Photon place lookup, keyless
NASA GIBS imagery and bounded USGS/EONET/adsb.lol observations. Existing C++ local
PBF import remains unchanged. Google satellite uses an experimental public tile
endpoint for an attributed RAM-only online preview: no disk cache, manifests,
pixel exports or model-facing pixels. Google imagery is proprietary, not open
data. Aleph's `src/mesh.py` protocol/decoder is now ported to Rust `earth.rs`
and `earth_runtime.rs` for continuous Earth Web textured meshes in navigation.
The fixed public `kh.google.com/rt/earth/` transport needs no API key or billing
account; it remains experimental proprietary online viewing. Mesh, metadata and
original rectangular JPEG buffers stay in bounded RAM, behind explicit
photogrammetry admission and the shared HTTP/session budgets. They never enter
disk caches, model input, captures or exports. The billed official Map Tiles API
and its provider permissions remain unchanged. Street View captures remain
disabled. This is not full Aleph provider/file-format parity. Exported GLB still
contains open terrain/buildings only. See `docs/WORLD-EXPLORATION.md` for source
selection, dates, budgets and limitations.

Data attribution is separate from the MIT code license. OSM requires contributor
credit and ODbL compliance. Terrain is a mosaic of source datasets; see
https://registry.opendata.aws/terrain-tiles/ and
https://github.com/tilezen/joerd/blob/master/docs/attribution.md (reviewed 2026-10-24).
