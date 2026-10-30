pragma Singleton
import QtQuick

// Shared material palette. Opacity belongs to surfaces, never their text/content.
// The canvas resolves a 60% white veil over a neutral base; panels use 75% coverage.
// Both ink palettes remain readable even over an unblurred black/white scene.
QtObject {
    id: t
    property string colorMode: "light"
    readonly property bool light: colorMode === "light"
    property bool opaque: false
    // Blur is a compositor capability, not something this theme pretends to provide.
    property bool blur: false
    property bool reducedMotion: false
    property bool runtimeConserve: false
    // Vivid colours (default): near-black surfaces, more saturated accents. Settings > Apariencia.
    property bool vivid: true

    // Fonts shipped with the shell (display/shell/fonts, SIL OFL 1.1).
    property FontLoader interFont: FontLoader { source: Qt.resolvedUrl("../fonts/inter/Inter-Variable.ttf") }
    property FontLoader groteskFont: FontLoader { source: Qt.resolvedUrl("../fonts/space-grotesk/SpaceGrotesk-Variable.ttf") }
    readonly property string font: interFont.status === FontLoader.Ready ? interFont.font.family : "sans-serif"
    readonly property string disp: groteskFont.status === FontLoader.Ready ? groteskFont.font.family : font

    readonly property real whiteBackdropOpacity: 0.60
    readonly property real surfaceOpacity: opaque ? 1.0 : 0.75
    readonly property color desktopBase: light ? "#50657e" : "#081321"
    function composite(foreground, background) {
        return Qt.rgba(foreground.r*foreground.a+background.r*(1-foreground.a),
                       foreground.g*foreground.a+background.g*(1-foreground.a),
                       foreground.b*foreground.a+background.b*(1-foreground.a),1)
    }
    readonly property color desktop: light ? composite(Qt.rgba(1,1,1,whiteBackdropOpacity),desktopBase) : desktopBase
    function toggleColorMode() { colorMode=light ? "dark" : "light" }

    // Surfaces: glass / opaque.
    readonly property color bar: light ? Qt.rgba(.86,.90,.95,surfaceOpacity) : Qt.rgba(.025,.045,.075,surfaceOpacity)
    readonly property color dock: light ? Qt.rgba(.86,.90,.95,surfaceOpacity) : Qt.rgba(.035,.055,.09,surfaceOpacity)
    readonly property color win: light ? Qt.rgba(.86,.90,.95,surfaceOpacity) : Qt.rgba(.035,.055,.09,surfaceOpacity)
    readonly property color side: light ? Qt.rgba(.84,.88,.94,surfaceOpacity) : Qt.rgba(.055,.08,.125,surfaceOpacity)
    readonly property color panel: light ? Qt.rgba(.86,.90,.95,surfaceOpacity) : Qt.rgba(.035,.055,.09,surfaceOpacity)
    // Resolve glass over our canvas for dense reading areas. This avoids ghost
    // text from another window showing through without pretending to blur it.
    readonly property color reading: composite(panel,desktop)
    readonly property color raised: light ? "#c6d3e2" : "#1b2b40"
    readonly property color hover: light ? "#b8cde0" : "#263b53"
    readonly property color pressed: light ? "#a1bed7" : "#304b68"
    readonly property color hair: light ? "#4f6b89" : "#718ba7"
    readonly property color inner: light ? Qt.rgba(1,1,1,.38) : Qt.rgba(.6,.75,.9,.18)
    readonly property color iconSurface: light ? "#d2dce8" : "#bbcbdc"
    readonly property color iconInk: "#16324c"
    readonly property color sel: light ? "#c6e4fa" : "#184c6a"

    // Text.
    readonly property color t1: light ? "#142236" : "#f5f8ff"
    readonly property color t2: light ? "#22374f" : "#dfe9f5"
    readonly property color t3: light ? "#293d53" : "#c4d2e3"
    readonly property color tdis: light ? "#425368" : "#91a5bd"

    // Accents and states. A state is never colour alone: icon + word always.
    readonly property color acc: light ? "#004d72" : "#40c4ff"
    readonly property color arch: light ? "#704500" : "#ffbf64"
    readonly property color ok: light ? "#07583e" : "#57dfb0"
    readonly property color warn: light ? "#684200" : "#ffd077"
    readonly property color bad: light ? "#8c1931" : "#ff9cb2"
    readonly property color info: light ? "#005166" : "#83d9ee"
    readonly property color unk: t3

    // ArchieOS brand (logo package v3.5, design by Xavi): mint neon on deep matte black.
    // The light theme carries the dark-ink wordmark; the dark theme carries the neon one.
    readonly property color brandMint: "#20d79d"
    readonly property color brandInk: "#0a0a0a"
    readonly property url logo: Qt.resolvedUrl(light ? "../brand/archieos-logo-light.png" : "../brand/archieos-logo-dark.png")
    readonly property url mark: Qt.resolvedUrl(light ? "../brand/archieos-icon-light-256.png" : "../brand/archieos-icon-256.png")
    readonly property color onAccent: light ? "#ffffff" : "#07131c"
    readonly property color dockInk: "#ffffff"
    readonly property color track: light ? "#d9e3ed" : Qt.rgba(1,1,1,0.10)

    // File categories (colour-blind safe, same as the sphere's nodes).
    readonly property var cat: (light ? { documents:"#704500",media:"#005b45",data:"#555100",archive:"#713773",executable:"#8d350b",other:"#324d68" }
                                : vivid ? { documents: "#ffa000", media: "#00c98d", data: "#fff000",
                                  archive: "#e85cb4", executable: "#ff5a00", other: "#d8d8d8" }
                                : { documents: "#e69f00", media: "#009e73", data: "#f0e442",
                                  archive: "#cc79a7", executable: "#d55e00", other: "#bbbbbb" })
    // Dock icons: solid colour under a 165° white→black wash.
    readonly property var dk: ({orb:"#036889",world:"#276080",vault:"#1b59ad",forge:"#895100",
                                sentry:"#066643",stage:"#6742b3",set:"#455b75"})
    // Window controls.
    readonly property color tlClose: "#d9605a"
    readonly property color tlMin: "#d9a441"
    readonly property color tlZoom: "#4fb67b"
    readonly property color tlOff: light ? "#a4b3c2" : (Qt.rgba(1, 1, 1, 0.17))

    // Radii.
    readonly property int rWindow: 14
    readonly property int rPanel: 16
    readonly property int rCard: 10
    readonly property int rRow: 8
    readonly property int rDock: 22
    readonly property int rDockIcon: 14

    readonly property int barH: 32
    readonly property int dockIcon: 56

    // Motion (handoff MOVIMIENTO.md): four profiles. Only transform and opacity ever animate;
    // never blur or shadows. The person's setting decides the profile; `reducedMotion` (also what
    // the OS "reduce motion" preference forces) kills every spatial movement. The default is the
    // handoff's recommendation (Balanced); it is an open owner decision (DECISIONES.md, 10).
    property string motion: "balanced"   // full | balanced | quiet
    readonly property string profile: reducedMotion ? "reduced" : runtimeConserve ? "quiet" : motion
    function pick(full, balanced, quiet) { return profile === "full" ? full : profile === "balanced" ? balanced : profile === "quiet" ? quiet : 0 }
    // Quiet keeps only opacity: no translation or scale.
    readonly property bool spatial: profile === "full" || profile === "balanced"
    readonly property int durOpen: pick(220, 160, 100)
    readonly property int durClose: pick(160, 120, 100)
    readonly property int durPanel: pick(200, 150, 100)       // control center, activity, workspace menu
    readonly property int durPalette: pick(180, 140, 100)     // palette and search, opening
    readonly property int durPaletteClose: pick(120, 100, 100)
    readonly property int durNav: pick(200, 150, 100)         // folders, list/grid switch
    readonly property int durToast: pick(240, 180, 100)
    readonly property int durToastOut: pick(200, 150, 100)
    readonly property int durDock: pick(160, 120, 100)
    readonly property int durSel: profile === "reduced" ? 0 : 100
    readonly property int durPulse: pick(600, 400, 120)
    // Curves: Salida (out), Entrada (in), Estándar; as cubic-bezier control points.
    readonly property var easeOut: [0.16, 1, 0.3, 1, 1, 1]
    readonly property var easeIn: [0.4, 0, 1, 1, 1, 1]
    readonly property var easeStd: [0.4, 0, 0.2, 1, 1, 1]

    // Tinted chip fills: only warn, bad and arch carry a background tint.
    function tint(c, a) { return Qt.rgba(c.r, c.g, c.b, a) }
}
