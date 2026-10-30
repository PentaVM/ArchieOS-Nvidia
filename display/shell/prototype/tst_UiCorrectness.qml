import QtQuick
import QtTest
import "."

Item {
    width: 1920; height: 1080
    Component { id: modelComponent; BridgeModel {} }
    Component { id: vaultComponent; VaultWindow { width: 1060; height: 720; tabsModel:[{url:String(Qt.resolvedUrl(".")).replace(/\/$/, "")}] } }
    Component { id: searchComponent; SearchPanel {} }
    Component { id: workspaceComponent; WorkspaceMenu {} }
    Component { id: panelComponent; Panel { width: 100; height: 100 } }
    Component { id: switcherComponent; WindowSwitcher { st: "degraded" } }
    TestCase {
        name: "ShellCorrectness"
        when: windowShown
        function init() { failOnWarning(/.*/) }
        function frame(extra) {
            var f = { schema: 1, at_ms: 1000, link: { state: "live" }, system: null, kernel: null }
            for (var k in extra) f[k] = extra[k]
            return JSON.stringify(f)
        }
        function make(c) { return createTemporaryObject(c, parent) }
        function input(item) {
            if (item.cursorPosition !== undefined && item.text !== undefined) return item
            for (var i = 0; i < item.children.length; i++) {
                var found = input(item.children[i]); if (found) return found
            }
            return null
        }
        function visibleText(item) {
            if (!item.visible) return ""
            var out = typeof item.text === "string" ? item.text : ""
            for (var i=0;i<item.children.length;i++) out += " " + visibleText(item.children[i])
            return out
        }
        function test_unknown_link_is_rejected_atomically() {
            var m = make(modelComponent); m.nowMs = 1000
            verify(m.ingest(frame({})))
            verify(!m.ingest(frame({ link: { state: "trusted" } })))
            compare(m.link, "live"); compare(m.receivedAtMs, 1000)
        }
        function test_real_backend_offline_frame_is_accepted_without_invented_sources() {
            // Emitted by the real Rust bridge with no server on 127.0.0.1:1.
            // The test runner needs QML_XHR_ALLOW_FILE_READ=1; no service is started here.
            var xhr = new XMLHttpRequest()
            xhr.open("GET", Qt.resolvedUrl("../../../core/crates/archie-shell-bridge/tests/fixtures/backend-offline.jsonl"), false)
            xhr.send()
            verify(xhr.responseText.length > 0)
            var m = make(modelComponent)
            verify(m.ingest(xhr.responseText.trim()), m.lastRejection)
            compare(m.link,"displayd_offline"); compare(m.system,null); compare(m.kernel,null)
            compare(m.catalog.state,"unavailable"); compare(m.inspector.source,"unavailable"); compare(m.workflows.source,"unavailable")
            compare(m.runningJobs,0)
        }
        function test_partial_jobs_cannot_replace_good_frame() {
            var m = make(modelComponent)
            verify(m.ingest(frame({})))
            verify(!m.ingest(frame({ jobs: { state: "ok" } })))
            compare(m.jobs.state, "not_configured"); compare(m.runningJobs, 0)
        }
        function test_local_jobs_survive_displayd_offline_but_not_bridge_silence() {
            var m = make(modelComponent); m.nowMs = 1000
            verify(m.ingest(frame({link: {state:"displayd_offline",reason:"refused"}, jobs: {
                state:"ok", jobs:[{workspace:"w",id:"j",bridge:"freecad",kind:"sweep",state:"running",
                done:1,total:2,started_ms:0,updated_ms:1000,idle_ms:0,error:null}], skipped:0,unreadable:[]}})))
            compare(m.runningJobs, 1)
            m.nowMs = 7001
            compare(m.link, "bridge_silent"); compare(m.runningJobs, 0)
            verify(m.jobs.state !== "ok")
        }
        // This folder, as qmltestrunner reads it: real files the test can name.
        function here() { return String(Qt.resolvedUrl(".")).replace(/\/$/, "") }
        function test_vault_lists_a_real_folder_and_offers_no_mutation() {
            var v = make(vaultComponent)
            v.setFolder(here())
            var names = []
            tryVerify(function() {
                names = []
                for (var i = 0; i < v.count; i++) names.push(v.entry(i).name)
                return names.indexOf("VaultWindow.qml") >= 0 }, 5000, "this folder's own files are listed")
            verify(!v.fileOperationsAvailable); verify(!v.canUndo)
            compare(typeof v.cancelCopy, "undefined"); compare(typeof v.undoCopy, "undefined")
            var text = visibleText(v)
            verify(text.indexOf("Cancelar") < 0 && text.indexOf("Deshacer") < 0)
        }
        function test_vault_navigates_up_into_and_back_without_inventing_paths() {
            var v = make(vaultComponent)
            var dir = here()
            v.setFolder(dir)
            tryVerify(function() { return v.ready && v.count > 0 }, 5000)
            var leaf = dir.split("/").pop()
            compare(v.crumbs[v.crumbs.length - 1].label, decodeURIComponent(leaf))
            verify(v.up())
            tryVerify(function() { return v.folder === dir.slice(0, dir.lastIndexOf("/")) }, 3000)
            // The listing changes after the folder does: wait for the entry, not for a status.
            var at = -1
            tryVerify(function() {
                at = -1
                for (var i = 0; i < v.count; i++) if (v.entry(i).name === leaf) at = i
                return at >= 0 }, 5000, "the folder just left is listed in its parent")
            verify(v.openItem(at))
            wait(50) // Allow the queued folder reset before another navigation.
            compare(v.folder.replace(/^file:\/\/\/[A-Z]:/, function(v){return v.toLowerCase()}), dir.replace(/^file:\/\/\/[A-Z]:/, function(v){return v.toLowerCase()}))
            tryVerify(function() { return v.ready }, 5000)
            // Windows has drive roots; file:/// is the Unix system root.
            if (Qt.platform.os !== "windows") { v.setFolder("file:///"); compare(v.up(), false); wait(50) }
            tryVerify(function() { return v.ready }, 5000)
            v.newTab(); compare(v.tabsModel.length, 2); compare(v.selected, -1)
            // FolderListModel queues its reset; let it start before testing Ready/teardown.
            wait(50)
            tryVerify(function() { return v.ready }, 5000)
        }
        function test_hidden_search_capability_cannot_be_activated_by_enter() {
            var s = make(searchComponent); var count = 0
            s.activated.connect(function() { count++ })
            s.st = "offline"; s.query = "Leer espacios"
            var field = input(s); verify(field !== null); field.forceActiveFocus()
            keyClick(Qt.Key_Return)
            compare(count, 0)
        }
        function test_error_search_cannot_activate_any_result() {
            var s = make(searchComponent); var count = 0
            s.activated.connect(function() { count++ })
            s.st = "error"; s.query = ""
            var field = input(s); field.forceActiveFocus(); keyClick(Qt.Key_Return)
            compare(count, 0)
        }
        function test_workspace_without_contract_does_not_switch_domain() {
            var w = make(workspaceComponent); var count = 0
            w.chosen.connect(function() { count++ })
            mouseClick(w, 120, 95)
            compare(count, 0)
        }
        function test_closing_panel_stops_input_immediately() {
            var p = make(panelComponent)
            p.shown = false
            compare(p.enabled, false)
        }
        function test_unknown_window_colour_remains_valid() {
            var w = make(switcherComponent)
            verify(w !== null)
            // init() makes binding warnings failures, including the unknown app QColor.
        }
        function test_nested_payloads_are_rejected_without_refreshing_good_frame() {
            var m = make(modelComponent); m.nowMs = 1000; verify(m.ingest(frame({})))
            var cases = [
                {system:{hostname:"invented"}},
                {kernel:{schema_version:1,read_at:1,display_budget:"balanced",pressure_availability:{state:"available"},thermal_availability:{state:"unavailable",reason:"none"}}},
                {lan:{state:"ok",scanned_unix:1,total:1,age_seconds:0,stale:false,devices:[{name:"partial"}]}},
                {jobs:{state:"ok",skipped:0,unreadable:[],jobs:[{id:"j",state:"invented"}]}},
                {catalog:{source:"live",daemon_reachable:true,modules:[],models:[]}},
                {inspector:{source:"unavailable",state:"ready",reason:null,generated_at_ms:1,providers:[],cortex_trajectories:[]}},
                {workflows:{source:"live",state:"ready",reason:null,generated_at_ms:1,workflows:[{component:"voice",phase:"partial"}]}}
            ]
            m.nowMs = 2000
            for (var i = 0; i < cases.length; i++) { verify(!m.ingest(frame(cases[i])), "partial case " + i); compare(m.receivedAtMs,1000); compare(m.link,"live") }
        }
        function test_additive_projections_keep_fixture_provenance_and_become_stale() {
            var m = make(modelComponent); m.nowMs = 1000
            compare(m.catalog.state,"unavailable"); compare(m.inspector.providers.length,0); compare(m.workflows.workflows.length,0)
            verify(m.ingest(frame({link:{state:"fixture"},
                catalog:{source:"fixture",generated_at:1000,daemon_reachable:true,staleness_ms:0,error:null,modules:[],models:[],nodes:[],capabilities:{}},
                inspector:{source:"fixture",state:"ready",reason:null,generated_at_ms:1000,providers:[{id:"p",title:"Provider",capability:"c",status:"disabled",records:[],known_gaps:[]}],cortex_trajectories:[]},
                workflows:{source:"fixture",state:"ready",reason:null,generated_at_ms:1000,workflows:[{component:"wanda",phase:"observe",active:true,observed:true,count:1,age_ms:0}]}})))
            compare(m.catalog.state,"ready"); compare(m.inspector.providers[0].id,"p"); compare(m.workflows.workflows[0].phase,"observe")
            m.nowMs = 7001
            compare(m.inspector.state,"stale"); compare(m.workflows.state,"stale"); compare(m.catalog.source,"fixture")
        }
        function test_fractional_job_progress_is_not_accepted_as_typed_counter() {
            var m = make(modelComponent)
            verify(!m.ingest(frame({jobs:{state:"ok",skipped:0,unreadable:[],jobs:[{workspace:"w",id:"j",bridge:"b",kind:"k",state:"running",done:0.5,total:2,started_ms:0,updated_ms:1000,idle_ms:0,error:null}]}})))
        }
        function test_unobserved_workflow_has_no_invented_zero_age() {
            var m = make(modelComponent)
            verify(m.ingest(frame({workflows:{source:"live",state:"stale",reason:"cached",generated_at_ms:1000,
                workflows:[{component:"iris",phase:"waiting",active:false,observed:false,count:0,age_ms:null}]}})))
            compare(m.workflows.workflows[0].age_ms,null)
            compare(m.workflows.workflows[0].observed,false)
            compare(m.workflows.state,"stale")
        }
        function test_unknown_workflow_phase_is_rejected() {
            var m = make(modelComponent)
            var lane = {component:"iris",phase:"waiting",active:false,observed:false,count:0,age_ms:null}
            verify(!m.ingest(frame({workflows:{source:"live",state:"ready",reason:null,generated_at_ms:1,workflows:[Object.assign({},lane,{phase:"execute_unleased"})]}})))
        }
        function test_oversized_workflow_projection_is_rejected() {
            var m = make(modelComponent)
            var lane = {component:"iris",phase:"waiting",active:false,observed:false,count:0,age_ms:null}
            verify(!m.ingest(frame({workflows:{source:"live",state:"ready",reason:null,generated_at_ms:1,workflows:[lane,lane,lane,lane,lane]}})))
        }
        function test_closed_search_does_not_activate_cached_result() {
            var s = make(searchComponent); var count = 0
            s.activated.connect(function(){count++})
            s.shown = false
            verify(!s.activateIndex(0)); compare(count,0)
        }
        function test_keyboard_and_mouse_share_search_eligibility() {
            var s = make(searchComponent); var actions = []
            s.activated.connect(function(a){actions.push(a)})
            s.query = "actividad"; compare(s.eligibleRows.length,1)
            var field = input(s); field.forceActiveFocus(); keyClick(Qt.Key_Return)
            mouseClick(s,100,110); compare(actions.length,2); compare(actions[0],"overlay.activity"); compare(actions[1],"overlay.activity")
        }
        function test_vault_places_are_the_menu_and_palette_places() {
            var v = make(vaultComponent)
            compare(v.places.length, 6)
            compare(v.places[0].name, "Inicio"); compare(v.places[5].url, "file:///")
            verify(v.goPlace(5)); compare(v.folder, "file:///"); compare(v.place, 5)
            verify(!v.goPlace(6))
        }
    }
}
