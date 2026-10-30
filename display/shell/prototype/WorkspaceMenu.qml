import QtQuick
import "Lang.js" as Tr

// Workspace selector, a 300 px menu under the "Dominio" chip of the bar. DESIGN SAMPLE.
// The switch itself is done by the compositor (as archie-session.sh does today); the shell only
// shows the menu. The Orbe is one of the entries and is never drawn here.
Panel {
    id: wm
    kind: "panel"
    width: 300
    height: col.implicitHeight + 16
    enterScale: 1; enterDy: -8
    property string st: Demo.state
    property string sel: ""
    readonly property bool domainSwitchAvailable: false
    readonly property string domainSwitchReason: Tr.t("Cambiar el dominio requiere un contrato del compositor y del backend; no está disponible desde la shell.")
    signal chosen(string which)

    component Row2: Rectangle {
        id: rw
        property string domain
        property string icon
        property string name
        property bool on: false
        property bool ring: false
        property bool available: true
        default property alias trailing: rt.data
        signal clicked()
        width: parent ? parent.width : 280; height: 42; radius: 9
        color: (on || ring) ? Theme.sel : "transparent"
        Rectangle { anchors.fill: parent; radius: 9; color: "transparent"; border.width: 2; border.color: Theme.acc; visible: rw.ring }
        Row { x: 10; anchors.verticalCenter: parent.verticalCenter; spacing: 10
            Icon { visible: rw.icon !== ""; name: rw.icon; size: 16; color: Theme.t2; anchors.verticalCenter: parent.verticalCenter }
            DomainDot { visible: rw.domain !== ""; domain: rw.domain; anchors.verticalCenter: parent.verticalCenter }
            Text { text: rw.name; color: Theme.t1; font.family: Theme.font; font.pixelSize: 13; font.weight: Font.Medium; anchors.verticalCenter: parent.verticalCenter } }
        Row { id: rt; anchors.right: parent.right; anchors.rightMargin: 10; anchors.verticalCenter: parent.verticalCenter; spacing: 6 }
        MouseArea { anchors.fill: parent; hoverEnabled: true; enabled: rw.available; onClicked: rw.clicked() }
    }

    Column { id: col; x: 8; y: 8; width: parent.width - 16; spacing: 0
        Eyebrow { text: Tr.t("Espacio de trabajo"); leftPadding: 10; topPadding: 8; bottomPadding: 6 }
        Row2 { icon: "dkOrb"; name: Tr.t("Orbe"); on: wm.st === "vacio" || wm.sel === "orb"; ring: Demo.focusRing; onClicked: { wm.sel = "orb"; wm.chosen("orb") } }
        Row2 { visible: wm.st !== "vacio"; available: wm.domainSwitchAvailable; domain: "personal"; name: "Personal"; on: false
            Icon { visible: wm.st !== "offline" && wm.sel === "personal"; name: "done"; size: 14; color: Theme.ok }
            Chip { visible: wm.st === "offline"; small: true; tone: "unk"; icon: "unknown"; text: "Archie"; sub: Tr.t("sin lectura") }
            Chip { visible: wm.st === "degraded"; small: true; tone: "unk"; icon: "unknown"; text: Tr.t("ventanas"); sub: Tr.t("sin lectura") } }
        Row2 { visible: wm.st !== "vacio"; available: wm.domainSwitchAvailable; domain: "work"; name: Tr.t("Trabajo"); on: false
            Chip { visible: wm.st === "offline"; small: true; tone: "unk"; icon: "unknown"; text: "Archie"; sub: Tr.t("sin lectura") }
            Icon { visible: wm.st !== "offline" && wm.sel === "work"; name: "done"; size: 14; color: Theme.ok } }
        Item { visible: wm.st === "vacio"; width: parent.width; height: vb.height + 12
            VoidBox { id: vb; x: 4; y: 8; width: parent.width - 8; icon: "spaces"; title: Tr.t("Espacios sin lectura"); text: Tr.t("No hay un contrato de lectura del compositor. Esta vista no determina qué espacios existen.") } }
        NoteBox { width: parent.width; text: wm.domainSwitchReason }
        Item { visible: wm.st === "error"; width: parent.width; height: eb.height + 8
            ErrBox { id: eb; x: 4; y: 8; width: parent.width - 8; text: Tr.t("Sin lectura confirmada del compositor. No se ha cambiado el dominio.") } }
        Item { visible: wm.st === "stale"; width: parent.width; height: nb.height + 8
            NoteBox { id: nb; x: 4; y: 8; width: parent.width - 8; icon: "recents"; text: Tr.t("Sin lectura actual del compositor.") } }
        Row { leftPadding: 10; topPadding: 12; bottomPadding: 4; spacing: 4
            Kbd { label: "Esc"; anchors.verticalCenter: parent.verticalCenter } Text { text: Tr.t("salir"); color: Theme.t3; font.family: Theme.font; font.pixelSize: 12; anchors.verticalCenter: parent.verticalCenter; leftPadding: 2 } }
    }
}
