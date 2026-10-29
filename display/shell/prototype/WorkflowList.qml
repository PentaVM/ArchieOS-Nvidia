import QtQuick
import "Lang.js" as Tr

Column {
    id:list
    property var records: []
    width: parent ? parent.width : 400
    spacing: 2
    Repeater {
        model: list.records
        delegate: ListRow {
            glyph: modelData.observed ? "activity" : "unknown"
            title: String(modelData.component || "componente")
            sub: modelData.observed
                ? String(modelData.phase || Tr.t("sin fase")) + " · " + modelData.count + Tr.t(" señales · edad ") + Math.floor(modelData.age_ms / 1000) + " s"
                : Tr.t("Sin señal observada; no demuestra que el servicio esté detenido")
            Chip { small: true; tone: modelData.observed ? "neutral" : "unk"
                text: !modelData.observed ? Tr.t("sin señal") : modelData.active ? Tr.t("actividad observada") : Tr.t("última observación") }
        }
    }
}
