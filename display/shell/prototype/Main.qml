import QtQuick
import "ActionRules.js" as ActionRules
import "Lang.js" as Tr

// Native desktop surfaces managed locally by Wm. Core, Policy and the external
// compositor keep their own authority; window state never grants a capability.
Item {
    id: root
    width: 1920; height: 1080
    property string overlay: ""
    property string menuName: ""
    property string activeSpace: "orb"
    property bool worldAvailable: typeof displayControl !== "undefined" && displayControl.scenes.indexOf("world") >= 0
    // Test hosts may inject a view; production loads the shared native content.
    property Component worldViewComponent: null
    readonly property var worldItem: worldLoader.item
    property string presentationNote:""
    property point presentationPointer:Qt.point(0,0)
    function viewVerb(action,args) {
        if (activeSpace === "world" && worldItem) return worldItem.viewVerb(action,args)
        if (action === "popup") { presentationNote=args.text; presentationTimer.interval=Math.round(args.seconds*1000); presentationTimer.restart(); return "" }
        if (action === "pointer") { presentationPointer=Qt.point(args.x,args.y); pointerTimer.restart() }
        return live.viewVerb(action,args)
    }
    focus: true

    property alias bridge: bridgeModel
    readonly property string bridgeLink: bridgeModel.link
    readonly property int bridgeRejected: bridgeModel.rejected
    readonly property bool opaque: Theme.opaque
    readonly property var wmState: Wm.st
    readonly property string wmFront: Wm.front()
    readonly property var wmOffsets: Wm.off
    readonly property bool vaultOpen: Wm.isOpen("vault")

    BridgeModel { id: bridgeModel }
    property string actionNote: ""
    signal actionDenied(string action, string reason)
    readonly property bool nativeHost: typeof shellRuntime !== "undefined" && shellRuntime !== null
    readonly property string hostError: nativeHost ? shellRuntime.error : ""
    Connections {
        target: root.nativeHost ? shellRuntime : null
        function onFrameReady(line) { bridgeModel.ingest(line) }
    }
    Binding { target: Theme; property: "runtimeConserve"; value: root.bridge.kernel !== null && ["conserve","headless"].indexOf(root.bridge.kernel.display_budget) >= 0 }
    Timer { interval: 1000; repeat: true; running: root.visible; onTriggered: bridgeModel.tick() }
    onOverlayChanged: if (overlay !== "palette" && overlay !== "search") root.forceActiveFocus()

    // Everything a click can change that a person would see, as one JSON string. The UI walk
    // (archie-display --ui-walk) compares it before and after each click: a control that
    // changes none of it, and shows no note, does nothing.
    function uiState() {
        var hasRuntime = typeof shellRuntime !== "undefined" && shellRuntime !== null
        if (activeSpace === "world" && worldLoader.item) {
            var world = JSON.parse(worldLoader.item.uiState())
            world.overlay=overlay; world.front=Wm.front(); world.wm=Wm.st
            return JSON.stringify(world)
        }
        return JSON.stringify({
            overlay: overlay, menu: menuName, note: actionNote, front: Wm.front(), wm: Wm.st,
            bare: Demo.bare, orbNav: Demo.orbNav, orbShow: Demo.orbShow, orbProfile: Demo.orbProfile,
            orbLook: [Demo.orbNodesI, Demo.orbGradeI, Demo.orbDust, Demo.orbBrightI, Demo.orbActI, Demo.orbSpin],
            silent: Demo.silent,
            theme: [Theme.reducedMotion, Theme.opaque, Theme.motion, Theme.colorMode, Theme.vivid, Theme.blur],
            vault: { folder: vault.folder, tab: vault.tabIndex, tabs: vault.tabsModel.length, selected: vault.selected,
                     grid: vault.gridView, sort: vault.sortMode, note: vault.note },
            forge: { section: forgeWin.section, note: forgeWin.note }, settings: setWin.section,
            dockMenu: dockMenu.target, ai: hasRuntime ? [shellRuntime.aiPolicy, shellRuntime.aiPolicyBusy] : null
        })
    }
    function handleEscape() { overlay = ""; Demo.orbNav = false; root.forceActiveFocus() }
    function toggle(name) { overlay = overlay === name ? "" : name }
    function ingestRecording(line) {
        var frame
        try { frame = JSON.parse(line) } catch (e) { return bridgeModel.ingest(line) }
        if (!frame || typeof frame !== "object") return bridgeModel.ingest(line)
        frame.link = { state: "fixture" }
        if (frame.system) frame.system.source = "fixture"
        for (var field of ["catalog","inspector","workflows"])
            if (frame[field] && typeof frame[field] === "object") frame[field].source = "fixture"
        return bridgeModel.ingest(JSON.stringify(frame))
    }
    function actionContext() {
        return { nativeReady: live.ok,
            worldReady: worldAvailable,
            stageLauncher: typeof appCatalog !== "undefined" && appCatalog.canLaunch,
            compositorBlur: false }
    }
    // Explicit development replay: recordings cannot present themselves as live.
    property string replayDebug: ""
    Component.onCompleted: {
        replayDebug = typeof replayFile === "undefined" ? "no-context-property" : "file=" + replayFile
        if (typeof replayFile === "undefined" || replayFile === "") return
        var xhr = new XMLHttpRequest()
        xhr.open("GET", replayFile, false); xhr.send()
        var lines = xhr.responseText.split(/\r?\n/)
        replayDebug += " status=" + xhr.status + " bytes=" + xhr.responseText.length + " lines=" + lines.length
        for (var i = 0; i < lines.length; i++) if (lines[i].trim() !== "") root.ingestRecording(lines[i])
    }

    function run(id) {
        if (typeof id !== "string" || !ActionRules.allowed(id, actionContext())) {
            actionNote = typeof id === "string" ? ActionRules.reason(id, actionContext()) : Tr.t("Acción no válida")
            actionDenied(String(id), actionNote)
            return false
        }
        actionNote = ""
        if (id.indexOf("dock.") === 0) { overlay = ""; Wm.dockClick(id.slice(5)); return true }
        // Vault's places, read only: "go:<index>" opens the window on that folder.
        if (id.indexOf("go:") === 0) { overlay = ""; Wm.open("vault"); return vault.goPlace(Number(id.slice(3))) }
        if (id.indexOf("window.") === 0) { overlay = ""; Wm.open(id.slice(7)); return true }
        if (id.indexOf("overlay.") === 0) {
            var name = id.slice(8)
            var windows = { forge: "forge", settings: "set", sentry: "sentry" }
            if (windows[name]) { overlay = ""; Wm.open(windows[name]) }
            else overlay = name
            return true
        }
        switch (id) {
        case "app.world":
            overlay=""; activeSpace="world"; Demo.orbNav=false; Demo.bare=true
            if (!worldViewComponent) worldLoader.setSource("qrc:/qt/qml/Archie/Observatory/qml/ObservatoryContent.qml", {scene:"world",embedded:true})
            if (worldLoader.status === Loader.Error) { activeSpace="orb"; actionNote=Tr.t("World no pudo cargar su vista nativa"); return false }
            return true
        case "app.orb": activeSpace="orb"; overlay = ""; Demo.bare = true; if (typeof archieBridge !== "undefined") archieBridge.requestMode("cerebra"); break
        case "app.vault": case "app.forge": case "app.sentry": case "app.set":
            overlay = ""; Wm.open(id.slice(4)); break
        case "app.stage": {
            var stage = appCatalog.apps.filter(function(a) { return a.available && a.id.indexOf("archie-stage") >= 0 })
            if (!stage.length) { actionNote = Tr.t("Stage no está instalado en el catálogo"); actionDenied(id, actionNote); return false }
            return appCatalog.launch(stage[0].id)
        }
        case "win.close": Wm.closeFront(); break
        case "win.min": Wm.minimizeFront(); break
        case "win.zoom": Wm.zoomFront(); break
        case "win.closeAll": Wm.closeAll(); break
        case "win.minAll": Wm.minimizeAll(); break
        case "win.showAll": overlay = ""; Wm.showAll(); break
        case "win.desktop": case "desk.toggle": overlay = ""; Demo.bare = !Demo.bare; break
        case "vault.newTab": overlay = ""; Wm.open("vault"); vault.newTab(); break
        case "vault.closeTab": vault.closeTab(vault.tabIndex); break
        case "vault.open": overlay = ""; Wm.open("vault"); return vault.openSelected()
        case "vault.up": overlay = ""; Wm.open("vault"); return vault.up()
        case "vault.sort": vault.toggleSort(); break
        case "vault.list": vault.gridView = false; break
        case "vault.grid": vault.gridView = true; break
        case "theme.motion": Theme.reducedMotion = !Theme.reducedMotion; break
        case "theme.opaque": Theme.opaque = !Theme.opaque; break
        case "theme.blur": Theme.blur = !Theme.blur; break
        case "orb.nav": overlay = ""; Demo.bare = true; Demo.orbNav = !Demo.orbNav; break
        }
        return true
    }

    Keys.onPressed: function(e) {
        if (e.key === Qt.Key_Escape) handleEscape()
        else if ((e.modifiers & Qt.ControlModifier) && e.key === Qt.Key_K) toggle("palette")
        else if ((e.modifiers & Qt.ControlModifier) && e.key === Qt.Key_Space) run("orb.nav")
        else if (overlay === "switcher" && e.key === Qt.Key_Tab) switcher.step(1)
        else if (overlay === "switcher" && e.key === Qt.Key_Backtab) switcher.step(-1)
        else if (overlay === "switcher" && (e.key === Qt.Key_Return || e.key === Qt.Key_Enter)) switcher.choose()
        else if ((e.modifiers & Qt.ControlModifier) && e.key === Qt.Key_T) run("vault.newTab")
        else if ((e.modifiers & Qt.ControlModifier) && e.key === Qt.Key_W) run((e.modifiers & Qt.ShiftModifier) ? "win.closeAll" : "win.close")
        else if ((e.modifiers & Qt.ControlModifier) && e.key === Qt.Key_M) run((e.modifiers & Qt.ShiftModifier) ? "win.minAll" : "win.min")
        else if (e.key === Qt.Key_C) toggle("control")
        else if (e.key === Qt.Key_S) Wm.toggle("sentry")
        else if (e.key === Qt.Key_A) toggle("activity")
        else if (e.key === Qt.Key_G && Wm.isOpen("vault")) vault.gridView = !vault.gridView
        else if (e.key === Qt.Key_D) run("win.desktop")
        else if (overlay === "" && Wm.front() === "vault" && e.key === Qt.Key_Down) vault.move(vault.gridView ? 4 : 1)
        else if (overlay === "" && Wm.front() === "vault" && e.key === Qt.Key_Up) vault.move(vault.gridView ? -4 : -1)
        else if (overlay === "" && Wm.front() === "vault" && e.key === Qt.Key_Right && vault.gridView) vault.move(1)
        else if (overlay === "" && Wm.front() === "vault" && e.key === Qt.Key_Left && vault.gridView) vault.move(-1)
        else if (overlay === "" && (e.key === Qt.Key_Return || e.key === Qt.Key_Enter)) run("vault.open")
        else if (overlay === "" && e.key === Qt.Key_Backspace) run("vault.up")
        else if (e.key === Qt.Key_W) toggle("switcher")
        else if (e.key === Qt.Key_N || e.key === Qt.Key_T) toggle("notif")
        else if (e.key === Qt.Key_F) Wm.toggle("forge")
        else if (e.key === Qt.Key_I) toggle("inspector")
        else if (e.key === Qt.Key_Comma) Wm.toggle("set")
        else if (e.key === Qt.Key_L) run("overlay.lock")
        else if (e.key === Qt.Key_X) toggle("workspace")
        else if (e.key === Qt.Key_Slash) toggle("search")
        else if (e.key === Qt.Key_V) Demo.next()
        else if (e.key === Qt.Key_Period) Demo.focusRing = !Demo.focusRing
        else if (e.key === Qt.Key_H) run("win.desktop")
        else if (e.key === Qt.Key_P) Theme.motion = Theme.motion === "full" ? "balanced" : Theme.motion === "balanced" ? "quiet" : "full"
        else if (e.key === Qt.Key_M) Theme.reducedMotion = !Theme.reducedMotion
        else if (e.key === Qt.Key_O) Theme.opaque = !Theme.opaque
        else if (e.key === Qt.Key_B) run("theme.blur")
    }

    Background { anchors.fill: parent; nodes: !Demo.orbShow || !live.ok }
    LiveOrb { id: live; anchors.fill: parent; anchors.topMargin:32; visible: root.activeSpace === "orb" && (Demo.orbShow || Demo.orbNav) }
    Loader {
        id:worldLoader
        objectName:"worldViewport"
        // World runs under the bar and the dock, edge to edge, like the globe it is.
        anchors.fill:parent; anchors.topMargin:32
        active:root.activeSpace === "world"
        sourceComponent:root.worldViewComponent
    }
    Rectangle {
        visible:pointerTimer.running; z:800; width:28; height:28; radius:14
        x:root.presentationPointer.x*root.width-14; y:root.presentationPointer.y*root.height-14
        color:"transparent"; border.width:2; border.color:Theme.acc
        Timer { id:pointerTimer; interval:2000 }
    }
    Rectangle {
        visible:presentationTimer.running; z:800; y:48
        anchors.horizontalCenter:parent.horizontalCenter
        width:Math.min(parent.width-48, presentationText.implicitWidth+36); height:presentationText.implicitHeight+20
        radius:8; color:Theme.panel; border.color:Theme.hair
        Text { id:presentationText; anchors.centerIn:parent; width:parent.width-36; text:root.presentationNote; textFormat:Text.PlainText
            wrapMode:Text.Wrap; horizontalAlignment:Text.AlignHCenter; color:Theme.t1; font.family:Theme.font; font.pixelSize:15 }
        Timer { id:presentationTimer; interval:4000 }
    }
    readonly property string orbDiagnostics:live.diagnostics

    // A separate layer keeps arbitrarily many local raises below menus and overlays.
    Item { anchors.fill: parent; z: 100
        WinHost { wid: "vault"; baseX: 88; baseY: vault.gridView ? 92 : 100
            VaultWindow { id: vault; objectName: "win.vault"; shown: Wm.isShown("vault"); enterDy: Wm.isMin("vault") ? 160 : 0; enterScale: Wm.isMin("vault") ? 0.8 : 0.96
                width: Wm.isMax("vault") ? root.width - 80 : 1060; height: Wm.isMax("vault") ? root.height - 154 : gridView ? 840 : 720
                onCloseRequested: Wm.close("vault") } }
        WinHost { wid: "stage"; baseX: root.width - 710; baseY: 172
            StageWindow { objectName: "win.stage"; shown: Wm.isShown("stage"); enterDy: Wm.isMin("stage") ? 160 : 0; enterScale: Wm.isMin("stage") ? 0.8 : 0.96
                width: Wm.isMax("stage") ? root.width - 80 : Math.min(600,root.width-32); height: Wm.isMax("stage") ? root.height - 154 : Math.min(400,Math.max(240,root.height-154)) } }
        WinHost { wid: "forge"; baseX: 460; baseY: 130
            ForgeWindow { id: forgeWin; objectName: "win.forge"; wid: "forge"; bridge: bridgeModel; shown: Wm.isShown("forge"); enterDy: Wm.isMin("forge") ? 160 : 0; enterScale: Wm.isMin("forge") ? 0.8 : 0.96
                width: Wm.isMax("forge") ? root.width - 80 : Math.min(1000,root.width-32); height: Wm.isMax("forge") ? root.height - 154 : Math.min(600,Math.max(240,root.height-154)) } }
        WinHost { wid: "set"; baseX: 490; baseY: 140
            SettingsWindow { id: setWin; objectName: "win.set"; wid: "set"; bridge: bridgeModel; shown: Wm.isShown("set"); enterDy: Wm.isMin("set") ? 160 : 0; enterScale: Wm.isMin("set") ? 0.8 : 0.96
                width: Wm.isMax("set") ? root.width - 80 : Math.min(940,root.width-32); height: Wm.isMax("set") ? root.height - 154 : Math.min(650,Math.max(240,root.height-154)) } }
        WinHost { wid: "sentry"; baseX: (root.width - 780) / 2; baseY: 110
            SentryPanel { objectName: "win.sentry"; bridge: bridgeModel; height:Math.min(700,Math.max(240,root.height-154)); shown: Wm.isShown("sentry"); enterDy: Wm.isMin("sentry") ? 160 : 0; enterScale: Wm.isMin("sentry") ? 0.8 : 0.96 } }
    }

    TopBar { objectName: "bar"; z: 600; width: parent.width; overlay: root.overlay; bridge: bridgeModel; hostError: root.hostError; menu: root.overlay === "menu" ? root.menuName : ""
        app: Wm.front() === "" && root.activeSpace === "world" ? "World" : Wm.frontTitle()
        onMenuClicked: function(n,x) { if (root.overlay === "menu" && root.menuName === n) root.overlay = ""; else { root.menuName = n; menuDrop.x = x; root.overlay = "menu" } }
        onToggle: function(n) { root.toggle(n) } }
    Dock { id: dock; objectName: "dock"; z: 500; anchors.horizontalCenter: parent.horizontalCenter; y: parent.height - height - 14
        desktopSpace:root.activeSpace
        onLaunched: function(id) { root.run(id === "orb" || id === "world" ? "app." + id : "dock." + id) }
        onMenuRequested: function(id,cx) { dockMenu.target = id; dockMenu.x = Math.max(8, Math.min(root.width - dockMenu.width - 8, dock.x + cx - dockMenu.width / 2)); root.overlay = "dockmenu" } }
    Rectangle { visible: Demo.orbNav; z: 700; anchors.horizontalCenter: parent.horizontalCenter; y: 40; width: navTxt.implicitWidth + 28; height: 30; radius: 15
        MouseArea { anchors.fill: parent; onClicked: Demo.orbNav = false }
        color: Theme.panel; border.width: 1; border.color: Theme.hair
        Text { id: navTxt; anchors.centerIn: parent; text: Tr.t("Navegando la esfera en 3D · Esc o clic aquí para fijar la cámara"); color: Theme.t2; font.family: Theme.font; font.pixelSize: 12 } }

    Item { anchors.fill: parent; z: 1000
        // Popover dismissal covers the desktop, leaving the global bar usable.
        // A lock overlay keeps its full input boundary.
        MouseArea { x:0; y:root.overlay === "lock" ? 0 : Theme.barH; width:parent.width; height:parent.height-y
            enabled: root.overlay !== ""; onClicked: root.overlay = "" }
        ControlCenter { objectName: "panel.control"; bridge: bridgeModel; shown: root.overlay === "control"; enterDy: -8; x: root.width - width - 24; y: 44 }
        ActivityCenter { objectName: "panel.activity"; bridge: bridgeModel; shown: root.overlay === "activity"; enterDy: -8; x: root.width - width - 24; y: 44 }
        CommandPalette { objectName: "panel.palette"; actionContext: root.actionContext(); onActivated: function(act) { root.overlay = ""; root.run(act) }; shown: root.overlay === "palette"; enterDy: -8; durIn: Theme.durPalette; durOut: Theme.durPaletteClose; x: (root.width - width) / 2; y: 140 }
        WindowSwitcher { id: switcher; objectName: "panel.switcher"; onChosen: function(k) { root.overlay = ""; if (k !== "") root.run("window." + k) }; shown: root.overlay === "switcher"; enterDy: -8; durIn: Theme.durPalette; durOut: Theme.durPaletteClose; x: (root.width - width) / 2; y: 250 }
        NotificationCenter { objectName: "panel.notif"; bridge: bridgeModel; shown: root.overlay === "notif"; enterDy: -8; x: root.width - width - 24; y: 44 }
        DockMenu { id: dockMenu; objectName: "panel.dockmenu"; shown: root.overlay === "dockmenu"; y: dock.y - height - 8; onSpaceRequested: function(space) { root.run("app."+space) }; onActivated: root.overlay = "" }
        InspectorPanel { objectName: "panel.inspector"; bridge: bridgeModel; shown: root.overlay === "inspector"; enterDy: -8; x: root.width - width - 24; y: 44 }
        WorkspaceMenu { objectName: "panel.workspace"; onChosen: function(w) { if (w === "orb") root.run("app.orb"); root.overlay = "" }; shown: root.overlay === "workspace"; x: Math.max(8, root.width - width - 24); y: 42 }
        MenuDropdown { id: menuDrop; objectName: "panel.menu"; actionContext: root.actionContext(); vault: vault; name: root.menuName; shown: root.overlay === "menu"; y: 34; enterDy: -6
            onActivated: function(id) { root.overlay = ""; root.run(id) } }
        SearchPanel { objectName: "globalSearch"; bridge: bridgeModel; onActivated: function(act) { root.overlay = ""; root.run(act) }; shown: root.overlay === "search"; x: (root.width - width) / 2; y: 140 }
    }
    NoteBox { z: 1200; visible: root.actionNote !== ""; text: root.actionNote; icon: "unknown"; x: 24; y: root.height - height - 100; width: Math.min(720,root.width - 48) }
}
