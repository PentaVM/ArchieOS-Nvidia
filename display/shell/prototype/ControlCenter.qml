import QtQuick
import "Lang.js" as Tr

// Control center, 440 px wide. REAL sections come from archie-shell-bridge (archie-displayd);
// DISEÑO sections are placeholders with no service behind them.
Panel {
    id: cc
    kind: "panel"
    property var bridge
    width: 440
    height: content.implicitHeight + 40
    readonly property var sys: bridge.system

    Rectangle {
        x:12; y:12; width:parent.width-24; height:Math.max(0,parent.height-24)
        radius:Math.max(0,Theme.rPanel-6); color:Theme.reading
    }
    Column { id: content; x: 20; y: 20; width: parent.width - 40; spacing: 0
        Text { text: Tr.t("Centro de control"); color: Theme.t1; font.family: Theme.disp; font.pixelSize: 22; font.weight: Font.DemiBold; font.letterSpacing: -0.33 }
        Flow { width: parent.width; topPadding: 12; spacing: 8
            SourceChip { bridge:cc.bridge; projection:cc.sys }
            LinkChip { bridge: cc.bridge } }

        // Failure box: the real reason, and why the summary is hidden.
        Rectangle { visible: !sys && bridge.reason !== ""; width: parent.width; height: fail.implicitHeight + 24; radius: Theme.rCard
            color: Theme.tint(Theme.bad, 0.09); border.width: 1; border.color: Theme.tint(Theme.bad, 0.32)
            Row { id: fail; x: 14; y: 12; spacing: 12; width: parent.width - 28
                Icon { name: "bad"; size: 18; color: Theme.bad }
                Column { width: parent.width - 30; spacing: 4
                    Text { text: Tr.t("No se puede leer el estado del sistema"); color: Theme.t1; font.family: Theme.font; font.pixelSize: 13; font.weight: Font.DemiBold }
                    Text { width: parent.width; text: bridge.reason; wrapMode: Text.Wrap; maximumLineCount: 3; elide: Text.ElideRight
                        color: Theme.t2; font.family: Theme.font; font.pixelSize: 12; lineHeight: 17; lineHeightMode: Text.FixedHeight }
                    Text { width: parent.width; wrapMode: Text.Wrap; text: Tr.t("Resumen del sistema oculto: solo se muestra si el daemon respondió."); color: Theme.t3; font.family: Theme.font; font.pixelSize: 11 } } } }
        Item { visible: !sys && bridge.reason !== ""; width: 1; height: 16 }

        // System summary: only when archied answered.
        Item { visible: sys !== null; width: 1; height: 16 }
        Rectangle { visible: sys !== null; width: parent.width; height: kv.implicitHeight + 28; radius: Theme.rCard
            color: Theme.raised; border.width: 1; border.color: Theme.hair
            Grid { id: kv; x: 14; y: 14; columns: 3; columnSpacing: 16; rowSpacing: 12; width: parent.width - 28
                Repeater { model: sys ? [
                    [Tr.t("Equipo"), sys.hostname], [Tr.t("Versión"), sys.archieos_version], [Tr.t("Salud"), sys.health],
                    [Tr.t("Módulos"), sys.modules_loaded + " / " + sys.modules_installed], [Tr.t("Modelos"), String(sys.models_registered)],
                    [Tr.t("Tareas activas"), String(sys.active_tasks)] ] : []
                    delegate: Column { width: (kv.width - 32) / 3; spacing: 2
                        Text { text: modelData[0]; color: Theme.t3; font.family: Theme.font; font.pixelSize: 11 }
                        Row { spacing: 6
                            Icon { visible: modelData[0] === Tr.t("Salud") && modelData[1] === "healthy"; name: "done"; size: 14; color: Theme.ok; anchors.verticalCenter: parent.verticalCenter }
                            Text { text: modelData[1]; color: Theme.t1; font.family: Theme.font; font.pixelSize: 13; font.weight: Font.Medium; elide: Text.ElideRight; width: parent.parent.width - (modelData[0] === Tr.t("Salud") && modelData[1] === "healthy" ? 20 : 0) } } } } } }

        Item { width: 1; height: 22 }
        ResourceGauges { bridge: cc.bridge; width: parent.width }

        Rectangle { width: parent.width; height: 1; color: Theme.hair; y: 0 }
        Item { width: 1; height: 18 }
        // Only what works is offered. Silencio governs this shell's own notifications; Wi-Fi,
        // Bluetooth and privacy toggles have no service behind them here, so they are not shown.
        Item { id: silence; objectName: "control.silence"; width: (content.width - 10) / 2; height: 62
            readonly property bool on: Demo.silent
            Rectangle { anchors.fill: parent; radius: Theme.rCard; color: silence.on ? Theme.sel : (sma.containsMouse ? Theme.hover : Theme.raised)
                border.width: 1; border.color: silence.on ? Theme.tint(Theme.acc, 0.45) : Theme.hair }
            Column { x: 12; y: 12; spacing: 6
                Row { spacing: 8; Icon { name: "moon"; size: 20; color: silence.on ? Theme.acc : Theme.t1; anchors.verticalCenter: parent.verticalCenter }
                    Text { text: Tr.t("Silencio"); color: Theme.t1; font.family: Theme.font; font.pixelSize: 13; font.weight: Font.Medium; anchors.verticalCenter: parent.verticalCenter } }
                Row { spacing: 5; Icon { name: silence.on ? "done" : "unknown"; size: 12; color: silence.on ? Theme.acc : Theme.t3; anchors.verticalCenter: parent.verticalCenter }
                    Text { text: silence.on ? Tr.t("activado · solo local") : Tr.t("desactivado · solo local")
                        color: Theme.t3; font.family: Theme.font; font.pixelSize: 11; anchors.verticalCenter: parent.verticalCenter } } }
            MouseArea { id: sma; anchors.fill: parent; hoverEnabled: true; onClicked: Demo.silent = !Demo.silent } }

        // Core's emergency pause: refuses every AI dispatch and mutating lease until resumed.
        // The daemon admits only the owner of the node's inference profile, for both directions.
        Item { width: 1; height: 12 }
        Item { id: emergency; width: parent.width; height: emg.implicitHeight + 28
            readonly property bool host: typeof shellRuntime !== "undefined" && shellRuntime !== null
            readonly property string state: host ? shellRuntime.aiPolicy : "unknown"
            readonly property bool busy: host && shellRuntime.aiPolicyBusy
            Component.onCompleted: if (host) shellRuntime.aiPolicyRefresh()
            Connections { target: cc; function onShownChanged() { if (cc.shown && emergency.host) shellRuntime.aiPolicyRefresh() } }
            Rectangle { anchors.fill: parent; radius: Theme.rCard
                color: emergency.state === "paused" ? Theme.tint(Theme.warn, 0.14) : Theme.raised
                border.width: 1; border.color: emergency.state === "paused" ? Theme.tint(Theme.warn, 0.5) : Theme.hair }
            Row { id: emg; x: 14; y: 14; width: parent.width - 28; spacing: 12
                Icon { name: "pause"; size: 20; color: emergency.state === "paused" ? Theme.warn : Theme.t1 }
                Column { width: parent.width - 32 - pauseBtn.width - 12; spacing: 3
                    Text { text: Tr.t("Pausa de emergencia de la IA"); color: Theme.t1; font.family: Theme.font; font.pixelSize: 13; font.weight: Font.Medium }
                    Text { width: parent.width; wrapMode: Text.Wrap
                        text: !emergency.host ? Tr.t("Este host no puede hablar con archied.")
                            : emergency.busy ? Tr.t("Preguntando a Core…")
                            : emergency.state === "paused" ? Tr.t("En pausa: Core rechaza toda inferencia y toda acción.")
                            : emergency.state === "running" ? Tr.t("En marcha. Pausar detiene ya las inferencias admitidas.")
                            : (shellRuntime.aiPolicyNote || Tr.t("Sin respuesta de Core."))
                        color: emergency.state === "paused" ? Theme.warn : Theme.t3; font.family: Theme.font; font.pixelSize: 11 }
                    Text { visible: emergency.host && emergency.state !== "unknown" && shellRuntime.aiPolicyNote !== ""; width: parent.width; wrapMode: Text.Wrap
                        text: emergency.host ? shellRuntime.aiPolicyNote : ""; color: Theme.warn; font.family: Theme.font; font.pixelSize: 11 } }
                Btn { id: pauseBtn; objectName: "control.emergency"; anchors.verticalCenter: parent.verticalCenter
                    text: emergency.state === "paused" ? Tr.t("Reanudar") : emergency.state === "running" ? Tr.t("Pausar") : Tr.t("Reintentar")
                    dis: !emergency.host || emergency.busy
                    onClicked: emergency.state === "paused" ? shellRuntime.aiResume()
                             : emergency.state === "running" ? shellRuntime.aiPause() : shellRuntime.aiPolicyRefresh() } } }
    }
}
