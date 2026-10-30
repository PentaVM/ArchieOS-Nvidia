#include "AppCatalog.h"
#include "ArchieBridge.h"
#include "LumenField.h"
#include "DisplayControl.h"
#include "ShellRuntime.h"
#include "WorldConsole.h"
#include "WorldImageProvider.h"
#include "GlobeStreamBridge.h"
#include "UiWalk.h"

#include <QGuiApplication>
#include <QIcon>
#include <QJsonArray>
#include <QJsonDocument>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQmlComponent>
#include <QPointer>
#include <QQuickStyle>
#include <QQuickWindow>
#include <QTimer>
#include <QFile>
#include <QImage>
#include <QCommandLineParser>
#include <QScreen>
#include <QEvent>
#include <QtQuick3D/qquick3d.h>

#include <algorithm>
#include <cstdlib>

// Presentation is a property of the actual host surface. A window can still
// exist while minimized or unexposed, and then must not buy a render budget.
class PresentationTracker final : public QObject {
public:
    PresentationTracker(QQuickWindow *window, ArchieBridge *bridge)
        : QObject(window), m_bridge(bridge) { trackWindow(window); }
    void trackWindow(QQuickWindow *window) {
        m_windows.push_back(window);
        window->installEventFilter(this);
        connect(window, &QWindow::visibilityChanged, this, [this] { update(); });
        connect(window, &QObject::destroyed, this, [this] { update(); });
        update();
    }
    bool eventFilter(QObject *, QEvent *event) override {
        if (event->type() == QEvent::Expose || event->type() == QEvent::Show
            || event->type() == QEvent::Hide || event->type() == QEvent::WindowStateChange) {
            QTimer::singleShot(0, this, [this] { update(); });
        }
        return false;
    }
private:
    void update() {
        const bool presented=std::any_of(m_windows.cbegin(),m_windows.cend(),[](const auto &window) {
            return window && window->isVisible() && window->isExposed()
                && window->visibility()!=QWindow::Minimized;
        });
        m_bridge->setPresented(presented);
    }
    QList<QPointer<QQuickWindow>> m_windows;
    ArchieBridge *m_bridge;
};

int main(int argc, char *argv[])
{
    QGuiApplication::setApplicationName(QStringLiteral("Archie Observatory"));
    QGuiApplication::setOrganizationName(QStringLiteral("PentaVM"));
    QQuickStyle::setStyle(QStringLiteral("Basic"));

    QGuiApplication app(argc, argv);
    // The ArchieOS mark everywhere the system shows this program: taskbar, switcher, title.
    QGuiApplication::setApplicationDisplayName(QStringLiteral("ArchieOS"));
    // Also the Wayland app id: the Stage's policy and the image's .desktop entry both say archie-display.
    QGuiApplication::setDesktopFileName(QStringLiteral("archie-display"));
    QGuiApplication::setWindowIcon(QIcon(QStringLiteral(":/archie/shell/brand/archieos-app-512.png")));
    QSurfaceFormat::setDefaultFormat(QQuick3D::idealSurfaceFormat());
    LumenField lumen;
    AppCatalog appCatalog;
    DisplayControl displayControl(&lumen);
    QCommandLineParser arguments;
    arguments.addHelpOption();
    arguments.addOption(QCommandLineOption(QStringLiteral("imagery-source"),QStringLiteral("Initial loaded imagery source preference; viewer-only."),QStringLiteral("source")));
    arguments.addOption(QCommandLineOption(QStringLiteral("displayd-port"),QStringLiteral("Trusted loopback read-only projection port."),QStringLiteral("port"),QStringLiteral("7666")));
    arguments.addOption(QCommandLineOption(QStringLiteral("control-port"),QStringLiteral("Loopback visual controls port."),QStringLiteral("port"),QStringLiteral("7668")));
    arguments.addOption(QCommandLineOption(QStringLiteral("observatory"), QStringLiteral("Open the standalone rendering/verification host instead of the desktop.")));
    arguments.addOption(QCommandLineOption(QStringLiteral("shell"), QStringLiteral("Open the native Archie desktop shell.")));
    arguments.addOption(QCommandLineOption(QStringLiteral("theme"), QStringLiteral("Shell theme: light or dark."), QStringLiteral("mode"), QStringLiteral("light")));
    arguments.addOption(QCommandLineOption(QStringLiteral("shell-bridge"), QStringLiteral("Trusted read-only shell bridge executable."),
        QStringLiteral("path"), QStringLiteral("archie-shell-bridge")));
    arguments.addOption(QCommandLineOption(QStringLiteral("lan-file"), QStringLiteral("Read-only LAN inventory projection."),
        QStringLiteral("path"), QStringLiteral("/var/lib/archie/sentry/lan.json")));
    arguments.addOption(QCommandLineOption(QStringLiteral("workspaces-root"), QStringLiteral("Read-only workspaces projection root."),
        QStringLiteral("path"), QStringLiteral("/archie/workspaces")));
    arguments.addOption(QCommandLineOption(QStringLiteral("render-check"),QStringLiteral("Write one real Qt frame to a new PNG and exit."),QStringLiteral("path")));
    arguments.addOption(QCommandLineOption(QStringLiteral("screen"), QStringLiteral("Full-screen display number (one-based)."), QStringLiteral("number"), QStringLiteral("1")));
    arguments.addOption(QCommandLineOption(QStringLiteral("scene"), QStringLiteral("Initial observatory scene: ") + displayControl.scenes().join(QStringLiteral(", ")) + QLatin1Char('.'), QStringLiteral("name"), displayControl.scenes().front()));
    arguments.addOption(QCommandLineOption(QStringLiteral("lens"), QStringLiteral("Initial orb lens: architecture, folders, or entities."), QStringLiteral("name"), QStringLiteral("architecture")));
    arguments.addOption(QCommandLineOption(QStringLiteral("preset"), QStringLiteral("Initial look preset, e.g. holograma (Xavi's desktop sphere)."), QStringLiteral("name")));
    arguments.addOption(QCommandLineOption(QStringLiteral("content-root"), QStringLiteral("Folder the content sphere lists (read-only)."), QStringLiteral("path")));
    arguments.addOption(QCommandLineOption(QStringLiteral("ui-walk"), QStringLiteral("Click through this JSON's UI steps and record real window captures (verification)."), QStringLiteral("steps")));
    arguments.addOption(QCommandLineOption(QStringLiteral("ui-walk-out"), QStringLiteral("Where --ui-walk writes its screenshots and report.jsonl."), QStringLiteral("dir"), QStringLiteral("ui-walk")));
    // World's console: every value is the host's. The console itself accepts no flags.
    arguments.addOption(QCommandLineOption(QStringLiteral("archie-root"), QStringLiteral("Core node root the World console's archie CLI reads."), QStringLiteral("path")));
    arguments.addOption(QCommandLineOption(QStringLiteral("world-context"), QStringLiteral("ContextDomain (JSON string) for World console operations, e.g. \"personal/owner\"."), QStringLiteral("json")));
    arguments.addOption(QCommandLineOption(QStringLiteral("stage-socket"), QStringLiteral("Stage operator socket for IRIS goals from the console."), QStringLiteral("path")));
    arguments.addOption(QCommandLineOption(QStringLiteral("stage-key-file"), QStringLiteral("Stage operator key file for IRIS goals."), QStringLiteral("path")));
    arguments.addOption(QCommandLineOption(QStringLiteral("stage-policy"), QStringLiteral("Stage Policy file for IRIS goals."), QStringLiteral("path")));
    arguments.addOption(QCommandLineOption(QStringLiteral("ground-model"), QStringLiteral("IRIS grounding model for console goals."), QStringLiteral("name")));
    arguments.addOption(QCommandLineOption(QStringLiteral("iris-health"), QStringLiteral("Loopback IRIS health URL."), QStringLiteral("url"), QStringLiteral("http://127.0.0.1:8765/health")));
    arguments.addOption(QCommandLineOption(QStringLiteral("no-boot-splash"), QStringLiteral("Start without the three-second ArchieOS boot animation.")));
    arguments.process(app);
    const bool shellMode = !arguments.isSet(QStringLiteral("observatory"));
    const QString theme=arguments.value(QStringLiteral("theme"));
    if (theme != QLatin1String("dark") && theme != QLatin1String("light")) {
        qCritical("archie-display: theme must be dark or light");
        return EXIT_FAILURE;
    }
    bool portValid=false,controlValid=false;
    const auto projectionPort=arguments.value(QStringLiteral("displayd-port")).toUInt(&portValid);
    const auto controlPort=arguments.value(QStringLiteral("control-port")).toUInt(&controlValid);
    if(!portValid||projectionPort==0||projectionPort>65535||!controlValid||controlPort>65535){qCritical("archie-display: invalid loopback port");return EXIT_FAILURE;}
    ArchieBridge bridge(nullptr, shellMode,static_cast<quint16>(projectionPort));
    GlobeStreamBridge globeStream;
    const auto captureAllowed=[&bridge,&globeStream] {
        if(globeStream.sourceId()==QStringLiteral("google_satellite")||globeStream.proprietaryActive())return false;
        for(const auto &entry:bridge.imagerySources()) {
            const auto source=entry.toMap();
            if(source.value(QStringLiteral("source_id")).toString()==bridge.selectedImagery() &&
               source.value(QStringLiteral("mode")).toString()==QStringLiteral("online_preview_only")) return false;
        }
        return true;
    };
    bridge.setPresented(false);
    if(arguments.isSet(QStringLiteral("imagery-source"))){bridge.setSelectedImagery(arguments.value(QStringLiteral("imagery-source")));globeStream.setSourceId(arguments.value(QStringLiteral("imagery-source")));}
    displayControl.setShellMode(shellMode);
    ShellRuntime shellRuntime(arguments.value(QStringLiteral("shell-bridge")),
        arguments.value(QStringLiteral("lan-file")), arguments.value(QStringLiteral("workspaces-root")),
        static_cast<quint16>(projectionPort));
    WorldConsole::Config consoleConfig;
    if (arguments.isSet(QStringLiteral("archie-root")))
        consoleConfig.programPrefixArguments={QStringLiteral("--root"),arguments.value(QStringLiteral("archie-root"))};
    // A bare personal/owner is the same domain as its JSON string "personal/owner".
    QString context=arguments.value(QStringLiteral("world-context")).trimmed();
    if(!context.isEmpty() && !context.startsWith(QLatin1Char('"')))
        context=QString::fromUtf8(QJsonDocument(QJsonArray{context}).toJson(QJsonDocument::Compact)).mid(1).chopped(1);
    consoleConfig.contextJson=context;
    consoleConfig.stageSocket=arguments.value(QStringLiteral("stage-socket"));
    consoleConfig.stageKeyFile=arguments.value(QStringLiteral("stage-key-file"));
    consoleConfig.stagePolicy=arguments.value(QStringLiteral("stage-policy"));
    consoleConfig.groundModel=arguments.value(QStringLiteral("ground-model"));
    consoleConfig.irisHealthUrl=QUrl(arguments.value(QStringLiteral("iris-health")));
    WorldConsole worldConsole(consoleConfig);
    QQmlApplicationEngine engine;
    // The shell's words follow ARCHIE_UI_LANG (Lang.js reads it as Qt.uiLanguage); Spanish otherwise.
    engine.setUiLanguage(qEnvironmentVariable("ARCHIE_UI_LANG"));
    auto *worldImages=new WorldImageProvider;engine.addImageProvider(QStringLiteral("world"),worldImages);bridge.setWorldImageProvider(worldImages);
    auto *globeImages=new WorldImageProvider;engine.addImageProvider(QStringLiteral("globe"),globeImages);globeStream.setImageProvider(globeImages);
    engine.rootContext()->setContextProperty(QStringLiteral("globeStream"), &globeStream);
    engine.rootContext()->setContextProperty(QStringLiteral("archieBridge"), &bridge);
    engine.rootContext()->setContextProperty(QStringLiteral("lumen"), &lumen);
    engine.rootContext()->setContextProperty(QStringLiteral("appCatalog"), &appCatalog);
    engine.rootContext()->setContextProperty(QStringLiteral("displayControl"), &displayControl);
    engine.rootContext()->setContextProperty(QStringLiteral("shellRuntime"), &shellRuntime);
    engine.rootContext()->setContextProperty(QStringLiteral("worldConsole"), &worldConsole);
    // The bar's domain chip shows the context the host chose for this desktop, and nothing
    // when it chose none. Core still validates it on every operation.
    const auto contextValue=QJsonDocument::fromJson("[" + consoleConfig.contextJson.toUtf8() + "]").array();
    engine.rootContext()->setContextProperty(QStringLiteral("hostDomain"),
        contextValue.size()==1 && contextValue.first().isString() ? contextValue.first().toString().left(160) : QString());
    // Run as the Archie Stage's desktop (display/stage/run-archie.sh DESKTOP=1): the stage
    // frames the applications Archie uses over this shell, so the shell starts bare.
    engine.rootContext()->setContextProperty(QStringLiteral("insideStage"),
        qEnvironmentVariableIntValue("ARCHIE_STAGE_DESKTOP") == 1);
    engine.rootContext()->setContextProperty(QStringLiteral("shellColorMode"), theme);
    engine.rootContext()->setContextProperty(QStringLiteral("bootSplash"),
        !arguments.isSet(QStringLiteral("no-boot-splash")) && qEnvironmentVariable("ARCHIE_BOOT_SPLASH") != QLatin1String("0")
        && !arguments.isSet(QStringLiteral("ui-walk")) && !arguments.isSet(QStringLiteral("render-check")));
    lumen.setLens(arguments.value(QStringLiteral("lens")));
    if (arguments.isSet(QStringLiteral("content-root")))
        lumen.setContentRoot(arguments.value(QStringLiteral("content-root")));
    if (arguments.isSet(QStringLiteral("preset")) && !lumen.applyProfile(arguments.value(QStringLiteral("preset"))))
        qWarning("archie-display: unknown preset %s", qPrintable(arguments.value(QStringLiteral("preset"))));
    if (shellMode) engine.load(QUrl(QStringLiteral("qrc:/archie/shell/prototype/ShellWindow.qml")));
    else engine.loadFromModule(QStringLiteral("Archie.Observatory"), QStringLiteral("Main"));
    if (engine.rootObjects().isEmpty())
        return EXIT_FAILURE;
    auto *window = qobject_cast<QQuickWindow *>(engine.rootObjects().front());
    auto *presentation=window ? new PresentationTracker(window, &bridge) : nullptr;
    // `archie visual capture`: the window as it is drawn now, into the display's own folder.
    if (window)
        displayControl.setViewHandler([window](const QString &action,const QVariantMap &arguments) {
            QVariant error;
            const bool invoked=QMetaObject::invokeMethod(window,"viewVerb",Q_RETURN_ARG(QVariant,error),
                Q_ARG(QVariant,QVariant(action)),Q_ARG(QVariant,QVariant(arguments)));
            return invoked && error.isValid() ? error.toString() : QStringLiteral("the active view did not acknowledge this command");
        });
    if (window)
        displayControl.setCapture([window,captureAllowed](const QString &path) {
            if (!captureAllowed()) return false;
            const QImage frame = window->grabWindow();
            return !frame.isNull() && frame.save(path, "PNG");
        });
    if (!shellMode) {
        QString scene = displayControl.sceneNamed(arguments.value(QStringLiteral("scene")));
        if (scene.isEmpty()) {
            qWarning("archie-display: unknown initial scene %s; using %s",
                     qPrintable(arguments.value(QStringLiteral("scene"))), qPrintable(displayControl.scenes().front()));
            scene = displayControl.scenes().front();
        }
        engine.rootObjects().front()->setProperty("scene", scene);
        bridge.requestMode(displayControl.modeOf(scene));
    }
    bool screenNumberOk = false;
    const int screenNumber = arguments.value(QStringLiteral("screen")).toInt(&screenNumberOk);
    const auto screens = QGuiApplication::screens();
    if (window && screenNumberOk && screenNumber > 0 && screenNumber <= screens.size()) {
        window->setScreen(screens.at(screenNumber - 1));
        window->setVisibility(QWindow::FullScreen);
    } else if (screenNumberOk && screenNumber > 0) {
        qWarning("archie-display: requested screen %d is unavailable; this WSL session exposes %lld screen(s)",
                 screenNumber, static_cast<long long>(screens.size()));
    }
    // World is a content view of this same desktop window: its dock, menus and
    // controller remain the existing shell's, with no overlapping World window.
    if (shellMode && window) {
        displayControl.setWorldAvailable(true);
        QMetaObject::invokeMethod(window, "showScene", Q_ARG(QVariant, arguments.value(QStringLiteral("scene"))));
    }
    if (!displayControl.start(static_cast<quint16>(controlPort)))
        qWarning("archie-display: local visual controls unavailable");
    if (shellMode) {
        shellRuntime.setRunning(true);
        QObject::connect(&app, &QCoreApplication::aboutToQuit, &shellRuntime, [&shellRuntime] { shellRuntime.setRunning(false); });
    }
    if (arguments.isSet(QStringLiteral("ui-walk"))) {
        if (!window) {
            qCritical("archie-display: --ui-walk needs a display window");
            return EXIT_FAILURE;
        }
        window->setVisibility(QWindow::Windowed);
        window->resize(1920, 1080);
        auto *walk = new UiWalk(window, arguments.value(QStringLiteral("ui-walk")), arguments.value(QStringLiteral("ui-walk-out")), &app);
        if (!walk->ready()) {
            qCritical("archie-display: --ui-walk: no steps read, or the report cannot be written");
            return EXIT_FAILURE;
        }
        // The bridge's first frames and the orb's first field arrive in the first seconds.
        walk->setCaptureAllowed(captureAllowed);
        walk->start(8000);
    }
    if (arguments.isSet(QStringLiteral("render-check"))) {
        if (!window) return EXIT_FAILURE;
        window->setVisibility(QWindow::Windowed);
        window->resize(1600,900);
        const QString path=arguments.value(QStringLiteral("render-check"));
        // A software renderer may need longer than 10 s for a field to settle.
        bool delayValid=false;
        const int delay=qEnvironmentVariableIntValue("ARCHIE_RENDER_CHECK_MS",&delayValid);
        QTimer::singleShot(delayValid ? std::clamp(delay,1000,120000) : 10000,&app,[window,path,shellMode,&shellRuntime,&bridge,&app,&lumen,captureAllowed] {
            if(!captureAllowed()) {
                qCritical("archie-display: online-preview imagery cannot be captured or exported");
                app.exit(EXIT_FAILURE);
                return;
            }
            qInfo("archie-display render-check: orb fps=%d", lumen.fps());
            const QImage frame=window->grabWindow();
            QFile file(path);
            const bool saved=!frame.isNull() && file.open(QIODevice::WriteOnly | QIODevice::NewOnly) && frame.save(&file,"PNG");
            if (shellMode) qInfo("archie-display render-check: bridge=%s child-running=%s", qPrintable(window->property("bridgeLink").toString()), shellRuntime.running() ? "true" : "false");
            if (shellMode) qInfo("archie-display render-check: orb=%s bridge-presented=%s budget-interval=%d", qPrintable(window->property("orbDiagnostics").toString()), bridge.presented() ? "true" : "false", bridge.frame().value(QStringLiteral("frameIntervalMs")).toInt());
            app.exit(saved ? EXIT_SUCCESS : EXIT_FAILURE);
        });
    }
    return app.exec();
}
