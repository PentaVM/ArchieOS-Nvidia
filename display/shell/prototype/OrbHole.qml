import QtQuick
import "Lang.js" as Tr

// Placeholder where the Orbe lives. The compositor draws the sphere; this shell never does, so
// the design leaves a labelled hole. Shown only with the workspace menu.
Item {
    width: 520; height: 520
    Rectangle { anchors.fill: parent; radius: width / 2; color: Theme.raised }
    DashedRect { anchors.fill: parent; radius: width / 2; strokeWidth: 1.5; dash: [4, 4]; color: Theme.t3 }
    Column { anchors.centerIn: parent; spacing: 6
        Text { anchors.horizontalCenter: parent.horizontalCenter; text: Tr.t("ORBE"); color: Theme.t2; font.family: Theme.disp; font.pixelSize: 11; font.weight: Font.DemiBold; font.letterSpacing: 1.3 }
        Text { width: 240; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; text: Tr.t("Lo dibuja el compositor. No forma parte de este diseño.")
            color: Theme.t3; font.family: Theme.font; font.pixelSize: 11; lineHeight: 15; lineHeightMode: Text.FixedHeight } }
}
