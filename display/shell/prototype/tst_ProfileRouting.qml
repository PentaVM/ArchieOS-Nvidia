import QtQuick
import QtTest

Item {
    Component { id:shell; ShellWindow { visible:false } }
    Component { id:live; LiveOrb { visible:false; nativeBridge:null } }
    Component { id:controller; QtObject {
        property var presets:["field","polar-prisma","polar-holograma"]
        property string profile:"field"
        property int calls:0
        property var settings:({exposure:1.32,pulseSpeed:.55,accentWarm:"#e8b47d",accentCool:"#78c2ca"})
        function applyProfile(name) {
            calls++;profile=name
            settings={exposure:1.32,pulseSpeed:.55,accentWarm:"#e8b47d",accentCool:"#78c2ca"}
            return true
        }
        function setSetting(key,value) {var s=Object.assign({},settings);s[key]=value;settings=s}
    } }
    TestCase {
        name:"ProfileRouting"; when:windowShown
        function init() { Demo.orbAcceptedProfile="";Demo.nativeProfiles=[];Demo.orbProfile=0 }
        function cleanup() { Demo.orbAcceptedProfile="";Demo.nativeProfiles=[];Demo.orbProfile=0 }
        function test_source_values_survive_until_explicit_user_change() {
            var fakeController=createTemporaryObject(controller,null)
            var view=createTemporaryObject(live,null,{nativeController:fakeController})
            verify(view!==null)
            Demo.orbProfile=Demo.orbProfiles.indexOf("polar-prisma")
            compare(fakeController.profile,"polar-prisma")
            compare(fakeController.settings.exposure,1.32)
            compare(fakeController.settings.pulseSpeed,.55)
            compare(fakeController.settings.accentWarm,"#e8b47d")
            view.applyLook("brightness")
            compare(fakeController.settings.exposure,[1.6,2.2,2.5][Demo.orbBrightI])
            compare(fakeController.settings.pulseSpeed,.55)
        }
        function test_acknowledged_profile_is_not_applied_twice() {
            var fakeController=createTemporaryObject(controller,null)
            var view=createTemporaryObject(live,null,{nativeController:fakeController})
            Demo.orbProfile=Demo.orbProfiles.indexOf("polar-prisma")
            fakeController.applyProfile("polar-prisma")
            const before=fakeController.calls
            Demo.orbAcceptedProfile="polar-prisma"
            Demo.orbProfileChanged()
            compare(fakeController.calls,before)
            compare(fakeController.settings.exposure,1.32)
        }
        function test_ipc_preset_keeps_the_next_user_selection_effective() {
            var window=createTemporaryObject(shell,null)
            verify(window!==null)
            Demo.orbProfile=0
            verify(typeof window.applyVisualProfile==="function","IPC and settings share profile state")
            verify(window.applyVisualProfile("neural"))
            compare(Demo.orbProfile,Demo.orbProfiles.indexOf("neural"))
            Demo.orbProfile=0
            compare(Demo.orbProfile,0)
            verify(!window.applyVisualProfile("execute"))
            compare(Demo.orbProfile,0)
        }
    }
}
