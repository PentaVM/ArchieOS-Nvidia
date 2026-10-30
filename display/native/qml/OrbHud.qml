import QtQuick
import "qrc:/archie/shell/prototype" as Shell
import "../../shell/prototype/Lang.js" as Tr

// What sits over the orb: almost nothing. The orb is configured by Cortex, not by
// panels, so this layer only keeps the hands (drag to turn, scroll to approach, click to
// select, double click to open a module), a name beside the pointer, and one quiet line
// saying what Archie is doing and what it is using.
Item {
    id: hud
    // The project's typeface (the shell embeds it); the HUD used the system monospace.
    readonly property string family: inter.status === FontLoader.Ready ? inter.font.family : "sans-serif"
    FontLoader { id: inter; source: "qrc:/archie/shell/fonts/inter/Inter-Variable.ttf" }
    required property var controller
    property bool navigationEnabled: true

    required property var view
    required property var orbScene
    property bool labelsVisible: true
    property bool lightMode: Shell.Theme.light
    property color textColor: Shell.Theme.t1
    property color secondaryColor: Shell.Theme.t2
    property color mutedColor: Shell.Theme.t3
    property color errorColor: Shell.Theme.bad

    function pickAt(x, y) {
        if (!hud.view.camera) return ""
        const near = hud.view.mapTo3DScene(Qt.vector3d(x, y, 0))
        const far = hud.view.mapTo3DScene(Qt.vector3d(x, y, 4000))
        const field = hud.orbScene.field
        return hud.controller.pick(field.mapPositionFromScene(near), field.mapPositionFromScene(far))
    }

    readonly property var s: hud.controller.scene
    readonly property string mode: Number(s.speak || 0) > 0.05 ? Tr.t("Hablando")
                                   : Number(s.think || 0) > 0.05 ? Tr.t("Pensando")
                                   : Number(s.listen || 0) > 0.05 ? Tr.t("Escuchando") : Tr.t("En reposo")
    readonly property color modeColour: mode === Tr.t("Hablando") ? Shell.Theme.info
        : mode === Tr.t("Pensando") ? Shell.Theme.warn : mode === Tr.t("Escuchando") ? Shell.Theme.ok : mutedColor

    HoverHandler {
        id: hover
        enabled: hud.visible
        onPointChanged: hoverDelay.restart()
        onHoveredChanged: if (!hovered) hud.controller.hover("")
    }
    Timer {
        id: hoverDelay
        interval: 40
        onTriggered: hud.controller.hover(hover.hovered ? hud.pickAt(hover.point.position.x, hover.point.position.y) : "")
    }

    MouseArea {
        anchors.fill: parent
        enabled: hud.visible
        property point last
        property point origin
        property bool moved: false
        onPressed: function (mouse) { last = Qt.point(mouse.x, mouse.y); origin = last; moved = false }
        onPositionChanged: function (mouse) {
            if (!hud.navigationEnabled) return
            if (Math.abs(mouse.x - origin.x) + Math.abs(mouse.y - origin.y) > 5)
                moved = true
            if (moved) {
                hud.orbScene.yaw -= (mouse.x - last.x) * 0.3
                hud.orbScene.pitch = Math.max(-80, Math.min(80, hud.orbScene.pitch - (mouse.y - last.y) * 0.3))
            }
            last = Qt.point(mouse.x, mouse.y)
        }
        onClicked: function (mouse) {
            if (moved) return
            const id = hud.pickAt(mouse.x, mouse.y)
            if (id.length > 0) hud.controller.select(id)
            else hud.controller.clearSelection()
        }
        onDoubleClicked: function (mouse) {
            const id = hud.pickAt(mouse.x, mouse.y)
            if (id.length > 0) hud.controller.toggleModule(id)
        }
        onWheel: function (wheel) {
            if (!hud.navigationEnabled) { wheel.accepted=false; return }
            hud.orbScene.approach(Math.pow(0.88, wheel.angleDelta.y / 120))
        }
    }

    // The content sphere's labels, as on Xavi's desktop: the top-level folders in mono
    // capitals beside their orbs, and the root's name with what the folder holds.
    Item {
        id: contentLabels
        anchors.fill: parent
        visible: hud.labelsVisible && !!hud.controller.look.content && !!hud.s.contentName
        property var places: []
        // Positions follow the field every frame; the delegates stay, only their places move.
        Connections {
            target: hud.controller
            enabled: contentLabels.visible
            function onFrameAdvanced() { contentLabels.places = hud.controller.labelAnchors() }
        }
        function screenOf(x, y, z) {
            return hud.view.mapFrom3DScene(hud.orbScene.field.mapPositionToScene(Qt.vector3d(x, y, z)))
        }
        function sizeText(bytes) {
            if (bytes >= 1e9) return Math.round(bytes / 1e9) + " GB"
            if (bytes >= 1e6) return Math.round(bytes / 1e6) + " MB"
            return Math.max(1, Math.round(bytes / 1e3)) + " KB"
        }
        Repeater {
            model: contentLabels.places.length
            Text {
                readonly property var place: contentLabels.places[index] || ({ x: 0, y: 0, z: 0, name: "" })
                readonly property vector3d at: contentLabels.screenOf(place.x, place.y, place.z)
                visible: at.z > 0
                x: at.x + 7
                y: at.y - height / 2
                text: String(place.name).toUpperCase()
                color: hud.textColor
                font.family: hud.family
                font.pixelSize: 11
                font.letterSpacing: 0.4
                Rectangle { anchors.fill: parent; anchors.margins: -3; z: -1; radius: 3; color: Shell.Theme.panel; border.color: Shell.Theme.hair }
            }
        }
        Column {
            // Re-read with every frame's places, so it follows the camera like the folders.
            readonly property vector3d at: contentLabels.places.length >= 0 ? contentLabels.screenOf(0, 0, 0) : Qt.vector3d(0, 0, -1)
            visible: at.z > 0
            x: at.x + 52
            y: at.y - 62
            spacing: 3
            Text {
                text: String(hud.s.contentName || "").toUpperCase()
                color: hud.secondaryColor
                font.family: hud.family
                font.pixelSize: 13
                font.weight: Font.DemiBold
                font.letterSpacing: 0.6
                Rectangle { anchors.fill: parent; anchors.margins: -3; z: -1; radius: 3; color: Shell.Theme.panel; border.color: Shell.Theme.hair }
            }
            Rectangle { width: 170; height: 1; color: hud.secondaryColor; opacity: 0.55 }
            Text {
                text: Number(hud.s.contentElements || 0).toFixed(0) + Tr.t(" ELEMENTOS  ·  ")
                      + contentLabels.sizeText(Number(hud.s.contentBytes || 0))
                color: hud.secondaryColor
                font.family: hud.family
                font.pixelSize: 11
                Rectangle { anchors.fill: parent; anchors.margins: -3; z: -1; radius: 3; color: Shell.Theme.panel; border.color: Shell.Theme.hair }
            }
        }
    }

    // A name beside the pointer, nothing more.
    Text {
        visible: hud.labelsVisible && !!hud.controller.hovered.id && hover.hovered
        x: Math.min(parent.width - width - 12, hover.point.position.x + 18)
        y: Math.max(12, hover.point.position.y - 26)
        text: hud.controller.hovered.name || ""
        color: hud.textColor
        font.pixelSize: 13
        font.weight: Font.Medium
        Rectangle { anchors.fill: parent; anchors.margins: -6; z: -1; radius: Shell.Theme.rCard; color: Shell.Theme.panel; border.color: Shell.Theme.hair }
    }

    // The one line: state, then what is lit and why.
    Rectangle {
        anchors.fill: stateLine; anchors.margins: -9; radius: Shell.Theme.rCard
        visible: stateLine.visible; color: Shell.Theme.panel; border.color: Shell.Theme.hair
    }
    Row {
        id: stateLine
        visible: hud.labelsVisible
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 34
        spacing: 12

        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: 7; height: 7; radius: 3.5
            color: hud.modeColour
            SequentialAnimation on opacity {
                running: hud.visible && hud.labelsVisible && !hud.controller.reducedMotion && hud.mode !== Tr.t("En reposo")
                loops: 3
                NumberAnimation { to: 0.35; duration: 700; easing.type: Easing.InOutSine }
                NumberAnimation { to: 1.0; duration: 700; easing.type: Easing.InOutSine }
            }
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: hud.mode
            color: hud.secondaryColor
            font.pixelSize: 13
            font.weight: Font.Medium
            font.letterSpacing: 0.3
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            visible: hud.controller.attention.length > 0
            text: "·  " + hud.controller.attention.slice(0, 3).map(function (a) { return a.name }).join("  ·  ")
            color: hud.mutedColor
            font.pixelSize: 13
        }
    }

    Text {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 14
        visible: hud.labelsVisible && hud.controller.error.length > 0
        text: Tr.t("Sin conexión con el observador del sistema")
        color: hud.errorColor
        font.pixelSize: 11
        Rectangle { anchors.fill: parent; anchors.margins: -4; z: -1; radius: 4; color: Shell.Theme.panel; border.color: Shell.Theme.hair }
    }
}
