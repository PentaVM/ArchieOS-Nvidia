# Observatory controls

The native Observatory opens on Orb. Its scene switcher is ordered Orb, Forge, World.
`--scene orb|forge|world` still selects a specific opening view.

Archie's live voice exposes two visual tools: `observatory_scene` and `orb_preset`. The same
controls are on the CLI: `archie visual scene <name>`, `archie visual preset <name>` and
`archie visual list`. Both are clients of `archie_orb::control`, which sends one JSON line to
the native display on `127.0.0.1:7668` and reads one line back (`ok` or `error: <reason>`).

The display is the only place the names live. `list` answers with its scenes (`orb`, `forge`,
`world`, from `DisplayControl`) and its presets: every `presets/*.json` file is embedded in the
binary and is a preset named after its file. `field` (Regiones vivas) is the original look,
with no overrides. Applying a preset opens Orb. A preset may only set keys that exist in the
renderer's defaults, with values that pass the same checks as the panel (type, `#rrggbb`
colours, known layout, numbers inside their range, not clamped): one bad key refuses the whole
file and the reply says which. Broken preset files are reported when the display starts. HUD
labels are the `labels` setting, so a preset can hide them; there are no overlay toggles.

The listener changes only the local view. It cannot run commands, access Stage,
or write to displayd. If the Observatory is closed, Archie reports that the local
window did not respond.

Forge includes a CLI tile that starts `foot` when a graphical Wayland or X11
session and `foot` are present. The EGLFS image has no window manager, so Forge
shows its existing launch limitation there.
