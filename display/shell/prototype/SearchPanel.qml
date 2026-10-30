import QtQuick
import "Lang.js" as Tr

// Searches local views through one eligible list for keyboard and mouse. It neither searches
// the web while typing nor claims a permission to operate files or change policy.
Panel {
    id: sp
    kind: "panel"
    width: 720
    height: foot.y + foot.height
    enterDy: -8
    durIn: Theme.durPalette; durOut: Theme.durPaletteClose
    property var bridge: null
    property string st: bridge ? (bridge.silent ? "stale" : (bridge.link === "waiting" || bridge.link === "displayd_offline" || bridge.link === "daemon_unreachable" ? "offline" : bridge.link === "invalid" ? "error" : "normal")) : "normal"
    property string query: inp.text
    property int current: 0
    signal activated(string act)
    function m(a, b) { var q = query.trim().toLowerCase(); return q === "" || (a + " " + b).toLowerCase().indexOf(q) >= 0 }
    readonly property var acts: [
        { t: Tr.t("Mostrar actividad de Archie"), s: Tr.t("Abre la vista local de trabajos"), a: "overlay.activity", icon: "activity", requiresLive: false },
        { t: Tr.t("Abrir la vista general de ventanas"), s: Tr.t("Ventanas locales de esta shell"), a: "overlay.switcher", icon: "windows", requiresLive: false } ]
    readonly property var eligibleRows: acts.filter(function(r) {
        return shown && st !== "error" && st !== "vacio" && r.a !== "" && m(r.t, r.s)
            && (!r.requiresLive || (st !== "offline" && st !== "degraded" && st !== "stale"))
    })
    readonly property bool noMatch: eligibleRows.length === 0
    function activateIndex(i) { var row = eligibleRows[i]; if (!shown || !row) return false; activated(row.a); return true }
    function activateCurrent() { return activateIndex(current) }
    onQueryChanged: current = 0
    onEligibleRowsChanged: current = Math.max(0, Math.min(current, eligibleRows.length - 1))
    onShownChanged: if (shown) { inp.text = ""; current = 0; inp.forceActiveFocus() }

    component Result: Rectangle {
        id: r
        property string glyph
        property string title
        property string sub
        property string kind
        property string statusText
        property string statusIcon
        property string statusTone: "neutral"
        property bool on: false
        property bool dis: false
        property bool ring: false
        property string act: ""
        property int rowIndex: -1
        width: parent ? parent.width : 600; height: 50; radius: 9
        color: (on || ring) ? Theme.sel : "transparent"; border.width: 1; border.color: on ? Theme.tint(Theme.acc, 0.35) : "transparent"
        Rectangle { anchors.fill: parent; radius: 9; color: "transparent"; border.width: 2; border.color: Theme.acc; visible: r.ring }
        Rectangle { x: 12; anchors.verticalCenter: parent.verticalCenter; width: 32; height: 32; radius: 8; color: Theme.raised; border.width: 1; border.color: Theme.hair
            Icon { anchors.centerIn: parent; name: r.glyph; size: 16; color: r.dis ? Theme.tdis : Theme.t2 } }
        Column { x: 56; anchors.verticalCenter: parent.verticalCenter; width: parent.width - 56 - 230
            Text { width: parent.width; elide: Text.ElideRight; text: r.title; color: r.dis ? Theme.tdis : Theme.t1; font.family: Theme.font; font.pixelSize: 14; font.weight: Font.Medium }
            Text { width: parent.width; elide: Text.ElideRight; text: r.sub; color: Theme.t3; font.family: Theme.font; font.pixelSize: 12 } }
        Row { anchors.right: parent.right; anchors.rightMargin: 12; anchors.verticalCenter: parent.verticalCenter; spacing: 8
            Chip { kind: true; small: true; text: r.kind; anchors.verticalCenter: parent.verticalCenter }
            Chip { visible: r.statusText !== ""; small: true; text: r.statusText; icon: r.statusIcon; tone: r.statusTone; anchors.verticalCenter: parent.verticalCenter } }
        MouseArea { anchors.fill: parent; enabled: !r.dis && r.act !== ""; hoverEnabled: true; onEntered: sp.current = r.rowIndex; onClicked: sp.activateIndex(r.rowIndex) }
    }

    Column { id: col; x: 16; y: 16; width: parent.width - 32; spacing: 0
        Item { width: parent.width; height: 40
            Rectangle { anchors.fill: parent; radius: 12; color: Theme.raised; border.width: 1; border.color: Theme.hair }
            Row { x: 14; anchors.verticalCenter: parent.verticalCenter; spacing: 10
                Icon { name: "search"; size: 18; color: Theme.t2; anchors.verticalCenter: parent.verticalCenter }
                Item { width: 440; height: 26; anchors.verticalCenter: parent.verticalCenter
                    TextInput { id: inp; anchors.fill: parent; verticalAlignment: TextInput.AlignVCenter; clip: true
                        color: Theme.t1; font.family: Theme.font; font.pixelSize: 17; selectionColor: Theme.acc; selectedTextColor: Theme.onAccent; cursorDelegate: Rectangle { width: 1.5; color: Theme.acc }
                        Keys.onPressed: function (e) {
                            if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter) { sp.activateCurrent(); e.accepted = true }
                            else if (e.key === Qt.Key_Down) { sp.current = Math.min(sp.eligibleRows.length - 1, sp.current + 1); e.accepted = true }
                            else if (e.key === Qt.Key_Up) { sp.current = Math.max(0, sp.current - 1); e.accepted = true } } }
                    Text { visible: inp.text === ""; text: Tr.t("Buscar acciones y capacidades"); color: Theme.tdis; font.family: Theme.font; font.pixelSize: 14; anchors.verticalCenter: parent.verticalCenter } } }
            Row { anchors.right: parent.right; anchors.rightMargin: 12; anchors.verticalCenter: parent.verticalCenter; spacing: 10
                Chip { visible: sp.st !== "normal" || Demo.focusRing; anchors.verticalCenter: parent.verticalCenter
                    text: sp.st === "vacio" ? Tr.t("0 resultados") : (sp.st === "error" ? Tr.t("error de lectura") : (sp.st === "offline" ? Tr.t("archied no responde") : (sp.st === "degraded" ? Tr.t("datos parciales") : (sp.st === "stale" ? "desactualizado" : "actualizado"))))
                    sub: sp.bridge ? Tr.t("hace ") + Math.floor(sp.bridge.ageMs / 1000) + " s" : Tr.t("vista local")
                    icon: sp.st === "vacio" ? "recents" : (sp.st === "error" || sp.st === "offline" ? "bad" : (sp.st === "degraded" ? "warn" : (sp.st === "stale" ? "recents" : "done")))
                    tone: sp.st === "vacio" ? "neutral" : (sp.st === "error" || sp.st === "offline" ? "bad" : (sp.st === "degraded" || sp.st === "stale" ? "warn" : "ok")) }
                Kbd { label: "Esc"; anchors.verticalCenter: parent.verticalCenter } } }
        Item { width: 1; height: 4 }

        ErrBox { visible: sp.st === "error"; text: Tr.t("No se pudo consultar el catálogo: respuesta no válida del bridge.") }
        VoidBox { visible: sp.st === "vacio" || sp.noMatch; icon: "search"; title: sp.query === "" ? Tr.t("Sin resultados") : Tr.t("Sin resultados para «") + sp.query + "»"; text: Tr.t("No hay acciones ni capacidades que coincidan y estén permitidas para ti.") }

        Column { visible: !sp.noMatch; width: parent.width
            Eyebrow { text: Tr.t("Vistas locales"); leftPadding: 12; topPadding: 12; bottomPadding: 4 }
            Repeater { model: sp.eligibleRows
                delegate: Result { rowIndex: index; act: modelData.a; glyph: modelData.icon; title: modelData.t; sub: modelData.s; kind: Tr.t("Vista"); on: index === sp.current } }
        }
        Item { width: 1; height: 8 }
        NoteBox { text: Tr.t("Leer o escribir archivos y cambiar permisos no está disponible: no hay un contrato de ejecución desde esta búsqueda.") }
        Item { visible: sp.st === "offline" || sp.st === "stale"; width: 1; height: 10 }
        NoteBox { visible: sp.st === "offline"; text: Tr.t("El catálogo de capacidades no se puede filtrar por permisos sin archied.") }
        NoteBox { visible: sp.st === "stale"; icon: "recents"; text: Tr.t("Sin lectura actual del catálogo.") }
        Item { width: 1; height: 8 }
    }
    FootHints { id: foot; y: col.y + col.implicitHeight + 4; x: 0; width: sp.width
        hints: [["↑↓", "elegir"], ["↵", "abrir"], ["Esc", "cerrar"]]; note: Tr.t("sin autoridad de ejecución") }
}
