#include "LumenField.h"
#include "UiText.h"

#include "LumenGeometry.h"
#include "XaviDesignBridge.h"
#include "archie_lumen_design.h"

#include <QColor>
#include <QCoreApplication>
#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QThreadPool>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QQuick3DInstancing>
#include <QUrl>
#include <QVector4D>

#include <algorithm>
#include <cmath>
#include <cstring>
#include <limits>
#include <memory>
#include <optional>
#include <span>

namespace lumen = archie::visual::lumen;
using archie::visual::Vec3;

namespace {

constexpr auto lumenUrl = "http://127.0.0.1:7666/api/orb/lumen";
constexpr qsizetype maximumSceneBytes = 8 * 1024 * 1024;
constexpr int maximumSatellites = 2200;
constexpr int maximumPulses = 64;

struct Category {
    const char *key;
    const char *label;
    const char *colour;
};

// The design's seven types, for what an operating system holds instead of files: the
// same order and the same family of colours, one more for what is only structure.
constexpr Category categoryTable[] = {
    {"processes", "Procesos", "#73b9d5"},     {"services", "Servicios", "#e8c189"},
    {"devices", "Dispositivos", "#7fceb2"},   {"interface", "Interfaz", "#b8d08c"},
    {"cognition", "Cognición", "#b6a3cf"},    {"modules", "Módulos", "#dc9887"},
    {"storage", "Almacenamiento", "#9fb4d9"}, {"system", "Sistema", "#adbfbd"},
};

QString categoryOfKind(const QString &kind)
{
    if (kind == QLatin1String("process")) return QStringLiteral("processes");
    if (kind == QLatin1String("service")) return QStringLiteral("services");
    if (kind == QLatin1String("device")) return QStringLiteral("devices");
    if (kind == QLatin1String("app") || kind == QLatin1String("window") || kind == QLatin1String("control"))
        return QStringLiteral("interface");
    if (kind == QLatin1String("model") || kind == QLatin1String("memory") || kind == QLatin1String("tool"))
        return QStringLiteral("cognition");
    if (kind == QLatin1String("module") || kind == QLatin1String("library")) return QStringLiteral("modules");
    if (kind == QLatin1String("file") || kind == QLatin1String("directory")) return QStringLiteral("storage");
    return QStringLiteral("system");
}

// The content sphere's types are the design's own seven, with its colours (field.js and
// main.js of Xavi's source), for files rather than system entities.
constexpr Category contentCategoryTable[] = {
    {"code", "Código", "#73b9d5"},       {"documents", "Documentos", "#e8c189"},
    {"media", "Medios", "#7fceb2"},      {"data", "Datos", "#b8d08c"},
    {"archive", "Comprimidos", "#b6a3cf"}, {"executable", "Ejecutables", "#dc9887"},
    {"other", "Otros", "#bdc7bf"},
};

QString categoryLabel(const QString &key)
{
    // The keys are English words already: in English the label is the key, capitalised.
    if (uiEnglish() && !key.isEmpty())
        return key.left(1).toUpper() + key.mid(1);
    for (const auto &c : categoryTable)
        if (key == QLatin1String(c.key))
            return QString::fromUtf8(c.label);
    for (const auto &c : contentCategoryTable)
        if (key == QLatin1String(c.key))
            return QString::fromUtf8(c.label);
    return key;
}

// "neural" is Xavi's neural sphere (NeuralField): it lists its own folder, so this object
// neither asks displayd for a field nor lists one for it.
const QStringList fieldSources{QStringLiteral("tfog"), QStringLiteral("content"), QStringLiteral("neural")};

/// The folder the content sphere lists when nothing chose one: ARCHIE_CONTENT_ROOT, the
/// repository this display was built from (as Xavi's desktop showed ArchieOS itself),
/// the desktop's copy of it, or the home folder.
QString defaultContentRoot()
{
    if (const QString chosen = qEnvironmentVariable("ARCHIE_CONTENT_ROOT"); !chosen.isEmpty())
        return chosen;
    QDir dir(QCoreApplication::applicationDirPath());
    for (int up = 0; up < 6; ++up) {
        if (dir.exists(QStringLiteral("core/Cargo.toml")) && dir.exists(QStringLiteral("display/native")))
            return dir.absolutePath();
        if (!dir.cdUp())
            break;
    }
    for (const QString &candidate : {QStringLiteral("/srv/archieos"), QStringLiteral("/usr/share/archieos")})
        if (QFileInfo(candidate).isDir())
            return candidate;
    return QDir::homePath();
}

/// The design's defaults ("Regiones vivas"), with the density limit in this scene's
/// units: orbs, at most 420, rather than index entries.
QVariantMap defaults()
{
    QVariantMap s{
        {QStringLiteral("layoutMode"), QStringLiteral("regions")},
        {QStringLiteral("categoryPull"), 0.44}, {QStringLiteral("hierarchyPull"), 0.59},
        {QStringLiteral("radialLayers"), 0.63}, {QStringLiteral("sizeInfluence"), 0.09},
        {QStringLiteral("scale"), 1.0}, {QStringLiteral("nodeSize"), 1.75},
        {QStringLiteral("shellScale"), 1.0}, {QStringLiteral("shellOpacity"), 0.16},
        {QStringLiteral("ambientCount"), 600}, {QStringLiteral("ambientSize"), 0.7},
        {QStringLiteral("ghostOpacity"), 0.4}, {QStringLiteral("metalness"), 0.35},
        {QStringLiteral("roughness"), 0.45}, {QStringLiteral("emissive"), 0.7},
        {QStringLiteral("textureContrast"), 1.2}, {QStringLiteral("textureDepth"), 0.045},
        {QStringLiteral("linkOpacity"), 0.045}, {QStringLiteral("linkBudget"), 190},
        {QStringLiteral("highlightOpacity"), 0.72}, {QStringLiteral("highlightBudget"), 110},
        {QStringLiteral("reaction"), 145}, {QStringLiteral("reach"), 46},
        {QStringLiteral("spring"), 25}, {QStringLiteral("damping"), 0.85},
        {QStringLiteral("focusSpread"), 0.24}, {QStringLiteral("rotationSpeed"), 0.36},
        {QStringLiteral("pulseSpeed"), 0.33}, {QStringLiteral("exposure"), 2.2},
        {QStringLiteral("bloomStrength"), 0.75}, {QStringLiteral("bloomRadius"), 0.45},
        {QStringLiteral("bloomThreshold"), 0.6}, {QStringLiteral("visibleLimit"), 360},
        {QStringLiteral("ambientLight"), 1.1}, {QStringLiteral("warmLight"), 15000},
        {QStringLiteral("coolLight"), 11000}, {QStringLiteral("background"), QStringLiteral("#090d18")},
        {QStringLiteral("accentWarm"), QStringLiteral("#e8b47d")},
        {QStringLiteral("accentCool"), QStringLiteral("#78c2ca")},
        {QStringLiteral("shell"), true}, {QStringLiteral("ambient"), true},
        {QStringLiteral("guides"), false}, {QStringLiteral("links"), true},
        {QStringLiteral("pulses"), true}, {QStringLiteral("bloom"), true},
        {QStringLiteral("physics"), true}, {QStringLiteral("rotate"), true},
        {QStringLiteral("pauseOnInspect"), true}, {QStringLiteral("labels"), true},
        // The content sphere (Xavi's desktop): what it lists and how its frame looks.
        {QStringLiteral("fieldSource"), QStringLiteral("tfog")},
        {QStringLiteral("contentLimit"), 2500}, {QStringLiteral("contentFiles"), 30},
        {QStringLiteral("linkSegments"), 5}, {QStringLiteral("holoFrame"), false},
        {QStringLiteral("holoOpacity"), 0.35}, {QStringLiteral("coreGlow"), 0.0},
        // A colour grade over the finished frame, as the CSS filter Xavi's desktop laid
        // over the sphere's canvas: hue-rotate, saturate, contrast and brightness.
        {QStringLiteral("gradeHue"), 0.0}, {QStringLiteral("gradeSaturate"), 1.0},
        {QStringLiteral("gradeContrast"), 1.0}, {QStringLiteral("gradeBrightness"), 1.0},
        // The hologram's screen: faint scan lines and a small red/cyan split at edges.
        {QStringLiteral("gradeScanlines"), 0.0}, {QStringLiteral("gradeSplit"), 0.0},
        // The design's bloom inside the grade pass, for when the scene's own bloom is off.
        {QStringLiteral("gradeGlow"), 0.0},
    };
    for (const auto &c : categoryTable)
        s.insert(QStringLiteral("color") + QLatin1String(c.key), QString::fromLatin1(c.colour));
    for (const auto &c : contentCategoryTable)
        s.insert(QStringLiteral("color") + QLatin1String(c.key), QString::fromLatin1(c.colour));
    // Source appearance defaults extend the existing OS model. The OS keeps its
    // density ceiling and semantic category palette; source file scanning is absent.
    const auto delivered = xaviDefaults();
    for (auto it = delivered.begin(); it != delivered.end(); ++it) {
        if (it.key() == QLatin1String("visibleLimit")) continue;
        const auto current = s.value(it.key());
        if (current.typeId() == QMetaType::Int || it.key() == QLatin1String("textureResolution")
            || it.key() == QLatin1String("captureWidth"))
            s.insert(it.key(), it.value().toInt());
        else s.insert(it.key(), it.value());
    }
    return s;
}


struct Range {
    const char *key;
    double low;
    double high;
};

constexpr Range ranges[] = {
    {"categoryPull", 0, 1}, {"hierarchyPull", 0, 1}, {"radialLayers", 0, 0.85},
    {"sizeInfluence", 0, 0.4}, {"scale", 0.4, 2}, {"nodeSize", 0.3, 4},
    {"visibleLimit", 12, 420}, {"focusSpread", 0, 0.6}, {"shellScale", 0.65, 1.3},
    {"shellOpacity", 0, 0.4}, {"metalness", 0, 1}, {"roughness", 0, 1}, {"emissive", 0, 2},
    {"textureContrast", 0, 3}, {"textureDepth", 0, 0.15}, {"ambientCount", 0, 3500},
    {"ambientSize", 0.1, 2}, {"ghostOpacity", 0, 0.6}, {"linkOpacity", 0, 0.25},
    {"linkBudget", 0, 6000}, {"highlightOpacity", 0, 1}, {"highlightBudget", 10, 500},
    {"contentLimit", 60, 6000}, {"contentFiles", 1, 200}, {"linkSegments", 1, 24},
    {"holoOpacity", 0, 1}, {"coreGlow", 0, 3}, {"gradeHue", 0, 360}, {"gradeSaturate", 0, 3},
    {"gradeContrast", 0, 2}, {"gradeBrightness", 0, 2}, {"gradeScanlines", 0, 1}, {"gradeSplit", 0, 4}, {"gradeGlow", 0, 3},
    {"reaction", 0, 500}, {"reach", 5, 180}, {"spring", 1, 60}, {"damping", 0.5, 0.98},
    {"rotationSpeed", 0, 2}, {"pulseSpeed", 0, 2}, {"bloomStrength", 0, 2},
    {"bloomRadius", 0, 1}, {"bloomThreshold", 0, 1}, {"exposure", 0.4, 2.5},
    {"ambientLight", 0, 2}, {"warmLight", 0, 40000}, {"coolLight", 0, 30000},
    {"shellDensity", 0.25, 2}, {"shellFlow", 0, 2}, {"textureResolution", 256, 4096},
    {"colorVariation", 0, 2}, {"focusDim", 0, 0.95}, {"energyWidth", 0.2, 2}, {"energyIntensity", 0, 2},
    {"ambientFlowIntensity", 0, 2}, {"ambientFlowSpeed", 0, 2},
    {"relationFlowIntensity", 0, 2}, {"relationFlowSpeed", 0, 2},
    {"pixelRatio", 0.5, 3}, {"captureWidth", 1024, 3840},
    {"visualPoleColorMix", 0, 1}, {"visualGlowMaskIntensity", 0, 2},
    {"visualLineWidthPx", 0.5, 4}, {"visualDetailFrequency", 0.1, 3},
};

const QStringList layoutModes{QStringLiteral("regions"), QStringLiteral("legacy")};

/// How a value outside its range is treated: the panel's sliders are clamped to it, a
/// preset file that leaves it is wrong and is refused.
enum class Bounds { Clamp, Strict };

/// The one check every setting passes, whoever sets it: the key must be one defaults()
/// has, the value of its type, a colour a `#rrggbb`, a layout one the model knows and a
/// number finite and in range. Returns the value to store, or an invalid QVariant with
/// `reason` saying why not.
QVariant validated(const QString &key, const QVariant &current, const QVariant &value, Bounds bounds,
                   QString &reason)
{
    const auto refuse = [&](const QString &why) {
        reason = key + QStringLiteral(": ") + why;
        return QVariant();
    };
    const bool strict = bounds == Bounds::Strict;
    if (!current.isValid())
        return refuse(QStringLiteral("not a setting"));
    if (current.typeId() == QMetaType::Bool) {
        if (strict && value.typeId() != QMetaType::Bool)
            return refuse(QStringLiteral("expected true or false"));
        return value.toBool();
    }
    if (current.typeId() == QMetaType::QString) {
        if (strict && value.typeId() != QMetaType::QString)
            return refuse(QStringLiteral("expected text"));
        const QString text = value.toString().trimmed().toLower();
        if (key == QLatin1String("layoutMode")) {
            if (!layoutModes.contains(text))
                return refuse(QStringLiteral("expected ") + layoutModes.join(QStringLiteral(" or ")));
        } else if (key == QLatin1String("fieldSource")) {
            if (!fieldSources.contains(text))
                return refuse(QStringLiteral("expected ") + fieldSources.join(QStringLiteral(" or ")));
        } else if (const auto choices = xaviChoices(key); !choices.isEmpty()) {
            if (!choices.contains(text))
                return refuse(QStringLiteral("expected ") + choices.join(QStringLiteral(" or ")));
        } else if (!lumen::parse_hex(text.toStdString())) {
            return refuse(QStringLiteral("expected a #rrggbb colour"));
        }
        return text;
    }
    const int type = value.typeId();
    if (strict && type != QMetaType::Double && type != QMetaType::Int && type != QMetaType::LongLong)
        return refuse(QStringLiteral("expected a number"));
    bool ok = false;
    double number = value.toDouble(&ok);
    if (!ok || !std::isfinite(number))
        return refuse(QStringLiteral("expected a finite number"));
    const bool whole = current.typeId() == QMetaType::Int;
    if (strict && whole && number != std::floor(number))
        return refuse(QStringLiteral("expected a whole number"));
    for (const auto &range : ranges) {
        if (key != QLatin1String(range.key))
            continue;
        if (strict && (number < range.low || number > range.high))
            return refuse(QStringLiteral("outside %1..%2").arg(range.low).arg(range.high));
        number = std::clamp(number, range.low, range.high);
    }
    return whole ? QVariant(static_cast<int>(std::lround(number))) : QVariant(number);
}

/// `defaults()` with overrides from the design's catalog, each one checked as a preset
/// file's would be; nullopt, with the reason, if one is refused.
std::optional<QVariantMap> withOverrides(const QVariantMap &overrides, QString &reason)
{
    QVariantMap s = defaults();
    for (auto it = overrides.begin(); it != overrides.end(); ++it) {
        const QVariant next = validated(it.key(), s.value(it.key()), it.value(), Bounds::Strict, reason);
        if (!next.isValid())
            return std::nullopt;
        s.insert(it.key(), next);
    }
    return s;
}

/// The design's composition profiles beyond "field" (crown, atlas, dense): main.js
/// PROFILES, from the Rust catalog. Empty for any other name.
std::optional<QVariantMap> originalProfile(const QString &name, QString &reason)
{
    const auto overrides = xaviProfile(name);
    if (!overrides || overrides->isEmpty())
        return std::nullopt;
    return withOverrides(*overrides, reason);
}

/// The earlier V2 look, which the design's "Antes" button shows for comparison
/// (main.js LEGACY_SETTINGS).
QVariantMap legacy()
{
    QString reason;
    const auto settings = withOverrides(xaviLegacy(), reason);
    if (!settings)
        qWarning("archie-display: the catalog's legacy look is refused: %s", qPrintable(reason));
    return settings.value_or(defaults());
}

/// Look presets are the JSON files embedded under :/archie/presets (presets/*.json in the
/// source tree): a file is a preset, and its name is the file's. They are selected from the
/// CLI or the voice (`archie visual preset <name>`). Cinematics and overlay switches
/// remain separate commands. `field` is the original "Regiones vivas", with no overrides.
const QString presetDirectory = QStringLiteral(":/archie/presets");
const QString basePreset = QStringLiteral("field");

QStringList presetNames()
{
    static const QStringList names = [] {
        QStringList out;
        const auto files = QDir(presetDirectory).entryList({QStringLiteral("*.json")}, QDir::Files, QDir::Name);
        for (const QString &file : files)
            out.append(file.chopped(5));
        for (const QString &name : xaviPresetNames())
            if (!out.contains(name)) out.append(name);
        return out;
    }();
    return names;
}

/// The settings a preset gives: defaults() with the file's overrides on top. The file is
/// read whole and checked before anything is used; one bad key or value refuses all of it.
std::optional<QVariantMap> readPreset(const QString &name, QString &reason)
{
    if (!presetNames().contains(name)) {
        reason = QStringLiteral("unknown preset ") + name;
        return std::nullopt;
    }
    QJsonObject overrides;
    if (const auto delivered = xaviPreset(name)) {
        overrides = QJsonObject::fromVariantMap(*delivered);
    } else {
        QFile file(presetDirectory + QLatin1Char('/') + name + QStringLiteral(".json"));
        if (!file.open(QIODevice::ReadOnly)) {
            reason = QStringLiteral("preset ") + name + QStringLiteral(" is unreadable");
            return std::nullopt;
        }
        QJsonParseError parse{};
        const QJsonDocument document = QJsonDocument::fromJson(file.readAll(), &parse);
        if (parse.error != QJsonParseError::NoError || !document.isObject()) {
            reason = QStringLiteral("preset ") + name + QStringLiteral(" is not a JSON object");
            return std::nullopt;
        }
        const QJsonObject root = document.object();
        if (root.contains(QStringLiteral("name")) && root.value(QStringLiteral("name")).toString() != name) {
            reason = QStringLiteral("preset ") + name + QStringLiteral(" calls itself ")
                     + root.value(QStringLiteral("name")).toString();
            return std::nullopt;
        }
        if (!root.value(QStringLiteral("settings")).isObject()) {
            reason = QStringLiteral("preset ") + name + QStringLiteral(" has no settings object");
            return std::nullopt;
        }
        overrides = root.value(QStringLiteral("settings")).toObject();
    }
    QVariantMap settings = defaults();
    for (auto it = overrides.begin(); it != overrides.end(); ++it) {
        QString why;
        const QVariant next =
            validated(it.key(), settings.value(it.key()), it.value().toVariant(), Bounds::Strict, why);
        if (!next.isValid()) {
            reason = QStringLiteral("preset ") + name + QStringLiteral(": ") + why;
            return std::nullopt;
        }
        settings.insert(it.key(), next);
    }
    return settings;
}

QVector3D toQ(const Vec3 &v) { return QVector3D(v.x, v.y, v.z); }
Vec3 toV(const QVector3D &v) { return Vec3{v.x(), v.y(), v.z()}; }
QVector3D rgb(const lumen::Rgb &c) { return QVector3D(c.r, c.g, c.b); }

void entry(QByteArray &table, const QVector3D &position, float scale, const lumen::Rgb &colour,
           float alpha, const QVector4D &data)
{
    QQuick3DInstancing::InstanceTableEntry e;
    e.row0 = QVector4D(scale, 0, 0, position.x());
    e.row1 = QVector4D(0, scale, 0, position.y());
    e.row2 = QVector4D(0, 0, scale, position.z());
    e.color = QVector4D(colour.r, colour.g, colour.b, alpha);
    // Qt Quick 3D hands custom materials a flat grey for INSTANCE_COLOR, so the colour
    // travels in the data channel instead: sRGB at 8 bits a channel in one float (24 bits
    // are exact in a float), and activity and focus at 8 bits each in another. The
    // shaders unpack both.
    const auto byte = [](float linear) {
        return static_cast<int>(std::lround(std::pow(std::clamp(linear, 0.0F, 1.0F), 1.0F / 2.2F) * 255.0F));
    };
    const auto unit = [](float v) { return static_cast<int>(std::lround(std::clamp(v, 0.0F, 1.0F) * 255.0F)); };
    const float rgb = static_cast<float>(byte(colour.r) * 65536 + byte(colour.g) * 256 + byte(colour.b));
    const float state = static_cast<float>(unit(data.x()) * 256 + unit(data.y()));
    e.instanceData = QVector4D(rgb, state, data.z(), data.w());
    table.append(reinterpret_cast<const char *>(&e), sizeof(e));
}

std::vector<float> meshVertices(const archie::visual::Mesh &mesh)
{
    std::vector<float> out;
    out.reserve(mesh.vertices.size() * 6);
    for (std::size_t i = 0; i < mesh.vertices.size(); ++i) {
        const Vec3 &v = mesh.vertices[i];
        const Vec3 n = i < mesh.normals.size() ? mesh.normals[i] : v;
        out.insert(out.end(), {v.x, v.y, v.z, n.x, n.y, n.z});
    }
    return out;
}

void appendCurve(std::vector<float> &out, const QVector3D &a, const QVector3D &b, int steps, bool dashed)
{
    const Vec3 va = toV(a);
    const Vec3 vb = toV(b);
    for (int part = 0; part < steps; ++part) {
        if (dashed && part % 3 == 1)
            continue;
        const Vec3 s = steps == 1 ? va : lumen::curve(va, vb, static_cast<float>(part) / static_cast<float>(steps));
        const Vec3 e = steps == 1 ? vb : lumen::curve(va, vb, static_cast<float>(part + 1) / static_cast<float>(steps));
        out.insert(out.end(), {s.x, s.y, s.z, e.x, e.y, e.z});
    }
}

std::uint32_t seedOf(const QString &id)
{
    std::uint32_t hash = 2166136261U;
    for (const char byte : id.toUtf8()) {
        hash ^= static_cast<std::uint8_t>(byte);
        hash *= 16777619U;
    }
    return hash;
}

float statusCode(const QString &status)
{
    if (status == QLatin1String("predicted")) return 1.0F;
    if (status == QLatin1String("active")) return 2.0F;
    if (status == QLatin1String("verified")) return 3.0F;
    if (status == QLatin1String("error")) return 4.0F;
    return 0.0F;
}

}  // namespace

LumenField::LumenField(QObject *parent, QUrl endpoint)
    : QObject(parent), m_endpoint(endpoint.isEmpty() ? QUrl(QString::fromLatin1(lumenUrl)) : std::move(endpoint)),
      m_settings(defaults())
{
    m_frameTimer.setTimerType(Qt::PreciseTimer);
    m_frameTimer.setInterval(m_frameIntervalMs);
    connect(&m_frameTimer, &QTimer::timeout, this, &LumenField::tick);
    bool pollValid = false;
    const int pollOverride = qEnvironmentVariableIntValue("ARCHIE_ORB_POLL_MS", &pollValid);
    m_pollTimer.setInterval(pollValid ? std::clamp(pollOverride, 200, 2000) : 500);
    m_pollTimer.setTimerType(Qt::CoarseTimer);
    connect(&m_pollTimer, &QTimer::timeout, this, &LumenField::request);
    m_requestDelay.setSingleShot(true);
    connect(&m_requestDelay, &QTimer::timeout, this, &LumenField::request);
    connect(this, &LumenField::settingsChanged, this, &LumenField::updateTimers);
    connect(this, &LumenField::sceneChanged, this, &LumenField::updateTimers);
    connect(this, &LumenField::selectionChanged, this, &LumenField::updateTimers);
    // A broken preset file is found when the display starts, not when someone asks for it.
    for (const QString &name : presetNames()) {
        QString reason;
        if (!readPreset(name, reason))
            qWarning("archie-display: %s", qPrintable(reason));
    }
    if (!presetNames().contains(basePreset))
        qWarning("archie-display: the base preset %s is not embedded", qPrintable(basePreset));
    m_profile = basePreset;
    m_contentRoot = defaultContentRoot();
    updateLook();
    updateCategoryRows();
    setStatus(QStringLiteral("ESPERANDO EL CAMPO"));
}

void LumenField::setContentRoot(const QString &root)
{
    const QString cleaned = root.isEmpty() ? defaultContentRoot() : QDir::cleanPath(root);
    if (cleaned == m_contentRoot)
        return;
    m_contentRoot = cleaned;
    m_contentDirty = true;
    if (contentSource())
        scheduleRequest(0);
    emit contentRootChanged();
}

bool LumenField::contentSource() const
{
    return m_settings.value(QStringLiteral("fieldSource")).toString() == QLatin1String("content");
}

QVariantList LumenField::labelAnchors() const
{
    std::vector<int> top;
    const int root = indexOf(m_rootId);
    for (std::size_t i = 0; i < m_nodes.size(); ++i)
        if (m_nodes[i].parentIndex == root && root >= 0 && m_nodes[i].branch && drawn(static_cast<int>(i)))
            top.push_back(static_cast<int>(i));
    std::sort(top.begin(), top.end(), [this](int a, int b) {
        return m_nodes[static_cast<std::size_t>(a)].memberCount > m_nodes[static_cast<std::size_t>(b)].memberCount;
    });
    if (top.size() > 24)
        top.resize(24);
    QVariantList out;
    for (const int i : top) {
        const QVector3D p = positionAt(i);
        out.push_back(QVariantMap{{QStringLiteral("id"), m_nodes[static_cast<std::size_t>(i)].id},
                                  {QStringLiteral("name"), m_nodes[static_cast<std::size_t>(i)].name},
                                  {QStringLiteral("x"), p.x()}, {QStringLiteral("y"), p.y()}, {QStringLiteral("z"), p.z()}});
    }
    return out;
}

void LumenField::setActive(bool active)
{
    if (m_active == active)
        return;
    m_active = active;
    if (active) {
        m_clock.start();
        m_fpsClock.start();
        m_framesCounted = 0;
    } else {
        ++m_queryGeneration;
        m_requestDelay.stop();
        m_requestAgain = false;
        if (m_reply) m_reply->abort();
    }
    updateTimers();
    if (active) request();
    emit activeChanged();
}

void LumenField::updateTimers()
{
    if (!m_active || m_frameIntervalMs == 0) {
        m_frameTimer.stop();
        m_pollTimer.stop();
        return;
    }
    m_simulationActive = true;
    m_frameTimer.start(m_frameIntervalMs);
    m_pollTimer.start();
    m_clock.restart();
}

void LumenField::setFrameIntervalMs(int interval)
{
    interval = interval <= 0 ? 0 : std::clamp(interval, 16, 1000);
    if (interval == m_frameIntervalMs) return;
    const bool resuming = m_frameIntervalMs == 0;
    m_frameIntervalMs = interval;
    if (interval == 0) {
        ++m_queryGeneration;
        m_requestDelay.stop();
        m_requestAgain = false;
        if (m_reply) m_reply->abort();
    }
    updateTimers();
    if (resuming && interval > 0) request();
    emit budgetChanged();
}

bool LumenField::ambientAllowed() const
{
    return !m_reducedMotion && m_renderBudget.value(QStringLiteral("ambientMotion"), true).toBool();
}

int LumenField::particleBudget() const
{
    return std::clamp(m_renderBudget.value(QStringLiteral("maxParticles"), 2000).toInt(), 0, 6000);
}

void LumenField::setReducedMotion(bool reduced)
{
    if (m_reducedMotion == reduced) return;
    m_reducedMotion = reduced;
    updateLook();
    rebuildGhosts();
    m_positionsDirty = true;
    emit settingsChanged();
    emit budgetChanged();
}

void LumenField::setRenderBudget(const QVariantMap &budget)
{
    // Frame summaries also carry camera/history state. Only budget changes can
    // rebuild decoration; advancing those unrelated fields is not a new budget.
    const QVariantMap normalized{
        {QStringLiteral("tier"), budget.value(QStringLiteral("tier"), QStringLiteral("normal"))},
        {QStringLiteral("frameIntervalMs"), budget.value(QStringLiteral("frameIntervalMs"), 16)},
        {QStringLiteral("maxVisibleNodes"), budget.value(QStringLiteral("maxVisibleNodes"), 420)},
        {QStringLiteral("maxParticles"), budget.value(QStringLiteral("maxParticles"), 2000)},
        {QStringLiteral("postProcessing"), budget.value(QStringLiteral("postProcessing"), false)},
        {QStringLiteral("ambientMotion"), budget.value(QStringLiteral("ambientMotion"), true)},
    };
    if (m_renderBudget == normalized) return;
    const int oldLimit = m_renderBudget.value(QStringLiteral("maxVisibleNodes"), 420).toInt();
    m_renderBudget = normalized;
    setFrameIntervalMs(budget.value(QStringLiteral("frameIntervalMs"), 16).toInt());
    updateLook();
    rebuildGhosts();
    m_positionsDirty = true;
    if (oldLimit != budget.value(QStringLiteral("maxVisibleNodes"), 420).toInt()) scheduleRequest();
    emit settingsChanged();
    emit budgetChanged();
}

bool LumenField::acquireView(QObject *owner)
{
    if (!owner || (m_viewOwner && m_viewOwner != owner)) return false;
    if (m_viewOwner == owner) return true;
    m_viewOwner = owner;
    connect(owner, &QObject::destroyed, this, [this] { m_viewOwner.clear(); setActive(false); });
    return true;
}

void LumenField::releaseView(QObject *owner)
{
    if (m_viewOwner != owner) return;
    disconnect(owner, nullptr, this, nullptr);
    m_viewOwner.clear();
    setActive(false);
}

void LumenField::bindLayer(const QString &name, QObject *target)
{
    if (auto *table = qobject_cast<LumenInstancing *>(target)) {
        m_tables.insert(name, table);
    } else if (auto *geometry = qobject_cast<LumenGeometry *>(target)) {
        m_geometry.insert(name, geometry);
        if (name == QLatin1String("orbMesh")) {
            const auto mesh = lumen::icosphere(3);
            geometry->setSolid(meshVertices(mesh), mesh.indices);
        } else if (name == QLatin1String("entityMesh")) {
            const auto mesh = lumen::icosphere(2);
            geometry->setSolid(meshVertices(mesh), mesh.indices);
        } else if (name == QLatin1String("satelliteMesh")) {
            const auto mesh = lumen::icosphere(1);
            geometry->setSolid(meshVertices(mesh), mesh.indices);
        } else if (name == QLatin1String("ring")) {
            const auto mesh = lumen::torus(1.0F, 0.035F, 64, 5);
            geometry->setSolid(meshVertices(mesh), mesh.indices);
        } else if (name == QLatin1String("quad")) {
            geometry->setQuad();
        } else if (name == QLatin1String("membrane") || name == QLatin1String("membraneEdges")) {
            rebuildMembrane();
        }
    }
    m_structureDirty = true;
    m_positionsDirty = true;
}

float LumenField::number(const char *key) const
{
    return static_cast<float>(m_settings.value(QLatin1String(key)).toDouble());
}

bool LumenField::flag(const char *key) const
{
    return m_settings.value(QLatin1String(key)).toBool();
}

void LumenField::setSetting(const QString &key, const QVariant &value)
{
    const QVariant current = m_settings.value(key);
    QString reason;
    const QVariant next = validated(key, current, value, Bounds::Clamp, reason);
    if (!next.isValid() || next == current)
        return;
    m_settings.insert(key, next);
    updateLook();
    static const QStringList shapeKeys{QStringLiteral("categoryPull"), QStringLiteral("hierarchyPull"),
                                       QStringLiteral("radialLayers"), QStringLiteral("sizeInfluence"),
                                       QStringLiteral("scale"), QStringLiteral("visibleLimit"),
                                       QStringLiteral("layoutMode"), QStringLiteral("fieldSource"),
                                       QStringLiteral("contentLimit"), QStringLiteral("contentFiles")};
    if (shapeKeys.contains(key)) {
        m_contentDirty = true;
        scheduleRequest(140);
    }
    if (key == QLatin1String("fieldSource"))
        updateCategoryRows();
    if (key == QLatin1String("holoFrame") || key == QLatin1String("linkSegments"))
        m_structureDirty = true;
    if (key.startsWith(QLatin1String("color")) || key == QLatin1String("accentWarm")
        || key == QLatin1String("accentCool") || key == QLatin1String("shellScale")
        || key == QLatin1String("shellStyle") || key == QLatin1String("shellDensity")
        || key == QLatin1String("shellTint") || key == QLatin1String("shell"))
        rebuildMembrane();
    if (key == QLatin1String("ambient") || key == QLatin1String("ambientCount"))
        rebuildGhosts();
    if (key == QLatin1String("linkBudget"))
        m_structureDirty = true;
    if (key.startsWith(QLatin1String("color")))
        updateCategoryRows();
    if (key == QLatin1String("focusSpread") || key == QLatin1String("highlightBudget"))
        select(m_selected);
    m_positionsDirty = true;
    emit settingsChanged();
}

QStringList LumenField::presets() const
{
    return presetNames();
}

QString LumenField::loadProfile(const QString &name)
{
    QString reason;
    const std::optional<QVariantMap> settings = readPreset(name, reason);
    if (!settings)
        return reason;
    m_saved.clear();
    m_profile = name;
    m_settings = *settings;
    m_contentDirty = true;
    updateLook();
    updateCategoryRows();
    rebuildMembrane();
    rebuildGhosts();
    m_structureDirty = true;
    m_positionsDirty = true;
    scheduleRequest(0);
    emit settingsChanged();
    return {};
}

bool LumenField::applyProfile(const QString &name)
{
    QString refused;
    const std::optional<QVariantMap> original = originalProfile(name, refused);
    if (!refused.isEmpty()) {
        qWarning("archie-display: profile %s: %s", qPrintable(name), qPrintable(refused));
        return false;
    }
    if (!original) {
        const QString reason = loadProfile(name);
        if (!reason.isEmpty())
            qWarning("archie-display: %s", qPrintable(reason));
        return reason.isEmpty();
    }
    // The design's own composition profiles (the voice's cinematics) stay apart from the
    // file-backed preset catalog.
    m_saved.clear();
    m_profile = name;
    m_settings = *original;
    m_contentDirty = true;
    updateLook();
    updateCategoryRows();
    rebuildMembrane();
    rebuildGhosts();
    m_structureDirty = true;
    m_positionsDirty = true;
    scheduleRequest(0);
    emit settingsChanged();
    return true;
}

void LumenField::resetProfile()
{
    applyProfile(m_profile);
}

void LumenField::toggleCompare()
{
    if (m_saved.isEmpty()) {
        m_saved = m_settings;
        m_settings = legacy();
    } else {
        m_settings = m_saved;
        m_saved.clear();
    }
    updateLook();
    updateCategoryRows();
    rebuildMembrane();
    rebuildGhosts();
    m_structureDirty = true;
    m_positionsDirty = true;
    scheduleRequest(0);
    emit settingsChanged();
    emit fitRequested();
}

void LumenField::setLens(const QString &lens)
{
    if (lens != QLatin1String("architecture") && lens != QLatin1String("folders")
        && lens != QLatin1String("entities"))
        return;
    if (lens == m_lens)
        return;
    m_lens = lens;
    m_selected.clear();
    m_hoveredId.clear();
    updateSelectionInfo();
    updateHoveredInfo();
    scheduleRequest(0);
    emit lensChanged();
    emit fitRequested();
}

void LumenField::toggleModule(const QString &id)
{
    const int i = indexOf(id);
    if (m_lens == QLatin1String("entities") || i < 0)
        return;
    const Node &node = m_nodes[static_cast<std::size_t>(i)];
    if (!node.expandable && !node.expanded)
        return;
    if (m_expanded.contains(id) || node.expanded)
        m_expanded.remove(id);
    else if (m_expanded.size() < 256)
        m_expanded.insert(id);
    m_selected = id;
    scheduleRequest(0);
}

void LumenField::collapse(const QString &id)
{
    if (m_expanded.remove(id))
        scheduleRequest(0);
}

void LumenField::toggleCategory(const QString &key)
{
    if (m_hiddenCategories.contains(key))
        m_hiddenCategories.remove(key);
    else
        m_hiddenCategories.insert(key);
    m_structureDirty = true;
    m_positionsDirty = true;
    updateCategoryRows();
    rebuildMembrane();
}

void LumenField::select(const QString &id)
{
    const int i = indexOf(id);
    m_selected = i >= 0 ? id : QString();
    // A selected module's members spread out from it, as far as the setting allows.
    std::vector<Vec3> anchors;
    std::vector<int> parents;
    anchors.reserve(m_nodes.size());
    for (const Node &node : m_nodes) {
        anchors.push_back(toV(node.anchor));
        parents.push_back(node.parentIndex);
    }
    const bool spread = i >= 0 && m_nodes[static_cast<std::size_t>(i)].branch;
    const auto focused = lumen::focus_region(anchors, parents, spread ? i : -1, number("focusSpread"),
                                             m_fieldRadius);
    for (std::size_t k = 0; k < m_bodies.size() && k < focused.size(); ++k)
        m_bodies[k].anchor = focused[k];
    updateSelectionInfo();
    updateHighlight();
    m_positionsDirty = true;
}

void LumenField::clearSelection()
{
    select(QString());
}

void LumenField::explore(const QString &id)
{
    m_focus = id == m_rootId ? QString() : id;
    m_selected.clear();
    updateSelectionInfo();
    scheduleRequest(0);
    emit fitRequested();
}

void LumenField::up()
{
    if (m_focusPath.isEmpty())
        return;
    explore(m_focusPath.first());
}

void LumenField::reveal(const QString &id, const QStringList &ancestors, const QString &kind)
{
    if (id.isEmpty())
        return;
    if (!m_focus.isEmpty() && !ancestors.contains(m_focus) && id != m_focus)
        m_focus.clear();
    if (m_lens != QLatin1String("entities")) {
        for (const QString &ancestor : ancestors)
            if (ancestor != m_rootId && m_expanded.size() < 256)
                m_expanded.insert(ancestor);
        const QString parent = ancestors.isEmpty() ? m_rootId : ancestors.last();
        m_expanded.insert(QStringLiteral("@grupo:") + parent + QLatin1Char(':') + categoryOfKind(kind));
    }
    m_pendingReveal = id;
    if (indexOf(id) >= 0) {
        select(id);
        emit focusRequested(positionAt(indexOf(id)));
    }
    scheduleRequest(0);
}

QString LumenField::pick(const QVector3D &origin, const QVector3D &through) const
{
    QVector3D direction = through - origin;
    if (direction.lengthSquared() < 1e-12F)
        return {};
    direction.normalize();
    float best = std::numeric_limits<float>::max();
    int chosen = -1;
    for (std::size_t i = 0; i < m_nodes.size(); ++i) {
        if (!drawn(static_cast<int>(i)))
            continue;
        const QVector3D centre = positionAt(static_cast<int>(i));
        const float radius = orbScale(static_cast<int>(i)) * 1.2F + 0.6F;
        const QVector3D toCentre = centre - origin;
        const float along = QVector3D::dotProduct(toCentre, direction);
        if (along < 0.0F)
            continue;
        const float missSquared = toCentre.lengthSquared() - along * along;
        if (missSquared <= radius * radius && along < best) {
            best = along;
            chosen = static_cast<int>(i);
        }
    }
    return chosen >= 0 ? m_nodes[static_cast<std::size_t>(chosen)].id : QString();
}

void LumenField::hover(const QString &id)
{
    const QString next = indexOf(id) >= 0 ? id : QString();
    if (next == m_hoveredId)
        return;
    m_hoveredId = next;
    updateHoveredInfo();
    updateHighlight();
    m_positionsDirty = true;
    // A frame timer stopped under reduced motion or pause must still draw the highlight.
    wake();
}

void LumenField::togglePause()
{
    m_paused = !m_paused;
    wake();
    emit pausedChanged();
}

void LumenField::refresh()
{
    scheduleRequest(0);
}

QVector3D LumenField::positionOf(const QString &id) const
{
    return positionAt(indexOf(id));
}

void LumenField::scheduleRequest(int delayMs)
{
    // Debounced: a burst of changes (a slider dragged, a look applied key by key) becomes
    // one request when it settles. The reply already in flight is not thrown away; if the
    // view's structure changed meanwhile it is not accepted, and the debounced request
    // brings the current one.
    if (!m_active || m_frameIntervalMs == 0) return;
    m_requestDelay.start(std::max(0, delayMs));
}

QString LumenField::structureKey() const
{
    QStringList expanded(m_expanded.cbegin(), m_expanded.cend());
    expanded.sort();
    if (contentSource())
        return QStringLiteral("content|") + m_contentRoot;
    return m_lens + QLatin1Char('|') + expanded.join(QLatin1Char(',')) + QLatin1Char('|') + m_focus
           + QLatin1Char('|') + m_settings.value(QStringLiteral("layoutMode")).toString();
}

void LumenField::wake()
{
    m_simulationActive = true;
    if (m_active && m_frameIntervalMs > 0 && !m_frameTimer.isActive()) {
        m_clock.restart();
        m_frameTimer.start(m_frameIntervalMs);
    }
}

void LumenField::request()
{
    if (!m_active || m_frameIntervalMs == 0)
        return;
    if (m_requestPending) {
        m_requestAgain = true;
        return;
    }
    if (m_settings.value(QStringLiteral("fieldSource")).toString() == QLatin1String("neural"))
        return;
    if (contentSource()) {
        requestContent();
        return;
    }
    QStringList expanded;
    for (const QString &id : std::as_const(m_expanded))
        expanded.push_back(QString::fromLatin1(QUrl::toPercentEncoding(id)));
    expanded.sort();
    QString query = QStringLiteral("lens=") + m_lens
                    + QStringLiteral("&limit=") + QString::number(std::clamp(std::min(static_cast<int>(number("visibleLimit")),
                        m_renderBudget.value(QStringLiteral("maxVisibleNodes"), 420).toInt()), 12, 420))
                    + QStringLiteral("&layout=") + m_settings.value(QStringLiteral("layoutMode")).toString()
                    + QStringLiteral("&category_pull=") + QString::number(number("categoryPull"))
                    + QStringLiteral("&hierarchy_pull=") + QString::number(number("hierarchyPull"))
                    + QStringLiteral("&radial_layers=") + QString::number(number("radialLayers"))
                    + QStringLiteral("&size_influence=") + QString::number(number("sizeInfluence"))
                    + QStringLiteral("&scale=") + QString::number(number("scale"));
    if (!expanded.isEmpty())
        query += QStringLiteral("&expand=") + expanded.join(QLatin1Char(','));
    if (!m_focus.isEmpty())
        query += QStringLiteral("&focus=") + QString::fromLatin1(QUrl::toPercentEncoding(m_focus));
    QUrl url(m_endpoint);
    url.setQuery(query, QUrl::StrictMode);
    QNetworkRequest get(url);
    get.setTransferTimeout(3000);
    m_requestPending = true;
    QNetworkReply *reply = m_network.get(get);
    m_reply = reply;
    const quint64 generation = m_queryGeneration;
    const QString structure = structureKey();
    auto body = std::make_shared<QByteArray>();
    connect(reply, &QNetworkReply::readyRead, this, [reply, body] {
        if (reply->isOpen()) body->append(reply->read(maximumSceneBytes + 1 - body->size()));
        if (body->size() > maximumSceneBytes) reply->abort();
    });
    connect(reply, &QNetworkReply::finished, this, [this, reply, body, generation, structure] {
        reply->deleteLater();
        m_requestPending = false;
        m_reply.clear();
        // The generation changes only when the field stops drawing; a reply from before
        // that is dropped, and a field drawing again asks afresh.
        if (!m_active || m_frameIntervalMs == 0 || generation != m_queryGeneration) {
            m_requestAgain = false;
            if (m_active && m_frameIntervalMs > 0) request();
            return;
        }
        if (reply->isOpen()) body->append(reply->read(maximumSceneBytes + 1 - body->size()));
        if (body->size() > maximumSceneBytes) {
            setStatus(uiText(QStringLiteral("SIN CAMPO"), QStringLiteral("NO FIELD")), uiText(QStringLiteral("la escena superó el límite de 8 MiB"), QStringLiteral("the scene went past the 8 MiB limit")));
        } else
        if (reply->error() != QNetworkReply::NoError) {
            setStatus(uiText(QStringLiteral("SIN CAMPO"), QStringLiteral("NO FIELD")), uiText(QStringLiteral("archie-displayd no responde: "), QStringLiteral("archie-displayd is not responding: ")) + reply->errorString());
        } else if (structure == structureKey()) {
            // Shape and look settings only move what is drawn; a reply for slightly older
            // values is still the right field, so continuous changes never freeze it.
            accept(*body);
        }
        if (m_requestAgain) {
            m_requestAgain = false;
            // A debounce still running sends it when the burst settles.
            if (!m_requestDelay.isActive()) this->request();
        }
    });
}

void LumenField::requestContent()
{
    // The folder changes rarely: list it again every 5 s (so new files appear, as on
    // Xavi's desktop) or as soon as what it is asked for changes.
    if (!m_contentDirty && m_contentClock.isValid() && m_contentClock.elapsed() < 5000)
        return;
    m_contentDirty = false;
    m_contentClock.restart();
    m_requestPending = true;
    const QByteArray root = m_contentRoot.toUtf8();
    const DesignContentOptions options{
        static_cast<std::uint32_t>(number("contentLimit")), static_cast<std::uint32_t>(number("contentFiles")),
        number("categoryPull"), number("hierarchyPull"), number("radialLayers"), number("sizeInfluence"), number("scale")};
    const qint64 now = QDateTime::currentMSecsSinceEpoch();
    const quint64 generation = m_queryGeneration;
    const QString structure = structureKey();
    QPointer<LumenField> self(this);
    QThreadPool::globalInstance()->start([self, root, options, now, generation, structure] {
        QByteArray body(maximumSceneBytes, Qt::Uninitialized);
        const std::size_t written = design_content_scene(root.constData(), static_cast<std::size_t>(root.size()), &options,
                                                         now, body.data(), static_cast<std::size_t>(body.size()));
        body.truncate(static_cast<qsizetype>(written));
        if (!self)
            return;
        QMetaObject::invokeMethod(self.data(), [self, body, generation, structure, root] {
            if (!self)
                return;
            LumenField &field = *self;
            field.m_requestPending = false;
            if (!field.m_active || field.m_frameIntervalMs == 0 || generation != field.m_queryGeneration) {
                // A listing from before the view stopped drawing; a view drawing again asks afresh.
                field.m_requestAgain = false;
                field.m_contentDirty = true;
                if (field.m_active && field.m_frameIntervalMs > 0) field.request();
                return;
            }
            if (body.isEmpty())
                field.setStatus(uiText(QStringLiteral("SIN CAMPO"), QStringLiteral("NO FIELD")), uiText(QStringLiteral("no se pudo leer "), QStringLiteral("could not read ")) + QString::fromUtf8(root));
            else if (structure == field.structureKey())
                field.accept(body);
            else
                field.m_contentDirty = true;
            if (field.m_requestAgain) {
                field.m_requestAgain = false;
                if (!field.m_requestDelay.isActive()) field.request();
            }
        }, Qt::QueuedConnection);
    });
}

int LumenField::indexOf(const QString &id) const
{
    return m_index.value(id, -1);
}

bool LumenField::drawn(int index) const
{
    if (index < 0 || static_cast<std::size_t>(index) >= m_nodes.size())
        return false;
    const Node &node = m_nodes[static_cast<std::size_t>(index)];
    if (node.kind == QLatin1String("root") || node.kind == QLatin1String("module"))
        return true;
    return !m_hiddenCategories.contains(node.category);
}

float LumenField::attentionOf(int index) const
{
    if (index < 0 || static_cast<std::size_t>(index) >= m_nodes.size())
        return 0.0F;
    const Node &node = m_nodes[static_cast<std::size_t>(index)];
    // What the system is doing with this node, from what the model observed: a Stage
    // action on it, Cortex holding it in focus, or the voice speaking about it. Load alone
    // is not use: a busy daemon glows from its core but is not lit as engaged.
    float score = std::max(node.focus, std::min(1.0F, node.voice * 1.2F));
    if (node.status == QLatin1String("active") || node.status == QLatin1String("error"))
        score = 1.0F;
    else if (node.status == QLatin1String("predicted"))
        score = std::max(score, 0.8F);
    else if (node.status == QLatin1String("verified"))
        score = std::max(score, 0.6F);
    return std::clamp(score, 0.0F, 1.0F);
}

void LumenField::updateAttention()
{
    std::vector<std::pair<float, int>> ranked;
    for (std::size_t i = 0; i < m_nodes.size(); ++i) {
        const float score = attentionOf(static_cast<int>(i));
        if (score >= 0.25F && m_nodes[i].kind != QLatin1String("root"))
            ranked.emplace_back(score, static_cast<int>(i));
    }
    std::sort(ranked.begin(), ranked.end(), [](const auto &a, const auto &b) { return a.first > b.first; });
    if (ranked.size() > 6)
        ranked.resize(6);
    m_attention.clear();
    QVariantList rows;
    for (const auto &[score, i] : ranked) {
        m_attention.push_back(i);
        const Node &node = m_nodes[static_cast<std::size_t>(i)];
        rows.push_back(QVariantMap{{QStringLiteral("id"), node.id}, {QStringLiteral("name"), node.name},
                                  {QStringLiteral("category"), categoryLabel(node.category)},
                                  {QStringLiteral("score"), score}});
    }
    const QString lead = m_attention.empty() ? QString() : m_nodes[static_cast<std::size_t>(m_attention.front())].id;
    if (rows != m_attentionRows) {
        m_attentionRows = rows;
        emit attentionChanged();
    }
    if (lead != m_attentionLead) {
        m_attentionLead = lead;
        emit attentionMoved(lead.isEmpty() ? QVector3D() : m_nodes[static_cast<std::size_t>(m_attention.front())].anchor,
                            !lead.isEmpty());
    }
}

QVector3D LumenField::positionAt(int index) const
{
    if (index < 0 || static_cast<std::size_t>(index) >= m_bodies.size())
        return {};
    return toQ(m_bodies[static_cast<std::size_t>(index)].position);
}

float LumenField::orbScale(int index) const
{
    if (index < 0 || static_cast<std::size_t>(index) >= m_nodes.size())
        return 1.0F;
    const Node &node = m_nodes[static_cast<std::size_t>(index)];
    const float nodeSize = number("nodeSize");
    const bool legacyLayout = m_settings.value(QStringLiteral("layoutMode")).toString() == QLatin1String("legacy");
    float scale = 1.0F;
    if (legacyLayout) {
        scale = (node.kind == QLatin1String("root") ? 5.0F : node.branch ? 2.3F : 1.0F) * nodeSize
                * (1.0F + std::min(0.8F, node.size * 0.8F));
    } else {
        const float members = std::log1p(static_cast<float>(node.memberCount));
        scale = (node.kind == QLatin1String("root") ? 4.4F : node.branch ? 2.2F + std::min(1.9F, members * 0.32F) : 1.0F)
                * nodeSize * (1.0F + std::min(0.52F, node.size * 0.52F));
    }
    return node.id == m_selected ? scale * 1.35F : scale;
}

lumen::Rgb LumenField::colourOf(const QString &category) const
{
    const auto parsed = lumen::parse_hex(m_settings.value(QStringLiteral("color") + category).toString().toStdString());
    return lumen::to_linear(parsed.value_or(lumen::Rgb{0.7F, 0.75F, 0.75F}));
}

void LumenField::updateLook()
{
    const auto hex = [this](const char *key) {
        return lumen::to_linear(lumen::parse_hex(m_settings.value(QLatin1String(key)).toString().toStdString())
                                    .value_or(lumen::Rgb{0.5F, 0.5F, 0.5F}));
    };
    const lumen::Rgb warm = hex("accentWarm");
    const lumen::Rgb cool = hex("accentCool");
    const lumen::Rgb ambient = lumen::to_linear(lumen::Rgb{0xa6 / 255.0F, 0xb8 / 255.0F, 0xba / 255.0F});
    const float scale = std::max(0.5F, m_fieldRadius / 180.0F);
    const auto bloomMode = m_settings.value(QStringLiteral("visualBloomMode")).toString();
    const int bloomModeId = bloomMode == QLatin1String("selection") ? 1 : bloomMode == QLatin1String("polar") ? 2
                           : bloomMode == QLatin1String("off") ? 3 : 0;
    float maximumNodeRadius = std::max(0.3F, number("nodeSize"));
    for (std::size_t i = 0; i < m_nodes.size(); ++i)
        if (drawn(static_cast<int>(i))) maximumNodeRadius = std::max(maximumNodeRadius, orbScale(static_cast<int>(i)));
    const auto sceneProfile = m_sceneInfo.value(QStringLiteral("profile")).toString();
    int qualityId = sceneProfile == QLatin1String("low") ? 2 : sceneProfile == QLatin1String("balanced") ? 1 : 0;
    const auto tier = m_renderBudget.value(QStringLiteral("tier")).toString();
    if (tier == QLatin1String("conserve") || tier == QLatin1String("headless")
        || !m_renderBudget.value(QStringLiteral("postProcessing"), false).toBool()
        || !m_renderBudget.value(QStringLiteral("ambientMotion"), true).toBool())
        qualityId = 2;
    else if (m_frameIntervalMs >= 33)
        qualityId = std::max(1, qualityId);
    m_look = {
        {QStringLiteral("maximumNodeRadius"), maximumNodeRadius},
        {QStringLiteral("qualityId"), qualityId},
        {QStringLiteral("shellStyleId"), m_settings.value(QStringLiteral("layoutMode")).toString() == QLatin1String("legacy") ? -1 : xaviChoices(QStringLiteral("shellStyle")).indexOf(m_settings.value(QStringLiteral("shellStyle")).toString())},
        {QStringLiteral("shellDensity"), number("shellDensity")},
        {QStringLiteral("shellFlow"), number("shellFlow")},
        {QStringLiteral("shellTint"), rgb(hex("shellTint"))},
        {QStringLiteral("nodeStyleId"), xaviStyle(m_settings.value(QStringLiteral("visualNodeStyle")).toString())},
        {QStringLiteral("folderStyleId"), xaviStyle(m_settings.value(QStringLiteral("visualFolderStyle")).toString())},
        {QStringLiteral("linkStyleId"), xaviLink(m_settings.value(QStringLiteral("visualLinkStyle")).toString())},
        {QStringLiteral("pulseStyleId"), xaviPulse(m_settings.value(QStringLiteral("visualPulseStyle")).toString())},
        {QStringLiteral("bloomModeId"), bloomModeId},
        {QStringLiteral("poleMix"), number("visualPoleColorMix")},
        {QStringLiteral("glowMaskIntensity"), number("visualGlowMaskIntensity")},
        {QStringLiteral("lineWidthPx"), number("visualLineWidthPx")},
        {QStringLiteral("detailFrequency"), number("visualDetailFrequency")},
        {QStringLiteral("textureResolution"), number("textureResolution")},
        {QStringLiteral("energyIntensity"), number("energyIntensity")},
        {QStringLiteral("fieldRadius"), m_fieldRadius},
        {QStringLiteral("background"), QColor(m_settings.value(QStringLiteral("background")).toString())},
        {QStringLiteral("warm"), rgb(warm)},
        {QStringLiteral("cool"), rgb(cool)},
        {QStringLiteral("warmLight"), rgb(warm) * (number("warmLight") / 6500.0F)},
        {QStringLiteral("coolLight"), rgb(cool) * (number("coolLight") / 6500.0F)},
        {QStringLiteral("ambientLight"), rgb(ambient) * (number("ambientLight") * 0.55F)},
        {QStringLiteral("warmPosition"), QVector3D(-170, 100, 180) * scale},
        {QStringLiteral("coolPosition"), QVector3D(165, -105, -85) * scale},
        {QStringLiteral("metalness"), number("metalness")},
        {QStringLiteral("roughness"), number("roughness")},
        {QStringLiteral("emissive"), number("emissive")},
        {QStringLiteral("textureContrast"), number("textureContrast")},
        {QStringLiteral("textureDepth"), number("textureDepth")},
        {QStringLiteral("shellOpacity"), flag("shell") ? number("shellOpacity") : 0.0F},
        {QStringLiteral("legacy"), m_settings.value(QStringLiteral("layoutMode")).toString() == QLatin1String("legacy") ? 1.0F : 0.0F},
        {QStringLiteral("ghostOpacity"), flag("ambient") && ambientAllowed() && particleBudget() > 0 ? number("ghostOpacity") : 0.0F},
        {QStringLiteral("ghostSize"), number("ambientSize")},
        {QStringLiteral("linkOpacity"), flag("links") ? number("linkOpacity") : 0.0F},
        {QStringLiteral("highlightOpacity"), flag("links") ? number("highlightOpacity") : 0.0F},
        {QStringLiteral("links"), flag("links")},
        {QStringLiteral("pulses"), flag("pulses") && ambientAllowed() && particleBudget() > 0},
        {QStringLiteral("guides"), flag("guides")},
        {QStringLiteral("bloom"), flag("bloom") && bloomModeId != 3 && m_renderBudget.value(QStringLiteral("postProcessing"), false).toBool()},
        {QStringLiteral("bloomStrength"), number("bloomStrength")},
        {QStringLiteral("bloomRadius"), number("bloomRadius")},
        {QStringLiteral("bloomThreshold"), number("bloomThreshold")},
        {QStringLiteral("exposure"), number("exposure")},
        {QStringLiteral("holoFrame"), flag("holoFrame")},
        {QStringLiteral("holoOpacity"), flag("holoFrame") ? number("holoOpacity") : 0.0F},
        {QStringLiteral("coreGlow"), number("coreGlow")},
        {QStringLiteral("content"), contentSource()},
        {QStringLiteral("gradeHue"), number("gradeHue")},
        {QStringLiteral("gradeSaturate"), number("gradeSaturate")},
        {QStringLiteral("gradeContrast"), number("gradeContrast")},
        {QStringLiteral("gradeBrightness"), number("gradeBrightness")},
        {QStringLiteral("gradeScanlines"), number("gradeScanlines")},
        {QStringLiteral("gradeSplit"), number("gradeSplit")},
        {QStringLiteral("gradeGlow"), number("gradeGlow")},
        {QStringLiteral("graded"), number("gradeHue") != 0.0F || number("gradeSaturate") != 1.0F
                                       || number("gradeContrast") != 1.0F || number("gradeBrightness") != 1.0F
                                       || number("gradeScanlines") > 0.0F || number("gradeSplit") > 0.0F
                                       || number("gradeGlow") > 0.0F},
    };
}

void LumenField::updateCategoryRows()
{
    QVariantList rows;
    const bool content = contentSource();
    for (const auto &c : content ? std::span<const Category>(contentCategoryTable) : std::span<const Category>(categoryTable)) {
        const QString key = QString::fromLatin1(c.key);
        rows.push_back(QVariantMap{
            {QStringLiteral("key"), key},
            {QStringLiteral("label"), QString::fromUtf8(c.label)},
            {QStringLiteral("color"), QColor(m_settings.value(QStringLiteral("color") + key).toString())},
            {QStringLiteral("count"), m_counts.value(key).toInt()},
            {QStringLiteral("hidden"), m_hiddenCategories.contains(key)},
        });
    }
    m_categoryRows = rows;
    emit categoriesChanged();
}

void LumenField::setStatus(const QString &status, const QString &error)
{
    if (status == m_status && error == m_error)
        return;
    m_status = status;
    m_error = error;
    emit statusChanged();
}

void LumenField::accept(const QByteArray &body)
{
    QJsonParseError parseError;
    const QJsonDocument document = QJsonDocument::fromJson(body, &parseError);
    if (parseError.error != QJsonParseError::NoError || !document.isObject()) {
        setStatus(uiText(QStringLiteral("SIN CAMPO"), QStringLiteral("NO FIELD")), uiText(QStringLiteral("escena ilegible: "), QStringLiteral("unreadable scene: ")) + parseError.errorString());
        return;
    }
    const QJsonObject object = document.object();
    const QJsonArray nodeArray = object.value(QStringLiteral("nodes")).toArray();

    QHash<QString, std::pair<Vec3, Vec3>> previous;
    for (std::size_t i = 0; i < m_nodes.size() && i < m_bodies.size(); ++i)
        previous.insert(m_nodes[i].id, {m_bodies[i].position, m_bodies[i].velocity});

    std::vector<Node> nodes;
    nodes.reserve(static_cast<std::size_t>(nodeArray.size()));
    QHash<QString, int> index;
    for (const auto &value : nodeArray) {
        const QJsonObject o = value.toObject();
        Node node;
        node.id = o.value(QStringLiteral("id")).toString();
        if (node.id.isEmpty() || index.contains(node.id))
            continue;
        const QJsonArray p = o.value(QStringLiteral("position")).toArray();
        if (p.size() != 3)
            continue;
        node.anchor = QVector3D(static_cast<float>(p[0].toDouble()), static_cast<float>(p[1].toDouble()),
                                static_cast<float>(p[2].toDouble()));
        if (!std::isfinite(node.anchor.x()) || !std::isfinite(node.anchor.y()) || !std::isfinite(node.anchor.z()))
            continue;
        node.name = o.value(QStringLiteral("name")).toString();
        node.parent = o.value(QStringLiteral("parent")).toString();
        node.kind = o.value(QStringLiteral("kind")).toString();
        node.category = o.value(QStringLiteral("category")).toString();
        node.role = o.value(QStringLiteral("role")).toString();
        node.entityKind = o.value(QStringLiteral("entity_kind")).toString();
        node.status = o.value(QStringLiteral("status")).toString();
        node.memberCount = std::max(1, o.value(QStringLiteral("member_count")).toInt(1));
        node.depth = o.value(QStringLiteral("depth")).toInt();
        const auto unit = [&o](const char *key) {
            const double v = o.value(QLatin1String(key)).toDouble();
            return std::isfinite(v) ? std::clamp(static_cast<float>(v), 0.0F, 1.0F) : 0.0F;
        };
        node.size = unit("size");
        node.activity = unit("activity");
        node.focus = unit("focus");
        node.voice = unit("voice");
        node.uncertainty = unit("uncertainty");
        node.selected = o.value(QStringLiteral("selected")).toBool();
        node.expandable = o.value(QStringLiteral("expandable")).toBool();
        node.expanded = o.value(QStringLiteral("expanded")).toBool();
        for (const auto &s : o.value(QStringLiteral("signals")).toArray())
            node.signalNames.push_back(s.toString());
        node.branch = node.kind != QLatin1String("entity");
        index.insert(node.id, static_cast<int>(nodes.size()));
        nodes.push_back(std::move(node));
    }
    bool structure = nodes.size() != m_nodes.size();
    for (std::size_t i = 0; !structure && i < nodes.size(); ++i)
        structure = nodes[i].id != m_nodes[i].id || nodes[i].anchor != m_nodes[i].anchor
                    || nodes[i].memberCount != m_nodes[i].memberCount;
    for (Node &node : nodes)
        node.parentIndex = index.value(node.parent, -1);

    // A field with nothing in common with the last one (the first, or a new lens) is
    // projected: every orb starts at the centre and springs out to its place.
    bool fresh = true;
    for (const Node &node : nodes)
        if (previous.contains(node.id))
            fresh = false;
    std::vector<lumen::Body> bodies;
    bodies.reserve(nodes.size());
    for (const Node &node : nodes) {
        lumen::Body body;
        body.anchor = toV(node.anchor);
        body.fixed = node.kind == QLatin1String("root");
        if (const auto it = previous.constFind(node.id); it != previous.constEnd()) {
            body.position = it->first;
            body.velocity = it->second;
        } else if (const auto parent = previous.constFind(node.parent); parent != previous.constEnd()) {
            // A newly opened member grows out of the module that held it.
            body.position = parent->first;
        } else {
            body.position = fresh ? Vec3{} : body.anchor;
        }
        bodies.push_back(body);
    }

    std::vector<Edge> relations;
    for (const auto &value : object.value(QStringLiteral("relations")).toArray()) {
        const QJsonObject o = value.toObject();
        Edge edge;
        edge.source = index.value(o.value(QStringLiteral("source")).toString(), -1);
        edge.target = index.value(o.value(QStringLiteral("target")).toString(), -1);
        if (edge.source < 0 || edge.target < 0)
            continue;
        edge.kind = o.value(QStringLiteral("kind")).toString();
        edge.evidence = o.value(QStringLiteral("evidence")).toString();
        edge.count = o.value(QStringLiteral("count")).toInt();
        edge.strength = static_cast<float>(o.value(QStringLiteral("strength")).toDouble());
        for (const auto &r : o.value(QStringLiteral("relations")).toArray())
            edge.relations.push_back(r.toString());
        relations.push_back(std::move(edge));
    }

    m_nodes = std::move(nodes);
    m_bodies = std::move(bodies);
    m_index = std::move(index);
    m_relations = std::move(relations);
    const double radius = object.value(QStringLiteral("radius")).toDouble(200.0);
    m_fieldRadius = std::isfinite(radius) && radius > 1.0 ? static_cast<float>(radius) : 200.0F;
    m_rootId = object.value(QStringLiteral("root_id")).toString();
    m_focusId = object.value(QStringLiteral("focus_id")).toString();
    m_focusPath.clear();
    for (const auto &link : object.value(QStringLiteral("focus_path")).toArray())
        m_focusPath.push_back(link.toObject().value(QStringLiteral("source")).toString());
    m_counts.clear();
    for (const auto &count : object.value(QStringLiteral("counts")).toArray()) {
        const QJsonObject o = count.toObject();
        m_counts.insert(o.value(QStringLiteral("category")).toString(), o.value(QStringLiteral("count")).toInt());
    }

    int observed = 0;
    int semantic = 0;
    for (const Edge &edge : m_relations)
        (edge.kind == QLatin1String("observed") ? observed : semantic) += 1;
    const QJsonObject state = object.value(QStringLiteral("state")).toObject();
    m_sceneInfo = {
        {QStringLiteral("lens"), m_lens},
        {QStringLiteral("rootId"), m_rootId},
        {QStringLiteral("focusId"), m_focusId},
        {QStringLiteral("focusName"), indexOf(m_focusId) >= 0 ? m_nodes[static_cast<std::size_t>(indexOf(m_focusId))].name : QString()},
        {QStringLiteral("canGoUp"), !m_focusPath.isEmpty()},
        {QStringLiteral("sourceCount"), object.value(QStringLiteral("source_count")).toInt()},
        {QStringLiteral("collapsedCount"), object.value(QStringLiteral("collapsed_count")).toInt()},
        {QStringLiteral("entityCount"), object.value(QStringLiteral("entity_count")).toInt()},
        {QStringLiteral("nodeCount"), static_cast<int>(m_nodes.size())},
        {QStringLiteral("observedCount"), observed},
        {QStringLiteral("semanticCount"), semantic},
        {QStringLiteral("staleSignals"), object.value(QStringLiteral("stale_signals")).toInt()},
        {QStringLiteral("profile"), object.value(QStringLiteral("profile")).toString()},
        {QStringLiteral("listen"), state.value(QStringLiteral("listen")).toDouble()},
        {QStringLiteral("think"), state.value(QStringLiteral("think")).toDouble()},
        {QStringLiteral("speak"), state.value(QStringLiteral("speak")).toDouble()},
        {QStringLiteral("audio"), state.value(QStringLiteral("audio_envelope")).toDouble()},
        {QStringLiteral("workflows"), object.value(QStringLiteral("workflows")).toArray().toVariantList()},
    };
    // The content sphere says what it lists, as the root label of Xavi's desktop did
    // ("ARCHIEOS-NVIDIA · 2499 ELEMENTOS · 3 GB").
    if (const QJsonObject content = object.value(QStringLiteral("content")).toObject(); !content.isEmpty()) {
        const QString named = qEnvironmentVariable("ARCHIE_CONTENT_NAME");
        m_sceneInfo.insert(QStringLiteral("contentName"), named.isEmpty() ? content.value(QStringLiteral("name")).toString() : named);
        m_sceneInfo.insert(QStringLiteral("contentElements"), content.value(QStringLiteral("elements")).toDouble());
        m_sceneInfo.insert(QStringLiteral("contentBytes"), content.value(QStringLiteral("bytes")).toDouble());
        m_sceneInfo.insert(QStringLiteral("contentTruncated"), content.value(QStringLiteral("truncated")).toBool());
    }

    QVariantList modules;
    for (const Node &node : m_nodes) {
        if ((node.kind != QLatin1String("module") && node.kind != QLatin1String("group")) || modules.size() >= 80)
            continue;
        modules.push_back(QVariantMap{
            {QStringLiteral("id"), node.id}, {QStringLiteral("name"), node.name},
            {QStringLiteral("role"), node.role}, {QStringLiteral("kind"), node.kind},
            {QStringLiteral("memberCount"), node.memberCount}, {QStringLiteral("expanded"), node.expanded},
            {QStringLiteral("expandable"), node.expandable},
            {QStringLiteral("color"), QColor(m_settings.value(QStringLiteral("color") + node.category).toString())},
        });
    }
    m_modules = modules;
    QVariantList tree;
    if (const int f = indexOf(m_focusId); f >= 0)
        tree.push_back(QVariantMap{{QStringLiteral("id"), m_focusId},
                                   {QStringLiteral("name"), m_nodes[static_cast<std::size_t>(f)].name},
                                   {QStringLiteral("current"), true}});
    for (const Node &node : m_nodes)
        if (node.parent == m_focusId && node.kind == QLatin1String("module") && tree.size() < 81)
            tree.push_back(QVariantMap{{QStringLiteral("id"), node.id}, {QStringLiteral("name"), node.name},
                                       {QStringLiteral("current"), false}});
    m_tree = tree;

    if (!m_pendingReveal.isEmpty() && indexOf(m_pendingReveal) >= 0) {
        m_selected = m_pendingReveal;
        m_pendingReveal.clear();
        emit focusRequested(toQ(m_bodies[static_cast<std::size_t>(indexOf(m_selected))].anchor));
    }
    if (!m_selected.isEmpty() && indexOf(m_selected) < 0)
        m_selected.clear();
    if (!m_hoveredId.isEmpty() && indexOf(m_hoveredId) < 0) {
        m_hoveredId.clear();
        updateHoveredInfo();
    }
    if (structure) {
        m_structureDirty = true;
        rebuildMembrane();
        rebuildGhosts();
    }
    updateAttention();
    select(m_selected);
    updateLook();
    updateCategoryRows();
    if (fresh && !m_nodes.empty()) {
        m_projection.start();
        emit projectionStarted();
    }
    if (m_sceneInfo.contains(QStringLiteral("contentName")))
        setStatus(uiText(QStringLiteral("CONTENIDO · "), QStringLiteral("CONTENT · ")) + m_sceneInfo.value(QStringLiteral("contentName")).toString().toUpper());
    else
        setStatus(m_lens == QLatin1String("entities") ? uiText(QStringLiteral("ENTIDADES · UNA POR OBSERVACIÓN"), QStringLiteral("ENTITIES · ONE PER OBSERVATION"))
                                                      : uiText(QStringLiteral("ESTRUCTURA + RELACIONES OBSERVADAS"), QStringLiteral("STRUCTURE + OBSERVED RELATIONS")));
    emit sceneChanged();
}

void LumenField::rebuildStatic()
{
    // The backbone: tree links, sampled so a large field stays legible.
    std::vector<int> linked;
    std::vector<bool> branch;
    for (std::size_t i = 0; i < m_nodes.size(); ++i) {
        const Node &node = m_nodes[i];
        if (node.parentIndex < 0 || !drawn(static_cast<int>(i)) || !drawn(node.parentIndex))
            continue;
        linked.push_back(static_cast<int>(i));
        branch.push_back(node.branch);
    }
    m_backbone.clear();
    for (const std::size_t k : lumen::backbone(branch, static_cast<std::size_t>(number("linkBudget"))))
        m_backbone.push_back(linked[k]);

    // Satellites: density around each module or group, never an entity of their own.
    m_satellites.clear();
    for (std::size_t i = 0; i < m_nodes.size(); ++i) {
        const Node &node = m_nodes[i];
        if ((node.kind != QLatin1String("module") && node.kind != QLatin1String("group")) || !drawn(static_cast<int>(i)))
            continue;
        const int count = lumen::satellite_budget(static_cast<std::size_t>(node.memberCount));
        const float spread = lumen::satellite_radius(static_cast<std::size_t>(node.memberCount), m_fieldRadius);
        const auto offsets = lumen::satellite_offsets(count, spread, seedOf(node.id) & 0xffffU);
        for (int k = 0; k < count && static_cast<int>(m_satellites.size()) < maximumSatellites; ++k) {
            const float jitter = lumen::hash01(static_cast<std::uint32_t>(m_satellites.size()) * 17U + 2U);
            m_satellites.push_back(Satellite{static_cast<int>(i), toQ(offsets[static_cast<std::size_t>(k)]),
                                             (0.34F + jitter * 0.42F) * (node.kind == QLatin1String("group") ? 0.81F : 1.0F)});
        }
    }

    if (const auto guides = m_geometry.value(QStringLiteral("guides")); guides) {
        const auto g = lumen::guides(m_fieldRadius);
        std::vector<float> points;
        points.reserve(g.points.size() * 3);
        for (const Vec3 &p : g.points)
            points.insert(points.end(), {p.x, p.y, p.z});
        guides->setLines(points);
    }

    // The hologram's frame (Xavi's desktop): orbit rings, dotted rim, projector and beam.
    const auto frame = flag("holoFrame") ? lumen::holo_frame(std::max(m_shellRadius, m_fieldRadius * 0.6F))
                                         : lumen::HoloFrame{};
    const auto upload = [this](const char *name, const std::vector<Vec3> &segments) {
        if (const auto g = m_geometry.value(QLatin1String(name)); g) {
            std::vector<float> points;
            points.reserve(segments.size() * 3);
            for (const Vec3 &p : segments)
                points.insert(points.end(), {p.x, p.y, p.z});
            g->setLines(points);
        }
    };
    upload("holoRings", frame.rings);
    upload("holoBeam", frame.beam);
    upload("holoCore", frame.core);
    m_structureDirty = false;
}

void LumenField::rebuildMembrane()
{
    const auto target = m_geometry.value(QStringLiteral("membrane"));
    if (!target)
        return;
    std::vector<Vec3> points;
    std::vector<lumen::Rgb> colours;
    std::vector<XaviShellNode> shellNodes;
    for (std::size_t i = 0; i < m_nodes.size(); ++i) {
        if (m_nodes[i].kind == QLatin1String("root") || !drawn(static_cast<int>(i)))
            continue;
        points.push_back(toV(m_nodes[i].anchor));
        colours.push_back(colourOf(m_nodes[i].category));
        const auto color=colours.back();
        shellNodes.push_back({points.back(),{color.r,color.g,color.b},m_nodes[i].branch});
    }
    // The design's shells take at most the 420 orbs of an observed field; a content
    // sphere of thousands of files gives them an even sample of its nodes.
    if (shellNodes.size() > 420) {
        std::vector<XaviShellNode> sample;
        sample.reserve(420);
        for (std::size_t k = 0; k < 420; ++k)
            sample.push_back(shellNodes[k * shellNodes.size() / 420]);
        shellNodes = std::move(sample);
    }
    const auto warm = lumen::to_linear(lumen::parse_hex(m_settings.value(QStringLiteral("accentWarm")).toString().toStdString())
                                           .value_or(lumen::Rgb{0.9F, 0.7F, 0.5F}));
    const bool legacyLayout = m_settings.value(QStringLiteral("layoutMode")).toString() == QLatin1String("legacy");
    const auto edges=m_geometry.value(QStringLiteral("membraneEdges"));
    if(!legacyLayout){
        const auto parse=[this](const char *key){const auto c=lumen::to_linear(lumen::parse_hex(m_settings.value(QLatin1String(key)).toString().toStdString()).value_or(lumen::Rgb{.5F,.7F,.8F}));return Vec3{c.r,c.g,c.b};};
        const int kind=xaviChoices(QStringLiteral("shellStyle")).indexOf(m_settings.value(QStringLiteral("shellStyle")).toString());
        std::vector<float> vertices;std::vector<std::uint32_t> indices;
        if(xaviShell(kind,shellNodes,m_fieldRadius,number("shellDensity"),number("shellScale"),
            Vec3{warm.r,warm.g,warm.b},parse("accentCool"),parse("shellTint"),vertices,indices,m_shellRadius)){
            target->setTinted(vertices,indices);
            if(edges){
                std::vector<float> lines;
                if(kind==0)for(std::size_t k=0;k+2<indices.size();k+=3)for(int j=0;j<3;++j){
                    const auto a=std::size_t(indices[k+j])*10,b=std::size_t(indices[k+(j+1)%3])*10;
                    lines.insert(lines.end(),{vertices[a],vertices[a+1],vertices[a+2],vertices[b],vertices[b+1],vertices[b+2]});
                }
                edges->setLines(lines);
            }
            return;
        }
    }
    if(edges)edges->setLines({});
    const auto shell = lumen::membrane(points, colours, warm, m_fieldRadius, legacyLayout, number("shellScale"));
    m_shellRadius = shell.radius;
    target->setTinted(shell.vertices, shell.indices);
}

void LumenField::rebuildGhosts()
{
    std::vector<Vec3> centres;
    std::vector<std::size_t> members;
    for (std::size_t i = 0; i < m_nodes.size(); ++i) {
        const Node &node = m_nodes[i];
        if ((node.kind == QLatin1String("module") || node.kind == QLatin1String("group")) && drawn(static_cast<int>(i))) {
            centres.push_back(toV(node.anchor));
            members.push_back(static_cast<std::size_t>(node.memberCount));
        }
    }
    const int count = flag("ambient") && ambientAllowed()
        ? std::min({3500, static_cast<int>(number("ambientCount")), particleBudget()}) : 0;
    m_ghostOffsets = lumen::ghosts(centres, members, count, m_fieldRadius);
    const auto table = m_tables.value(QStringLiteral("ghosts"));
    if (!table)
        return;
    const auto warm = lumen::to_linear(*lumen::parse_hex("#e8b47d"));
    const auto cool = lumen::to_linear(*lumen::parse_hex("#78c2ca"));
    const auto grey = lumen::to_linear(*lumen::parse_hex("#7e9092"));
    const auto accentWarm = lumen::to_linear(lumen::parse_hex(m_settings.value(QStringLiteral("accentWarm")).toString().toStdString()).value_or(warm));
    const auto accentCool = lumen::to_linear(lumen::parse_hex(m_settings.value(QStringLiteral("accentCool")).toString().toStdString()).value_or(cool));
    QByteArray data;
    data.reserve(static_cast<qsizetype>(m_ghostOffsets.size() * sizeof(QQuick3DInstancing::InstanceTableEntry)));
    for (std::size_t i = 0; i < m_ghostOffsets.size(); ++i) {
        const lumen::Rgb tint = lumen::mix(i % 5 == 0 ? accentCool : accentWarm, grey, 0.58F);
        entry(data, toQ(m_ghostOffsets[i]), 1.0F, tint, 1.0F,
              QVector4D(lumen::hash01(static_cast<std::uint32_t>(i)), 0, 0, 0));
    }
    table->setTable(data, static_cast<int>(m_ghostOffsets.size()));
}

void LumenField::updateHighlight()
{
    m_highlightNodes.clear();
    m_highlightRelations.clear();
    // What a person points at wins; otherwise what the system is using is lit: each
    // engaged node, the way from it to the centre, and the relations it takes part in.
    const int pointed = indexOf(m_selected.isEmpty() ? m_hoveredId : m_selected);
    const std::vector<int> focuses = pointed >= 0 ? std::vector<int>{pointed} : m_attention;
    const auto budget = static_cast<std::size_t>(number("highlightBudget"));
    std::vector<int> parents;
    parents.reserve(m_nodes.size());
    for (const Node &node : m_nodes)
        parents.push_back(node.parentIndex);
    QSet<int> seenNodes;
    QSet<int> seenRelations;
    for (const int focus : focuses) {
        const std::size_t share = pointed >= 0 ? budget : std::max<std::size_t>(4, budget / std::max<std::size_t>(1, focuses.size()));
        for (const std::size_t i : lumen::neighbourhood(parents, focus, share))
            if (drawn(static_cast<int>(i)) && drawn(parents[i]) && !seenNodes.contains(static_cast<int>(i))) {
                seenNodes.insert(static_cast<int>(i));
                m_highlightNodes.push_back(static_cast<int>(i));
            }
        for (std::size_t r = 0; r < m_relations.size() && m_highlightNodes.size() + m_highlightRelations.size() < budget; ++r)
            if ((m_relations[r].source == focus || m_relations[r].target == focus) && !seenRelations.contains(static_cast<int>(r))) {
                seenRelations.insert(static_cast<int>(r));
                m_highlightRelations.push_back(static_cast<int>(r));
            }
    }
    m_pulseLinks.clear();
    for (const int i : m_highlightNodes)
        m_pulseLinks.emplace_back(m_nodes[static_cast<std::size_t>(i)].parentIndex, i);
    for (const int r : m_highlightRelations)
        m_pulseLinks.emplace_back(m_relations[static_cast<std::size_t>(r)].source, m_relations[static_cast<std::size_t>(r)].target);
    if (m_pulseLinks.empty()) {
        // At rest, signals travel the observed relations first, then the backbone.
        for (const Edge &edge : m_relations)
            if (edge.kind == QLatin1String("observed") && m_pulseLinks.size() < 18)
                m_pulseLinks.emplace_back(edge.source, edge.target);
        for (std::size_t k = 0; k < m_backbone.size() && m_pulseLinks.size() < 18; ++k)
            m_pulseLinks.emplace_back(m_nodes[static_cast<std::size_t>(m_backbone[k])].parentIndex, m_backbone[k]);
    }
    if (m_pulseLinks.size() > static_cast<std::size_t>(maximumPulses))
        m_pulseLinks.resize(maximumPulses);
}

void LumenField::updateLayers()
{
    if (m_structureDirty)
        rebuildStatic();
    const float nodeSize = number("nodeSize");
    const auto warm = lumen::to_linear(lumen::parse_hex(m_settings.value(QStringLiteral("accentWarm")).toString().toStdString())
                                           .value_or(lumen::Rgb{0.9F, 0.7F, 0.5F}));

    QByteArray modules;
    QByteArray entities;
    int moduleCount = 0;
    int entityCount = 0;
    for (std::size_t i = 0; i < m_nodes.size(); ++i) {
        const int at = static_cast<int>(i);
        if (!drawn(at))
            continue;
        const Node &node = m_nodes[i];
        lumen::Rgb colour = colourOf(node.category);
        // The centre is Archie itself: a warm core. Modules keep most of their own hue.
        if (node.kind == QLatin1String("root"))
            colour = lumen::mix(colour, warm, 0.75F);
        else if (node.branch)
            colour = lumen::mix(colour, warm, 0.1F);
        float flags = statusCode(node.status) * 4.0F;
        if (node.id == m_selected)
            flags += 1.0F;
        if (node.id == m_hoveredId)
            flags += 2.0F;
        const bool designMaterial=xaviStyle(m_settings.value(node.branch?QStringLiteral("visualFolderStyle"):QStringLiteral("visualNodeStyle")).toString())>0;
        if(designMaterial && (node.id==m_selected || std::find(m_highlightNodes.begin(),m_highlightNodes.end(),at)!=m_highlightNodes.end()))
            flags+=32.0F;
        const QVector4D data(std::max(node.activity, node.uncertainty * 0.3F), attentionOf(at), flags,
                             static_cast<float>(seedOf(node.id) & 1023U) / 1023.0F);
        if (node.branch) {
            entry(modules, positionAt(at), orbScale(at), colour, 1.0F, data);
            ++moduleCount;
        } else {
            entry(entities, positionAt(at), orbScale(at), colour, 1.0F, data);
            ++entityCount;
        }
    }
    if (const auto t = m_tables.value(QStringLiteral("modules")); t)
        t->setTable(modules, moduleCount);
    if (const auto t = m_tables.value(QStringLiteral("entities")); t)
        t->setTable(entities, entityCount);

    QByteArray satellites;
    std::vector<float> fibres;
    satellites.reserve(static_cast<qsizetype>(m_satellites.size() * sizeof(QQuick3DInstancing::InstanceTableEntry)));
    fibres.reserve(m_satellites.size() * 6);
    int satelliteCount = 0;
    for (const Satellite &s : m_satellites) {
        if (!drawn(s.centre))
            continue;
        const QVector3D centre = positionAt(s.centre);
        const QVector3D at = centre + s.offset;
        const Node &owner = m_nodes[static_cast<std::size_t>(s.centre)];
        entry(satellites, at, s.size * nodeSize, colourOf(owner.category), 1.0F,
              QVector4D(owner.activity, 0.0F, 0.0F, lumen::hash01(static_cast<std::uint32_t>(satelliteCount))));
        fibres.insert(fibres.end(), {centre.x(), centre.y(), centre.z(), at.x(), at.y(), at.z()});
        ++satelliteCount;
    }
    if (const auto t = m_tables.value(QStringLiteral("satellites")); t)
        t->setTable(satellites, satelliteCount);
    if (const auto g = m_geometry.value(QStringLiteral("fibres")); g)
        g->setLines(fibres);

    const bool curved = m_lens != QLatin1String("entities") && xaviLink(m_settings.value(QStringLiteral("visualLinkStyle")).toString())==0;
    const int segments = std::max(1, static_cast<int>(number("linkSegments")));
    std::vector<float> backbone;
    if (contentSource() && curved) {
        // The content sphere's links are filaments: smooth, wavy strands, as on Xavi's desktop.
        backbone.reserve(m_backbone.size() * static_cast<std::size_t>(segments) * 6);
        for (const int i : m_backbone) {
            const Vec3 a = toV(positionAt(m_nodes[static_cast<std::size_t>(i)].parentIndex));
            const Vec3 b = toV(positionAt(i));
            const std::uint32_t seed = seedOf(m_nodes[static_cast<std::size_t>(i)].id);
            Vec3 previous = a;
            for (int s = 1; s <= segments; ++s) {
                const Vec3 next = lumen::filament(a, b, static_cast<float>(s) / static_cast<float>(segments), seed);
                backbone.insert(backbone.end(), {previous.x, previous.y, previous.z, next.x, next.y, next.z});
                previous = next;
            }
        }
    } else {
        for (const int i : m_backbone)
            appendCurve(backbone, positionAt(m_nodes[static_cast<std::size_t>(i)].parentIndex), positionAt(i), curved ? segments : 1, false);
    }
    if (const auto g = m_geometry.value(QStringLiteral("backbone")); g)
        g->setLines(backbone);

    std::vector<float> highlight;
    for (const int i : m_highlightNodes)
        appendCurve(highlight, positionAt(m_nodes[static_cast<std::size_t>(i)].parentIndex), positionAt(i), curved ? 9 : 1, false);
    for (const int r : m_highlightRelations) {
        const Edge &edge = m_relations[static_cast<std::size_t>(r)];
        appendCurve(highlight, positionAt(edge.source), positionAt(edge.target), curved ? 9 : 1, false);
    }
    if (const auto g = m_geometry.value(QStringLiteral("highlight")); g)
        g->setLines(highlight);

    std::vector<float> observed;
    std::vector<float> semantic;
    for (const Edge &edge : m_relations) {
        if (!drawn(edge.source) || !drawn(edge.target))
            continue;
        const bool isObserved = edge.kind == QLatin1String("observed");
        appendCurve(isObserved ? observed : semantic, positionAt(edge.source), positionAt(edge.target), curved ? 9 : 1, !isObserved && curved);
    }
    if (const auto g = m_geometry.value(QStringLiteral("observed")); g)
        g->setLines(observed);
    if (const auto g = m_geometry.value(QStringLiteral("semantic")); g)
        g->setLines(semantic);

    const int selected = indexOf(m_selected);
    m_selectionVisible = selected >= 0;
    if (selected >= 0) {
        m_selectionPosition = positionAt(selected);
        m_selectionScale = nodeSize * (m_nodes[static_cast<std::size_t>(selected)].branch ? 4.7F : 2.5F);
    }
}

void LumenField::updatePulses()
{
    const bool designLinks=xaviLink(m_settings.value(QStringLiteral("visualLinkStyle")).toString())>0;
    if(const auto ribbon=m_geometry.value(QStringLiteral("pulseRibbons"));ribbon){
        std::vector<float> lines;
        if(designLinks && flag("pulses") && ambientAllowed() && particleBudget()>0)
            for(const auto &[from,to]:m_pulseLinks)if(drawn(from)&&drawn(to))appendCurve(lines,positionAt(from),positionAt(to),1,false);
        ribbon->setLines(lines);
    }
    const auto table = m_tables.value(QStringLiteral("pulses"));
    if (!table)
        return;
    QByteArray data;
    int count = 0;
    if (!designLinks && flag("pulses") && ambientAllowed() && particleBudget() > 0) {
        const auto warm = lumen::to_linear(lumen::parse_hex(m_settings.value(QStringLiteral("accentWarm")).toString().toStdString())
                                               .value_or(lumen::Rgb{0.9F, 0.7F, 0.5F}));
        const bool relation=!m_selected.isEmpty()||!m_hoveredId.isEmpty();
        const float speed = number("pulseSpeed")*number(relation?"relationFlowSpeed":"ambientFlowSpeed");
        for (std::size_t a = 0; a < m_pulseLinks.size(); ++a) {
            const auto [from, to] = m_pulseLinks[a];
            if (!drawn(from) || !drawn(to))
                continue;
            const float phase = std::fmod(m_pulseTime * speed + static_cast<float>(a) * 0.113F, 1.0F);
            const QVector3D p = positionAt(from) + (positionAt(to) - positionAt(from)) * phase;
            const float intensity=number("energyIntensity")*number(relation?"relationFlowIntensity":"ambientFlowIntensity");
            entry(data, p, number("nodeSize") * number("energyWidth") * 1.3F, warm, std::clamp(intensity,0.0F,1.0F), QVector4D(phase, 0, 0, 0));
            ++count;
        }
    }
    table->setTable(data, count);
}

void LumenField::tick()
{
    const float dt = std::min(0.05F, static_cast<float>(m_clock.restart()) / 1000.0F);
    const bool inspecting = !m_selected.isEmpty() && flag("pauseOnInspect");
    if (!m_paused && ambientAllowed()) {
        m_time += dt;
        m_pulseTime += dt;
        if (flag("rotate") && !inspecting)
            m_spin = std::fmod(m_spin + dt * number("rotationSpeed") * 0.1F * 57.29578F, 360.0F);
    }
    if (flag("physics") && !m_paused && ambientAllowed() && m_simulationActive) {
        lumen::Motion motion;
        motion.reaction = number("reaction");
        motion.reach = number("reach");
        motion.spring = number("spring");
        // A projection eases in: the pull starts soft and reaches full strength over
        // 2.2 s, so the field unfolds rather than snaps into place.
        if (m_projection.isValid() && m_projection.elapsed() < 2200) {
            const float t = static_cast<float>(m_projection.elapsed()) / 2200.0F;
            motion.spring *= 0.06F + 0.94F * t * t;
        }
        motion.damping = number("damping");
        motion.radius = m_fieldRadius * 1.12F;
        std::optional<Vec3> pointer;
        if (const int h = indexOf(m_hoveredId); h >= 0)
            pointer = m_bodies[static_cast<std::size_t>(h)].position;
        const unsigned changes = xaviStep(m_bodies, pointer, dt, motion);
        if ((changes & 1U) != 0) m_positionsDirty = true;
        m_simulationActive = (changes & 2U) != 0;
    } else if (!flag("physics") || !ambientAllowed()) {
        for (lumen::Body &body : m_bodies) {
            if (!(body.position == body.anchor)) {
                body.position = body.anchor;
                body.velocity = Vec3{};
                m_positionsDirty = true;
            }
        }
    }
    if (m_positionsDirty || m_structureDirty) {
        updateLayers();
        m_positionsDirty = false;
    }
    updatePulses();
    emit frameAdvanced();
    ++m_framesCounted;
    if (m_fpsClock.elapsed() >= 1000) {
        m_fps = static_cast<int>(std::lround(m_framesCounted * 1000.0 / static_cast<double>(m_fpsClock.restart())));
        m_framesCounted = 0;
        emit fpsChanged();
    }
    if (!ambientAllowed() || m_paused) m_frameTimer.stop();
}

void LumenField::updateSelectionInfo()
{
    const int i = indexOf(m_selected);
    if (i < 0) {
        m_selectionInfo.clear();
        emit selectionChanged();
        return;
    }
    const Node &node = m_nodes[static_cast<std::size_t>(i)];
    QVariantList attached;
    for (const Edge &edge : m_relations) {
        if (attached.size() >= 14 || (edge.source != i && edge.target != i))
            continue;
        const int other = edge.source == i ? edge.target : edge.source;
        attached.push_back(QVariantMap{
            {QStringLiteral("kind"), edge.kind == QLatin1String("observed") ? uiText(QStringLiteral("OBSERVADA"), QStringLiteral("OBSERVED")) : uiText(QStringLiteral("SEMÁNTICA"), QStringLiteral("SEMANTIC"))},
            {QStringLiteral("id"), m_nodes[static_cast<std::size_t>(other)].id},
            {QStringLiteral("name"), m_nodes[static_cast<std::size_t>(other)].name},
            {QStringLiteral("evidence"), edge.evidence},
            {QStringLiteral("relations"), edge.relations.join(QStringLiteral(" · "))},
            {QStringLiteral("count"), edge.count},
        });
    }
    const QString tag = node.kind == QLatin1String("root") ? uiText(QStringLiteral("RAÍZ"), QStringLiteral("ROOT"))
                        : node.kind == QLatin1String("group") ? uiText(QStringLiteral("GRUPO DE ENTIDADES"), QStringLiteral("ENTITY GROUP"))
                        : node.kind == QLatin1String("module") ? uiText(QStringLiteral("MÓDULO OBSERVADO"), QStringLiteral("OBSERVED MODULE"))
                                                                : uiText(QStringLiteral("ENTIDAD OBSERVADA"), QStringLiteral("OBSERVED ENTITY"));
    m_selectionInfo = {
        {QStringLiteral("id"), node.id},
        {QStringLiteral("name"), node.name},
        {QStringLiteral("tag"), tag},
        {QStringLiteral("kind"), node.kind},
        {QStringLiteral("entityKind"), node.entityKind.isEmpty() ? QStringLiteral("—") : node.entityKind},
        {QStringLiteral("category"), categoryLabel(node.category)},
        {QStringLiteral("role"), node.role},
        {QStringLiteral("memberCount"), node.memberCount},
        {QStringLiteral("depth"), node.depth},
        {QStringLiteral("activity"), node.activity},
        {QStringLiteral("focus"), node.focus},
        {QStringLiteral("voice"), node.voice},
        {QStringLiteral("uncertainty"), node.uncertainty},
        {QStringLiteral("status"), node.status},
        {QStringLiteral("signals"), node.signalNames},
        {QStringLiteral("relations"), attached},
        {QStringLiteral("expandable"), node.expandable || node.expanded},
        {QStringLiteral("expanded"), node.expanded},
        {QStringLiteral("explorable"), node.kind == QLatin1String("module")},
        {QStringLiteral("isRoot"), node.kind == QLatin1String("root")},
    };
    emit selectionChanged();
}

void LumenField::updateHoveredInfo()
{
    const int i = indexOf(m_hoveredId);
    if (i < 0) {
        m_hoveredInfo.clear();
    } else {
        const Node &node = m_nodes[static_cast<std::size_t>(i)];
        m_hoveredInfo = {{QStringLiteral("id"), node.id},
                         {QStringLiteral("name"), node.name},
                         {QStringLiteral("label"), node.kind == QLatin1String("group") ? uiText(QStringLiteral("Grupo"), QStringLiteral("Group"))
                                                   : node.kind == QLatin1String("module") ? uiText(QStringLiteral("Módulo"), QStringLiteral("Module"))
                                                   : node.kind == QLatin1String("root") ? uiText(QStringLiteral("Raíz"), QStringLiteral("Root"))
                                                                                         : categoryLabel(node.category)}};
    }
    emit hoveredChanged();
}
