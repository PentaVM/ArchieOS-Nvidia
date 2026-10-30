import QtQuick
import "Lang.js" as Tr

// Provenance of a particular projection, not a verdict about another service.
Chip {
    property var bridge: null
    property var projection: null
    readonly property string source: projection && projection.source ? projection.source : "unavailable"
    readonly property bool old: !!(bridge && bridge.silent) || !!(projection &&
        (projection.state === "stale" || projection.daemon_reachable === false))
    readonly property bool missing: !projection || source === "unavailable" || projection.state === "unavailable"
    text: source === "fixture" ? Tr.t("Grabación") : missing ? Tr.t("Sin lectura") : old ? Tr.t("Último estado") : Tr.t("Observado")
    tone: source === "fixture" || old ? "warn" : missing ? "unk" : "neutral"
    icon: source === "fixture" ? "sample" : old ? "stale" : missing ? "unknown" : "done"
    sub: old && bridge ? Math.floor(bridge.ageMs / 1000) + Tr.t(" s sin trama") : ""
}
