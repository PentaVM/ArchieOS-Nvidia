import QtQuick
import QtTest

Item {
    property color tileProbe
    width: 600; height: 400
    Loader { id: control; source: "ThemeToggle.qml" }
    TestCase {
        name: "ThemeContrast"; when: windowShown
        function cleanup() { Theme.colorMode="light"; Theme.opaque=false }
        function channel(v) { return v<=0.04045 ? v/12.92 : Math.pow((v+0.055)/1.055,2.4) }
        function lum(c) { return channel(c.r)*.2126+channel(c.g)*.7152+channel(c.b)*.0722 }
        function ratio(a,b) { return (Math.max(lum(a),lum(b))+.05)/(Math.min(lum(a),lum(b))+.05) }
        function over(a,b) { return Qt.rgba(a.r*a.a+b.r*(1-a.a),a.g*a.a+b.g*(1-a.a),a.b*a.a+b.b*(1-a.a),1) }
        function test_toggle_changes_shared_theme_by_pointer_and_keyboard() {
            compare(control.status,Loader.Ready,"The theme toggle must actually load")
            Theme.colorMode="light"
            const light=String(Theme.panel), motion=Theme.motion
            mouseClick(control.item,control.item.width/2,control.item.height/2)
            compare(Theme.colorMode,"dark"); verify(String(Theme.panel)!==light)
            control.item.forceActiveFocus(); keyClick(Qt.Key_Space)
            compare(Theme.colorMode,"light"); compare(Theme.motion,motion)
        }
        function test_requested_material_opacity_does_not_fade_text() {
            Theme.colorMode="light"
            compare(Theme.whiteBackdropOpacity,.60)
            for (const c of [Theme.panel,Theme.win,Theme.bar,Theme.dock]) verify(Math.abs(c.a-.75)<.005)
            compare(Theme.t1.a,1); compare(Theme.t2.a,1); compare(Theme.t3.a,1)
            Theme.opaque=true; compare(Theme.panel.a,1)
        }
        function test_text_contrast_after_real_alpha_compositing() {
            for (const mode of ["light","dark"]) {
                Theme.colorMode=mode
                for (const backing of [Qt.rgba(0,0,0,1),Qt.rgba(1,1,1,1),Theme.desktop])
                    for (const panel of [Theme.panel,Theme.win,Theme.side,Theme.bar,Theme.dock])
                        for (const ink of [Theme.t1,Theme.t2,Theme.t3]) {
                            const bg=over(panel,backing)
                            verify(ratio(ink,bg)>=4.5,mode+" contrast "+ratio(ink,bg)+" for "+ink+" on "+bg)
                        }
                verify(ratio(Theme.onAccent,Theme.acc)>=4.5)
            }
        }
        function test_dock_glyphs_contrast_under_highlight_wash() {
            for (const mode of ["light","dark"]) {
                Theme.colorMode=mode
                for (const key of Object.keys(Theme.dk)) {
                    tileProbe=Theme.dk[key]
                    const washed=over(Qt.rgba(1,1,1,.12),over(Qt.rgba(1,1,1,.10),tileProbe))
                    verify(ratio(Theme.dockInk,washed)>=3,key+" icon contrast "+ratio(Theme.dockInk,washed))
                }
            }
        }
    }
}
