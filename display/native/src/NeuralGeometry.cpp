#include "NeuralGeometry.h"

#include "NeuralField.h"

#include <algorithm>
#include <vector>

namespace {
// Frustum culling reads these; points move and the field is a few hundred units across.
constexpr float kBound = 4000.0F;

std::vector<float> decor(unsigned part)
{
    const std::size_t count = neural_decor(part, nullptr, 0);
    std::vector<float> out(count);
    if (count && neural_decor(part, out.data(), count) != count)
        out.clear();
    return out;
}
} // namespace

NeuralGeometry::NeuralGeometry(QQuick3DObject *parent)
    : QQuick3DGeometry(parent)
{
    setBounds(QVector3D(-kBound, -kBound, -kBound), QVector3D(kBound, kBound, kBound));
}

void NeuralGeometry::setField(NeuralField *field)
{
    if (field == m_field)
        return;
    if (m_field)
        disconnect(m_field, nullptr, this, nullptr);
    m_field = field;
    if (m_field)
        connect(m_field, &NeuralField::updated, this, &NeuralGeometry::refresh);
    m_revision = ~0ULL;
    emit fieldChanged();
    refresh();
}

void NeuralGeometry::setKind(Kind kind)
{
    if (kind == m_kind)
        return;
    m_kind = kind;
    m_revision = ~0ULL;
    emit kindChanged();
    refresh();
}

void NeuralGeometry::setLayer(int layer)
{
    if (layer == m_layer)
        return;
    m_layer = layer;
    m_revision = ~0ULL;
    emit kindChanged();
    refresh();
}

void NeuralGeometry::commit(const QByteArray &data, int stride, PrimitiveType type, int attributes)
{
    clear();
    setStride(stride);
    setPrimitiveType(type);
    // The channels in order; each is a run of floats, and Quick3D knows how wide a semantic is.
    addAttribute(Attribute::PositionSemantic, 0, Attribute::F32Type);
    if (attributes >= 2)
        addAttribute(Attribute::NormalSemantic, 3 * 4, Attribute::F32Type);
    if (attributes >= 3)
        addAttribute(Attribute::TexCoord0Semantic, 6 * 4, Attribute::F32Type);
    if (attributes >= 4)
        addAttribute(Attribute::TexCoord1Semantic, 8 * 4, Attribute::F32Type);
    setVertexData(data);
    setBounds(QVector3D(-kBound, -kBound, -kBound), QVector3D(kBound, kBound, kBound));
    update();
}

void NeuralGeometry::refresh()
{
    // Static decoration is uploaded once; the live buffers upload when the sphere says so.
    const bool live = m_kind == Points || m_kind == Lines || m_kind == Sparks;
    if (live && !m_field)
        return;
    const qulonglong revision = m_field ? m_field->revision() : 0;
    if ((m_kind == Points || m_kind == Lines) && revision == m_revision)
        return;
    if (!live && m_revision != ~0ULL)
        return;
    m_revision = revision;

    switch (m_kind) {
    case Points: {
        // position3, colour3, kind, fire, ring, born: the original's attributes, 10 floats a vertex.
        std::vector<NeuralPoint> points = m_field->points();
        if (points.empty())
            points.push_back(NeuralPoint{}); // born = 0: size 0, alpha 0
        static_assert(sizeof(NeuralPoint) == 10 * sizeof(float));
        commit(QByteArray(reinterpret_cast<const char *>(points.data()), static_cast<qsizetype>(points.size() * sizeof(NeuralPoint))),
               static_cast<int>(sizeof(NeuralPoint)), PrimitiveType::Points, 4);
        break;
    }
    case Lines: {
        std::vector<NeuralVertex> lines = m_field->lines(m_layer);
        if (lines.size() < 2)
            lines.assign(2, NeuralVertex{}); // a degenerate segment draws nothing
        static_assert(sizeof(NeuralVertex) == 6 * sizeof(float));
        commit(QByteArray(reinterpret_cast<const char *>(lines.data()), static_cast<qsizetype>(lines.size() * sizeof(NeuralVertex))),
               static_cast<int>(sizeof(NeuralVertex)), PrimitiveType::Lines, 2);
        break;
    }
    case Sparks: {
        // position3, colour3, fade, 0: UV0 is two floats wide.
        const std::vector<NeuralSpark> sparks = m_field->sparks();
        std::vector<float> floats;
        floats.reserve(std::max<std::size_t>(sparks.size(), 1) * 8);
        for (const NeuralSpark &s : sparks)
            floats.insert(floats.end(), {s.position[0], s.position[1], s.position[2], s.colour[0], s.colour[1], s.colour[2], s.fade, 0.0F});
        if (floats.empty())
            floats.assign(8, 0.0F);
        commit(QByteArray(reinterpret_cast<const char *>(floats.data()), static_cast<qsizetype>(floats.size() * sizeof(float))), 8 * 4,
               PrimitiveType::Points, 3);
        break;
    }
    case Ghosts:
    case Segments: {
        const std::vector<float> xyz = decor(m_kind == Ghosts ? 0U : static_cast<unsigned>(std::clamp(m_layer, 1, 3)));
        if (xyz.empty())
            return;
        commit(QByteArray(reinterpret_cast<const char *>(xyz.data()), static_cast<qsizetype>(xyz.size() * sizeof(float))), 3 * 4,
               m_kind == Ghosts ? PrimitiveType::Points : PrimitiveType::Lines, 1);
        break;
    }
    case Cone: {
        // xyz and the cylinder's v: the shader reads v as UV0.y, so the vertex is position3 + (0, v).
        const std::vector<float> in = decor(4);
        std::vector<float> floats;
        floats.reserve(in.size() / 4 * 5);
        for (std::size_t i = 0; i + 3 < in.size(); i += 4)
            floats.insert(floats.end(), {in[i], in[i + 1], in[i + 2], 0.0F, in[i + 3]});
        if (floats.empty())
            return;
        // Position and UV0 with nothing between: UV0 sits at float 3 here, so it is declared by hand.
        clear();
        setStride(5 * 4);
        setPrimitiveType(PrimitiveType::Triangles);
        addAttribute(Attribute::PositionSemantic, 0, Attribute::F32Type);
        addAttribute(Attribute::TexCoord0Semantic, 3 * 4, Attribute::F32Type);
        setVertexData(QByteArray(reinterpret_cast<const char *>(floats.data()), static_cast<qsizetype>(floats.size() * sizeof(float))));
        setBounds(QVector3D(-kBound, -kBound, -kBound), QVector3D(kBound, kBound, kBound));
        update();
        break;
    }
    }
}
