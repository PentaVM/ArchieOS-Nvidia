#include "MeshGeometry.h"
#include <QVector3D>
#include <cstring>
#include <algorithm>

MeshGeometry::MeshGeometry(QQuick3DGeometry *parent) : QQuick3DGeometry(parent)
{
}

void MeshGeometry::setHasUv(bool value) {if(m_hasUv==value)return;m_hasUv=value;rebuild();emit meshChanged();}

void MeshGeometry::setVertices(const QByteArray &data)
{
    if (m_vertices == data)
        return;
    m_vertices = data;
    rebuild();
    emit meshChanged();
}

void MeshGeometry::setTriangleIndices(const QByteArray &data)
{
    if (m_triangleIndices == data)
        return;
    m_triangleIndices = data;
    rebuild();
    emit meshChanged();
}

void MeshGeometry::rebuild()
{
    clear();
    if (m_vertices.isEmpty() || m_triangleIndices.isEmpty()) {
        // Nothing real to draw -- an empty footprint, or one still waiting on data --
        // is a cleared geometry, not a shape made of zero-length garbage.
        update();
        return;
    }

    const int kStride = (m_hasUv?8:6) * static_cast<int>(sizeof(float));  // position.xyz + normal.xyz
    if(m_vertices.size()%kStride!=0){update();return;}
    float xyz[3];std::memcpy(xyz,m_vertices.constData(),sizeof(xyz));QVector3D minimum(xyz[0],xyz[1],xyz[2]),maximum=minimum;
    for(qsizetype offset=0;offset<m_vertices.size();offset+=kStride){std::memcpy(xyz,m_vertices.constData()+offset,sizeof(xyz));minimum.setX(std::min(minimum.x(),xyz[0]));minimum.setY(std::min(minimum.y(),xyz[1]));minimum.setZ(std::min(minimum.z(),xyz[2]));maximum.setX(std::max(maximum.x(),xyz[0]));maximum.setY(std::max(maximum.y(),xyz[1]));maximum.setZ(std::max(maximum.z(),xyz[2]));}
    setBounds(minimum,maximum);
    setStride(kStride);
    setVertexData(m_vertices);
    addAttribute(QQuick3DGeometry::Attribute::PositionSemantic, 0,
                 QQuick3DGeometry::Attribute::F32Type);
    addAttribute(QQuick3DGeometry::Attribute::NormalSemantic, 3 * static_cast<int>(sizeof(float)),
                 QQuick3DGeometry::Attribute::F32Type);

    if(m_hasUv)addAttribute(QQuick3DGeometry::Attribute::TexCoord0Semantic,6*static_cast<int>(sizeof(float)),QQuick3DGeometry::Attribute::F32Type);

    setIndexData(m_triangleIndices);
    addAttribute(QQuick3DGeometry::Attribute::IndexSemantic, 0,
                 QQuick3DGeometry::Attribute::U32Type);

    setPrimitiveType(QQuick3DGeometry::PrimitiveType::Triangles);
    update();
}
