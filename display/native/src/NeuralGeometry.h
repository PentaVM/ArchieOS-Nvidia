#pragma once

// What the neural sphere's materials draw from, in the layouts the original's buffers had:
//   Points    node dots: position, colour (the normal channel), kind+fire (UV0), ring+born (UV1)
//   Lines     one layer of link segments: position and a colour per vertex (the normal channel)
//   Sparks    the travelling signals: position, colour, fade (UV0.x)
//   Ghosts    the 1600 particles on the surface; Segments the globe, base and orbit; Cone the beam
// Every colour is already linear. Nothing here decides a shape: `NeuralField` (Rust) does.

#include <QPointer>
#include <QQuick3DGeometry>
#include <QtQml/qqmlregistration.h>

class NeuralField;

class NeuralGeometry : public QQuick3DGeometry
{
    Q_OBJECT
    QML_ELEMENT
    Q_PROPERTY(NeuralField *field READ field WRITE setField NOTIFY fieldChanged)
    Q_PROPERTY(Kind kind READ kind WRITE setKind NOTIFY kindChanged)
    /// Lines: the link layer (0 and 2..7). Segments: 1 globe, 2 base, 3 orbit.
    Q_PROPERTY(int layer READ layer WRITE setLayer NOTIFY kindChanged)

public:
    enum Kind { Points, Lines, Sparks, Ghosts, Segments, Cone };
    Q_ENUM(Kind)

    explicit NeuralGeometry(QQuick3DObject *parent = nullptr);

    NeuralField *field() const { return m_field; }
    void setField(NeuralField *field);
    Kind kind() const { return m_kind; }
    void setKind(Kind kind);
    int layer() const { return m_layer; }
    void setLayer(int layer);

signals:
    void fieldChanged();
    void kindChanged();

private:
    void refresh();
    void commit(const QByteArray &data, int stride, PrimitiveType type, int attributes);

    QPointer<NeuralField> m_field;
    Kind m_kind = Points;
    int m_layer = 0;
    qulonglong m_revision = ~0ULL;
};
