import QtQuick
import QtTest

Item {
    id: host
    width: 1920; height: 1080
    QtObject {
        id: wire
        property string link: "fixture"
        property bool silent: false
        property real ageMs: 0
        property var catalog: ({source:"fixture", daemon_reachable:true, staleness_ms:0,
            models:[{id:"local-model",privacy:"local_only",locality:"local"}],
            modules:[{id:"forge",state:"unimplemented",explanation:"No worker"}]})
        property var inspector: ({source:"fixture",state:"ready",providers:[
            {id:"provider-one",title:"One",capability:"vision",status:"available"}],
            cortex_trajectories:[{trajectory_id:"record-one",model_id:"local-model",
                correction_kind:"user_correction",selected_context_count:2,omitted_candidate_count:3}]})
        property var workflows: ({source:"fixture",state:"ready",workflows:[
            {component:"iris",phase:"observe",observed:true,active:true,count:4,age_ms:10}]})
        property var jobs: ({state:"ok",jobs:[
            {id:"job-one",workspace:"work-one",bridge:"cad",kind:"export",state:"done",
                done:2,total:2,updated_ms:1,error:null}]})
    }
    Component { id: inspector; InspectorPanel { shown: true } }
    Component { id: settings; SettingsWindow { shown: true; width:940; height:620 } }
    Component { id: forge; ForgeWindow { shown:true; width:1000; height:600 } }
    Component { id: notices; NotificationCenter { shown:true } }
    TestCase {
        name: "BackendPanels"
        when: windowShown
        function content(o) {
            var text=[]
            function walk(i) {
                if (!i || !i.visible) return
                if (typeof i.text === "string") text.push(i.text)
                if (typeof i.title === "string") text.push(i.title)
                if (typeof i.sub === "string") text.push(i.sub)
                for (var n=0;n<i.children.length;n++) walk(i.children[n])
            }
            walk(o); return text.join(" | ")
        }
        function test_no_fabricated_cortex_history() {
            var o=inspector.createObject(host)
            verify(o!==null); wait(250)
            verify(content(o).indexOf("tray-0412")<0,"no invented Cortex trajectory")
            o.destroy()
        }
        function test_inspector_uses_minimized_backend() {
            var o=inspector.createObject(host)
            verify("bridge" in o,"inspector accepts backend bridge")
            o.bridge=wire
            compare(o.trajectories.length,1)
            compare(o.trajectories[0].trajectory_id,"record-one")
            compare(o.observations.length,1)
            verify(content(o).indexOf("Grabación")>=0,"fixture is visibly labelled")
            o.destroy()
        }
        function test_settings_has_real_catalogue() {
            var o=settings.createObject(host)
            verify("bridge" in o,"settings accepts backend bridge")
            o.bridge=wire
            compare(o.providers.length,1)
            compare(o.models.length,1)
            o.destroy()
        }
        function test_forge_derives_jobs_not_fake_services() {
            var o=forge.createObject(host)
            verify("bridge" in o,"Forge accepts backend bridge")
            o.bridge=wire
            verify(content(o).indexOf("Manifiesto de Forge")<0,"Forge only contains application launching")
            verify(content(o).indexOf("freecad-bridge")<0,"no invented live bridge")
            o.destroy()
        }
        function test_forge_has_no_developer_jobs_tab() {
            var o=forge.createObject(host)
            verify(findChild(o,"forge.section.jobs")===null,"Forge is the application launcher; diagnostics belong in Activity")
            o.destroy()
        }
        function test_notifications_only_observed_jobs() {
            var o=notices.createObject(host)
            verify("bridge" in o,"notifications accept backend bridge")
            o.bridge=wire
            compare(o.records.length,1)
            verify(content(o).indexOf("Permiso concedido")<0,"no fabricated Policy event")
            o.destroy()
        }
    }
}
