import QtQuick

// Chip: icon + word (+ optional sub). Neutral by default; only warn, bad and arch carry a tint.
// `unk` (no reading) and `design` (sample data) have a dashed border: what has no reading
// must look different. `src` is the origin chip (Real · bridge / Diseño · …).
Item {
    id: c
    property string text
    property string sub
    property string icon
    property string tone: "neutral"   // neutral | ok | info | warn | bad | arch | unk
    property bool small: false
    property bool src: false
    property bool design: false        // Diseño · … origin chip (dashed amber)
    property bool kind: false          // App / Archivo / Acción type chip
    property string domain             // personal (circle) | work (diamond): mark replaces the icon
    readonly property bool hasLead: icon !== "" || domain !== ""

    readonly property bool dashed: tone === "unk" || design
    readonly property color iconColor: design ? Theme.warn
        : ({ ok: Theme.ok, info: Theme.info, warn: Theme.warn, bad: Theme.bad, arch: Theme.arch, unk: Theme.unk, neutral: Theme.t3 })[tone]
    readonly property color textColor: (tone === "warn" || tone === "bad" || design) ? Theme.t1
        : tone === "arch" ? Theme.arch : (kind ? Theme.t3 : Theme.t2)

    implicitHeight: small ? 20 : 22
    implicitWidth: row.implicitWidth + (hasLead ? (small ? 6 : 7) : (small ? 8 : 9)) + (small ? 8 : 9)
    width: kind ? Math.max(58, implicitWidth) : implicitWidth
    height: implicitHeight

    Rectangle {
        anchors.fill: parent; radius: height / 2
        visible: !c.dashed
        color: c.tone === "warn" ? Theme.tint(Theme.warn, 0.12) : c.tone === "bad" ? Theme.tint(Theme.bad, 0.13)
             : c.tone === "arch" ? Theme.tint(Theme.arch, 0.12) : Theme.raised
        border.width: 1
        border.color: c.tone === "warn" ? Theme.tint(Theme.warn, 0.36) : c.tone === "bad" ? Theme.tint(Theme.bad, 0.40)
                    : c.tone === "arch" ? Theme.tint(Theme.arch, 0.34) : Theme.hair
    }
    DashedRect {
        anchors.fill: parent; visible: c.dashed; radius: height / 2
        color: c.design ? Theme.tint(Theme.warn, 0.75) : Theme.t3
    }
    Row {
        id: row
        anchors.verticalCenter: parent.verticalCenter
        x: c.kind ? (parent.width - width) / 2 : (c.hasLead ? (c.small ? 6 : 7) : (c.small ? 8 : 9))
        spacing: c.small ? 5 : 6
        Item { visible: c.domain !== ""; width: 12; height: 12; anchors.verticalCenter: parent.verticalCenter
            Rectangle { visible: c.domain === "personal"; anchors.centerIn: parent; width: 7; height: 7; radius: 3.5; color: Theme.info }
            Rectangle { visible: c.domain === "work"; anchors.centerIn: parent; width: 7; height: 7; radius: 2; color: Theme.warn; rotation: 45 } }
        Icon { visible: c.icon !== ""; name: c.icon; size: 12; color: c.iconColor; anchors.verticalCenter: parent.verticalCenter }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: c.text; color: c.textColor
            font.family: (c.src || c.kind) ? Theme.disp : Theme.font
            font.pixelSize: c.src ? 11 : (c.kind ? 10 : (c.small ? 11 : 12))
            font.weight: (c.src || c.kind) ? Font.DemiBold : Font.Medium
            font.capitalization: (c.src || c.kind) ? Font.AllUppercase : Font.MixedCase
            font.letterSpacing: c.src ? 0.84 : (c.kind ? 1.0 : 0)
            font.features: { "tnum": 1 }
        }
        Text {
            visible: c.sub !== ""; anchors.verticalCenter: parent.verticalCenter
            text: c.sub; color: c.tone === "arch" ? Theme.tint(Theme.arch, 0.78) : Theme.t3
            font.family: Theme.font; font.pixelSize: c.small ? 11 : 12; font.features: { "tnum": 1 }
        }
    }
}
