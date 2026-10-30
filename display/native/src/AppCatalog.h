#pragma once

// FORGE: the applications this machine can open.
//
// Three sources, each said for what it is:
// - Archie's own applications, shipped with the observatory as desktop entries;
// - every desktop entry installed under the XDG application directories, which is what
//   any application menu on Linux lists (KiCad, Firefox, Kdenlive...);
// - a short list of well-known tools that are not installed, with the package that
//   provides them. Nothing is installed from here.
// Installed icons take precedence. Known applications have unmodified official
// fallback artwork embedded offline; source pins/licenses live in assets/app-icons.
//
// For each it reports whether it can start (its executable exists), whether it is running
// now (a process of that executable in /proc), and whether Archie may operate it (the
// Stage policy names it).
// Forge uses forgeApps, which omits manifests marked X-Archie-Internal=true. The full
// apps catalog remains available to shell controls such as the Stage dock launcher.
//
// Opening one is a person's action at the keyboard, not an agent's: it starts the
// program the entry names, as the user running this observatory, in this observatory's
// desktop session. Where there is none (the image runs the observatory on eglfs, with no
// other windows), it says so and opens nothing; there, opening goes through the Stage.

#include <QHash>
#include <QObject>
#include <QSet>
#include <QStringList>
#include <QTimer>
#include <QVariantList>

class AppCatalog final : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QVariantList apps READ apps NOTIFY appsChanged)
    Q_PROPERTY(QVariantList forgeApps READ forgeApps NOTIFY appsChanged)
    Q_PROPERTY(bool active READ active WRITE setActive NOTIFY activeChanged)
    Q_PROPERTY(bool canLaunch READ canLaunch CONSTANT)
    Q_PROPERTY(QString launchNote READ launchNote CONSTANT)
    Q_PROPERTY(QString policyPath READ policyPath NOTIFY appsChanged)

public:
    explicit AppCatalog(QObject *parent = nullptr);
    explicit AppCatalog(const QStringList &applicationRoots, QObject *parent = nullptr);

    QVariantList apps() const { return m_rows; }
    QVariantList forgeApps() const { return m_forgeRows; }
    bool active() const { return m_active; }
    void setActive(bool active);
    bool canLaunch() const { return m_canLaunch; }
    QString launchNote() const { return m_launchNote; }
    QString policyPath() const { return m_policyPath; }

    Q_INVOKABLE void refresh();
    Q_INVOKABLE bool launch(const QString &id);

signals:
    void appsChanged();
    void activeChanged();
    void launched(const QString &id, bool ok, const QString &message);

private:
    struct App {
        QString id;
        QString name;
        QString comment;
        QString exec;
        QString binary;
        QString icon;
        QString glyph;
        QString group;
        QString section;
        QString package;
        QString appId;
        QString wmClass;
        QString missing;
        bool terminal = false;
        bool internal = false;
        bool available = false;
        bool featured = false;
        int running = 0;
        bool operable = false;
    };

    void scan();
    void readPolicy();
    void countRunning();
    void publish();
    QString resolveIcon(const QString &name) const;

    QList<App> m_apps;
    QVariantList m_rows;
    QVariantList m_forgeRows;
    QStringList m_applicationRoots;
    QSet<QString> m_policyApps;
    QString m_policyPath;
    QTimer m_runningTimer;
    bool m_active = false;
    bool m_canLaunch = false;
    QString m_launchNote;
};
