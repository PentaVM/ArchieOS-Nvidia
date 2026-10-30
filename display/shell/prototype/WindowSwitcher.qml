import QtQuick
import "Lang.js" as Tr

// Local Qt surfaces only. The external compositor and Core domains are not inferred.
Panel {
    id: sw
    kind: "panel"
    width: 640
    height: foot.y + foot.height
    property string st: Demo.state // Kept for reference harness compatibility; not a source of windows.
    signal chosen(string key)
    readonly property var all: Wm.list()
    readonly property var flat: all.map(function(w) {
        return { title: w.title, app: w.min ? "minimizada" : w.front ? Tr.t("en primer plano") : "abierta", key: w.id, on: w.front }
    })
    readonly property bool none: flat.length === 0
    property int sel: 0
    readonly property string selKey: flat.length ? flat[Math.max(0, Math.min(sel,flat.length - 1))].key : ""
    onShownChanged: if (shown) sel = flat.length > 1 ? 1 : 0
    function step(n) { if (flat.length) sel = (sel + n + flat.length) % flat.length }
    function activateKey(key) {
        if (!shown || !Wm.isOpen(key)) return false
        chosen(key)
        return true
    }
    function choose() { return activateKey(selKey) }

    component Tile: Rectangle {
        id: tile
        property var info: ({key:"",on:false,title:"",app:""})
        property bool ring: false
        signal requested(string key)
        readonly property var iconOf: ({forge:"dkForge",vault:"dkVault",stage:"dkStage",sentry:"dkSentry",set:"dkSet"})
        width: 180; height: tcol.implicitHeight + 26; radius: 12
        color: info.on ? Theme.sel : Theme.raised
        border.width: 1; border.color: info.on ? Theme.tint(Theme.acc,0.45) : Theme.hair
        Rectangle { anchors.fill: parent; anchors.margins: -4; radius: 15; color: "transparent"; border.width: 2; border.color: Theme.acc; visible: tile.ring }
        Column { id: tcol; x: 12; y: 14; width: parent.width - 24; spacing: 8
            Rectangle { width: 44; height: 44; radius: 11; color: Theme.dk[tile.info.key] || Theme.raised
                Rectangle { anchors.fill: parent; radius: 11; gradient: Gradient { GradientStop { position: 0; color: Theme.track } GradientStop { position: 1; color: "transparent" } } }
                Icon { anchors.centerIn: parent; name: tile.iconOf[tile.info.key] || "devunk"; size: 20; color: Theme.dk[tile.info.key] ? Theme.dockInk : Theme.t1 } }
            Text { width: parent.width; elide: Text.ElideRight; text: tile.info.title; color: Theme.t1; font.family: Theme.font; font.pixelSize: 13; font.weight: Font.Medium }
            Text { width: parent.width; elide: Text.ElideRight; text: tile.info.app; color: Theme.t3; font.family: Theme.font; font.pixelSize: 11 }
            Chip { small: true; icon: "unknown"; tone: "unk"; text: Tr.t("Dominio sin lectura") }
        }
        MouseArea { anchors.fill: parent; enabled: tile.info.key !== ""; onClicked: tile.requested(tile.info.key) }
    }
    Column { id: col; x: 20; y: 20; width: parent.width - 40; spacing: 12
        Text { text: Tr.t("Ventanas"); color: Theme.t1; font.family: Theme.disp; font.pixelSize: 22; font.weight: Font.DemiBold }
        Chip { icon: "windows"; text: Tr.t("Ventanas locales de esta shell") }
        VoidBox { visible: sw.none; icon: "windows"; title: Tr.t("No hay ventanas locales abiertas"); text: Tr.t("Abre una vista desde el dock o la paleta.") }
        Flow { visible: !sw.none; width: parent.width; spacing: 16
            Repeater { model: sw.flat
                delegate: Tile { info: modelData; ring: modelData.key === sw.selKey
                    onRequested: function(key) { sw.activateKey(key) } }
            }
        }
        NoteBox { text: Tr.t("No enumera ni cambia ventanas de otras aplicaciones. La selección local no establece dominio ni autoridad de Core.") }
    }
    FootHints { id: foot; y: col.y + col.implicitHeight + 16; hints: [["Tab","siguiente"],["⇧ Tab","anterior"],["↵","mostrar"],["Esc","salir"]] }
}
