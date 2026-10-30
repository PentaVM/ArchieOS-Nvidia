import QtQuick
import QtQuick.Shapes

// PROVISIONAL desktop background from the Archie Mac mockups: a neutral gradient and a node
// network. The real background is the sphere (Archie Lumenfield), which is not part of this
// redesign and is never touched; nothing here depends on it. Drawn for 1920x1080 and scaled.
Item {
    id: bg
    // The node network stands in for the sphere; lock and boot frames of the design have none.
    property bool nodes: true
    Rectangle { anchors.fill: parent; color: Theme.desktop }
    Shape {
        width: 1920; height: 1080
        scale: Math.max(bg.width / 1920, bg.height / 1080)
        transformOrigin: Item.TopLeft
        preferredRendererType: Shape.CurveRenderer
        // soft washes
        ShapePath { strokeColor: "transparent"
            fillGradient: RadialGradient { centerX: 998; centerY: 497; centerRadius: 520; focalX: 998; focalY: 497
                GradientStop { position: 0; color: Theme.tint(Theme.info, Theme.light ? 0.07 : 0.13) } GradientStop { position: 1; color: "transparent" } }
            PathRectangle { x: 0; y: 0; width: 1920; height: 1080 } }
        ShapePath { strokeColor: "transparent"
            fillGradient: RadialGradient { centerX: 346; centerY: 842; centerRadius: 420; focalX: 346; focalY: 842
                GradientStop { position: 0; color: Theme.tint(Theme.arch, Theme.light ? 0.05 : 0.10) } GradientStop { position: 1; color: "transparent" } }
            PathRectangle { x: 0; y: 0; width: 1920; height: 1080 } }
        // node network: links, then nodes
        ShapePath { strokeColor: bg.nodes ? Theme.tint(Theme.t3, Theme.light ? 0.23 : 0.20) : "transparent"; fillColor: "transparent"; strokeWidth: 1
            PathSvg { path: "M260 140 520 90 840 120 1120 70 1460 130 1740 90M260 140 430 330 700 300 980 250 1280 310 1590 260 1820 380M430 330 160 420M700 300 620 520 900 470 1180 520 1450 480 1700 610M980 250 900 470M1280 310 1180 520M1590 260 1450 480M160 420 380 620 120 700M380 620 620 520M620 520 580 820 300 900M580 820 860 760 1140 820 1420 760 1680 880 1840 1000M900 470 860 760M1180 520 1140 820M1450 480 1420 760M1700 610 1680 880M860 760 1000 1000 1140 820M520 90 430 330M840 120 700 300M1120 70 980 250M1460 130 1280 310M1740 90 1590 260" } }
        ShapePath { strokeColor: bg.nodes ? Theme.tint(Theme.acc, Theme.light ? 0.50 : 0.55) : "transparent"; fillColor: "transparent"; strokeWidth: 5; capStyle: ShapePath.RoundCap
            PathSvg { path: "M260 140h.01M520 90h.01M840 120h.01M1120 70h.01M1460 130h.01M1740 90h.01M160 420h.01M430 330h.01M700 300h.01M980 250h.01M1280 310h.01M1590 260h.01M1820 380h.01M120 700h.01M380 620h.01M620 520h.01M900 470h.01M1180 520h.01M1450 480h.01M1700 610h.01M300 900h.01M580 820h.01M860 760h.01M1140 820h.01M1420 760h.01M1680 880h.01M1840 1000h.01M1000 1000h.01" } }
    }
}
