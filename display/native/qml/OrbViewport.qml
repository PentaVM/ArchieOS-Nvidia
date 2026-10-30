import QtQuick
import QtQuick3D
import QtQuick3D.Helpers
import Archie.Observatory
import "OrbActions.js" as OrbActions
import "ModelActivity.js" as ModelActivity
import "qrc:/archie/shell/prototype" as Shell
import "../../shell/prototype/Lang.js" as Tr

// Shared by the shell and native renderer: one controller, one active scene.
Item {
    id: viewport
    required property var controller
    required property var bridge
    property bool navigationEnabled: true
    property bool hudVisible: true
    property bool reducedMotion: false
    property bool frozen: false
    property bool lightMode: Shell.Theme.light
    property double modelActivityNow: Date.now()
    property var modelActivity: ModelActivity.fromSnapshot(bridge.snapshot, modelActivityNow, bridge.connected)
    Timer { interval:1000; repeat:true; running:viewport.presented; onTriggered:viewport.modelActivityNow=Date.now() }
    property color backgroundColor: Shell.Theme.desktop
    property color textColor: Shell.Theme.t1
    property color secondaryColor: Shell.Theme.t2
    property color mutedColor: Shell.Theme.t3
    property color errorColor: Shell.Theme.bad
    property int frameIntervalMs: Number(bridge.frame.frameIntervalMs === undefined ? 16 : bridge.frame.frameIntervalMs)
    readonly property bool presented: viewport.visible && bridge.presented && viewport.width > 0 && viewport.height > 0
    // The hologram's grade is the look itself and costs one full-screen pass, so it runs
    // whenever the view draws; the governor's post-processing switch keeps the bloom.
    readonly property bool graded: !!controller.look.graded
    property bool ownsController: false
    // Xavi's neural sphere draws the orb when the look preset names it as the field source.
    readonly property bool neural: !!controller.settings.fieldSource && controller.settings.fieldSource === "neural"

    function viewVerb(action,args) {
        if (!viewport.presented || !viewport.ownsController) return Tr.t("El Orbe no está visible")
        if (action === "freeze") { viewport.frozen=args.enabled; return "" }
        return OrbActions.perform(action,args,controller,scene,orbHud,viewport.neural ? neuralView : null)
    }
    Component.onCompleted: ownsController = controller.acquireView(viewport)
    Component.onDestruction: controller.releaseView(viewport)
    Binding { target: controller; property: "active"; value: viewport.presented && !viewport.neural && !viewport.frozen; when: viewport.ownsController }
    Binding { target: controller; property: "renderBudget"; value: bridge.frame; when: viewport.ownsController }
    // renderBudget carries frameIntervalMs; the controller takes it from there alone.
    Binding { target: controller; property: "reducedMotion"; value: viewport.reducedMotion; when: viewport.ownsController }

    View3D {
        id: view
        anchors.fill: parent
        visible: viewport.ownsController && viewport.presented && viewport.frameIntervalMs > 0 && !viewport.neural
        renderMode: View3D.Offscreen
        camera: scene.camera
        environment: ExtendedSceneEnvironment {
            backgroundMode: SceneEnvironment.Color
            clearColor: viewport.backgroundColor
            // The hologram is thousands of one-pixel additive filaments: multisampling
            // changes how much light each one adds, so it stays off while the grade runs
            // (as in the Observatory window), and the picture is the same on every budget.
            antialiasingMode: Number(viewport.bridge.frame.msaaSamples || 0) > 0 && !viewport.graded
                && viewport.controller.scene.profile !== "low" ? SceneEnvironment.MSAA : SceneEnvironment.NoAA
            antialiasingQuality: Number(viewport.bridge.frame.msaaSamples || 0) >= 4
                ? SceneEnvironment.VeryHigh : SceneEnvironment.High
            // Xavi's sphere was tone mapped as Three.js does (ACES) before its CSS grade.
            tonemapMode: viewport.graded ? SceneEnvironment.TonemapModeAces : SceneEnvironment.TonemapModeFilmic
            // Under the hologram, the same finishing as the Observatory window. It also keeps
            // the post chain running, so the light of thousands of additive filaments sums
            // in HDR and is tone mapped once; without it each line is tone mapped on its
            // own and the dense centre clips to white.
            vignetteEnabled: viewport.graded
            vignetteStrength: 0.35
            ditheringEnabled: viewport.graded
            exposure: viewport.controller.look.exposure
            // Under the hologram's grade the glow is the grade's own, so it looks the same
            // whether or not the budget allows this chain.
            glowEnabled: !!viewport.bridge.frame.postProcessing && viewport.controller.look.bloom && !viewport.graded
            glowStrength: 0.9 + viewport.controller.look.bloomRadius * 0.6
            glowIntensity: viewport.controller.look.bloomStrength * 1.3
            glowBloom: viewport.controller.look.bloomRadius * 0.5
            glowHDRMinimumValue: viewport.controller.look.bloomThreshold
            glowHDRMaximumValue: 8.0
            glowHDRScale: 2.0
            glowQualityHigh: Number(viewport.bridge.frame.msaaSamples || 0) >= 4
        }
        // The hologram's grade over the finished frame, as the CSS filter Xavi's shell laid
        // over the sphere's canvas.
        layer.enabled: viewport.graded
        layer.effect: HologramGrade { look: viewport.controller.look; postProcessing: !!viewport.bridge.frame.postProcessing }
        LumenScene {
            id: scene
            controller: viewport.controller
            aspect: view.width / Math.max(1, view.height)
            viewportWidth: view.width
            viewportHeight: view.height
            reducedMotion: viewport.reducedMotion || !viewport.controller.renderBudget.ambientMotion
            presented: viewport.presented
        }
    }
    NeuralViewport {
        id:neuralView
        anchors.fill: parent
        controller: viewport.controller
        lightMode: viewport.lightMode
        backgroundColor: viewport.backgroundColor
        textColor: viewport.textColor
        mutedColor: viewport.mutedColor
        errorColor: viewport.errorColor
        modelActivity: viewport.modelActivity
        panelColor: Shell.Theme.panel
        borderColor: Shell.Theme.hair
        renderBudget: viewport.bridge.frame
        active: viewport.ownsController && viewport.presented && viewport.neural
        navigationEnabled: viewport.navigationEnabled
        frozen: viewport.frozen
        visible: viewport.ownsController && viewport.presented && viewport.neural
        presented: visible
        hudVisible: viewport.hudVisible
        reducedMotion: viewport.reducedMotion || !viewport.controller.renderBudget.ambientMotion
    }
    OrbHud {
        id:orbHud
        anchors.fill: parent
        controller: viewport.controller
        view: view
        orbScene: scene
        navigationEnabled: viewport.navigationEnabled
        labelsVisible: viewport.hudVisible
        lightMode: viewport.lightMode
        textColor:viewport.textColor
        secondaryColor:viewport.secondaryColor
        mutedColor:viewport.mutedColor
        errorColor:viewport.errorColor
        visible: viewport.ownsController && view.visible
    }
}
