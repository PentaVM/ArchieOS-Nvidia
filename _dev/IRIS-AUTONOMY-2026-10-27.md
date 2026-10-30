# IRIS autonomy attempt preparation — 2026-10-27

## Scope and ownership

The requested real task is one high-level prompt: open Firefox and play the latest
MrBeast video. Archie must choose its actions; the test operator must not supply
clicks, coordinates, search results, a channel URL, or a prepared video.

Remote checked before editing: `https://github.com/PentaVM/ArchieOS-Nvidia.git`.
Working branch: `codex/light-iris-cinema`. This change owns only the operator wrapper,
its task-construction test, and this note. Existing GUI, telemetry, WSL generation,
training, and other uncommitted work are not changed here. No commits were made.

## Audited existing path

`archie-operate` constructs a TOML task and calls `archie-arena-live`. That Rust
runner asks the configured reasoning router for a plan, captures the Stage window,
sends frames and the current instruction to local trained IRIS `POST /act`, and
passes its controls through the existing Stage operator gate and policy. It saves
`first.png`, `result.json`, `trajectory.jsonl`, and approximately one frame per
second under `~/archie-runs/operate/<run>/`.

The runner already emits explicit IRIS `click` and `right_click` step types when
`task.judge == "gui"`. The wrapper previously used `judge = "none"` for every
application, suppressing that explicit GUI-click branch. IRIS controller button
holds are a separate path and were not affected by this branch.

The generic `archie-session run firefox` accepts the installed program, but does
not create a dedicated Firefox profile. The existing `archie-session open` path
does create its own `$ARCHIE_RUN/ff` profile and opens the supplied address in the
Stage. It accepts `about:blank`; its argument is passed directly to Firefox.

## Minimal repair

- Added `--app firefox` with an unchanged task prompt and the existing isolated
  `archie-session open about:blank` reset. No YouTube or channel page is preloaded.
- GUI tasks use `judge = "gui"`; car-soccer retains its previous controller judge
  `none` to avoid adding the explicit GUI-click branch to game controls.
- `--app firefox` together with `--url` is rejected before session startup.
- Existing policy file, leases, credentials, app identities, routing, and action
  grants are unchanged.
- Runtime diagnostics explicitly disclose that the actor cannot type arbitrary
  text and that the operator policy does not grant text or scroll. They also state
  that the harness opens the isolated blank Firefox, so opening Firefox is not
  evidence of an IRIS-produced action.

Once the real environment is ready, the dispatch form is:

```text
archie-operate "Open Firefox and play the latest MrBeast video" --app firefox --seconds 120
```

This prepares a real attempt, not a supported-completion claim. In particular,
typing a search query or URL remains a known capability gap. No hardcoded
instructions or policy expansion was added to hide that gap.

## Verification performed

Tests execute the actual Bash wrapper and parse its written TOML. Session startup,
arena/model invocation, and JSON quoting are controlled boundary stubs; no real
Stage, browser, IRIS inference, Core, or GPU is started by them. The stubs prove
task construction and argument wiring only, not model accuracy, latency, hardware
readiness, or successful playback.

The new tests first failed for the missing Firefox app and `none` versus `gui`
judge. A separate failing check caught the non-isolated generic Firefox launcher
before replacing it with the existing isolated blank-browser path.

Windows validation used **Git Bash**, not `C:\Windows\system32\bash.exe` (the WSL
launcher):

```powershell
$env:ARCHIE_TEST_BASH='C:\Program Files\Git\bin\bash.exe'
$env:PATH='C:\Program Files\Git\bin;C:\Program Files\Git\usr\bin;'+$env:PATH
python -m unittest discover -s scripts\wsl -p 'test_*.py' -v
& 'C:\Program Files\Git\bin\bash.exe' -n distro/wsl-operator/rootfs/usr/bin/archie-operate
```

Result: **24 tests run, 23 passed, 1 skipped** (`test_symlink_escape_is_rejected`,
symlinks unavailable). Bash syntax check exited 0. This is the WSL packaging and
wrapper script suite, not a whole-repository Rust or GUI test run.

## Runtime evidence and readiness gate

Read-only Windows observations during the audit:

- Process `archie-display`, PID `61012`, runs from
  `D:\ArchieOS-Orbe-Review-20261026\native\archie-display.exe`; 61012 is a PID,
  not an HTTP port.
- WSL relay listeners existed on 7666/7667/7668/8081/8791/55432. Two-second HTTP
  reads of `7666/api/orb`, `8081/health`, and `8791/health` timed out. A listener
  therefore did not establish application readiness.
- No Windows listener was observed on IRIS port 8765. This alone cannot establish
  the state of a service reachable only within Linux.
- Windows artifacts include `iris-beta21-step132000.pt` (1,414,806,323 bytes) and
  `ArchieOS-Operator.wsl` (1,046,675,202 bytes). Their presence does not prove the
  installed checkpoint, digest, text tower, or runtime is ready. No checkpoint
  hash or Linux artifact validation was run in this subtask.

Before running the real attempt, establish bounded responses from the intended
WSL distribution without restarting or terminating WSL/training. Confirm the
installed wrapper matches this change; the installed Stage executable, control
socket/key presence (never print key contents), Firefox, current policy, configured
reasoning provider and credential presence, checkpoint digest, SigLIP text tower,
and IRIS `GET /health` must be available. Coordinate memory/GPU ownership before
any inference process starts. Confirm that the Stage session is owned by this
attempt before its isolated-profile reset replaces that session's Firefox.

`archie-session up` can auto-select CUDA and start IRIS. `archie-talk` calls
`archie-session down` before voice startup. The desktop PR41 launcher can start
the database and Core. None of those commands was run by this audit/repair.

The real attempt must retain its true prompt, timing, decisions, policy receipts,
frames, and failure/completion evidence. A `result.json` with actions is not a
playback verifier. REM learning is separate and must not treat this unjudged GUI
attempt as a verified successful training sample.

## Authorized restart follow-up (07:04 UTC)

The user subsequently authorized restarting WSL for the real test. The root agent
owns that restart and its elevated helper; this subtask issued no additional
shutdown, restart, process termination, or training command.

At `2026-10-27T07:04:26Z`, the expected elevated-helper status file
`D:\ArchieOS-Light-IRIS-Cinema\wsl-elevated-restart.status.json` was absent and
`WslService` still reported `Running`. A bounded `wsl --list --verbose` process
(PID 58504, started at 07:02:05 UTC) exceeded its five-second observation deadline
and remained running with empty stdout/stderr at 07:04:26. No other WSL probe was
launched while this probe remained stuck. This does not establish that the
elevated restart ran or completed.

Native read-only `nvidia-smi` reported an RTX 3070 with 8192 MiB total, 6509 MiB
used, and 30 C. This is real adapter telemetry, not evidence that IRIS inference
can allocate its working memory or that the memory owners may be interrupted.

Evidence files are in `D:\ArchieOS-Light-IRIS-Cinema\`:

- `runtime-readiness-blocked.json`
- `wsl-runtime-list.out.log` and `wsl-runtime-list.err.log` (both empty)
- `gpu-readiness.out.log` and `gpu-readiness.err.log`

The exact requested prompt for the eventual single dispatch is
`Abre Firefox y reproduce el último video de MrBeast.` It has not been submitted
to Archie in this follow-up. The corrected wrapper has not been copied into the
unresponsive distribution, and no services or models were started here. There
are no attempt screenshots, action traces, or playback recording to claim. The
existing eventual recording mode remains a local Stage frame sequence plus
trajectory/result JSON, not a completed screen-video recording.

### Service recovery and real distro-start failure (07:10–07:12 UTC)

After the root agent's separately authorized native Windows service recovery,
`WslService` reached `Stopped` at 07:10:27 UTC. A normal `wsl --list --verbose`
then returned exit 0 in under one second and auto-started the service. It listed
`ArchieOS-Operator-RTX3070-Test` as `Stopped`, WSL version 2. Only this subtask's
previous stuck listing client PID 58504 was terminated, as directed by root.

Starting that exact distro as `archie` to run a read-only preflight exited -1
before entering Linux with:

```text
Recursos insuficientes en el sistema para completar el servicio solicitado.
Error code: Wsl/Service/CreateInstance/CreateVm/HCS/0x800705aa
```

Windows CIM reported 16,721,892 KiB total visible physical memory and 2,020,580 KiB
free (about 1.93 GiB). The existing `.wslconfig` sets `memory=9GB`, `swap=4GB`,
`autoMemoryReclaim=gradual`; it was not modified. `WslService` and `vmcompute`
reported `Running`. The host memory figures accompany the actual resource error;
they do not independently prove which Windows resource allocation failed.

Additional artifacts in `D:\ArchieOS-Light-IRIS-Cinema\`:

- `wsl-service-transition-history.jsonl`
- `wsl-list-after-stop.decoded.txt`
- `runtime-preflight.sh` (read-only Linux probe; it never entered Linux)
- `runtime-preflight.out.log`, `runtime-preflight.err.log`, and decoded
  `runtime-preflight.failure.txt`
- `host-memory-readiness.json`
- `iris-dispatch-readiness.json` (current undispatched state)

No further shutdown, Hyper-V/VmCompute action, Windows restart, application
termination, WSL memory-setting change, model startup, or VRAM allocation was
performed by this subtask. The actual autonomous goal remains undispatched;
existing model files, installed wrapper, Core/Stage/IRIS, and playback remain
unverified in the distro because Linux did not start.
