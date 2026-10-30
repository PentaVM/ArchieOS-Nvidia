import QtQuick
import QtQuick.Window

// Host-owned native window. One viewport, one read-only bridge, no extra OS authority.
Window {
    id:window
    width:1920; height:1080; visible:true
    title:"ArchieOS"
    color:Theme.desktop
    readonly property string bridgeLink:desktop.bridge.link
    readonly property string orbDiagnostics:desktop.orbDiagnostics
    Component.onCompleted: {
        if (typeof shellColorMode !== "undefined") Theme.colorMode=shellColorMode
        // As the stage's desktop the shell starts bare: the orb, the bar and the dock.
        if (typeof insideStage !== "undefined" && insideStage) Wm.closeAll()
    }
    Main { id:desktop; objectName:"desktop"; anchors.fill:parent }
    // The boot animation covers the desktop while it comes up (`--no-boot-splash` skips it).
    Loader {
        id:boot
        anchors.fill:parent; z:10000
        active:typeof bootSplash !== "undefined" && bootSplash
        sourceComponent:BootSplash { objectName:"bootSplash"; onFinished:boot.active=false }
    }
    function showScene(value) { return desktop.run(value === "world" ? "app.world" : value === "forge" ? "app.forge" : "app.orb") }
    function viewVerb(action,args) {
        if (action === "window") {
            visibility=args.state === "fullscreen" ? Window.FullScreen : args.state === "maximized" ? Window.Maximized : Window.Windowed
            return ""
        }
        return desktop.viewVerb(action,args)
    }
    // The profile goes through LiveOrb, which applies the preset and then the look chosen in
    // Settings (grade, brightness, nodes...). Applying it to lumen directly here as well
    // reset those settings to the preset's defaults after the look had been applied.
    function applyVisualProfile(value, alreadyApplied) {
        if (typeof lumen !== "undefined") Demo.nativeProfiles=lumen.presets
        var index=Demo.orbProfiles.indexOf(value)
        if (index<0) return false
        Demo.orbAcceptedProfile=alreadyApplied ? value : ""
        if (Demo.orbProfile===index) Demo.orbProfileChanged()
        else Demo.orbProfile=index
        Demo.orbShow=true; Demo.bare=true
        return true
    }
    Connections {
        target:typeof displayControl === "undefined" ? null : displayControl
        function onCommandReceived(action,value,enabled) {
            if (action==="scene") {
                if (value==="orb") desktop.run("app.orb")
                else if (value==="forge") desktop.run("app.forge")
                else if (value==="world") window.showScene(value)
            } else if (action==="cinematic" || action==="preset") {
                window.applyVisualProfile(value,true)
            } else if (action==="overlay" && typeof lumen !== "undefined") {
                if (value==="hud") Demo.orbHud=enabled
                else lumen.setSetting(value,enabled)
            } else if (action==="panel") {
                desktop.run("overlay." + (value==="notifications" ? "notif" : value))
            }
        }
    }
    onClosing: if (typeof shellRuntime !== "undefined") shellRuntime.running=false
}
