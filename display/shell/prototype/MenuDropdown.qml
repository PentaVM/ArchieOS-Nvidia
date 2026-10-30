import QtQuick
import "ActionRules.js" as ActionRules
import "Lang.js" as Tr

// Drop-down of the top-bar menus (Archivo, Editar, Vista, Ir, Ventana). Each entry runs an
// action by id in Main.run(). An entry with no contract on this host is not listed at all;
// one that cannot apply right now (no window in front, nothing selected) is disabled and dim.
Panel {
    id: md
    kind: "panel"
    property string name: Tr.t("Archivo")
    property var actionContext: ({})
    property var vault
    signal activated(string id)
    width: 270
    height: col.implicitHeight + 12

    readonly property var items: {
        var v = vault
        var m = {
            [Tr.t("Archivo")]: [
                { l: Tr.t("Nueva pestaña"), id: "vault.newTab", k: "Ctrl T" },
                { l: Tr.t("Cerrar pestaña"), id: "vault.closeTab" },
                { l: Tr.t("Abrir seleccionado"), id: "vault.open", k: "↵", dis: !v || v.selected < 0 },
                { l: Tr.t("Subir un nivel"), id: "vault.up", k: "⌫" },
                { sep: true },
                { l: Tr.t("Buscar…"), id: "overlay.search", k: "/" },
                { l: Tr.t("Paleta de comandos"), id: "overlay.palette", k: "Ctrl K" } ],
            [Tr.t("Editar")]: [
                { l: Tr.t("Orden: ") + (!v ? Tr.t("nombre") : [Tr.t("nombre A→Z"), Tr.t("nombre Z→A"), Tr.t("más recientes")][v.sortMode]), id: "vault.sort" } ],
            [Tr.t("Vista")]: [
                { l: Tr.t("Lista"), id: "vault.list", k: "G", on: !!v && !v.gridView },
                { l: Tr.t("Cuadrícula"), id: "vault.grid", k: "G", on: !!v && v.gridView },
                { sep: true },
                { l: Tr.t("Movimiento reducido"), id: "theme.motion", k: "M", on: Theme.reducedMotion },
                { l: Tr.t("Superficies opacas"), id: "theme.opaque", k: "O", on: Theme.opaque },
                { l: Tr.t("Cristal con desenfoque"), id: "theme.blur", k: "B", on: Theme.blur },
                { sep: true },
                { l: Tr.t("Navegar la esfera en 3D"), id: "orb.nav", k: Tr.t("Ctrl Espacio"), on: Demo.orbNav },
                { l: Tr.t("Escritorio vacío"), id: "desk.toggle", k: "H", on: Demo.bare } ],
            // Vault's own places, so the menu and the sidebar never disagree.
            [Tr.t("Ir")]: (v ? v.places : []).map(function(p, i) {
                return { l: p.name, id: "go:" + i, on: !!v && v.place === i } }),
            [Tr.t("Ventana")]: [
                { l: Tr.t("Minimizar"), id: "win.min", k: "Ctrl M", dis: Wm.front() === "" },
                { l: "Zoom", id: "win.zoom", dis: Wm.front() === "" },
                { l: Tr.t("Cerrar ventana"), id: "win.close", k: "Ctrl W", dis: Wm.front() === "" },
                { sep: true },
                { l: Tr.t("Minimizar todas"), id: "win.minAll", k: "Ctrl ⇧ M", dis: Wm.shownCount() === 0 },
                { l: Tr.t("Mostrar todas"), id: "win.showAll", dis: Wm.openCount() === 0 },
                { l: Tr.t("Cerrar todas"), id: "win.closeAll", k: "Ctrl ⇧ W", dis: Wm.openCount() === 0 },
                { l: Tr.t("Mostrar el escritorio"), id: "win.desktop", k: "D", on: Demo.bare },
                { sep: true },
                { l: "Vault", id: "app.vault", on: Wm.isShown("vault") },
                { l: "Forge", id: "app.forge", k: "F", on: Wm.isShown("forge") },
                { l: "Sentry", id: "app.sentry", k: "S", on: Wm.isShown("sentry") },
                { l: Tr.t("Stage · vista local"), id: "window.stage", on: Wm.isShown("stage") },
                { l: Tr.t("Iniciar aplicación Stage"), id: "app.stage" },
                { l: Tr.t("Ajustes"), id: "app.set", k: ",", on: Wm.isShown("set") },
                { sep: true },
                { l: Tr.t("Cambiar de ventana"), id: "overlay.switcher", k: "W" },
                { l: Tr.t("Notificaciones"), id: "overlay.notif", k: "N" },
                { l: Tr.t("Actividad"), id: "overlay.activity", k: "A" },
                { l: Tr.t("Centro de control"), id: "overlay.control", k: "C" },
                { l: "Inspector", id: "overlay.inspector", k: "I" } ]
        }
        // Hide what has no contract here, then the separators left at an edge or doubled.
        var rows = (m[name] || []).filter(function(row) { return row.sep || ActionRules.allowed(row.id, md.actionContext) })
        return rows.filter(function(row, i) {
            return !row.sep || (i > 0 && i < rows.length - 1 && !rows[i - 1].sep)
        })
    }

    Column { id: col; x: 6; y: 6; width: parent.width - 12
        Repeater { model: md.items
            delegate: Item { objectName: modelData.sep ? "" : "menu." + modelData.id; width: col.width; height: modelData.sep ? 9 : 30
                Rectangle { visible: modelData.sep === true; x: 8; y: 4; width: parent.width - 16; height: 1; color: Theme.hair }
                Rectangle { visible: modelData.sep !== true; anchors.fill: parent; radius: 7
                    color: (ima.containsMouse && !modelData.dis) ? Theme.hover : "transparent"
                    Icon { visible: modelData.on === true; x: 8; anchors.verticalCenter: parent.verticalCenter; name: "done"; size: 14; color: Theme.acc }
                    Text { x: 28; anchors.verticalCenter: parent.verticalCenter; text: modelData.l || ""
                        color: modelData.dis ? Theme.tdis : Theme.t1; font.family: Theme.font; font.pixelSize: 13 }
                    Text { anchors.right: parent.right; anchors.rightMargin: 10; anchors.verticalCenter: parent.verticalCenter; text: modelData.k || ""
                        color: Theme.t3; font.family: Theme.font; font.pixelSize: 11 }
                    MouseArea { id: ima; anchors.fill: parent; hoverEnabled: true; enabled: !modelData.dis
                        onClicked: md.activated(modelData.id) } } } }
    }
}
