pragma Singleton
import QtQuick
import "Lang.js" as Tr

// SYNTHETIC DESIGN DATA, taken from the Archie Mac mockups. Nothing here comes from archied,
// Sentry or the filesystem; every screen that uses it says "Diseño" (the DATOS DE DISEÑO notice
// and per-section chips). The bridge-fed parts (link, control center, activity jobs, Sentry)
// do not read from here.
QtObject {
    readonly property var dock: [
        { id: "orb",    name: "Orb",     icon: "dkOrb",    running: true  },
        { id: "world",  name: "World",   icon: "dkWorld",  running: false },
        { id: "vault",  name: "Vault",   icon: "dkVault",  running: true  },
        { id: "forge",  name: "Forge",   icon: "dkForge",  running: false },
        { id: "sentry", name: "Sentry",  icon: "dkSentry", running: false },
        { id: "stage",  name: "Stage",   icon: "dkStage",  running: true  },
        { id: "set",    name: Tr.t("Ajustes"), icon: "dkSet",    running: false }
    ]
    readonly property var tabs: [Tr.t("Entregas"), "Workspaces"]
    readonly property var crumbs: [Tr.t("Inicio"), "Archie", "motor-bracket"]
    readonly property var places: [
        { name: Tr.t("Entregas"),   icon: "deliveries", domain: "" },
        { name: "Workspaces", icon: "workspaces", domain: "" },
        { name: Tr.t("Documentos"), icon: "documents",  domain: "personal" },
        { name: Tr.t("Trabajo"),    icon: "work",       domain: "work" },
        { name: Tr.t("Recientes"),  icon: "recents",    domain: "" },
        { name: Tr.t("Papelera"),   icon: "trash",      domain: "" }
    ]
    // icon tint: folders t2, otherwise the file category colour.
    readonly property var files: [
        { name: "motor-bracket",     icon: "folder", cat: "",          size: "—",      kind: Tr.t("carpeta"), when: Tr.t("hoy 14:21"),    domain: "",         archie: "" },
        { name: "soporte-v3.step",   icon: "cad",    cat: "other",     size: "2,4 MB", kind: "",        when: Tr.t("hoy 14:18"),    domain: "",         archie: "14:18" },
        { name: "informe-q3.docx",   icon: "doc",    cat: "documents", size: "148 KB", kind: "",        when: Tr.t("ayer 18:40"),   domain: "work",     archie: "" },
        { name: "render-final.mp4",  icon: "video",  cat: "media",     size: "86 MB",  kind: "",        when: "10 oct 09:12", domain: "",         archie: "", loading: true },
        { name: "fotos-viaje",       icon: "folder", cat: "",          size: "—",      kind: Tr.t("carpeta"), when: "3 oct 21:05",  domain: "personal", archie: "" },
        { name: "presupuesto.xlsx",  icon: "sheet",  cat: "data",      size: "62 KB",  kind: "",        when: "8 oct 11:30",  domain: "work",     archie: "" },
        { name: "notas.md",          icon: "doc",    cat: "documents", size: "3 KB",   kind: "",        when: "11 oct 22:47", domain: "personal", archie: "" },
        { name: "esquema.kicad_pcb", icon: "cad",    cat: "other",     size: "910 KB", kind: "",        when: Tr.t("hoy 13:56"),    domain: "",         archie: "13:56" },
        { name: "boceto.svg",        icon: "image",  cat: "media",     size: "44 KB",  kind: "",        when: "5 oct 16:02",  domain: "",         archie: "", loading: true }
    ]
    readonly property var queue: ({ title: Tr.t("Copiando a Entregas"), detail: Tr.t("3 de 9 archivos"), progress: 1 / 3,
                                    permission: Tr.t("Permiso concedido · lectura de /archie/workspaces/motor-bracket"),
                                    undoWhy: Tr.t("Deshacer: aún no hay nada que deshacer"), updated: Tr.t("actualizado hace 12 s") })
    readonly property var authorization: ({ title: Tr.t("Archie pide escribir en motor-bracket"), path: "/archie/workspaces/motor-bracket" })
    readonly property var audit: [
        { t: "14:09:12", text: Tr.t("Policy concedió lectura de /archie/workspaces/motor-bracket (1 operación)"), kind: "done" },
        { t: "14:08:40", text: Tr.t("Policy rechazó una escritura fuera del espacio de trabajo"), kind: "bad" },
        { t: "14:07:55", text: Tr.t("Stage: Archie hizo clic en KiCad (recibo: executed)"), kind: "archie" }
    ]
    readonly property var quick: [
        { name: "Wi-Fi", icon: "wifi" }, { name: "Bluetooth", icon: "bt" },
        { name: Tr.t("Silencio"), icon: "moon" }, { name: Tr.t("Privacidad"), icon: "eye" }
    ]
    // Contents of the two sample folders, for double-click navigation (still design data).
    readonly property var children: ({
        "motor-bracket": [
            { name: "soporte-v3.step",   icon: "cad",   cat: "other", size: "2,4 MB", kind: "", when: Tr.t("hoy 14:18"), domain: "", archie: "14:18" },
            { name: "esquema.kicad_pcb", icon: "cad",   cat: "other", size: "910 KB", kind: "", when: Tr.t("hoy 13:56"), domain: "", archie: "13:56" },
            { name: "render-final.mp4",  icon: "video", cat: "media", size: "86 MB",  kind: "", when: "10 oct 09:12", domain: "", archie: "", loading: true },
            { name: "notas.md",          icon: "doc",   cat: "documents", size: "3 KB", kind: "", when: "11 oct 22:47", domain: "personal", archie: "" } ],
        "fotos-viaje": [
            { name: "IMG_0412.jpg", icon: "image", cat: "media", size: "3,1 MB", kind: "", when: "3 oct 21:05", domain: "personal", archie: "" },
            { name: "IMG_0413.jpg", icon: "image", cat: "media", size: "2,8 MB", kind: "", when: "3 oct 21:06", domain: "personal", archie: "" },
            { name: "IMG_0420.jpg", icon: "image", cat: "media", size: "3,4 MB", kind: "", when: "3 oct 21:14", domain: "personal", archie: "" } ]
    })
    // What each sidebar place lists (sample data, no filesystem).
    function placeFiles(i) {
        var f = files
        if (i === 0) return f
        if (i === 1) return f.filter(function (x) { return x.icon === "folder" })
        if (i === 2) return f.filter(function (x) { return x.domain === "personal" })
        if (i === 3) return f.filter(function (x) { return x.domain === "work" })
        if (i === 4) return f.filter(function (x) { return x.archie !== "" || x.when.indexOf(Tr.t("hoy")) === 0 || x.when.indexOf(Tr.t("ayer")) === 0 })
        return []
    }
    // Catalogue of the palette: apps, the sample files and actions. `act` is what Main.run() does;
    // `why` marks an entry that has no contract yet (disabled). `pref` = shown with an empty query.
    readonly property var catalog: [
        { group: "Apps", icon: "dkOrb",    name: "Orb",      hint: Tr.t("Escritorio con el Orbe"), kind: "App", act: "app.orb" },
        { group: "Apps", icon: "dkVault",  name: "Vault",    hint: Tr.t("Gestor de archivos"), kind: "App", act: "app.vault" },
        { group: "Apps", icon: "dkForge",  name: "Forge",    hint: Tr.t("Catálogo de puentes y pipeline"), kind: "App", act: "app.forge" },
        { group: "Apps", icon: "dkSentry", name: "Sentry",   hint: Tr.t("Red local · inventario de dispositivos"), kind: "App", act: "app.sentry", pref: true },
        { group: "Apps", icon: "dkStage",  name: "Stage",    hint: Tr.t("Ventana de composición"), kind: "App", act: "app.stage", pref: true },
        { group: "Apps", icon: "dkSet",    name: Tr.t("Ajustes"),  hint: Tr.t("Proveedores, modelos y diagnóstico"), kind: "App", act: "app.set" },
        // Vault's places (VaultWindow.places, same order): read-only navigation.
        { group: Tr.t("Carpetas"), icon: "personal", name: Tr.t("Inicio"), hint: Tr.t("Vault · tu carpeta personal"), kind: Tr.t("Carpeta"), act: "go:0", pref: true },
        { group: Tr.t("Carpetas"), icon: "spaces", name: Tr.t("Escritorio"), hint: Tr.t("Vault · Escritorio"), kind: Tr.t("Carpeta"), act: "go:1" },
        { group: Tr.t("Carpetas"), icon: "documents", name: Tr.t("Documentos"), hint: Tr.t("Vault · Documentos"), kind: Tr.t("Carpeta"), act: "go:2" },
        { group: Tr.t("Carpetas"), icon: "deliveries", name: Tr.t("Descargas"), hint: Tr.t("Vault · Descargas"), kind: Tr.t("Carpeta"), act: "go:3" },
        { group: Tr.t("Carpetas"), icon: "image", name: Tr.t("Imágenes"), hint: Tr.t("Vault · Imágenes"), kind: Tr.t("Carpeta"), act: "go:4" },
        { group: Tr.t("Carpetas"), icon: "stack", name: Tr.t("Sistema"), hint: Tr.t("Vault · la raíz del sistema"), kind: Tr.t("Carpeta"), act: "go:5" },
        { group: Tr.t("Acciones"), icon: "activity", name: Tr.t("Mostrar actividad de Archie"), hint: Tr.t("Abre el panel de trabajos y autorizaciones"), kind: Tr.t("Acción"), act: "overlay.activity", pref: true },
        { group: Tr.t("Acciones"), icon: "windows",  name: Tr.t("Abrir la vista general de ventanas"), hint: Tr.t("Muestra las ventanas por app y por dominio"), kind: Tr.t("Acción"), act: "overlay.switcher" },
        { group: Tr.t("Acciones"), icon: "bell",     name: Tr.t("Mostrar notificaciones"), hint: Tr.t("Historial de avisos de Archie"), kind: Tr.t("Acción"), act: "overlay.notif" },
        { group: Tr.t("Acciones"), icon: "control",  name: Tr.t("Abrir el centro de control"), hint: Tr.t("Estado del sistema y medidores"), kind: Tr.t("Acción"), act: "overlay.control" },
        { group: Tr.t("Acciones"), icon: "trace",    name: Tr.t("Abrir el inspector"), hint: Tr.t("IRIS y Cortex"), kind: Tr.t("Acción"), act: "overlay.inspector" },
        { group: Tr.t("Acciones"), icon: "dkOrb",    name: Tr.t("Cambiar al Orbe"), hint: Tr.t("Volver al escritorio con el Orbe"), kind: Tr.t("Acción"), act: "app.orb", pref: true },
        { group: Tr.t("Acciones"), icon: "windows",  name: Tr.t("Mostrar todas las ventanas"), hint: Tr.t("Restaura las ventanas minimizadas"), kind: Tr.t("Acción"), act: "win.showAll" },
        { group: Tr.t("Acciones"), icon: "dkSet",    name: Tr.t("Abrir los ajustes"), hint: Tr.t("Proveedores, modelos, autoridad y apariencia"), kind: Tr.t("Acción"), act: "app.set" }
    ]
    function search(q) {
        q = (q || "").trim().toLowerCase()
        return catalog.filter(function (c) {
            return q === "" ? c.pref === true : (c.name + " " + c.hint).toLowerCase().indexOf(q) >= 0 })
    }
    readonly property string query: "s"
    readonly property var results: [
        { group: "Apps",     icon: "dkSentry", name: "Sentry",   hint: Tr.t("Red local · inventario de dispositivos"), kind: "App" },
        { group: "Apps",     icon: "dkStage",  name: "Stage",    hint: Tr.t("Ventana de composición"),                kind: "App" },
        { group: Tr.t("Archivos"), icon: "file",     name: "soporte-v3.step", hint: Tr.t("Vault · Diseño / Mecánica · modificado hoy 13:56"), kind: Tr.t("Archivo") },
        { group: Tr.t("Acciones"), icon: "activity", name: Tr.t("Mostrar actividad de Archie"), hint: Tr.t("Abre el panel de trabajos y autorizaciones"), kind: Tr.t("Acción") },
        { group: Tr.t("Acciones"), icon: "dkOrb",    name: Tr.t("Cambiar al Orbe"), hint: Tr.t("Volver al modo principal"), kind: Tr.t("Acción"), why: Tr.t("sin contrato todavía") }
    ]
}
