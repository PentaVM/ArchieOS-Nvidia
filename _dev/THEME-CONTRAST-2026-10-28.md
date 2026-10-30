# Readable sphere and complete theme — implementation plan

Goal: make Orb, Forge, World and the shell useful in both themes, including a
visible theme toggle. The user explicitly requests autonomous screenshot-driven
iteration; implementation and visual corrections proceed without a design pause.

Architecture: retain native Qt/QML and the shared Theme singleton. Composite a
60% white veil over a neutral opaque canvas; use 75% white panel fills while text
and controls remain fully opaque. This is a material palette, not global item
opacity. Keep the existing premultiplied-alpha light shader path. Preserve all
Core/IRIS/Policy/lease authority and the camera motion implementation.

Design choices: ink blue/graphite text and borders, deeper category colors in
light mode, clearly separated node/connection/atmosphere layers, backed labels
that avoid collisions. Keep semantic accent/status differences in both themes.
Do not manufacture model readings or treat offline views as live execution.

## Work and interfaces

- [x] Root: Theme.qml palette, compositing/contrast helpers, shared ThemeToggle,
  shell TopBar and Observatory header. Theme.toggleColorMode() changes the one
  singleton; existing Settings and all surfaces follow that same state.
- [x] Sphere: NeuralScene, NeuralHud and light shaders. Checked actual node
  positions only; strengthen geometry, atmospheric boundary, connections and
  label backing/layout. No changes to camera authority or hidden-time handling.
- [x] Other GUI: Forge, World and shell component surfaces, status and disabled
  states. Replace inappropriate hardcoded dark fills/white text with tokens.
- [x] Verify: palette composited contrast, keyboard/pointer theme toggle and
  shared state; retain existing native and shell contracts.
- [x] Build with existing D: Qt toolchain, two jobs. Save real PNGs for Orb,
  Forge, World, shell/Vault and both themes; exercise the actual toggle.
- [x] Inspect every scene, list concrete problems, fix them and capture again.
  Continue until text is readable, labels do not form the previous central pile,
  sphere layers are visible, and theme changes cover the complete interface.
- [x] Independent review, final affected checks, local commit and evidence.

Repository verified: PentaVM/ArchieOS-Nvidia. Reuse codex/light-iris-cinema at
29b2e2d. Preserve unrelated untracked codebase-LOC reports. Results belong here
and in an external evidence folder, D:\ArchieOS-Theme-20261028.

## Visual iterations

Prior baseline:
D:\ArchieOS-Light-IRIS-Cinema\orbe-cli-light-confirmation.png has a pale field,
thin dull lines and overlapping unbacked labels. These are explicit failures to
correct, not an accepted visual target.

1. First complete theme: readable backed labels and shared toggles; found weak
   one-pixel points on D3D11, thin atmosphere, World's dark sky in light mode,
   and ghost text through overlapping window bodies.
2. Added bounded real-position 3D node markers, blue atmospheric volume, stronger
   rim/orbits, light World sky/limb and diffuse basemap reading light. Reading
   bodies resolve the panel material over the canvas; chrome remains translucent.
   Both neural themes and World were inspected from actual Qt PNGs.
3. Independent review found excessive label search work, fixed HUD reservation
   height and low dock icon contrast. Fixed all three with regression contracts.
   Final captures include both themes, Forge, World and shell reading windows.
   A complete tour had one no-effect final Orb click after an unexpected return
   to Orb; an isolated Forge->World check then held World across eight seconds
   and changed its theme successfully (2 clicks, no no-effect). Do not silently
   describe the first tour as entirely passing.

## Final design details

Light canvas: neutral #c5ceda plus a 60% white veil, resolved to opaque #e8ebf0
for the renderer. Panel/window/bar/dock coverage is 75%; glyphs remain opaque.
Dense reading areas use the same composited material, preventing background text
from becoming a second layer of readable content. Opaque accessibility mode still
works. Dark canvas is #081321 with its own pale text and blue-gray materials.

Primary/secondary/supporting light text: #142236 / #263b52 / #32455a. Accents,
categories, borders, hover, pressed and selection states are independent tokens.
Tests require >=4.5:1 text contrast after alpha compositing over black, white and
the real canvas in both modes. Dock glyphs exceed 3:1 after both highlight washes;
review measured 4.06–4.39:1. Status still has icon/text, not color alone.

Sphere: main links nominal 75% coverage, secondary links 38%, distinguishable
fog/depth, atmospheric blue/cyan volume, real native-coordinate node markers
with halos. Labels use 75% backing, 12px text, actual projected leaders and
collision avoidance. Limit 24 annotations and 48 candidates per annotation;
selection has priority and Details reports omitted labels honestly. These are
annotation limits, not removal or invention of graph nodes. Real model HUD bounds
are reserved, including six rows. Long model names cannot elide measured TPS.

Global toggle: top-right Observatory and desktop top bar; pointer and Space
tested. It changes the existing singleton and all views immediately. Opening
embedded World no longer resets a previously selected shell theme. CLI themes
remain supported. No persistence mechanism or new OS authority was added.

World: light mode omits the decorative dark star sphere and uses the shared
canvas. Its atmospheric limb uses dark premultiplied ink. A light-theme diffuse
lift improves map reading, using the same NASA basemap; it is not measured
radiance or invented city/night-light observations. Geography and solar direction
remain unchanged. Private navigation still reports its missing session honestly.

## Verification and limits

- Native build succeeded with the existing Qt 6.8.3/MinGW toolchain, two jobs.
  Existing Rust/MinGW .drectve linker warnings remain.
- CTest 39/39; shell QML 108/108; native QML 24/24; label-layout Node 7/7.
- Windows headless tests require QT_QPA_FONTDIR=C:\Windows\Fonts and the fixture
  read test requires QML_XHR_ALLOW_FILE_READ=1. Initial failures without those
  environment settings were rechecked with the proper environment.
- Final shell tour: five real clicks, zero no-effect. Dark->embedded World stayed
  dark. Themes, reading bodies, Settings, Sentry and Vault were visually inspected.
- An additional real reduced-motion contrast check exposed the popover dismissal
  backdrop intercepting bar.theme. A pointer regression failed before the fix and
  passed afterward. The backdrop leaves the global bar available, except lock's
  existing full pointer shield. Existing menu/window dismissal tests still pass.
  Final Control Center captures retain overlay=control in both light and dark
  after the real toggle click (2 clicks, zero no-effect). Reduced motion here is
  the normal UI preference used for stable contrast inspection, not fake state.
- Captures and UI reports remain outside Git in D:\ArchieOS-Theme-20261028.
  Vault captures show local directory names and are private inspection artifacts;
  they are not included in the shareable gallery.
- Backend disconnected in these captures. No live model rates, IRIS execution,
  deployment or rendering/model latency claims follow from this visual QA.
