#include "NeuralField.h"
#include "UiText.h"

#include <QColor>
#include <QCoreApplication>
#include <QDir>
#include <QDateTime>
#include <QEventLoop>
#include <QFutureWatcher>
#include <QJsonDocument>
#include <QJsonObject>
#include <QtConcurrent>

#include <algorithm>
#include <cmath>
#include <limits>
#include <memory>

namespace {
// The original's RNG seed for the sparks (`?still` and live alike).
constexpr quint32 kSeed = 7;
constexpr double kDegrees = 180.0 / M_PI;

float channel(float srgb)
{
    return srgb <= 0.04045F ? srgb / 12.92F : std::pow((srgb + 0.055F) / 1.055F, 2.4F);
}

/// Rust's string queries: ask for the length, then fill a buffer one byte longer (the NUL).
template <class Query>
QString text(Query &&query)
{
    const std::size_t length = query(nullptr, 0);
    if (length == 0)
        return {};
    QByteArray bytes(static_cast<qsizetype>(length) + 1, '\0');
    if (query(bytes.data(), static_cast<std::size_t>(bytes.size())) != length)
        return {};
    return QString::fromUtf8(bytes.constData(), static_cast<qsizetype>(length));
}
} // namespace

NeuralField::NeuralField(QObject *parent)
    : QObject(parent)
{
    // The fog's colour is the background's, in linear light, before any sphere exists.
    setBackground(m_background);
}

NeuralField::~NeuralField()
{
    ++m_generation;
    release();
}

bool NeuralField::building() const
{
    return m_sphere && neural_building(m_sphere);
}

void NeuralField::setRoot(const QString &root)
{
    const QString cleaned = root.isEmpty() ? root : QDir::cleanPath(QDir(root).absolutePath());
    if (cleaned == m_root)
        return;
    m_root = cleaned;
    // A different root is a different domain: never draw the old domain under its name.
    release();
    publish();
    emit rootChanged();
    load();
}

void NeuralField::setActive(bool active)
{
    if (active == m_active)
        return;
    m_active = active;
    emit activeChanged();
    load();
}

void NeuralField::refresh()
{
    load();
}

void NeuralField::setStill(bool still)
{
    if (still == m_still)
        return;
    m_still = still;
    emit stillChanged();
    // The mode is a property of the sphere: list again to build one in the new mode.
    release();
    load();
}

void NeuralField::setLimit(int limit)
{
    limit = std::clamp(limit, 1, 20000);
    if (limit == m_limit)
        return;
    // A stricter governor ceiling takes effect immediately, before the asynchronous scan.
    if (limit < m_limit) {
        release();
        publish();
    }
    m_limit = limit;
    emit rootChanged();
    load();
}

void NeuralField::setFilesPerFolder(int files)
{
    files = std::clamp(files, 1, 1000);
    if (files == m_filesPerFolder)
        return;
    m_filesPerFolder = files;
    emit rootChanged();
    load();
}

void NeuralField::release()
{
    if (m_sphere)
        neural_free(m_sphere);
    m_sphere = nullptr;
    m_modelRevision = ~0ULL;
    m_time = 0.0;
    m_sourceRoot.clear();
    m_scannedAt.clear();
    emit sourceChanged();
}

void NeuralField::load()
{
    ++m_generation;
    m_loading = false;
    if (!m_error.isEmpty()) {
        m_error.clear();
        emit errorChanged();
    }
    if (m_root.isEmpty()) {
        release();
        publish();
        return;
    }
    if (!m_active) {
        emit sourceChanged();
        return;
    }
    m_loading = true;
    emit sourceChanged();
    const QByteArray root = m_root.toUtf8();
    const auto limit = static_cast<std::uint32_t>(m_limit);
    const auto files = static_cast<std::uint32_t>(m_filesPerFolder);
    const quint64 generation = m_generation;
    // The future owns its result even if the view is destroyed before scanning finishes.
    using Index = std::shared_ptr<NeuralIndex>;
    auto *watcher = new QFutureWatcher<Index>(this);
    connect(watcher, &QFutureWatcherBase::finished, this, [this, watcher, generation] {
        const Index index = watcher->result();
        watcher->deleteLater();
        if (generation != m_generation)
            return;
        accept(index.get());
    });
    // The listing reads directory entries only and is safe off the GUI thread.
    watcher->setFuture(QtConcurrent::run([root, limit, files] {
        return Index(neural_scan(root.constData(), static_cast<std::size_t>(root.size()), limit, files), neural_index_free);
    }));
}

void NeuralField::accept(NeuralIndex *index)
{
    m_loading = false;
    if (!index) {
        release();
        m_error = uiText(QStringLiteral("No se pudo leer la carpeta %1"), QStringLiteral("Could not read the folder %1")).arg(m_root);
        emit errorChanged();
        publish();
        return;
    }
    if (!m_error.isEmpty()) {
        m_error.clear();
        emit errorChanged();
    }
    if (m_sphere) {
        neural_set_index(m_sphere, index, !m_still);
    } else {
        m_sphere = neural_new(index, kSeed, m_still);
        if (m_sphere) {
            // Settings given before the folder was listed apply to the sphere that now exists.
            const QByteArray background = m_background.toUtf8();
            neural_set_background(m_sphere, background.constData());
            for (auto it = m_numbers.cbegin(); it != m_numbers.cend(); ++it)
                neural_set_number(m_sphere, it.key().toUtf8().constData(), it.value());
            for (auto it = m_flags.cbegin(); it != m_flags.cend(); ++it)
                neural_set_flag(m_sphere, it.key().toUtf8().constData(), it.value());
            for (auto it = m_links.cbegin(); it != m_links.cend(); ++it)
                neural_set_link(m_sphere, static_cast<std::uint32_t>(it.key()), it.value().first, it.value().second);
        }
    }
    if (!m_sphere) {
        m_error = uiText(QStringLiteral("La esfera neuronal no pudo crearse"), QStringLiteral("The neural sphere could not be created"));
        emit errorChanged();
        emit sourceChanged();
        publish();
        return;
    }
    m_sourceRoot = m_root;
    m_scannedAt = QDateTime::currentDateTimeUtc().toString(Qt::ISODate);
    emit sourceChanged();
    if (m_still)
        m_time = static_cast<double>(neural_prime_still(m_sphere));
    readSettings();
    publish();
}

bool NeuralField::loadNow()
{
    // A capture waits for the listing instead of racing it.
    while (m_loading)
        QCoreApplication::processEvents(QEventLoop::AllEvents | QEventLoop::WaitForMoreEvents, 50);
    return m_sphere != nullptr;
}

double NeuralField::advance(double seconds)
{
    if (!m_sphere || m_still || !m_active || !std::isfinite(seconds))
        return 0.0;
    // Rust widens the f32 argument then clamps it to 50ms. Match that accepted delta
    // exactly (0.05f is slightly larger than 0.05 in f64) for shaders and camera too.
    const float dt = static_cast<float>(std::clamp(seconds, 0.0, 0.05));
    const double accepted = std::min(static_cast<double>(dt), 0.05);
    neural_step(m_sphere, dt);
    m_time += accepted;
    publish();
    return accepted;
}

void NeuralField::publish()
{
    NeuralStats stats{};
    if (m_sphere && neural_stats(m_sphere, &stats)) {
        if (m_modelRevision != stats.revision) {
            m_modelRevision = stats.revision;
            ++m_revision;
        }
    } else {
        ++m_revision;
    }
    emit updated();
}

void NeuralField::readSettings()
{
    if (!m_sphere)
        return;
    const QString json = text([this](char *out, std::size_t capacity) { return neural_settings_json(m_sphere, out, capacity); });
    const QJsonObject object = QJsonDocument::fromJson(json.toUtf8()).object();
    if (object.isEmpty())
        return;
    m_settings = object.toVariantMap();
    const QString background = m_settings.value(QStringLiteral("background")).toString();
    if (!background.isEmpty() && background != m_background) {
        m_background = background;
        const QColor colour(background);
        m_backgroundLinear = QVector3D(channel(static_cast<float>(colour.redF())), channel(static_cast<float>(colour.greenF())),
                                       channel(static_cast<float>(colour.blueF())));
    }
    emit settingsChanged();
}

bool NeuralField::setNumber(const QString &key, double value)
{
    if (!std::isfinite(value))
        return false;
    const QByteArray name = key.toUtf8();
    if (m_sphere) {
        if (!neural_set_number(m_sphere, name.constData(), value))
            return false;
    }
    m_numbers.insert(key, value);
    readSettings();
    publish();
    return true;
}

bool NeuralField::setFlag(const QString &key, bool value)
{
    const QByteArray name = key.toUtf8();
    if (m_sphere) {
        if (!neural_set_flag(m_sphere, name.constData(), value))
            return false;
    }
    m_flags.insert(key, value);
    readSettings();
    publish();
    return true;
}

bool NeuralField::setBackground(const QString &hex)
{
    const QColor colour(hex);
    if (!colour.isValid() || !hex.startsWith(QLatin1Char('#')))
        return false;
    if (m_sphere && !neural_set_background(m_sphere, hex.toUtf8().constData()))
        return false;
    m_background = hex;
    m_backgroundLinear = QVector3D(channel(static_cast<float>(colour.redF())), channel(static_cast<float>(colour.greenF())),
                                   channel(static_cast<float>(colour.blueF())));
    readSettings();
    emit settingsChanged();
    return true;
}

bool NeuralField::setLink(int kind, int on, double opacity)
{
    if (kind < 0 || kind > 6)
        return false;
    if (m_sphere && !neural_set_link(m_sphere, static_cast<std::uint32_t>(kind), on, opacity))
        return false;
    m_links.insert(kind, {on, opacity});
    readSettings();
    publish();
    return true;
}

bool NeuralField::select(const QString &id)
{
    if (!m_sphere || !neural_select(m_sphere, id.toUtf8().constData()))
        return false;
    publish();
    emit selectionChanged();
    return true;
}

bool NeuralField::activate(const QString &id)
{
    if (!m_sphere || !neural_activate(m_sphere, id.toUtf8().constData()))
        return false;
    publish();
    emit selectionChanged();
    return true;
}

bool NeuralField::toggleFold(const QString &id)
{
    if (!m_sphere || !neural_toggle_fold(m_sphere, id.toUtf8().constData()))
        return false;
    publish();
    emit selectionChanged();
    return true;
}

QString NeuralField::pick(const QVector3D &origin, const QVector3D &direction, float threshold) const
{
    if (!m_sphere)
        return {};
    const float o[3] = {origin.x(), origin.y(), origin.z()};
    const float d[3] = {direction.x(), direction.y(), direction.z()};
    return text([&](char *out, std::size_t capacity) { return neural_pick(m_sphere, o, d, threshold, out, capacity); });
}

QVector3D NeuralField::positionOf(const QString &id) const
{
    float at[3] = {0, 0, 0};
    if (!m_sphere || !neural_position(m_sphere, id.toUtf8().constData(), at))
        return {};
    return {at[0], at[1], at[2]};
}

QVariantMap NeuralField::lookupPosition(const QString &id) const
{
    float at[3] = {0, 0, 0};
    if (!m_sphere || !neural_position(m_sphere, id.toUtf8().constData(), at)
        || !std::isfinite(at[0]) || !std::isfinite(at[1]) || !std::isfinite(at[2]))
        return {{QStringLiteral("found"), false}};
    return {{QStringLiteral("found"), true},
            {QStringLiteral("position"), QVariant::fromValue(QVector3D(at[0], at[1], at[2]))}};
}

QVariantList NeuralField::labels()
{
    QVariantList rows;
    if (!m_sphere)
        return rows;
    const QString all = text([this](char *out, std::size_t capacity) { return neural_labels(m_sphere, out, capacity); });
    for (const QString &line : all.split(QLatin1Char('\n'), Qt::SkipEmptyParts)) {
        const QStringList cells = line.split(QLatin1Char('\t'));
        if (cells.size() < 6)
            continue;
        rows.append(QVariantMap{{QStringLiteral("kind"), cells[0]},
                                {QStringLiteral("x"), cells[1].toDouble()},
                                {QStringLiteral("y"), cells[2].toDouble()},
                                {QStringLiteral("z"), cells[3].toDouble()},
                                {QStringLiteral("id"), cells[4]},
                                {QStringLiteral("text"), cells.mid(5).join(QLatin1Char('\t'))}});
    }
    return rows;
}

QStringList NeuralField::selection(const QString &rootLabel)
{
    if (!m_sphere)
        return {};
    const QByteArray label = rootLabel.toUtf8();
    const QString all = text([&](char *out, std::size_t capacity) { return neural_selection(m_sphere, label.constData(), out, capacity); });
    QStringList lines = all.split(QLatin1Char('\n'));
    while (!lines.isEmpty() && lines.last().isEmpty())
        lines.removeLast();
    return lines;
}

QVariantMap NeuralField::stats() const
{
    NeuralStats s{};
    if (!m_sphere || !neural_stats(m_sphere, &s))
        return {};
    return {{QStringLiteral("visible"), s.visible}, {QStringLiteral("moving"), s.moving}, {QStringLiteral("leaving"), s.leaving},
            {QStringLiteral("born"), s.born},       {QStringLiteral("total"), s.total},   {QStringLiteral("sparks"), s.sparks},
            {QStringLiteral("maxStep"), s.max_step}, {QStringLiteral("dt"), s.dt},         {QStringLiteral("time"), s.time},
            {QStringLiteral("rebuilds"), static_cast<qulonglong>(s.rebuilds)},
            {QStringLiteral("revision"), static_cast<qulonglong>(s.revision)}};
}

QVariantList NeuralField::linkCounts()
{
    QVariantList counts;
    std::uint32_t values[7] = {};
    if (m_sphere && neural_link_counts(m_sphere, values))
        for (const std::uint32_t value : values)
            counts.append(value);
    return counts;
}

QQuaternion NeuralField::orbitRotation(double time) const
{
    // three.js: orbit.rotation.set(0.3, 0, -0.1), then rotation.y = time * 0.05, Euler order XYZ.
    return QQuaternion::fromAxisAndAngle(1, 0, 0, static_cast<float>(0.3 * kDegrees))
        * QQuaternion::fromAxisAndAngle(0, 1, 0, static_cast<float>(time * 0.05 * kDegrees))
        * QQuaternion::fromAxisAndAngle(0, 0, 1, static_cast<float>(-0.1 * kDegrees));
}

QQuaternion NeuralField::baseRotation(double time) const
{
    return QQuaternion::fromAxisAndAngle(0, 1, 0, static_cast<float>(time * 0.04 * kDegrees));
}

std::vector<NeuralPoint> NeuralField::points() const
{
    std::vector<NeuralPoint> out;
    if (!m_sphere)
        return out;
    const std::size_t count = neural_points(m_sphere, nullptr, 0);
    out.resize(count);
    if (count && neural_points(m_sphere, out.data(), count) != count)
        out.clear();
    return out;
}

std::vector<NeuralVertex> NeuralField::lines(int layer) const
{
    std::vector<NeuralVertex> out;
    if (!m_sphere || layer < 0 || layer >= NEURAL_LINE_LAYERS)
        return out;
    const auto which = static_cast<std::uint32_t>(layer);
    const std::size_t count = neural_lines(m_sphere, which, nullptr, 0);
    out.resize(count);
    if (count && neural_lines(m_sphere, which, out.data(), count) != count)
        out.clear();
    return out;
}

std::vector<NeuralSpark> NeuralField::sparks() const
{
    std::vector<NeuralSpark> out;
    if (!m_sphere)
        return out;
    const std::size_t count = neural_sparks(m_sphere, nullptr, 0);
    out.resize(count);
    if (count && neural_sparks(m_sphere, out.data(), count) != count)
        out.clear();
    // Rust keeps the original fixed 70x5 trail storage. Upload only the budgeted
    // active trails; inactive slots are irrelevant even in a deterministic still frame.
    const auto active = static_cast<std::size_t>(std::clamp(std::lround(m_settings.value(QStringLiteral("pulses"), 70).toDouble()), 0L, 70L));
    out.resize(std::min(out.size(), active * 5));
    return out;
}
