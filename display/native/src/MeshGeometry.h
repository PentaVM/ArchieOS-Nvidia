#pragma once

// A QQuick3DGeometry that draws whatever real vertex/normal/index buffers it is given --
// nothing here decides what those buffers mean. `archie::visual::Mesh` (a building's
// walls and roof today, any other mesh-producing model tomorrow) is the only thing that
// decides shape; this class's entire job is the one Quick3D-specific step
// `archie::visual` and `archie::osm` are deliberately kept clean of: describing a byte
// layout a GPU vertex buffer understands.
//
// One interleaved buffer (position, then normal, 24 bytes per vertex) rather than two
// separate ones: Quick3D's documented custom-geometry API is built around a single
// vertex buffer with attributes at fixed offsets inside one stride, and matching that
// exactly -- rather than reaching for a less-travelled multi-buffer path -- is worth more
// here than saving the one interleaving pass this class does on every mesh it is handed.

#include <QByteArray>
#include <QQuick3DGeometry>
#include <QtQml/qqmlregistration.h>

// Not `final`: Qt's QML_ELEMENT registration generates a small wrapper class that
// derives from this one, which the compiler rejects outright if it is sealed.
class MeshGeometry : public QQuick3DGeometry
{
    Q_OBJECT
    QML_ELEMENT
    // Pre-interleaved: 6 floats per vertex (position.xyz, normal.xyz), 24 bytes each.
    // Packing happens on the C++ side that already holds an `archie::visual::Mesh` --
    // ArchieBridge today -- because that is the one place that has both the mesh and a
    // reason to know Quick3D wants its buffers this way.
    Q_PROPERTY(bool hasUv READ hasUv WRITE setHasUv NOTIFY meshChanged)
    Q_PROPERTY(QByteArray vertices READ vertices WRITE setVertices NOTIFY meshChanged)
    // Raw uint32 triangle indices, three per triangle, matching
    // `archie::visual::TriangleIndices` byte-for-byte.
    Q_PROPERTY(QByteArray triangleIndices READ triangleIndices WRITE setTriangleIndices NOTIFY meshChanged)

public:
    explicit MeshGeometry(QQuick3DGeometry *parent = nullptr);

    [[nodiscard]] QByteArray vertices() const { return m_vertices; }
    [[nodiscard]] QByteArray triangleIndices() const { return m_triangleIndices; }

    bool hasUv() const {return m_hasUv;}
    void setHasUv(bool value);
    void setVertices(const QByteArray &data);
    void setTriangleIndices(const QByteArray &data);

signals:
    void meshChanged();

private:
    /// Re-describes the buffers to Quick3D. Called whenever either buffer changes; an
    /// empty vertex or index buffer clears the geometry rather than describing a shape
    /// with nothing in it.
    void rebuild();

    bool m_hasUv=false;
    QByteArray m_vertices;
    QByteArray m_triangleIndices;
};
