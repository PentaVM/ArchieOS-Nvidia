import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "qrc:/archie/shell/prototype" as Shell
import "Exploration.js" as Exploration
import "../../shell/prototype/Lang.js" as Tr

// Xavi (ByXavi270 / XaviCottier): shell design; PentaVM: native implementation.
// A reader of daemon projections. There are deliberately no job mutation controls.
Shell.Panel {
    id: hud
    required property var controller
    property bool worldActive: false
    property var stream: null
    property var seenJobs: []
    property bool seededJobs: false
    property bool jobsReady: false
    property string cameraOwner: Tr.t("Vista libre")
    property string cameraJob: ""
    property bool cameraAutomatic: false
    property var elevationPlans: []
    property bool inspectorOpen: false
    property string selectedPointId: ""
    readonly property var selectedPoint:((stream ? stream.worldPoints || [] : []).concat(controller.worldPoints || [])).find(function(point){return point.id===hud.selectedPointId}) || null
    readonly property var imageSources:stream ? stream.sources : controller.imagerySources || []
    readonly property var signalSources:stream ? stream.status.signal_sources || [] : controller.signalSources || []
    readonly property var activeImage:imageSources.find(function(source){return source.source_id===(hud.stream ? hud.stream.sourceId : hud.controller.selectedImagery)}) || null
    readonly property string imageryCaption:activeImage ? (activeImage.label || activeImage.source_id)+" · "+(activeImage.attribution || Tr.t("Atribución no publicada"))+Tr.t(" · Fecha: ")+(activeImage.imagery_date || Tr.t("no publicada"))+Tr.t(" · Recuperada: ")+(Exploration.known(activeImage.retrieved_at_ms)?new Date(activeImage.retrieved_at_ms).toLocaleString():Tr.t("sin lectura"))+Tr.t(" · Estado: ")+(activeImage.state || Tr.t("sin lectura"))+(activeImage.mode==="online_preview_only"?Tr.t(" · Vista experimental en línea, no dato abierto"):Tr.t(" · Compuesto fechado, no imagen en vivo")) : Tr.t("NASA Blue Marble · contexto global incluido · sin teselas de imagen disponibles")
    readonly property var pointProperties:selectedPoint && selectedPoint.properties ? Object.keys(selectedPoint.properties).slice(0,24).map(function(key){return {key:key,value:typeof hud.selectedPoint.properties[key]==="object"?JSON.stringify(hud.selectedPoint.properties[key]):String(hud.selectedPoint.properties[key])}}) : []
    readonly property var projection: controller.exploration || ({source:"unavailable"})
    readonly property var jobs: projection.jobs || []
    readonly property var selectedJob:Exploration.selectedJob(hud.projection)
    readonly property var providers: (projection.providers || []).filter(function(provider){return !hud.stream || provider.requests_sent>0 || provider.in_flight>0 || provider.id===(hud.stream.sourceId==="google_satellite"?"google_satellite":"nasa_gibs")})
    readonly property var snapshot: controller.snapshot || ({})
    readonly property var status: snapshot.status || ({})
    readonly property var kernel: controller.kernel || ({})
    readonly property var metricRows: Exploration.metrics(snapshot)
    readonly property var inference: Exploration.inference(snapshot)
    function cedeCamera() { cameraAutomatic=false;cameraOwner=Tr.t("Cámara manual") }
    function applyCamera(job) {
        if(!Exploration.validCamera(job.camera)) return false
        var elevation=Exploration.cameraElevation(job)
        // Zero here is an explicit provisional reference datum, never a measured elevation.
        if(!controller.frameExploration(job.camera.latitude,job.camera.longitude,job.camera.radius_m,
                                        elevation===null ? 0 : elevation,Shell.Theme.spatial)) return false
        if(elevation!==null && elevationPlans.indexOf(job.id)<0) elevationPlans.push(job.id)
        cameraAutomatic=true;cameraOwner=elevation===null ? Tr.t("AOI · datum provisional") : Tr.t("AOI · elevación de la fuente");cameraJob=job.id
        return true
    }
    // Seed saved history without moving the planet. A new admitted voice/CLI
    // exploration retains the existing cinematic handoff; ordinary polls do not.
    function observeJobs() {
        if(!jobsReady || !controller) return
        var current=controller.exploration || {}
        if(current.source!=="live") return
        var fresh=Exploration.newJobs(current.jobs || [],seenJobs)
        if(!seededJobs) {seededJobs=true;return}
        if(worldActive && !controller.selectedExplorationJob && fresh.length) applyCamera(fresh[fresh.length-1])
        else if(worldActive && cameraAutomatic && elevationPlans.indexOf(cameraJob)<0) {
            var currentJob=(current.jobs || []).find(function(job){return job.id===hud.cameraJob})
            if(currentJob && Exploration.cameraElevation(currentJob)!==null) applyCamera(currentJob)
        }
    }
    Component.onCompleted: {jobsReady=true;observeJobs()}
    onWorldActiveChanged: observeJobs()
    Connections {target:hud.controller;function onExplorationChanged(){hud.observeJobs()}}
    ScrollView {
        anchors.fill:parent; anchors.margins:16
        clip:true; contentWidth:availableWidth
        ColumnLayout {
            width:parent.width; spacing:12
            RowLayout {
                Layout.fillWidth:true
                Text {textFormat:Text.PlainText; text:"World"; font.family:Shell.Theme.disp; font.pixelSize:20; font.weight:Font.DemiBold; color:Shell.Theme.t1; Layout.fillWidth:true }
                Shell.SourceChip { projection:hud.projection; small:true }
            }
            Label {textFormat:Text.PlainText; text:Tr.t("Exploración · GeoFabric / Aleph"); color:Shell.Theme.t2; font.family:Shell.Theme.font }
            Label {textFormat:Text.PlainText; Layout.fillWidth:true; wrapMode:Text.Wrap; text:Tr.t("Arrastra para recorrer el planeta. La rueda acerca hasta el suelo; arrastra con el botón derecho para inclinar la cámara. Las superficies 3D siguen la cámara donde existe cobertura."); color:Shell.Theme.t3; font.family:Shell.Theme.font; font.pixelSize:11 }
            Info { label:Tr.t("Cámara"); value:hud.cameraOwner }
            Info {visible:!!hud.stream;label:Tr.t("Globo continuo");value:hud.stream ? hud.stream.status.state || Tr.t("Conectando") : ""}
            Info {visible:!!hud.stream;label:Tr.t("Detalle deseado / listo");value:hud.stream ? String(hud.stream.status.desired_count || 0)+" / "+String(hud.stream.status.loaded_count || 0) : ""}
            Info {visible:!!hud.stream;label:Tr.t("Base / cola");value:hud.stream ? String(hud.stream.status.fallback_count || 0)+" / "+String(hud.stream.status.pending_count || 0) : ""}
            Info {visible:!!hud.stream;label:"LOD visible";value:hud.stream ? String(hud.stream.status.min_zoom || 0)+" / "+String(hud.stream.status.max_zoom || 0) : ""}
            Info {visible:!!hud.stream && hud.stream.sourceId==="google_satellite";label:Tr.t("Fotogrametría 3D");value:hud.stream ? (hud.stream.status.earth || {}).state || Tr.t("Esperando cobertura 3D") : ""}
            Info {visible:!!hud.stream && hud.stream.sourceId==="google_satellite";label:Tr.t("Mallas / triángulos");value:hud.stream ? String(hud.stream.status.earth_loaded_count || 0)+" / "+String(hud.stream.status.earth_triangle_count || 0) : ""}
            Label {visible:!!hud.stream && !!(hud.stream.status.earth || {}).error;textFormat:Text.PlainText;Layout.fillWidth:true;wrapMode:Text.Wrap;text:hud.stream ? (hud.stream.status.earth || {}).error || "" : "";color:Shell.Theme.warn;font.family:Shell.Theme.font;font.pixelSize:11}
            Label {visible:!!hud.stream && hud.stream.sourceId==="google_satellite";text:Tr.t("Botón derecho + arrastrar: inclinar la vista 3D");Layout.fillWidth:true;wrapMode:Text.Wrap;color:Shell.Theme.t3;font.family:Shell.Theme.font;font.pixelSize:10}
            Label {visible:!!hud.stream && hud.stream.sourceId==="google_satellite";textFormat:Text.PlainText;Layout.fillWidth:true;wrapMode:Text.Wrap;text:hud.stream ? (hud.stream.status.earth || {}).attribution || Tr.t("Google · vista experimental en memoria · sin captura ni exportación") : "";color:Shell.Theme.t3;font.family:Shell.Theme.font;font.pixelSize:10}
            Info {visible:!!hud.stream;label:Tr.t("HTTP de sesión");value:hud.stream ? Exploration.count((hud.stream.status.usage || {}).requests_sent)+" / "+Exploration.count((hud.stream.status.usage || {}).max_requests) : ""}
            Info {visible:!!hud.stream;label:Tr.t("Descarga / presupuesto");value:hud.stream ? Exploration.bytes((hud.stream.status.usage || {}).bytes_received)+" / "+Exploration.bytes((hud.stream.status.usage || {}).max_bytes) : ""}
            Label {visible:!!hud.stream && !!hud.stream.status.error;textFormat:Text.PlainText;Layout.fillWidth:true;wrapMode:Text.Wrap;text:hud.stream ? hud.stream.status.error || "" : "";color:Shell.Theme.warn;font.family:Shell.Theme.font;font.pixelSize:11}

            Info {label:Tr.t("Captura consultada");value:hud.selectedJob ? hud.selectedJob.id : Tr.t("Captura seleccionada no disponible")}
            Label {textFormat:Text.PlainText;Layout.fillWidth:true;wrapMode:Text.Wrap;text:Tr.t("NASA Blue Marble mantiene el contexto y los polos mientras llega el detalle.");color:Shell.Theme.t3;font.family:Shell.Theme.font;font.pixelSize:10}
            Button {
                id:latestArea;visible:!!hud.controller.selectedExplorationJob;Layout.fillWidth:true;text:Tr.t("Última área");font.family:Shell.Theme.font;font.pixelSize:12
                contentItem:Text {textFormat:Text.PlainText;text:latestArea.text;font:latestArea.font;color:Shell.Theme.t1;horizontalAlignment:Text.AlignHCenter;verticalAlignment:Text.AlignVCenter}
                background:Rectangle {implicitHeight:34;color:latestArea.down?Shell.Theme.pressed:latestArea.hovered?Shell.Theme.hover:Shell.Theme.raised;radius:Shell.Theme.rCard;border.color:Shell.Theme.hair}
                onClicked:{hud.selectedPointId="";hud.controller.latestExplorationJob();if(hud.jobs.length)hud.applyCamera(hud.jobs[hud.jobs.length-1])}
            }
            Info { label:Tr.t("Última proyección"); value:Exploration.known(hud.projection.generated_at_ms) ? new Date(hud.projection.generated_at_ms).toLocaleTimeString() : Tr.t("Sin lectura") }
            Label {textFormat:Text.PlainText; visible:!!hud.projection.error; text:hud.projection.error || ""; color:Shell.Theme.bad; font.family:Shell.Theme.font; wrapMode:Text.Wrap; Layout.fillWidth:true }
            Label {textFormat:Text.PlainText; visible:hud.jobs.length===0; text:hud.projection.source==="unavailable" ? Tr.t("Servicio de exploración sin lectura") : Tr.t("Sin exploraciones publicadas"); color:Shell.Theme.t3; font.family:Shell.Theme.font; wrapMode:Text.Wrap; Layout.fillWidth:true }
            Text {textFormat:Text.PlainText;text:Tr.t("Capas de imagen");color:Shell.Theme.t1;font.family:Shell.Theme.disp;font.pixelSize:14}
            ComboBox {
                id:imageryPicker
                indicator: Shell.Icon { name:"chevD";size:16;color:Shell.Theme.t1;x:imageryPicker.width-width-10;y:(imageryPicker.height-height)/2 }
                Layout.fillWidth:true;model:hud.imageSources;textRole:"label";valueRole:"source_id"
                enabled:hud.imageSources.length>0
                font.family:Shell.Theme.font;font.pixelSize:12
                palette.window:Shell.Theme.panel;palette.base:Shell.Theme.raised;palette.button:Shell.Theme.hover
                palette.text:Shell.Theme.t1;palette.buttonText:Shell.Theme.t1;palette.windowText:Shell.Theme.t1
                palette.highlight:Shell.Theme.sel;palette.highlightedText:Shell.Theme.t1
                contentItem:Text {textFormat:Text.PlainText;text:imageryPicker.displayText || Tr.t("Aún sin imágenes");color:imageryPicker.enabled?Shell.Theme.t1:Shell.Theme.t3;font:imageryPicker.font;verticalAlignment:Text.AlignVCenter;leftPadding:10;rightPadding:30;elide:Text.ElideRight}
                background:Rectangle {color:Shell.Theme.hover;radius:Shell.Theme.rCard;border.color:Shell.Theme.hair;implicitHeight:34}
                currentIndex:hud.imageSources.findIndex(function(source){return source.source_id===(hud.stream ? hud.stream.sourceId : hud.controller.selectedImagery)})
                onActivated: { if(hud.stream) hud.stream.sourceId=currentValue;else hud.controller.selectedImagery=currentValue }
            }
            Repeater {
                model:hud.stream ? (hud.activeImage ? [hud.activeImage] : []) : hud.imageSources
                delegate:ColumnLayout {
                    id:imageSource;required property var modelData;Layout.fillWidth:true;spacing:3
                    Info {label:imageSource.modelData.label || imageSource.modelData.source_id;value:imageSource.modelData.state || Tr.t("Sin lectura")}
                    Text {textFormat:Text.PlainText;Layout.fillWidth:true;wrapMode:Text.Wrap;text:(imageSource.modelData.imagery_date || Tr.t("Sin fecha de imagen"))+" · "+(imageSource.modelData.mode==="online_preview_only"?Tr.t("Vista experimental en línea"):Tr.t("Datos abiertos"))+(imageSource.modelData.requested_date?Tr.t(" · solicitada ")+imageSource.modelData.requested_date:"");font.family:Shell.Theme.font;font.pixelSize:10;color:Shell.Theme.t3}
                    Info {label:hud.stream ? Tr.t("Detalle cargado (aprox.)") : Tr.t("Resolución fuente / vista");value:hud.stream ? (Exploration.known(imageSource.modelData.preview_resolution_m)?"~"+Number(imageSource.modelData.preview_resolution_m).toFixed(1)+Tr.t(" m/píxel"):Tr.t("Pendiente")) : (Exploration.known(imageSource.modelData.source_resolution_m)?Number(imageSource.modelData.source_resolution_m).toFixed(1)+" m":Tr.t("sin lectura"))+" / "+(Exploration.known(imageSource.modelData.preview_resolution_m)?Number(imageSource.modelData.preview_resolution_m).toFixed(1)+" m":Tr.t("sin lectura"))}
                    Text {textFormat:Text.PlainText;visible:!!text;Layout.fillWidth:true;wrapMode:Text.Wrap;text:imageSource.modelData.error || imageSource.modelData.renderer_error || "";font.family:Shell.Theme.font;font.pixelSize:10;color:Shell.Theme.warn}
                }
            }
            Text {textFormat:Text.PlainText;text:Tr.t("Señales observadas");color:Shell.Theme.t1;font.family:Shell.Theme.disp;font.pixelSize:14}
            Repeater {model:hud.signalSources;delegate:Info {id:signalSource;required property var modelData;label:signalSource.modelData.label || signalSource.modelData.id;value:Exploration.signalSummary(signalSource.modelData)}}
            Info {
                visible:hud.stream ? !!hud.stream.status.signals : !!hud.selectedJob && !!hud.selectedJob.preview && !!hud.selectedJob.preview.signals
                label:Tr.t("Cobertura de señales")
                value:Exploration.signalSummary(hud.stream ? hud.stream.status.signals || {} : hud.selectedJob && hud.selectedJob.preview && hud.selectedJob.preview.signals || {})
            }
            Label {textFormat:Text.PlainText;visible:hud.signalSources.length===0;text:Tr.t("No se ha publicado ninguna fuente de señales para esta área.");Layout.fillWidth:true;wrapMode:Text.Wrap;font.family:Shell.Theme.font;color:Shell.Theme.t3;font.pixelSize:11}
            Label {objectName:"selectedPointExpired";textFormat:Text.PlainText;visible:!!hud.selectedPointId && !hud.selectedPoint;text:Tr.t("Punto seleccionado sin lectura en esta captura.");Layout.fillWidth:true;wrapMode:Text.Wrap;font.family:Shell.Theme.font;color:Shell.Theme.warn;font.pixelSize:11}
            ColumnLayout {
                visible:!!hud.selectedPoint;Layout.fillWidth:true;spacing:6
                Text {textFormat:Text.PlainText;Layout.fillWidth:true;wrapMode:Text.Wrap;text:hud.selectedPoint ? hud.selectedPoint.label || hud.selectedPoint.id : "";font.family:Shell.Theme.disp;color:Shell.Theme.t1;font.pixelSize:15}
                Info {label:Tr.t("Fuente");value:hud.selectedPoint ? hud.selectedPoint.source_id || Tr.t("Sin lectura") : ""}
                Info {label:Tr.t("Observado");value:hud.selectedPoint && Exploration.known(hud.selectedPoint.observed_at_ms)?new Date(hud.selectedPoint.observed_at_ms).toLocaleString():Tr.t("Sin hora publicada")}
                Info {label:Tr.t("Altitud publicada");value:hud.selectedPoint && hud.selectedPoint.altitudeKnown?hud.selectedPoint.altitude_m+" m":Tr.t("Sin lectura")}
                Info {label:Tr.t("Anclaje visual");value:hud.selectedPoint ? hud.selectedPoint.anchor_mode || Tr.t("Área solicitada") : ""}
                Text {textFormat:Text.PlainText;Layout.fillWidth:true;wrapMode:Text.Wrap;text:hud.selectedPoint ? hud.selectedPoint.source_url || "" : "";font.family:Shell.Theme.font;color:Shell.Theme.t3;font.pixelSize:10}
                Repeater {model:hud.pointProperties;delegate:Info {id:pointProperty;required property var modelData;label:pointProperty.modelData.key;value:pointProperty.modelData.value}}
            }
            Repeater {
                model:hud.jobs
                delegate:Rectangle {
                    id:jobCard
                    required property var modelData
                    Layout.fillWidth:true; implicitHeight:jobColumn.implicitHeight+20
                    radius:Shell.Theme.rCard; color:Shell.Theme.raised; border.color:Shell.Theme.hair
                    ColumnLayout {
                        id:jobColumn; anchors.fill:parent; anchors.margins:10; spacing:7
                        RowLayout {
                            Layout.fillWidth:true
                            Shell.Chip { text:jobCard.modelData.domain==="work" ? Tr.t("Trabajo") : jobCard.modelData.domain==="personal" ? "Personal" : Tr.t("Sin dominio"); domain:jobCard.modelData.domain || ""; small:true }
                            Shell.Chip { text:Exploration.state(jobCard.modelData.state); tone:Exploration.tone(jobCard.modelData.state); small:true }
                        }
                        Text {textFormat:Text.PlainText; Layout.fillWidth:true; text:jobCard.modelData.id || Tr.t("Sin ID"); elide:Text.ElideMiddle; font.family:Shell.Theme.font; font.pixelSize:11; color:Shell.Theme.t2 }
                        Text {textFormat:Text.PlainText; Layout.fillWidth:true; text:Exploration.validCamera(jobCard.modelData.camera) ? Number(jobCard.modelData.camera.latitude).toFixed(5)+", "+Number(jobCard.modelData.camera.longitude).toFixed(5)+" · "+jobCard.modelData.camera.radius_m+" m" : Tr.t("AOI sin lectura"); font.family:Shell.Theme.font; font.pixelSize:11; color:Shell.Theme.t3; wrapMode:Text.Wrap }
                        Info { label:Tr.t("Teselas"); value:Exploration.count(jobCard.modelData.tiles_done)+" / "+Exploration.count(jobCard.modelData.tiles_total) }
                        Info { label:Tr.t("Recibido / caché"); value:Exploration.bytes(jobCard.modelData.bytes_received)+" / "+Exploration.count(jobCard.modelData.cache_hits) }
                        Info { label:Tr.t("Capas solicitadas"); value:jobCard.modelData.request && jobCard.modelData.request.layers ? jobCard.modelData.request.layers.join(", ") : Tr.t("Sin lectura") }
                        Info { label:Tr.t("Elevación de cámara"); value:Exploration.cameraElevation(jobCard.modelData)===null ? Tr.t("Sin lectura · datum provisional") : Exploration.cameraElevation(jobCard.modelData)+Tr.t(" m · fuente") }
                        Info { label:Tr.t("Muestras de terreno"); value:Exploration.elevationRange(jobCard.modelData) }
                        Info { label:Tr.t("Artefactos en esta vista"); value:String((jobCard.modelData.artifacts || []).length) }
                        Label {textFormat:Text.PlainText; objectName:"projectionTruncationNotice"; visible:jobCard.modelData.projection_truncated===true; text:Tr.t("Proyección resumida: el plan y los artefactos están limitados en esta vista; los contadores del trabajo conservan sus totales."); color:Shell.Theme.warn; Layout.fillWidth:true; wrapMode:Text.Wrap; font.family:Shell.Theme.font; font.pixelSize:10 }
                        Label {textFormat:Text.PlainText; objectName:"omittedBuildingsNotice"; visible:!!jobCard.modelData.preview && Exploration.known(jobCard.modelData.preview.omitted_buildings) && jobCard.modelData.preview.omitted_buildings>0; text:jobCard.modelData.preview ? Tr.t("Edificios omitidos de la vista previa: ")+Exploration.count(jobCard.modelData.preview.omitted_buildings) : ""; color:Shell.Theme.warn; Layout.fillWidth:true; wrapMode:Text.Wrap; font.family:Shell.Theme.font; font.pixelSize:10 }
                        Text {textFormat:Text.PlainText; Layout.fillWidth:true; text:!jobCard.modelData.preview ? Tr.t("Vista de datos pendiente") : Tr.t("Terreno: muestras reales; edificios: contorno si falta altura. Datum base: terreno cuando existe; referencia del mar si falta. En vista local el terreno reemplaza el mapa global y el océano, conservando elevaciones negativas. Las imágenes conservan fuente, fecha y atribución."); color:Shell.Theme.t3; font.family:Shell.Theme.font; font.pixelSize:10; wrapMode:Text.Wrap }
                        Text {textFormat:Text.PlainText; visible:!!jobCard.modelData.error; Layout.fillWidth:true; text:jobCard.modelData.error || ""; font.family:Shell.Theme.font; font.pixelSize:11; color:Shell.Theme.bad; wrapMode:Text.Wrap }
                    }
                }
            }
            Text {textFormat:Text.PlainText; text:Tr.t("Proveedores y cuotas"); color:Shell.Theme.t1; font.family:Shell.Theme.disp; font.pixelSize:14 }
            Repeater {
                model:hud.providers
                delegate:ColumnLayout {
                    id:providerBlock
                    required property var modelData
                    Layout.fillWidth:true; spacing:5
                    Info { label:providerBlock.modelData.id || Tr.t("Sin proveedor"); value:providerBlock.modelData.state || Tr.t("Sin lectura") }
                    Info { label:Tr.t("Solicitudes / recibido"); value:Exploration.count(providerBlock.modelData.requests_sent)+" / "+Exploration.bytes(providerBlock.modelData.bytes_received) }
                    Info { label:Tr.t("Límite por minuto"); value:Exploration.count(providerBlock.modelData.requests_per_minute) }
                    Info { label:Tr.t("En vuelo / máximo"); value:Exploration.count(providerBlock.modelData.in_flight)+" / "+Exploration.count(providerBlock.modelData.max_in_flight) }
                    Info { label:Tr.t("Cuota restante"); value:Exploration.count(providerBlock.modelData.remaining_requests)+" · "+(providerBlock.modelData.quota_source || "unknown") }
                    Info { label:Tr.t("Reintento"); value:Exploration.known(providerBlock.modelData.retry_at_ms) ? new Date(providerBlock.modelData.retry_at_ms).toLocaleTimeString() : Tr.t("Sin espera publicada") }
                }
            }
            Button {
                id:inspectorButton;text:hud.inspectorOpen ? Tr.t("Cerrar inspector del sistema") : Tr.t("Inspector del sistema");Layout.fillWidth:true;font.family:Shell.Theme.font;font.pixelSize:12
                contentItem:Text {textFormat:Text.PlainText;text:inspectorButton.text;font:inspectorButton.font;color:Shell.Theme.t1;horizontalAlignment:Text.AlignHCenter;verticalAlignment:Text.AlignVCenter}
                background:Rectangle {implicitHeight:34;color:inspectorButton.down?Shell.Theme.pressed:inspectorButton.hovered?Shell.Theme.hover:Shell.Theme.raised;radius:Shell.Theme.rCard;border.color:Shell.Theme.hair}
                onClicked:hud.inspectorOpen=!hud.inspectorOpen
            }
            ColumnLayout {
                visible:hud.inspectorOpen; Layout.fillWidth:true; spacing:7
                Shell.SourceChip { projection:hud.snapshot; small:true }
                Info { label:"Core"; value:hud.snapshot.daemon_reachable===true ? Tr.t("Accesible") : Tr.t("Sin respuesta") }
                Info { label:"CPU"; value:hud.status.cpu_model || Tr.t("Sin lectura") }
                Info { label:Tr.t("Aceleradores"); value:hud.status.accelerators ? hud.status.accelerators.join(", ") || Tr.t("Ninguno publicado") : Tr.t("Sin lectura") }
                Info { label:Tr.t("Uso de GPU"); value:Tr.t("Sin fuente medida publicada") }
                Info { label:Tr.t("RAM disponible"); value:Exploration.bytes(hud.status.memory_available_bytes) }
                Info { label:"RAM total"; value:Exploration.bytes(hud.status.memory_total_bytes) }
                Info { label:Tr.t("Presión"); value:hud.kernel.pressure_availability && hud.kernel.pressure_availability.state==="available" ? (hud.kernel.verdict || Tr.t("Sin lectura")) : Tr.t("Sin lectura") }
                Info { label:Tr.t("Térmico"); value:hud.kernel.thermal_availability && hud.kernel.thermal_availability.state==="available" ? (hud.kernel.thermal_state || Tr.t("Sin lectura")) : Tr.t("Sin lectura") }
                Info { label:Tr.t("Nodos / modelos"); value:String((hud.snapshot.nodes || []).length)+" / "+String((hud.snapshot.models || []).length) }
                Repeater {
                    model:hud.snapshot.models || []
                    delegate:Info { id:modelRow; required property var modelData; label:modelRow.modelData.id || Tr.t("Sin ID"); value:(modelRow.modelData.provider || Tr.t("Sin proveedor"))+" · "+(modelRow.modelData.availability || Tr.t("Sin lectura")) }
                }
                Repeater {
                    model:hud.snapshot.nodes || []
                    delegate:Info { id:nodeRow; required property var modelData; label:nodeRow.modelData.hostname || nodeRow.modelData.node_id || Tr.t("Sin nodo"); value:(nodeRow.modelData.health || Tr.t("Sin lectura"))+" · "+(nodeRow.modelData.reachable===true ? Tr.t("Accesible") : Tr.t("Sin respuesta")) }
                }
                Info { label:Tr.t("TPS efectivos · e2e"); value:hud.inference.tps }
                Info { label:Tr.t("Última inferencia"); value:hud.inference.observed }
                Info { label:Tr.t("Tokens de salida"); value:hud.inference.tokens }
                Info { label:Tr.t("Duración de solicitud"); value:hud.inference.elapsed }
                Text {textFormat:Text.PlainText; Layout.fillWidth:true; text:Tr.t("E2e incluye toda la solicitud; no mide solo la generación del modelo."); font.family:Shell.Theme.font; color:Shell.Theme.t3; font.pixelSize:10; wrapMode:Text.Wrap }
                Repeater { model:hud.metricRows; delegate:Info { id:metricRow; required property var modelData; label:metricRow.modelData.name+" ("+metricRow.modelData.kind+")"; value:metricRow.modelData.value } }
            }
            Text {textFormat:Text.PlainText; Layout.fillWidth:true; text:Tr.t("Diseño: Xavi · Implementación nativa: PentaVM"); color:Shell.Theme.t3; font.family:Shell.Theme.font; font.pixelSize:10; wrapMode:Text.Wrap }
        }
    }
    component Info: RowLayout {
        id:infoRow
        property string label
        property string value
        Layout.fillWidth:true; spacing:8
        Text {textFormat:Text.PlainText; text:infoRow.label; Layout.fillWidth:true; Layout.preferredWidth:1; wrapMode:Text.Wrap; color:Shell.Theme.t3; font.family:Shell.Theme.font; font.pixelSize:11 }
        Text {textFormat:Text.PlainText; text:infoRow.value; Layout.fillWidth:true; Layout.preferredWidth:1; horizontalAlignment:Text.AlignRight; wrapMode:Text.Wrap; color:Shell.Theme.t1; font.family:Shell.Theme.font; font.pixelSize:11 }
    }
}
