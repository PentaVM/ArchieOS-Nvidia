import QtQuick
import "Lang.js" as Tr

// Forge: the applications on this machine, opened in this session through the native app
// catalog (the same one the Observatory's Forge uses). Opening an application is a launch in the person's own session; Forge never runs a
// tool, approves an operation or claims a permission.
AppWindow {
    id: fg
    title:"Forge"
    property var bridge:null
    property string st:Demo.state
    readonly property int section:0
    property string note:""
    readonly property bool hasCatalog: typeof appCatalog !== "undefined" && appCatalog !== null
    readonly property var allApps: hasCatalog ? appCatalog.forgeApps : []
    readonly property var apps: allApps.filter(function(a) { return a.section !== "suggested" })
    readonly property var suggested: allApps.filter(function(a) { return a.section === "suggested" })
    // The catalog counts running windows only while someone looks at it.
    Binding { when: fg.hasCatalog; target: fg.hasCatalog ? appCatalog : null; property: "active"; value: fg.shown && fg.section === 0 }
    Connections {
        target: fg.hasCatalog ? appCatalog : null
        function onLaunched(id, ok, message) {
            var app = fg.allApps.filter(function(a) { return a.id === id })[0]
            fg.note = (app ? app.name : id) + (ok ? Tr.t(": abierta") : ": " + message)
            noteTimer.restart()
        }
    }
    Timer { id: noteTimer; interval: 4000; onTriggered: fg.note = "" }
    function launch(id) { return fg.hasCatalog && appCatalog.launch(id) }

    trailing:[Chip { text:Tr.t("Este equipo"); tone:"neutral" }]
    sidebar:[Eyebrow { text:Tr.t("Aplicaciones") }]

    // ---- applications ----
    Column { visible:fg.section===0; width:parent.width; spacing:12
        Eyebrow { text:Tr.t("Aplicaciones de este equipo") }
        NoteBox { visible:!fg.hasCatalog; icon:"unknown"; text:Tr.t("Este host no tiene el catálogo nativo de aplicaciones.") }
        NoteBox { visible:fg.hasCatalog && !appCatalog.canLaunch; icon:"unknown"; text:fg.hasCatalog ? appCatalog.launchNote : "" }
        NoteBox { visible:fg.note!==""; icon:"done"; text:fg.note }
        Flow { width:parent.width; spacing:10
            Repeater { model:fg.apps
                delegate:Rectangle { objectName:"forge.app." + modelData.id; width:112; height:118; radius:12
                    readonly property bool ready: !!modelData.available && fg.hasCatalog && appCatalog.canLaunch
                    color:ama.containsMouse && ready ? Theme.hover : "transparent"
                    Rectangle { id:tile; anchors.horizontalCenter:parent.horizontalCenter; y:8; width:60; height:60; radius:16
                        color: Theme.iconSurface; border.width:1; border.color:Theme.hair
                        Image { id:img; anchors.centerIn:parent; width:40; height:40; source:modelData.icon || ""; sourceSize:Qt.size(80,80)
                            fillMode:Image.PreserveAspectFit; visible:status===Image.Ready; asynchronous:true }
                        Text { anchors.centerIn:parent; visible:img.status!==Image.Ready; text:modelData.glyph || "·"; color:Theme.iconInk; font.pixelSize:24; font.family:Theme.font } }
                    Rectangle { visible:modelData.running > 0; anchors.horizontalCenter:parent.horizontalCenter; anchors.top:tile.bottom; anchors.topMargin:4
                        width:5; height:5; radius:2.5; color:Theme.ok }
                    Text { anchors.horizontalCenter:parent.horizontalCenter; y:tile.y + tile.height + 12; width:parent.width - 8
                        horizontalAlignment:Text.AlignHCenter; text:modelData.name || ""; elide:Text.ElideRight; maximumLineCount:2; wrapMode:Text.Wrap
                        color:modelData.available ? Theme.t1 : Theme.t3; font.family:Theme.font; font.pixelSize:12 }
                    Accessible.role: Accessible.Button; Accessible.name: modelData.name || ""
                    MouseArea { id:ama; anchors.fill:parent; hoverEnabled:true; enabled:parent.ready; onClicked:fg.launch(modelData.id) } } } }
        Eyebrow { visible:fg.suggested.length>0; text:Tr.t("Para instalar") }
        Repeater { model:fg.suggested
            delegate:ListRow { glyph:"plus"; iconSource:modelData.icon || ""; title:modelData.name; sub:(modelData.comment || "") + " · " + (modelData.missing || "")
                Chip { small:true; text:Tr.t("no instalada"); tone:"unk" } } }
    }

}
