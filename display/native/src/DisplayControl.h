#pragma once

#include <QObject>
#include <QStringList>
#include <QTcpServer>
#include <QVariantMap>

#include <functional>

class LumenField;
class QJsonObject;

// Loopback-only visual controls for the CLI and Archie's live voice: one JSON line in,
// one line out on 127.0.0.1:7668. No OS or Stage action enters this channel. The display
// is the only authority on what it can show, so callers ask it (`list`) instead of keeping
// their own copy of the names, and every command is checked here before anything moves:
//
//   {"action":"list"}                  -> {"scenes":[...],"presets":[...]}
//   {"action":"scene","value":<name>}  -> ok | error: <reason>
//   {"action":"preset","value":<name>} -> ok | error: <reason>   (opens Orb)
//
// What the AI needs to see what it does (Observatory only, each checked before it moves):
//   {"action":"focus","value":<node id>}            -> ok   (an empty value lets go)
//   {"action":"camera","value":home|closer|farther|left|right|up|down}
//   {"action":"window","value":windowed|fullscreen|maximized}
//   {"action":"pointer","x":0..1,"y":0..1,"click":bool}   a marker, and a click when asked
//   {"action":"highlight","value":<node id>,"enabled":bool}
//   {"action":"popup","value":<text, 200 characters at most>,"seconds":1..30}
//   {"action":"freeze","enabled":bool}              stops the sphere's clock (for tests)
//   {"action":"capture"}                            -> ok <path of the PNG just written>
class DisplayControl final : public QObject
{
    Q_OBJECT
    /// The Observatory's scenes, in the order the dock shows them.
    Q_PROPERTY(QStringList scenes READ scenes NOTIFY scenesChanged)

public:
    explicit DisplayControl(LumenField *lumen = nullptr, QObject *parent = nullptr, bool shellMode = false);
    void setShellMode(bool enabled) { if(m_shellMode==enabled) return; m_shellMode = enabled; emit scenesChanged(); }
    // The shell advertises World only after its native host was constructed.
    void setWorldAvailable(bool available) { if(m_worldAvailable==available) return; m_worldAvailable = available; emit scenesChanged(); }
    bool start(quint16 port = 7668);
    quint16 port() const { return m_server.serverPort(); }
    /// What `capture` does: write the window to the path it is given and say whether it did.
    void setCapture(std::function<bool(const QString &)> capture) { m_capture = std::move(capture); }
    // Empty result means the active view applied the command; otherwise return its error.
    void setViewHandler(std::function<QString(const QString &, const QVariantMap &)> handler) { m_viewHandler=std::move(handler); }
    /// Where captures are written: a folder of the display's own, never one a caller names.
    QString captureFolder() const;

    QStringList scenes() const;
    /// The scene a name opens ("core" is FORGE's old name), or "" when there is none.
    Q_INVOKABLE QString sceneNamed(const QString &name) const;
    /// The displayd mode a scene runs in: WORLD watches, the others think.
    Q_INVOKABLE QString modeOf(const QString &scene) const;
    /// The shell's own dock asking for a scene: the same checked, presentation-only command
    /// the loopback channel takes, from inside the display. False when there is no such scene.
    Q_INVOKABLE bool showScene(const QString &name);

signals:
    void scenesChanged();
    /// A checked local view command; presets/cinematics are applied before notification.
    void commandReceived(const QString &action, const QString &value, bool enabled);
    /// A checked command of the new verbs; its arguments are already of the right type and range.
    void viewCommand(const QString &action, const QVariantMap &arguments);

private:
    QByteArray answer(const QByteArray &line);
    QByteArray answerView(const QString &action, const QJsonObject &command);
    std::function<bool(const QString &)> m_capture;
    std::function<QString(const QString &, const QVariantMap &)> m_viewHandler;

    LumenField *m_lumen = nullptr;
    QTcpServer m_server;
    bool m_shellMode = false;
    bool m_worldAvailable = false;
};
