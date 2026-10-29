pragma Singleton
import QtQuick
import "Lang.js" as Tr

// Which of the six states the DESIGN-SAMPLE surfaces (Alt-Tab, notifications, Forge, inspector,
// settings, lock, workspace menu, search) show. These surfaces have no data behind them yet, so
// the state is chosen by hand here (key V cycles it); it is never read from a service.
QtObject {
    readonly property var states: ["normal", "vacio", "error", "offline", "degraded", "stale"]
    property string state: "normal"
    // Draws the visible-focus ring on the first focusable item (state "07 · Foco visible").
    property bool focusRing: false
    // Hides Vault and Stage so a surface sits on an empty desktop, as in the design boards.
    property bool bare: true
    // Graph viewer (the desktop sphere): Settings > Visor gráfico.
    property bool orbShow: true
    // The orb's profiles, by index: the one list the shell, Settings and LiveOrb use.
    property var nativeProfiles: []
    readonly property var orbProfiles: {
        // First, Xavi's desktop sphere: the system's own files as a cold hologram.
        var result=["holograma", "holograma-calido", "sphere", "orb", "field", "neural", "crown", "atlas", "dense"]
        for (var i=0;i<nativeProfiles.length;i++)
            if (result.indexOf(nativeProfiles[i])<0) result.push(nativeProfiles[i])
        return nativeProfiles.length ? result.filter(function(p) { return nativeProfiles.indexOf(p)>=0 }) : result
    }
    // The local control server has already validated/applied this profile atomically.
    property string orbAcceptedProfile: ""
    property int orbProfile: 0          // index into orbProfiles; 0 Holograma (Xavi's desktop sphere)
    property int orbBrightI: 1
    property int orbActI: 2
    property bool orbSpin: true
    property int orbNodesI: 3           // visible nodes: 0 60 · 1 220 · 2 800 · 3 2 500 · 4 5 000
    property bool orbDust: false        // "dust of light": decorative particles that grey the picture
    property int orbGradeI: 1           // colour grade of the sphere: 0 original · 1 cold hologram · 2 vivid
    property bool orbNav: false         // "navigate the sphere in 3D" (free camera); otherwise the camera is fixed
    property bool orbHud: true
    // Silent mode: the shell's own notifications (toasts) stay hidden. It is local to this shell;
    // no system-wide Do Not Disturb exists behind it.
    property bool silent: false
    // Notifications the person has dismissed in the centre (by entry key). Local to this shell and not
    // persisted: the entries are design samples, so "restore" brings the examples back.
    property var notifGone: []
    function dismiss(k) { if (notifGone.indexOf(k) < 0) notifGone = notifGone.concat([k]) }
    function dismissAll(ks) { var g = notifGone.slice(); for (var i = 0; i < ks.length; i++) if (g.indexOf(ks[i]) < 0) g.push(ks[i]); notifGone = g }
    function restoreNotifs() { notifGone = [] }

    // The real time, to the minute, for the top bar and the lock screen.
    property date now: new Date()
    readonly property bool english: Tr.english()
    readonly property var dayShort: english ? ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
                                            : ["dom", "lun", "mar", "mié", "jue", "vie", "sáb"]
    readonly property var dayLong: english ? ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"]
                                           : ["domingo", "lunes", "martes", "miércoles", "jueves", "viernes", "sábado"]
    readonly property var monthShort: english ? ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
                                              : ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic"]
    readonly property var monthLong: english ? ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"]
                                             : ["enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre"]
    function two(n) { return (n < 10 ? "0" : "") + n }
    readonly property string time: two(now.getHours()) + ":" + two(now.getMinutes())
    readonly property string barClock: english
        ? dayShort[now.getDay()] + " " + monthShort[now.getMonth()] + " " + now.getDate() + "  " + time
        : dayShort[now.getDay()] + " " + now.getDate() + " " + monthShort[now.getMonth()] + "  " + time
    readonly property string longDate: english
        ? dayLong[now.getDay()] + ", " + monthLong[now.getMonth()] + " " + now.getDate()
        : dayLong[now.getDay()] + " " + now.getDate() + " de " + monthLong[now.getMonth()]
    property Timer tick: Timer { interval: 5000; running: true; repeat: true; onTriggered: Demo.now = new Date() }

    function next() { state = states[(states.indexOf(state) + 1) % states.length] }
}
