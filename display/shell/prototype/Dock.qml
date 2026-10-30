import QtQuick

// Dock: Orb, World, Vault, Forge, Sentry, Stage, Ajustes, in that order. 56 px icons, white line glyph
// on a solid colour under a 165° wash. A 4 px dot under apps whose window is open (dim while it is
// minimised); the app in front has a 1.5 px accent outline 3 px away. Clicking opens, restores,
// raises or minimises its window (Wm.dockClick).
Panel {
    id: dock
    kind: "dock"
    width: row.implicitWidth + 24
    height: Theme.dockIcon + 26
    // The app in front: the window manager's front window, or the Orb when the desktop is showing.
    property string desktopSpace:"orb"
    readonly property string current: Wm.front() === "" || Demo.bare ? desktopSpace : Wm.front()
    signal launched(string id)
    // Right click: ask the host for the icon's context menu, centred on the icon (x in the dock's coordinates).
    signal menuRequested(string id, real centerX)

    Row {
        id: row; x: 12; y: 10; spacing: 14
        Repeater {
            model: Synthetic.dock
            delegate: Item {
                id: cell
                objectName: "dock." + modelData.id
                width: Theme.dockIcon; height: Theme.dockIcon
                // Named to the accessibility bus, so a screen reader, or Archie, finds it by name.
                Accessible.role: Accessible.Button
                Accessible.name: modelData.name
                Accessible.onPressAction: dock.launched(modelData.id)
                // Quiet and reduced motion: no scaling, only the highlight below.
                scale: !Theme.spatial ? 1.0 : (ma.pressed ? 0.96 : (ma.containsMouse ? 1.12 : 1.0))
                Behavior on scale { NumberAnimation { duration: Theme.durDock; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } }
                // active outline
                Rectangle { visible: dock.current === modelData.id; anchors.fill: parent; anchors.margins: -4.5; radius: Theme.rDockIcon + 4.5
                    color: "transparent"; border.width: 1.5; border.color: Theme.acc }
                Rectangle {
                    anchors.fill: parent; radius: Theme.rDockIcon
                    color: Theme.dk[modelData.id]
                    Rectangle { // 165° wash: rgba(255,255,255,.10) → rgba(0,0,0,.14)
                        anchors.fill: parent; radius: parent.radius
                        gradient: Gradient {
                            GradientStop { position: 0; color: Qt.rgba(1, 1, 1, 0.10) }
                            GradientStop { position: 1; color: Qt.rgba(0, 0, 0, 0.14) }
                        }
                    }
                    Rectangle { anchors.fill: parent; radius: parent.radius; color: "transparent"; border.width: 1; border.color: Qt.rgba(1, 1, 1, 0.14) }
                    Rectangle { anchors.fill: parent; radius: parent.radius; color: Qt.rgba(0, 0, 0, ma.pressed ? 0.18 : 0) }
                    Rectangle { anchors.fill: parent; radius: parent.radius; color: "white"
                        opacity: (!Theme.spatial && ma.containsMouse) ? 0.12 : 0
                        Behavior on opacity { NumberAnimation { duration: Theme.durDock } } }
                    Icon { anchors.centerIn: parent; name: modelData.icon; size: 28; color: Theme.dockInk }
                }
                // A dot under what is open (the Orb always runs); dimmer while its window is minimised.
                Rectangle { visible: modelData.id === dock.desktopSpace || Wm.isOpen(modelData.id); width: 4; height: 4; radius: 2; color: Theme.t1
                    opacity: Wm.isMin(modelData.id) ? 0.4 : 1
                    anchors.horizontalCenter: parent.horizontalCenter; anchors.top: parent.bottom; anchors.topMargin: 8 }
                Rectangle { visible: ma.containsMouse; y: -30; anchors.horizontalCenter: parent.horizontalCenter
                    width: tip.implicitWidth + 16; height: 22; radius: 7; color: Theme.panel; border.width: 1; border.color: Theme.hair
                    Text { id: tip; anchors.centerIn: parent; text: modelData.id === "stage" ? Wm.title("stage") : modelData.name; color: Theme.t1; font.family: Theme.font; font.pixelSize: 12; font.weight: Font.Medium } }
                MouseArea { id: ma; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.LeftButton | Qt.RightButton
                    onClicked: function (m) { if (m.button === Qt.RightButton) dock.menuRequested(modelData.id, cell.mapToItem(dock, cell.width / 2, 0).x); else dock.launched(modelData.id) } }
            }
        }
    }
}
