import QtQuick

// Sidebar row: 30 px, icon + label; `on` = current, `ring` = visible focus.
Rectangle {
    id: s
    property string icon
    property string text
    property bool on: false
    property bool ring: false
    signal clicked()
    Accessible.role: Accessible.Button
    Accessible.name: s.text
    width: parent ? parent.width : 180; height: 30; radius: 8
    color: on ? Theme.sel : (ma.containsMouse ? Theme.hover : "transparent")
    Rectangle { anchors.fill: parent; radius: 8; color: "transparent"; border.width: 2; border.color: Theme.acc; visible: s.ring }
    Row { x: 10; anchors.verticalCenter: parent.verticalCenter; spacing: 10
        Icon { name: s.icon; size: 16; color: s.on ? Theme.acc : Theme.t3; anchors.verticalCenter: parent.verticalCenter }
        Text { text: s.text; color: s.on ? Theme.t1 : Theme.t2; font.family: Theme.font; font.pixelSize: 13; anchors.verticalCenter: parent.verticalCenter } }
    MouseArea { id: ma; anchors.fill: parent; hoverEnabled: true; onClicked: s.clicked() }
}
