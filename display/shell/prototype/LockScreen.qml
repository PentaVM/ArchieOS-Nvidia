import QtQuick
import "Lang.js" as Tr

// Lock / boot frame and install assistant. DESIGN SAMPLE: ArchieOS has no authentication service
// of its own, so the password field is disabled and says so; the boot list shows the real state
// of each service or "sin lectura", never a percentage. The assistant draws the shape of the
// existing installer (display/installer) and does not replace it.
Item {
    id: ls
    anchors.fill: parent
    property string st: Demo.state
    property bool installer: false
    signal leave()
    // vacio / error / degraded show the boot list; the rest show the lock.
    readonly property bool boot: st === "vacio" || st === "error" || st === "degraded"

    Background { anchors.fill: parent; nodes: false }
    DesignBanner { x: 14; y: 12 }
    // There is no authentication service, so nothing here unlocks anything: this only leaves the
    // demonstration screen and goes back to the desktop.
    Column { anchors.horizontalCenter: parent.horizontalCenter; y: parent.height - 120; spacing: 8
        Btn { anchors.horizontalCenter: parent.horizontalCenter; text: Tr.t("Volver al escritorio"); onClicked: ls.leave() }
        Row { anchors.horizontalCenter: parent.horizontalCenter; spacing: 6; Kbd { label: "Esc" }
            Text { text: Tr.t("salir de esta pantalla (demostración, no autentica)"); color: Theme.t3; font.family: Theme.font; font.pixelSize: 12 } } }

    component Check: Item {
        id: ck
        property string name
        property string word
        property string tone: "unk"
        width: parent ? parent.width : 300; height: 34
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.hair; visible: !ck.last }
        property bool last: false
        Row { spacing: 10; anchors.verticalCenter: parent.verticalCenter
            Text { text: ck.name; color: Theme.t2; font.family: Theme.font; font.pixelSize: 13 } }
        Chip { anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; small: true; text: ck.word
            icon: ck.tone === "ok" ? "done" : (ck.tone === "bad" ? "bad" : (ck.tone === "warn" ? "warn" : "unknown")); tone: ck.tone }
    }

    // ---- lock ------------------------------------------------------------------------------
    Panel { visible: !ls.installer && !ls.boot; kind: "panel"; x: (ls.width - width) / 2; y: 250; width: 440; height: lc.implicitHeight + 64
        Column { id: lc; x: 36; y: 34; width: parent.width - 72; spacing: 10
            Icon { anchors.horizontalCenter: parent.horizontalCenter; name: "archie"; size: 28; color: Theme.t1 }
            Text { anchors.horizontalCenter: parent.horizontalCenter; text: Demo.time; color: Theme.t1; font.family: Theme.disp; font.pixelSize: 68; font.weight: Font.Medium; font.letterSpacing: -2 }
            Text { anchors.horizontalCenter: parent.horizontalCenter; text: Demo.longDate; color: Theme.t2; font.family: Theme.font; font.pixelSize: 14 }
            Row { anchors.horizontalCenter: parent.horizontalCenter; spacing: 8
                Chip { design: true; icon: "sample"; text: Tr.t("Diseño · sin servicio") }
                StateChip { visible: ls.st === "offline" || ls.st === "stale"; st: ls.st; noun: "estados"
                    text: ls.st === "stale" ? Tr.t("estado del arranque desactualizado") : Tr.t("archied no responde") } }
            Item { width: parent.width; height: 40
                Rectangle { anchors.fill: parent; radius: 10; color: Theme.raised }
                DashedRect { anchors.fill: parent; radius: 10; color: Theme.hair }
                Rectangle { anchors.fill: parent; anchors.margins: -4; radius: 14; color: "transparent"; border.width: 2; border.color: Theme.acc; visible: Demo.focusRing }
                Row { x: 14; anchors.verticalCenter: parent.verticalCenter; spacing: 10
                    Icon { name: "lock"; size: 16; color: Theme.t3; anchors.verticalCenter: parent.verticalCenter }
                    Text { text: Tr.t("Contraseña"); color: Theme.tdis; font.family: Theme.font; font.pixelSize: 14; anchors.verticalCenter: parent.verticalCenter } } }
            Why { text: Tr.t("Desbloquear no está disponible: no hay servicio de autenticación propio.") }
            NoteBox { visible: ls.st === "offline"; icon: "stale"; text: Tr.t("Sin conexión con archied no se puede comprobar la sesión.") }
            Row { visible: Demo.focusRing; spacing: 6; Kbd { label: "Esc" } Text { text: Tr.t("salir del campo"); color: Theme.t3; font.family: Theme.font; font.pixelSize: 12 } } } }

    // ---- boot ------------------------------------------------------------------------------
    Panel { visible: !ls.installer && ls.boot; kind: "panel"; x: (ls.width - width) / 2; y: 250; width: 440; height: bc.implicitHeight + 40
        Column { id: bc; x: 20; y: 20; width: parent.width - 40; spacing: 12
            Row { spacing: 10
                Icon { name: "power"; size: 20; color: Theme.t1; anchors.verticalCenter: parent.verticalCenter }
                Text { text: ls.st === "error" ? Tr.t("archied no arrancó") : (ls.st === "degraded" ? Tr.t("Arranque incompleto") : Tr.t("Arrancando Archie"))
                    color: Theme.t1; font.family: Theme.disp; font.pixelSize: 20; font.weight: Font.DemiBold; font.letterSpacing: -0.3; anchors.verticalCenter: parent.verticalCenter } }
            Chip { design: true; icon: "sample"; text: Tr.t("Diseño · sin servicio") }
            Column { width: parent.width
                Check { name: "archied"; word: ls.st === "error" ? Tr.t("no arrancó") : (ls.st === "degraded" ? Tr.t("listo") : Tr.t("sin lectura")); tone: ls.st === "error" ? "bad" : (ls.st === "degraded" ? "ok" : "unk") }
                Check { name: "bridge"; word: ls.st === "degraded" ? Tr.t("bridge sin datos") : Tr.t("sin lectura"); tone: ls.st === "degraded" ? "warn" : "unk" }
                Check { name: "Policy"; word: Tr.t("sin lectura") }
                Check { name: "compositor"; last: true; word: ls.st === "vacio" ? Tr.t("sin lectura") : Tr.t("listo"); tone: ls.st === "vacio" ? "unk" : "ok" } }
            ErrBox { visible: ls.st === "error"; text: Tr.t("archied terminó con estado 1 antes de responder. No se muestra ningún avance inventado.") }
            NoteBox { visible: ls.st !== "error"; icon: "stale"; text: Tr.t("No hay progreso en %: solo el estado real de cada servicio, o «sin lectura» si no se sabe.") } } }

    // ---- install assistant -----------------------------------------------------------------
    Panel { visible: ls.installer; kind: "panel"; x: (ls.width - width) / 2; y: 250; width: 520; height: ic.implicitHeight + 40
        Column { id: ic; x: 20; y: 20; width: parent.width - 40; spacing: 0
            Item { width: parent.width; height: 24
                Eyebrow { text: Tr.t("Asistente de instalación"); anchors.verticalCenter: parent.verticalCenter }
                Chip { design: true; icon: "sample"; text: Tr.t("Diseño · ejemplo"); anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter } }
            Item { width: 1; height: 6 }
            Text { text: Tr.t("Primer arranque"); color: Theme.t1; font.family: Theme.disp; font.pixelSize: 18; font.weight: Font.DemiBold; font.letterSpacing: -0.27 }
            Item { width: 1; height: 10 }
            Repeater { model: [Tr.t("1 · Bienvenida"), Tr.t("2 · Carpeta de datos"), Tr.t("3 · Proveedores"), Tr.t("4 · Permisos"), Tr.t("5 · Listo")]
                delegate: Item { width: ic.width; height: 34
                    Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.hair; visible: index < 4 }
                    Row { spacing: 10; anchors.verticalCenter: parent.verticalCenter
                        Icon { name: "ok"; size: 14; color: Theme.t3; anchors.verticalCenter: parent.verticalCenter }
                        Text { text: modelData; color: Theme.t2; font.family: Theme.font; font.pixelSize: 13; anchors.verticalCenter: parent.verticalCenter } }
                    Chip { anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; small: true; tone: "unk"; icon: "unknown"; text: Tr.t("sin estado") } } }
            Item { width: 1; height: 12 }
            Btn { text: Tr.t("Siguiente"); dis: true }
            Item { width: 1; height: 8 }
            Why { text: Tr.t("Siguiente no está disponible en este marco: es solo la forma del asistente existente.") } } }
}
