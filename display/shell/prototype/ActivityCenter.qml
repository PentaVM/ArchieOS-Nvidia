import QtQuick
import QtQuick.Controls
import "Lang.js" as Tr

// Activity, 460 px wide. TRABAJOS LARGOS are real (status.json of archie-bridges via the bridge);
// Policy review remains unavailable until its typed graphical channel exists.
Panel {
    id: ac
    kind: "panel"
    property var bridge
    width: 460
    height: Math.min(900, parent ? Math.max(240,parent.height-y-24) : 900, content.implicitHeight + 40)
    // Merged over full defaults; see SentryPanel.qml's `lan` for why.
    readonly property var jobs: Object.assign(
        { state: "not_configured", jobs: [], unreadable: [], skipped: 0 },
        (bridge && bridge.jobs) || {})
    readonly property var spec: ({
        starting:  { t: "iniciando", i: "waiting",  k: "info" },
        running:   { t: Tr.t("en curso"),  i: "activity", k: "info" },
        done:      { t: Tr.t("listo"),     i: "done",     k: "ok" },
        cancelled: { t: "cancelado", i: "cancel",   k: "neutral" },
        failed:    { t: Tr.t("falló"),     i: "bad",      k: "bad" }
    })
    function ago(ms) { var s = Math.round(ms / 1000); return s < 90 ? s + " s" : (s < 5400 ? Math.round(s / 60) + " min" : Math.round(s / 3600) + " h") }
    function hhmm(ms) { var d = new Date(ms); return ("0" + d.getHours()).slice(-2) + ":" + ("0" + d.getMinutes()).slice(-2) }

    component SectionHead: Item {
        property string title
        default property alias chips: chipRow.data
        width: parent ? parent.width : 300; height: 22
        Text { text: parent.title.toUpperCase(); color: Theme.t3; font.family: Theme.disp; font.pixelSize: 10; font.weight: Font.DemiBold
            font.letterSpacing: 1.2; anchors.verticalCenter: parent.verticalCenter }
        Row { id: chipRow; anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; spacing: 8 }
    }
    component JobCard: Rectangle {
        id: jc
        property var job
        readonly property var s: ac.spec[job.state]
        width: parent ? parent.width : 300; height: col.implicitHeight + 28
        color: Theme.raised; border.width: 1; border.color: Theme.hair; radius: Theme.rCard
        Column { id: col; x: 16; y: 14; width: parent.width - 32; spacing: 9
            Item { width: parent.width; height: 22
                Text { text: jc.job.bridge + " · " + jc.job.kind; color: Theme.t1; font.family: Theme.font; font.pixelSize: 13; font.weight: Font.Medium; anchors.verticalCenter: parent.verticalCenter }
                Chip { anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; text: jc.s.t; icon: jc.s.i; tone: jc.s.k } }
            // a bar only when the job declared a total
            Rectangle { visible: jc.job.total > 0; width: parent.width; height: 4; radius: 2; color: Theme.track
                Rectangle { width: parent.width * Math.min(1, jc.job.done / Math.max(1, jc.job.total)); height: parent.height; radius: 2
                    color: jc.job.state === "done" ? Theme.ok : Theme.acc } }
            Text { text: (jc.job.total > 0 ? jc.job.done + Tr.t(" de ") + jc.job.total : jc.job.done + Tr.t(" iteraciones, sin total declarado"))
                         + " · " + jc.job.workspace + " · " + jc.job.id
                color: Theme.t2; font.family: Theme.font; font.pixelSize: 12; font.features: { "tnum": 1 } }
            Rectangle { visible: !!jc.job.error; width: parent.width; height: errT.implicitHeight + 16; radius: Theme.rRow
                color: Theme.tint(Theme.bad, 0.10); border.width: 1; border.color: Theme.tint(Theme.bad, 0.30)
                Row { x: 10; y: 8; spacing: 8; width: parent.width - 20
                    Icon { name: "bad"; size: 14; color: Theme.bad }
                    Text { id: errT; width: parent.width - 22; text: jc.job.error ? jc.job.error : ""; wrapMode: Text.Wrap; maximumLineCount: 3; elide: Text.ElideRight
                        color: Theme.bad; font.family: Theme.font; font.pixelSize: 12 } } }
            // Quiet is not stuck: say only what is known.
            Row { spacing: 6
                Icon { name: "stale"; size: 12; color: Theme.t3; anchors.verticalCenter: parent.verticalCenter }
                Text { anchors.verticalCenter: parent.verticalCenter; color: Theme.t3; font.family: Theme.font; font.pixelSize: 11; font.features: { "tnum": 1 }
                    text: (jc.job.state === "running" || jc.job.state === "starting") ? Tr.t("sin novedades hace ") + ac.ago(jc.job.idle_ms)
                        : (jc.job.state === "done" ? Tr.t("terminó a las ") : jc.job.state === "cancelled" ? Tr.t("se canceló a las ") : Tr.t("falló a las ")) + ac.hhmm(jc.job.updated_ms) } }
        }
    }

    Flickable {
        objectName:"projectionScroll"; anchors.fill:parent
        contentWidth:width; contentHeight:content.implicitHeight+40; clip:true
        boundsBehavior:Flickable.StopAtBounds
        ScrollBar.vertical:ScrollBar { }
    Column { id: content; x: 20; y: 20; width: parent.width - 40; spacing: 10
        Text { text: Tr.t("Actividad"); color: Theme.t1; font.family: Theme.disp; font.pixelSize: 22; font.weight: Font.DemiBold; font.letterSpacing: -0.33 }
        NoteBox { visible: ac.jobs.state === "bridge_silent"; width: parent.width; text: Tr.t("El bridge está en silencio. El historial retenido no acredita el estado actual de los trabajos.") }
        Item { width: 1; height: 2 }
        SectionHead { title: Tr.t("Trabajos largos"); SourceChip { bridge: ac.bridge; projection: ({ source: ac.bridge && ac.bridge.link === "fixture" ? "fixture" : ac.jobs.state === "ok" ? "live" : "unavailable", state: ac.jobs.state }) } }
        Text { visible: jobs.state === "not_configured"; width: parent.width; wrapMode: Text.Wrap
            text: Tr.t("Sin espacios de trabajo configurados: el bridge no sabe dónde mirar."); color: Theme.t3; font.family: Theme.font; font.pixelSize: 12 }
        Text { visible: jobs.state === "ok" && jobs.jobs.length === 0; text: Tr.t("Ningún trabajo largo en los espacios configurados."); color: Theme.t3; font.family: Theme.font; font.pixelSize: 12 }
        Text { visible: jobs.state === "ok" && jobs.skipped > 0; text: jobs.skipped + Tr.t(" sin leer (estado dañado o fuera del límite)."); color: Theme.warn; font.family: Theme.font; font.pixelSize: 12 }
        Repeater { model: jobs.state === "ok" ? jobs.unreadable : []
            delegate: Text { width: content.width; wrapMode: Text.Wrap; text: Tr.t("No se pudo leer ") + modelData[0] + ": " + modelData[1]; color: Theme.warn; font.family: Theme.font; font.pixelSize: 12 } }
        Repeater { model: jobs.state === "ok" ? jobs.jobs : []; delegate: JobCard { job: modelData } }
        // cancel is not offered: say so
        Item { visible: jobs.state === "ok" && jobs.jobs.length > 0; width: parent.width; height: 34
            DashedRect { anchors.fill: parent; radius: Theme.rRow; color: Theme.hair }
            Row { x: 12; anchors.verticalCenter: parent.verticalCenter; spacing: 8
                Icon { name: "lock"; size: 14; color: Theme.t3; anchors.verticalCenter: parent.verticalCenter }
                Text { text: Tr.t("Cancelar un trabajo no está disponible desde la shell todavía"); color: Theme.t3; font.family: Theme.font; font.pixelSize: 12; anchors.verticalCenter: parent.verticalCenter } } }

        NoteBox { width: parent.width; icon: "lock"
            text: Tr.t("La revisión de Policy y su auditoría todavía no tienen un canal gráfico. No hay operaciones pendientes deducidas desde muestras.") }
    }
    }
}
