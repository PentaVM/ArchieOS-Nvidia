import QtQuick
import "Lang.js" as Tr

// Machine pressure and temperature (archie-displayd /api/kernel via the bridge). RFC-006:
// unmeasured and idle never look the same. A source with no reading draws a dashed line and
// says why; it never draws an empty or zero bar.
Column {
    id: g
    property var bridge
    spacing: 10
    readonly property var kernel: bridge && bridge.kernel ? bridge.kernel : null
    readonly property var pressure: kernel && kernel.pressure_availability.state === "available" ? kernel.pressure : null
    readonly property string pressureWhy: !kernel ? Tr.t("sin lectura: el bridge no recibió telemetría")
        : (kernel.pressure_availability.state === "unavailable" ? kernel.pressure_availability.reason : "")
    readonly property bool thermal: kernel && kernel.thermal_availability.state === "available"
    readonly property string thermalWhy: !kernel ? Tr.t("sin lectura: el bridge no recibió telemetría")
        : (kernel.thermal_availability.state === "unavailable" ? kernel.thermal_availability.reason : "")
    function pct(w) { return Math.round(w) + " %" }
    // Verdict and the 60 s / 300 s windows of the resource that drove it (cpu when none is named).
    function verdictText() {
        if (!pressure) return ""
        var r = kernel.worst_resource
        var res = (r === "memory" || r === "io" || r === "cpu") ? pressure[r] : pressure.cpu
        return Tr.t("Veredicto de presión: ") + (kernel.verdict ? kernel.verdict : "—") + (r ? " (" + r + ")" : "")
            + " · 60 s: " + pct(res.some.avg60) + " · 300 s: " + pct(res.some.avg300)
    }
    function tone(v) { return v > 0.85 ? "bad" : (v > 0.7 ? "warn" : "ok") }

    // One meter row: icon · label · track · value.  v = 0..1 or -1 when not measured.
    component MeterRow: Item {
        id: m
        property string icon
        property string label
        property real v: -1
        property string valueText: ""
        property bool limit: false
        property string tone: "acc"
        width: parent ? parent.width : 300; height: 30
        Icon { x: 0; anchors.verticalCenter: parent.verticalCenter; name: m.icon; size: 16; color: Theme.t3 }
        Text { x: 26; width: 92; anchors.verticalCenter: parent.verticalCenter; text: m.label; elide: Text.ElideRight
            color: Theme.t2; font.family: Theme.font; font.pixelSize: 13 }
        Item { id: trk; x: 128; width: parent.width - 128 - 102; height: 6; anchors.verticalCenter: parent.verticalCenter
            Rectangle { visible: m.v >= 0; anchors.fill: parent; radius: 3; color: Theme.track
                Rectangle { width: parent.width * Math.min(1, m.v); height: parent.height; radius: 3
                    color: m.tone === "warn" ? Theme.warn : m.tone === "bad" ? Theme.bad : m.tone === "ok" ? Theme.ok : Theme.acc } }
            Rectangle { visible: m.v >= 0 && m.limit; x: parent.width - 2; y: -3; width: 2; height: 12; radius: 1; color: Theme.t2 }
            // no reading: dashed line only, no track
            Row { visible: m.v < 0; anchors.verticalCenter: parent.verticalCenter; spacing: 3
                Repeater { model: Math.max(0, Math.ceil(trk.width / 6)); Rectangle { width: 3; height: 2; color: Theme.t3 } } }
        }
        Row { anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; spacing: 5
            Icon { visible: m.v < 0 && m.valueText !== ""; name: "unknown"; size: 12; color: Theme.unk; anchors.verticalCenter: parent.verticalCenter }
            Icon { visible: m.v >= 0 && m.tone === "warn"; name: "warn"; size: 14; color: Theme.warn; anchors.verticalCenter: parent.verticalCenter }
            Text { text: m.valueText; color: m.v < 0 ? Theme.t2 : Theme.t1; font.family: Theme.font; font.features: { "tnum": 1 }
                font.pixelSize: m.v < 0 ? 12 : 13; font.weight: m.v < 0 ? Font.Medium : Font.DemiBold; anchors.verticalCenter: parent.verticalCenter }
        }
    }
    component Card: Rectangle {
        default property alias kids: col.data
        property string heading
        color: Theme.raised; border.width: 1; border.color: Theme.hair; radius: Theme.rCard
        width: parent ? parent.width : 300; height: col.implicitHeight + 22
        Column { id: col; x: 14; y: 10; width: parent.width - 28; spacing: 0
            Text { text: parent.parent.heading; color: Theme.t3; font.family: Theme.font; font.pixelSize: 11; bottomPadding: 4 } }
    }
    component Why: Row {
        property string text
        spacing: 6; topPadding: 6
        Text { text: parent.text; width: parent.parent ? parent.parent.width - 4 : 300; wrapMode: Text.Wrap; color: Theme.t3; font.family: Theme.font; font.pixelSize: 11 }
    }

    // Header: RECURSOS + the UI budget chip (real, from the telemetry, even with archied down).
    Item { width: parent.width; height: 22
        Text { text: Tr.t("RECURSOS"); color: Theme.t3; font.family: Theme.disp; font.pixelSize: 10; font.weight: Font.DemiBold
            font.letterSpacing: 1.2; anchors.verticalCenter: parent.verticalCenter }
        Chip { anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
            icon: kernel ? "ok" : "unknown"; tone: kernel ? "neutral" : "unk"
            text: kernel ? kernel.display_budget : Tr.t("presupuesto sin lectura") } }

    Card { heading: Tr.t("Presión · ventana de 10 s")
        MeterRow { icon: "cpu"; label: "CPU"; v: pressure ? pressure.cpu.some.avg10 / 100 : -1
            valueText: pressure ? pct(pressure.cpu.some.avg10) : Tr.t("no medido"); tone: pressure ? g.tone(pressure.cpu.some.avg10 / 100) === "ok" ? "acc" : g.tone(pressure.cpu.some.avg10 / 100) : "acc" }
        MeterRow { icon: "mem"; label: Tr.t("Memoria"); v: pressure ? pressure.memory.some.avg10 / 100 : -1
            valueText: pressure ? pct(pressure.memory.some.avg10) : Tr.t("no medido"); tone: pressure ? g.tone(pressure.memory.some.avg10 / 100) === "ok" ? "acc" : g.tone(pressure.memory.some.avg10 / 100) : "acc" }
        MeterRow { icon: "io"; label: Tr.t("E/S"); v: pressure ? pressure.io.some.avg10 / 100 : -1
            valueText: pressure ? pct(pressure.io.some.avg10) : Tr.t("no medido"); tone: pressure ? g.tone(pressure.io.some.avg10 / 100) === "ok" ? "acc" : g.tone(pressure.io.some.avg10 / 100) : "acc" }
        // Real data the old panel never showed: the verdict and the longer windows.
        Why { visible: pressure !== null; text: verdictText() }
        Why { visible: pressure === null; text: pressureWhy }
    }
    Card { heading: thermal ? Tr.t("Temperatura · hacia el límite crítico de cada zona") : Tr.t("Temperatura por zona")
        Repeater { model: thermal ? kernel.zones : []
            delegate: MeterRow { icon: "temp"; label: modelData.zone_type
                v: modelData.headroom_fraction === null || modelData.headroom_fraction === undefined ? -1 : modelData.headroom_fraction
                limit: v >= 0; tone: v >= 0 ? (g.tone(v) === "ok" ? "acc" : g.tone(v)) : "acc"
                valueText: modelData.celsius.toFixed(0) + " °C" } }
        MeterRow { visible: !thermal; icon: "temp"; label: Tr.t("Zonas"); v: -1; valueText: Tr.t("sin zonas") }
        // per zone without a declared critical trip: no scale, so no bar, and it says so
        Repeater { model: thermal ? kernel.zones : []
            delegate: Why { visible: modelData.headroom_fraction === null || modelData.headroom_fraction === undefined
                text: modelData.zone_type + Tr.t(": el firmware no declara un límite crítico: no hay escala") } }
        Why { visible: !thermal; text: thermalWhy }
    }
}
