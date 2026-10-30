import QtQuick
import QtQuick.Controls
import "../../shell/prototype/Lang.js" as Tr

// Read-only observations supplied by Core's projection. Catalog membership and a
// healthy server alone do not establish current model activity or a measured rate.
Item {
    id: hud
    property var models: null
    property color textColor: "#40566b"
    property color panelColor: Qt.rgba(1,1,1,.75)
    property color borderColor: "#71859d"
    property string family: "sans-serif"
    readonly property var activeModels: (Array.isArray(models) ? models : []).filter(function(model) {
        return model && typeof model.id === "string" && model.id.length > 0
            && model.active === true && model.stale === false
    })
    function rate(model) {
        return model.tps_available === true && typeof model.tps === "number"
            && Number.isFinite(model.tps) && model.tps >= 0
            ? model.tps.toFixed(1) + " tok/s" : "—"
    }
    implicitHeight: content.implicitHeight>0 ? content.implicitHeight+16 : 0
    height: implicitHeight
    Rectangle { anchors.fill: parent; visible: hud.height>0; radius: 8; color: hud.panelColor; border.color: hud.borderColor }
    Column {
    id: content
    anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
    anchors.margins: 8
    spacing: 5
    Text {
        objectName: "modelActivity.unavailable"
        visible: !Array.isArray(hud.models)
        text: Tr.t("Sin telemetría")
        color: hud.textColor; font.family: hud.family; font.pixelSize: 11
    }
    Text {
        visible: hud.activeModels.length > 0
        text: Tr.t("Modelos")
        color: hud.textColor; font.family: hud.family; font.pixelSize: 10
    }
    Repeater {
        model: hud.activeModels
        delegate: Item {
            id: modelRow
            required property var modelData
            width: content.width; height: modelName.implicitHeight
            Text {
            id: modelName
            objectName: "modelActivity." + modelRow.modelData.id
            anchors.left: parent.left; anchors.right: modelRate.left; anchors.rightMargin: 10
            text: (modelRow.modelData.label || modelRow.modelData.id) + (modelRow.modelData.state ? " · " + modelRow.modelData.state : "")
            textFormat: Text.PlainText
            color: hud.textColor
            font.family: hud.family
            font.pixelSize: 11
            elide: Text.ElideRight
            Accessible.name: text + " " + modelRate.text
            }
            Text {
                id: modelRate
                objectName: "modelRate." + modelRow.modelData.id
                anchors.right: parent.right
                text: hud.rate(modelRow.modelData)
                color: hud.textColor; font.family: hud.family; font.pixelSize: 11
                font.features: {"tnum":1}
            }
            HoverHandler { id: hover }
            ToolTip.visible: hover.hovered
            ToolTip.delay: 500
            ToolTip.text: Tr.t("Velocidad efectiva de la última ejecución medida, incluido el tiempo de la solicitud.")
        }
    }
    }
}
