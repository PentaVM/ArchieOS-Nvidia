import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "qrc:/archie/shell/prototype" as Shell
import "WorldTools.js" as Tools
import "../../shell/prototype/Lang.js" as Tr

// Xavi (ByXavi270 / XaviCottier): shell design; PentaVM: native implementation.
// World's minimal HUD: a search field, a navigation rail and panels that open only when
// asked (layers, ruler, details, console). The globe keeps the whole window. The full
// projection reader is still WorldDetails; it lives here as the Details drawer so its job
// observation and camera rules keep running while it is closed.
Item {
    id: hud
    required property var controller
    property var stream: null
    property bool worldActive: false
    // The View3D, for the ruler's ray. Null in tests: the ruler then stays off.
    property var view: null
    // The native WorldConsole, when the host configured one. Null: no name search, no CLI.
    property var cli: typeof worldConsole !== "undefined" ? worldConsole : null

    property alias selectedPointId: details.selectedPointId
    property alias cameraOwner: details.cameraOwner
    readonly property alias imageryCaption: details.imageryCaption
    readonly property alias selectedPoint: details.selectedPoint
    readonly property alias selectedJob: details.selectedJob
    readonly property alias imageSources: details.imageSources
    readonly property alias activeImage: details.activeImage
    function cedeCamera() { details.cedeCamera() }
    function applyCamera(job) { return details.applyCamera(job) }

    // Which one panel is open: "", "layers", "details", "console". The ruler is a mode.
    property string panel: ""
    property bool measuring: false
    property var marks: []
    readonly property var measured: marks.length === 2 ? Tools.distance(marks[0], marks[1]) : null
    property bool tilted: false
    property string searchNote: ""
    property var places: []
    property bool awaitingPlaces: false

    function toggle(name) { panel = panel === name ? "" : name }
    function fly(latitude, longitude, radius, label) {
        details.cedeCamera()
        details.cameraOwner = label ? Tr.t("Búsqueda · ") + label : Tr.t("Búsqueda")
        controller.frameExploration(latitude, longitude, radius, 0, Shell.Theme.spatial)
    }
    function search(text) {
        searchNote = ""; places = []
        const query = text.trim()
        if (!query.length) return
        const point = Tools.parseCoordinates(query)
        if (point) { fly(point.latitude, point.longitude, 25000, point.latitude.toFixed(4) + ", " + point.longitude.toFixed(4)); return }
        if (!hud.cli) { searchNote = Tr.t("Buscar por nombre necesita Core. Escribe latitud, longitud."); return }
        if (hud.cli.busy) { searchNote = Tr.t("La consola está ocupada; espera su respuesta."); return }
        awaitingPlaces = true
        hud.cli.submit("world resolve " + query)
    }
    // Core wraps the resolver's answer; the places list is found wherever it sits.
    function findPlaces(value, depth) {
        if (!value || typeof value !== "object" || depth > 4) return null
        if (Array.isArray(value.places)) return value.places
        for (const key in value) { const found = findPlaces(value[key], depth + 1); if (found) return found }
        return null
    }
    Connections {
        target: hud.cli
        ignoreUnknownSignals: true
        function onLastResultChanged() {
            if (!hud.awaitingPlaces) return
            hud.awaitingPlaces = false
            const found = hud.findPlaces(hud.cli.lastResult, 0) || []
            hud.places = found.filter(function(p) { return Tools.valid(p) }).slice(0, 5)
            hud.searchNote = hud.places.length ? "" : Tr.t("Sin lugares para esa búsqueda.")
        }
        function onErrorChanged() {
            if (!hud.awaitingPlaces || !hud.cli.error) return
            hud.awaitingPlaces = false
            hud.searchNote = hud.cli.error
        }
    }
    // The ruler reads the globe under the pointer from the camera's own ray.
    function surfaceAt(x, y) {
        if (!hud.view) return null
        const near = hud.view.mapTo3DScene(Qt.vector3d(x, y, 0))
        const far = hud.view.mapTo3DScene(Qt.vector3d(x, y, 1))
        const hit = Tools.surfacePoint(near, {x: far.x - near.x, y: far.y - near.y, z: far.z - near.z})
        return hit ? Tools.coordinates(hit) : null
    }
    readonly property var eye: controller.frame ? controller.frame.cameraEye : null
    readonly property var below: eye ? Tools.coordinates(eye) : null
    readonly property real altitudeKm: eye ? Math.max(0, (Math.sqrt(eye.x * eye.x + eye.y * eye.y + eye.z * eye.z) - 20) * 6371.0088 / 20) : 0
    onWorldActiveChanged: if (!worldActive) { panel = ""; measuring = false }

    // ---- Details drawer: the full reader, unchanged.
    WorldDetails {
        id: details
        controller: hud.controller
        stream: hud.stream
        worldActive: hud.worldActive
        width: Math.min(380, hud.width * 0.38)
        anchors.top: parent.top; anchors.bottom: parent.bottom; anchors.margins: 16
        anchors.topMargin: 72
        x: hud.panel === "details" ? 16 : -width - 24
        visible: x > -width - 20
        Behavior on x { enabled: Shell.Theme.spatial; NumberAnimation { duration: Shell.Theme.durPanel; easing.type: Easing.OutCubic } }
    }

    // ---- Search, top left.
    Rectangle {
        id: searchBox
        objectName: "world.search"
        x: 16; y: 16
        width: Math.min(400, hud.width - 120); height: 44; radius: 22
        color: Shell.Theme.reading; border.color: field.activeFocus ? Shell.Theme.acc : Shell.Theme.hair
        RowLayout {
            anchors.fill: parent; anchors.leftMargin: 14; anchors.rightMargin: 8; spacing: 8
            Shell.Icon { name: "search"; size: 16; color: Shell.Theme.t2 }
            TextField {
                id: field
                Layout.fillWidth: true
                placeholderText: Tr.t("Buscar un lugar o «lat, lon»")
                placeholderTextColor: Shell.Theme.t3
                color: Shell.Theme.t1; font.family: Shell.Theme.font; font.pixelSize: 14
                background: null
                maximumLength: 256
                onAccepted: hud.search(text)
                Keys.onEscapePressed: { text = ""; hud.places = []; hud.searchNote = ""; focus = false }
            }
            BusyIndicator { visible: hud.awaitingPlaces; running: visible; Layout.preferredWidth: 22; Layout.preferredHeight: 22 }
        }
    }
    Rectangle {
        visible: hud.places.length > 0 || hud.searchNote.length > 0
        anchors.left: searchBox.left; anchors.top: searchBox.bottom; anchors.topMargin: 6
        width: searchBox.width; height: results.implicitHeight + 12; radius: Shell.Theme.rCard
        color: Shell.Theme.reading; border.color: Shell.Theme.hair
        Column {
            id: results
            x: 6; y: 6; width: parent.width - 12
            Text {
                visible: hud.searchNote.length > 0; width: parent.width; padding: 8
                text: hud.searchNote; wrapMode: Text.Wrap; textFormat: Text.PlainText
                color: Shell.Theme.t2; font.family: Shell.Theme.font; font.pixelSize: 12
            }
            Repeater {
                model: hud.places
                delegate: Rectangle {
                    required property var modelData
                    width: results.width; height: 40; radius: 8
                    color: placeArea.containsMouse ? Shell.Theme.hover : "transparent"
                    Column {
                        anchors.verticalCenter: parent.verticalCenter; x: 10; width: parent.width - 20
                        Text { width: parent.width; elide: Text.ElideRight; textFormat: Text.PlainText; text: modelData.name || Tr.t("Sin nombre"); color: Shell.Theme.t1; font.family: Shell.Theme.font; font.pixelSize: 13 }
                        Text { text: modelData.latitude.toFixed(4) + ", " + modelData.longitude.toFixed(4) + " · " + (modelData.source || ""); color: Shell.Theme.t3; font.family: Shell.Theme.font; font.pixelSize: 10 }
                    }
                    MouseArea {
                        id: placeArea; anchors.fill: parent; hoverEnabled: true
                        onClicked: { hud.fly(modelData.latitude, modelData.longitude, 12000, modelData.name); hud.places = [] }
                    }
                }
            }
        }
    }

    // ---- Navigation rail, right edge.
    component RailButton: Rectangle {
        id: rb
        property string glyph: ""
        property string icon: ""
        property string tip: ""
        property bool active: false
        property bool enabledHere: true
        signal pressed()
        width: 40; height: 40; radius: 12
        opacity: enabledHere ? 1 : 0.45
        color: active ? Shell.Theme.sel : rba.containsMouse ? Shell.Theme.hover : "transparent"
        Accessible.role: Accessible.Button
        Accessible.name: tip
        Shell.Icon { visible: rb.icon.length > 0; anchors.centerIn: parent; name: rb.icon; size: 18; color: rb.active ? Shell.Theme.acc : Shell.Theme.t1 }
        Text { visible: rb.icon.length === 0; anchors.centerIn: parent; text: rb.glyph; color: rb.active ? Shell.Theme.acc : Shell.Theme.t1; font.family: Shell.Theme.disp; font.pixelSize: 18; font.weight: Font.DemiBold }
        MouseArea { id: rba; anchors.fill: parent; hoverEnabled: true; enabled: rb.enabledHere; onClicked: rb.pressed() }
        ToolTip.visible: rba.containsMouse && tip.length > 0
        ToolTip.text: tip
        ToolTip.delay: 400
    }
    Rectangle {
        id: rail
        objectName: "world.rail"
        anchors.right: parent.right; anchors.rightMargin: 16
        anchors.verticalCenter: parent.verticalCenter
        width: 48; height: railColumn.implicitHeight + 8; radius: 16
        color: Shell.Theme.reading; border.color: Shell.Theme.hair
        Column {
            id: railColumn
            anchors.centerIn: parent; spacing: 2
            RailButton { glyph: "+"; tip: Tr.t("Acercar"); onPressed: { hud.cedeCamera(); hud.controller.orbitGlobe(0, 0, 1) } }
            RailButton { glyph: "−"; tip: Tr.t("Alejar"); onPressed: { hud.cedeCamera(); hud.controller.orbitGlobe(0, 0, -1) } }
            RailButton {
                glyph: hud.tilted ? "2D" : "3D"; tip: hud.tilted ? Tr.t("Vista cenital") : Tr.t("Inclinar (cerca del suelo)")
                active: hud.tilted
                onPressed: { hud.cedeCamera(); hud.controller.tiltGlobe(hud.tilted ? -70 : 45); hud.tilted = !hud.tilted }
            }
            RailButton {
                icon: "dkWorld"; tip: Tr.t("Tierra completa")
                onPressed: { hud.cedeCamera(); hud.tilted = false; hud.controller.releaseFocus(); hud.controller.orbitGlobe(0, 0, -40) }
            }
            Rectangle { width: 28; height: 1; color: Shell.Theme.hair; anchors.horizontalCenter: parent.horizontalCenter }
            RailButton { icon: "stack"; tip: Tr.t("Capas de imagen"); active: hud.panel === "layers"; onPressed: hud.toggle("layers") }
            RailButton {
                glyph: "⟷"; tip: Tr.t("Regla"); active: hud.measuring; enabledHere: !!hud.view
                onPressed: { hud.measuring = !hud.measuring; hud.marks = [] }
            }
            RailButton { icon: "lines"; tip: Tr.t("Detalles"); active: hud.panel === "details"; onPressed: hud.toggle("details") }
            RailButton { icon: "control"; tip: Tr.t("Consola de Archie"); active: hud.panel === "console"; onPressed: hud.toggle("console") }
        }
    }

    // ---- Layers popover.
    Rectangle {
        objectName: "world.layers"
        visible: hud.panel === "layers"
        anchors.right: rail.left; anchors.rightMargin: 10; anchors.verticalCenter: rail.verticalCenter
        width: 300; height: layerColumn.implicitHeight + 24; radius: Shell.Theme.rCard
        color: Shell.Theme.reading; border.color: Shell.Theme.hair
        Column {
            id: layerColumn
            x: 12; y: 12; width: parent.width - 24; spacing: 4
            Text { text: Tr.t("Capas"); color: Shell.Theme.t1; font.family: Shell.Theme.disp; font.pixelSize: 15; font.weight: Font.DemiBold; bottomPadding: 4 }
            Text {
                visible: hud.imageSources.length === 0; width: parent.width; wrapMode: Text.Wrap
                text: Tr.t("Aún sin fuentes de imagen publicadas."); color: Shell.Theme.t3; font.family: Shell.Theme.font; font.pixelSize: 12
            }
            Repeater {
                model: hud.imageSources
                delegate: Rectangle {
                    required property var modelData
                    readonly property bool chosen: modelData.source_id === (hud.stream ? hud.stream.sourceId : hud.controller.selectedImagery)
                    width: layerColumn.width; height: 46; radius: 8
                    color: chosen ? Shell.Theme.sel : layerArea.containsMouse ? Shell.Theme.hover : "transparent"
                    Column {
                        anchors.verticalCenter: parent.verticalCenter; x: 10; width: parent.width - 20
                        Text { width: parent.width; elide: Text.ElideRight; textFormat: Text.PlainText; text: modelData.label || modelData.source_id; color: Shell.Theme.t1; font.family: Shell.Theme.font; font.pixelSize: 13; font.weight: chosen ? Font.DemiBold : Font.Normal }
                        Text { width: parent.width; elide: Text.ElideRight; textFormat: Text.PlainText; text: (modelData.imagery_date || Tr.t("sin fecha")) + " · " + (modelData.mode === "online_preview_only" ? Tr.t("vista en línea") : Tr.t("datos abiertos")) + " · " + (modelData.state || Tr.t("sin lectura")); color: Shell.Theme.t3; font.family: Shell.Theme.font; font.pixelSize: 10 }
                    }
                    MouseArea {
                        id: layerArea; anchors.fill: parent; hoverEnabled: true
                        onClicked: { if (hud.stream) hud.stream.sourceId = modelData.source_id; else hud.controller.selectedImagery = modelData.source_id }
                    }
                }
            }
        }
    }

    // ---- Ruler: while measuring, a click on the globe marks a point instead of picking.
    MouseArea {
        anchors.fill: parent
        enabled: hud.measuring
        visible: enabled
        z: -1
        cursorShape: Qt.CrossCursor
        acceptedButtons: Qt.LeftButton
        onClicked: function(mouse) {
            const point = hud.surfaceAt(mouse.x, mouse.y)
            if (!point) return
            hud.marks = hud.marks.length >= 2 ? [point] : hud.marks.concat([point])
        }
    }
    Rectangle {
        objectName: "world.ruler"
        visible: hud.measuring
        anchors.horizontalCenter: parent.horizontalCenter; y: 16
        width: rulerText.implicitWidth + 32; height: 40; radius: 20
        color: Shell.Theme.reading; border.color: Shell.Theme.acc
        Text {
            id: rulerText
            anchors.centerIn: parent
            text: hud.marks.length === 0 ? Tr.t("Regla: toca el primer punto")
                : hud.marks.length === 1 ? Tr.t("Regla: toca el segundo punto")
                : Tr.t("Distancia geodésica: ") + Tools.formatDistance(hud.measured)
            color: Shell.Theme.t1; font.family: Shell.Theme.font; font.pixelSize: 13; font.weight: Font.Medium
        }
    }

    // ---- Console drawer, bottom.
    Rectangle {
        id: consoleDrawer
        objectName: "world.console"
        visible: hud.panel === "console"
        anchors.left: parent.left; anchors.right: rail.left; anchors.margins: 16
        anchors.bottom: parent.bottom; anchors.bottomMargin: 112
        height: Math.min(320, hud.height * 0.42); radius: Shell.Theme.rCard
        color: Shell.Theme.reading; border.color: Shell.Theme.hair
        ColumnLayout {
            anchors.fill: parent; anchors.margins: 12; spacing: 8
            RowLayout {
                Layout.fillWidth: true
                Text { text: Tr.t("Consola"); color: Shell.Theme.t1; font.family: Shell.Theme.disp; font.pixelSize: 15; font.weight: Font.DemiBold }
                Text {
                    Layout.fillWidth: true
                    text: hud.cli ? Tr.t("world · os · iris — Core y Policy autorizan cada orden") : Tr.t("Sin consola: este host no configuró el CLI de Archie")
                    color: Shell.Theme.t3; font.family: Shell.Theme.font; font.pixelSize: 11; elide: Text.ElideRight
                }
                Button { visible: !!hud.cli && hud.cli.busy; text: Tr.t("Dejar de esperar"); onClicked: hud.cli.cancel() }
            }
            ListView {
                id: log
                Layout.fillWidth: true; Layout.fillHeight: true
                clip: true; spacing: 4
                model: hud.cli ? hud.cli.entries : []
                onCountChanged: Qt.callLater(positionViewAtEnd)
                delegate: Text {
                    required property var modelData
                    width: log.width; wrapMode: Text.Wrap; textFormat: Text.PlainText
                    text: (modelData.kind === "command" ? "› " : "") + modelData.text
                    color: modelData.kind === "error" ? Shell.Theme.bad : modelData.kind === "command" ? Shell.Theme.acc : Shell.Theme.t2
                    font.family: "monospace"; font.pixelSize: 12
                }
            }
            TextField {
                id: commandField
                Layout.fillWidth: true
                enabled: !!hud.cli && !hud.cli.busy
                placeholderText: "help · world resolve Santiago · os status · iris status"
                placeholderTextColor: Shell.Theme.t3
                color: Shell.Theme.t1; font.family: "monospace"; font.pixelSize: 13
                background: Rectangle { radius: 8; color: Shell.Theme.raised; border.color: commandField.activeFocus ? Shell.Theme.acc : Shell.Theme.hair }
                onAccepted: { if (text.trim().length) { hud.cli.submit(text.trim()); text = "" } }
            }
        }
    }

    // ---- Where the camera is, bottom right above the attribution.
    Rectangle {
        visible: !!hud.below
        anchors.right: parent.right; anchors.rightMargin: 16
        anchors.bottom: parent.bottom; anchors.bottomMargin: 112
        width: where.implicitWidth + 24; height: 30; radius: 15
        color: Shell.Theme.reading; border.color: Shell.Theme.hair
        Text {
            id: where
            anchors.centerIn: parent
            text: hud.below ? hud.below.latitude.toFixed(3) + "°, " + hud.below.longitude.toFixed(3) + Tr.t("° · cámara ")
                  + (hud.altitudeKm >= 10 ? Math.round(hud.altitudeKm).toLocaleString(Qt.locale(), "f", 0) + " km" : (hud.altitudeKm * 1000).toFixed(0) + " m") : ""
            color: Shell.Theme.t2; font.family: Shell.Theme.font; font.pixelSize: 11
            font.features: { "tnum": 1 }
        }
    }
}
