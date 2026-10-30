import QtQuick
import QtQuick3D
import Archie.Observatory
import "NeuralCameraMotion.js" as CameraMotion

// Xavi's neural sphere (Lumenfield's "Esfera neuronal"). Rust (NeuralField) decides every
// position, colour and spark; this file says which material each buffer wears, where the
// camera sits and how it turns. Layers, as scene.ts adds them: link layers, the selection's
// hot lines, node dots, ghost particles and the globe (all in the field), then the
// projection base, its beam and the tilted orbit.
//
// The camera is OrbitControls': it starts at HOME = (0.55, 0.42, 3.55) R looking at
// (0, -0.08 R, 0), turns on its own at 0.3 x rotationSpeed (a minute a turn at 1) and the
// operator drags and zooms it.
Node {
    id: scene
    required property NeuralField field
    property bool lightMode: false
    property real pixelRatio: 1
    property bool running: true
    property bool reducedMotion: false
    property int frameIntervalMs: 16
    property int particleLimit: 2000
    property int requestedPulses: 70
    property bool linksVisible: true
    property bool shellVisible: true
    property alias camera: camera
    readonly property real r: 100
    readonly property var cfg: field.settings
    readonly property real brightness: Number(cfg.brightness === undefined ? 1 : cfg.brightness)
    readonly property real linkOpacity: Number(cfg.linkOpacity === undefined ? 1 : cfg.linkOpacity)
    // Ghost geometry has 1600 fixed decorative points. Omit it when the governor cannot
    // afford the whole buffer; every spark has a trail of five points.
    readonly property bool drawGhosts: !!cfg.ghosts && particleLimit >= 1600
    readonly property int pulseLimit: Math.min(requestedPulses,70, Math.max(0, Math.floor((particleLimit - (drawGhosts ? 1600 : 0)) / 5)))
    readonly property vector3d cyan: lightMode ? Qt.vector3d(0.008, 0.085, 0.16) : Qt.vector3d(0.1144, 0.6584, 1.0)
    property var nodeMarkers: []
    function refreshNodeMarkers() {
        if (!field || !field.ready) { nodeMarkers=[]; return }
        const selected=field.selection("")
        const selectedId=selected.length ? String(selected[0]) : ""
        var markers=field.labels().map(function(row) {
            return {id:row.id,kind:row.kind,x:row.x,y:row.y,z:row.z,selected:row.id===selectedId}
        })
        if(selectedId.length && !markers.some(function(row){return row.id===selectedId})) {
            const lookup=field.lookupPosition(selectedId)
            if(lookup.found) markers.unshift({id:selectedId,kind:"selected",x:lookup.position.x,y:lookup.position.y,z:lookup.position.z,selected:true})
        }
        // These are additional marks for existing nodes, bounded by the same field.
        nodeMarkers=markers.slice(0,Math.min(32,field.limit || 32))
    }
    Connections {
        target: scene.field
        function onUpdated() { scene.refreshNodeMarkers() }
        function onSourceChanged() { scene.refreshNodeMarkers() }
        function onSelectionChanged() { scene.refreshNodeMarkers() }
    }

    // The orbit about the target: degrees and a distance. HOME in those terms.
    readonly property vector3d home: Qt.vector3d(r * 0.55, r * 0.42, r * 3.55)
    readonly property vector3d homeTarget: Qt.vector3d(0, -r * 0.08, 0)
    property vector3d target: homeTarget
    property real yaw: 0
    property real elevation: 0
    property real distance: 1
    property var cameraMotion: null
    property bool cameraMoving: false
    property string followedNode: ""
    readonly property bool cameraEnabled: running && frameIntervalMs > 0

    function currentPose() {
        return {target:{x:target.x,y:target.y,z:target.z}, yaw:yaw,
            elevation:elevation, distance:distance}
    }
    function homePose() {
        const o=orbitOf(home,homeTarget)
        return {target:{x:homeTarget.x,y:homeTarget.y,z:homeTarget.z},
            yaw:o.yaw,elevation:o.elevation,distance:o.distance}
    }
    function applyCameraPose(p) {
        target=Qt.vector3d(p.target.x,p.target.y,p.target.z)
        yaw=p.yaw; elevation=p.elevation; distance=p.distance
        cameraMoving=CameraMotion.moving(cameraMotion)
        followedNode=cameraMotion.focusId
    }
    function resetCamera() {
        if (!cameraMotion)
            cameraMotion=CameraMotion.create(homePose(),{minDistance:r*0.4,maxDistance:r*12})
        else CameraMotion.reset(cameraMotion,homePose())
        CameraMotion.setEnabled(cameraMotion,cameraEnabled)
        applyCameraPose(CameraMotion.pose(cameraMotion))
    }
    function updateCamera() {
        if (!cameraMotion) return
        if (cameraMotion.focusId)
            CameraMotion.follow(cameraMotion,cameraMotion.focusId,
                                field.lookupPosition(cameraMotion.focusId),reducedMotion)
        applyCameraPose(CameraMotion.advance(cameraMotion,Date.now(),reducedMotion))
    }
    function focusNode(id) {
        const lookup=field.lookupPosition(id)
        if (!lookup.found) return false
        if (!cameraMotion) resetCamera()
        CameraMotion.cancel(cameraMotion,currentPose())
        CameraMotion.advance(cameraMotion,Date.now(),reducedMotion)
        if (!CameraMotion.focus(cameraMotion,id,lookup,
                                {distance:Math.min(distance,r*2.2)},reducedMotion)) return false
        applyCameraPose(CameraMotion.pose(cameraMotion))
        return true
    }
    onCameraEnabledChanged: if (cameraMotion) CameraMotion.setEnabled(cameraMotion,cameraEnabled)
    onReducedMotionChanged: if (cameraMotion) updateCamera()

    function orbitOf(position, centre) {
        const d = Qt.vector3d(position.x - centre.x, position.y - centre.y, position.z - centre.z)
        const length = Math.sqrt(d.x * d.x + d.y * d.y + d.z * d.z)
        return { yaw: Math.atan2(d.x, d.z) * 180 / Math.PI, elevation: Math.asin(d.y / length) * 180 / Math.PI, distance: length }
    }
    function fit() {
        if (!cameraMotion) { resetCamera(); return }
        CameraMotion.cancel(cameraMotion,currentPose())
        CameraMotion.advance(cameraMotion,Date.now(),reducedMotion)
        CameraMotion.goHome(cameraMotion,reducedMotion)
        applyCameraPose(CameraMotion.pose(cameraMotion))
    }
    function orbit(dYaw, dElevation) {
        if (cameraMotion) CameraMotion.cancel(cameraMotion,currentPose())
        yaw += dYaw
        elevation = Math.max(-89, Math.min(89, elevation + dElevation))
        if (cameraMotion) {
            CameraMotion.cancel(cameraMotion,currentPose())
            applyCameraPose(CameraMotion.pose(cameraMotion))
        }
    }
    function zoom(factor) {
        if (cameraMotion) CameraMotion.cancel(cameraMotion,currentPose())
        distance = Math.max(r * 0.4, Math.min(r * 12, distance * factor))
        if (cameraMotion) {
            CameraMotion.cancel(cameraMotion,currentPose())
            applyCameraPose(CameraMotion.pose(cameraMotion))
        }
    }
    function place() {
        const e = elevation * Math.PI / 180
        const a = yaw * Math.PI / 180
        camera.position = Qt.vector3d(target.x + distance * Math.cos(e) * Math.sin(a),
                                      target.y + distance * Math.sin(e),
                                      target.z + distance * Math.cos(e) * Math.cos(a))
        camera.lookAt(target)
    }
    onYawChanged: place()
    onElevationChanged: place()
    onDistanceChanged: place()
    onTargetChanged: place()
    Component.onCompleted: { resetCamera(); applyParticleBudget() }

    function link(kind) {
        const links = cfg.links
        return links && links[String(kind)] ? links[String(kind)] : { on: kind < 2, opacity: 1 }
    }

    function applyParticleBudget() {
        if (field) field.setNumber("pulses", pulseLimit)
    }
    onPulseLimitChanged: applyParticleBudget()
    Connections {
        target: scene.field
        function onSourceChanged() { scene.applyParticleBudget() }
        function onRootChanged() { scene.resetCamera() }
        function onUpdated() { if (scene.followedNode && !scene.field.loading) scene.updateCamera() }
    }

    // One clock for the model, shaders and camera, paced by the governor. Timer stops
    // while hidden/frozen; restarting never charges elapsed time spent off screen.
    Timer {
        id: frameTimer
        property double previous: 0
        interval: Math.max(16, scene.frameIntervalMs)
        repeat: true
        running: scene.cameraEnabled && (!scene.field.still || scene.cameraMoving)
        onRunningChanged: previous = Date.now()
        onTriggered: {
            const now = Date.now()
            const dt = scene.field.advance(Math.max(0, (now - previous) / 1000))
            previous = now
            if (!scene.reducedMotion && !scene.cameraMoving && !scene.followedNode) {
                scene.yaw -= 6 * 0.3 * Number(scene.cfg.rotationSpeed === undefined ? 1 : scene.cfg.rotationSpeed) * dt
                if (scene.cameraMotion) CameraMotion.cancel(scene.cameraMotion,scene.currentPose())
            }
            scene.updateCamera()
        }
    }

    PerspectiveCamera {
        id: camera
        fieldOfView: 40
        clipNear: 1
        clipFar: 3000
    }

    Node {
        id: fieldNode

        // A volume at the field's existing origin and radius, never an entity.
        // Unshaded node/link materials below keep their own category colors.
        DirectionalLight {
            eulerRotation: Qt.vector3d(-28,-35,0)
            color: "#d9f2ff"; ambientColor: "#85a3bb"
            brightness: 0.8
        }
        Model {
            visible: scene.shellVisible && !scene.lightMode
            source: "#Sphere"
            scale: Qt.vector3d(scene.r/50,scene.r/50,scene.r/50)
            materials: PrincipledMaterial {
                baseColor: Qt.rgba(0.12,0.47,0.68,0.09)
                alphaMode: PrincipledMaterial.Blend
                roughness: 1; metalness: 0; specularAmount: 0
                depthDrawMode: Material.NeverDepthDraw
            }
        }
        // Light-mode volume/rim and external halo use the actual camera view
        // vector. Both are decorative envelopes of the existing sphere, not nodes.
        Model {
            visible: scene.shellVisible && scene.lightMode
            source: "#Sphere"
            scale: Qt.vector3d(scene.r*1.08/50,scene.r*1.08/50,scene.r*1.08/50)
            materials: CustomMaterial {
                shadingMode: CustomMaterial.Unshaded
                sourceBlend: CustomMaterial.One
                destinationBlend: CustomMaterial.OneMinusSrcAlpha
                cullMode: Material.BackFaceCulling
                depthDrawMode: Material.NeverDepthDraw
                property real atmospherePass: 1
                vertexShader: "qrc:/archie/shaders/neural_atmosphere.vert"
                fragmentShader: "qrc:/archie/shaders/neural_atmosphere_light.frag"
            }
        }
        Model {
            visible: scene.shellVisible && scene.lightMode
            source: "#Sphere"
            scale: Qt.vector3d(scene.r/50,scene.r/50,scene.r/50)
            materials: CustomMaterial {
                shadingMode: CustomMaterial.Unshaded
                sourceBlend: CustomMaterial.One
                destinationBlend: CustomMaterial.OneMinusSrcAlpha
                cullMode: Material.BackFaceCulling
                depthDrawMode: Material.NeverDepthDraw
                property real atmospherePass: 0
                vertexShader: "qrc:/archie/shaders/neural_atmosphere.vert"
                fragmentShader: "qrc:/archie/shaders/neural_atmosphere_light.frag"
            }
        }

        // Real 3D marks bypass D3D's one-pixel point limit. Their coordinates and
        // IDs are read from native labels/selection; the camera and picking stay native.
        Repeater3D {
            model: scene.nodeMarkers.length
            delegate: Node {
                required property int index
                readonly property var marker: scene.nodeMarkers[index]
                readonly property real radius: marker && marker.selected ? 2.25 : marker && marker.kind==="p" ? 1.85 : 1.6
                position: marker ? Qt.vector3d(marker.x,marker.y,marker.z) : Qt.vector3d(0,0,0)
                visible: !!marker
                Model {
                    source: "#Sphere"
                    scale: Qt.vector3d(parent.radius*1.9/50,parent.radius*1.9/50,parent.radius*1.9/50)
                    materials: PrincipledMaterial {
                        lighting: PrincipledMaterial.NoLighting
                        baseColor: scene.lightMode ? Qt.rgba(0.06,0.43,0.65,0.16) : Qt.rgba(0.25,0.77,1,0.18)
                        alphaMode: PrincipledMaterial.Blend
                        depthDrawMode: Material.NeverDepthDraw
                    }
                }
                Model {
                    source: "#Sphere"
                    scale: Qt.vector3d(parent.radius*1.30/50,parent.radius*1.30/50,parent.radius*1.30/50)
                    materials: PrincipledMaterial {
                        lighting: PrincipledMaterial.NoLighting
                        baseColor: scene.lightMode ? "#ffffff" : "#092737"
                        cullMode: Material.FrontFaceCulling
                    }
                }
                Model {
                    source: "#Sphere"
                    scale: Qt.vector3d(parent.radius/50,parent.radius/50,parent.radius/50)
                    materials: PrincipledMaterial {
                        lighting: PrincipledMaterial.NoLighting
                        baseColor: scene.lightMode ? "#154c73" : "#9ce7ff"
                        roughness: 1
                    }
                }
            }
        }

        // --- Link layers: 0 skeleton and axons (their opacity is inside the colours), 2..6 the
        // meshes, bridges and the host's links, 7 the selection's hot lines.
        LineLayer { linkLayer: 0; visible: scene.linksVisible; alpha: scene.lightMode ? 0.75 : 1 }
        LineLayer { linkLayer: 2; visible: scene.linksVisible && scene.link(2).on; alpha: (scene.lightMode ? 0.38 : 1) * scene.link(2).opacity * scene.linkOpacity }
        LineLayer { linkLayer: 3; visible: scene.linksVisible && scene.link(3).on; alpha: (scene.lightMode ? 0.38 : 1) * scene.link(3).opacity * scene.linkOpacity }
        LineLayer { linkLayer: 4; visible: scene.linksVisible && scene.link(4).on; alpha: (scene.lightMode ? 0.38 : 1) * scene.link(4).opacity * scene.linkOpacity }
        LineLayer { linkLayer: 5; visible: scene.linksVisible && scene.link(5).on; alpha: (scene.lightMode ? 0.38 : 1) * scene.link(5).opacity * scene.linkOpacity }
        LineLayer { linkLayer: 6; visible: scene.linksVisible && scene.link(6).on; alpha: (scene.lightMode ? 0.38 : 1) * scene.link(6).opacity * scene.linkOpacity }
        LineLayer { linkLayer: 7; visible: scene.linksVisible }

        // --- The nodes ---
        Model {
            geometry: NeuralGeometry { field: scene.field; kind: NeuralGeometry.Points }
            materials: CustomMaterial {
                id: pointMaterial
                shadingMode: CustomMaterial.Unshaded
                // Light ink is premultiplied: keep the offscreen alpha intact so
                // Qt Quick composition cannot turn translucent dark marks white.
                sourceBlend: scene.lightMode ? CustomMaterial.One : CustomMaterial.SrcAlpha
                destinationBlend: scene.lightMode ? CustomMaterial.OneMinusSrcAlpha : CustomMaterial.One
                cullMode: Material.NoCulling
                depthDrawMode: Material.NeverDepthDraw
                property real time: scene.field.time
                property real pixelRatio: scene.pixelRatio
                property real nearDistance: scene.r * 2.4
                property real farDistance: scene.r * 4.8
                property real brightness: scene.brightness
                property real nodeSize: (scene.lightMode ? 1.7 : 1) * Number(scene.cfg.nodeSize === undefined ? 1 : scene.cfg.nodeSize)
                vertexShader: "qrc:/archie/shaders/neural_point.vert"
                fragmentShader: scene.lightMode ? "qrc:/archie/shaders/neural_point_light.frag" : "qrc:/archie/shaders/neural_point.frag"
            }
        }

        // --- The signals travelling along the links ---
        Model {
            visible: scene.pulseLimit > 0 && Number(scene.cfg.pulses === undefined ? 70 : scene.cfg.pulses) > 0
            geometry: NeuralGeometry { field: scene.field; kind: NeuralGeometry.Sparks }
            materials: CustomMaterial {
                shadingMode: CustomMaterial.Unshaded
                sourceBlend: scene.lightMode ? CustomMaterial.One : CustomMaterial.SrcAlpha
                destinationBlend: scene.lightMode ? CustomMaterial.OneMinusSrcAlpha : CustomMaterial.One
                cullMode: Material.NoCulling
                depthDrawMode: Material.NeverDepthDraw
                property real pixelRatio: scene.pixelRatio
                property real brightness: scene.brightness
                vertexShader: "qrc:/archie/shaders/neural_spark.vert"
                fragmentShader: scene.lightMode ? "qrc:/archie/shaders/neural_spark_light.frag" : "qrc:/archie/shaders/neural_spark.frag"
            }
        }

        // --- Ghost particles on the surface (decoration, not files) and the globe's rings ---
        LineLayer {
            kind: NeuralGeometry.Ghosts
            visible: scene.drawGhosts
            tint: scene.cyan; vertexColours: 0; gain: 1
            alpha: (scene.lightMode ? 0.18 : 0.22) * Number(scene.cfg.ghostOpacity === undefined ? 1 : scene.cfg.ghostOpacity)
            pointSize: (scene.lightMode ? 1.6 : 1.3) * scene.pixelRatio
        }
        LineLayer { kind: NeuralGeometry.Segments; linkLayer: 1; visible: scene.shellVisible; tint: scene.cyan; vertexColours: 0; gain: 1; alpha: scene.lightMode ? 0.40 : 0.07 }
    }

    // --- The projection base, its beam and the tilted orbit ---
    Node {
        visible: !!scene.cfg.base
        Node {
            rotation: scene.field.baseRotation(scene.field.time)
            Repeater3D {
                model: 3
                delegate: LineLayer {
                    required property int index
                    kind: NeuralGeometry.Segments; linkLayer: 2
                    scale: Qt.vector3d(1+(index-1)*0.003,1,1+(index-1)*0.003)
                    tint: scene.cyan; vertexColours: 0; gain: 1
                    alpha: scene.lightMode ? 0.32 : 0.20
                }
            }
        }
        Model {
            geometry: NeuralGeometry { kind: NeuralGeometry.Cone }
            materials: CustomMaterial {
                shadingMode: CustomMaterial.Unshaded
                sourceBlend: scene.lightMode ? CustomMaterial.One : CustomMaterial.SrcAlpha
                destinationBlend: scene.lightMode ? CustomMaterial.OneMinusSrcAlpha : CustomMaterial.One
                cullMode: Material.NoCulling
                depthDrawMode: Material.NeverDepthDraw
                vertexShader: "qrc:/archie/shaders/neural_cone.vert"
                fragmentShader: scene.lightMode ? "qrc:/archie/shaders/neural_cone_light.frag" : "qrc:/archie/shaders/neural_cone.frag"
            }
        }
        Node {
            rotation: scene.field.orbitRotation(scene.field.time)
            Repeater3D {
                model: 3
                delegate: LineLayer {
                    required property int index
                    kind: NeuralGeometry.Segments; linkLayer: 3
                    scale: Qt.vector3d(1+(index-1)*0.003,1,1+(index-1)*0.003)
                    tint: scene.cyan; vertexColours: 0; gain: 1
                    alpha: scene.lightMode ? 0.38 : 0.24
                }
            }
        }
    }

    // One buffer of segments, points or particles with the original's line material: colour
    // per vertex (or one tint), additive, and the fog that takes distant light back to the
    // background.
    component LineLayer: Model {
        id: layer
        property int linkLayer: 0
        property int kind: NeuralGeometry.Lines
        property real alpha: 1
        property vector3d tint: Qt.vector3d(1, 1, 1)
        property real vertexColours: 1
        property real gain: scene.brightness
        property real pointSize: 1
        geometry: NeuralGeometry { field: scene.field; kind: layer.kind; layer: layer.linkLayer }
        materials: CustomMaterial {
            shadingMode: CustomMaterial.Unshaded
            sourceBlend: scene.lightMode ? CustomMaterial.One : CustomMaterial.SrcAlpha
            destinationBlend: scene.lightMode ? CustomMaterial.OneMinusSrcAlpha : CustomMaterial.One
            cullMode: Material.NoCulling
            depthDrawMode: Material.NeverDepthDraw
            property vector3d tint: layer.tint
            property real vertexColours: layer.vertexColours
            property real brightness: layer.gain
            property real alpha: layer.alpha
            property real pointSize: layer.pointSize
            property real fogNear: scene.r * 2
            property real fogFar: scene.r * 5
            property vector3d fogColour: scene.field.backgroundLinear
            vertexShader: "qrc:/archie/shaders/neural_line.vert"
            fragmentShader: scene.lightMode ? "qrc:/archie/shaders/neural_line_light.frag" : "qrc:/archie/shaders/neural_line.frag"
        }
    }
}
