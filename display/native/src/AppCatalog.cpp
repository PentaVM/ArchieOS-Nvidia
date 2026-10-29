#include "AppCatalog.h"
#include "UiText.h"

#include <QDir>
#include <QDirIterator>
#include <QFile>
#include <QFileInfo>
#include <QGuiApplication>
#include <QProcess>
#include <QRegularExpression>
#include <QStandardPaths>
#include <QTextStream>
#include <QUrl>

#include <algorithm>

namespace {

/// The `[Desktop Entry]` group of a desktop file, with localized keys resolved to Spanish
/// where the entry offers it.
QHash<QString, QString> readEntry(const QString &path)
{
    QHash<QString, QString> values;
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly | QIODevice::Text) || file.size() > 256 * 1024)
        return values;
    QTextStream in(&file);
    bool inEntry = false;
    QHash<QString, QString> localized;
    while (!in.atEnd()) {
        const QString line = in.readLine().trimmed();
        if (line.isEmpty() || line.startsWith(QLatin1Char('#')))
            continue;
        if (line.startsWith(QLatin1Char('['))) {
            inEntry = line == QLatin1String("[Desktop Entry]");
            continue;
        }
        if (!inEntry)
            continue;
        const qsizetype equals = line.indexOf(QLatin1Char('='));
        if (equals <= 0)
            continue;
        const QString key = line.left(equals).trimmed();
        const QString value = line.mid(equals + 1).trimmed();
        // Spanish names and comments, when the shell is read in Spanish.
        if (key.endsWith(QLatin1String("[es]")) && !uiEnglish())
            localized.insert(key.left(key.size() - 4), value);
        else if (!key.contains(QLatin1Char('[')))
            values.insert(key, value);
    }
    for (auto it = localized.begin(); it != localized.end(); ++it)
        values.insert(it.key(), it.value());
    return values;
}

QString groupOf(const QString &categories)
{
    const QStringList list = categories.split(QLatin1Char(';'), Qt::SkipEmptyParts);
    const auto has = [&list](const char *name) { return list.contains(QLatin1String(name)); };
    if (has("X-Archie")) return QStringLiteral("Archie");
    if (has("Electronics") || has("Engineering") || has("Science")) return uiText(QStringLiteral("Ingeniería"), QStringLiteral("Engineering"));
    if (has("Development") || has("IDE")) return uiText(QStringLiteral("Desarrollo"), QStringLiteral("Development"));
    if (has("Graphics") || has("3DGraphics")) return uiText(QStringLiteral("Gráficos"), QStringLiteral("Graphics"));
    if (has("AudioVideo") || has("Video") || has("Audio")) return QStringLiteral("Multimedia");
    if (has("Network") || has("WebBrowser")) return QStringLiteral("Internet");
    if (has("Office")) return uiText(QStringLiteral("Oficina"), QStringLiteral("Office"));
    if (has("Game")) return uiText(QStringLiteral("Juegos"), QStringLiteral("Games"));
    if (has("System") || has("Settings") || has("TerminalEmulator")) return uiText(QStringLiteral("Sistema"), QStringLiteral("System"));
    if (has("Utility")) return uiText(QStringLiteral("Utilidades"), QStringLiteral("Utilities"));
    return uiText(QStringLiteral("Otras"), QStringLiteral("Other"));
}

/// Exec without its field codes, split into a program and arguments.
QStringList commandOf(const QString &exec)
{
    QString text = exec;
    text.replace(QRegularExpression(QStringLiteral("%[fFuUdDnNickvm]")), QString());
    text.replace(QStringLiteral("%%"), QStringLiteral("%"));
    return QProcess::splitCommand(text.simplified());
}

QString binaryOf(const QStringList &command)
{
    // `env VAR=value program ...` names its program after the assignments.
    qsizetype at = 0;
    if (!command.isEmpty() && QFileInfo(command.first()).fileName() == QLatin1String("env")) {
        at = 1;
        while (at < command.size() && command.at(at).contains(QLatin1Char('=')))
            ++at;
    }
    return at < command.size() ? command.at(at) : QString();
}

QString expandShare(QString text)
{
    const QString share = qEnvironmentVariableIsEmpty("ARCHIE_SHARE")
                              ? QStringLiteral("/usr/share/archie")
                              : qEnvironmentVariable("ARCHIE_SHARE");
    text.replace(QStringLiteral("@ARCHIE_SHARE@"), share);
    return text;
}

struct Suggestion {
    const char *id;
    const char *name;
    const char *comment;
    const char *commentEn;
    const char *category;  // freedesktop main category; groupOf names it
    const char *package;
    const char *glyph;
    const char *iconName;
    const char *iconFile;
};

// Tools an operator of this machine is likely to want, shown when they are missing so the
// gap is visible. Installing is the operator's call and needs root; this only names the
// package.
constexpr Suggestion suggestions[] = {
    {"org.kicad.kicad", "KiCad", "Diseño de circuitos y placas", "Circuit and board design", "Engineering", "kicad", "K", "kicad", "kicad.png"},
    {"firefox", "Firefox", "Navegador web", "Web browser", "Network", "firefox", "F", "firefox", "firefox.svg"},
    {"org.kde.kdenlive", "Kdenlive", "Edición de video (el motor de Archie Cut)", "Video editing (Archie Cut's engine)", "AudioVideo", "kdenlive", "V", "kdenlive", "kdenlive.png"},
    {"org.freecad.FreeCAD", "FreeCAD", "Modelado CAD paramétrico", "Parametric CAD modeling", "Engineering", "freecad", "C", "org.freecad.FreeCAD", "freecad.svg"},
    {"blender", "Blender", "Modelado, animación y render 3D", "3D modeling, animation and rendering", "Graphics", "blender", "B", "blender", "blender.svg"},
    {"org.inkscape.Inkscape", "Inkscape", "Gráficos vectoriales", "Vector graphics", "Graphics", "inkscape", "I", "org.inkscape.Inkscape", "inkscape.svg"},
    {"gimp", "GIMP", "Edición de imágenes", "Image editing", "Graphics", "gimp", "G", "gimp", "gimp.svg"},
    {"com.obsproject.Studio", "OBS Studio", "Grabar y transmitir la pantalla", "Record and stream the screen", "AudioVideo", "obs-studio", "O", "com.obsproject.Studio", "obs-studio.png"},
    {"libreoffice-startcenter", "LibreOffice", "Documentos, hojas y presentaciones", "Documents, spreadsheets and presentations", "Office", "libreoffice-fresh", "L", "libreoffice-startcenter", "libreoffice.png"},
};

QString bundledIcon(const QString &id)
{
    for (const auto &suggestion : suggestions) {
        if (id != QLatin1String(suggestion.id)) continue;
        const QString path = QStringLiteral("/archie/assets/app-icons/") + QLatin1String(suggestion.iconFile);
        return QFile::exists(QLatin1Char(':') + path) ? QStringLiteral("qrc:") + path : QString();
    }
    return {};
}

}  // namespace

AppCatalog::AppCatalog(QObject *parent)
    : AppCatalog(QStandardPaths::standardLocations(QStandardPaths::ApplicationsLocation), parent)
{
}

AppCatalog::AppCatalog(const QStringList &applicationRoots, QObject *parent)
    : QObject(parent), m_applicationRoots(applicationRoots)
{
    const QString platform = QGuiApplication::platformName();
    m_canLaunch = platform == QLatin1String("wayland") || platform == QLatin1String("xcb");
    m_launchNote = m_canLaunch
                       ? uiText(QStringLiteral("Se abren en esta sesión, como el usuario que corre el observatorio."),
                                QStringLiteral("They open in this session, as the user running the observatory."))
                       : uiText(QStringLiteral("El observatorio corre sin escritorio (%1): aquí no hay dónde abrir una ventana. "
                                               "En la imagen, las aplicaciones se abren en el Stage."),
                                QStringLiteral("The observatory runs without a desktop (%1): there is nowhere to open a window here. "
                                               "In the image, applications open in the Stage."))
                             .arg(platform);
    m_runningTimer.setInterval(3000);
    m_runningTimer.setTimerType(Qt::CoarseTimer);
    connect(&m_runningTimer, &QTimer::timeout, this, [this] {
        countRunning();
        publish();
    });
    refresh();
}

void AppCatalog::setActive(bool active)
{
    if (m_active == active)
        return;
    m_active = active;
    if (active) {
        refresh();
        m_runningTimer.start();
    } else {
        m_runningTimer.stop();
    }
    emit activeChanged();
}

void AppCatalog::refresh()
{
    scan();
    readPolicy();
    countRunning();
    publish();
}

QString AppCatalog::resolveIcon(const QString &name) const
{
    if (name.isEmpty())
        return {};
    if (QFileInfo(name).isAbsolute())
        return QFileInfo::exists(name) ? QUrl::fromLocalFile(name).toString() : QString();
    static const char *sizes[] = {"scalable", "256x256", "128x128", "96x96", "64x64", "48x48"};
    QStringList bases = QStandardPaths::standardLocations(QStandardPaths::GenericDataLocation);
    for (const QString &base : std::as_const(bases)) {
        for (const char *size : sizes) {
            for (const char *extension : {".svg", ".png"}) {
                const QString path = base + QStringLiteral("/icons/hicolor/") + QLatin1String(size)
                                     + QStringLiteral("/apps/") + name + QLatin1String(extension);
                if (QFileInfo::exists(path))
                    return QUrl::fromLocalFile(path).toString();
            }
        }
        for (const char *extension : {".png", ".svg"}) {
            const QString path = base + QStringLiteral("/pixmaps/") + name + QLatin1String(extension);
            if (QFileInfo::exists(path))
                return QUrl::fromLocalFile(path).toString();
        }
    }
    return {};
}

void AppCatalog::scan()
{
    m_apps.clear();
    QSet<QString> seen;
    const auto add = [this, &seen](const QString &id, const QHash<QString, QString> &entry, const QString &section) {
        if (seen.contains(id) || entry.value(QStringLiteral("Type")) != QLatin1String("Application"))
            return;
        if (entry.value(QStringLiteral("NoDisplay")) == QLatin1String("true")
            || entry.value(QStringLiteral("Hidden")) == QLatin1String("true"))
            return;
        App app;
        app.id = id;
        app.section = section;
        app.name = entry.value(QStringLiteral("Name"), id);
        app.comment = entry.value(QStringLiteral("Comment"), entry.value(QStringLiteral("GenericName")));
        app.exec = expandShare(entry.value(QStringLiteral("Exec")));
        app.terminal = entry.value(QStringLiteral("Terminal")) == QLatin1String("true");
        app.internal = entry.value(QStringLiteral("X-Archie-Internal")) == QLatin1String("true");
        app.binary = binaryOf(commandOf(app.exec));
        app.group = groupOf(entry.value(QStringLiteral("Categories")));
        app.icon = resolveIcon(entry.value(QStringLiteral("Icon")));
        if (app.icon.isEmpty()) app.icon = bundledIcon(id);
        app.glyph = entry.value(QStringLiteral("X-Archie-Glyph"), app.name.left(1).toUpper());
        app.appId = entry.value(QStringLiteral("X-Archie-App-Id"), id);
        app.wmClass = entry.value(QStringLiteral("StartupWMClass"));
        const QString tryExec = expandShare(entry.value(QStringLiteral("TryExec"), app.binary));
        const bool found = !tryExec.isEmpty()
                           && (QFileInfo(tryExec).isAbsolute() ? QFileInfo(tryExec).isExecutable()
                                                               : !QStandardPaths::findExecutable(tryExec).isEmpty());
        if (!found)
            app.missing = QStringLiteral("falta ") + (tryExec.isEmpty() ? QStringLiteral("el ejecutable") : tryExec);
        const QString needsFile = expandShare(entry.value(QStringLiteral("X-Archie-Requires-File")));
        if (app.missing.isEmpty() && !needsFile.isEmpty() && !QFileInfo::exists(needsFile))
            app.missing = uiText(QStringLiteral("falta "), QStringLiteral("missing ")) + needsFile;
        const QString needsEnv = entry.value(QStringLiteral("X-Archie-Requires-Env"));
        if (app.missing.isEmpty() && !needsEnv.isEmpty() && qEnvironmentVariableIsEmpty(needsEnv.toLatin1().constData()))
            app.missing = uiText(QStringLiteral("necesita "), QStringLiteral("needs ")) + needsEnv
                + uiText(QStringLiteral(" en el entorno"), QStringLiteral(" in the environment"));
        app.available = app.missing.isEmpty();
        for (const auto &s : suggestions)
            if (id == QLatin1String(s.id))
                app.featured = true;
        seen.insert(id);
        m_apps.push_back(app);
    };

    // Archie's own, shipped inside the observatory.
    QDirIterator own(QStringLiteral(":/archie/apps"), {QStringLiteral("*.desktop")}, QDir::Files);
    while (own.hasNext()) {
        const QString path = own.next();
        add(QFileInfo(path).completeBaseName(), readEntry(path), QStringLiteral("archie"));
    }
    // Everything installed, as any application menu lists it. Earlier directories win,
    // as the XDG specification orders them.
    for (const QString &root : std::as_const(m_applicationRoots)) {
        QDirIterator it(root, {QStringLiteral("*.desktop")}, QDir::Files, QDirIterator::Subdirectories);
        int budget = 4000;
        while (it.hasNext() && budget-- > 0) {
            const QString path = it.next();
            QString id = QDir(root).relativeFilePath(path);
            id.chop(8);
            id.replace(QLatin1Char('/'), QLatin1Char('-'));
            add(id, readEntry(path), QStringLiteral("installed"));
        }
    }
    for (const auto &s : suggestions) {
        const QString id = QString::fromLatin1(s.id);
        if (seen.contains(id))
            continue;
        App app;
        app.id = id;
        app.name = QString::fromUtf8(s.name);
        app.comment = QString::fromUtf8(uiEnglish() ? s.commentEn : s.comment);
        app.group = groupOf(QString::fromLatin1(s.category));
        app.package = QString::fromLatin1(s.package);
        app.glyph = QString::fromUtf8(s.glyph);
        app.section = QStringLiteral("suggested");
        app.missing = uiText(QStringLiteral("no instalada · pacman -S "), QStringLiteral("not installed · pacman -S ")) + app.package;
        app.appId = id;
        app.icon = resolveIcon(QString::fromLatin1(s.iconName));
        if (app.icon.isEmpty()) app.icon = bundledIcon(id);
        m_apps.push_back(app);
    }
}

void AppCatalog::readPolicy()
{
    m_policyApps.clear();
    m_policyPath.clear();
    QStringList candidates;
    if (!qEnvironmentVariableIsEmpty("ARCHIE_STAGE_POLICY"))
        candidates << qEnvironmentVariable("ARCHIE_STAGE_POLICY");
    candidates << QStringLiteral("/etc/archie/stage/apps.toml") << expandShare(QStringLiteral("@ARCHIE_SHARE@/stage/apps.toml"));
    for (const QString &path : std::as_const(candidates)) {
        QFile file(path);
        if (!file.open(QIODevice::ReadOnly | QIODevice::Text))
            continue;
        m_policyPath = path;
        static const QRegularExpression appId(QStringLiteral("^\\s*app_id\\s*=\\s*\"([^\"]+)\""));
        QTextStream in(&file);
        while (!in.atEnd()) {
            const auto match = appId.match(in.readLine());
            if (match.hasMatch())
                m_policyApps.insert(match.captured(1));
        }
        break;
    }
    for (App &app : m_apps) {
        const QString binary = QFileInfo(app.binary).fileName();
        app.operable = m_policyApps.contains(app.appId) || m_policyApps.contains(app.id)
                       || (!app.wmClass.isEmpty() && m_policyApps.contains(app.wmClass))
                       || (!binary.isEmpty() && m_policyApps.contains(binary));
    }
}

void AppCatalog::countRunning()
{
    QHash<QString, int> counts;
    const QDir proc(QStringLiteral("/proc"));
    const QStringList pids = proc.entryList(QDir::Dirs | QDir::NoDotAndDotDot);
    for (const QString &pid : pids) {
        bool numeric = false;
        pid.toInt(&numeric);
        if (!numeric)
            continue;
        QFile comm(QStringLiteral("/proc/") + pid + QStringLiteral("/comm"));
        if (comm.open(QIODevice::ReadOnly))
            counts[QString::fromUtf8(comm.readAll()).trimmed()] += 1;
    }
    for (App &app : m_apps) {
        // The kernel keeps 15 characters of a process name.
        const QString name = QFileInfo(app.binary).fileName().left(15);
        app.running = name.isEmpty() ? 0 : counts.value(name, 0);
    }
}

void AppCatalog::publish()
{
    std::stable_sort(m_apps.begin(), m_apps.end(), [](const App &a, const App &b) {
        const auto rank = [](const App &x) {
            return x.section == QLatin1String("archie") ? 0 : x.section == QLatin1String("installed") ? 1 : 2;
        };
        if (rank(a) != rank(b))
            return rank(a) < rank(b);
        if (a.featured != b.featured)
            return a.featured;
        return a.name.localeAwareCompare(b.name) < 0;
    });
    QVariantList rows;
    QVariantList forgeRows;
    for (const App &app : std::as_const(m_apps)) {
        const QVariantMap row{
            {QStringLiteral("id"), app.id},
            {QStringLiteral("name"), app.name},
            {QStringLiteral("comment"), app.comment},
            {QStringLiteral("group"), app.group},
            {QStringLiteral("section"), app.section},
            {QStringLiteral("icon"), app.icon},
            {QStringLiteral("iconSource"), app.icon.startsWith(QLatin1String("qrc:/archie/assets/app-icons/"))
                ? QStringLiteral("official_bundle") : app.icon.isEmpty() ? QString() : QStringLiteral("installed")},
            {QStringLiteral("glyph"), app.glyph},
            {QStringLiteral("available"), app.available},
            {QStringLiteral("missing"), app.missing},
            {QStringLiteral("running"), app.running},
            {QStringLiteral("operable"), app.operable},
            {QStringLiteral("featured"), app.featured},
            {QStringLiteral("package"), app.package},
        };
        rows.push_back(row);
        if (!app.internal)
            forgeRows.push_back(row);
    }
    m_rows = rows;
    m_forgeRows = forgeRows;
    emit appsChanged();
}

bool AppCatalog::launch(const QString &id)
{
    const auto found = std::find_if(m_apps.cbegin(), m_apps.cend(), [&id](const App &app) { return app.id == id; });
    if (found == m_apps.cend()) {
        emit launched(id, false, uiText(QStringLiteral("no está en el catálogo"), QStringLiteral("not in the catalog")));
        return false;
    }
    if (!m_canLaunch) {
        emit launched(id, false, m_launchNote);
        return false;
    }
    if (!found->available) {
        emit launched(id, false, found->missing);
        return false;
    }
    QStringList command = commandOf(found->exec);
    if (command.isEmpty()) {
        emit launched(id, false, uiText(QStringLiteral("la entrada no dice qué ejecutar"), QStringLiteral("the entry does not say what to run")));
        return false;
    }
    if (found->terminal) {
        const QString terminal = QStandardPaths::findExecutable(QStringLiteral("foot"));
        if (terminal.isEmpty()) {
            emit launched(id, false, uiText(QStringLiteral("es de terminal y no hay terminal instalada"), QStringLiteral("it runs in a terminal and no terminal is installed")));
            return false;
        }
        command.prepend(terminal);
    }
    const QString program = command.takeFirst();
    const bool ok = QProcess::startDetached(program, command);
    emit launched(id, ok, ok ? QStringLiteral("abierta") : QStringLiteral("no se pudo iniciar ") + program);
    QTimer::singleShot(1500, this, [this] {
        countRunning();
        publish();
    });
    return ok;
}
