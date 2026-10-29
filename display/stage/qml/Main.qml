pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Shapes
import QtQuick.Window
import QtWayland.Compositor
import QtWayland.Compositor.XdgShell
import "qrc:/archie/shell/prototype/Lang.js" as Tr

// The stage. Applications are hosted as surfaces on a desk that the director's camera
// frames; everything drawn over the desk — the agent's cursor, its ripples, the caption,
// the takeover banner — comes from StageController, which takes it from the tested model
// in display/visual. This file lays things out and draws them. It decides nothing.
WaylandCompositor {
    id: compositor

    required property var stage
    required property string stageSocketName
    required property bool fullscreen
    /// Which of the machine's screens the stage stands on.
    required property int screenIndex
    required property int initialWidth
    required property int initialHeight
    /// The desktop: the window of this app id and title lies under every other one, across
    /// the whole screen and unframed, and the others are framed between its top bar and its
    /// dock — ArchieOS's own shell, so its apps and the ones Archie uses share one screen.
    required property string desktopApp
    required property string desktopTitle
    /// A local file Archie's voice rewrites with each line it says on its own; "" for none.
    required property string narrationUrl

    socketName: stageSocketName

    readonly property color ink: "#e6edf7"
    readonly property color soft: "#93a2b8"
    readonly property color dim: "#5b6879"
    readonly property color edge: "#23303d"
    readonly property color cyan: "#6fe3f5"
    readonly property color green: "#47d18b"
    readonly property color amber: "#f0b429"
    readonly property color red: "#f2626b"
    // The shell's own window tokens (display/shell/prototype/Theme.qml, vivid dark), so a
    // window the stage frames looks like one the shell does.
    readonly property color winFill: "#050507"
    readonly property color hair: Qt.rgba(1, 1, 1, 0.09)
    readonly property color accent: "#1ec0ff"
    readonly property color title1: "#ffffff"
    readonly property color title2: "#cfcfdc"
    readonly property string sans: "Inter, Noto Sans, DejaVu Sans, sans-serif"

    // Bumped whenever a window comes, goes or is renamed: what is the desktop, and where the
    // other windows tile, is read again.
    property int layoutVersion: 0
    // Whether the desktop has the keyboard: the shell's own windows (Sentry, Forge, Vault)
    // are inside it, so the stage's windows step aside to the shelf while it is in use.
    property bool desktopFocused: false
    // What the stage's caption calls an app id: its last dotted part (org.mozilla.firefox: firefox).
    function labelOf(appId) {
        const dot = appId.lastIndexOf(".")
        return dot >= 0 ? appId.slice(dot + 1) : appId
    }
    function setStashed(index, stashed) {
        if (index < 0 || index >= shellSurfaces.count || shellSurfaces.get(index).stashed === stashed)
            return
        shellSurfaces.setProperty(index, "stashed", stashed)
        compositor.layoutVersion++
    }
    function isDesktop(xdg) {
        if (!xdg || !xdg.toplevel || compositor.desktopApp.length === 0)
            return false
        return xdg.toplevel.appId === compositor.desktopApp
                && (compositor.desktopTitle.length === 0 || xdg.toplevel.title === compositor.desktopTitle)
    }
    readonly property bool hasDesktop: {
        void compositor.layoutVersion
        for (let i = 0; i < shellSurfaces.count; ++i)
            if (compositor.isDesktop(shellSurfaces.get(i).shellSurface))
                return true
        return false
    }
    // Where window `index` stands among the framed ones, and how many there are.
    function authorityTint() {
        return compositor.stage.authority === "autonomous" ? compositor.cyan
             : compositor.stage.authority === "operator" ? compositor.amber : compositor.red
    }
    function authorityLabel() {
        return compositor.stage.authority === "autonomous" ? Tr.t("ARCHIE TRABAJA")
             : compositor.stage.authority === "operator" ? Tr.t("CONTROL HUMANO") : Tr.t("PAUSADO")
    }
    function windowSlot(index) {
        void compositor.layoutVersion
        let slot = 0, count = 0
        for (let i = 0; i < shellSurfaces.count; ++i) {
            if (compositor.isDesktop(shellSurfaces.get(i).shellSurface) || shellSurfaces.get(i).stashed)
                continue
            if (i < index)
                ++slot
            ++count
        }
        return { slot: slot, count: Math.max(1, count) }
    }

    Component.onCompleted: stage.compositor = compositor

    // What each verb reads as, in the operator's language. The model hands over a verb, an
    // application and a count; there is no field that could carry typed text.
    // What a person calls an application: the first part of its window's title (Photopea,
    // not "Photopea | Online Photo Editor — Mozilla Firefox"), else its app id.
    // `appId` is as the caption gives it (labelOf).
    function appName(appId) {
        // The desktop's own windows (World beside the shell) go by the desktop's name.
        if (appId.length > 0 && appId === compositor.labelOf(compositor.desktopApp) && compositor.hasDesktop)
            return compositor.desktopTitle
        void compositor.layoutVersion
        for (let i = 0; i < shellSurfaces.count; ++i) {
            const xdg = shellSurfaces.get(i).shellSurface
            if (!xdg || !xdg.toplevel || compositor.labelOf(xdg.toplevel.appId) !== appId || compositor.isDesktop(xdg))
                continue
            const name = xdg.toplevel.title.split(/ [|—–-] /)[0].trim()
            if (name.length > 0 && name.length <= 40)
                return name
        }
        return appId
    }

    function captionText() {
        const app = compositor.appName(stage.captionApp)
        switch (stage.captionVerb) {
        case "moving": return Tr.t("moviendo el puntero · ") + app
        case "clicking": return Tr.t("clic · ") + app
        case "double_clicking": return Tr.t("doble clic · ") + app
        case "dragging": return Tr.t("arrastrando · ") + app
        case "scrolling": return Tr.t("desplazando · ") + app
        case "typing": return Tr.t("escribiendo ") + stage.captionCount + Tr.t(" caracteres · ") + app
        case "pressing_keys": return Tr.t("pulsando ") + stage.captionCount + (stage.captionCount === 1 ? Tr.t(" tecla · ") : Tr.t(" teclas · ")) + app
        case "focusing": return Tr.t("enfocando · ") + app
        case "drawing": return Tr.t("dibujando · ") + app
        case "observing": return Tr.t("mirando · ") + app
        case "interrupted": return Tr.t("acción detenida · el control es tuyo")
        case "refused": return Tr.t("rechazado · ") + refusalText(stage.captionRefusal) + " · " + app
        }
        return ""
    }

    // The same verbs, past tense, for the strip of recent actions in the status line.
    function activityText(raw) {
        const entry = { verb: raw.verb, count: raw.count, refusal: raw.refusal, app: compositor.appName(raw.app) }
        switch (entry.verb) {
        case "moving": return Tr.t("movió el puntero · ") + entry.app
        case "clicking": return Tr.t("clic · ") + entry.app
        case "double_clicking": return Tr.t("doble clic · ") + entry.app
        case "dragging": return Tr.t("arrastró · ") + entry.app
        case "scrolling": return Tr.t("desplazó · ") + entry.app
        case "typing": return Tr.t("escribió ") + entry.count + Tr.t(" car. · ") + entry.app
        case "pressing_keys": return (entry.count === 1 ? "tecla · " : Tr.t("atajo · ")) + entry.app
        case "focusing": return Tr.t("enfocó · ") + entry.app
        case "drawing": return Tr.t("dibujó · ") + entry.app
        case "observing": return Tr.t("miró · ") + entry.app
        case "interrupted": return Tr.t("detenida por ti · ") + entry.app
        case "refused": return Tr.t("no se hizo: ") + refusalText(entry.refusal) + " · " + entry.app
        }
        return entry.app
    }

    function refusalText(code) {
        switch (code) {
        case "operator_has_control": return Tr.t("tienes el control")
        case "stage_paused": return Tr.t("en pausa")
        case "surface_changed": return Tr.t("la ventana cambió")
        case "surface_unknown":
        case "surface_unmapped": return Tr.t("la ventana ya no existe")
        case "app_mismatch": return Tr.t("otra aplicación")
        case "point_outside_surface": return Tr.t("fuera de la ventana")
        case "command_expired": return Tr.t("orden vencida")
        case "command_replayed": return Tr.t("orden repetida")
        case "not_typeable": return Tr.t("ese texto no se puede escribir aquí")
        case "capture_failed": return Tr.t("no se pudo capturar la ventana")
        }
        return code
    }

    // Tiles for n windows: one fills the desk, two share it, three give the first the left
    // half, more form a grid. Space is kept for the status line above and the caption below,
    // or, over the desktop, for its top bar and its dock.
    function tileFor(index, count, width, height) {
        const top = compositor.hasDesktop ? 52 : 64
        const bottom = compositor.hasDesktop ? 116 : 104
        const side = compositor.hasDesktop ? 40 : 32
        const gap = 20
        const w = width - 2 * side
        const h = height - top - bottom
        if (count <= 1)
            return Qt.rect(side, top, w, h)
        if (count === 2)
            return Qt.rect(side + index * ((w + gap) / 2), top, (w - gap) / 2, h)
        if (count === 3) {
            if (index === 0)
                return Qt.rect(side, top, (w - gap) * 0.58, h)
            const x = side + (w - gap) * 0.58 + gap
            return Qt.rect(x, top + (index - 1) * ((h + gap) / 2), w - (x - side), (h - gap) / 2)
        }
        const columns = Math.ceil(Math.sqrt(count))
        const rows = Math.ceil(count / columns)
        const cw = (w - (columns - 1) * gap) / columns
        const ch = (h - (rows - 1) * gap) / rows
        return Qt.rect(side + (index % columns) * (cw + gap), top + Math.floor(index / columns) * (ch + gap), cw, ch)
    }

    ListModel { id: shellSurfaces }

    XdgShell {
        onToplevelCreated: (toplevel, xdgSurface) => {
            shellSurfaces.append({ shellSurface: xdgSurface, stashed: false, born: Date.now() })
            compositor.layoutVersion++
        }
        // A menu is a surface of its own. The stage gives it an item under the window
        // it belongs to and puts it on the table, so the agent can name it.
        onPopupCreated: (popup, xdgSurface) => compositor.stage.addPopup(xdgSurface)
    }
    // Ask applications to leave window decoration to the stage, which draws one consistent
    // frame around every application. Clients that do not speak the protocol keep their own.
    XdgDecorationManagerV1 {
        preferredMode: XdgToplevel.ServerSideDecoration
    }
    // Applications that take text through an input method can be given characters the
    // keymap has no key for. Without one of these, such a character is refused, not dropped.
    TextInputManager {}
    TextInputManagerV3 {}
    QtTextInputMethodManager {}

    WaylandOutput {
        id: output
        sizeFollowsWindow: true
        window: Window {
            id: win
            // The screen is chosen before the window is shown, so it opens where it belongs
            // rather than opening on the first screen and moving.
            screen: Qt.application.screens[compositor.screenIndex]
                    ?? Qt.application.screens[0]
            width: compositor.initialWidth
            height: compositor.initialHeight
            x: win.screen ? win.screen.virtualX : 0
            y: win.screen ? win.screen.virtualY : 0
            visible: true
            visibility: compositor.fullscreen ? Window.FullScreen : Window.Windowed
            title: "Archie Stage"
            color: "#04070c"

            Component.onCompleted: {
                compositor.stage.window = win
                compositor.stage.setOutputSize(width, height)
            }
            onWidthChanged: compositor.stage.setOutputSize(width, height)
            onHeightChanged: compositor.stage.setOutputSize(width, height)

            // The desk: everything the camera frames. It zooms and pans as one piece, so a
            // window and the agent's work on it never drift apart.
            Item {
                id: desk
                width: win.width
                height: win.height
                Component.onCompleted: compositor.stage.desk = desk
                transform: [
                    Translate { x: -compositor.stage.viewX; y: -compositor.stage.viewY },
                    Scale { xScale: compositor.stage.viewZoom; yScale: compositor.stage.viewZoom }
                ]

                // The desk's own ground is below everything, the desktop included.
                Rectangle {
                    z: -3
                    anchors.fill: parent
                    gradient: Gradient {
                        GradientStop { position: 0.0; color: "#07101a" }
                        GradientStop { position: 1.0; color: "#030509" }
                    }
                }
                // A faint grid, so a zoom reads as moving closer rather than things growing.
                Repeater {
                    model: Math.ceil(win.width / 64)
                    Rectangle { required property int index; z: -3; x: index * 64; width: 1; height: win.height; color: "#0a1420" }
                }
                Repeater {
                    model: Math.ceil(win.height / 64)
                    Rectangle { required property int index; z: -3; y: index * 64; height: 1; width: win.width; color: "#0a1420" }
                }

                Repeater {
                    id: frames
                    model: shellSurfaces
                    delegate: Item {
                        id: frame
                        required property var shellSurface
                        required property bool stashed
                        required property int index
                        readonly property bool desktopRole: compositor.isDesktop(frame.shellSurface)
                        readonly property var slot: compositor.windowSlot(index)
                        readonly property rect tile: frame.desktopRole
                            ? Qt.rect(0, 0, win.width, win.height)
                            : compositor.tileFor(frame.slot.slot, frame.slot.count, win.width, win.height)
                        readonly property bool focused: compositor.stage.focusedItem === surfaceItem
                        // The desktop's own top: its header and its edge are the shell's.
                        readonly property int header: frame.desktopRole ? 0 : 35
                        readonly property int edgeWidth: frame.desktopRole ? 0 : 1
                        z: frame.desktopRole ? -1 : 0
                        x: tile.x
                        y: tile.y
                        width: tile.width
                        height: tile.height
                        // On the shelf: out of sight, still a live window where it was. Its
                        // place and size do not change, so nothing Archie is doing in it is
                        // refused for a window that moved.
                        opacity: frame.stashed ? 0 : 1
                        enabled: !frame.stashed
                        Behavior on opacity { NumberAnimation { duration: 240; easing.type: Easing.OutCubic } }
                        onFocusedChanged: {
                            if (frame.desktopRole)
                                compositor.desktopFocused = frame.focused
                            else if (frame.focused)
                                compositor.setStashed(frame.index, false)
                        }
                        Connections {
                            target: compositor
                            function onDesktopFocusedChanged() {
                                if (compositor.desktopFocused && !frame.desktopRole)
                                    compositor.setStashed(frame.index, true)
                            }
                        }
                        // Whatever Archie is working in comes back into view.
                        Connections {
                            target: frame.stashed ? compositor.stage : null
                            function onFrameChanged() {
                                // The desktop's own windows share its app id: acting on the desktop
                                // is not acting on them.
                                if (compositor.stage.captionOpacity > 0.5 && compositor.stage.captionVerb !== "refused"
                                        && compositor.stage.captionApp === compositor.labelOf(frame.appId()) && !frame.desktopRole
                                        && frame.appId() !== compositor.desktopApp)
                                    compositor.setStashed(frame.index, false)
                            }
                        }
                        Behavior on x { NumberAnimation { duration: 380; easing.type: Easing.OutCubic } }
                        Behavior on y { NumberAnimation { duration: 380; easing.type: Easing.OutCubic } }
                        Behavior on width { NumberAnimation { duration: 380; easing.type: Easing.OutCubic } }
                        Behavior on height { NumberAnimation { duration: 380; easing.type: Easing.OutCubic } }

                        function appId() {
                            return frame.shellSurface && frame.shellSurface.toplevel ? frame.shellSurface.toplevel.appId : ""
                        }
                        function title() {
                            const t = frame.shellSurface && frame.shellSurface.toplevel ? frame.shellSurface.toplevel.title : ""
                            return t.length ? t : (frame.appId().length ? frame.appId() : Tr.t("aplicación sin nombre"))
                        }
                        function sendSize() {
                            if (frame.shellSurface && frame.shellSurface.toplevel)
                                frame.shellSurface.toplevel.sendMaximized(Qt.size(Math.max(1, width - 2 * frame.edgeWidth),
                                                                                  Math.max(1, height - frame.header - frame.edgeWidth)))
                        }
                        onDesktopRoleChanged: sendSize()
                        onWidthChanged: sendSize()
                        onHeightChanged: sendSize()
                        onXChanged: compositor.stage.updateSurface(surfaceItem, appId())
                        onYChanged: compositor.stage.updateSurface(surfaceItem, appId())

                        // A window the way the shell draws one: its fill, its hairline, the three
                        // lights and the title in the middle of the header.
                        Rectangle {
                            visible: !frame.desktopRole
                            anchors.fill: parent
                            radius: 14
                            color: compositor.winFill
                            border.width: 1
                            border.color: frame.focused ? Qt.rgba(30 / 255, 192 / 255, 1, 0.55) : compositor.hair
                            Behavior on border.color { ColorAnimation { duration: 200 } }
                        }
                        Rectangle {
                            visible: !frame.desktopRole
                            x: 1; y: 34; width: frame.width - 2; height: 1
                            color: compositor.hair
                        }
                        Row {
                            visible: !frame.desktopRole
                            x: 14
                            y: 12
                            spacing: 8
                            Repeater {
                                model: ["#d9605a", "#d9a441", "#4fb67b"]
                                Rectangle {
                                    id: light
                                    required property string modelData
                                    required property int index
                                    width: 11; height: 11; radius: 5.5
                                    color: frame.focused || lightArea.containsMouse ? modelData : Qt.rgba(1, 1, 1, 0.17)
                                    // A person's hand only: close asks the application to close, the
                                    // middle one puts the window on the shelf.
                                    MouseArea {
                                        id: lightArea
                                        anchors.fill: parent
                                        anchors.margins: -3
                                        hoverEnabled: true
                                        enabled: compositor.stage.physicalInput && light.index < 2
                                        onClicked: {
                                            if (light.index === 0 && frame.shellSurface && frame.shellSurface.toplevel)
                                                frame.shellSurface.toplevel.sendClose()
                                            else if (light.index === 1)
                                                compositor.setStashed(frame.index, true)
                                        }
                                    }
                                }
                            }
                        }
                        Text {
                            visible: !frame.desktopRole
                            anchors.horizontalCenter: parent.horizontalCenter
                            y: 9
                            width: Math.min(implicitWidth, frame.width - 160)
                            elide: Text.ElideRight
                            text: frame.title()
                            color: frame.focused ? compositor.title1 : compositor.title2
                            font.family: compositor.sans
                            font.pixelSize: 13
                            font.weight: Font.DemiBold
                        }

                        // The surface sits in its own container. When a client acknowledges the
                        // maximized state, its item moves itself to the output's origin; inside this
                        // container that origin is the content area rather than the frame's corner,
                        // so the window cannot slide up over the header.
                        Item {
                            id: content
                            x: frame.edgeWidth
                            y: frame.header
                            width: frame.width - 2 * frame.edgeWidth
                            height: frame.height - frame.header - frame.edgeWidth
                            clip: true
                            ShellSurfaceItem {
                                id: surfaceItem
                                shellSurface: frame.shellSurface
                                autoCreatePopupItems: true
                                // Only a person in control reaches applications through the
                                // scene; the agent's input goes through the seat directly.
                                inputEventsEnabled: compositor.stage.physicalInput
                                onSurfaceDestroyed: {
                                    compositor.stage.unregisterSurface(surfaceItem)
                                    shellSurfaces.remove(frame.index)
                                    compositor.layoutVersion++
                                }
                                onXChanged: compositor.stage.updateSurface(surfaceItem, frame.appId())
                                onYChanged: compositor.stage.updateSurface(surfaceItem, frame.appId())
                                onWidthChanged: compositor.stage.updateSurface(surfaceItem, frame.appId())
                                onHeightChanged: compositor.stage.updateSurface(surfaceItem, frame.appId())
                                Component.onCompleted: {
                                    frame.sendSize()
                                    compositor.stage.registerSurface(surfaceItem, frame.appId())
                                }
                            }
                        }
                        Connections {
                            target: frame.shellSurface ? frame.shellSurface.toplevel : null
                            function onAppIdChanged() {
                                compositor.stage.updateSurface(surfaceItem, frame.appId())
                                compositor.layoutVersion++
                            }
                            function onTitleChanged() { compositor.layoutVersion++ }
                            // A window that asks to be maximised or full screen is answered with
                            // the whole output by Qt's own handling; its size is its frame's.
                            function onSetMaximized() { Qt.callLater(frame.sendSize) }
                            function onSetFullscreen() { Qt.callLater(frame.sendSize) }
                        }
                    }
                }
            }

            // ---- Screen space: never zoomed. ----

            // Archie's own presses move no keyboard focus, so where they land is watched instead:
            // a press on the desktop, outside every window, is the desktop in use too.
            Item {
                id: pressWatch
                // How bright the brightest ripple was last frame: a new press is one that is
                // brighter than that (the camera moves ripples, so their place says nothing).
                property real last: 0
                Connections {
                    target: compositor.hasDesktop ? compositor.stage : null
                    function onFrameChanged() {
                        const ripples = compositor.stage.ripples
                        // The newest press is the brightest ripple; a slow frame may already
                        // find it half faded.
                        let fresh = null
                        for (let i = 0; i < ripples.length; ++i)
                            if (!fresh || ripples[i].opacity > fresh.opacity)
                                fresh = ripples[i]
                        const brightest = fresh ? fresh.opacity : 0
                        const pressed = brightest > pressWatch.last + 0.05
                        pressWatch.last = brightest
                        // Only a press on the desktop's own app counts: a stroke in Photopea
                        // is Photopea's, wherever the camera has put it.
                        if (!pressed || compositor.stage.captionApp !== compositor.labelOf(compositor.desktopApp))
                            return
                        for (let i = 0; i < frames.count; ++i) {
                            const window = frames.itemAt(i)
                            if (!window || window.stashed || window.desktopRole)
                                continue
                            const at = window.mapFromItem(null, fresh.x, fresh.y)
                            if (at.x >= 0 && at.y >= 0 && at.x <= window.width && at.y <= window.height)
                                return
                        }

                        // A window that opened just now was opened by that press (World from
                        // the dock): it is what the press asked for, not what it set aside.
                        for (let i = 0; i < shellSurfaces.count; ++i)
                            if (!compositor.isDesktop(shellSurfaces.get(i).shellSurface)
                                    && Date.now() - shellSurfaces.get(i).born > 2000)
                                compositor.setStashed(i, true)
                    }
                }
            }

            // The shelf: windows that stepped aside while the desktop was in use, one press away.
            Row {
                visible: compositor.hasDesktop
                x: 24
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 96
                spacing: 8
                z: 2
                Repeater {
                    model: shellSurfaces
                    delegate: Rectangle {
                        id: shelved
                        required property var shellSurface
                        required property bool stashed
                        required property int index
                        visible: shelved.stashed && !compositor.isDesktop(shelved.shellSurface)
                        width: shelvedText.implicitWidth + 30
                        height: 30
                        radius: 15
                        color: shelfArea.containsMouse ? "#ee101015" : "#cc050507"
                        border.color: compositor.hair
                        Row {
                            anchors.centerIn: parent
                            spacing: 7
                            Rectangle { width: 6; height: 6; radius: 3; color: compositor.accent; anchors.verticalCenter: parent.verticalCenter }
                            Text {
                                id: shelvedText
                                text: {
                                    void compositor.layoutVersion
                                    const top = shelved.shellSurface && shelved.shellSurface.toplevel
                                    const title = top ? shelved.shellSurface.toplevel.title : ""
                                    const name = title.split(/ [|—–-] /)[0].trim()
                                    return name.length ? name : (top ? shelved.shellSurface.toplevel.appId : "")
                                }
                                color: compositor.title2
                                font.family: compositor.sans
                                font.pixelSize: 12
                            }
                        }
                        MouseArea {
                            id: shelfArea
                            anchors.fill: parent
                            hoverEnabled: true
                            onClicked: compositor.setStashed(shelved.index, false)
                        }
                    }
                }
            }

            // Over the desktop, who holds the stage and the newest thing Archie did, in the corner
            // beside its dock; its top bar keeps the shell's own status.
            Row {
                visible: compositor.hasDesktop && compositor.stage.surfaceCount > 1
                anchors.right: parent.right
                anchors.rightMargin: 24
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 44
                spacing: 8
                z: 2
                // The newest thing Archie did, beside who holds the stage.
                Repeater {
                    model: compositor.stage.activity.slice(0, 1)
                    delegate: Rectangle {
                        id: lastChip
                        required property var modelData
                        opacity: modelData.opacity
                        width: lastRow.implicitWidth + 18
                        height: 26
                        radius: 13
                        color: "#cc050507"
                        border.color: compositor.hair
                        Row {
                            id: lastRow
                            anchors.centerIn: parent
                            spacing: 6
                            Rectangle {
                                width: 6; height: 6; radius: 3
                                anchors.verticalCenter: parent.verticalCenter
                                color: lastChip.modelData.verb === "refused" ? compositor.amber
                                       : lastChip.modelData.verb === "interrupted" ? compositor.red : compositor.green
                            }
                            Text { text: compositor.activityText(lastChip.modelData); color: compositor.title2; font.pixelSize: 11 }
                        }
                    }
                }
                Rectangle {
                    width: deskPillText.implicitWidth + 22
                    height: 26
                    radius: 13
                    color: compositor.hasDesktop ? "#cc050507" : "transparent"
                    border.color: compositor.authorityTint()
                    Text {
                        id: deskPillText
                        anchors.centerIn: parent
                        text: compositor.authorityLabel()
                        color: compositor.authorityTint()
                        font.family: "monospace"
                        font.pixelSize: 11
                        font.bold: true
                        font.letterSpacing: 1.5
                    }
                }
            }

            // The status line.
            Rectangle {
                visible: !compositor.hasDesktop
                width: parent.width
                height: 48
                color: "#cc070c12"
                Row {
                    id: brand
                    anchors.verticalCenter: parent.verticalCenter
                    x: 24
                    spacing: 18
                    Image { source: "qrc:/archie/brand/archieos-logo-dark.png"; height: 22; fillMode: Image.PreserveAspectFit; smooth: true; mipmap: true; sourceSize.height: 88; anchors.verticalCenter: parent.verticalCenter }
                    Text { text: "STAGE"; color: compositor.dim; font.family: "monospace"; font.pixelSize: 12; font.letterSpacing: 3; anchors.verticalCenter: parent.verticalCenter }
                }
                // What Archie did lately, newest first: someone who looks up, or who has just
                // taken over, can see the last few actions without a log. Verbs and
                // applications only, like the caption.
                Item {
                    anchors.left: brand.right
                    anchors.leftMargin: 28
                    anchors.right: statusRight.left
                    anchors.rightMargin: 20
                    height: parent.height
                    clip: true
                    Row {
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 8
                        Repeater {
                            model: compositor.stage.activity
                            delegate: Rectangle {
                                id: chip
                                required property var modelData
                                required property int index
                                readonly property color tint: modelData.verb === "refused" ? compositor.amber
                                                              : modelData.verb === "interrupted" ? compositor.red : compositor.green
                                // Older entries recede, so the newest reads first.
                                opacity: modelData.opacity * Math.max(0.35, 1.0 - index * 0.16)
                                width: chipRow.implicitWidth + 18
                                height: 24
                                radius: 12
                                color: "#0f1822"
                                border.color: compositor.edge
                                Row {
                                    id: chipRow
                                    anchors.centerIn: parent
                                    spacing: 6
                                    Rectangle { width: 6; height: 6; radius: 3; color: chip.tint; anchors.verticalCenter: parent.verticalCenter }
                                    Text {
                                        text: compositor.activityText(chip.modelData)
                                        color: compositor.soft
                                        font.pixelSize: 11
                                    }
                                }
                            }
                        }
                    }
                }
                Row {
                    id: statusRight
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.right: parent.right
                    anchors.rightMargin: 24
                    spacing: 14
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: compositor.stage.surfaceCount === 1 ? Tr.t("1 aplicación") : compositor.stage.surfaceCount + Tr.t(" aplicaciones")
                        color: compositor.dim
                        font.family: "monospace"
                        font.pixelSize: 11
                    }
                    Rectangle {
                        width: authorityText.implicitWidth + 22
                        height: 26
                        radius: 13
                        color: compositor.hasDesktop ? "#cc050507" : "transparent"
                        border.color: compositor.authorityTint()
                        Text {
                            id: authorityText
                            anchors.centerIn: parent
                            text: compositor.authorityLabel()
                            color: compositor.authorityTint()
                            font.family: "monospace"
                            font.pixelSize: 11
                            font.bold: true
                            font.letterSpacing: 1.5
                        }
                    }
                }
            }

            // An empty stage says what it is waiting for, instead of looking broken.
            Column {
                visible: compositor.stage.surfaceCount === 0
                anchors.centerIn: parent
                spacing: 10
                Image { anchors.horizontalCenter: parent.horizontalCenter; source: "qrc:/archie/brand/archieos-logo-dark.png"; height: 64; fillMode: Image.PreserveAspectFit; smooth: true; mipmap: true; sourceSize.height: 256 }
                Text { anchors.horizontalCenter: parent.horizontalCenter; text: "Stage"; color: compositor.ink; font.pixelSize: 24; font.weight: Font.Light; font.letterSpacing: 3 }
                Text { anchors.horizontalCenter: parent.horizontalCenter; text: Tr.t("Las aplicaciones que Archie use aparecerán aquí."); color: compositor.soft; font.pixelSize: 16 }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: "WAYLAND_DISPLAY=" + compositor.socketName + (compositor.stage.listening ? Tr.t("  ·  control listo") : Tr.t("  ·  sin control: Archie no puede actuar"))
                    color: compositor.dim
                    font.family: "monospace"
                    font.pixelSize: 12
                }
                Text { anchors.horizontalCenter: parent.horizontalCenter; text: Tr.t("Mueve el mouse o pulsa una tecla para tomar el control · Ctrl+Alt+Esc pausa · Ctrl+Alt+Enter reanuda"); color: compositor.dim; font.pixelSize: 12 }
            }

            // Ripples: a press that happened, expanding once.
            Repeater {
                model: compositor.stage.ripples
                delegate: Rectangle {
                    required property var modelData
                    x: modelData.x - modelData.r
                    y: modelData.y - modelData.r
                    width: modelData.r * 2
                    height: modelData.r * 2
                    radius: modelData.r
                    color: "transparent"
                    border.width: 3
                    border.color: compositor.cyan
                    opacity: modelData.opacity
                }
            }

            // A drag being carried.
            Shape {
                anchors.fill: parent
                visible: compositor.stage.dragging && compositor.stage.cursorVisible
                ShapePath {
                    strokeColor: compositor.cyan
                    strokeWidth: 2
                    strokeStyle: ShapePath.DashLine
                    dashPattern: [4, 3]
                    fillColor: "transparent"
                    startX: compositor.stage.dragFromX
                    startY: compositor.stage.dragFromY
                    PathLine { x: compositor.stage.cursorX; y: compositor.stage.cursorY }
                }
            }

            // The agent's cursor. Only while it holds the stage.
            Item {
                x: compositor.stage.cursorX
                y: compositor.stage.cursorY
                visible: compositor.stage.cursorVisible
                opacity: compositor.stage.cursorOpacity
                Rectangle {
                    x: -22; y: -22; width: 44; height: 44; radius: 22
                    color: compositor.cyan
                    opacity: compositor.stage.cursorPressed ? 0.35 : 0.0
                    Behavior on opacity { NumberAnimation { duration: 90 } }
                }
                Shape {
                    ShapePath {
                        strokeColor: "#04070c"
                        strokeWidth: 2
                        fillColor: compositor.ink
                        startX: 0; startY: 0
                        PathLine { x: 0; y: 28 }
                        PathLine { x: 7; y: 21 }
                        PathLine { x: 12; y: 33 }
                        PathLine { x: 18; y: 30 }
                        PathLine { x: 13; y: 19 }
                        PathLine { x: 23; y: 19 }
                        PathLine { x: 0; y: 0 }
                    }
                }
                Rectangle {
                    x: 20; y: 28; width: badge.implicitWidth + 12; height: 18; radius: 9
                    color: "#cc0b1119"; border.color: compositor.cyan
                    Text { id: badge; anchors.centerIn: parent; text: "Archie"; color: compositor.cyan; font.pixelSize: 10; font.bold: true }
                }
            }

            // The caption.
            Rectangle {
                visible: compositor.stage.captionOpacity > 0.01
                opacity: compositor.stage.captionOpacity
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.bottom: parent.bottom
                anchors.bottomMargin: compositor.hasDesktop ? 124 : 30
                width: Math.min(parent.width - 80, captionLabel.implicitWidth + 48)
                height: 52
                radius: 26
                color: "#e60b1119"
                border.color: compositor.stage.captionVerb === "refused" ? compositor.amber : "#3c5169"
                Text {
                    id: captionLabel
                    anchors.centerIn: parent
                    text: "Archie · " + compositor.captionText()
                    color: compositor.ink
                    font.pixelSize: 19
                }
            }

            // What Archie says on its own while it works, as a subtitle while it is said. The
            // voice writes each line whole to a file; it shows for about as long as it takes
            // to say, and the newest line replaces the one before.
            Item {
                id: subtitle
                property string text: ""
                property double shownAt: 0
                property double lastAt: 0
                property double now: 0
                readonly property double showFor: Math.min(14000, 2200 + subtitle.text.length * 75)
                readonly property bool shown: subtitle.text.length > 0 && subtitle.now - subtitle.shownAt < subtitle.showFor
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.bottom: parent.bottom
                anchors.bottomMargin: (compositor.hasDesktop ? 124 : 30) + 66
                width: Math.min(parent.width * 0.72, subtitleText.implicitWidth + 64)
                height: subtitleText.implicitHeight + 40
                opacity: subtitle.shown ? 1 : 0
                visible: opacity > 0.01
                Behavior on opacity { NumberAnimation { duration: 260 } }
                Timer {
                    interval: 250
                    repeat: true
                    running: compositor.narrationUrl.length > 0
                    onTriggered: {
                        subtitle.now = Date.now()
                        const request = new XMLHttpRequest()
                        request.onreadystatechange = function() {
                            if (request.readyState !== XMLHttpRequest.DONE || request.responseText.length === 0)
                                return
                            let line
                            try { line = JSON.parse(request.responseText) } catch (e) { return }
                            if (!line || typeof line.text !== "string" || typeof line.at_ms !== "number")
                                return
                            if (line.at_ms !== subtitle.lastAt) {
                                subtitle.lastAt = line.at_ms
                                subtitle.text = line.text
                                // A line written while the stage was not looking is not news.
                                subtitle.shownAt = Math.min(Date.now(), line.at_ms)
                            }
                        }
                        request.open("GET", compositor.narrationUrl)
                        request.send()
                    }
                }
                Rectangle {
                    anchors.fill: parent
                    radius: 16
                    color: "#e6050507"
                    border.color: compositor.hair
                }
                Text {
                    x: 32
                    y: -9
                    text: "ARCHIE"
                    color: compositor.accent
                    font.family: "monospace"
                    font.pixelSize: 11
                    font.bold: true
                    font.letterSpacing: 2
                    style: Text.Outline
                    styleColor: "#050507"
                }
                Text {
                    id: subtitleText
                    anchors.centerIn: parent
                    width: Math.min(implicitWidth, compositor.stage ? win.width * 0.72 - 64 : implicitWidth)
                    wrapMode: Text.Wrap
                    horizontalAlignment: Text.AlignHCenter
                    text: subtitle.text
                    color: compositor.title1
                    font.family: compositor.sans
                    font.pixelSize: 22
                    lineHeight: 1.15
                }
            }

            // The banner: whenever the agent does not hold the stage.
            Rectangle {
                visible: compositor.stage.banner.length > 0
                opacity: compositor.stage.bannerOpacity
                anchors.horizontalCenter: parent.horizontalCenter
                y: 64
                width: bannerLabel.implicitWidth + 56
                height: 54
                radius: 10
                color: compositor.stage.banner === "paused" ? "#ee2b1216" : "#ee2a1f07"
                border.color: compositor.stage.banner === "paused" ? compositor.red : compositor.amber
                Text {
                    id: bannerLabel
                    anchors.centerIn: parent
                    text: compositor.stage.banner === "paused"
                          ? Tr.t("PAUSADO · Archie no actuará hasta que pulses Ctrl+Alt+Enter")
                          : Tr.t("CONTROL HUMANO · Archie retoma cuando dejes de usar el mouse y el teclado")
                    color: compositor.stage.banner === "paused" ? compositor.red : compositor.amber
                    font.pixelSize: 19
                }
            }
        }
    }
}
