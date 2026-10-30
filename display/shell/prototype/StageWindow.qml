import QtQuick
import "Lang.js" as Tr

// Stage: a window of its own program. Its inside (compositor, agent cursor, captions) is outside
// this redesign, so the window only says so. The window itself is real: it opens, moves,
// minimises, zooms and closes like the others (see Wm).
Panel {
    id: st
    kind: "window"
    // This shell is the stage's own desktop: the stage is not elsewhere, it is around it.
    readonly property bool live: typeof insideStage !== "undefined" && insideStage
    readonly property string wid: "stage"
    width: 600; height: 400
    enterScale: 0.96
    durIn: Theme.durOpen; durOut: Theme.durClose

    Item { width: parent.width; height: 52
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.hair }
        TitleDrag { wid: st.wid }
        Lights { x: 16; anchors.verticalCenter: parent.verticalCenter; wid: st.wid }
        Row { anchors.centerIn: parent
            Text { text: "Stage"; color: Theme.t1; font.family: Theme.disp; font.pixelSize: 17; font.weight: Font.DemiBold; font.letterSpacing: -0.17 }
            Text { text: st.live ? Tr.t(" — este escritorio") : Tr.t(" — vista local"); color: Theme.t2; font.family: Theme.disp; font.pixelSize: 17; font.weight: Font.Medium; font.letterSpacing: -0.17 } } }
    Rectangle {
        x:1; y:52; width:parent.width-2; height:Math.max(0,parent.height-y-1)
        bottomLeftRadius:Theme.rWindow-1; bottomRightRadius:Theme.rWindow-1
        color:Theme.reading
    }
    Column { x: 20; y: 72; width: parent.width - 40; spacing: 16
        Row { spacing: 14
            Chip { icon: st.live ? "pointer" : "unknown"; tone: st.live ? "ok" : "unk"; text: st.live ? Tr.t("Autoridad: abajo a la derecha") : Tr.t("Autoridad: sin lectura"); anchors.verticalCenter: parent.verticalCenter }
            Row { spacing: 6; anchors.verticalCenter: parent.verticalCenter
                Icon { name: "pointer"; size: 14; color: Theme.t3; anchors.verticalCenter: parent.verticalCenter }
                Text { text: Tr.t("Core y Policy autorizan las operaciones"); color: Theme.t2; font.family: Theme.font; font.pixelSize: 13; anchors.verticalCenter: parent.verticalCenter } } }
        Item { width: parent.width; height: st.height - 72 - 20 - 34
            DashedRect { anchors.fill: parent; radius: Theme.rCard; color: Theme.hair }
            Rectangle { anchors.fill: parent; radius: Theme.rCard; color: Theme.raised }
            DashedRect { anchors.fill: parent; radius: Theme.rCard; color: Theme.hair }
            Column { anchors.centerIn: parent; spacing: 4
                Text { anchors.horizontalCenter: parent.horizontalCenter; text: st.live ? Tr.t("Estás dentro del Stage") : Tr.t("Sin canal gráfico del compositor"); color: Theme.t2; font.family: Theme.font; font.pixelSize: 13 }
                Text { anchors.horizontalCenter: parent.horizontalCenter; text: st.live ? Tr.t("Las aplicaciones que Archie usa se abren enmarcadas sobre este escritorio") : Tr.t("Esta vista no inicia Stage ni representa su sesión"); color: Theme.t3; font.family: Theme.font; font.pixelSize: 11 } } } }
}
