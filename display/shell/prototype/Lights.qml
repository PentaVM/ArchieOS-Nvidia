import QtQuick
import "Lang.js" as Tr

// The three window lights, wired to the window manager: close, minimise (to the dock) and zoom.
// Grey while the window is not in front, coloured while it is or while the pointer is over the
// group; a glyph shows on hover so each one says what it does.
Item {
    id: l
    property string wid
    width: row.width
    height: 12
    readonly property bool lit: Wm.front() === wid || group.containsMouse

    // Hover over the whole group (the glyphs appear on all three together, as on a Mac).
    MouseArea { id: group; anchors.fill: parent; anchors.margins: -4; hoverEnabled: true; acceptedButtons: Qt.NoButton }
    Row {
        id: row
        spacing: 8
        Repeater {
            model: [ { c: Theme.tlClose, g: "×", act: "close", n: Tr.t("Cerrar") }, { c: Theme.tlMin, g: "–", act: "min", n: Tr.t("Minimizar") }, { c: Theme.tlZoom, g: "+", act: "zoom", n: Tr.t("Ampliar") } ]
            delegate: Rectangle {
                width: 12; height: 12; radius: 6
                Accessible.role: Accessible.Button
                Accessible.name: modelData.n + " " + Wm.title(l.wid)
                color: l.lit ? modelData.c : Theme.tlOff
                border.width: 1; border.color: Qt.rgba(0, 0, 0, 0.22)
                Text { anchors.centerIn: parent; anchors.verticalCenterOffset: -0.5; visible: group.containsMouse
                    text: modelData.g; color: Qt.rgba(0, 0, 0, 0.62); font.family: Theme.font; font.pixelSize: 10; font.weight: Font.Bold }
                MouseArea { anchors.fill: parent; anchors.margins: -2
                    onClicked: { if (modelData.act === "close") Wm.close(l.wid); else if (modelData.act === "min") Wm.minimize(l.wid); else Wm.toggleMax(l.wid) } }
            }
        }
    }
}
