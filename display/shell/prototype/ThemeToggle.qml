import QtQuick
import QtQuick.Controls
import "Lang.js" as Tr

AbstractButton {
    id: control
    objectName: "theme.toggle"
    property bool compact: false
    implicitWidth: compact ? 78 : 104
    implicitHeight: compact ? 26 : 32
    text: Theme.light ? Tr.t("Claro") : Tr.t("Oscuro")
    activeFocusOnTab: true
    Accessible.name: Theme.light ? Tr.t("Cambiar a tema oscuro") : Tr.t("Cambiar a tema claro")
    Accessible.description: Tr.t("Tema de toda la interfaz")
    onClicked: { Theme.toggleColorMode(); ToolTip.hide() }
    Keys.onReturnPressed: clicked()
    background: Rectangle {
        radius: 7
        color: control.down ? Theme.pressed : control.hovered ? Theme.hover : Theme.panel
        border.color: control.activeFocus ? Theme.acc : Theme.hair
        border.width: control.activeFocus ? 2 : 1
    }
    contentItem: Text {
        text: control.text
        color: Theme.t1
        font.family: Theme.font
        font.pixelSize: control.compact ? 11 : 12
        font.weight: Font.DemiBold
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
    }
    ToolTip.visible: hovered
    ToolTip.text: Theme.light ? Tr.t("Cambiar a tema oscuro") : Tr.t("Cambiar a tema claro")
    ToolTip.delay: 500
}
