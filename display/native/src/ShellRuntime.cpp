#include "ShellRuntime.h"
#include "UiText.h"
#include <QCoreApplication>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QStandardPaths>
#include <QStringDecoder>

namespace {
constexpr qsizetype maximumFrameBytes = 1024 * 1024;
constexpr qsizetype maximumErrorBytes = 4096;
}

ShellRuntime::ShellRuntime(QString program, QString lanFile, QString workspacesRoot,
                           quint16 projectionPort, QObject *parent)
    : QObject(parent) {
    // A bare name is looked up on PATH and then beside this executable, where a build or an
    // install puts archie-shell-bridge next to archie-display.
    if (!program.contains(QLatin1Char('/')) && !program.contains(QLatin1Char('\\'))
        && QStandardPaths::findExecutable(program).isEmpty()) {
        const QString beside = QStandardPaths::findExecutable(program, {QCoreApplication::applicationDirPath()});
        if (!beside.isEmpty()) program = beside;
    }
    m_process.setProgram(program);
    m_process.setArguments({QStringLiteral("watch"), QStringLiteral("--lan-file"), lanFile,
                           QStringLiteral("--workspaces-root"), workspacesRoot,
                           QStringLiteral("--displayd"), QStringLiteral("127.0.0.1:%1").arg(projectionPort)});
    m_restart.setSingleShot(true);
    m_restart.setInterval(3000);
    m_stop.setSingleShot(true);
    m_stop.setInterval(500);
    connect(&m_restart, &QTimer::timeout, this, &ShellRuntime::start);
    connect(&m_stop, &QTimer::timeout, this, [this] { m_process.kill(); });
    connect(&m_process, &QProcess::stateChanged, this, [this] { emit runningChanged(); });
    connect(&m_process, &QProcess::started, this, [this] { setError({}); });
    connect(&m_process, &QProcess::readyReadStandardOutput, this, &ShellRuntime::readFrames);
    connect(&m_process, &QProcess::readyReadStandardError, this, [this] {
        const auto diagnostic = m_process.readAllStandardError().right(maximumErrorBytes);
        if (m_wanted && !diagnostic.isEmpty()) setError(QString::fromUtf8(diagnostic).trimmed());
    });
    connect(&m_process, &QProcess::errorOccurred, this, [this](QProcess::ProcessError error) {
        if (!m_wanted) return;
        setError(m_process.errorString());
        if (error == QProcess::FailedToStart) reconnect();
    });
    connect(&m_process, qOverload<int, QProcess::ExitStatus>(&QProcess::finished), this,
            [this](int code, QProcess::ExitStatus status) {
        m_stop.stop();
        m_pending.clear();
        if (!m_wanted) return;
        if (m_error.isEmpty()) setError(QStringLiteral("shell bridge exited (%1, %2)")
            .arg(code).arg(status == QProcess::CrashExit ? QStringLiteral("crash") : QStringLiteral("exit")));
        reconnect();
    });
}

void ShellRuntime::aiPolicyRequest(const QString &verb)
{
    if (m_aiProcess) return; // one request at a time; the next click waits for this answer
    const QString archie = QStandardPaths::findExecutable(QStringLiteral("archie"));
    if (archie.isEmpty()) {
        m_aiPolicy = QStringLiteral("unknown");
        m_aiPolicyNote = uiText(QStringLiteral("La CLI archie no está instalada en este equipo."), QStringLiteral("The archie CLI is not installed on this machine."));
        emit aiPolicyChanged();
        return;
    }
    m_aiProcess = new QProcess(this);
    m_aiProcess->setProgram(archie);
    m_aiProcess->setArguments({QStringLiteral("--json"), QStringLiteral("ai"), verb});
    m_aiProcess->setProcessChannelMode(QProcess::SeparateChannels);
    auto *process = m_aiProcess;
    QTimer::singleShot(15000, process, [process] { process->kill(); });
    connect(process, qOverload<int, QProcess::ExitStatus>(&QProcess::finished), this,
            [this, process, verb](int code, QProcess::ExitStatus status) {
        const QByteArray out = process->readAllStandardOutput().left(4096);
        const QString err = QString::fromUtf8(process->readAllStandardError().right(400)).trimmed();
        const QJsonObject answer = QJsonDocument::fromJson(out.trimmed()).object();
        if (status == QProcess::NormalExit && code == 0 && answer.contains(QStringLiteral("paused"))) {
            const bool paused = answer.value(QStringLiteral("paused")).toBool();
            const bool recorded = answer.value(QStringLiteral("recorded")).toBool();
            m_aiPolicy = paused ? QStringLiteral("paused") : QStringLiteral("running");
            m_aiPolicyNote = paused == recorded ? QString()
                : (paused ? uiText(QStringLiteral("Pausada, pero sin registro: un reinicio la levantaría."), QStringLiteral("Paused, but not recorded: a restart would resume it."))
                          : uiText(QStringLiteral("En marcha, pero queda un registro de pausa: un reinicio pausaría."), QStringLiteral("Running, but a pause record remains: a restart would pause.")));
        } else {
            m_aiPolicy = QStringLiteral("unknown");
            m_aiPolicyNote = (verb == QLatin1String("status") ? uiText(QStringLiteral("Core no respondió: "), QStringLiteral("Core did not answer: "))
                                                              : uiText(QStringLiteral("Core rechazó la orden: "), QStringLiteral("Core refused the order: ")))
                + (err.isEmpty() ? QStringLiteral("sin respuesta") : err.split(QLatin1Char('\n')).last());
        }
        m_aiProcess = nullptr;
        process->deleteLater();
        emit aiPolicyChanged();
    });
    connect(process, &QProcess::errorOccurred, this, [this, process](QProcess::ProcessError error) {
        if (error != QProcess::FailedToStart) return;
        m_aiPolicy = QStringLiteral("unknown");
        m_aiPolicyNote = QStringLiteral("No se pudo ejecutar archie: ") + process->errorString();
        m_aiProcess = nullptr;
        process->deleteLater();
        emit aiPolicyChanged();
    });
    process->start();
    emit aiPolicyChanged();
}

void ShellRuntime::localModelsRefresh() { localModelRequest({QStringLiteral("list"), QStringLiteral("--json")}, 15000); }

void ShellRuntime::useLocalModel(const QString &name)
{
    for (const QVariant &model : std::as_const(m_localModels)) {
        if (model.toMap().value(QStringLiteral("name")).toString() == name) {
            // The 27B reads 5.9 GB before it answers; from a slow disk that takes minutes.
            localModelRequest({QStringLiteral("use"), name}, 15 * 60 * 1000);
            return;
        }
    }
    m_localModelNote = uiText(QStringLiteral("archie-local no ofrece ese modelo."), QStringLiteral("archie-local does not offer that model."));
    emit localModelsChanged();
}

void ShellRuntime::localModelRequest(const QStringList &arguments, int timeoutMs)
{
    if (m_localProcess) return; // one at a time; a switch in progress finishes first
    const QString program = QStandardPaths::findExecutable(QStringLiteral("archie-local"));
    if (program.isEmpty()) {
        m_localModels.clear();
        m_localModelNote = uiText(QStringLiteral("archie-local no está instalado en este equipo."), QStringLiteral("archie-local is not installed on this machine."));
        emit localModelsChanged();
        return;
    }
    m_localProcess = new QProcess(this);
    m_localProcess->setProgram(program);
    m_localProcess->setArguments(arguments);
    m_localProcess->setProcessChannelMode(QProcess::SeparateChannels);
    auto *process = m_localProcess;
    QTimer::singleShot(timeoutMs, process, [process] { process->kill(); });
    const bool listing = arguments.first() == QLatin1String("list");
    connect(process, qOverload<int, QProcess::ExitStatus>(&QProcess::finished), this,
            [this, process, listing](int code, QProcess::ExitStatus status) {
        const QByteArray out = process->readAllStandardOutput().left(16384);
        const QString err = QString::fromUtf8(process->readAllStandardError().right(400)).trimmed();
        const bool ok = status == QProcess::NormalExit && code == 0;
        if (listing && ok) {
            QVariantList models;
            for (const QJsonValue &value : QJsonDocument::fromJson(out.trimmed()).array()) {
                const QJsonObject model = value.toObject();
                if (model.value(QStringLiteral("name")).isString()) models.append(model.toVariantMap());
            }
            m_localModels = models;
            m_localModelNote = models.isEmpty() ? uiText(QStringLiteral("archie-local no listó modelos."), QStringLiteral("archie-local listed no models.")) : QString();
        } else if (!ok) {
            m_localModelNote = (listing ? uiText(QStringLiteral("archie-local no respondió: "), QStringLiteral("archie-local did not answer: "))
                                        : uiText(QStringLiteral("No se pudo cambiar de modelo: "), QStringLiteral("Could not switch model: ")))
                + (err.isEmpty() ? QStringLiteral("sin respuesta") : err.split(QLatin1Char('\n')).last());
        }
        m_localProcess = nullptr;
        process->deleteLater();
        emit localModelsChanged();
        // After a switch, read back which one answers now.
        if (!listing && ok) localModelsRefresh();
    });
    connect(process, &QProcess::errorOccurred, this, [this, process](QProcess::ProcessError error) {
        if (error != QProcess::FailedToStart) return;
        m_localModelNote = QStringLiteral("No se pudo ejecutar archie-local: ") + process->errorString();
        m_localProcess = nullptr;
        process->deleteLater();
        emit localModelsChanged();
    });
    process->start();
    emit localModelsChanged();
}

ShellRuntime::~ShellRuntime()
{
    m_wanted = false;
    m_restart.stop();
    if (m_aiProcess) {
        m_aiProcess->kill();
        m_aiProcess->waitForFinished(500);
    }
    if (m_localProcess) {
        m_localProcess->kill();
        m_localProcess->waitForFinished(500);
    }
    if (m_process.state() != QProcess::NotRunning) {
        m_process.kill();
        m_process.waitForFinished(500);
    }
}

bool ShellRuntime::running() const { return m_process.state() == QProcess::Running; }

void ShellRuntime::setError(const QString &message)
{
    if (message == m_error) return;
    m_error = message;
    emit errorChanged();
}

void ShellRuntime::setRunning(bool enabled)
{
    if (m_wanted == enabled) return;
    m_wanted = enabled;
    if (enabled) {
        start();
    } else {
        m_restart.stop();
        m_pending.clear();
        if (m_process.state() != QProcess::NotRunning) {
            m_process.terminate();
            m_stop.start();
        }
    }
}

void ShellRuntime::start()
{
    if (!m_wanted || m_process.state() != QProcess::NotRunning) return;
    m_pending.clear();
    m_process.start();
}

void ShellRuntime::reconnect()
{
    if (m_wanted && !m_restart.isActive()) m_restart.start();
}

void ShellRuntime::readFrames()
{
    while (m_process.bytesAvailable() > 0) {
        const QByteArray chunk = m_process.read(64 * 1024);
        if (!m_wanted) continue;
        m_pending.append(chunk);
        while (true) {
            const qsizetype end = m_pending.indexOf('\n');
            if (end < 0) break;
            if (end > maximumFrameBytes) break;
            QByteArray line = m_pending.left(end);
            m_pending.remove(0, end + 1);
            if (line.endsWith('\r')) line.chop(1);
            QStringDecoder decode(QStringDecoder::Utf8);
            const QString text = decode(line);
            if (decode.hasError()) {
                setError(QStringLiteral("shell bridge emitted invalid UTF-8"));
                m_pending.clear();
                m_process.kill();
                return;
            }
            if (!line.isEmpty()) emit frameReady(text);
        }
        if (m_pending.size() > maximumFrameBytes) {
            setError(QStringLiteral("shell bridge frame exceeded 1 MiB"));
            m_pending.clear();
            m_process.kill();
            return;
        }
    }
}
