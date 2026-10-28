# Backend integration notes

Repository remote verified as `https://github.com/PentaVM/ArchieOS-Nvidia.git`.
Work stays in the existing `codex/world-geofabric-rust` worktree. No commit or
push was performed by this implementation task.

The staged production drafts under `D:\ArchieOS-Geo-Recovery-20261024` were
compared with the current sources and restored into the corresponding owned
Rust files. New regression tests were preserved. The existing provider
governor, response budgets, redirect refusal, pause/cancel path, Core Policy
admission, operation leases and exact ContextDomain job binding remain the
execution path. The native display files are owned by the separate native task.

The pipeline now admits fixed typed imagery sources and observation feeds.
Google satellite is an experimental online preview: provider policy forcibly
disables disk cache reads and writes, no Google source artifact is created,
every manifest serialization strips Google tiles, and Core removes all imagery
pixels before returning a model-facing text value. Open source imagery artifacts are retained
under the existing storage bound. Tile previews keep their original encoding
and dimensions when they fit the remaining aggregate inline budget; smaller
previews or explicit omissions carry their actual dimensions and source zoom.
Planning allows up to 16 tiles per source and 32 across the selected sources,
also respecting the request's tile limit. The same allocation formula drives
estimates and execution. Preview allocation reserves space for remaining AOI
tiles within the 384 KiB aggregate limit.

Signal providers use independent 60/600/15 second cache TTLs and the same
network governor. Explicit requests bound refresh to 1..60 cycles with a
60..3600 second cadence. Completed cycle count and next deadline are persisted
with the normalized view in one checkpoint. Pauses, restarts and explicit
resumes retain that schedule. Failed updates preserve last-good observations
and their retrieval time as stale; a source that has never succeeded remains
an error. Combined points are capped at 128 and 128 KiB, with truncation counts.

CLI flags and strict voice tools carry imagery selection/date and bounded
refresh options. Core derives provider hosts from the exact stored request on
resume; work project bindings and public-data egress admission remain enforced.
Model-facing replies retain imagery metadata but remove every `data_base64`,
replace terrain samples and building rings with truthful counts, and show at
most 16 signal points with exact known total/returned/omitted counts. Source
freshness, failures, bounds and imagery dates remain available. Owner CLI and
native projections retain their complete bounded previews.
Nonempty plan tile lists become their actual tile and omission counts; an
already-cleared projection does not invent a zero tile count.

Selecting an existing AOI uses the read-only authenticated routes
`/v1/exploration/jobs/<32hexid>` and `/api/exploration/jobs/<32hexid>`.
`snapshot_for` projects the existing in-memory normalized preview; it does not
open source files, fetch providers, execute commands or alter the job schedule.
Job metadata retains its original order, `selected_job_id` identifies the
requested capture, and only that capture retains its preview. Missing IDs never
fall back to a later capture. Default status continues to show the latest
preview without a selected ID, including Core's ordinary scoped status reply.
The display proxy preserves the trusted numeric loopback port configuration.
Imagery-only previews are valid without a fabricated empty building layer.

Signal source metadata carries matched totals, valid point omissions and an
explicit lower-bound indicator for bounded/unknown input. Aggregate totals count
valid matches before the provider and combined output caps, while rejected
counts exclude valid omitted points. Last-good counts survive stale updates.
The existing runtime freshness test now proves 140 valid matches become 128
visible and 12 omitted with zero rejected, then preserves those facts after a
failed update; Core's bounded text projection preserves 140 total with 16
returned and 124 omitted.

Verification evidence is coordinated by the root task. The initial runtime
RED run is recorded in `world-open-data-runtime-red.log`. The intermediate
`world-open-data-runtime-progress.log` reported 58/60 passing, including all
ten observation parser tests, with the two expected failures demonstrating
Google cache override and incorrect stale state without a previous success.
Both responsible layers were then corrected. Final GREEN results and real
network/native rendering evidence belong in the main task's verification log;
unit fixtures alone do not establish provider coverage or rendered quality.
These earlier logs are now grouped under
`verification/world-open-data/history/`. The misleading provisional filename
`world-open-data-quality-red.log` was corrected to `world-open-data-quality-green.log`:
that run was already green because the responsible fix had landed before it ran.

Final follow-up: a real ADS-B response was 80.5 ms ahead of the receiving clock.
The decoder now permits a five-second clock lead, keeps the reported timestamp
and records `provider_clock_lead_ms`; it still rejects stale or larger future
frames. EONET responses at its shared 200-event server limit are marked partial
lower bounds, even when the returned set has zero AOI matches. Both corrections
were reproduced by failing tests before their fixes and independently reviewed.
Five-package `cargo clippy --all-targets -- -D warnings` passed after correcting
three new test lints and documenting the explicit observation constructor arity.
