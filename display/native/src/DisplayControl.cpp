#include "DisplayControl.h"

#include "LumenField.h"

#include <QHostAddress>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
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
    return refusal(QStringLiteral("unknown action ") + action);
}
