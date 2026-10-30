# Light theme, compact model observations, cinematic camera, IRIS trial

Request: make the Xavi sphere and surrounding surfaces light and readable; replace
the diagnostic block with a small top-left model/rate list; finish node camera
motion; run and record the autonomous Firefox/MrBeast request through IRIS.

Repository verified: `https://github.com/PentaVM/ArchieOS-Nvidia.git`.
Working branch: `codex/light-iris-cinema`, based on the existing local Xavi/World
integration. Main security/keyring changes through 994740a are merged. Existing
untracked codebase-LOC reports belong to another task and are excluded.
Design credit remains ByXavi270/XaviCottier; native implementation PentaVM.

## Design and execution

1. Default shared palette to light. Pass theme colors into native views and use
   alpha-blended category ink instead of additive glow on a light background.
2. Move verbose provenance into Details. Project only real per-model completion
   receipts, expire observations after 30 seconds, show missing rates explicitly.
   Rates cover full request elapsed time; they are not generation-only benchmarks
   or evidence of an in-flight model. Never attribute a global gauge to a catalog.
3. Smoothly focus real checked node positions, follow moving nodes, return home,
   and cancel on manual movement. Freeze hidden time and honor reduced motion.
   Filesystem IDs are not OS/window IDs; do not invent mappings between them.
4. Restore the existing WSL runtime, submit one exact high-level instruction to
   the existing Policy/Stage/IRIS loop, and preserve actual frames/trace/results.
   Browser setup by the harness must be disclosed. No manually supplied browser
   steps, preselected video URL, replacement grounding model, or authority bypass.

## Validation and implementation results

- Native Windows build succeeded (MinGW/Rust .drectve warnings remain).
- Camera helper: 10 contracts; model projection: 15 contracts passed.
- Camera QML wiring: 6 tests passed using a passive field, not hardware evidence.
- CTest: 37/37 passed. The World HUD test previously assumed a dark default;
  it now verifies both dark and light transitions against the shared Theme.
- Real Qt renders exposed an alpha-composition bug. Light materials now emit
  premultiplied ink with One/OneMinusSrcAlpha blending. Dark fragment sources are
  unchanged. Both explicit Observatory CLI themes were rendered and inspected;
  commands, executable hash and evidence are recorded in
  [verification/light-mode-2026-10-27.md](verification/light-mode-2026-10-27.md).
- Review regressions reject replayed/unknown-source snapshots and unreachable
  daemons. Only Completed receipts create or renew per-model observations;
  transport Ok with a Failed/Cancelled receipt does not count as completion.
- Observability: 105 unit + 14 integration tests passed. Core production metrics
  module: 9/9 tests passed in a harness with real archie-memory and observability
  dependencies. WSL wrapper/task tests: 24 run, 23 passed, 1 Windows symlink skip.
  These harnesses do not establish full Core compilation or deployment.
- No live model rates, model accuracy, task success or latency are claimed.

The neural camera follows checked filesystem positions. OS action cinematics
already follow the existing StageMotorExecutor -> Operator/Policy/lease/gate/seal
-> StageDirector path, for apps hosted in Stage. A window ID is not a filesystem
path: no fabricated link makes the neural sphere appear to follow IRIS. Actual
IRIS execution and both camera paths running together remain unverified.

## Authorized runtime recovery

User explicitly approved restarting WSL despite possible interrupted workloads.
`wsl --shutdown` timed out. Non-elevated service restart was denied. A native
Windows elevation request to run a helper did not execute because Windows
PowerShell blocks script files; no execution policy was changed or bypassed.
The authorized native service stop reached StopPending. After it stayed stuck,
the verified wslservice process was stopped under the same restart authorization.
The service recovered: ordinary distro listing works again.

The target distro fails before entering Linux with
`Wsl/Service/CreateInstance/CreateVm/HCS/0x800705aa`. User subsequently authorized
closing Rocket League; it was already absent when this task resumed. No other
application was closed. Boot probes with a temporary 6 GB limit, then 6 GB/8 CPUs,
also failed. Original .wslconfig bytes were restored after each probe; this error
is not proven to be caused solely by low RAM. An additional administrative HCS
diagnostic elevation was cancelled in Windows; it was not retried another way.

The exact order `Abre Firefox y reproduce el último video de MrBeast.` has NOT
been dispatched. No autonomous attempt video, action trace or playback success
exists. The wrapper repair is in source, not installed into the stopped distro.
Its about:blank setup is harness preparation, not IRIS opening Firefox. The
current actor/runner has no arbitrary-string payload and limited GUI verbs;
individual ASCII held keys are not proof of reliable search/text entry. Do not
describe these capability limits as an observed model failure.

Work is saved locally on the integration branch. Its pre-existing base contains
large Xavi/World changes relative to origin/main; publishing that whole integration
or merging it into main is not part of this local UI/runtime verification.

Evidence directory (outside Git): `D:\ArchieOS-Light-IRIS-Cinema`.
Additional runtime details: [IRIS-AUTONOMY-2026-10-27.md](IRIS-AUTONOMY-2026-10-27.md).
