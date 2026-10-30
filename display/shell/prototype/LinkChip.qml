import QtQuick
import "Lang.js" as Tr

// Honest link state of the whole shell, derived only from BridgeModel.link. Seven states, each
// with its own icon, word and tone (icons and tones as in the Archie Mac components sheet).
Chip {
    id: c
    property var bridge
    // The local bridge process could not start or stopped: its reason, in place of a link state.
    property string hostError: ""
    readonly property var spec: ({
        waiting:            { t: Tr.t("conectando…"),               i: "waiting",    k: "info" },
        live:               { t: Tr.t("archied en vivo"),           i: "ok",         k: "ok" },
        fixture:            { t: Tr.t("GRABACIÓN · no es en vivo"), i: "tape",       k: "warn" },
        daemon_unreachable: { t: Tr.t("archied no responde"),       i: "bad",        k: "bad" },
        displayd_offline:   { t: Tr.t("displayd sin respuesta"),    i: "displayoff", k: "bad" },
        invalid:            { t: Tr.t("respuesta no válida"),       i: "warn",       k: "warn" },
        bridge_silent:      { t: Tr.t("bridge sin datos"),          i: "linkoff",    k: "warn" },
        host_stopped:       { t: Tr.t("puente local detenido"),     i: "linkoff",    k: "warn" }
    })
    readonly property var s: spec[hostError !== "" ? "host_stopped" : bridge.link]
    Accessible.description: hostError
    function ageText(ms) {
        var s = Math.round(ms / 1000)
        return s < 60 ? s + " s" : Math.round(s / 60) + " min"
    }
    text: s.t
    // "último estado hace N" only when a last state really exists (system != null).
    sub: bridge.link === "daemon_unreachable" && bridge.system !== null && bridge.staleMs > 0 ? Tr.t("último estado hace ") + ageText(bridge.staleMs) : ""
    icon: s.i
    tone: s.k
}
