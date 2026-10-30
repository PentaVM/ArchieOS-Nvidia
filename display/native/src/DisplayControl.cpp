#include "DisplayControl.h"

#include "LumenField.h"

#include <QDateTime>
#include <QDir>
#include <QHostAddress>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QStandardPaths>
#include <QTcpSocket>
#include <QTimer>

#include <memory>

namespace {

constexpr qsizetype maximumRequest = 4096;

struct Scene {
    const char *name;
    const char *mode;
};

// The views Main.qml draws. A scene that is added there is added here, and the CLI and
// the voice learn it from `list`.
constexpr Scene sceneTable[] = {{"orb", "cerebra"}, {"forge", "cerebra"}, {"world", "watcher"}};

QByteArray refusal(const QString &reason)
{
    // A reply is one line, whatever a caller put in the name it sent.
    QString line = reason.left(200);
    line.replace(QLatin1Char('\n'), QLatin1Char(' ')).replace(QLatin1Char('\r'), QLatin1Char(' '));
    return QByteArrayLiteral("error: ") + line.toUtf8() + '\n';
}

}  // namespace

DisplayControl::DisplayControl(LumenField *lumen, QObject *parent, bool shellMode)
    : QObject(parent), m_lumen(lumen), m_shellMode(shellMode)
{
    connect(&m_server, &QTcpServer::newConnection, this, [this] {
        while (QTcpSocket *socket = m_server.nextPendingConnection()) {
            socket->setParent(this);
            socket->setReadBufferSize(maximumRequest + 1);
            QTimer::singleShot(3000, socket, [socket] {
                if (socket->state() == QAbstractSocket::ConnectedState)
                    socket->disconnectFromHost();
            });
            auto buffer = std::make_shared<QByteArray>();
            connect(socket, &QTcpSocket::readyRead, socket, [this, socket, buffer] {
                buffer->append(socket->read(maximumRequest + 1 - buffer->size()));
                const qsizetype end = buffer->indexOf('\n');
                if (end < 0 && buffer->size() <= maximumRequest)
                    return;
                socket->write(end < 0 || end > maximumRequest ? refusal(QStringLiteral("request too large"))
                                                              : answer(buffer->left(end)));
                socket->disconnectFromHost();
            });
            connect(socket, &QTcpSocket::disconnected, socket, &QTcpSocket::deleteLater);
        }
    });
}

bool DisplayControl::start(quint16 port)
{
    return m_server.listen(QHostAddress::LocalHost, port);
}

QStringList DisplayControl::scenes() const
{
    QStringList names;
    for (const auto &scene : sceneTable) {
        if (!m_shellMode || m_worldAvailable || QLatin1String(scene.name) != QLatin1String("world"))
            names.append(QString::fromLatin1(scene.name));
    }
    return names;
}

QString DisplayControl::sceneNamed(const QString &name) const
{
    const QString wanted = name.trimmed().toLower();
    if (wanted == QLatin1String("core"))
        return QStringLiteral("forge");
    return scenes().contains(wanted) ? wanted : QString();
}

bool DisplayControl::showScene(const QString &name)
{
    const QString scene = sceneNamed(name);
    if (scene.isEmpty())
        return false;
    emit commandReceived(QStringLiteral("scene"), scene, true);
    return true;
}

QString DisplayControl::modeOf(const QString &scene) const
{
    for (const auto &entry : sceneTable)
        if (scene == QLatin1String(entry.name))
            return QString::fromLatin1(entry.mode);
    return {};
}

QByteArray DisplayControl::answer(const QByteArray &line)
{
    QJsonParseError parse{};
    const QJsonDocument document = QJsonDocument::fromJson(line, &parse);
    if (parse.error != QJsonParseError::NoError || !document.isObject())
        return refusal(QStringLiteral("the request is not a JSON object"));
    const QJsonObject command = document.object();
    const QString action = command.value(QStringLiteral("action")).toString();
    const QString value = command.value(QStringLiteral("value")).toString();

    if (action == QLatin1String("list")) {
        if (!m_lumen)
            return refusal(QStringLiteral("the renderer is unavailable"));
        const QJsonObject catalog{
            {QStringLiteral("scenes"), QJsonArray::fromStringList(scenes())},
            {QStringLiteral("presets"), QJsonArray::fromStringList(m_lumen->presets())},
        };
        return QJsonDocument(catalog).toJson(QJsonDocument::Compact) + '\n';
    }
    if (action == QLatin1String("scene")) {
        if (m_shellMode && !m_worldAvailable && value.trimmed().toLower() == QLatin1String("world"))
            return refusal(QStringLiteral("World is available in Observatory, not in the shell"));
        const QString scene = sceneNamed(value);
        if (scene.isEmpty())
            return refusal(QStringLiteral("unknown scene ") + value);
        emit commandReceived(action, scene, false);
        return QByteArrayLiteral("ok\n");
    }
    if (action == QLatin1String("preset")) {
        if (!m_lumen)
            return refusal(QStringLiteral("the renderer is unavailable"));
        // Applied here, synchronously, so the reply says whether it took.
        const QString reason = m_lumen->loadProfile(value);
        if (!reason.isEmpty())
            return refusal(reason);
        emit commandReceived(action, value, false);
        return QByteArrayLiteral("ok\n");
    }
    if (action == QLatin1String("cinematic")) {
        const QStringList profiles{QStringLiteral("field"), QStringLiteral("crown"),
                                   QStringLiteral("atlas"), QStringLiteral("dense")};
        if (!profiles.contains(value))
            return refusal(QStringLiteral("unknown cinematic ") + value);
        if (!m_lumen || !m_lumen->applyProfile(value))
            return refusal(QStringLiteral("the cinematic could not be applied"));
        emit commandReceived(action, value, false);
        return QByteArrayLiteral("ok\n");
    }
    if (action == QLatin1String("overlay")) {
        const QStringList overlays{QStringLiteral("hud"), QStringLiteral("shell"), QStringLiteral("ambient"),
            QStringLiteral("guides"), QStringLiteral("links"), QStringLiteral("pulses"), QStringLiteral("bloom")};
        if (!overlays.contains(value) || !command.value(QStringLiteral("enabled")).isBool())
            return refusal(QStringLiteral("an overlay requires a known layer and a boolean enabled value"));
        if (value == QLatin1String("guides") && m_lumen->settings().value(QStringLiteral("fieldSource")).toString() == QLatin1String("neural"))
            return refusal(QStringLiteral("guides are unavailable in the neural file snapshot"));
        emit commandReceived(action, value, command.value(QStringLiteral("enabled")).toBool());
        return QByteArrayLiteral("ok\n");
    }
    if (action == QLatin1String("panel")) {
        const QStringList panels{QStringLiteral("forge"), QStringLiteral("inspector"), QStringLiteral("settings"),
            QStringLiteral("sentry"), QStringLiteral("activity"), QStringLiteral("notifications"), QStringLiteral("search")};
        if (!m_shellMode || !panels.contains(value))
            return refusal(QStringLiteral("unknown or unavailable shell panel ") + value);
        emit commandReceived(action, value, false);
        return QByteArrayLiteral("ok\n");
    }
    return answerView(action, command);
}

QString DisplayControl::captureFolder() const
{
    QString base = QStandardPaths::writableLocation(QStandardPaths::RuntimeLocation);
    if (base.isEmpty())
        base = QStandardPaths::writableLocation(QStandardPaths::TempLocation);
    return base + QStringLiteral("/archie-display");
}

QByteArray DisplayControl::answerView(const QString &action, const QJsonObject &command)
{
    static const QStringList views{QStringLiteral("focus"), QStringLiteral("camera"), QStringLiteral("window"),
        QStringLiteral("pointer"), QStringLiteral("highlight"), QStringLiteral("popup"), QStringLiteral("freeze"),
        QStringLiteral("capture")};
    if (!views.contains(action))
        return refusal(QStringLiteral("unknown action ") + action);
    // The shell has its own window and no sphere of its own to move: only a capture is the same there.
    if (m_shellMode && !m_viewHandler && action != QLatin1String("capture"))
        return refusal(action + QStringLiteral(" is available in Observatory, not in the shell"));

    const QJsonValue raw = command.value(QStringLiteral("value"));
    const QString value = raw.toString();
    const auto plain = [](const QString &text, qsizetype longest) {
        if (text.size() > longest)
            return false;
        for (const QChar c : text)
            if (c.unicode() < 0x20 || c.unicode() == 0x7f)
                return false;
        return true;
    };

    if (action == QLatin1String("capture")) {
        if (!m_capture)
            return refusal(QStringLiteral("the window cannot be captured here"));
        const QString folder = captureFolder();
        if (!QDir().mkpath(folder))
            return refusal(QStringLiteral("the capture folder is not writable"));
        const QString path = folder + QStringLiteral("/capture-")
            + QDateTime::currentDateTimeUtc().toString(QStringLiteral("yyyyMMdd-HHmmss-zzz")) + QStringLiteral(".png");
        if (!m_capture(path))
            return refusal(QStringLiteral("the capture failed"));
        return QByteArrayLiteral("ok ") + path.toUtf8() + '\n';
    }

    QVariantMap arguments;
    if (action == QLatin1String("focus")) {
        // An empty value lets go of the node.
        if (!raw.isString() || value.size() > 256 || !plain(value, 256))
            return refusal(QStringLiteral("focus requires a node id of at most 256 characters"));
        arguments.insert(QStringLiteral("id"), value);
    } else if (action == QLatin1String("camera")) {
        static const QStringList moves{QStringLiteral("home"), QStringLiteral("closer"), QStringLiteral("farther"),
            QStringLiteral("left"), QStringLiteral("right"), QStringLiteral("up"), QStringLiteral("down")};
        if (!moves.contains(value))
            return refusal(QStringLiteral("unknown camera move ") + value.left(40));
        arguments.insert(QStringLiteral("move"), value);
    } else if (action == QLatin1String("window")) {
        static const QStringList states{QStringLiteral("windowed"), QStringLiteral("fullscreen"), QStringLiteral("maximized")};
        if (!states.contains(value))
            return refusal(QStringLiteral("unknown window state ") + value.left(40));
        arguments.insert(QStringLiteral("state"), value);
    } else if (action == QLatin1String("pointer")) {
        const QJsonValue x = command.value(QStringLiteral("x")), y = command.value(QStringLiteral("y"));
        const QJsonValue click = command.value(QStringLiteral("click"));
        if (!x.isDouble() || !y.isDouble() || x.toDouble() < 0.0 || x.toDouble() > 1.0 || y.toDouble() < 0.0 || y.toDouble() > 1.0
            || (!click.isUndefined() && !click.isBool()))
            return refusal(QStringLiteral("a pointer requires x and y between 0 and 1, and click as a boolean when given"));
        arguments.insert(QStringLiteral("x"), x.toDouble());
        arguments.insert(QStringLiteral("y"), y.toDouble());
        arguments.insert(QStringLiteral("click"), click.toBool(false));
    } else if (action == QLatin1String("highlight")) {
        if (!raw.isString() || value.isEmpty() || !plain(value, 256) || !command.value(QStringLiteral("enabled")).isBool())
            return refusal(QStringLiteral("highlight requires a node id and a boolean enabled value"));
        arguments.insert(QStringLiteral("id"), value);
        arguments.insert(QStringLiteral("enabled"), command.value(QStringLiteral("enabled")).toBool());
    } else if (action == QLatin1String("popup")) {
        const QJsonValue seconds = command.value(QStringLiteral("seconds"));
        if (!raw.isString() || value.trimmed().isEmpty() || !plain(value, 200)
            || (!seconds.isUndefined() && (!seconds.isDouble() || seconds.toDouble() < 1.0 || seconds.toDouble() > 30.0)))
            return refusal(QStringLiteral("a popup requires text of at most 200 characters and seconds between 1 and 30"));
        arguments.insert(QStringLiteral("text"), value.trimmed());
        arguments.insert(QStringLiteral("seconds"), seconds.toDouble(4.0));
    } else {  // freeze
        if (!command.value(QStringLiteral("enabled")).isBool())
            return refusal(QStringLiteral("freeze requires a boolean enabled value"));
        arguments.insert(QStringLiteral("enabled"), command.value(QStringLiteral("enabled")).toBool());
    }
    if (!m_viewHandler)
        return refusal(QStringLiteral("no active view can apply this command"));
    const QString error=m_viewHandler(action,arguments);
    if (!error.isEmpty()) return refusal(error);
    emit viewCommand(action, arguments);
    return QByteArrayLiteral("ok\n");
}
