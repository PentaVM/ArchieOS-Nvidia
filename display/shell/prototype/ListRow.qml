import QtQuick

// Row of a list: optional 32 px glyph, title (+ optional sub line), trailing chips.
Rectangle {
    id: r
    property string glyph
    property url iconSource: ""
    property string title
    property string sub
    property bool mono: false
    property bool on: false
    property bool ring: false
    property bool dim: false
    default property alias trailing: tr.data
    width: parent ? parent.width : 400
    height: Math.max(44, lbl.implicitHeight + 12)
    radius: 8; color: (on || ring) ? Theme.sel : "transparent"
    Rectangle { anchors.fill: parent; radius: 8; color: "transparent"; border.width: 2; border.color: Theme.acc; visible: r.ring }
    Rectangle { id: gl; visible: r.glyph !== "" || String(r.iconSource).length>0; x: 12; anchors.verticalCenter: parent.verticalCenter; width: 32; height: 32; radius: 8
        color: String(r.iconSource).length>0 ? Theme.iconSurface : Theme.raised; border.width: 1; border.color: Theme.hair
        Image { id: logo; anchors.centerIn:parent; width:24; height:24; source:r.iconSource; sourceSize:Qt.size(48,48); fillMode:Image.PreserveAspectFit; asynchronous:true; visible:status===Image.Ready }
        Icon { anchors.centerIn: parent; visible:logo.status!==Image.Ready; name: r.glyph; size: 16; color: String(r.iconSource).length>0 ? Theme.iconInk : Theme.t2 } }
    Column { id: lbl; x: gl.visible ? 56 : 12; anchors.verticalCenter: parent.verticalCenter; width: parent.width - x - tr.width - 24
        Text { width: parent.width; elide: Text.ElideRight; text: r.title; color: r.dim ? Theme.t3 : Theme.t1
            font.family: r.mono ? "monospace" : Theme.font; font.pixelSize: r.mono ? 12 : 13; font.weight: Font.Medium }
        Text { visible: r.sub !== ""; width: parent.width; elide: Text.ElideRight; text: r.sub; color: Theme.t3; font.family: Theme.font; font.pixelSize: 11 } }
    Row { id: tr; anchors.right: parent.right; anchors.rightMargin: 12; anchors.verticalCenter: parent.verticalCenter; spacing: 8 }
}
