import QtQuick
import QtTest

Item {
    width:1920; height:1080
    Main { id:desktop; anchors.fill:parent }
    Component {
        id:worldStub
        Item {
            property string scene:"world"
            property bool embedded:true
            function uiState() { return JSON.stringify({scene:"world",source:"fixture",stream:{state:"fixture"}}) }
            function viewVerb(action,args) { return "fixture view rejects " + action }
        }
    }
    TestCase {
        name:"WorldDesktop"
        when:windowShown
        function init() { Wm.closeAll(); Demo.bare=true }
        function cleanup() { if (typeof desktop.activeSpace !== "undefined") desktop.activeSpace="orb" }
        function prepareWorld() {
            verify(typeof desktop.worldAvailable !== "undefined","World must use the desktop host, not a detached three-tab window")
            desktop.worldAvailable=true
            desktop.worldViewComponent=worldStub
            compare(desktop.run("app.world"),true)
            compare(desktop.activeSpace,"world")
            tryVerify(function() { return desktop.worldItem !== null })
        }
        function test_world_keeps_full_bottom_dock() {
            prepareWorld()
            var dock=findChild(desktop,"dock")
            verify(dock!==null)
            verify(dock.y>=desktop.height-120,"desktop navigation stays at the bottom")
            for(var id of ["orb","world","vault","forge","sentry","stage","set"])
                verify(findChild(desktop,"dock."+id)!==null,"missing dock icon: "+id)
            compare(dock.current,"world")
        }
        function test_world_forge_and_orb_share_navigation() {
            prepareWorld()
            compare(JSON.parse(desktop.uiState()).scene,"world")
            compare(desktop.run("app.forge"),true)
            verify(Wm.isOpen("forge"))
            compare(desktop.run("app.orb"),true)
            compare(desktop.activeSpace,"orb")
        }
        function test_unavailable_world_is_not_reported_open() {
            verify(typeof desktop.worldAvailable !== "undefined")
            desktop.worldAvailable=false
            compare(desktop.run("app.world"),false)
            verify(desktop.actionNote.length>0)
        }
    }
}
