import QtQuick
import QtQuick.Controls
import "qrc:/archie/shell/prototype" as Shell
import "../../shell/prototype/Lang.js" as Tr

// FORGE: the machine's applications as a desktop. Large icons, their names, and nothing
// else to operate: Archie opens and uses them; a person can still click one open.
// Archie's own come first, then everything installed, then, faintly, the well-known
// tools that are missing.
Item {
    id: forge

    property string notice: ""
    readonly property var apps: appCatalog.forgeApps.filter(function (a) { return a.section !== "suggested" })
    readonly property var missing: appCatalog.forgeApps.filter(function (a) { return a.section === "suggested" })

    Connections {
        target: appCatalog
        function onLaunched(id, ok, message) {
            forge.notice = ok ? "" : message
            noticeTimer.restart()
        }
    }
    Timer { id: noticeTimer; interval: 5000; onTriggered: forge.notice = "" }

    Flickable {
        id: desk
        anchors.fill: parent
        anchors.topMargin: 92
        anchors.leftMargin: 56
        anchors.rightMargin: 56
        contentHeight: column.implicitHeight + 60
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {
            policy: ScrollBar.AsNeeded
            contentItem: Rectangle { implicitWidth: 4; radius: 2; color: Shell.Theme.hair }
        }

        Column {
            id: column
            width: desk.width
            spacing: 34

            Grid {
                id: grid
                width: parent.width
                columns: Math.max(1, Math.floor(width / 132))
                columnSpacing: (width - columns * 112) / Math.max(1, columns - 1)
                rowSpacing: 26
                Repeater {
                    model: forge.apps
                    delegate: AppIcon { required property var modelData; app: modelData }
                }
            }

            Column {
                visible: forge.missing.length > 0
                width: parent.width
                spacing: 16
                Text { text: Tr.t("Para instalar"); color: Shell.Theme.t3; font.pixelSize: 12; font.weight: Font.Medium; font.letterSpacing: 0.4 }
                Grid {
                    width: parent.width
                    columns: grid.columns
                    columnSpacing: grid.columnSpacing
                    rowSpacing: 26
                    Repeater {
                        model: forge.missing
                        delegate: AppIcon { required property var modelData; app: modelData; faint: true }
                    }
                }
            }
        }
    }

    Text {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 22
        visible: forge.notice.length > 0
        text: forge.notice
        color: Shell.Theme.t2
        font.pixelSize: 12
        Rectangle { anchors.fill: parent; anchors.margins: -8; z: -1; radius: Shell.Theme.rCard; color: Shell.Theme.panel; border.color: Shell.Theme.hair }
    }

    component AppIcon: Item {
        id: icon
        property var app: ({})
        property bool faint: false
        readonly property bool ready: !!app.available && appCatalog.canLaunch
        width: 112
        height: 128

        Rectangle {
            id: tile
            anchors.horizontalCenter: parent.horizontalCenter
            width: 76; height: 76; radius: 20
            scale: area.pressed ? 0.94 : (area.containsMouse ? 1.05 : 1.0)
            Behavior on scale { NumberAnimation { duration: 140; easing.type: Easing.OutCubic } }
            color: Shell.Theme.iconSurface
            border.color: area.containsMouse ? Shell.Theme.acc : Shell.Theme.hair
            Image {
                id: image
                anchors.centerIn: parent
                width: 50; height: 50
                source: icon.app.icon || ""
                sourceSize: Qt.size(96, 96)
                fillMode: Image.PreserveAspectFit
                visible: status === Image.Ready
                asynchronous: true
                smooth: true
            }
            Text {
                anchors.centerIn: parent
                visible: image.status !== Image.Ready
                text: icon.app.glyph || "·"
                color: Shell.Theme.iconInk
                font.pixelSize: 30
                font.weight: Font.Light
            }
        }
        // Running: a small light under the icon, as a dock shows it.
        Rectangle {
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.top: tile.bottom
            anchors.topMargin: 6
            width: 5; height: 5; radius: 2.5
            visible: icon.app.running > 0
            color: Shell.Theme.ok
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.top: tile.bottom
            anchors.topMargin: 16
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: icon.app.name || ""
            color: icon.faint || !icon.app.available ? Shell.Theme.t3 : Shell.Theme.t1
            font.pixelSize: 12
            elide: Text.ElideRight
            maximumLineCount: 2
            wrapMode: Text.Wrap
        }
        MouseArea {
            id: area
            anchors.fill: parent
            hoverEnabled: true
            onClicked: if (!icon.faint) appCatalog.launch(icon.app.id)
        }
        ToolTip {
            id: appTip
            visible: area.containsMouse && (!!icon.app.comment || !!icon.app.missing)
            delay: 600
            text: icon.app.available ? (icon.app.comment || "") : (icon.app.missing || "")
            contentItem: Text { text: appTip.text; color: Shell.Theme.t1; font.family: Shell.Theme.font; font.pixelSize: 12; wrapMode: Text.Wrap }
            background: Rectangle { color: Shell.Theme.panel; border.color: Shell.Theme.hair; radius: Shell.Theme.rCard }
        }
    }
}
