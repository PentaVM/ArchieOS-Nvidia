import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick3D
import QtQuick3D.Helpers
import Archie.Observatory
import "qrc:/archie/shell/prototype" as Shell
import "Exploration.js" as Exploration

ApplicationWindow {
    id: root
    visible: true
    visibility: Window.FullScreen
    color: Shell.Theme.desktop
    property bool worldHost: false
    onVisibleChanged: if (worldHost && !visible) archieBridge.requestMode("cerebra")
    onClosing: function(event) { if(worldHost) { event.accepted=false; hide() } }
    title: "Archie Observatory"

    readonly property color ink: root.scene === "world" ? Shell.Theme.t1 : "#e9edf3"
    readonly property color soft: root.scene === "world" ? Shell.Theme.t2 : "#a4acb9"
    readonly property color dim: root.scene === "world" ? Shell.Theme.t3 : "#687080"
    readonly property color edge: root.scene === "world" ? Shell.Theme.hair : "#1c222c"
    // The corner-bracket frames want more presence than a hairline divider but less
    // than a highlight — bright enough to read as a deliberate frame against the
    // panel fill, dim enough to stay out of the way of anything real on top of it.
    readonly property color frame: "transparent"
    readonly property color cyan: root.scene === "world" ? Shell.Theme.info : "#6fe3f5"
    readonly property color green: root.scene === "world" ? Shell.Theme.ok : "#47d18b"
    readonly property color amber: root.scene === "world" ? Shell.Theme.warn : "#f0b429"
    readonly property color red: root.scene === "world" ? Shell.Theme.bad : "#f2626b"
    property string scene: "orb"
    property bool orbHudVisible: true
    property bool worldHudVisible: true
    property var selection: null
    readonly property bool streamedWorld: globeStream.meshes.length>0 || globeStream.proprietaryActive
    readonly property bool localTerrainPreview: root.scene === "world" && Exploration.localTerrainView(archieBridge.frame,archieBridge.explorationMeshes)
    readonly property bool texturedTerrain: (archieBridge.imageryMeshes || []).some(function(mesh) {return mesh.source_id===archieBridge.selectedImagery && mesh.replacesTerrain===true})
    function uiState() {
        const job=Exploration.selectedJob(archieBridge.exploration || {})
        const stats=view.renderStats
        if (stats.extendedDataCollectionEnabled !== undefined) stats.extendedDataCollectionEnabled=true
        return JSON.stringify({scene:root.scene,selectedArea:job ? job.id : "",selectedPoint:worldHud.selectedPointId,selectedImagery:globeStream.sourceId,cameraOwner:worldHud.cameraOwner,stream:globeStream.status,eye:archieBridge.frame.cameraEye,target:archieBridge.frame.cameraTarget,render:{fps:stats.fps,graphics_api:stats.graphicsApiName,draw_calls:stats.drawCallCount,draw_vertices:stats.drawVertexCount}})
    }

    function updateNavigation() {
        if(root.scene!=="world" || !archieBridge.frame.cameraEye || !archieBridge.frame.cameraTarget) return
        globeStream.setViewport(archieBridge.frame.cameraEye,archieBridge.frame.cameraTarget,
                                Math.round(view.width),Math.round(view.height),archieBridge.frame.cameraFov)
    }
    onSceneChanged: Qt.callLater(root.updateNavigation)
    Connections {target:archieBridge;function onFrameChanged() {root.updateNavigation()} }
    // The scenes and the mode each runs in are DisplayControl's; the CLI and the voice ask
    // it for the same list.
    function changeScene(next) {
        if (displayControl.scenes.indexOf(next) < 0) return
        if (root.worldHost && next !== "world") { root.hide(); return }
        root.scene = next
        root.selection = null
        archieBridge.requestMode(displayControl.modeOf(next))
    }

    Connections {
        target: displayControl
        function onCommandReceived(action, value, enabled) {
            if (action === "scene") root.changeScene(value)
            else if (action === "preset" || action === "cinematic") {
                // DisplayControl applies and validates the look before notifying us.
                root.changeScene("orb")
            } else if (action === "overlay") {
                if (value === "hud") {root.orbHudVisible = enabled;root.worldHudVisible=enabled}
                else lumen.setSetting(value, enabled)
            }
        }
    }

    // Where the sun is, in the Earth's own frame, right now.
    //
    // This is not decoration and it is not a rotating prop. The globe is Earth-fixed —
    // Greenwich sits at +Z and stays there — so over a day the sun is what moves through
    // this frame, and its direction is a real astronomical quantity computed from UTC.
    // The terminator the shaders draw is therefore the actual day/night line at the moment
    // the frame was rendered, which is the whole point: a cinematic Earth that is also a
    // true one. First-order — the equation of time is omitted, worth at most four degrees
    // of longitude — and honest about being so.
    property vector3d sunDirection: Qt.vector3d(0, 0, 1)
    function computeSun() {
        var now = new Date()
        var hour = now.getUTCHours() + now.getUTCMinutes() / 60 + now.getUTCSeconds() / 3600
        var yearStart = Date.UTC(now.getUTCFullYear(), 0, 0)
        var today = Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate())
        var dayOfYear = (today - yearStart) / 86400000
        var deg = Math.PI / 180
        // Solar declination, first order.
        var declination = -23.44 * Math.cos(deg * (360 / 365) * (dayOfYear + 10))
        // The sun stands over 0 degrees longitude at 12:00 UTC and moves west 15 deg/hour.
        var longitude = -15 * (hour - 12)
        var phi = declination * deg
        var lambda = longitude * deg
        // Mesh frame: Greenwich +Z, 90 east +X, north +Y — the same frame the texture and
        // every marker already use.
        root.sunDirection = Qt.vector3d(Math.cos(phi) * Math.sin(lambda),
                                        Math.sin(phi),
                                        Math.cos(phi) * Math.cos(lambda))
    }

    // The model has to be told which view this starts in, not just which view it is
    // switched to. Wiring only the toggle left the two disagreeing until the first click:
    // the interface drew the globe while the model was still in Cerebra, so the camera
    // sat at the registry's home and the globe overflowed the frame. A default that is
    // only correct after somebody presses something is not a default.
    // The sun is computed once here for the opening frame, and then on every real data
    // poll below — never from a free-running timer. The observatory's motion comes from
    // events that actually happened, and the clock advancing on its own is not one of
    // them; over a poll interval the sun moves a quarter of a degree, so tying it to the
    // real pulse loses nothing and keeps the one rule the whole display is built on.
    Component.onCompleted: {
        if (!root.worldHost && typeof shellColorMode !== "undefined") Shell.Theme.colorMode = shellColorMode
        if (!root.worldHost || root.visible) archieBridge.requestMode(displayControl.modeOf(root.scene))
        root.computeSun()
    }

    // Escape goes home. Clicking empty space does the same, but the scene is not always
    // reachable by a click — a panel may be over the part you can see — and returning to
    // a known pose is the one thing an operator should never have to hunt for.
    //
    // A `Shortcut` rather than `Keys.onEscapePressed`: this is an ApplicationWindow, which
    // is a Window and not an Item, so it has neither `focus` nor key handlers. Assigning
    // them is not a no-op — it stops the whole component loading.
    Shortcut {
        sequence: "Escape"
        onActivated: {
            if (root.worldHost) { root.hide(); return }
            root.selection = null
            archieBridge.releaseFocus()
            lumen.clearSelection()
        }
    }
    // The orb's keys: Enter opens or closes the selected module, Backspace folds it.
    Shortcut { sequence: "Return"; enabled: root.scene === "orb" && !!lumen.selection.id; onActivated: lumen.toggleModule(lumen.selection.id) }
    Shortcut { sequence: "Backspace"; enabled: root.scene === "orb" && !!lumen.selection.id; onActivated: lumen.collapse(lumen.selection.id) }

    // Only the scene on screen does work: the orb fetches and steps while it is shown,
    // FORGE watches /proc while it is shown, and neither costs anything otherwise.
    Binding { when: !root.worldHost || root.visible; target: lumen; property: "active"; value: root.scene === "orb" && archieBridge.presented }
    Binding { when: !root.worldHost || root.visible; target: lumen; property: "renderBudget"; value: archieBridge.frame }
    Binding { when: !root.worldHost || root.visible; target: appCatalog; property: "active"; value: root.scene === "forge" && archieBridge.presented }

    // A real event (a new snapshot, a changed machine condition) flashes the orb's
    // membrane once. It never loops: nothing moves without something having happened.
    property real eventFlash: 0
    property var snapshot: archieBridge.snapshot || ({})
    property var status: snapshot.status || ({})
    property var world: archieBridge.world || ({})
    property var observations: world.observations || []
    property var providers: world.providers || []
    property var provider: providers.length ? providers[0] : ({ condition: "unavailable", authority: "no provider" })
    property var kernel: archieBridge.kernel || ({})
    property var orb: archieBridge.orb || ({})
    property var pressure: kernel.pressure || null
    property var zones: kernel.zones || []
    // "unavailable" and "idle" must never render the same. One says the machine is quiet,
    // the other says nothing is being measured, and an operator acts differently on each.
    // `a && b` yields `undefined` when `a` is undefined, not false — so before the first
    // kernel payload arrives these assigned undefined to a bool and Qt warned and kept
    // whatever was there. The distinction they exist to make is exactly the one that
    // breaks: an unset flag reads as "not measured", which is right by luck here and
    // would not be somewhere else.
    property bool pressureKnown: !!(kernel.pressure_availability
                                    && kernel.pressure_availability.state === "available")
    property bool thermalKnown: !!(kernel.thermal_availability
                                   && kernel.thermal_availability.state === "available")
    property var modules: snapshot.modules || []
    property var stages: snapshot.boot ? (snapshot.boot.stages || []) : []

    function stateColor(state) {
        if (state === "ready" || state === "loaded" || state === "healthy") return green
        if (state === "failed" || state === "denied" || state === "degraded") return red
        if (state === "stale" || state === "unsatisfied") return amber
        return dim
    }

    // Thermal states are their own vocabulary — nominal, warm, throttling, critical — and
    // none of them are the module states `stateColor` knows. Passing them through it made
    // a critical zone render grey, which is the one reading that must never look calm.
    function thermalColor(state) {
        if (state === "nominal") return green
        if (state === "warm") return amber
        if (state === "throttling") return red
        if (state === "critical") return red
        return dim
    }

    function verdictColor(verdict) {
        if (verdict === "idle") return green
        if (verdict === "light") return cyan
        if (verdict === "contended") return amber
        if (verdict === "saturated") return red
        return dim
    }

    // Whether the governor has cut the visual budget for a real reason: the machine
    // needs its GPU back (`conserve`), or nothing is even being presented
    // (`headless`) — `RenderTier`'s own two degraded values. One place for this
    // rather than each expensive layer repeating the pair of string comparisons, so a
    // third degraded tier added later cannot update three of four checks and miss one
    // — which is exactly the shape of bug this fixes: the atmosphere, starfield and
    // bloom added for the cinematic pass each checked only `!== "conserve"`, so under
    // `headless` — nothing being presented — they would have kept costing GPU that
    // tier exists to give back.
    function budgetReduced() {
        return archieBridge.frame.tier === "conserve" || archieBridge.frame.tier === "headless"
    }

    // The worst of the three resources, matching how the verdict itself is taken. An
    // average would hide a machine that is fine on CPU and drowning in IO.
    function worstPressureFraction() {
        const p = kernel.pressure
        if (!p) return 0
        const some = function (resource) {
            return resource && resource.some ? Number(resource.some.avg10 || 0) : 0
        }
        return Math.max(some(p.cpu), some(p.memory), some(p.io)) / 100
    }

    function gib(bytes) {
        return (Number(bytes || 0) / 1073741824).toFixed(1) + " GiB"
    }

    // How far the camera is from the globe's own scale, expressed as a level of
    // detail — the same numbering GeoFabric's provider table is defined against.
    //
    // Approximate, and said so wherever it is shown. There is no real ground-sample
    // distance to calibrate against: the four GeoFabric bands (planet/region/city/
    // street) are themselves the qualitative judgement recorded in
    // `archie-geofabric`'s catalogue comments, not a measured resolution threshold, so
    // a more precise formula here would be inventing precision the thing it is
    // approximating does not have. What is real is the mapping's endpoints: `nearest`
    // and `farthest` are `orbit_globe`'s own zoom clamps (`radius * 1.15` and
    // `radius * 8.0`), so this always reflects where the camera actually can and
    // cannot go rather than a range picked to look right.
    function currentLod() {
        const eye = archieBridge.frame.cameraEye
        if (!eye) return 4
        const distance = Math.sqrt(eye.x * eye.x + eye.y * eye.y + eye.z * eye.z)
        if (!(distance > 0)) return 4
        const globeRadius = 20.0  // GlobePolicy default; see the globe Model's own comment
        const altitude=Math.max(10, (distance-globeRadius)*6371008.8/globeRadius)
        return Math.max(0,Math.min(20,Math.round(Math.log2(40075016/altitude))))
    }

    // Which of GeoFabric's named bands is closest to a LOD value. Nearest by absolute
    // distance in LOD-space, not by range membership, so a camera between two bands
    // still gets an answer rather than falling through a gap.
    //
    // Reads the band names and their LODs from `archieBridge.geoFabric.selection`
    // rather than keeping a second copy of that table here. The real table lives once,
    // in `archie-displayd`'s `geofabric.rs`, computed from the same plan the sidebar's
    // GEOFABRIC panel already renders — a QML-side copy would be exactly the kind of
    // duplicate this display avoids everywhere else, one more place for the four bands
    // to silently stop matching each other.
    function nearestBand(lod) {
        const bands = archieBridge.geoFabric.selection || []
        if (bands.length === 0) return ""
        let best = 0
        for (let i = 1; i < bands.length; i++) {
            if (Math.abs(bands[i].lod - lod) < Math.abs(bands[best].lod - lod)) best = i
        }
        return bands[best].band
    }

    // Who is answerable for the layer active at a given band, by id. Looked up in
    // `geoFabric.enabled` rather than duplicated: that list is the one place a
    // provider's authority is recorded, computed once in `archie-displayd` from the
    // real catalogue, and every consumer of it — the sidebar table, this — reads the
    // same entries rather than each keeping its own copy.
    function providerAuthority(id) {
        if (!id) return null
        const providers = archieBridge.geoFabric.enabled || []
        for (let i = 0; i < providers.length; i++) {
            if (providers[i].id === id) return providers[i].authority
        }
        return null
    }

    // What is actually answerable for the ground under the camera right now, in one
    // line. This is the provenance the master directive asks for, put where an
    // operator looking at the globe — not hunting through a sidebar — can see it: two
    // sources says two providers cover the current band and their claims do not carry
    // authority conflicts they do not know about, because each is exactly what its
    // license names it as.
    function bandProvenance(band) {
        const bands = archieBridge.geoFabric.selection || []
        const row = bands.find(function (b) { return b.band === band })
        if (!row) return ""
        const parts = []
        const terrain = providerAuthority(row.terrain)
        const buildings = providerAuthority(row.buildings)
        const photoreal = providerAuthority(row.photoreal)
        if (terrain) parts.push("terrain: " + terrain)
        if (buildings) parts.push("buildings: " + buildings)
        if (photoreal) parts.push("surface: " + photoreal)
        return parts.join("  ·  ")
    }

    Rectangle { anchors.fill:parent; color:Shell.Theme.desktop; visible:root.scene === "world" }
    Rectangle {
        visible:root.scene !== "world"
        anchors.fill: parent
        color: "#04060a"
        gradient: Gradient {
            GradientStop { position: 0.0; color: "#0b0e14" }
            GradientStop { position: 0.55; color: "#07090e" }
            GradientStop { position: 1.0; color: "#050609" }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // The top bar: the name, the three places, and a quiet word on whether Archie is
        // there. Nothing else competes with what is below it.
        Item {
            Layout.fillWidth: true
            Layout.preferredHeight: 64

            Text {
                anchors.left: parent.left; anchors.leftMargin: 28
                anchors.verticalCenter: parent.verticalCenter
                text: "Archie"
                color: root.ink
                font.pixelSize: 19
                font.weight: Font.DemiBold
                font.letterSpacing: 0.2
            }

            // The places, as one segmented control. The mode is told to the model, not
            // merely kept here: the model owns the morph between scenes and re-homes the
            // camera when it changes.
            Rectangle {
                anchors.centerIn: parent
                width: sceneRow.implicitWidth + 8; height: 38; radius: 19
                color: root.scene === "world" ? Shell.Theme.raised : "#10141b"
                border.color: root.scene === "world" ? Shell.Theme.hair : "#1d232d"
                Row {
                    id: sceneRow
                    anchors.centerIn: parent
                    spacing: 2
                    SceneButton {
                        label: "Orb"
                        selected: root.scene === "orb"
                        onClicked: root.changeScene("orb")
                    }
                    // The applications this machine can open. It replaced CORE, whose
                    // nodes repeated what the orb already holds.
                    SceneButton {
                        label: "Forge"
                        selected: root.scene === "forge"
                        onClicked: root.changeScene("forge")
                    }
                    SceneButton {
                        label: "World"
                        selected: root.scene === "world"
                        onClicked: root.changeScene("world")
                    }
                }
            }

            Row {
                anchors.right: parent.right; anchors.rightMargin: 28
                anchors.verticalCenter: parent.verticalCenter
                spacing: 18
                Text {
                    visible: archieBridge.busy
                    anchors.verticalCenter: parent.verticalCenter
                    text: "Leyendo…"; color: root.dim; font.pixelSize: 12
                }
                Row {
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 8
                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        width: 7; height: 7; radius: 3.5
                        color: !archieBridge.connected ? "#d9807f" : snapshot.daemon_reachable ? "#8fe3c0" : "#e6c07a"
                    }
                    Text {
                        id: liveText
                        anchors.verticalCenter: parent.verticalCenter
                        text: !archieBridge.connected ? "Sin conexión"
                              : snapshot.daemon_reachable ? "Archie en línea" : "Archie no responde"
                        color: root.soft
                        font.pixelSize: 13
                    }
                }
            }
            Rectangle {
                anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom
                height: archieBridge.busy ? 2 : 0
                color: "#3aa7ff"
                opacity: 0.6
                Behavior on height { NumberAnimation { duration: 120 } }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            WorldHud {
                id:worldHud
                visible:root.scene === "world" && root.worldHudVisible
                Layout.preferredWidth:Math.min(370,root.width*0.36)
                Layout.fillHeight:true
                controller:archieBridge
                stream:globeStream
                worldActive:root.scene === "world" && root.visible
            }

            Item {
                Layout.fillWidth: true
                Layout.fillHeight: true

                View3D {
                    id: view
                    onWidthChanged: Qt.callLater(root.updateNavigation)
                    onHeightChanged: Qt.callLater(root.updateNavigation)
                    anchors.fill: parent
                    // FORGE is a list, not a scene; the GPU rests while it is shown.
                    visible: root.scene !== "forge"
                    camera: root.scene === "orb" ? lumenScene.camera : camera
                    // The hologram's grade (Xavi's desktop sphere) over the orb's finished frame.
                    layer.enabled: root.scene === "orb" && !!lumen.look.graded
                    layer.effect: HologramGrade { look: lumen.look; postProcessing: !!archieBridge.frame.postProcessing }
                    environment: ExtendedSceneEnvironment {
                        backgroundMode: SceneEnvironment.Color
                        clearColor: root.scene === "orb" ? lumen.look.background : Shell.Theme.desktop
                        // The budget is not a caption. Under real memory or CPU pressure
                        // the observatory spends less GPU, because it is the largest
                        // discretionary consumer on a node that has other work to do.
                        // The sample count is the model's, in the renderer's own units.
                        // It used to be derived here from a budget word that was itself
                        // derived from the kernel verdicts — two mappings between the
                        // telemetry and the setting, and two chances to disagree with the
                        // tier that was decided from the same numbers.
                        //
                        // The quality enum has no Low: Qt offers Medium, High and
                        // VeryHigh. Naming one that does not exist assigned undefined,
                        // so on every machine that was neither idle nor lightly loaded —
                        // which is most of them, most of the time — the quality was never
                        // set at all.
                        // On the low Orb profile, MSAA doubled the WSLg present cost.
                        // Higher profiles keep the model's sample budget.
                        antialiasingMode: archieBridge.frame.msaaSamples > 0
                                          && !(root.scene === "orb" && (!root.orb.profile || root.orb.profile === "low"))
                                          ? SceneEnvironment.MSAA
                                          : SceneEnvironment.NoAA
                        antialiasingQuality: archieBridge.frame.msaaSamples >= 4
                                             ? SceneEnvironment.VeryHigh
                                             : (archieBridge.frame.msaaSamples >= 2
                                                ? SceneEnvironment.High
                                                : SceneEnvironment.Medium)
                        // The orb is lit in linear light and tonemapped as the design is:
                        // ACES, with the design's exposure.
                        // Dated map pixels use the sRGB transfer function without
                        // cinematic contrast; global globe/orb keep their own look.
                        tonemapMode: root.scene === "world" || root.localTerrainPreview ? SceneEnvironment.TonemapModeLinear : root.scene === "orb" && !lumen.look.graded ? SceneEnvironment.TonemapModeFilmic : SceneEnvironment.TonemapModeAces
                        exposure: root.scene === "orb" ? lumen.look.exposure : 1.0

                        // Bloom, and only bloom, and only where the image is genuinely
                        // bright: the sunlit limb, a specular glint, a bright marker. The
                        // HDR floor is above 1.0 so nothing in the midtones blooms — a
                        // globe where everything glows is the cyberpunk cliche this is
                        // meant to avoid. The orb's bloom is the design's: its strength,
                        // radius and threshold are the operator's settings, and it goes off
                        // with the rest of the budget.
                        glowEnabled: !!archieBridge.frame.postProcessing && (root.scene !== "orb" || (lumen.look.bloom && !lumen.look.graded))
                        glowStrength: root.scene === "orb" ? 0.9 + lumen.look.bloomRadius * 0.6 : 0.85
                        glowIntensity: root.scene === "orb" ? lumen.look.bloomStrength * 1.3 : 0.28
                        glowBloom: root.scene === "orb" ? lumen.look.bloomRadius * 0.5 : 0.18
                        glowHDRMinimumValue: root.scene === "orb" ? lumen.look.bloomThreshold : 1.05
                        glowHDRMaximumValue: 8.0
                        glowHDRScale: 2.0
                        glowQualityHigh: archieBridge.frame.msaaSamples >= 4

                        // A whisper of a vignette. Not a filter — it keeps the eye on the
                        // planet by letting the corners fall a little into the dark the
                        // stars already live in.
                        vignetteEnabled: root.scene !== "world" && !root.localTerrainPreview
                        vignetteStrength: 0.35

                        // Break up the gradients in the atmosphere and the dark sky so
                        // they band on an 8-bit panel.
                        ditheringEnabled: true
                    }
                    // Placed by the model, not by this file. It owns the eases between
                    // views, the framing of a focused node, and letting go of a node the
                    // graph has forgotten — which a camera parked here could not do,
                    // because it would not know the node was gone.
                    //
                    // No Behavior on position: the movement is already the model's, and
                    // animating on top of it would be a second clock keeping the same
                    // appointment. They disagree at exactly the interesting moment, when
                    // a mode change arrives during a transition.
                    PerspectiveCamera {
                        frustumCullingEnabled:root.scene==="world"
                        id: camera
                        position: archieBridge.frame.cameraEye
                        eulerRotation: Qt.vector3d(0, 0, 0)
                        fieldOfView: archieBridge.frame.cameraFov
                        // Keep depth precision at AOI scale so a half-metre image
                        // drape is distinct from terrain without moving either layer.
                        clipNear: root.scene === "world" ? Exploration.worldNearClip(archieBridge.frame) : 0.1
                        // Far enough to enclose the star sphere at every framing. The old
                        // 400 sat inside the sky, so the whole starfield was clipped away
                        // and the planet hung in a void — the sky was there, just beyond
                        // the far plane.
                        clipFar: 1600
                        Component.onCompleted: lookAt(archieBridge.frame.cameraTarget)
                        onPositionChanged: lookAt(archieBridge.frame.cameraTarget)
                        Connections { target:archieBridge; function onFrameChanged() { camera.lookAt(archieBridge.frame.cameraTarget) } }
                    }
                    // The sun. Its direction is the real one, so the lit hemisphere below
                    // is the hemisphere in daylight now. A directional light points along
                    // its own forward (-Z); the euler here turns that forward onto the
                    // vector from the Earth toward the sun, so the surface that faces the
                    // sun is the surface that is lit.
                    DirectionalLight {
                        id: sunLight
                        property vector3d toSun: root.sunDirection
                        eulerRotation: Qt.vector3d(
                            Math.asin(Math.max(-1, Math.min(1, toSun.y))) * 180 / Math.PI,
                            Math.atan2(toSun.x, toSun.z) * 180 / Math.PI,
                            0)
                        brightness: 2.3
                        color: "#fff4e6"
                        castsShadow: false
                    }

                    // The sky. A large sphere drawn from the inside, procedural stars on a
                    // near-black ground, so the planet sits in space rather than on a flat
                    // fill. Off under memory pressure and in the Orb scene, where
                    // the reference's black stage keeps the data paths legible.
                    Model {
                        source: "#Sphere"
                        scale: Qt.vector3d(7, 7, 7)
                        visible: root.scene !== "orb" && !root.budgetReduced()
                        materials: CustomMaterial {
                            shadingMode: CustomMaterial.Unshaded
                            cullMode: Material.NoCulling
                            vertexShader: "qrc:/archie/shaders/star.vert"
                            fragmentShader: "qrc:/archie/shaders/star.frag"
                        }
                    }

                    Node {
                        id: worldRoot
                        visible: root.scene === "world"
                        // Earth-fixed coordinates: the camera and every geographic
                        // marker use the same frame. UTC must not rotate it afterward.

                        Model {
                            visible: !root.localTerrainPreview || root.streamedWorld
                            source: "#Sphere"
                            // Geographic placement matches this mesh: Greenwich +Z,
                            // 90 degrees east +X, north +Y. Do not rotate the texture.
                            scale: Qt.vector3d(0.3999996, 0.3999996, 0.3999996)   // radius 20, the model's GlobePolicy default
                            materials: PrincipledMaterial {
                                baseColorMap: Texture {
                                    id: earthTexture
                                    source: archieBridge.frame.tier === "conserve"
                                            ? "qrc:/archie/assets/earth-blue-marble.png"
                                            : "qrc:/archie/assets/earth-bmng-5400.jpg"
                                    generateMipmaps: true
                                    mipFilter: Texture.Linear
                                }
                                // Earthshine, not city lights. The dark side keeps the
                                // faintest trace of its own texture so the planet has form
                                // in the night — the way the new moon's dark limb is dimly
                                // lit by light bounced off the Earth. It is deliberately
                                // almost nothing: inventing bright cities here, from a
                                // daytime texture that does not know where they are, would
                                // be exactly the fake photorealism this must not add. Real
                                // night lights are a real dataset (NASA Black Marble) and
                                // arrive as their own layer, not as a trick played on the
                                // day map.
                                emissiveMap: earthTexture
                                emissiveFactor: Qt.vector3d(0.02, 0.022, 0.028)
                                // The ocean is not a blackboard. A little specular gives the
                                // sea a moving highlight under the sun and, at the
                                // terminator, the thin gold line that says this is water.
                                specularAmount: 0.35
                                roughness: 0.75
                                metalness: 0.0
                            }
                        }
                        Model {
                            source: "#Sphere"
                            scale: Qt.vector3d(0.407,0.407,0.407)
                            visible: !root.budgetReduced() && !root.localTerrainPreview
                            materials: CustomMaterial {
                                shadingMode: CustomMaterial.Unshaded
                                sourceBlend: CustomMaterial.SrcAlpha
                                destinationBlend: CustomMaterial.OneMinusSrcAlpha
                                // Handed to the shader as a uniform: the same real sun the
                                // light uses, so the warm dusk band on the limb lands
                                // exactly where the terminator crosses the surface.
                                property vector3d sunDirection: root.sunDirection
                                vertexShader: "qrc:/archie/shaders/atmosphere.vert"
                                fragmentShader: "qrc:/archie/shaders/atmosphere.frag"
                            }
                        }

                        // Camera-driven quadtree patches share Earth's exact frame.
                        // Each patch has a local origin so street vertices retain precision.
                        Repeater3D {
                            id:earthRepeater
                            model:globeStream.earthModel
                            delegate:Model {
                                pickable:true
                                property bool photogrammetryMesh:true
                                required property var meshData
                                position:meshData.origin
                                geometry:MeshGeometry {vertices:meshData.vertices;triangleIndices:meshData.triangleIndices;hasUv:true}
                                materials:WorldImageryMaterial {textureSource:meshData.textureSource;terrainColor:Shell.Theme.t3}
                            }
                        }
                        Repeater3D {
                            model:globeStream.tileModel
                            delegate:Model {
                                pickable:true
                                property bool rasterTile:true
                                required property var meshData
                                position:meshData.origin
                                geometry:MeshGeometry {vertices:meshData.vertices;triangleIndices:meshData.triangleIndices;hasUv:true}
                                materials:WorldImageryMaterial {textureSource:meshData.textureSource;terrainColor:Shell.Theme.t3}
                            }
                        }
                        // Safe decoded preview only: bounded samples/footprints, never local artifact paths.
                        Repeater3D {
                            model:archieBridge.explorationMeshes
                            delegate:Model {
                                required property var modelData
                                visible:modelData.kind!=="terrain" || !root.texturedTerrain
                                geometry:MeshGeometry { vertices:modelData.vertices; triangleIndices:modelData.triangleIndices }
                                materials:PrincipledMaterial {
                                    baseColor:modelData.kind==="terrain" ? Shell.Theme.t3 : modelData.kind==="buildings" ? Shell.Theme.t2 : Shell.Theme.acc
                                    lighting:modelData.kind==="terrain" ? PrincipledMaterial.FragmentLighting : PrincipledMaterial.NoLighting
                                    cullMode:Material.NoCulling
                                    roughness:1.0
                                }
                            }
                        }

                        Repeater3D {
                            model:archieBridge.imageryMeshes
                            delegate:Model {
                                required property var modelData
                                visible:!root.streamedWorld && modelData.source_id===archieBridge.selectedImagery
                                geometry:MeshGeometry {vertices:modelData.vertices;triangleIndices:modelData.triangleIndices;hasUv:true}
                                materials:modelData.kind==="terrain" ? uncoveredMaterial : imageryMaterial
                                PrincipledMaterial {id:uncoveredMaterial;baseColor:Shell.Theme.t3;roughness:1.0;cullMode:Material.NoCulling}
                                WorldImageryMaterial {id:imageryMaterial;textureSource:modelData.textureSource || "";terrainColor:Shell.Theme.t3}
                            }
                        }

                        // Real building footprints -- extruded and placed on the globe by
                        // archie::visual from an actual .osm.pbf extract, when
                        // ArchieBridge.buildings.available says one was found. Empty by
                        // default: no sample or placeholder building stands in for real
                        // map data that has not been configured (see loadBuildings).
                        //
                        // Gated to the same "street" band GeoFabric's own selection names
                        // for this LOD -- not an optimisation guess, but the honest fact
                        // that a building is genuinely smaller than a pixel against the
                        // whole planet at any wider framing, the same real number the
                        // GEOFABRIC sidebar and viewport caption already agree on.
                        Repeater3D {
                            model: archieBridge.buildingMeshes
                            delegate: Model {
                                required property var modelData
                                visible: root.nearestBand(root.currentLod()) === "street"
                                geometry: MeshGeometry {
                                    vertices: modelData.vertices
                                    triangleIndices: modelData.triangleIndices
                                }
                                materials: PrincipledMaterial {
                                    baseColor: "#8a94a3"
                                    roughness: 0.85
                                    metalness: 0.0
                                }
                            }
                        }
                    }

                    // The orb: the TFOG graph, composed and drawn the Lumenfield way.
                    LumenScene {
                        id: lumenScene
                        controller: lumen
                        reducedMotion: !archieBridge.frame.ambientMotion
                        presented: archieBridge.presented && root.scene === "orb"
                        visible: root.scene === "orb"
                        aspect: view.width / Math.max(1, view.height)
                        flash: root.eventFlash
                    }
                }

                SequentialAnimation {
                    id: realEventPulse
                    NumberAnimation { target: root; property: "eventFlash"; to: 1.0; duration: 130; easing.type: Easing.OutCubic }
                    NumberAnimation { target: root; property: "eventFlash"; to: 0.0; duration: 340; easing.type: Easing.OutCubic }
                }
                Connections {
                    target: archieBridge
                    function onSnapshotRevisionChanged() { realEventPulse.restart() }
                    // kernelRevision moves only when a verdict changes, never when a
                    // pressure number merely drifts — so this pulse marks the machine
                    // actually changing condition, not the clock advancing.
                    function onKernelRevisionChanged() { realEventPulse.restart() }
                }
                // A second, separate connection for the sun. Kept apart from the pulse
                // above rather than folded into it, so the packaging guard can keep
                // matching the exact literal handler body it was written against instead
                // of a string that happens to still contain the same two calls.
                Connections {
                    target: archieBridge
                    function onSnapshotRevisionChanged() { root.computeSun() }
                }

                MouseArea {
                    // The globe's hands. The orb has its own (OrbHud) and FORGE is a list.
                    enabled: root.scene === "world"
                    acceptedButtons: Qt.LeftButton | Qt.RightButton
                    anchors.fill: parent
                    property point previous
                    property point origin
                    property bool moved: false
                    onPressed: function(mouse) { previous=Qt.point(mouse.x,mouse.y);origin=previous;moved=false }
                    onPositionChanged: function(mouse) {
                        if (!pressed || root.scene !== "world") return
                        if (Math.abs(mouse.x-origin.x)+Math.abs(mouse.y-origin.y)>5) moved=true
                        if (moved) {
                            root.selection=null
                            worldHud.cedeCamera()
                            if (pressedButtons & Qt.RightButton) archieBridge.tiltGlobe((mouse.y-previous.y)*0.3)
                            else archieBridge.orbitGlobe(-(mouse.x-previous.x)*0.2,(mouse.y-previous.y)*0.2,0)
                        }
                        previous=Qt.point(mouse.x,mouse.y)
                    }
                    onWheel: function(wheel) {
                        if (root.scene === "world") { worldHud.cedeCamera();root.selection=null;archieBridge.orbitGlobe(0,0,wheel.angleDelta.y/120);wheel.accepted=true }
                    }
                    // Picking tells the model, not just the panel. The model owns the
                    // dive onto a node and the way back out, and it lets go of a node the
                    // graph has forgotten — which a selection held only here cannot do,
                    // because it does not know the node is gone.
                    onClicked: function(mouse) {
                        if (moved || mouse.button!==Qt.LeftButton) return
                        worldHud.cedeCamera()
                        const result = view.pick(mouse.x, mouse.y)
                        const hit = result.objectHit
                        root.selection = hit && hit.payload ? hit.payload : null
                        if (hit && hit.nodeId)
                            archieBridge.focusNode(hit.nodeId)
                        else if (!(hit && (hit.photogrammetryMesh===true || hit.rasterTile===true)))
                            archieBridge.releaseFocus()
                    }
                }

                OrbHud {
                    controller: lumen
                    anchors.fill: parent
                    visible: root.scene === "orb"
                    labelsVisible: root.orbHudVisible && lumen.settings.labels
                    view: view
                    orbScene: lumenScene
                }

                ForgeView {
                    anchors.fill: parent
                    visible: root.scene === "forge"
                }

                WorldBeacons {
                    id:worldBeacons;anchors.fill:parent;visible:root.scene === "world"
                    controller:archieBridge;view:view;selectedId:worldHud.selectedPointId
                    points:globeStream.worldPoints.concat((archieBridge.worldPoints || []).filter(function(point){return point.kind==="aoi"}))
                    onChosen:function(point) {
                        worldHud.cedeCamera();worldHud.selectedPointId=point.id;root.worldHudVisible=true;root.selection=null
                        if(point.kind==="aoi" && point.job && Exploration.validCamera(point.job.camera) && archieBridge.selectExplorationJob(point.job.id)) {
                            const c=point.job.camera;const elevation=Exploration.cameraElevation(point.job)
                            archieBridge.frameExploration(c.latitude,c.longitude,c.radius_m,elevation===null?0:elevation,Shell.Theme.spatial)
                        }
                    }
                }
                Rectangle {
                    visible:root.scene === "world";anchors.left:parent.left;anchors.right:parent.right;anchors.bottom:parent.bottom;anchors.margins:16
                    height:provenance.implicitHeight+20;radius:Shell.Theme.rCard;color:Shell.Theme.panel;border.color:Shell.Theme.hair
                    Column {
                        id:provenance;anchors.left:parent.left;anchors.right:parent.right;anchors.margins:12;anchors.verticalCenter:parent.verticalCenter;spacing:4
                        Text {textFormat:Text.PlainText;width:parent.width;text:worldHud.imageryCaption;color:Shell.Theme.t1;font.family:Shell.Theme.font;font.pixelSize:11;wrapMode:Text.Wrap}
                        Text {width:parent.width;text:"Arrastrar · órbita   Rueda · escala   Puntos · datos   Esc · volver  ·  Balizas: ayuda visual, no tamaño físico";color:Shell.Theme.t3;font.family:Shell.Theme.font;font.pixelSize:10;wrapMode:Text.Wrap}
                    }
                }

                Rectangle {
                    // Fades rather than snapping, because a reticle that simply
                    // appears reads as a glitch and one that eases in reads as
                    // acquiring a lock. Tied only to `root.selection`, which the model
                    // sets from a real pick or a real focus release — this plays once
                    // per real change and never loops.
                    visible: opacity > 0.01
                    opacity: root.selection !== null ? 1 : 0
                    Behavior on opacity { NumberAnimation { duration: 160; easing.type: Easing.OutCubic } }
                    anchors.right: parent.right; anchors.rightMargin: 18
                    anchors.bottom: parent.bottom; anchors.bottomMargin: 18
                    width: 330; height: detailColumn.implicitHeight + 24
                    color: "#ec080d14"
                    CornerBrackets { legLength: 16; tint: root.cyan }
                    Column {
                        id: detailColumn
                        anchors.left: parent.left; anchors.right: parent.right; anchors.margins: 12
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 5
                        Text { width: parent.width; text: root.selection ? (root.selection.summary || root.selection.label || root.selection.id) : ""; color: root.ink; wrapMode: Text.Wrap; font.pixelSize: 13; font.bold: true }
                        Text {
                            width: parent.width
                            text: root.selection && root.selection.details
                                  ? "M" + Number(root.selection.details.magnitude).toFixed(1) + " · " + root.selection.details.severity
                                  : root.selection ? (root.selection.detail || "") : ""
                            color: root.soft; wrapMode: Text.Wrap; font.pixelSize: 10
                        }
                        Text {
                            width: parent.width
                            text: root.selection && root.selection.provenance
                                  ? root.selection.provenance.authority + " · " + root.selection.provenance.source_record_id
                                  : ""
                            visible: text.length > 0
                            color: root.dim; wrapMode: Text.Wrap; font.pixelSize: 9
                        }
                    }
                }

                Rectangle {
                    visible: archieBridge.error.length > 0
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.bottom: parent.bottom; anchors.bottomMargin: 18
                    width: Math.min(errorText.implicitWidth + 26, parent.width - 50); height: 34; radius: 4
                    color: "#33171319"; border.color: root.red
                    Text { id: errorText; anchors.centerIn: parent; text: archieBridge.error; color: root.red; font.pixelSize: 10; elide: Text.ElideRight; width: parent.width - 20 }
                }
            }

            Rectangle {
                visible: false
                Layout.preferredWidth: 338
                Layout.fillHeight: true
                color: "#0a0d13"
                CornerBrackets { legLength: 14; tint: root.frame }
                ScrollView {
                    anchors.fill: parent
                    anchors.margins: 18
                    clip: true
                    Column {
                        width: parent.width
                        spacing: 12
                        SectionTitle { text: "MODULES · " + root.modules.length }
                        Repeater {
                            model: root.modules
                            delegate: Rectangle {
                                required property var modelData
                                width: parent.width; height: moduleText.implicitHeight + 16; radius: 4
                                color: "#10141b"; border.color: root.stateColor(modelData.state)
                                RowLayout {
                                    anchors.fill: parent; anchors.margins: 8; spacing: 9
                                    Rectangle { width: 7; height: 7; radius: 4; color: root.stateColor(modelData.state) }
                                    ColumnLayout {
                                        Layout.fillWidth: true; spacing: 1
                                        Text { id: moduleText; text: modelData.id; color: root.ink; font.pixelSize: 11 }
                                        Text { text: modelData.explanation || modelData.family || ""; color: root.dim; font.pixelSize: 9; visible: text.length > 0; wrapMode: Text.Wrap; Layout.fillWidth: true }
                                    }
                                    Text { text: String(modelData.state || "").toUpperCase(); color: root.stateColor(modelData.state); font.pixelSize: 8; font.letterSpacing: 1 }
                                }
                            }
                        }
                        SectionTitle { text: "MODELS" }
                        Text {
                            width: parent.width
                            text: snapshot.models && snapshot.models.length ? snapshot.models.length + " registered" : "no model providers registered"
                            color: root.dim; font.pixelSize: 10; wrapMode: Text.Wrap
                        }
                        SectionTitle { text: "CONTROL BOUNDARY" }
                        Text {
                            width: parent.width
                            text: "Proyecciones de lectura y cámara con sesión limitada del propietario. La voz y Core conservan la autoridad; el daemon obtiene las fuentes."
                            color: root.dim; font.pixelSize: 10; wrapMode: Text.Wrap
                        }

                        Column {
                            visible: root.scene === "world"
                            width: parent.width
                            spacing: 12

                        // Provider catalog context. Actual camera-driven selection,
                        // downloads and coverage are reported by the streaming HUD.
                        SectionTitle { text: "GEOFABRIC · PROVIDER SELECTION" }
                        Text {
                            width: parent.width
                            text: "Cobertura y fuentes del catálogo GeoFabric. El globo solicita el detalle según la cámara."
                            color: root.dim; font.pixelSize: 9; wrapMode: Text.Wrap
                        }
                        Repeater {
                            model: archieBridge.geoFabric.selection || []
                            delegate: Column {
                                required property var modelData
                                // True for the one band nearest the camera's actual
                                // position right now, in the world scene — the same
                                // real value the caption under the globe prints, so
                                // the sidebar and the viewport never name two
                                // different answers.
                                readonly property bool active: root.scene === "world"
                                                               && modelData.band === root.nearestBand(root.currentLod())
                                width: parent ? parent.width : implicitWidth
                                spacing: 1
                                Text {
                                    text: (parent.active ? "▸ " : "  ") + String(modelData.band).toUpperCase() + " · lod " + modelData.lod
                                    color: parent.active ? root.cyan : root.soft
                                    font.pixelSize: 9; font.letterSpacing: 1
                                    font.bold: parent.active
                                }
                                Text {
                                    width: parent.width
                                    text: "terrain " + (modelData.terrain || "—")
                                          + "  ·  buildings " + (modelData.buildings || "—")
                                          + "  ·  surface " + (modelData.photoreal || "—")
                                    color: root.ink; font.pixelSize: 9; wrapMode: Text.Wrap
                                }
                            }
                        }
                        Text {
                            width: parent.width
                            readonly property var g: archieBridge.geoFabric
                            text: (g.enabled ? g.enabled.length : 0) + " providers enabled"
                                  + (g.free_only === true ? " · nothing here bills" : "")
                            color: root.dim; font.pixelSize: 9
                        }
                        // Absent is not the same as unexplained. Google Photorealistic
                        // is in this list, not simply never mentioned — the master
                        // directive is explicit that it may exist only as an optional
                        // display provider, and a panel that just never shows it leaves
                        // an operator guessing whether that was a choice.
                        Text {
                            visible: (archieBridge.geoFabric.disabled || []).length > 0
                            width: parent.width
                            text: (archieBridge.geoFabric.disabled || []).length
                                  + " more known, off by default:"
                            color: root.dim; font.pixelSize: 9
                        }
                        Repeater {
                            model: archieBridge.geoFabric.disabled || []
                            delegate: Text {
                                required property var modelData
                                width: parent ? parent.width : implicitWidth
                                // A billed provider's reason is a fact about the world
                                // an operator cannot change from here. A missing
                                // account is the opposite — the reason names exactly
                                // the switch to flip — so it reads as an instruction,
                                // not a wall, and the two must not look the same.
                                readonly property bool needsAccount: modelData.cost_class === "free_with_account"
                                text: needsAccount
                                      ? "· " + modelData.authority + " — set " + modelData.reason + " to enable"
                                      : "· " + modelData.authority + " — " + modelData.reason
                                color: needsAccount ? root.soft : root.dim
                                font.pixelSize: 9
                                wrapMode: Text.Wrap
                            }
                        }

                        // Real footprints extruded from an actual .osm.pbf extract, when
                        // one is configured (ARCHIE_OSM_EXTRACT) — not a placeholder or a
                        // sample building. The default, honest state is "not configured":
                        // no demo geometry stands in for real map data anywhere in this
                        // panel or the viewport.
                        SectionTitle { text: "BUILDINGS · OSM EXTRACT" }
                        Text {
                            width: parent.width
                            readonly property var b: archieBridge.buildings
                            // Parsing runs off the GUI thread (see loadBuildings): a real
                            // extract's tens of thousands of ways are real work, and
                            // "loading" is what a frame that has not frozen looks like
                            // while that runs, not a state to hide.
                            text: b.loading === true
                                  ? "parsing extract…"
                                  : (b.available === true
                                     ? b.count + " of " + (b.totalFound !== undefined ? b.totalFound : b.count) + " footprints drawn"
                                     : (b.reason || "no extract configured"))
                            color: b.loading === true ? root.soft : (b.available === true ? root.green : root.dim)
                            font.pixelSize: 9; wrapMode: Text.Wrap
                        }
                        Text {
                            visible: archieBridge.buildings.loading !== true
                                     && archieBridge.buildings.available === true
                                     && !!archieBridge.buildings.reason
                            width: parent.width
                            text: archieBridge.buildings.reason || ""
                            color: root.soft; font.pixelSize: 9; wrapMode: Text.Wrap
                        }
                        Text {
                            visible: archieBridge.buildings.loading !== true
                                     && archieBridge.buildings.available === true
                            width: parent.width
                            text: "visible only at street scale — genuinely smaller than a pixel from further out"
                            color: root.dim; font.pixelSize: 9; wrapMode: Text.Wrap
                        }
                        }
                    }
                }
            }
        }

        Rectangle {
            visible: false
            Layout.fillWidth: true
            Layout.preferredHeight: 112
            color: "#0a0d13"
            CornerBrackets { legLength: 12; tint: root.frame }
            ColumnLayout {
                anchors.fill: parent; anchors.margins: 14; spacing: 8
                SectionTitle { text: "BOOT SEQUENCE · REAL LAST REPORT" }
                RowLayout {
                    Layout.fillWidth: true; Layout.fillHeight: true; spacing: 6
                    Repeater {
                        model: root.stages
                        delegate: Rectangle {
                            required property var modelData
                            Layout.fillWidth: true; Layout.fillHeight: true
                            radius: 3; color: "#10141b"; border.color: root.stateColor(modelData.status.status)
                            Column {
                                anchors.fill: parent; anchors.margins: 8; spacing: 3
                                Text { width: parent.width; text: modelData.name; color: root.soft; font.pixelSize: 9; elide: Text.ElideRight }
                                Text { text: String(modelData.status.status).toUpperCase(); color: root.stateColor(modelData.status.status); font.pixelSize: 9; font.bold: true }
                                Text { width: parent.width; text: modelData.status.detail || ""; color: root.dim; font.pixelSize: 8; elide: Text.ElideRight; visible: text.length > 0 }
                            }
                        }
                    }
                }
            }
        }
    }

    // Four L-shaped corner marks instead of a border that encloses the whole panel.
    // A fully enclosed rectangle reads as a box; an open corner reads as a frame
    // around something being observed — the difference between a dashboard and a
    // HUD, and the single chrome change this file makes to get there. Purely
    // geometric: it carries no state, invents nothing, and never animates on its
    // own — the moment a corner mark pulses without a real reason behind it, it
    // stops being restraint and starts being decoration.
    component CornerBrackets: Item {
        id: brackets
        // Kept for the layout it anchors, drawn no more: the frame read as a HUD.
        visible: false
        property real legLength: 14
        property real thickness: 1
        property color tint: root.edge
        property real inset: 0
        anchors.fill: parent

        Rectangle { anchors.left: parent.left; anchors.top: parent.top; anchors.margins: brackets.inset; width: brackets.legLength; height: brackets.thickness; color: brackets.tint }
        Rectangle { anchors.left: parent.left; anchors.top: parent.top; anchors.margins: brackets.inset; width: brackets.thickness; height: brackets.legLength; color: brackets.tint }

        Rectangle { anchors.right: parent.right; anchors.top: parent.top; anchors.margins: brackets.inset; width: brackets.legLength; height: brackets.thickness; color: brackets.tint }
        Rectangle { anchors.right: parent.right; anchors.top: parent.top; anchors.margins: brackets.inset; width: brackets.thickness; height: brackets.legLength; color: brackets.tint }

        Rectangle { anchors.left: parent.left; anchors.bottom: parent.bottom; anchors.margins: brackets.inset; width: brackets.legLength; height: brackets.thickness; color: brackets.tint }
        Rectangle { anchors.left: parent.left; anchors.bottom: parent.bottom; anchors.margins: brackets.inset; width: brackets.thickness; height: brackets.legLength; color: brackets.tint }

        Rectangle { anchors.right: parent.right; anchors.bottom: parent.bottom; anchors.margins: brackets.inset; width: brackets.legLength; height: brackets.thickness; color: brackets.tint }
        Rectangle { anchors.right: parent.right; anchors.bottom: parent.bottom; anchors.margins: brackets.inset; width: brackets.thickness; height: brackets.legLength; color: brackets.tint }
    }

    component SectionTitle: Text {
        width: parent ? parent.width : implicitWidth
        color: root.dim
        font.pixelSize: 9
        font.letterSpacing: 2
        font.bold: true
    }

    component Fact: RowLayout {
        property string label: ""
        property string value: ""
        property color valueColor: root.ink
        width: parent ? parent.width : implicitWidth
        spacing: 8
        Text { text: parent.label; color: root.soft; font.pixelSize: 10; Layout.fillWidth: true; elide: Text.ElideRight }
        Text { text: parent.value; color: parent.valueColor; font.pixelSize: 10; font.bold: true }
    }

    // A bar that refuses to draw when there is no scale to draw it against.
    //
    // `fraction` is null when the firmware declares no critical trip point for a zone.
    // Rendering that as an empty bar would read as "cold"; rendering it as a full bar
    // would read as "on fire". Both are inventions, so it renders as neither.
    component Meter: Item {
        property real fraction: 0
        property bool known: true
        property color fill: root.cyan
        width: parent ? parent.width : 120
        height: 4
        Rectangle {
            anchors.fill: parent
            radius: 2
            color: "#131c26"
        }
        Rectangle {
            visible: parent.known
            height: parent.height
            radius: 2
            width: parent.width * Math.max(0, Math.min(1, parent.fraction))
            color: parent.fill
            // Real values move; the bar follows them at a speed a human can read rather
            // than snapping. The animation carries a measurement, it does not invent one.
            Behavior on width { NumberAnimation { duration: 260; easing.type: Easing.OutCubic } }
        }
        // No scale, no bar. A dashed rule says "not measured" and cannot be mistaken for
        // a reading of zero.
        Row {
            visible: !parent.known
            anchors.verticalCenter: parent.verticalCenter
            spacing: 3
            Repeater {
                model: 14
                Rectangle { width: 5; height: 1; color: root.edge }
            }
        }
    }

    component SceneButton: Button {
        property string label: ""
        property bool selected: false
        width: 84; height: 30
        text: label
        hoverEnabled: true
        background: Rectangle {
            radius: 15
            color: parent.selected ? "#e9edf3" : (parent.hovered ? "#1a1f28" : "transparent")
            Behavior on color { ColorAnimation { duration: 160 } }
        }
        contentItem: Text {
            text: parent.text
            color: parent.selected ? "#0b0e14" : root.soft
            font.pixelSize: 13; font.weight: parent.selected ? Font.DemiBold : Font.Normal
            horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter
        }
    }
}
