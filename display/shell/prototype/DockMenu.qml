import QtQuick
import "Lang.js" as Tr

// Context menu of a dock icon (right click): what that app's window can do. The Orb's entry shows
// or hides the desktop's windows. Every action goes through the window manager (Wm).
Panel {
    id: dm
    kind: "panel"
    property string target: "vault"       // dock id: orb, vault, forge, sentry, stage, set
    signal activated()
    signal spaceRequested(string space)
    width: 230
    height: col.implicitHeight + 12
    enterDy: 8

    readonly property bool isOrb: target === "orb"
    readonly property var items: target === "world" ? [{l:Tr.t("Mostrar World"),act:"world"},{l:Tr.t("Volver al Orbe"),act:"orb"}] : isOrb ? [
        { l: Demo.bare ? Tr.t("Mostrar las ventanas") : Tr.t("Mostrar el escritorio"), act: "desk" },
        { l: Tr.t("Minimizar todas"), act: "minAll", dis: Wm.shownCount() === 0 },
        { l: Tr.t("Cerrar todas"), act: "closeAll", dis: Wm.openCount() === 0 } ] : [
        { l: !Wm.isOpen(target) ? Tr.t("Abrir ") + Wm.title(target) : (Wm.isMin(target) || Demo.bare ? Tr.t("Mostrar") : Tr.t("Traer al frente")), act: "open" },
        { l: Tr.t("Minimizar"), act: "min", dis: !Wm.isShown(target) },
        { l: "Zoom", act: "zoom", dis: !Wm.isShown(target) },
        { sep: true },
        { l: Tr.t("Cerrar ventana"), act: "close", dis: !Wm.isOpen(target) } ]

    function run(act) {
        if (act === "world" || act === "orb") spaceRequested(act)
        else if (act === "desk") Demo.bare = !Demo.bare
        else if (act === "minAll") Wm.minimizeAll()
        else if (act === "closeAll") Wm.closeAll()
        else if (act === "open") Wm.open(target)
        else if (act === "min") Wm.minimize(target)
        else if (act === "zoom") Wm.toggleMax(target)
        else if (act === "close") Wm.close(target)
        activated()
    }

    Column { id: col; x: 6; y: 6; width: parent.width - 12
        Item { width: col.width; height: 26
            Text { x: 10; anchors.verticalCenter: parent.verticalCenter; text: dm.target === "world" ? "World" : dm.isOrb ? "Orb" : Wm.title(dm.target)
                color: Theme.t3; font.family: Theme.font; font.pixelSize: 11; font.weight: Font.DemiBold; font.letterSpacing: 0.6; font.capitalization: Font.AllUppercase } }
        Repeater { model: dm.items
            delegate: Item { objectName: modelData.sep ? "" : "dockmenu." + modelData.act; width: col.width; height: modelData.sep ? 9 : 30
                Rectangle { visible: modelData.sep === true; x: 8; y: 4; width: parent.width - 16; height: 1; color: Theme.hair }
                Rectangle { visible: modelData.sep !== true; anchors.fill: parent; radius: 7
                    color: (ima.containsMouse && !modelData.dis) ? Theme.hover : "transparent"
                    Text { x: 10; anchors.verticalCenter: parent.verticalCenter; text: modelData.l || ""
                        color: modelData.dis ? Theme.tdis : Theme.t1; font.family: Theme.font; font.pixelSize: 13 }
                    MouseArea { id: ima; anchors.fill: parent; hoverEnabled: true; enabled: !modelData.dis; onClicked: dm.run(modelData.act) } } } }
    }
}
