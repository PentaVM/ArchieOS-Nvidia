import QtQuick
import QtTest
import "../../qml" as Native

Item {
    width: 640; height: 480
    QtObject {
        id: field
        property bool loading: false
        property bool ready: true
        property string error: ""
        property int limit: 420
        property int filesPerFolder: 30
        property string scannedAt: "Local snapshot"
        property int revision: 1
        signal updated()
        signal selectionChanged()
        signal sourceChanged()
        function labels() { return [] }
        function selection(root) { return [] }
        function refresh() {}
    }
    QtObject {
        id: view
        property real width: 640
        property real height: 480
        property QtObject camera: QtObject {
            property vector3d position: Qt.vector3d(0, 0, 0)
            property quaternion rotation: Qt.quaternion(1, 0, 0, 0)
        }
    }
    Native.NeuralHud { id: hud; anchors.fill: parent; field: field; view: view; rootLabel: "/local/root" }
    TestCase {
        name: "NeuralHudDetails"; when: windowShown
        function test_provenance_is_collapsed_but_inspectable() {
            var popup = findChild(hud, "neuralDetails")
            var button = findChild(hud, "neuralDetailsButton")
            verify(popup !== null)
            verify(button !== null)
            verify(!popup.visible)
            mouseClick(button, button.width / 2, button.height / 2)
            tryCompare(popup, "visible", true)
            popup.close()
            tryCompare(popup, "visible", false)
        }
    }
}
