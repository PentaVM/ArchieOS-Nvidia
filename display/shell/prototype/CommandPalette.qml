import QtQuick
import "ActionRules.js" as ActionRules
import "Lang.js" as Tr

// Command palette / universal launcher, 720 px wide. Search field, grouped results with a type
// chip, key hints at the foot. Never searches the web while typing; actions without a contract
// are disabled and say why.
Panel {
    id: pal
    kind: "panel"
    width: 720
    height: sf.height + body.implicitHeight + pf.height + 18
    property int current: 0
    signal activated(string act)
    property var actionContext: ({})
    readonly property var rows: Synthetic.search(inp.text).filter(function(r) {
        return r.group !== Tr.t("Archivos")
    }).map(function(r) {
        var row = Object.assign({}, r)
        if (!row.act || !ActionRules.allowed(row.act, pal.actionContext))
            row.why = ActionRules.reason(row.act || "", pal.actionContext)
        return row
    })
    function run(i) { var r = rows[i]; if (shown && r && !r.why) activated(r.act) }
    onShownChanged: if (shown) { inp.text = ""; current = 0; inp.forceActiveFocus() }

    component Kbd: Rectangle {
        property string label
        implicitWidth: Math.max(20, kt.implicitWidth + 12); implicitHeight: 20; radius: 5
        color: Theme.raised; border.width: 1; border.color: Theme.hair
        Text { id: kt; anchors.centerIn: parent; text: parent.label; color: Theme.t2; font.family: Theme.font; font.pixelSize: 11; font.weight: Font.Medium }
    }

    // search field
    Item { id: sf; x: 0; y: 0; width: parent.width; height: 52
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.hair }
        Row { x: 16; anchors.verticalCenter: parent.verticalCenter; spacing: 10
            Icon { name: "search"; size: 18; color: Theme.t2; anchors.verticalCenter: parent.verticalCenter }
            Item { width: 540; height: 26; anchors.verticalCenter: parent.verticalCenter
                TextInput { id: inp; anchors.fill: parent; verticalAlignment: TextInput.AlignVCenter; clip: true
                    color: Theme.t1; font.family: Theme.font; font.pixelSize: 17; selectionColor: Theme.acc; selectedTextColor: Theme.onAccent; cursorDelegate: Rectangle { width: 1.5; color: Theme.acc }
                    onTextChanged: pal.current = 0
                    Keys.onPressed: function (e) {
                        if (e.key === Qt.Key_Down) { pal.current = Math.min(pal.rows.length - 1, pal.current + 1); e.accepted = true }
                        else if (e.key === Qt.Key_Up) { pal.current = Math.max(0, pal.current - 1); e.accepted = true }
                        else if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter) { pal.run(pal.current); e.accepted = true } } }
                Text { visible: inp.text === ""; text: Tr.t("Buscar apps, carpetas y acciones"); color: Theme.tdis; font.family: Theme.font; font.pixelSize: 13; anchors.verticalCenter: parent.verticalCenter } } }
        Kbd { anchors.right: parent.right; anchors.rightMargin: 16; anchors.verticalCenter: parent.verticalCenter; label: "esc" } }

    Column { id: body; y: sf.height + 4; width: parent.width
        Repeater { model: pal.rows
            delegate: Column { width: pal.width
                Text { visible: index === 0 || pal.rows[index - 1].group !== modelData.group
                    text: modelData.group.toUpperCase(); leftPadding: 20; topPadding: 12; bottomPadding: 4
                    color: Theme.t3; font.family: Theme.disp; font.pixelSize: 11; font.weight: Font.DemiBold; font.letterSpacing: 1.26 }
                Rectangle { x: 8; width: pal.width - 16; height: 52; radius: Theme.rRow
                    readonly property bool sel: index === pal.current
                    readonly property bool dis: !!modelData.why
                    color: sel ? Theme.sel : "transparent"; border.width: 1; border.color: sel ? Theme.tint(Theme.acc, 0.35) : "transparent"
                    Rectangle { x: 12; anchors.verticalCenter: parent.verticalCenter; width: 32; height: 32; radius: 8; color: Theme.raised; border.width: 1; border.color: Theme.hair
                        Icon { anchors.centerIn: parent; name: modelData.icon; size: 20; color: parent.parent.dis ? Theme.tdis : Theme.t2 } }
                    Column { x: 56; anchors.verticalCenter: parent.verticalCenter; width: parent.width - 56 - 150
                        Text { text: modelData.name; elide: Text.ElideRight; width: parent.width; color: parent.parent.dis ? Theme.tdis : Theme.t1
                            font.family: Theme.font; font.pixelSize: 14; font.weight: Font.Medium }
                        Text { text: modelData.hint; elide: Text.ElideRight; width: parent.width; color: Theme.t3; font.family: Theme.font; font.pixelSize: 12 } }
                    Row { anchors.right: parent.right; anchors.rightMargin: 12; anchors.verticalCenter: parent.verticalCenter; spacing: 12
                        Text { visible: !!modelData.why; text: modelData.why ? modelData.why : ""; color: Theme.warn; font.family: Theme.font; font.pixelSize: 11; anchors.verticalCenter: parent.verticalCenter }
                        Chip { small: true; text: modelData.kind; anchors.verticalCenter: parent.verticalCenter }
                        Kbd { visible: parent.parent.sel && !parent.parent.dis; label: "↵"; anchors.verticalCenter: parent.verticalCenter } }
                    MouseArea { anchors.fill: parent; hoverEnabled: true; onEntered: pal.current = index; onClicked: pal.run(index) } } } }
        Item { visible: pal.rows.length === 0; width: pal.width; height: 64
            Text { anchors.centerIn: parent; text: Tr.t("Sin resultados para «") + inp.text + "»"; color: Theme.t3; font.family: Theme.font; font.pixelSize: 13 } } }

    Item { id: pf; x: 0; y: sf.height + 4 + body.implicitHeight + 10; width: parent.width; height: 40
        Rectangle { width: parent.width; height: 1; color: Theme.hair }
        Row { x: 20; anchors.verticalCenter: parent.verticalCenter; spacing: 18
            Row { spacing: 4; Kbd { label: "↑" } Kbd { label: "↓" } Text { text: "elegir"; color: Theme.t3; font.family: Theme.font; font.pixelSize: 12; leftPadding: 4; anchors.verticalCenter: parent.verticalCenter } }
            Row { spacing: 4; Kbd { label: "↵" } Text { text: "abrir"; color: Theme.t3; font.family: Theme.font; font.pixelSize: 12; leftPadding: 4; anchors.verticalCenter: parent.verticalCenter } }
            Row { spacing: 4; Kbd { label: "esc" } Text { text: "cerrar"; color: Theme.t3; font.family: Theme.font; font.pixelSize: 12; leftPadding: 4; anchors.verticalCenter: parent.verticalCenter } } }
        Text { anchors.right: parent.right; anchors.rightMargin: 20; anchors.verticalCenter: parent.verticalCenter
            text: pal.rows.length + Tr.t(" resultados"); color: Theme.t3; font.family: Theme.font; font.pixelSize: 12 } }
}
