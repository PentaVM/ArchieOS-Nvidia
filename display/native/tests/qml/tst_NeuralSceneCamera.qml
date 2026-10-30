import QtQuick
import QtTest

// Exercise the production camera bindings, handlers and timer without a native
// field, scene geometry, model execution or graphics device. Only the field's
// passive lookup/clock API is substituted; the camera code is loaded unchanged.
Item {
    QtObject {
        id: field
        property var settings: ({})
        property bool still: true
        property bool loading: false
        property bool ready: false
        property int limit: 32
        property var markerRows: []
        property string selected: ""
        property string root: "/first"
        property vector3d at: Qt.vector3d(80, 20, 0)
        property int advances: 0
        signal sourceChanged()
        signal updated()
        signal selectionChanged()
        function labels() { return markerRows }
        function selection(root) { return selected.length ? [selected] : [] }
        function setNumber(key, value) { return true }
        function lookupPosition(id) { return {found:id === "folder", position:at} }
        function advance(dt) { advances++; return 0 }
    }
    TestCase {
        id: test
        name: "NeuralSceneCameraWiring"; when: windowShown
        property var scene: null
        readonly property var fieldStub: field
        function init() {
            field.root = "/first"; field.advances = 0; field.still = true
            field.ready = false; field.markerRows = []; field.selected = ""; field.limit = 32
            var request = new XMLHttpRequest()
            request.open("GET", Qt.resolvedUrl("../../qml/NeuralScene.qml"), false)
            request.send()
            verify(request.status === 0 || request.status === 200)
            var source = request.responseText.replace(/\r\n/g, "\n")
            var end = source.indexOf("    Node {\n        id: fieldNode")
            verify(end > 0, "Camera harness must use the production scene prefix")
            source = source.slice(0, end) + "}\n"
            source = source.replace("import Archie.Observatory", "")
                .replace("required property NeuralField field", "property var field: test.fieldStub")
            scene = Qt.createQmlObject(source, test, Qt.resolvedUrl("../../qml/CameraWiringHarness.qml"))
            verify(scene !== null)
            scene.field = field
            scene.cameraMotion.focusMs = 160
            scene.cameraMotion.returnMs = 160
        }
        function cleanup() { if (scene) { scene.destroy(); scene = null } }
        function test_focus_ticks_when_field_is_still() {
            verify(scene.focusNode("folder"))
            verify(scene.cameraMoving)
            wait(50)
            verify(scene.target.x > 0 && scene.target.x < field.at.x, "Production timer must interpolate the live camera")
            tryCompare(scene, "cameraMoving", false, 1000)
            compare(scene.target.x, field.at.x)
            compare(scene.followedNode, "folder")
            var count = field.advances
            wait(50)
            compare(field.advances, count, "Still camera must stop its timer after settling")
        }
        function test_freeze_excludes_hidden_elapsed_time() {
            verify(scene.focusNode("folder"))
            wait(50)
            scene.running = false
            var x = scene.target.x
            wait(200)
            compare(scene.target.x, x)
            scene.running = true
            wait(20)
            verify(scene.target.x < field.at.x, "Resume must not charge frozen wall time")
            tryCompare(scene, "cameraMoving", false, 1000)
            compare(scene.target.x, field.at.x)
        }
        function test_manual_orbit_cancels_follow_and_pending_transition() {
            verify(scene.focusNode("folder"))
            wait(50)
            scene.orbit(15, 5)
            verify(!scene.cameraMoving)
            compare(scene.followedNode, "")
            var x = scene.target.x, yaw = scene.yaw
            field.updated()
            wait(80)
            compare(scene.target.x, x)
            compare(scene.yaw, yaw)
        }
        function test_node_markers_keep_native_identity_and_position() {
            field.ready = true
            field.markerRows = [{id:"folder",kind:"d",x:80,y:20,z:0}]
            field.selected = "missing"
            field.updated()
            compare(scene.nodeMarkers.length,1)
            compare(scene.nodeMarkers[0].id,"folder")
            compare(scene.nodeMarkers[0].x,80)
            // Native lookup refuses a nonexistent selected ID: no origin marker appears.
            verify(!scene.nodeMarkers.some(function(marker){return marker.id==="missing"}))
            field.selected = "folder"; field.markerRows = []
            field.selectionChanged()
            compare(scene.nodeMarkers.length,1)
            compare(scene.nodeMarkers[0].id,"folder")
            compare(scene.nodeMarkers[0].x,field.at.x)
            verify(scene.nodeMarkers[0].selected)
        }
        function test_reduced_motion_and_root_reset_reach_real_pose() {
            verify(scene.focusNode("folder"))
            scene.reducedMotion = true
            verify(!scene.cameraMoving)
            compare(scene.target.x, field.at.x)
            field.root = "/second"
            compare(scene.followedNode, "")
            verify(!scene.cameraMoving)
            compare(scene.target.x, scene.homeTarget.x)
            compare(scene.target.y, scene.homeTarget.y)
        }
    }
}
