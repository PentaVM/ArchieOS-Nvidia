import QtQuick
import QtQuick.Controls
import "NeuralLabelLayout.js" as LabelLayout
import "../../shell/prototype/Lang.js" as Tr

// The neural sphere's HUD (hud.ts): a label over each folder of the first level and over
// every running program, and the selection's card. Positions come from Rust in the
// field's coordinates; the view projects them. Set in Inter, the project's typeface.
Item {
    id: hud
    required property var field
    required property var view
    property bool labelsVisible: true
    property bool lightMode: true
    property color textColor: "#142236"
    property color mutedColor: "#566c80"
    property color errorColor: "#ba263b"
    property color surfaceColor: "#eef3f8"
    property color panelColor: lightMode ? Qt.rgba(1,1,1,0.75) : Qt.rgba(0.035,0.055,0.09,0.75)
    property color borderColor: lightMode ? "#71859d" : "#718ba7"
    property rect modelActivityBounds: Qt.rect(0,0,0,0)
    property string rootLabel: ""
    // Whether Core is publishing model activity to this view (shown in Details only).
    property bool telemetryKnown: true
    // The machine's condition, when the host reads it (the Observatory does).
    property Component machine: null
    property bool navigationEnabled: true
    // Advances whenever the sphere or the camera moved, so projections are taken again.
    property int frame: 0

    readonly property string family: inter.status === FontLoader.Ready ? inter.font.family : "sans-serif"
    FontLoader { id: inter; source: "qrc:/archie/shell/fonts/inter/Inter-Variable.ttf" }

    property var rows: []
    property var card: []
    readonly property color labelBacking: panelColor
    readonly property color labelEdge: borderColor
    readonly property string rootDisplayName: {
        const path=String(hud.rootLabel || (hud.card.length>1 ? hud.card[1] : "")).replace(/\\/g,"/").replace(/\/+$/,"")
        return (path.slice(path.lastIndexOf("/")+1) || Tr.t("Orbe")).toUpperCase()
    }
    readonly property var labelPlan: { hud.frame; return hud.positionLabels() }
    readonly property var placedLabels: labelPlan.labels
    readonly property int overflowLabelCount: labelPlan.overflow
    FontMetrics { id: labelMetrics; font.family: hud.family; font.pixelSize: 12; font.weight: Font.Medium }
    function positionLabels() {
        if (!hud.labelsVisible || !hud.view || !hud.view.camera) return LabelLayout.plan([],hud.width,hud.height)
        const projected = hud.rows.map(function(row) {
            const at = hud.view.mapFrom3DScene(Qt.vector3d(row.x, row.y, row.z))
            const title = String(row.text || "")
            return {id:row.id,text:title,kind:row.kind,selected:hud.card.length>0 && row.id===hud.card[0],anchorX:at.x,anchorY:at.y,depth:at.z,
                width:Math.min(216,Math.max(64,Math.ceil(labelMetrics.advanceWidth(title))+22)),height:28}
        })
        const reserved = [{x:detailsButton.x-6,y:detailsButton.y-6,width:detailsButton.width+12,height:detailsButton.height+12}]
        const activity=hud.modelActivityBounds
        if(activity.width>0 && activity.height>0) reserved.push({x:activity.x-6,y:activity.y-6,width:activity.width+12,height:activity.height+12})
        if (selectionPlate.visible) reserved.push({x:0,y:Math.max(0,hud.height-selectionPlate.height-30),
            width:selectionPlate.width+36,height:selectionPlate.height+30})
        return LabelLayout.plan(projected,hud.width,hud.height,{gap:7,reserved:reserved})
    }
    function refresh() {
        rows = labelsVisible ? field.labels() : []
        card = field.selection(rootLabel)
    }
    Connections {
        target: hud.field
        function onUpdated() { hud.frame++; if (hud.field.revision !== hud.seen) { hud.seen = hud.field.revision; hud.refresh() } }
        function onSelectionChanged() { hud.refresh() }
        function onSourceChanged() { hud.refresh() }
    }
    // Camera/input and resizes must reproject labels even while the model is frozen.
    Connections {
        target: hud.view.camera
        function onPositionChanged() { hud.frame++ }
        function onRotationChanged() { hud.frame++ }
    }
    Connections {
        target: hud.view
        function onWidthChanged() { hud.frame++ }
        function onHeightChanged() { hud.frame++ }
    }
    property var seen: -1
    onLabelsVisibleChanged: refresh()
    Component.onCompleted: refresh()

    // Provenance remains inspectable without competing with active model readings.
    Rectangle {
        id: detailsButton
        objectName: "neuralDetailsButton"
        anchors.right: parent.right; anchors.rightMargin: 18
        anchors.top: parent.top; anchors.topMargin: 18
        width: 96; height: 32; radius: 8
        visible: hud.labelsVisible
        color: hud.labelBacking; border.color: hud.labelEdge
        Text {
            anchors.centerIn: parent
            text: hud.field.error.length > 0 ? Tr.t("!  Detalles") : hud.field.loading ? Tr.t("·  Leyendo") : Tr.t("Detalles")
            color: hud.field.error.length > 0 ? hud.errorColor : hud.mutedColor
            font.family: hud.family; font.pixelSize: 12
        }
        MouseArea {
            anchors.fill: parent
            onClicked: details.opened ? details.close() : details.open()
        }
        Accessible.role: Accessible.Button
        Accessible.name: Tr.t("Detalles de la esfera")
        Accessible.onPressAction: details.open()
    }
    Popup {
        id: details
        objectName: "neuralDetails"
        x: Math.max(12, hud.width - width - 18); y: 52
        width: Math.min(360, Math.max(0, hud.width - 24))
        padding: 14
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { color: hud.labelBacking; radius: 10; border.color: hud.labelEdge }
        contentItem: Column {
            spacing: 8
            Text { width: parent.width; text: Tr.t("Instantánea de archivos"); color: hud.textColor; font.family: hud.family; font.pixelSize: 14; font.weight: Font.Medium }
            Text { width: parent.width; text: hud.rootLabel; textFormat: Text.PlainText; wrapMode: Text.WrapAnywhere; color: hud.mutedColor; font.family: hud.family; font.pixelSize: 11 }
            Text {
                width: parent.width
                text: Tr.t("Hasta ") + hud.field.limit + " nodos · " + hud.field.filesPerFolder + Tr.t(" archivos por carpeta\n") + hud.field.scannedAt
                    + Tr.t("\nOmite ocultos y carpetas de compilación. Destellos decorativos; enlaces de estructura visual.")
                    + Tr.t("\nRótulos en pantalla: ") + hud.placedLabels.length + Tr.t(" de ") + hud.rows.length
                    + Tr.t("\nRótulos omitidos por espacio o límite: ") + hud.overflowLabelCount
                color: hud.mutedColor; font.family: hud.family; font.pixelSize: 11; wrapMode: Text.Wrap
            }
            Text { width: parent.width; visible: text.length > 0; text: hud.field.error; textFormat: Text.PlainText; color: hud.errorColor; font.family: hud.family; font.pixelSize: 11; wrapMode: Text.Wrap }
            Text {
                objectName: "neuralDetails.telemetry"
                width: parent.width
                text: hud.telemetryKnown ? Tr.t("Actividad de modelos: publicada por Core") : Tr.t("Actividad de modelos: sin telemetría de Core todavía")
                color: hud.mutedColor; font.family: hud.family; font.pixelSize: 11; wrapMode: Text.Wrap
            }
            Loader { width: parent.width; active: hud.machine !== null; visible: active; sourceComponent: hud.machine }
            Button {
                text: Tr.t("Actualizar")
                enabled: hud.navigationEnabled && !hud.field.loading
                onClicked: hud.field.refresh()
                palette.button: hud.surfaceColor; palette.buttonText: hud.textColor
            }
        }
    }

    // Leaders always point to the real projected coordinates, behind every label.
    Repeater {
        model: hud.placedLabels
        delegate: Rectangle {
            required property var modelData
            readonly property real dx: modelData.leaderX-modelData.anchorX
            readonly property real dy: modelData.leaderY-modelData.anchorY
            x: modelData.anchorX; y: modelData.anchorY
            width: Math.sqrt(dx*dx+dy*dy); height: 1
            rotation: Math.atan2(dy,dx)*180/Math.PI
            transformOrigin: Item.Left
            color: hud.lightMode ? Qt.rgba(0.22,0.38,0.57,0.65) : Qt.rgba(0.65,0.77,0.91,0.65)
        }
    }
    Repeater {
        model: hud.placedLabels
        delegate: Rectangle {
            required property var modelData
            objectName: "neuralLabel." + modelData.id
            x: modelData.x; y: modelData.y
            width: modelData.width; height: modelData.height
            radius: 7
            color: hud.labelBacking; border.color: hud.labelEdge
            Text {
                anchors.fill: parent; anchors.leftMargin: 11; anchors.rightMargin: 11
                verticalAlignment: Text.AlignVCenter
                text: modelData.text; textFormat: Text.PlainText
                elide: Text.ElideRight
                color: hud.textColor; font.family: hud.family; font.pixelSize: 12
                font.weight: modelData.kind === "p" ? Font.DemiBold : Font.Medium
            }
            HoverHandler { id: labelHover }
            ToolTip.visible: labelHover.hovered
            ToolTip.delay: 500
            ToolTip.text: modelData.text
            Accessible.name: modelData.text
        }
    }

    // The selection: id, title, what it holds, and whether it is folded.
    Rectangle {
        id: selectionPlate
        visible: hud.labelsVisible && hud.field.ready && hud.card.length >= 2 && hud.card[0].length > 0
        anchors.left: parent.left; anchors.leftMargin: 18
        anchors.bottom: parent.bottom; anchors.bottomMargin: 18
        width: Math.min(340,Math.max(0,hud.width-36)); height: selectionText.implicitHeight+24
        radius: 10
        color: hud.labelBacking; border.color: hud.labelEdge
        Column {
            id: selectionText
            anchors.left: parent.left; anchors.right: parent.right; anchors.margins: 12
            anchors.verticalCenter: parent.verticalCenter
            spacing: 5
            Repeater {
                model: hud.card.slice(1)
                delegate: Text {
                    required property var modelData
                    required property int index
                    objectName: "neuralSelectionText." + index
                    width: selectionText.width
                    visible: modelData.length > 0
                    text: index===0 && hud.card[0]==="." ? hud.rootDisplayName : Tr.composed(modelData); textFormat: Text.PlainText
                    wrapMode: Text.WrapAnywhere
                    color: index === 0 ? hud.textColor : hud.mutedColor
                    font.family: hud.family
                    font.pixelSize: index === 0 ? 14 : 12
                    font.weight: index === 0 ? Font.DemiBold : Font.Normal
                }
            }
        }
    }
}
