import QtQuick
import "Lang.js" as Tr

// Notification toasts, 380 px, stacked at the top right. DESIGN SAMPLE. Three examples and the
// pile behind them with its counter; the counter changes without animation.
Item {
    id: ts
    width: 380
    height: col.implicitHeight
    property bool shown: true
    opacity: shown ? 1 : 0
    visible: opacity > 0
    transform: Translate { y: (ts.shown || !Theme.spatial) ? 0 : -12
        Behavior on y { NumberAnimation { duration: Theme.durToast; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeOut } } }
    Behavior on opacity { NumberAnimation { duration: ts.shown ? Theme.durToast : Theme.durToastOut; easing.type: Easing.BezierSpline; easing.bezierCurve: ts.shown ? Theme.easeOut : Theme.easeIn } }

    component Toast: Panel {
        id: t
        property string glyph
        property color glyphColor: Theme.t2
        property string title
        property string detail
        property real progress: -1
        property string chipText
        property string chipIcon
        property string chipTone: "neutral"
        width: 380; height: tc.implicitHeight + 24
        Rectangle { x: 14; y: 12; width: 32; height: 32; radius: 8; color: Theme.raised; border.width: 1; border.color: Theme.hair
            Icon { anchors.centerIn: parent; name: t.glyph; size: 16; color: t.glyphColor } }
        Column { id: tc; x: 58; y: 12; width: parent.width - 58 - 14; spacing: 2
            Text { width: parent.width; elide: Text.ElideRight; text: t.title; color: Theme.t1; font.family: Theme.font; font.pixelSize: 13; font.weight: Font.Medium }
            Text { width: parent.width; elide: Text.ElideRight; text: t.detail; color: Theme.t2; font.family: Theme.font; font.pixelSize: 12 }
            Item { visible: t.progress >= 0; width: parent.width; height: 12
                Rectangle { y: 4; width: parent.width; height: 4; radius: 2; color: Theme.track
                    Rectangle { width: parent.width * t.progress; height: 4; radius: 2; color: Theme.acc } } }
            Item { width: 1; height: 4 }
            Row { spacing: 8
                Chip { small: true; text: t.chipText; icon: t.chipIcon; tone: t.chipTone; anchors.verticalCenter: parent.verticalCenter }
                Text { text: "ahora"; color: Theme.t3; font.family: Theme.font; font.pixelSize: 11; anchors.verticalCenter: parent.verticalCenter } } }
    }

    Column { id: col; width: parent.width; spacing: 10
        Toast { glyph: "activity"; glyphColor: Theme.info; title: "freecad · sweep"; detail: Tr.t("31 de 50 · motor-bracket · job-7f3a"); progress: 0.62
            chipText: Tr.t("en curso"); chipIcon: "activity"; chipTone: "info" }
        Toast { glyph: "bad"; glyphColor: Theme.bad; title: Tr.t("Policy rechazó una escritura"); detail: Tr.t("Fuera del espacio de trabajo")
            chipText: Tr.t("rechazada"); chipIcon: "bad"; chipTone: "bad" }
        Toast { glyph: "dkSentry"; title: Tr.t("Dispositivo nuevo en la red"); detail: Tr.t("192.168.1.88 · visto por ssdp")
            chipText: "Sentry"; chipIcon: "dkSentry"; chipTone: "neutral" }
        Item { width: parent.width; height: 8
            Rectangle { x: 14; y: -3; width: parent.width - 28; height: 8; radius: 4; color: Theme.panel; border.width: 1; border.color: Theme.hair; opacity: 0.8 }
            Rectangle { x: 28; y: 2; width: parent.width - 56; height: 8; radius: 4; color: Theme.panel; border.width: 1; border.color: Theme.hair; opacity: 0.55 } }
        Item { width: parent.width; height: 24
            Chip { anchors.right: parent.right; icon: "stack"; text: Tr.t("+3 agrupadas") } }
    }
}
