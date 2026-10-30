#pragma once

#include <QObject>
#include <QByteArray>
#include <QStringList>
#include <QUrl>
#include <QVariantList>
#include <QVariantMap>

class QProcess;
class QTimer;
class QNetworkAccessManager;
class QNetworkReply;

// A deliberately small command language. Only the native host may supply
// executables, scope, transport configuration or Stage credentials.
class WorldConsole final : public QObject {
    Q_OBJECT
    Q_PROPERTY(QVariantList entries READ entries NOTIFY entriesChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)
    Q_PROPERTY(QString error READ error NOTIFY errorChanged)
    Q_PROPERTY(QVariantMap lastResult READ lastResult NOTIFY lastResultChanged)
public:
    struct Config {
        QString program;
        QStringList programPrefixArguments;
        QString contextJson;
        QString voiceProgram;
        QStringList voicePrefixArguments;
        QString stageSocket;
        QString stageKeyFile;
        QString stagePolicy;
        QString groundModel;
        QUrl irisHealthUrl;
        int processTimeoutMs = 30000;
        int maxOutputBytes = 262144;
    };

    explicit WorldConsole(QObject *parent = nullptr);
    explicit WorldConsole(const Config &config, QObject *parent = nullptr);
    ~WorldConsole() override;
    QVariantList entries() const { return m_entries; }
    bool busy() const { return m_busy; }
    QString error() const { return m_error; }
    QVariantMap lastResult() const { return m_lastResult; }
    Q_INVOKABLE void submit(const QString &command);
    Q_INVOKABLE void cancel();
signals:
    void entriesChanged();
    void busyChanged();
    void errorChanged();
    void lastResultChanged();
    void resultReady(QVariantMap result);
private:
    const Config m_config;
    QVariantList m_entries;
    QVariantMap m_lastResult;
    QString m_error;
    QString m_operation;
    bool m_busy = false;
    bool m_voice = false;
    QProcess *m_process = nullptr;
    QNetworkAccessManager *m_network = nullptr;
    QNetworkReply *m_reply = nullptr;
    QTimer *m_timer = nullptr;
    QByteArray m_stdout;
    QByteArray m_stderr;
    void append(const QString &kind, const QString &text);
    void setBusy(bool busy);
    void fail(const QString &message);
    void succeed(QVariantMap result);
    void startProcess(const QString &program, const QStringList &arguments, bool voice);
    bool drainProcess();
    void finishProcess(int exitCode, bool crashed);
    void startHealth();
    void stopOwnedTransport();
    QString displayText(QString text) const;
    QVariant sanitize(const QVariant &value) const;
};
