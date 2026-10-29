import QtQuick
import "Lang.js" as Tr

// Top bar, 32 px. Order is frozen: left = brand, app, menus; right = link chip, camera/mic,
// domain, search, activity count, control center, clock.
Rectangle {
    id: bar
    height: Theme.barH
    color: Theme.bar
    property string overlay: ""
    property string app: "Vault"
    property var bridge
    // Why the local bridge process is not running, if it is not (the link chip says so).
    property string hostError: ""
    // The context domain this desktop's host was started with (`--world-context`), if any.
    readonly property string domain: typeof hostDomain !== "undefined" ? hostDomain : ""
    signal toggle(string name)
    property string menu: ""                 // name of the open drop-down, "" if none
    signal menuClicked(string name, real x)

    Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.inner }

    component MenuItem: Rectangle {
        id: mi
        property string label
        objectName: "bar.menu." + label
        Accessible.role: Accessible.MenuItem
        Accessible.name: label
        height: 24; radius: 6; width: mt.implicitWidth + 18
        color: bar.menu === label ? Theme.pressed : (mma.containsMouse ? Theme.hover : "transparent")
        Text { id: mt; anchors.centerIn: parent; text: mi.label; color: (bar.menu === mi.label || mma.containsMouse) ? Theme.t1 : Theme.t2; font.family: Theme.font; font.pixelSize: 13 }
        MouseArea { id: mma; anchors.fill: parent; hoverEnabled: true; onClicked: bar.menuClicked(mi.label, mi.mapToItem(bar, 0, 0).x) }
    }
    component IconButton: Rectangle {
        id: ib
        property string icon
        property string count: ""
        property bool active: false
        /// What the button is called, for whoever cannot see its icon.
        property string label: icon
        signal clicked()
        Accessible.role: Accessible.Button
        Accessible.name: label
        width: count === "" ? 28 : cr.implicitWidth + 16; height: 26; radius: 7
        color: active ? Theme.pressed : (ma.containsMouse ? Theme.hover : "transparent")
        Row { id: cr; anchors.centerIn: parent; spacing: 6
            Icon { name: ib.icon; size: 16; color: ib.active || ma.containsMouse || ib.count !== "" ? Theme.t1 : Theme.t2; anchors.verticalCenter: parent.verticalCenter }
            Text { visible: ib.count !== ""; text: ib.count; color: Theme.t1; font.family: Theme.font; font.pixelSize: 13; font.weight: Font.DemiBold; font.features: { "tnum": 1 }; anchors.verticalCenter: parent.verticalCenter }
        }
        MouseArea { id: ma; anchors.fill: parent; hoverEnabled: true; onClicked: ib.clicked() }
    }

    Row {
        x: 14; anchors.verticalCenter: parent.verticalCenter; spacing: 4
        Row { spacing: 8; rightPadding: 14; anchors.verticalCenter: parent.verticalCenter
            Image { objectName: "bar.logo"; source: Theme.logo; height: 20; fillMode: Image.PreserveAspectFit; smooth: true; mipmap: true
                sourceSize.height: 80; anchors.verticalCenter: parent.verticalCenter
                Accessible.role: Accessible.StaticText; Accessible.name: "ArchieOS" } }
        Text { text: bar.app; rightPadding: 12; color: Theme.t1; font.family: Theme.font; font.pixelSize: 13; font.weight: Font.DemiBold; anchors.verticalCenter: parent.verticalCenter }
        MenuItem { label: Tr.t("Archivo"); anchors.verticalCenter: parent.verticalCenter } MenuItem { label: Tr.t("Editar"); anchors.verticalCenter: parent.verticalCenter } MenuItem { label: Tr.t("Vista"); anchors.verticalCenter: parent.verticalCenter } MenuItem { label: Tr.t("Ir"); anchors.verticalCenter: parent.verticalCenter } MenuItem { label: Tr.t("Ventana"); anchors.verticalCenter: parent.verticalCenter }
    }
    Row {
        anchors.right: parent.right; anchors.rightMargin: 12; anchors.verticalCenter: parent.verticalCenter; spacing: 6
        ThemeToggle { objectName: "bar.theme"; compact: true; anchors.verticalCenter: parent.verticalCenter }
        LinkChip { bridge: bar.bridge; hostError: bar.hostError; anchors.verticalCenter: parent.verticalCenter }
        // Camera and microphone have no source wired yet (GUI-SENTRY-002): no chip claims one.
        // The domain is the one the host was started with; with none, the chip is not drawn.
        // It opens the workspace menu (decision 5 of the handoff: the selector lives in this chip).
        Item { objectName: "bar.domain"; visible: bar.domain !== ""; width: visible ? dom.width : 0; height: dom.height; anchors.verticalCenter: parent.verticalCenter
            Chip { id: dom; text: Tr.t("Dominio"); sub: bar.domain; domain: bar.domain.indexOf("work/") === 0 ? "work" : "personal"; tone: "neutral" }
            MouseArea { anchors.fill: parent; onClicked: bar.toggle("workspace") } }
        Item { width: 13; height: 14; anchors.verticalCenter: parent.verticalCenter
            Rectangle { anchors.centerIn: parent; width: 1; height: 14; color: Theme.hair } }
        IconButton { objectName: "bar.palette"; icon: "search"; label: Tr.t("Buscar"); active: bar.overlay === "palette"; onClicked: bar.toggle("palette"); anchors.verticalCenter: parent.verticalCenter }
        // Running long jobs from the bridge; "—" when no workspace is configured, never a made-up 0.
        IconButton { objectName: "bar.activity"; icon: "activity"; label: Tr.t("Actividad"); count: bar.bridge.jobs.state === "ok" ? String(bar.bridge.runningJobs) : "—"
            active: bar.overlay === "activity"; onClicked: bar.toggle("activity"); anchors.verticalCenter: parent.verticalCenter }
        IconButton { objectName: "bar.control"; icon: "control"; label: Tr.t("Centro de control"); active: bar.overlay === "control"; onClicked: bar.toggle("control"); anchors.verticalCenter: parent.verticalCenter }
        Text { leftPadding: 8; text: Demo.barClock; color: Theme.t1; font.family: Theme.font; font.pixelSize: 13; font.weight: Font.Medium
            font.features: { "tnum": 1 }; anchors.verticalCenter: parent.verticalCenter }
    }
}
