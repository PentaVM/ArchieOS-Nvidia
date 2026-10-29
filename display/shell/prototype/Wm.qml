pragma Singleton
import QtQuick
import "Lang.js" as Tr

// Window manager of the shell prototype: which windows exist, which are minimised or zoomed, which
// one is in front. This manages local Qt surfaces only, never external applications, OS
// workspaces, personal/work domains or Core authority.
//
//   open    = the window exists (closing it forgets its position and zoom)
//   min     = hidden in the dock, still open
//   max     = zoomed to the desktop area
//   z       = stacking order; the shown window with the highest z is in front
//
// "Show desktop" (Demo.bare) hides every window without changing any of this.
QtObject {
    id: wm
    readonly property var ids: ["vault", "forge", "sentry", "stage", "set"]
    readonly property var meta: ({
        vault:  { title: "Vault",   icon: "dkVault",  domain: "" },
        forge:  { title: "Forge",   icon: "dkForge",  domain: "" },
        sentry: { title: "Sentry",  icon: "dkSentry", domain: "" },
        stage:  { title: Tr.t("Stage · vista local"), icon: "dkStage", domain: "" },
        set:    { title: Tr.t("Ajustes"), icon: "dkSet",    domain: "" } })

    property int top: 3
    property var st: ({
        vault:  { open: true,  min: false, max: false, z: 2 },
        forge:  { open: false, min: false, max: false, z: 0 },
        sentry: { open: false, min: false, max: false, z: 0 },
        stage:  { open: true,  min: false, max: false, z: 1 },
        set:    { open: false, min: false, max: false, z: 0 } })
    // Drag offsets from each window's home position.
    property var off: ({})
    // WinHost of each window, so a drag can ask for a clamped position (not reactive on purpose).
    property var hosts: ({})

    function _set(id, patch) {
        var n = {}
        for (var k in st) n[k] = st[k]
        var w = {}
        for (var p in st[id]) w[p] = st[id][p]
        for (var q in patch) w[q] = patch[q]
        n[id] = w
        st = n
    }
    function isOpen(id) { return !!st[id] && st[id].open }
    function isMin(id) { return !!st[id] && st[id].open && st[id].min }
    function isMax(id) { return !!st[id] && st[id].max }
    function isShown(id) { return isOpen(id) && !st[id].min && !Demo.bare }
    function z(id) { return st[id] ? st[id].z : 0 }
    function ox(id) { return off[id] ? off[id].x : 0 }
    function oy(id) { return off[id] ? off[id].y : 0 }
    function title(id) { return meta[id] ? meta[id].title : "" }

    // The shown window with the highest z, or "".
    function front() {
        var best = "", bz = -1
        for (var i = 0; i < ids.length; i++) {
            var id = ids[i]
            if (isShown(id) && st[id].z > bz) { best = id; bz = st[id].z }
        }
        return best
    }
    function frontTitle() { var f = front(); return f === "" ? Tr.t("Escritorio") : meta[f].title }
    function shownCount() { var n = 0; for (var i = 0; i < ids.length; i++) if (isShown(ids[i])) n++; return n }
    function openCount() { var n = 0; for (var i = 0; i < ids.length; i++) if (isOpen(ids[i])) n++; return n }

    // Open windows, front first (minimised ones last among equals by z). For the switcher and the menu.
    function list() {
        var out = []
        for (var i = 0; i < ids.length; i++) {
            var id = ids[i]
            if (isOpen(id)) out.push({ id: id, title: meta[id].title, icon: meta[id].icon, domain: meta[id].domain, min: st[id].min, z: st[id].z })
        }
        out.sort(function (a, b) { return b.z - a.z })
        var f = front()
        for (var j = 0; j < out.length; j++) out[j].front = (out[j].id === f)
        return out
    }

    function open(id) {
        if (!st[id]) return
        Demo.bare = false
        top += 1
        _set(id, { open: true, min: false, z: top })
    }
    function close(id) {
        if (!isOpen(id)) return
        _set(id, { open: false, min: false, max: false })
        var o = {}
        for (var k in off) if (k !== id) o[k] = off[k]
        off = o
    }
    function minimize(id) { if (isOpen(id)) _set(id, { min: true }) }
    function raise(id) {
        if (!isOpen(id) || st[id].min) return
        if (front() === id) return
        top += 1
        _set(id, { z: top })
    }
    function toggleMax(id) {
        if (!isOpen(id)) return
        var o = {}
        for (var k in off) if (k !== id) o[k] = off[k]
        off = o
        top += 1
        _set(id, { max: !st[id].max, z: top, min: false })
    }
    function moveTo(id, x, y) {
        if (!st[id] || !isFinite(x) || !isFinite(y)) return
        var h = hosts[id]
        var p = h ? h.clampOffset(x, y) : { x: x, y: y }
        var o = {}
        for (var k in off) o[k] = off[k]
        o[id] = p
        off = o
    }

    // What clicking an app's dock icon does: open it, bring it back, raise it, or (when it already is in front) minimise it.
    function dockClick(id) {
        if (!isOpen(id) || isMin(id) || Demo.bare) open(id)
        else if (front() !== id) raise(id)
        else minimize(id)
    }
    // What the app's key does: open or bring it back; pressed again while it is in front, close it.
    function toggle(id) {
        if (isShown(id) && front() === id) close(id)
        else open(id)
    }

    function closeFront() { var f = front(); if (f !== "") close(f) }
    function minimizeFront() { var f = front(); if (f !== "") minimize(f) }
    function zoomFront() { var f = front(); if (f !== "") toggleMax(f) }
    function closeAll() { for (var i = 0; i < ids.length; i++) close(ids[i]) }
    function minimizeAll() { for (var i = 0; i < ids.length; i++) minimize(ids[i]) }
    // Brings every open window back (and leaves "show desktop").
    function showAll() {
        Demo.bare = false
        for (var i = 0; i < ids.length; i++) if (isOpen(ids[i])) _set(ids[i], { min: false })
    }
    // The window after the front one, for cycling (Tab in the switcher).
    function next(step) {
        var l = list()
        if (l.length === 0) return ""
        var f = front(), at = 0
        for (var i = 0; i < l.length; i++) if (l[i].id === f) at = i
        return l[(at + (step || 1) + l.length) % l.length].id
    }
}
