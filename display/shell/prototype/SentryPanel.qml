import QtQuick
import QtQuick.Controls
import "Lang.js" as Tr

// Sentry · red local, 780 px wide, two-column device grid. Passive: opening it starts no scan
// and changes nothing on any device; there are no action buttons because no control contract
// exists. States of the inventory are all distinct: not configured, scanner not run yet,
// unreadable, invalid, empty ("ran and found nothing"), stale, truncated.
Panel {
    id: sp
    kind: "window"
    readonly property string wid: "sentry"
    property var bridge
    width: 780
    height: Math.min(900, parent ? Math.max(240,parent.height-y-24) : 900, content.implicitHeight + 46)
    // Merged over full defaults: the bridge's own fallback (state only, before `bridge` is even
    // assigned, or for frames older than this field) omits devices/path/etc., and every binding
    // below reads them unconditionally (QML property bindings do not share another property's
    // `visible` guard), so a bare { state: ... } would throw on first paint.
    readonly property var lan: Object.assign(
        { state: "not_configured", devices: [], total: 0, age_seconds: 0, path: "", reason: "", stale: false },
        (bridge && bridge.lan) || {})
    function ageText(s) { return s < 90 ? s + " s" : (s < 5400 ? Math.round(s / 60) + " min" : Math.round(s / 3600) + " h") }
    function deviceIcon(c) { return ({ television: "tv", printer: "printer", speaker: "speaker", camera: "camera" })[c] || "devunk" }

    // A dashed explanatory box ("what this state means").
    component Void: Item {
        property string title
        property string body
        property string iconName: "unknown"
        default property alias extra: extraCol.data
        width: parent ? parent.width : 300; height: vc.implicitHeight + 64
        Rectangle { anchors.fill: parent; radius: 12; color: Theme.raised }
        DashedRect { anchors.fill: parent; radius: 12; color: Theme.t3 }
        Column { id: vc; anchors.horizontalCenter: parent.horizontalCenter; y: 34; spacing: 6; width: Math.min(parent.width - 80, 470)
            Item { width: 84; height: 84; anchors.horizontalCenter: parent.horizontalCenter
                Canvas { id:emptyCanvas; anchors.fill: parent
                    Connections { target:Theme; function onColorModeChanged() { emptyCanvas.requestPaint() } }
                    onPaint: { var c = getContext("2d"); c.clearRect(0, 0, 84, 84); c.lineCap = "round"
                        c.strokeStyle = Theme.tint(Theme.t3,0.8); c.lineWidth = 1.5; c.setLineDash([3, 7]); c.beginPath(); c.arc(42, 42, 36, 0, 6.2832); c.stroke()
                        c.strokeStyle = Theme.tint(Theme.t3,0.6); c.setLineDash([2, 6]); c.beginPath(); c.arc(42, 42, 20, 0, 6.2832); c.stroke()
                        c.setLineDash([]); c.strokeStyle = Theme.t2; c.lineWidth = 5; c.beginPath(); c.moveTo(42, 42); c.lineTo(42.01, 42); c.stroke() } } }
            Item { width: 1; height: 10 }
            Text { text: title; width: parent.width; horizontalAlignment: Text.AlignHCenter; color: Theme.t1; font.family: Theme.disp; font.pixelSize: 17; font.weight: Font.DemiBold }
            Text { text: body; width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; color: Theme.t2; font.family: Theme.font; font.pixelSize: 13; lineHeight: 18; lineHeightMode: Text.FixedHeight }
            Column { id: extraCol; width: parent.width; spacing: 8 } }
    }

    Item { id:tbar; width:parent.width; height:52
        Rectangle { anchors.bottom:parent.bottom; width:parent.width; height:1; color:Theme.hair }
        TitleDrag { wid:sp.wid }
        Lights { x:16; anchors.verticalCenter:parent.verticalCenter; wid:sp.wid }
        Text { anchors.centerIn:parent; text:"Sentry"; color:Theme.t1; font.family:Theme.disp; font.pixelSize:17 }
    }
    Rectangle {
        x:1; y:tbar.height; width:parent.width-2; height:Math.max(0,parent.height-y-1)
        bottomLeftRadius:Theme.rWindow-1; bottomRightRadius:Theme.rWindow-1
        color:Theme.reading
    }
    Flickable {
        objectName:"projectionScroll"; x:0; y:tbar.height; width:parent.width; height:parent.height-tbar.height
        contentWidth:width; contentHeight:content.implicitHeight+46; clip:true
        boundsBehavior:Flickable.StopAtBounds
        ScrollBar.vertical:ScrollBar { }
    Column { id: content; x: 22; y: 22; width: parent.width - 46; spacing: 0
        Text { text: Tr.t("Sentry · red local"); color: Theme.t1; font.family: Theme.disp; font.pixelSize: 22; font.weight: Font.DemiBold; font.letterSpacing: -0.33 }
        Flow { width: parent.width; topPadding: 12; spacing: 8
            SourceChip { bridge: sp.bridge; projection: ({ source: sp.bridge && sp.bridge.link === "fixture" ? "fixture" : sp.lan.state === "ok" ? "live" : "unavailable", state: sp.lan.state }) }
            // inventory state
            Chip { visible: lan.state === "ok"; icon: lan.stale ? "stale" : "done"; tone: lan.stale ? "warn" : "ok"
                text: lan.stale ? Tr.t("inventario desactualizado") : Tr.t("inventario actualizado"); sub: Tr.t("hace ") + sp.ageText(lan.age_seconds) }
            Chip { visible: lan.state === "missing"; icon: "unknown"; tone: "unk"; text: Tr.t("escáner"); sub: Tr.t("sin ejecutar") }
            Chip { visible: lan.state === "not_configured"; icon: "unknown"; tone: "unk"; text: Tr.t("inventario"); sub: Tr.t("no configurado") }
            Chip { visible: lan.state === "unreadable"; icon: "bad"; tone: "bad"; text: Tr.t("sin permiso para leer el inventario") }
            Chip { visible: lan.state === "invalid"; icon: "warn"; tone: "warn"; text: Tr.t("inventario no válido") }
            // device counter: "—" when unknown, never 0
            Chip { visible: lan.state === "ok"; text: lan.total + (lan.total === 1 ? Tr.t(" dispositivo") : Tr.t(" dispositivos")) }
            Chip { visible: lan.state !== "ok"; icon: "unknown"; tone: "unk"; text: Tr.t("dispositivos"); sub: "—" }
        }
        NoteBox { visible: sp.lan.state === "bridge_silent"; width: parent.width; text: Tr.t("El bridge está en silencio. El inventario retenido es histórico; no confirma presencia ni estado actual.") }
        Item { width: 1; height: 16 }
        Text { visible: lan.state === "ok" && lan.total > lan.devices.length; width: parent.width
            text: Tr.t("Se muestran ") + lan.devices.length + Tr.t(" de ") + lan.total + "."; color: Theme.warn; font.family: Theme.font; font.pixelSize: 12; bottomPadding: 8 }

        // ok + devices: two-column grid
        Grid { visible: lan.state === "ok" && lan.devices.length > 0; columns: 2; spacing: 12; width: parent.width
            Repeater { model: lan.state === "ok" ? lan.devices : []
                delegate: Rectangle { width: (content.width - 12) / 2; height: dv.implicitHeight + 28; radius: Theme.rCard
                    color: Theme.raised; border.width: 1; border.color: Theme.hair
                    Column { id: dv; x: 14; y: 14; width: parent.width - 28; spacing: 8
                        Item { width: parent.width; height: 36
                            Rectangle { width: 36; height: 36; radius: 9; color: Theme.raised; border.width: 1; border.color: Theme.hair
                                Icon { anchors.centerIn: parent; name: sp.deviceIcon(modelData.class); size: 20; color: Theme.t2 } }
                            Column { x: 46; width: parent.width - 46 - kindChip.width - 8; anchors.verticalCenter: parent.verticalCenter
                                Text { width: parent.width; text: modelData.name !== "" ? modelData.name : modelData.address; elide: Text.ElideRight
                                    color: Theme.t1; font.family: Theme.font; font.pixelSize: 14; font.weight: Font.Medium }
                                Text { text: modelData.name !== "" ? modelData.address : Tr.t("sin nombre"); color: Theme.t3; font.family: Theme.font; font.pixelSize: 11 } }
                            Chip { id: kindChip; small: true; anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                                text: modelData.class ? modelData.class : Tr.t("tipo sin determinar"); tone: modelData.class ? "neutral" : "unk" } }
                        Text { width: parent.width; elide: Text.ElideRight; color: Theme.t2; font.family: Theme.font; font.pixelSize: 12
                            text: (modelData.manufacturer || modelData.model) ? [modelData.manufacturer, modelData.model, modelData.address].filter(function (x) { return x !== "" }).join(" · ")
                                                                               : Tr.t("fabricante y modelo no declarados · ") + modelData.address }
                        Text { width: parent.width; wrapMode: Text.Wrap; color: Theme.t3; font.family: Theme.font; font.pixelSize: 11
                            text: Object.keys(modelData.state).length > 0 ? Object.keys(modelData.state).map(function (k) { return k + " " + modelData.state[k] }).join("   ") : Tr.t("no declara nada de sí mismo") }
                        Row { spacing: 6
                            Repeater { model: modelData.seen_by; delegate: Chip { small: true; text: modelData } } } } } } }

        // ok but empty: a real reading, different from "not run"
        Void { visible: lan.state === "ok" && lan.total === 0; title: Tr.t("El escáner corrió y no encontró nada")
            body: Tr.t("Sí hay lectura: «0 dispositivos» con la hora de la pasada. Es otro estado que «aún no ha corrido», y se muestra distinto.") }
        // missing: scanner not run
        Void { visible: lan.state === "missing"; title: Tr.t("El escáner aún no ha corrido")
            body: Tr.t("Todavía no hay ningún inventario que mostrar. Hasta que Sentry termine su primera pasada, esta lista no tiene datos, ni positivos ni negativos.")
            Row { spacing: 8; anchors.horizontalCenter: parent.horizontalCenter
                Text { text: Tr.t("se espera en"); color: Theme.t3; font.family: Theme.font; font.pixelSize: 11; anchors.verticalCenter: parent.verticalCenter }
                Text { text: lan.path; color: Theme.t1; font.family: "monospace"; font.pixelSize: 12; anchors.verticalCenter: parent.verticalCenter }
                Chip { small: true; tone: "unk"; text: Tr.t("archivo inexistente") } } }
        Void { visible: lan.state === "not_configured"; title: Tr.t("El inventario no está configurado")
            body: Tr.t("El bridge no sabe dónde está el inventario de la red local, así que no puede decir nada de ella.") }
        Void { visible: lan.state === "unreadable" || lan.state === "invalid"; title: lan.state === "unreadable" ? Tr.t("No se puede leer el inventario") : Tr.t("El inventario no es válido")
            body: lan.reason ? lan.reason : "" }
        // the two states side by side, so they are never confused
        Grid { visible: lan.state === "missing"; columns: 2; spacing: 12; width: parent.width; topPadding: 12
            Rectangle { width: (content.width - 12) / 2; height: c1.implicitHeight + 24; radius: Theme.rCard; color: Theme.raised; border.width: 1; border.color: Theme.hair
                Column { id: c1; x: 14; y: 12; spacing: 8; width: parent.width - 28
                    Chip { small: true; icon: "unknown"; tone: "unk"; text: Tr.t("aún no ha corrido") }
                    Text { width: parent.width; wrapMode: Text.Wrap; color: Theme.t2; font.family: Theme.font; font.pixelSize: 12; lineHeight: 17; lineHeightMode: Text.FixedHeight
                        text: Tr.t("No se sabe cuántos dispositivos hay. El contador queda en «—», nunca en 0.") } } }
            Rectangle { width: (content.width - 12) / 2; height: c2.implicitHeight + 24; radius: Theme.rCard; color: Theme.raised; border.width: 1; border.color: Theme.hair
                Column { id: c2; x: 14; y: 12; spacing: 8; width: parent.width - 28
                    Chip { small: true; icon: "done"; tone: "ok"; text: Tr.t("corrió y no encontró nada") }
                    Text { width: parent.width; wrapMode: Text.Wrap; color: Theme.t2; font.family: Theme.font; font.pixelSize: 12; lineHeight: 17; lineHeightMode: Text.FixedHeight
                        text: Tr.t("Sí hay lectura: «0 dispositivos» con la hora de la pasada. Es otro estado, y se muestra distinto.") } } } }

        Item { width: 1; height: 18 }
        Rectangle { width: parent.width; height: 1; color: Theme.hair }
        Row { topPadding: 14; spacing: 8; width: parent.width
            Icon { name: "lock"; size: 14; color: Theme.t3; y: 15 }
            Text { width: parent.width - 24; wrapMode: Text.Wrap; color: Theme.t3; font.family: Theme.font; font.pixelSize: 12; lineHeight: 17; lineHeightMode: Text.FixedHeight
                text: Tr.t("Inventario producido por Sentry. Cada dispositivo indica sus fuentes de detección; esta vista no inicia escaneos ni ejecuta operaciones sobre dispositivos.") } }
    }
    }
}
