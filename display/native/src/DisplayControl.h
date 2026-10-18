#pragma once

#include <QObject>
#include <QStringList>
#include <QTcpServer>

class LumenField;

// Loopback-only visual controls for the CLI and Archie's live voice: one JSON line in,
// one line out on 127.0.0.1:7668. No OS or Stage action enters this channel. The display
// is the only authority on what it can show, so callers ask it (`list`) instead of keeping
// their own copy of the names, and every command is checked here before anything moves:
//
//   {"action":"list"}                  -> {"scenes":[...],"presets":[...]}
//   {"action":"scene","value":<name>}  -> ok | error: <reason>
//   {"action":"preset","value":<name>} -> ok | error: <reason>   (opens Orb)
class DisplayControl final : public QObject
{
    Q_OBJECT
    /// The Observatory's scenes, in the order the dock shows them.
    Q_PROPERTY(QStringList scenes READ scenes CONSTANT)

public:
    explicit DisplayControl(LumenField *lumen, QObject *parent = nullptr);
    bool start();

    QStringList scenes() const;
    /// The scene a name opens ("core" is FORGE's old name), or "" when there is none.
    Q_INVOKABLE QString sceneNamed(const QString &name) const;
    /// The displayd mode a scene runs in: WORLD watches, the others think.
    Q_INVOKABLE QString modeOf(const QString &scene) const;

signals:
    /// A checked command: `scene` with a scene name, or `preset` once the preset is applied.
    void commandReceived(const QString &action, const QString &value);

private:
    QByteArray answer(const QByteArray &line);

    LumenField *m_lumen;
    QTcpServer m_server;
};
