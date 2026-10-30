import QtQuick
import QtQuick.Controls

// Window with title bar (traffic lights, centred title, trailing slot), a 200 px sidebar and a
// main area. Used by Forge and Settings. With `wid` set, the lights close, minimise and zoom it
// and the title bar drags it (see Wm); without it they are only drawn.
Panel {
    id: w
    kind: "window"
    property string title
    property string wid: ""
    default property alias main: mainCol.data
    property alias sidebar: sideCol.data
    property alias trailing: trail.data
    enterScale: 0.96
    durIn: Theme.durOpen; durOut: Theme.durClose

    Item { id: tbar; width: parent.width; height: 52
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.hair }
        TitleDrag { wid: w.wid; enabled: w.wid !== "" }
        Row { x: 16; anchors.verticalCenter: parent.verticalCenter; spacing: 8; visible: w.wid === ""
            Rectangle { width: 12; height: 12; radius: 6; color: Theme.tlClose; border.width: 1; border.color: Qt.rgba(0, 0, 0, 0.22) }
            Rectangle { width: 12; height: 12; radius: 6; color: Theme.tlMin; border.width: 1; border.color: Qt.rgba(0, 0, 0, 0.22) }
            Rectangle { width: 12; height: 12; radius: 6; color: Theme.tlZoom; border.width: 1; border.color: Qt.rgba(0, 0, 0, 0.22) } }
        Lights { x: 16; anchors.verticalCenter: parent.verticalCenter; wid: w.wid; visible: w.wid !== "" }
        Text { anchors.centerIn: parent; text: w.title; color: Theme.t1; font.family: Theme.disp; font.pixelSize: 17; font.weight: Font.DemiBold; font.letterSpacing: -0.17 }
        Row { id: trail; anchors.right: parent.right; anchors.rightMargin: 16; anchors.verticalCenter: parent.verticalCenter; spacing: 8 } }
    // Dense content resolves the glass over our canvas so overlapping windows
    // cannot contribute a second table or graph behind the text. Chrome stays glass.
    Rectangle {
        x: 1; y: tbar.height; width: parent.width - 2; height: Math.max(0,parent.height-y-1)
        bottomLeftRadius: Theme.rWindow - 1; bottomRightRadius: Theme.rWindow - 1
        color: Theme.reading
    }
    Rectangle { id: side; y: tbar.height; width: 200; height: parent.height - tbar.height - 1; color: Theme.side
        Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: Theme.hair }
        Column { id: sideCol; x: 10; y: 14; width: parent.width - 20; spacing: 2 } }
    Flickable {
        id:scroll; objectName:"appContentScroll"
        x:side.width+20; y:tbar.height+18
        width:parent.width-side.width-40; height:Math.max(0,parent.height-y-18)
        contentWidth:width; contentHeight:mainCol.implicitHeight; clip:true
        boundsBehavior:Flickable.StopAtBounds
        ScrollBar.vertical:ScrollBar { }
        Column { id:mainCol; width:scroll.width; spacing:12 }
    }
}
