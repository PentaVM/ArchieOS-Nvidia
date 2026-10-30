#include "WorldConsole.h"

#include <QHostAddress>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QNetworkAccessManager>
#include <QNetworkProxy>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QProcess>
#include <QRegularExpression>
#include <QStandardPaths>
#include <QTimer>
#include <algorithm>
#include <cmath>

namespace {
constexpr int HistoryLimit = 120;
constexpr int EntryLimit = 8192;
constexpr int InputLimit = 2048;

WorldConsole::Config normalized(WorldConsole::Config config) {
    if (config.program.isEmpty()) config.program = QStandardPaths::findExecutable(QStringLiteral("archie"));
    if (config.voiceProgram.isEmpty()) config.voiceProgram = QStandardPaths::findExecutable(QStringLiteral("archie-voice"));
    config.processTimeoutMs = std::clamp(config.processTimeoutMs, 100, 120000);
    config.maxOutputBytes = std::clamp(config.maxOutputBytes, 4096, 1048576);
    return config;
}

bool tokenize(const QString &input, QStringList &tokens, QString &error) {
    if (input.size() > InputLimit) {
        error = QStringLiteral("Command exceeds the 2048 character input bound.");
        return false;
    }
    QString token;
    QChar quote;
    bool started = false;
    for (qsizetype i = 0; i < input.size(); ++i) {
        const QChar ch = input[i];
        if (ch.unicode() < 32 || ch.unicode() == 127 || QStringLiteral(";|&<>`$").contains(ch)) {
            error = QStringLiteral("Shell syntax and control characters are not supported.");
            return false;
        }
        if (!quote.isNull()) {
            if (ch == quote) quote = QChar();
            else if (ch == QLatin1Char('\\') && i + 1 < input.size()
                && (input[i + 1] == quote || input[i + 1] == QLatin1Char('\\'))) token += input[++i];
            else token += ch;
        } else if ((ch == QLatin1Char('"') || ch == QLatin1Char('\'')) && !started) {
            quote = ch;
            started = true;
        } else if (ch.isSpace()) {
            if (started) { tokens.append(token); token.clear(); started = false; }
        } else { token += ch; started = true; }
    }
    if (!quote.isNull()) { error = QStringLiteral("Unclosed quoted argument."); return false; }
    if (started) tokens.append(token);
    for (const auto &part : tokens) {
        if (part.startsWith(QStringLiteral("--"))) {
            error = QStringLiteral("Command flags are not accepted; scope and configuration belong to the host.");
            return false;
        }
    }
    return true;
}

QString helpText() {
    return QStringLiteral("help\nworld status\nworld resolve <place>\nworld plan|explore <latitude> <longitude> <radius_m> [terrain,buildings,imagery]\nworld navigate\nworld navigation-status|navigation-stop <id>\nworld pause|resume|cancel <id>\nos status|topology|modules|providers|peers|models|metrics|boot|ping\niris status\niris <goal> (requires trusted Stage configuration)\nWorld jobs accepted by Core may continue after this console stops waiting.");
}

bool parseWorld(const QStringList &tokens, QJsonObject &request) {
    if (tokens.size() < 2) return false;
    const auto op = tokens[1];
    if (tokens[0] == QStringLiteral("os")) {
        const QStringList operations{QStringLiteral("status"), QStringLiteral("topology"), QStringLiteral("modules"),
            QStringLiteral("providers"), QStringLiteral("peers"), QStringLiteral("models"), QStringLiteral("metrics"),
            QStringLiteral("boot"), QStringLiteral("ping")};
        if (tokens.size() != 2 || !operations.contains(op)) return false;
        request.insert(QStringLiteral("op"), QStringLiteral("os_") + op);
        return true;
    }
    if (tokens[0] != QStringLiteral("world")) return false;
    request.insert(QStringLiteral("op"), op);
    if (op == QStringLiteral("status")) return tokens.size() == 2;
    if (op == QStringLiteral("navigate")) {
        request.insert(QStringLiteral("request"), QJsonObject{});
        return tokens.size() == 2;
    }
    if (op == QStringLiteral("resolve")) {
        const auto query = tokens.mid(2).join(QLatin1Char(' ')).trimmed();
        if (query.isEmpty() || query.size() > 256) return false;
        request.insert(QStringLiteral("query"), query);
        return true;
    }
    const QStringList jobOps{QStringLiteral("pause"), QStringLiteral("resume"), QStringLiteral("cancel"),
        QStringLiteral("navigation-status"), QStringLiteral("navigation-stop")};
    if (jobOps.contains(op)) {
        static const QRegularExpression idPattern(QStringLiteral("^[a-fA-F0-9]{32}$"));
        if (tokens.size() != 3 || !idPattern.match(tokens[2]).hasMatch()) return false;
        auto apiOp = op;
        apiOp.replace(QLatin1Char('-'), QLatin1Char('_'));
        request.insert(QStringLiteral("op"), apiOp);
        request.insert(QStringLiteral("id"), tokens[2]);
        return true;
    }
    if (op != QStringLiteral("plan") && op != QStringLiteral("explore")) return false;
    if (tokens.size() != 5 && tokens.size() != 6) return false;
    bool latOk = false, lonOk = false, radiusOk = false;
    const double latitude = tokens[2].toDouble(&latOk), longitude = tokens[3].toDouble(&lonOk), radius = tokens[4].toDouble(&radiusOk);
    if (!latOk || !lonOk || !radiusOk || !std::isfinite(latitude) || !std::isfinite(longitude) || !std::isfinite(radius)
        || latitude < -90 || latitude > 90 || longitude < -180 || longitude > 180 || radius <= 0 || radius > 100000) return false;
    QJsonObject region{{QStringLiteral("latitude"), latitude}, {QStringLiteral("longitude"), longitude}, {QStringLiteral("radius_m"), radius}};
    if (tokens.size() == 6) {
        QJsonArray layers;
        const auto parts = tokens[5].split(QLatin1Char(','));
        for (const auto &layer : parts) {
            if (layer != QStringLiteral("terrain") && layer != QStringLiteral("buildings") && layer != QStringLiteral("imagery")) return false;
            if (!layers.contains(layer)) layers.append(layer);
        }
        region.insert(QStringLiteral("layers"), layers);
    }
    request.insert(QStringLiteral("request"), region);
    return true;
}
}

WorldConsole::WorldConsole(QObject *parent) : WorldConsole(Config{}, parent) {}
WorldConsole::WorldConsole(const Config &config, QObject *parent) : QObject(parent), m_config(normalized(config)) {
    m_timer = new QTimer(this);
    m_timer->setSingleShot(true);
    connect(m_timer, &QTimer::timeout, this, [this] {
        stopOwnedTransport();
        fail(QStringLiteral("Response deadline expired. Core work may still be running; inspect world status before retrying."));
    });
}
WorldConsole::~WorldConsole() {
    stopOwnedTransport();
    if (m_process && m_process->state() != QProcess::NotRunning) m_process->waitForFinished(1000);
}

void WorldConsole::setBusy(bool busy) {
    if (m_busy == busy) return;
    m_busy = busy;
    emit busyChanged();
}
QString WorldConsole::displayText(QString text) const {
    for (const auto &privateValue : {m_config.stageKeyFile, m_config.stageSocket, m_config.stagePolicy})
        if (!privateValue.isEmpty()) text.replace(privateValue, QStringLiteral("[host configuration]"));
    static const QRegularExpression credentials(QStringLiteral("(?i)(bearer\\s+|(?:token|api[_-]?key|password|secret)[\\\"'\\s:=]+)[A-Za-z0-9_./+\\-=]{4,}"));
    text.replace(credentials, QStringLiteral("[redacted credential]"));
    for (auto &ch : text) if ((ch.unicode() < 32 && ch != QLatin1Char('\n') && ch != QLatin1Char('\t')) || ch.unicode() == 127) ch = QLatin1Char(' ');
    if (text.size() > EntryLimit) text = text.left(EntryLimit) + QStringLiteral("\n[display truncated]");
    return text;
}
QVariant WorldConsole::sanitize(const QVariant &value) const {
    if (value.metaType().id() == QMetaType::QVariantMap) {
        QVariantMap result;
        const auto map = value.toMap();
        static const QRegularExpression sensitive(QStringLiteral("(?i)(token|secret|password|credential|authorization|api.?key|key_file)"));
        for (auto it = map.cbegin(); it != map.cend(); ++it)
            result.insert(it.key(), sensitive.match(it.key()).hasMatch() ? QVariant(QStringLiteral("[redacted]")) : sanitize(it.value()));
        return result;
    }
    if (value.metaType().id() == QMetaType::QVariantList || value.metaType().id() == QMetaType::QStringList) {
        QVariantList result;
        for (const auto &item : value.toList()) result.append(sanitize(item));
        return result;
    }
    if (value.metaType().id() == QMetaType::QString) return displayText(value.toString());
    return value;
}
void WorldConsole::append(const QString &kind, const QString &text) {
    m_entries.append(QVariantMap{{QStringLiteral("kind"), kind}, {QStringLiteral("text"), displayText(text)}});
    while (m_entries.size() > HistoryLimit) m_entries.removeFirst();
    emit entriesChanged();
}
void WorldConsole::fail(const QString &message) {
    m_timer->stop();
    m_error = displayText(message);
    emit errorChanged();
    append(QStringLiteral("error"), m_error);
    setBusy(false);
}
void WorldConsole::succeed(QVariantMap result) {
    m_timer->stop();
    result.insert(QStringLiteral("op"), m_operation);
    m_lastResult = sanitize(result).toMap();
    emit lastResultChanged();
    append(QStringLiteral("result"), QString::fromUtf8(QJsonDocument::fromVariant(m_lastResult).toJson(QJsonDocument::Indented)));
    setBusy(false);
    emit resultReady(m_lastResult);
}

void WorldConsole::submit(const QString &command) {
    if (m_busy) { append(QStringLiteral("note"), QStringLiteral("A command is already pending. Stop waiting before submitting another.")); return; }
    m_error.clear();
    emit errorChanged();
    m_lastResult.clear();
    emit lastResultChanged();
    append(QStringLiteral("command"), command.left(InputLimit));
    QStringList tokens;
    QString parserError;
    if (!tokenize(command, tokens, parserError)) { fail(parserError); return; }
    if (tokens.isEmpty()) return;
    if (tokens[0] == QStringLiteral("help") && tokens.size() == 1) { append(QStringLiteral("help"), helpText()); return; }
    if (tokens[0] == QStringLiteral("iris")) {
        if (tokens.size() < 2) { fail(QStringLiteral("iris requires status or a goal.")); return; }
        if (tokens.size() == 2 && tokens[1] == QStringLiteral("status")) { m_operation = QStringLiteral("iris_status"); startHealth(); return; }
        if (m_config.stageSocket.isEmpty() || m_config.stageKeyFile.isEmpty() || m_config.stagePolicy.isEmpty()) {
            fail(QStringLiteral("IRIS actions require trusted host Stage socket, key file and Policy configuration.")); return;
        }
        m_operation = QStringLiteral("iris_goal");
        auto args = m_config.voicePrefixArguments;
        args << QStringLiteral("do") << tokens.mid(1).join(QLatin1Char(' ')) << QStringLiteral("--grade") << QStringLiteral("public")
             << QStringLiteral("--socket") << m_config.stageSocket << QStringLiteral("--key-file") << m_config.stageKeyFile
             << QStringLiteral("--policy") << m_config.stagePolicy << QStringLiteral("--steps") << QStringLiteral("6");
        if (!m_config.groundModel.isEmpty()) args << QStringLiteral("--ground-model") << m_config.groundModel;
        startProcess(m_config.voiceProgram, args, true);
        return;
    }
    QJsonObject request;
    if (!parseWorld(tokens, request)) { fail(QStringLiteral("Unsupported or invalid command. Type help for the bounded command catalog.")); return; }
    // JSON strings are intentionally not decoded/reconstructed here: the Core CLI
    // owns ContextDomain validation and admission. Never infer a personal fallback.
    if (m_config.contextJson.isEmpty()) { fail(QStringLiteral("The native host has not configured a Core context domain.")); return; }
    m_operation = request.value(QStringLiteral("op")).toString();
    auto args = m_config.programPrefixArguments;
    args << QStringLiteral("--json") << QStringLiteral("world") << QStringLiteral("operator") << QStringLiteral("--context")
         << m_config.contextJson << QStringLiteral("--request") << QString::fromUtf8(QJsonDocument(request).toJson(QJsonDocument::Compact));
    startProcess(m_config.program, args, false);
}

void WorldConsole::startProcess(const QString &program, const QStringList &arguments, bool voice) {
    if (program.isEmpty()) { fail(QStringLiteral("The configured command executable is unavailable.")); return; }
    m_stdout.clear(); m_stderr.clear(); m_voice = voice;
    if (m_process) { m_process->deleteLater(); m_process = nullptr; }
    auto *process = new QProcess(this);
    m_process = process;
    process->setProcessChannelMode(QProcess::SeparateChannels);
    connect(process, &QProcess::readyReadStandardOutput, this, [this] { drainProcess(); });
    connect(process, &QProcess::readyReadStandardError, this, [this] { drainProcess(); });
    connect(process, &QProcess::errorOccurred, this, [this](QProcess::ProcessError error) {
        if (m_busy && error == QProcess::FailedToStart) fail(QStringLiteral("Command executable failed to start. Check the native host configuration."));
    });
    connect(process, qOverload<int, QProcess::ExitStatus>(&QProcess::finished), this,
        [this](int code, QProcess::ExitStatus status) { if (m_busy) finishProcess(code, status == QProcess::CrashExit); });
    setBusy(true);
    m_timer->start(m_config.processTimeoutMs);
    process->start(program, arguments, QIODevice::ReadOnly);
}
bool WorldConsole::drainProcess() {
    if (!m_process) return false;
    // Read a bounded chunk from each pipe. Never retain unbounded process output.
    m_process->setReadChannel(QProcess::StandardOutput);
    m_stdout += m_process->read(m_config.maxOutputBytes + 1 - m_stdout.size());
    m_process->setReadChannel(QProcess::StandardError);
    m_stderr += m_process->read(m_config.maxOutputBytes + 1 - m_stderr.size());
    if (m_stdout.size() + m_stderr.size() > m_config.maxOutputBytes) {
        stopOwnedTransport();
        fail(QStringLiteral("Command output exceeded the configured bound; stopped waiting. Core work may still be running."));
        return false;
    }
    return true;
}
void WorldConsole::finishProcess(int exitCode, bool crashed) {
    if (!drainProcess() || !m_busy) return;
    if (m_voice) {
        if (crashed || exitCode != 0) { fail(QStringLiteral("IRIS command failed: ") + QString::fromUtf8(m_stderr)); return; }
        succeed({{QStringLiteral("ok"), true}, {QStringLiteral("output"), QString::fromUtf8(m_stdout)},
            {QStringLiteral("note"), QStringLiteral("Stage/Policy controls this action; inspect the response for its actual outcome.")}});
        return;
    }
    QJsonParseError parseError;
    const auto document = QJsonDocument::fromJson(m_stdout, &parseError);
    if (parseError.error != QJsonParseError::NoError || !document.isObject()) {
        fail(QStringLiteral("Core CLI returned invalid JSON; no success was inferred.")); return;
    }
    const auto result = document.object();
    if (crashed || exitCode != 0 || !result.value(QStringLiteral("ok")).isBool() || !result.value(QStringLiteral("ok")).toBool()) {
        fail(result.value(QStringLiteral("error")).toString(QStringLiteral("Core command failed or was refused."))); return;
    }
    if (!result.contains(QStringLiteral("result"))) { fail(QStringLiteral("Core CLI response is missing its result.")); return; }
    auto value = result.toVariantMap();
    if (m_operation == QStringLiteral("explore") || m_operation == QStringLiteral("navigate"))
        value.insert(QStringLiteral("note"), QStringLiteral("Core accepted this operation; acceptance does not verify capture completion or provider availability."));
    succeed(value);
}

void WorldConsole::startHealth() {
    const auto url = m_config.irisHealthUrl;
    QHostAddress host;
    if (url.isEmpty()) { fail(QStringLiteral("The native host has not configured an IRIS health endpoint.")); return; }
    if (!url.isValid() || url.scheme() != QStringLiteral("http") || !host.setAddress(url.host()) || !host.isLoopback()
        || url.path() != QStringLiteral("/health") || !url.userInfo().isEmpty() || url.hasQuery() || url.hasFragment()) {
        fail(QStringLiteral("IRIS health requires a fixed HTTP loopback /health endpoint.")); return;
    }
    if (!m_network) { m_network = new QNetworkAccessManager(this); m_network->setProxy(QNetworkProxy::NoProxy); }
    QNetworkRequest request(url);
    request.setAttribute(QNetworkRequest::RedirectPolicyAttribute, QNetworkRequest::ManualRedirectPolicy);
    m_stdout.clear();
    setBusy(true);
    m_timer->start(std::min(m_config.processTimeoutMs, 5000));
    auto *reply = m_network->get(request);
    m_reply = reply;
    reply->setReadBufferSize(m_config.maxOutputBytes + 1);
    connect(reply, &QNetworkReply::readyRead, this, [this, reply] {
        if (m_reply != reply) return;
        m_stdout += reply->read(m_config.maxOutputBytes + 1 - m_stdout.size());
        if (m_stdout.size() > m_config.maxOutputBytes) {
            stopOwnedTransport();
            fail(QStringLiteral("IRIS health response exceeded the configured bound."));
        }
    });
    connect(reply, &QNetworkReply::finished, this, [this, reply] {
        if (m_reply != reply) return;
        m_stdout += reply->read(m_config.maxOutputBytes + 1 - m_stdout.size());
        m_reply = nullptr;
        const auto status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
        const bool valid = reply->error() == QNetworkReply::NoError && status == 200 && m_stdout.size() <= m_config.maxOutputBytes;
        reply->deleteLater();
        if (!valid) { fail(QStringLiteral("IRIS health endpoint is unavailable or returned an invalid response.")); return; }
        QJsonParseError error;
        const auto json = QJsonDocument::fromJson(m_stdout, &error);
        if (error.error != QJsonParseError::NoError || !json.isObject()) { fail(QStringLiteral("IRIS health returned invalid JSON.")); return; }
        if (json.object().contains(QStringLiteral("error"))) { fail(QStringLiteral("IRIS health reports an unavailable runtime.")); return; }
        succeed({{QStringLiteral("ok"), true}, {QStringLiteral("health"), json.object().toVariantMap()},
            {QStringLiteral("note"), QStringLiteral("Health response only; no model accuracy, action or latency verification.")}});
    });
}
void WorldConsole::stopOwnedTransport() {
    m_timer->stop();
    if (m_reply) {
        auto *reply = m_reply; m_reply = nullptr;
        reply->disconnect(this); reply->abort(); reply->deleteLater();
    }
    if (m_process) {
        m_process->disconnect(this);
        if (m_process->state() != QProcess::NotRunning) m_process->kill();
    }
}
void WorldConsole::cancel() {
    if (!m_busy) return;
    stopOwnedTransport();
    fail(QStringLiteral("Stopped waiting for this console command. Core or Stage work may still be running; inspect status and use an explicit job cancel/stop command."));
}
