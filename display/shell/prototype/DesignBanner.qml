import QtQuick
import "Lang.js" as Tr

// "Datos de diseño": shown while synthetic data is on screen. Dashed amber, pill.
Item {
    implicitHeight: 26
    implicitWidth: row.implicitWidth + 21
    Rectangle { anchors.fill: parent; radius: height / 2; color: Theme.bar }
    DashedRect { anchors.fill: parent; radius: height / 2; color: Theme.tint(Theme.warn, 0.6) }
    Row { id: row; x: 9; anchors.verticalCenter: parent.verticalCenter; spacing: 8
        Icon { name: "sample"; size: 14; color: Theme.warn; anchors.verticalCenter: parent.verticalCenter }
        Text { text: Tr.t("Datos de diseño"); color: Theme.t1; font.family: Theme.disp; font.pixelSize: 11; font.weight: Font.DemiBold
            font.capitalization: Font.AllUppercase; font.letterSpacing: 1.05; anchors.verticalCenter: parent.verticalCenter }
    }
}
