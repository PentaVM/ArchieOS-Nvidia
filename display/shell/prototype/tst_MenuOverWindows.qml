import QtQuick
import QtTest

// Found by the shell's UI walk, which sends its clicks straight to the window: three menu
// entries drawn over a window's title bar did nothing there. Through Qt's own input path,
// which is what a mouse goes through, they must run; these clicks take that path.
Item {
    width: 1920; height: 1080
    Main { id: desktop; width: parent.width; height: parent.height }
    TestCase {
        name: "MenuOverWindows"; when: windowShown
        function openMenu(n) {
            var label = findChild(desktop, "bar.menu." + n)
            verify(label !== null, n)
            mouseClick(label)
            var menu = findChild(desktop, "panel.menu")
            tryVerify(function() { return desktop.overlay === "menu" && menu.opacity === 1 })
        }
        function overTitleBar(item, win) {
            var p = item.mapToItem(desktop, item.width / 2, item.height / 2)
            var top = win.mapToItem(desktop, 0, 0)
            return p.x > top.x + 72 && p.x < top.x + win.width && p.y > top.y && p.y < top.y + 50
        }
        function test_menu_entries_over_a_title_bar_run() {
            var vault = findChild(desktop, "win.vault")
            verify(desktop.run("go:0"))
            tryVerify(function() { return vault.opacity === 1 })
            openMenu("Ir")
            var entry = findChild(desktop, "menu.go:2")
            verify(entry !== null)
            tryVerify(function() { return overTitleBar(entry, vault) }, 2000, "the entry lies over the vault's title bar")
            mouseClick(entry)
            compare(desktop.overlay, "")
            compare(vault.place, 2)

            var reduced = Theme.reducedMotion
            openMenu("Vista")
            entry = findChild(desktop, "menu.theme.motion")
            verify(entry !== null)
            tryVerify(function() { return overTitleBar(entry, vault) }, 2000, "the entry lies over the vault's title bar")
            mouseClick(entry)
            compare(desktop.overlay, "")
            compare(Theme.reducedMotion, !reduced)
            Theme.reducedMotion = reduced
        }
        function test_closing_a_menu_by_clicking_a_window_does_not_raise_it() {
            var vault = findChild(desktop, "win.vault")
            verify(desktop.run("go:0"))
            verify(desktop.run("dock.forge"))
            tryVerify(function() { return Wm.front() === "forge" })
            openMenu("Ventana")
            // Outside the menu, on the vault: the click only closes the menu.
            mouseClick(vault, vault.width - 40, vault.height - 40)
            compare(desktop.overlay, "")
            compare(Wm.front(), "forge")
        }
        function test_theme_toggle_remains_reachable_with_a_panel_open() {
            const previousMode=Theme.colorMode, previousMotion=Theme.reducedMotion
            Theme.reducedMotion=true
            desktop.overlay="control"
            mouseClick(findChild(desktop,"bar.theme"))
            compare(Theme.colorMode,previousMode==="light" ? "dark" : "light")
            compare(desktop.overlay,"control")
            desktop.overlay=""; Theme.colorMode=previousMode; Theme.reducedMotion=previousMotion
        }
    }
}
