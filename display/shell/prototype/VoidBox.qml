import QtQuick

// "Nothing to show" block: dashed, icon, title, one honest sentence.
Item {
    id: v
    property string icon: "lock"
    property string title
    property string text
    property bool small: true
    width: parent ? parent.width : 400
    height: col.implicitHeight + (small ? 44 : 60)
    Rectangle { anchors.fill: parent; radius: 12; color: Theme.raised }
    DashedRect { anchors.fill: parent; radius: 12; color: Theme.t3 }
    Column { id: col; anchors.centerIn: parent; width: Math.min(parent.width - 48, 380); spacing: 5
        Icon { anchors.horizontalCenter: parent.horizontalCenter; name: v.icon; size: 28; color: Theme.t3 }
        Item { width: 1; height: 5 }
        Text { width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; text: v.title
            color: Theme.t1; font.family: Theme.disp; font.pixelSize: 16; font.weight: Font.DemiBold; font.letterSpacing: -0.16 }
        Text { width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; text: v.text
            color: Theme.t2; font.family: Theme.font; font.pixelSize: 12; lineHeight: 17; lineHeightMode: Text.FixedHeight } }
}
