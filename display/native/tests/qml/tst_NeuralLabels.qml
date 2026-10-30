import QtQuick
import QtTest
import "../../qml" as Native

Item {
    width: 800; height: 600
    QtObject {
        id: field
        property var labelRows: []
        property var selectionRows: []
        property bool ready: true
        property bool loading: false
        property string error: ""
        property int limit: 420
        property int filesPerFolder: 30
        property int revision: 1
        property string scannedAt: "local snapshot"
        signal updated()
        signal selectionChanged()
        signal sourceChanged()
        function labels() { return labelRows }
        function selection(root) { return selectionRows }
        function refresh() {}
    }
    QtObject {
        id: view
        property real width: 800
        property real height: 600
        property QtObject camera: QtObject {
            property vector3d position: Qt.vector3d(0,0,0)
            property quaternion rotation: Qt.quaternion(1,0,0,0)
        }
        function mapFrom3DScene(position) { return Qt.vector3d(position.x+camera.position.x,position.y,1) }
    }
    Native.ModelActivityHud { id: modelStatus; x:18; y:18; width:300; models:[] }
    Native.NeuralHud {
        id: hud; anchors.fill: parent; field: field; view: view
        modelActivityBounds: Qt.rect(modelStatus.x,modelStatus.y,modelStatus.width,modelStatus.height)
    }
    TestCase {
        name: "NeuralReadableLabels"; when: windowShown
        function cleanup() { field.labelRows=[]; field.selectionRows=[]; modelStatus.models=[]; hud.rootLabel=""; view.camera.position=Qt.vector3d(0,0,0); hud.refresh() }
        function overlap(a,b) { return a.x<b.x+b.width && a.x+a.width>b.x && a.y<b.y+b.height && a.y+a.height>b.y }
        function test_dense_real_projection_has_no_stacked_labels() {
            var rows=[]
            for(var i=0;i<20;i++) rows.push({id:"folder-"+i,text:"Folder "+i,kind:"d",x:400+i%2,y:300+i%3,z:0})
            field.labelRows=rows; hud.refresh()
            compare(hud.placedLabels.length,20)
            for(var a=0;a<hud.placedLabels.length;a++) {
                var label=findChild(hud,"neuralLabel."+hud.placedLabels[a].id)
                verify(label!==null)
                verify(Math.abs(label.color.a-0.75)<0.01,"A white backing must keep the specified 75% opacity")
                for(var b=a+1;b<hud.placedLabels.length;b++) verify(!overlap(hud.placedLabels[a],hud.placedLabels[b]))
            }
        }
        function test_camera_reprojects_anchor_without_changing_identity() {
            field.labelRows=[{id:"real-folder",text:"Real folder",kind:"d",x:400,y:300,z:0}]
            hud.refresh()
            compare(hud.placedLabels[0].anchorX,400)
            view.camera.position=Qt.vector3d(37,0,0)
            compare(hud.placedLabels[0].id,"real-folder")
            compare(hud.placedLabels[0].anchorX,437)
        }
        function test_root_card_has_a_readable_basename() {
            hud.rootLabel="C:/Users/Penta/Projects/ArchieOS/"
            field.selectionRows=[".","C:/USERS/PENTA/PROJECTS/ARCHIEOS","359 ELEMENTOS"]
            hud.refresh()
            compare(findChild(hud,"neuralSelectionText.0").text,"ARCHIEOS")
        }
        function test_six_real_model_rows_reserve_their_measured_height() {
            var models=[]
            for(var i=0;i<6;i++) models.push({id:"model-"+i,active:true,stale:false,tps_available:true,tps:12.5})
            modelStatus.models=models
            tryVerify(function(){return modelStatus.height>0})
            var labels=[]
            for(var j=0;j<8;j++) labels.push({id:"folder-"+j,text:"Folder "+j,kind:"d",x:150,y:modelStatus.y+modelStatus.height-8,z:0})
            field.labelRows=labels; hud.refresh()
            verify(hud.placedLabels.length>0)
            compare(hud.modelActivityBounds.height,modelStatus.height)
            var actualBounds={x:modelStatus.x-6,y:modelStatus.y-6,width:modelStatus.width+12,height:modelStatus.height+12}
            for(var k=0;k<hud.placedLabels.length;k++) verify(!overlap(hud.placedLabels[k],actualBounds))
        }
    }
}
