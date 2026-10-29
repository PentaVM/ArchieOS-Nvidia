import QtQuick
import QtQuick.Controls
import "Lang.js" as Tr

Panel {
    id:nc; kind:"panel"; width:540; height:Math.min(900, parent ? Math.max(240,parent.height-y-24) : 900, content.implicitHeight+40)
    property var bridge:null
    property string st:Demo.state
    readonly property var records: bridge && bridge.jobs && bridge.jobs.jobs ? bridge.jobs.jobs : []
    property var dismissed: []
    // A dismissal lasts until the job's state changes (running -> done/failed), not until
    // its next progress write: updated_ms moves every few seconds while a job runs.
    function key(record) { return JSON.stringify([record.workspace,record.id,record.state]) }
    readonly property var visibleRecords: records.filter(function(record) { return nc.dismissed.indexOf(nc.key(record))<0 })
    function dismiss(record) { var id=key(record); if (dismissed.indexOf(id)<0) dismissed=dismissed.concat([id]).slice(-256) }
    function dismissAll() { dismissed=records.map(function(record) { return nc.key(record) }) }
    function restore() { dismissed=[] }
    Flickable {
        objectName:"projectionScroll"; anchors.fill:parent
        contentWidth:width; contentHeight:content.implicitHeight+40; clip:true
        boundsBehavior:Flickable.StopAtBounds
        ScrollBar.vertical:ScrollBar { }
    Column {
        id:content; x:20; y:20; width:parent.width-40; spacing:12
        Eyebrow { text:Tr.t("Notificaciones · trabajos observados") }
        Row { spacing:8
            Btn { text:Tr.t("Descartar todos"); dis:nc.visibleRecords.length===0; onClicked:nc.dismissAll() }
            Btn { text:Tr.t("Mostrar descartados"); dis:nc.dismissed.length===0; onClicked:nc.restore() }
        }
        NoteBox { visible:nc.records.length>0 && nc.visibleRecords.length===0; text:Tr.t("Avisos descartados en esta vista. El historial del backend se conserva.") }
        NoteBox { visible:!!(nc.bridge && nc.bridge.silent); icon:"stale"; text:Tr.t("Último estado: el puente no está enviando tramas.") }
        VoidBox { visible:nc.records.length===0; icon:"bell"; title:Tr.t("Sin avisos observados")
            text:Tr.t("No hay trabajos en la proyección disponible. No se fabrican eventos de Policy ni alertas de Sentry.") }
        Repeater { model:nc.visibleRecords
            delegate:ListRow { glyph:"activity"; title:modelData.bridge + " · " + modelData.kind
                sub:modelData.workspace + " · " + modelData.id
                Chip { small:true; text:String(modelData.state); tone:modelData.state==="failed" ? "bad" : "neutral" }
                Btn { text:Tr.t("Descartar"); onClicked:nc.dismiss(modelData) }
            }
        }
        NoteBox { text:Tr.t("El historial de autorizaciones y alertas necesita su propia proyección. Un job terminado no acredita un permiso actual.") }
    }
    }
}
