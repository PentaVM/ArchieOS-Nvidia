import QtQuick
import QtQuick.Controls
import "qrc:/archie/shell/prototype" as Shell

// Explicit Observatory/QA host. The normal entry point uses the complete desktop.
ApplicationWindow {
    id: window
    visible: true
    visibility: Window.FullScreen
    color: Shell.Theme.desktop
    title: "Archie Observatory"
    property alias scene: content.scene
    property alias worldHost: content.worldHost
    function uiState() { return content.uiState() }
    function viewVerb(action,args) { return content.viewVerb(action,args) }
    onClosing: function(event) { if(worldHost) { event.accepted=false; hide() } }
    ObservatoryContent { id:content; anchors.fill:parent; onExitRequested:window.hide() }
}
