import QtQuick
import "Lang.js" as Tr

// The ArchieOS start: the three-second boot animation from the logo package (design by
// Xavi), drawn over the desktop while it comes up, then faded away. It is the same 90
// frames at 30 fps in every theme; reduced motion shows its last frame and fades. A click
// or any key skips it. It holds no state and grants nothing: it only covers the screen.
Rectangle {
    id: splash
    color: Theme.brandInk
    z: 10000
    signal finished()
    readonly property int frames: 90
    property int frame: Theme.reducedMotion ? frames : 1
    function pad(n) { return n < 10 ? "00" + n : n < 100 ? "0" + n : String(n) }
    function finish() { if (!fade.running && opacity > 0) fade.start() }

    // Two images, so the next frame is decoded before it is shown and nothing flickers.
    Image {
        id: shown
        anchors.fill: parent
        fillMode: Image.PreserveAspectCrop
        asynchronous: false
        cache: false
        smooth: true
        source: "../brand/boot/f" + splash.pad(splash.frame) + ".jpg"
    }
    Image {
        visible: false
        asynchronous: true
        cache: false
        source: splash.frame < splash.frames ? "../brand/boot/f" + splash.pad(splash.frame + 1) + ".jpg" : ""
    }
    Timer {
        interval: 33; repeat: true
        running: splash.visible && splash.frame < splash.frames && !fade.running
        onTriggered: splash.frame += 1
    }
    // The last frame (the full logo) stays a moment before the desktop shows through.
    Timer {
        interval: Theme.reducedMotion ? 900 : 350
        running: splash.frame >= splash.frames
        onTriggered: splash.finish()
    }
    NumberAnimation {
        id: fade
        target: splash; property: "opacity"; to: 0
        duration: Theme.reducedMotion ? 150 : 450; easing.type: Easing.InOutQuad
        onFinished: { splash.visible = false; splash.finished() }
    }
    MouseArea { anchors.fill: parent; onClicked: splash.finish() }
    Keys.onPressed: function(event) { splash.finish(); event.accepted = true }
    focus: visible
    Accessible.role: Accessible.Animation
    Accessible.name: Tr.t("ArchieOS iniciando")
}
