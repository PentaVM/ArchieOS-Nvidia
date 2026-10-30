import QtQuick
import QtTest
import "../../qml" as Native

Item {
    width: 640; height: 480
    Native.ModelActivityHud { id: hud; width: 300; models: [] }
    TestCase {
        name: "ModelActivityHud"; when: windowShown
        function cleanup() { hud.models = [] }
        function test_only_fresh_active_models() {
            hud.models = [
                {id:"ready",label:"Registered only",active:false,stale:false},
                {id:"old",label:"Old",active:true,stale:true,tps_available:true,tps:99},
                {id:"live",label:"Measured model",active:true,stale:false,tps_available:true,tps:12.34}
            ]
            compare(hud.activeModels.length, 1)
            compare(hud.activeModels[0].id, "live")
            tryCompare(findChild(hud,"modelActivity.live"), "text", "Measured model")
            compare(findChild(hud,"modelRate.live").text,"12.3 tok/s")
        }
        function test_absent_and_invalid_rates_are_not_zero() {
            hud.models = [{id:"new",active:true,stale:false,tps_available:false,tps:0}]
            tryCompare(findChild(hud,"modelActivity.new"), "text", "new")
            compare(findChild(hud,"modelRate.new").text,"—")
            compare(hud.rate({tps_available:true,tps:NaN}), "—")
            compare(hud.rate({tps_available:true,tps:-1}), "—")
            compare(hud.rate({tps_available:true,tps:0}), "0.0 tok/s")
        }
        function test_empty_source_stays_empty() {
            compare(hud.activeModels.length, 0)
            compare(hud.implicitHeight, 0)
        }
        function test_missing_source_has_one_compact_indicator() {
            hud.models = null
            compare(hud.activeModels.length, 0)
            verify(findChild(hud,"modelActivity.unavailable").visible)
        }
        function test_recent_observation_is_labeled_as_recent() {
            hud.models = [{id:"recent",active:true,stale:false,state:"reciente",tps_available:true,tps:1.5}]
            tryCompare(findChild(hud,"modelActivity.recent"), "text", "recent · reciente")
            compare(findChild(hud,"modelRate.recent").text,"1.5 tok/s")
        }
        function test_long_model_name_cannot_hide_its_rate() {
            hud.models=[{id:"long",label:"A very long model identifier ".repeat(8),active:true,stale:false,tps_available:true,tps:12.3}]
            const rate=findChild(hud,"modelRate.long")
            verify(rate!==null,"A measured rate has its own reserved visible column")
            compare(rate.text,"12.3 tok/s")
            verify(rate.x+rate.width<=hud.width)
        }
    }
}
