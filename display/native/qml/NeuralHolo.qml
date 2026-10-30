import QtQuick

// The neural sphere's hologram pass over the finished 3D frame (scene.ts HOLO_FRAG): a
// small red/blue split, scan lines, a slow bright band and a vignette. Used as the layer
// effect of the View3D that draws the sphere.
ShaderEffect {
    required property var field
    property real pixelRatio: 1
    property bool lightMode: false
    property real lightTheme: lightMode ? 1 : 0
    property real time: Number(field.time)
    property real strength: Number(field.settings.holo === undefined ? 1 : field.settings.holo)
    property size resolution: Qt.size(width * pixelRatio, height * pixelRatio)
    fragmentShader: "qrc:/archie/shaders/neural_holo.frag.qsb"
}
