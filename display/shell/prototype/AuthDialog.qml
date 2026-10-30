import QtQuick
import "Lang.js" as Tr

// Authorization dialog, 440 px, centred. DESIGN: Policy has no way to escalate a request to the
// shell yet (no IPC), so "Permitir" is disabled and says why. Nothing is approved by default:
// the focus starts on "Rechazar" and Esc closes the dialog granting nothing.
Panel {
    id: ad
    kind: "panel"
    width: 440
    height: col.implicitHeight + 40
    enterScale: 0.98
    durIn: Theme.durPanel; durOut: Theme.durPanel
    signal rejected()

    Column { id: col; x: 20; y: 20; width: parent.width - 40; spacing: 0
        Item { width: parent.width; height: 24
            Eyebrow { text: Tr.t("Autorización"); anchors.verticalCenter: parent.verticalCenter }
            Chip { design: true; icon: "sample"; text: Tr.t("Diseño · sin contrato"); anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter } }
        Item { width: 1; height: 10 }
        Text { width: parent.width; wrapMode: Text.Wrap; text: Synthetic.authorization.title; color: Theme.t1
            font.family: Theme.disp; font.pixelSize: 18; font.weight: Font.DemiBold; font.letterSpacing: -0.27; lineHeight: 24; lineHeightMode: Text.FixedHeight }
        Item { width: 1; height: 6 }
        Text { text: Synthetic.authorization.path; color: Theme.t2; font.family: Theme.font; font.pixelSize: 12 }
        Row { spacing: 8; topPadding: 12; bottomPadding: 14
            Chip { icon: "waiting"; tone: "warn"; text: "pendiente" }
            Chip { icon: "pause"; text: "escalada" } }
        Row { spacing: 10
            Btn { text: Tr.t("Rechazar"); ring: true; onClicked: ad.rejected() }
            Btn { text: Tr.t("Permitir"); dis: true } }
        Item { width: 1; height: 10 }
        Why { text: Tr.t("Permitir no está disponible: Policy todavía no tiene una vía para escalar autorizaciones.") }
        Item { width: 1; height: 12 }
        NoteBox { text: Tr.t("Nada se aprueba por omisión: el foco empieza en Rechazar y el diálogo se cierra con Esc sin conceder nada.") }
    }
}
