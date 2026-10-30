.pragma library
.import "Lang.js" as Tr

// Local presentation actions are distinct from authority-bearing operations.
function allowed(id, context) {
    context = context || {}
    if (id === "orb.nav") return context.nativeReady === true
    if (id === "app.world") return context.worldReady === true
    if (id === "app.stage") return context.stageLauncher === true
    if (id.indexOf("dock.") === 0 || id.indexOf("window.") === 0)
        return ["vault","forge","sentry","stage","set"].indexOf(id.slice(id.indexOf(".") + 1)) >= 0
    // Vault reads the person's folders; it never creates, moves, copies or deletes a file.
    if (/^go:[0-5]$/.test(id)) return true
    if (["app.orb","app.vault","app.forge","app.sentry","app.set", "desk.toggle",
         "vault.newTab","vault.closeTab","vault.sort","vault.list","vault.grid","vault.open","vault.up",
         "theme.motion","theme.opaque", "win.close","win.min","win.zoom",
         "win.closeAll","win.minAll","win.showAll","win.desktop"].indexOf(id) >= 0) return true
    if (id === "theme.blur") return context.compositorBlur === true
    if (id.indexOf("overlay.") === 0)
        return ["activity","control","forge","inspector","settings","sentry","search",
                "palette","switcher","notif","workspace"].indexOf(id.slice(8)) >= 0
    return false
}
function reason(id, context) {
    if (allowed(id, context)) return ""
    if (id === "overlay.lock") return Tr.t("No hay un bloqueo de sesión conectado; una pantalla de muestra no bloquea el equipo.")
    if (id.indexOf("vault.") === 0 || id.indexOf("file:") === 0 || id.indexOf("go:") === 0)
        return Tr.t("Vault solo lee archivos: no copia, mueve ni borra.")
    if (id === "orb.nav") return Tr.t("El viewport nativo aún no está disponible.")
    if (id === "app.world") return Tr.t("World necesita el host nativo del escritorio.")
    if (id === "app.stage") return Tr.t("El lanzador de Stage no está disponible en este host.")
    if (id === "theme.blur") return Tr.t("El compositor no ha declarado soporte de desenfoque.")
    return Tr.t("Acción sin contrato disponible.")
}
