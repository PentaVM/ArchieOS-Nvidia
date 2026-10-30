import QtQuick
import QtQuick3D
import QtQuick3D.Helpers
import Archie.Observatory
import "NeuralControls.js" as Controls

// The orb when the field source is "neural": Xavi's neural sphere, drawn by NeuralScene,
// finished by the bloom and the hologram pass, labelled by NeuralHud. The folder it draws is
// the controller's content root; the controller keeps the presets and the contract.
Item {
    id: viewport
    required property var controller
    property bool presented: viewport.visible
    property bool hudVisible: true
    property bool reducedMotion: false
    property bool lightMode: true
    property color backgroundColor: "#eef3f8"
    property color panelColor: lightMode ? Qt.rgba(1,1,1,.75) : Qt.rgba(.035,.055,.09,.75)
    property color borderColor: lightMode ? "#71859d" : "#718ba7"
    property color textColor: "#142236"
    property color mutedColor: "#566c80"
    property color errorColor: "#ba263b"
    property var modelActivity: null
    // The host's reading of the machine, shown in the HUD's Details.
    property Component machineDetails: null
    // The existing controller/governor supplies authority; this view only applies its ceilings.
    property var renderBudget: controller.renderBudget || ({})
    property bool active: viewport.presented
    property bool navigationEnabled: true
    readonly property int frameIntervalMs: Math.max(0, Number(renderBudget.frameIntervalMs === undefined ? 16 : renderBudget.frameIntervalMs))
    readonly property var requested: Controls.settings(controller.settings)
    readonly property int nodeLimit: Math.max(0, Number(renderBudget.maxVisibleNodes === undefined ? 420 : renderBudget.maxVisibleNodes))
    readonly property int particleLimit: Math.max(0, Number(renderBudget.maxParticles === undefined ? 2000 : renderBudget.maxParticles))
    readonly property bool ambientMotion: renderBudget.ambientMotion !== false
    readonly property bool postProcessing: renderBudget.postProcessing === true
    readonly property bool drawing: viewport.active && viewport.presented && frameIntervalMs > 0 && nodeLimit > 0
    /// The original's deterministic frame, for captures and the SSIM reference.
    property bool still: false
    property alias field: neural
    property alias sphereScene: scene
    property alias view: view
    /// The sphere's clock stopped (`archie visual freeze`): the tests' still frame without a restart.
    property bool frozen: false

    // The display's verbs (DisplayControl checks every argument before these run). Each answers
    // whether it did something, so a caller never shows a view that did not move.
    function focusNode(id) {
        if (id === "") { scene.fit(); return neural.select(".") }
        if (!neural.select(id)) return false
        return scene.focusNode(id)
    }
    function highlightNode(id, on) { return neural.select(on ? id : ".") }
    function moveCamera(move) {
        if (move === "home") scene.fit()
        else if (move === "closer") scene.zoom(0.8)
        else if (move === "farther") scene.zoom(1 / 0.8)
        else if (move === "left") scene.orbit(-15, 0)
        else if (move === "right") scene.orbit(15, 0)
        else if (move === "up") scene.orbit(0, 10)
        else if (move === "down") scene.orbit(0, -10)
        else return false
        return true
    }
    // What a click at (x, y) of this view does: the node under it is selected and opened.
    function clickAt(x, y) {
        if (!viewport.navigationEnabled || !viewport.drawing) return false
        const near = view.mapTo3DScene(Qt.vector3d(x, y, 0))
        const far = view.mapTo3DScene(Qt.vector3d(x, y, 1))
        const direction = Qt.vector3d(far.x - near.x, far.y - near.y, far.z - near.z).normalized()
        const id = neural.pick(scene.camera.position, direction, 3.0)
        return id.length > 0 && neural.activate(id) && scene.focusNode(id)
    }

    NeuralField {
        id: neural
        active: viewport.drawing
        root: viewport.controller.contentRoot
        still: viewport.still || viewport.reducedMotion || !viewport.ambientMotion
        limit: Math.max(1, Math.min(viewport.nodeLimit, viewport.requested.visibleLimit, Number(viewport.controller.settings.contentLimit || 2500)))
        filesPerFolder: Number(viewport.controller.settings.contentFiles || 30)
    }

    function syncSettings() {
        const s=viewport.controller.settings
        const look=Controls.settings(s)
        neural.setBackground(String(viewport.backgroundColor))
        neural.setNumber("brightness",look.brightness)
        neural.setNumber("rotationSpeed",look.rotationSpeed)
        neural.setNumber("pulseSpeed",look.pulseSpeed)
        neural.setFlag("ghosts",look.ghosts)
    }
    onLightModeChanged: syncSettings()
    onBackgroundColorChanged: syncSettings()
    Component.onCompleted: syncSettings()
    Connections {
        target: viewport.controller
        function onSettingsChanged() { viewport.syncSettings() }
    }

    View3D {
        id: view
        anchors.fill: parent
        visible: viewport.drawing && neural.ready
        renderMode: View3D.Offscreen
        camera: scene.camera
        environment: ExtendedSceneEnvironment {
            backgroundMode: SceneEnvironment.Color
            clearColor: neural.background
            // Thousands of one-pixel additive points: multisampling would change how much
            // light each adds, so it stays off, as the original rendered.
            antialiasingMode: SceneEnvironment.NoAA
            tonemapMode: SceneEnvironment.TonemapModeLinear
            // UnrealBloomPass(strength 0.6 x glow, radius 0.45, threshold 0.08)
            glowEnabled: !viewport.lightMode && viewport.postProcessing && viewport.requested.bloom && Number(neural.settings.glow === undefined ? 1 : neural.settings.glow) > 0
            glowIntensity: 0.6 * Number(neural.settings.glow === undefined ? 1 : neural.settings.glow)
            glowBloom: 0.45
            glowStrength: 1.0
            glowHDRMinimumValue: 0.08
            glowHDRMaximumValue: 8.0
            glowHDRScale: 2.0
            glowQualityHigh: true
        }
        layer.enabled: viewport.postProcessing
        layer.effect: NeuralHolo { field: neural; lightMode: viewport.lightMode; pixelRatio: Screen.devicePixelRatio }

        NeuralScene {
            id: scene
            field: neural
            lightMode: viewport.lightMode
            pixelRatio: Screen.devicePixelRatio
            running: viewport.drawing && !viewport.frozen
            frameIntervalMs: viewport.frameIntervalMs
            particleLimit: viewport.particleLimit
            requestedPulses: viewport.requested.requestedPulses
            linksVisible: viewport.requested.links
            shellVisible: viewport.requested.shell
            reducedMotion: viewport.reducedMotion || !viewport.ambientMotion
        }
    }

    NeuralHud {
        z: 1
        anchors.fill: parent
        field: neural
        view: view
        labelsVisible: viewport.hudVisible
        textColor: viewport.textColor
        mutedColor: viewport.mutedColor
        errorColor: viewport.errorColor
        surfaceColor: viewport.backgroundColor
        panelColor: viewport.panelColor
        borderColor: viewport.borderColor
        modelActivityBounds: modelStatusHud.visible ? Qt.rect(modelStatusHud.x,modelStatusHud.y,modelStatusHud.width,modelStatusHud.height) : Qt.rect(0,0,0,0)
        lightMode: viewport.lightMode
        rootLabel: neural.sourceRoot
        telemetryKnown: Array.isArray(viewport.modelActivity)
        machine: viewport.machineDetails
        navigationEnabled: viewport.navigationEnabled && viewport.drawing
    }

    ModelActivityHud {
        id: modelStatusHud
        z: 3
        anchors.left: parent.left; anchors.top: parent.top; anchors.margins: 18
        width: Math.min(300, Math.max(0, parent.width - 150))
        // Shown when Core publishes model activity; when it does not, Details says so.
        visible: viewport.hudVisible && Array.isArray(viewport.modelActivity)
        models: viewport.modelActivity
        panelColor: viewport.panelColor
        borderColor: viewport.borderColor
        textColor: viewport.mutedColor
    }

    // The hands: drag turns, the wheel zooms, a click selects and opens a folded program,
    // a double click returns to HOME.
    MouseArea {
        anchors.fill: parent
        enabled: viewport.navigationEnabled && viewport.drawing
        property point previous
        property bool moved: false
        onPressed: function(mouse) { previous = Qt.point(mouse.x, mouse.y); moved = false }
        onPositionChanged: function(mouse) {
            if (!pressed) return
            if (Math.abs(mouse.x - previous.x) + Math.abs(mouse.y - previous.y) > 0) moved = true
            scene.orbit(-(mouse.x - previous.x) * 0.4, (mouse.y - previous.y) * 0.4)
            previous = Qt.point(mouse.x, mouse.y)
        }
        onWheel: function(wheel) { scene.zoom(wheel.angleDelta.y > 0 ? 0.92 : 1 / 0.92); wheel.accepted = true }
        onDoubleClicked: scene.fit()
        onClicked: function(mouse) {
            if (moved) return
            viewport.clickAt(mouse.x, mouse.y)
        }
    }
}
