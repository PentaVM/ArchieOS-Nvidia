# World operator gateway

Repository verified before edits: `https://github.com/PentaVM/ArchieOS-Nvidia.git`,
worktree `archieos-nvidia-latest/ArchieOS`, initial branch `codex/light-iris-cinema`.

The native World console can invoke one fixed program with argv:

```text
archie --json world operator --context <ContextDomainJSON> --request <operationJSON>
```

Context is a JSON string, for example `"work/acme/maps"` or `"personal/owner"`.
It is selected by the host, and privacy is fixed to Public. Neither field is accepted
inside an operation or its request object. No caller executable, arbitrary endpoint,
file path, shell command, fallback process or worldd credential is accepted.

Operations:

* `{"op":"status"}` and `{"op":"resolve","query":"Santiago"}`.
* `{"op":"plan","request":{"latitude":-33.4,"longitude":-70.6,"radius_m":750}}`.
* `explore` has the same request shape as `plan`.
* `{"op":"navigate","request":{"sources":["blue_marble"],"photogrammetry":false}}`.
* `navigation_status`, `navigation_stop`, `pause`, `resume`, `cancel` require `id`,
  a 32-character hexadecimal job/session identifier.
* Read-only Core catalog: `os_status`, `os_topology`, `os_modules`, `os_providers`,
  `os_peers`, `os_models`, `os_metrics`, `os_boot`, `os_ping`.

Region requests require latitude, longitude and radius_m. Other bounded fields
inherit the existing ExplorationRequest defaults. Navigation inherits its existing
defaults. Known DTO fields are accepted except domain; unknown fields are refused.
Existing pure region planning and navigation validation run before dispatch.

World operations use `archie_core::world_control::send`, preserving exact-domain
bindings, kernel-identified operator admission, Public-only provider disclosure,
Core Policy, single-operation leases and existing cancellation behavior. OS queries
use existing read-only ControlRequests against Core, with a ten-second deadline.
No request is replayed after a failed response.

Success is `{"ok":true,"result":<Core observation>}`; failures are
`{"ok":false,"error":"..."}` with exit code 1. Acceptance is not capture completion.
Inspect job state and error together; the existing runtime can complete with partial
provider failures. The gateway does not broaden existing direct owner CLI commands.

The voice summary now uses Core's `plan.tiles_total` when dense tiles were omitted,
falling back to the legacy tiles array. This fixes nonempty plans spoken as zero tiles.

## Verification

Native GNU toolchain via `D:/ArchieOS-Orbe-Review-20261026/env.ps1`, two build jobs.
RED witnessed: gateway tests failed with unavailable stub; voice regression failed
with observed 0 versus expected 12. GREEN: `cargo test -p archie-cli -p archie-voice`
passed all default-feature suites (39 CLI, 66 voice library, 3 voice binary and
17 voice integration tests). Existing Windows voice warnings concern Unix-only
imports/fields. `cargo build -p archie-cli --bin archie` succeeded.

Real executable smoke checks: injected domain refused before dispatch with JSON
failure/exit 1; valid status reported Core unavailable with JSON failure/exit 1.
No standalone/worldd fallback occurred. This does not verify a live Core deployment,
provider network success, hardware inference, model accuracy or latency.

An additional unchanged-Core authority test compilation was cancelled via Ctrl-C
at the parent agent's request to release memory; those additional tests did not run.
The completed CLI/voice tests and executable build above remain the verification.

## IRIS integration boundary

IRIS runtime health is available at its configured loopback `/health`, but health
alone is not use of its visual inference. Existing browser-camera detection does
not inspect World. World Google/Earth previews remain forbidden for capture and
analysis; preserve the existing native capture guard.

`archie-operate` currently allows firefox, car-soccer, photopea and kdenlive only;
it does not admit World. Existing Stage/Sight tools (`archie-look --app <live app_id>
--find <control> ...`, `archie-voice do ...`) require a real discovered surface,
admitted app policy, Stage socket/key and model. No verified World app_id or World
policy entry exists. Do not invent one or expose IRIS `/act` as OS actuation.
