import QtQuick
import "qrc:/archie/shell/prototype" as Shell
import "Exploration.js" as Exploration

// Lumenfield's nucleus, halo and selection language, kept to a few screen pixels.
// The light taper is a selection aid, not a claimed physical illuminated column.
Item {
    id: beacons
    required property var controller
    required property var view
    property var points:controller.worldPoints || []
    property string selectedId: ""
    signal chosen(var point)
    Repeater {
        model: beacons.points
        delegate: Item {
            id: marker
            required property var modelData
            objectName:"world.beacon."+modelData.id
            // Requested areas are navigation targets. Nearby observations must
            // not steal their pointer hit when markers overlap at this scale.
            z:modelData.kind==="aoi" ? 3 : beacons.selectedId===modelData.id ? 2 : 1
            readonly property var frame: beacons.controller.frame || ({})
            readonly property var metrics: Exploration.beaconMetrics(beacons.selectedId===modelData.id)
            readonly property vector3d projected: { var eye=frame.cameraEye; return beacons.view.camera===null ? Qt.vector3d(0,0,-1) : beacons.view.mapFrom3DScene(modelData.position) }
            readonly property bool facing: Exploration.pointFacingEye(modelData.position,frame.cameraEye)
            readonly property color tint: modelData.kind==="aoi" ? Shell.Theme.arch : modelData.kind==="aircraft" ? Shell.Theme.info : Shell.Theme.ok
            visible: facing && projected.z>0 && projected.x>=0 && projected.x<beacons.width && projected.y>=0 && projected.y<beacons.height
            x:projected.x-10; y:projected.y-metrics.beamHeight-10
            width:20; height:metrics.beamHeight+20
            Canvas {
                id:beaconCanvas
                anchors.fill:parent
                onPaint: {
                    var ctx=getContext("2d");ctx.reset();var foot=marker.metrics.beamHeight+10,top=foot-marker.metrics.beamHeight
                    var gradient=ctx.createLinearGradient(0,top,0,foot)
                    gradient.addColorStop(0,Shell.Theme.tint(marker.tint,0));gradient.addColorStop(1,Shell.Theme.tint(marker.tint,0.36))
                    ctx.fillStyle=gradient;ctx.beginPath();ctx.moveTo(10,top);ctx.lineTo(12.5,foot);ctx.lineTo(7.5,foot);ctx.closePath();ctx.fill()
                    ctx.strokeStyle=Shell.Theme.tint(marker.tint,0.65);ctx.lineWidth=0.8;ctx.beginPath();ctx.arc(10,foot,marker.metrics.haloRadius,0,2*Math.PI);ctx.stroke()
                    ctx.fillStyle=marker.tint;ctx.beginPath();ctx.arc(10,foot,marker.metrics.coreRadius,0,2*Math.PI);ctx.fill()
                    ctx.fillStyle=Shell.Theme.t1;ctx.beginPath();ctx.arc(9.5,foot-0.5,0.7,0,2*Math.PI);ctx.fill()
                }
                Connections {target:marker;function onTintChanged(){beaconCanvas.requestPaint()} function onMetricsChanged(){beaconCanvas.requestPaint()} }
                Connections {target:Shell.Theme;function onColorModeChanged(){beaconCanvas.requestPaint()} }
            }
            Text {textFormat:Text.PlainText;
                visible:pick.containsMouse || beacons.selectedId===marker.modelData.id || marker.modelData.kind==="aoi"
                x:marker.projected.x>beacons.width-270?-250:23;y:marker.metrics.beamHeight+2;width:240
                text:marker.modelData.label || marker.modelData.id
                color:Shell.Theme.t1;font.family:Shell.Theme.font;font.pixelSize:11;elide:Text.ElideRight
                Rectangle {anchors.fill:parent;anchors.margins:-4;z:-1;radius:4;color:Shell.Theme.panel;border.color:Shell.Theme.hair}
            }
            MouseArea {id:pick;objectName:"world.pick."+marker.modelData.id;anchors.fill:parent;hoverEnabled:true;cursorShape:Qt.PointingHandCursor;onClicked:beacons.chosen(marker.modelData)}
        }
    }
}
