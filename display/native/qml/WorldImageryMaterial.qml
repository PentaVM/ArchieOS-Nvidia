import QtQuick
import QtQuick3D

// Inline imagery stays in the bounded memory provider. Texture.source itself
// accepts only file/qrc URLs, so the Image is the provider-aware loading boundary.
PrincipledMaterial {
    id: material
    required property url textureSource
    property color terrainColor: "#808080"
    lighting: PrincipledMaterial.NoLighting
    baseColorMap: Texture {
        // Missing pixels retain neutral terrain on this same measured surface.
        sourceItem: Rectangle {
            width: tile.implicitWidth
            height: tile.implicitHeight
            color: material.terrainColor
            Image {id:tile;anchors.fill:parent;source:material.textureSource}
        }
        // GeoFabric uses north at image row/v=0; Quick3D's mesh convention has
        // v=0 at the bottom. Correct this once for every georeferenced tile.
        flipV: true
        generateMipmaps: true
        mipFilter: Texture.Linear
        minFilter: Texture.Linear
        magFilter: Texture.Linear
        tilingModeHorizontal: Texture.ClampToEdge
        tilingModeVertical: Texture.ClampToEdge
    }
    roughness: 1.0
    cullMode: Material.NoCulling
}
