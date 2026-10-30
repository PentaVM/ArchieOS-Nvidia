import QtQuick
import "Lang.js" as Tr

AppWindow {
    id:sw; title:Tr.t("Ajustes")
    property var bridge:null
    property string st:Demo.state
    property int section:0
    readonly property bool neuralPalette: typeof lumen !== "undefined" && lumen.settings.fieldSource === "neural"
    readonly property var catalog:bridge && bridge.catalog ? bridge.catalog : null
    readonly property var inspection:bridge && bridge.inspector ? bridge.inspector : null
    readonly property var providers:inspection && inspection.providers ? inspection.providers : []
    readonly property var models:catalog && catalog.models ? catalog.models : []
    readonly property var modules:catalog && catalog.modules ? catalog.modules : []
    trailing:[SourceChip { bridge:sw.bridge; projection:sw.section===0 ? sw.inspection : sw.catalog }]
    sidebar:[
        SideRow { objectName:"settings.section.0"; icon:"key"; text:Tr.t("Proveedores"); on:sw.section===0; onClicked:sw.section=0 },
        SideRow { objectName:"settings.section.1"; icon:"cpu"; text:Tr.t("Modelos"); on:sw.section===1; onClicked:sw.section=1 },
        SideRow { objectName:"settings.section.2"; icon:"lock"; text:Tr.t("Autoridad"); on:sw.section===2; onClicked:sw.section=2 },
        SideRow { objectName:"settings.section.3"; icon:"activity"; text:Tr.t("Diagnóstico"); on:sw.section===3; onClicked:sw.section=3 },
        SideRow { objectName:"settings.section.4"; icon:"dkOrb"; text:Tr.t("Orbe nativo"); on:sw.section===4; onClicked:sw.section=4 },
        SideRow { objectName:"settings.section.5"; icon:"control"; text:Tr.t("Apariencia"); on:sw.section===5; onClicked:sw.section=5 }
    ]
    Column { visible:sw.section===0; width:parent.width; spacing:8
        Eyebrow { text:Tr.t("Proveedores declarados por Core") }
        VoidBox { visible:sw.providers.length===0; icon:"key"; title:Tr.t("Sin proveedores en la proyección")
            text:sw.inspection && (sw.inspection.providers_reason || sw.inspection.reason) ? (sw.inspection.providers_reason || sw.inspection.reason) : Tr.t("El visor no solicita ni muestra valores de claves.") }
        Repeater { model:sw.providers
            delegate:ListRow { glyph:"key"; title:modelData.title || modelData.id; sub:modelData.id + " · " + modelData.capability
                Chip { small:true; text:modelData.status; tone:modelData.status==="available" ? "neutral" : "unk" }
            }
        }
    }
    Column { id:localModels; visible:sw.section===1; width:parent.width; spacing:8
        readonly property bool host: typeof shellRuntime !== "undefined" && shellRuntime !== null
        readonly property var list: host ? shellRuntime.localModels : []
        onVisibleChanged: if (visible && host) shellRuntime.localModelsRefresh()
        Eyebrow { text:Tr.t("Modelo local que responde") }
        VoidBox { visible:localModels.list.length===0; icon:"cpu"; title:Tr.t("Sin modelos locales en la lista")
            text:!localModels.host ? Tr.t("Este host no puede hablar con archie-local.") : (shellRuntime.localModelNote || Tr.t("archie-local todavía no respondió.")) }
        Repeater { model:localModels.list
            delegate:ListRow { objectName:"settings.localModel." + modelData.name; glyph:"cpu"; title:modelData.label
                sub:modelData.answering ? Tr.t("Responde ahora en 127.0.0.1:") + modelData.port
                    : modelData.here ? (modelData.chosen ? Tr.t("Elegido; su servidor no está en marcha") : Tr.t("En este equipo"))
                    : Tr.t("No está en este equipo: archie-local setup ") + modelData.name
                Btn { objectName:"settings.useModel." + modelData.name
                    text:modelData.answering ? Tr.t("En uso") : shellRuntime.localModelBusy ? Tr.t("Cargando…") : Tr.t("Usar")
                    dis:!modelData.here || modelData.answering || shellRuntime.localModelBusy
                    onClicked:shellRuntime.useLocalModel(modelData.name) } }
        }
        NoteBox { visible:localModels.list.length>0; icon:"cpu"
            text:(localModels.host && shellRuntime.localModelNote ? shellRuntime.localModelNote + " " : "")
                + Tr.t("Uno responde a la vez: la GPU tiene lugar para uno. El 27B responde mejor y tarda unos minutos en cargar; el router de Core pregunta al que esté en marcha, sin reiniciar nada.") }
        Eyebrow { text:Tr.t("Modelos registrados") }
        VoidBox { visible:sw.models.length===0; icon:"cpu"; title:Tr.t("Sin modelos en la proyección"); text:Tr.t("No se deduce si hay un modelo cargado a partir de un catálogo vacío.") }
        Repeater { model:sw.models
            delegate:ListRow { glyph:"cpu"; title:modelData.id
                sub:(modelData.locality || Tr.t("localidad no declarada")) + Tr.t(" · privacidad ") + (modelData.max_privacy_grade || Tr.t("no declarada"))
                Chip { small:true; text:modelData.availability || Tr.t("sin lectura"); tone:"neutral" }
            }
        }
    }
    Column { id:authority; visible:sw.section===2; width:parent.width; spacing:8
        readonly property bool host: typeof shellRuntime !== "undefined" && shellRuntime !== null
        readonly property string state: host ? shellRuntime.aiPolicy : "unknown"
        onVisibleChanged: if (visible && host) shellRuntime.aiPolicyRefresh()
        Eyebrow { text:Tr.t("Pausa de emergencia de Core") }
        ListRow { glyph:"pause"; title:authority.state === "paused" ? Tr.t("La IA está en pausa") : authority.state === "running" ? Tr.t("La IA está en marcha") : Tr.t("Estado desconocido")
            sub:!authority.host ? Tr.t("Este host no puede hablar con archied.") : (shellRuntime.aiPolicyNote || Tr.t("Pausar rechaza toda inferencia y acción hasta reanudar; solo el dueño del nodo puede hacerlo."))
            Btn { objectName:"settings.emergency"; text:authority.state === "paused" ? Tr.t("Reanudar") : authority.state === "running" ? Tr.t("Pausar") : Tr.t("Reintentar")
                dis:!authority.host || shellRuntime.aiPolicyBusy
                onClicked:authority.state === "paused" ? shellRuntime.aiResume() : authority.state === "running" ? shellRuntime.aiPause() : shellRuntime.aiPolicyRefresh() } }
        NoteBox { icon:"lock"
            text:Tr.t("Core y Policy conservan la autoridad. La pausa es el único control de autoridad de esta pantalla; aprobar o revocar permisos sigue en archie (CLI) y en la voz.") }
    }
    Column { visible:sw.section===3; width:parent.width; spacing:8
        Eyebrow { text:Tr.t("Módulos y diagnóstico real") }
        Repeater { model:sw.modules
            delegate:ListRow { glyph:"activity"; title:modelData.id; sub:modelData.explanation
                Chip { small:true; text:modelData.state; tone:"neutral" }
            }
        }
        ResourceGauges { bridge:sw.bridge; width:parent.width }
    }
    Column { visible:sw.section===4; width:parent.width; spacing:8
        OrbOpt { label:Tr.t("Mostrar en escritorio"); options:[Tr.t("Sí"),"No"]; index:Demo.orbShow ? 0 : 1; onPicked:function(i) { Demo.orbShow=(i===0) } }
        OrbOpt { label:Tr.t("Representación"); options:Demo.orbProfiles; index:typeof lumen!=="undefined" ? Demo.orbProfiles.indexOf(lumen.profile) : Demo.orbProfile; onPicked:function(i) { Demo.orbProfile=i } }
        OrbOpt { label:Tr.t("Nodos visibles (máximo nativo)"); options:["60","120","220","360","420"]; index:Demo.orbNodesI; onPicked:function(i) { Demo.orbNodesI=i } }
        ListRow { visible:sw.neuralPalette; title:Tr.t("Color de la esfera"); sub:Tr.t("Paleta de categorías de archivo · diseño de Xavi") }
        OrbOpt { visible:!sw.neuralPalette; label:Tr.t("Color de la esfera"); options:["Original",Tr.t("Holograma frío"),Tr.t("Vivo")]; index:Demo.orbGradeI; onPicked:function(i) { Demo.orbGradeI=i } }
        OrbOpt { label:Tr.t("Polvo de luz"); options:["No",Tr.t("Sí")]; index:Demo.orbDust ? 1 : 0; onPicked:function(i) { Demo.orbDust=i===1 } }
        OrbOpt { label:Tr.t("Brillo"); options:[Tr.t("Bajo"),"Normal",Tr.t("Alto")]; index:Demo.orbBrightI; onPicked:function(i) { Demo.orbBrightI=i } }
        OrbOpt { label:Tr.t("Actividad visual"); options:[Tr.t("Apagada"),Tr.t("Baja"),Tr.t("Media"),Tr.t("Alta")]; index:Demo.orbActI; onPicked:function(i) { Demo.orbActI=i } }
        OrbOpt { label:Tr.t("Giro automático"); options:[Tr.t("Sí"),"No"]; index:Demo.orbSpin ? 0 : 1; onPicked:function(i) { Demo.orbSpin=(i===0) } }
        NoteBox { text:Tr.t("Renderer nativo Lumenfield, geometría y cinemáticas existentes. El presupuesto del OS y movimiento reducido limitan efectos; una opción visual no cambia modelos, inferencia ni autoridad.") }
    }
    Column { visible:sw.section===5; width:parent.width; spacing:8
        OrbOpt { objectName:"themeMode"; label:Tr.t("Tema"); options:[Tr.t("Oscuro"),Tr.t("Claro")]; index:Theme.light ? 1 : 0; onPicked:function(i) { Theme.colorMode=i===1 ? "light" : "dark" } }
        OrbOpt { label:Tr.t("Colores"); options:[Tr.t("Vívidos"),Tr.t("Originales")]; index:Theme.vivid ? 0 : 1; onPicked:function(i) { Theme.vivid=(i===0) } }
        OrbOpt { label:Tr.t("Perfil de movimiento"); options:[Tr.t("Completo"),Tr.t("Equilibrado"),Tr.t("Tranquilo")]; index:["full","balanced","quiet"].indexOf(Theme.motion); onPicked:function(i) { Theme.motion=["full","balanced","quiet"][i] } }
        OrbOpt { label:Tr.t("Superficies opacas"); options:["No",Tr.t("Sí")]; index:Theme.opaque ? 1 : 0; onPicked:function(i) { Theme.opaque=i===1 } }
        OrbOpt { label:Tr.t("Movimiento reducido"); options:[Tr.t("Sí"),"No"]; index:Theme.reducedMotion ? 0 : 1; onPicked:function(i) { Theme.reducedMotion=(i===0) } }
        NoteBox { text:Tr.t("Preferencias locales de presentación. No modifican la configuración de Core.") }
    }
}
