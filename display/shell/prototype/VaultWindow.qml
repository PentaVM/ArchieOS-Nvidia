import QtQuick
import QtCore
import Qt.labs.folderlistmodel
import "Lang.js" as Tr

// Vault window (file manager) in the Archie Mac look: the person's own folders, read only.
// Places, tabs, breadcrumbs, list or grid, sorting and going up all read the real file system
// through FolderListModel; nothing here creates, moves, renames or deletes a file. Opening a
// file hands it to the desktop's default application, as a double click in any file manager
// does. Copy queues and undo need a file-operation contract with Core, which does not exist:
// they are not offered.
Panel {
    id: win
    kind: "window"
    property bool gridView: false
    property int selected: -1
    readonly property string wid: "vault"
    property real dx: 0                      // drag offset, applied by the host
    property real dy: 0
    signal closeRequested()
    signal zoomRequested()
    readonly property color noteColor: Theme.t3
    // Mutations stay out of the shell: no copy, move, rename or delete is ever offered.
    readonly property bool fileOperationsAvailable: false
    readonly property bool canUndo: false

    // ---- places: the person's standard folders, then the whole system -----------------------
    readonly property url homeUrl: StandardPaths.writableLocation(StandardPaths.HomeLocation)
    readonly property var places: [
        { name: Tr.t("Inicio"),     icon: "personal",   url: String(homeUrl) },
        { name: Tr.t("Escritorio"), icon: "spaces",     url: String(StandardPaths.writableLocation(StandardPaths.DesktopLocation)) },
        { name: Tr.t("Documentos"), icon: "documents",  url: String(StandardPaths.writableLocation(StandardPaths.DocumentsLocation)) },
        { name: Tr.t("Descargas"),  icon: "deliveries", url: String(StandardPaths.writableLocation(StandardPaths.DownloadLocation)) },
        { name: Tr.t("Imágenes"),   icon: "image",      url: String(StandardPaths.writableLocation(StandardPaths.PicturesLocation)) },
        { name: Tr.t("Sistema"),    icon: "stack",      url: "file:///" }
    ]

    // ---- state: tabs, navigation, sort -------------------------------------------------------
    property var tabsModel: [ { url: String(homeUrl) } ]
    property int tabIndex: 0
    readonly property var cur: tabsModel[Math.min(tabIndex, tabsModel.length - 1)]
    readonly property string folder: cur.url
    readonly property int place: placeOf(folder)
    property int sortMode: 0                 // 0 name A→Z, 1 name Z→A, 2 newest first
    property string note: ""
    readonly property int count: listing.count
    readonly property bool loading: listing.status === FolderListModel.Loading
    // The listing shown is the folder asked for, read to its end.
    readonly property bool ready: listing.status === FolderListModel.Ready && String(listing.folder) === folder

    FolderListModel {
        id: listing
        folder: win.folder
        showDirsFirst: true
        showDotAndDotDot: false
        showHidden: false
        showOnlyReadable: true
        caseSensitive: false
        sortField: win.sortMode === 2 ? FolderListModel.Time : FolderListModel.Name
        sortReversed: win.sortMode === 1
        onFolderChanged: win.selected = -1
    }

    Timer { id: noteTimer; interval: 3500; onTriggered: win.note = "" }
    function flash(text) { note = text; noteTimer.restart() }

    function placeOf(url) {
        for (var i = 0; i < places.length - 1; i++) if (url === places[i].url) return i
        return url === "file:///" ? places.length - 1 : -1
    }
    function setFolder(url) { var t = tabsModel.slice(); t[tabIndex] = { url: url }; tabsModel = t; selected = -1 }
    function goPlace(i) { if (i < 0 || i >= places.length) return false; setFolder(places[i].url); return true }
    function setTab(i) { tabIndex = i; selected = -1 }
    function newTab() { var t = tabsModel.concat([ { url: String(homeUrl) } ]); tabsModel = t; tabIndex = t.length - 1; selected = -1 }
    function closeTab(i) {
        if (tabsModel.length <= 1) { closeRequested(); return }
        var t = tabsModel.slice(); t.splice(i, 1)
        var ni = tabIndex > i ? tabIndex - 1 : tabIndex
        tabsModel = t; tabIndex = Math.min(ni, t.length - 1); selected = -1
    }

    // Breadcrumbs: "Inicio" and what follows inside the home folder, otherwise the path from /.
    readonly property var crumbs: {
        var home = String(homeUrl), f = folder, out = []
        if (f === home || f.indexOf(home + "/") === 0) {
            out.push({ label: Tr.t("Inicio"), url: home })
            var rest = f.slice(home.length).split("/").filter(function (s) { return s !== "" })
            for (var i = 0; i < rest.length; i++) out.push({ label: decodeURIComponent(rest[i]), url: home + "/" + rest.slice(0, i + 1).join("/") })
        } else {
            out.push({ label: Tr.t("Sistema"), url: "file:///" })
            var parts = f.replace(/^file:\/\//, "").split("/").filter(function (s) { return s !== "" })
            for (var j = 0; j < parts.length; j++) out.push({ label: decodeURIComponent(parts[j]), url: "file:///" + parts.slice(0, j + 1).join("/") })
        }
        return out
    }
    function crumbTo(n) { if (n >= 0 && n < crumbs.length - 1) setFolder(crumbs[n].url) }

    function entry(i) {
        return i < 0 || i >= listing.count ? null : {
            name: listing.get(i, "fileName"), url: String(listing.get(i, "fileUrl")),
            dir: listing.get(i, "fileIsDir"), size: listing.get(i, "fileSize"),
            modified: listing.get(i, "fileModified"), suffix: listing.get(i, "fileSuffix") }
    }
    function select(i) { selected = (i >= 0 && i < listing.count) ? i : -1 }
    function openItem(i) {
        var f = entry(i)
        if (!f) { flash(Tr.t("Elige un archivo o una carpeta.")); return false }
        if (f.dir) { setFolder(f.url); return true }
        if (Qt.openUrlExternally(f.url)) { flash(Tr.t("Abriendo «") + f.name + Tr.t("» con la aplicación predeterminada.")); return true }
        flash(Tr.t("Ninguna aplicación instalada abre «") + f.name + Tr.t("»."))
        return false
    }
    function openSelected() { return openItem(selected) }
    function up() {
        if (folder === "file:///") { flash(Tr.t("Ya estás en la raíz del sistema.")); return false }
        var parent = String(listing.parentFolder)
        setFolder(parent === "" || parent === "file:" ? "file:///" : parent)
        return true
    }
    function move(d) { if (listing.count) selected = Math.max(0, Math.min(listing.count - 1, (selected < 0 ? -1 : selected) + d)) }
    function toggleSort() { sortMode = (sortMode + 1) % 3 }
    function reveal(name) {                  // select a name in the current folder, if it is there
        for (var i = 0; i < listing.count; i++) if (listing.get(i, "fileName") === name) { selected = i; return true }
        flash("«" + name + Tr.t("» no está en esta carpeta."))
        return false
    }

    // ---- presentation: kind, icon and colour from the file's suffix ---------------------------
    readonly property var kinds: ({
        pdf: ["doc", "documents"], doc: ["doc", "documents"], docx: ["doc", "documents"], odt: ["doc", "documents"],
        txt: ["doc", "documents"], md: ["doc", "documents"], rtf: ["doc", "documents"],
        xls: ["sheet", "documents"], xlsx: ["sheet", "documents"], ods: ["sheet", "documents"], csv: ["sheet", "data"],
        png: ["image", "media"], jpg: ["image", "media"], jpeg: ["image", "media"], gif: ["image", "media"],
        svg: ["image", "media"], webp: ["image", "media"],
        mp4: ["video", "media"], mkv: ["video", "media"], mov: ["video", "media"], webm: ["video", "media"],
        mp3: ["speaker", "media"], wav: ["speaker", "media"], flac: ["speaker", "media"], ogg: ["speaker", "media"],
        zip: ["archive", "archive"], tar: ["archive", "archive"], gz: ["archive", "archive"], xz: ["archive", "archive"],
        "7z": ["archive", "archive"], rar: ["archive", "archive"], zst: ["archive", "archive"],
        exe: ["exe", "executable"], sh: ["exe", "executable"], appimage: ["exe", "executable"], bat: ["exe", "executable"],
        step: ["cad", "other"], stl: ["cad", "other"], kicad_pcb: ["cad", "other"], blend: ["cad", "other"], fcstd: ["cad", "other"],
        json: ["data", "data"], toml: ["data", "data"], yaml: ["data", "data"], yml: ["data", "data"],
        gguf: ["data", "data"], safetensors: ["data", "data"], onnx: ["data", "data"]
    })
    function iconOf(dir, suffix) { return dir ? "folder" : (kinds[String(suffix).toLowerCase()] || ["file"])[0] }
    function colorOf(dir, suffix) {
        if (dir) return Theme.t2
        var k = kinds[String(suffix).toLowerCase()]
        return k ? Theme.cat[k[1]] : Theme.cat.other
    }
    function sizeText(dir, bytes) {
        if (dir) return "—"
        if (bytes < 1024) return bytes + " B"
        var units = ["KB", "MB", "GB", "TB"], v = bytes / 1024, u = 0
        while (v >= 1024 && u < units.length - 1) { v /= 1024; u++ }
        return (v < 10 ? v.toFixed(1) : Math.round(v)).toString().replace(".", ",") + " " + units[u]
    }
    function whenText(d) { return Qt.formatDateTime(d, "d MMM yyyy HH:mm") }

    // ---- title bar -------------------------------------------------------------------------
    Item { id: tbar; width: parent.width; height: 52
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.hair }
        TitleDrag { wid: win.wid; reserveRight: 100 }
        Lights { x: 16; anchors.verticalCenter: parent.verticalCenter; wid: win.wid }
        Row { anchors.centerIn: parent
            Text { text: "Vault"; color: Theme.t1; font.family: Theme.disp; font.pixelSize: 17; font.weight: Font.DemiBold; font.letterSpacing: -0.17 }
            Text { text: " — " + (win.crumbs.length ? win.crumbs[win.crumbs.length - 1].label : ""); color: Theme.t2; font.family: Theme.disp; font.pixelSize: 17; font.weight: Font.Medium; font.letterSpacing: -0.17 } }
        Rectangle { id: seg; anchors.right: parent.right; anchors.rightMargin: 16; anchors.verticalCenter: parent.verticalCenter
            width: 76; height: 28; radius: 9; color: Theme.raised; border.width: 1; border.color: Theme.hair
            Rectangle { x: win.gridView ? 38 : 2; y: 2; width: 34; height: 24; radius: 7; color: Theme.pressed
                Behavior on x { NumberAnimation { duration: Theme.durNav; easing.type: Easing.BezierSpline; easing.bezierCurve: Theme.easeStd } } }
            Item { objectName: "vault.viewList"; x: 2; y: 2; width: 34; height: 24; Icon { anchors.centerIn: parent; name: "list"; size: 16; color: win.gridView ? Theme.t3 : Theme.t1 }
                MouseArea { anchors.fill: parent; onClicked: win.gridView = false } }
            Item { objectName: "vault.viewGrid"; x: 38; y: 2; width: 34; height: 24; Icon { anchors.centerIn: parent; name: "grid"; size: 16; color: win.gridView ? Theme.t1 : Theme.t3 }
                MouseArea { anchors.fill: parent; onClicked: win.gridView = true } } } }

    // ---- tabs + up + breadcrumbs -------------------------------------------------------------
    Item { id: tabs; y: tbar.height; width: parent.width; height: 42
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.hair }
        Row { x: 12; anchors.verticalCenter: parent.verticalCenter; spacing: 6
            Repeater { model: win.tabsModel
                delegate: Rectangle { objectName: "vault.tab." + index; height: 28; radius: Theme.rRow; width: Math.min(180, tl.implicitWidth) + 8 + 12 + 19
                    color: index === win.tabIndex ? Theme.sel : (tma.containsMouse ? Theme.hover : "transparent")
                    MouseArea { id: tma; anchors.fill: parent; hoverEnabled: true; onClicked: win.setTab(index) }
                    Row { x: 11; anchors.verticalCenter: parent.verticalCenter; spacing: 8
                        Text { id: tl; width: Math.min(180, implicitWidth); elide: Text.ElideMiddle
                            text: { var p = win.placeOf(modelData.url); if (p >= 0) return win.places[p].name
                                    var s = decodeURIComponent(String(modelData.url)).split("/"); return s[s.length - 1] || "/" }
                            color: index === win.tabIndex ? Theme.t1 : Theme.t2; font.family: Theme.font; font.pixelSize: 13; font.weight: Font.Medium; anchors.verticalCenter: parent.verticalCenter }
                        Item { objectName: "vault.tabClose." + index; width: 12; height: 12; anchors.verticalCenter: parent.verticalCenter
                            Icon { anchors.centerIn: parent; name: "close"; size: 12; color: Theme.t3 }
                            MouseArea { anchors.fill: parent; anchors.margins: -4; onClicked: win.closeTab(index) } } } } }
            Item { objectName: "vault.newTab"; width: 28; height: 26; anchors.verticalCenter: parent.verticalCenter; Icon { anchors.centerIn: parent; name: "plus"; size: 14; color: Theme.t2 }
                MouseArea { anchors.fill: parent; onClicked: win.newTab() } } }
        Row { anchors.right: parent.right; anchors.rightMargin: 14; anchors.verticalCenter: parent.verticalCenter; spacing: 4
            Item { objectName: "vault.up"; width: 24; height: 24; anchors.verticalCenter: parent.verticalCenter
                Icon { anchors.centerIn: parent; name: "chevL"; size: 14; color: win.folder === "file:///" ? Theme.tdis : Theme.t2 }
                MouseArea { anchors.fill: parent; enabled: win.folder !== "file:///"; onClicked: win.up() } }
            Repeater { model: win.crumbs.length > 5 ? win.crumbs.slice(win.crumbs.length - 5) : win.crumbs
                delegate: Row { id: crumb; spacing: 4
                    readonly property int at: win.crumbs.length > 5 ? win.crumbs.length - 5 + index : index
                    readonly property bool last: at === win.crumbs.length - 1
                    Text { objectName: "vault.crumb." + crumb.at; text: modelData.label; color: crumb.last ? Theme.t1 : (cma.containsMouse ? Theme.t2 : Theme.t3); font.family: Theme.font; font.pixelSize: 12
                        font.weight: crumb.last ? Font.Medium : Font.Normal; anchors.verticalCenter: parent.verticalCenter
                        MouseArea { id: cma; anchors.fill: parent; anchors.margins: -3; hoverEnabled: true; enabled: !crumb.last; onClicked: win.crumbTo(crumb.at) } }
                    Icon { visible: !crumb.last; name: "chevR"; size: 12; color: Theme.t3; anchors.verticalCenter: parent.verticalCenter } } } } }

    // ---- footer ----------------------------------------------------------------------------
    Item { id: foot; y: parent.height - 30; width: parent.width; height: 30
        Rectangle { width: parent.width; height: 1; color: Theme.hair }
        Row { x: 16; anchors.verticalCenter: parent.verticalCenter; spacing: 14
            Row { spacing: 4
                Text { text: win.loading ? "…" : String(win.count); color: Theme.t2; font.family: Theme.font; font.pixelSize: 11; font.weight: Font.Medium; font.features: { "tnum": 1 } }
                Text { text: win.selected >= 0 ? Tr.t("elementos · 1 seleccionado") : Tr.t("elementos"); color: Theme.t3; font.family: Theme.font; font.pixelSize: 11 } }
            Text { text: win.note !== "" ? win.note : decodeURIComponent(win.folder.replace(/^file:\/\//, "")); elide: Text.ElideMiddle; width: Math.min(560, implicitWidth)
                color: win.note !== "" ? Theme.warn : Theme.t3; font.family: Theme.font; font.pixelSize: 11 } }
        Row { anchors.right: parent.right; anchors.rightMargin: 16; anchors.verticalCenter: parent.verticalCenter; spacing: 6
            Icon { name: "eye"; size: 12; color: Theme.t3; anchors.verticalCenter: parent.verticalCenter }
            Text { text: Tr.t("Solo lectura · este equipo"); color: Theme.t3; font.family: Theme.font; font.pixelSize: 11; anchors.verticalCenter: parent.verticalCenter } } }

    // ---- sidebar ---------------------------------------------------------------------------
    Rectangle { id: side; x: 0; y: tbar.height + tabs.height; width: 228; height: parent.height - y - foot.height
        color: Theme.side
        Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: Theme.hair }
        Column { x: 10; y: 16; width: parent.width - 20; spacing: 2
            Text { text: Tr.t("UBICACIONES"); leftPadding: 10; bottomPadding: 6; color: Theme.t3; font.family: Theme.disp; font.pixelSize: 10; font.weight: Font.DemiBold; font.letterSpacing: 1.2 }
            Repeater { model: win.places
                delegate: Rectangle { objectName: "vault.place." + index; width: parent.width; height: 30; radius: Theme.rRow; color: win.place === index ? Theme.sel : (sma.containsMouse ? Theme.hover : "transparent")
                    Row { x: 10; anchors.verticalCenter: parent.verticalCenter; spacing: 10
                        Icon { name: modelData.icon; size: 16; color: win.place === index ? Theme.acc : Theme.t3; anchors.verticalCenter: parent.verticalCenter }
                        Text { text: modelData.name; color: win.place === index ? Theme.t1 : Theme.t2; font.family: Theme.font; font.pixelSize: 13; anchors.verticalCenter: parent.verticalCenter } }
                    MouseArea { id: sma; anchors.fill: parent; hoverEnabled: true; onClicked: win.goPlace(index) } } }
            Item { width: 1; height: 14 }
            Text { text: Tr.t("SISTEMA"); leftPadding: 10; bottomPadding: 6; color: Theme.t3; font.family: Theme.disp; font.pixelSize: 10; font.weight: Font.DemiBold; font.letterSpacing: 1.2 }
            // Sealed is a status row, never a folder you can open.
            Item { width: parent.width; height: 30
                Row { x: 10; anchors.verticalCenter: parent.verticalCenter; spacing: 10
                    Icon { name: "sealed"; size: 16; color: Theme.t3; anchors.verticalCenter: parent.verticalCenter }
                    Text { text: "Sealed"; color: Theme.t2; font.family: Theme.font; font.pixelSize: 13; anchors.verticalCenter: parent.verticalCenter } }
                Text { anchors.right: parent.right; anchors.rightMargin: 10; anchors.verticalCenter: parent.verticalCenter; text: Tr.t("solo estado"); color: Theme.t3; font.family: Theme.font; font.pixelSize: 11 } } } }

    // ---- main: list | grid -------------------------------------------------------------------
    Item { id: main; x: side.width; y: side.y; width: parent.width - side.width; height: side.height
        Rectangle { anchors.fill: parent; color: Theme.reading }
        VoidBox { visible: !win.loading && win.count === 0; x: 14; y: 14; width: parent.width - 28; icon: "folder"; title: Tr.t("Carpeta vacía")
            text: Tr.t("No hay archivos visibles aquí, o la carpeta no existe en este equipo. Los archivos ocultos no se muestran.") }

        // ---- list ----
        Item { visible: !win.gridView && win.count > 0; x: 14; y: 8; width: parent.width - 28; height: parent.height - 16
            Item { id: head; width: parent.width; height: 32
                Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.hair }
                Row { objectName: "vault.sortHeader"; x: 12; anchors.verticalCenter: parent.verticalCenter; spacing: 4
                    Text { text: win.sortMode === 2 ? Tr.t("Más recientes") : Tr.t("Nombre"); color: Theme.t1; font.family: Theme.font; font.pixelSize: 11; font.weight: Font.Medium; anchors.verticalCenter: parent.verticalCenter }
                    Icon { name: "chevD"; size: 12; color: Theme.t1; rotation: win.sortMode === 1 ? 180 : 0; anchors.verticalCenter: parent.verticalCenter } }
                MouseArea { x: 0; width: 140; height: parent.height; onClicked: win.toggleSort() }
                Text { x: parent.width - 12 - 200 - 12 - 92; width: 92; horizontalAlignment: Text.AlignRight; anchors.verticalCenter: parent.verticalCenter
                    text: Tr.t("Tamaño"); color: Theme.t3; font.family: Theme.font; font.pixelSize: 11; font.weight: Font.Medium }
                Text { x: parent.width - 12 - 200; anchors.verticalCenter: parent.verticalCenter; text: Tr.t("Modificado"); color: Theme.t3; font.family: Theme.font; font.pixelSize: 11; font.weight: Font.Medium } }
            ListView { id: list; y: head.height + 4; width: parent.width; height: parent.height - y; clip: true
                model: listing; currentIndex: win.selected; boundsBehavior: Flickable.StopAtBounds
                onCurrentIndexChanged: positionViewAtIndex(currentIndex, ListView.Contain)
                delegate: Rectangle { objectName: "vault.row." + index; width: list.width; height: 40; radius: Theme.rRow
                    color: win.selected === index ? Theme.sel : (ra.containsMouse ? Theme.hover : "transparent")
                    Row { x: 12; anchors.verticalCenter: parent.verticalCenter; spacing: 10; width: parent.width - 24 - 92 - 200 - 24
                        Icon { name: win.iconOf(fileIsDir, fileSuffix); size: 18; color: win.colorOf(fileIsDir, fileSuffix); anchors.verticalCenter: parent.verticalCenter }
                        Text { text: fileName; elide: Text.ElideRight; width: parent.width - 28; color: Theme.t1; font.family: Theme.font; font.pixelSize: 13; font.weight: Font.Medium; anchors.verticalCenter: parent.verticalCenter } }
                    Text { x: parent.width - 12 - 200 - 12 - 92; width: 92; horizontalAlignment: Text.AlignRight; anchors.verticalCenter: parent.verticalCenter
                        text: win.sizeText(fileIsDir, fileSize); color: Theme.t2; font.family: Theme.font; font.pixelSize: 13; font.features: { "tnum": 1 } }
                    Text { x: parent.width - 12 - 200; anchors.verticalCenter: parent.verticalCenter; text: win.whenText(fileModified); color: Theme.t2; font.family: Theme.font; font.pixelSize: 13; font.features: { "tnum": 1 } }
                    MouseArea { id: ra; anchors.fill: parent; hoverEnabled: true; onClicked: win.selected = index; onDoubleClicked: win.openItem(index) } } } }

        // ---- grid ----
        GridView { id: grid; visible: win.gridView && win.count > 0; x: 14; y: 14; width: parent.width - 28; height: parent.height - 28; clip: true
            cellWidth: width / 4; cellHeight: 122; model: listing; currentIndex: win.selected; boundsBehavior: Flickable.StopAtBounds
            onCurrentIndexChanged: positionViewAtIndex(currentIndex, GridView.Contain)
            delegate: Item { objectName: "vault.cell." + index; width: grid.cellWidth; height: grid.cellHeight
                Rectangle { anchors.fill: parent; anchors.margins: 6; radius: 12
                    color: win.selected === index ? Theme.sel : "transparent"; border.width: 1; border.color: win.selected === index ? Theme.tint(Theme.acc, 0.45) : "transparent"
                    Column { x: 8; y: 8; width: parent.width - 16; spacing: 4
                        Rectangle { width: parent.width; height: 62; radius: 9; color: Theme.raised; border.width: 1; border.color: Theme.hair
                            Icon { anchors.centerIn: parent; name: win.iconOf(fileIsDir, fileSuffix); size: 34; color: win.colorOf(fileIsDir, fileSuffix) } }
                        Text { text: fileName; elide: Text.ElideRight; width: parent.width; color: Theme.t1; font.family: Theme.font; font.pixelSize: 13; font.weight: Font.Medium }
                        Text { text: win.sizeText(fileIsDir, fileSize) + " · " + Qt.formatDateTime(fileModified, "d MMM HH:mm"); elide: Text.ElideRight; width: parent.width
                            color: Theme.t3; font.family: Theme.font; font.pixelSize: 11; font.features: { "tnum": 1 } } }
                    MouseArea { anchors.fill: parent; onClicked: win.selected = index; onDoubleClicked: win.openItem(index) } } } }
    }
}
