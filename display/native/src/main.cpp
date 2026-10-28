#include "AppCatalog.h"
#include "ArchieBridge.h"
#include "LumenField.h"
#include "DisplayControl.h"
#include "ShellRuntime.h"
#include "WorldImageProvider.h"
#include "GlobeStreamBridge.h"
#include "UiWalk.h"

#include <QGuiApplication>
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
    QSurfaceFormat::setDefaultFormat(QQuick3D::idealSurfaceFormat());
    LumenField lumen;
    AppCatalog appCatalog;
    DisplayControl displayControl(&lumen);
    QCommandLineParser arguments;
    arguments.addHelpOption();
    arguments.addOption(QCommandLineOption(QStringLiteral("imagery-source"),QStringLiteral("Initial loaded imagery source preference; viewer-only."),QStringLiteral("source")));
    arguments.addOption(QCommandLineOption(QStringLiteral("displayd-port"),QStringLiteral("Trusted loopback read-only projection port."),QStringLiteral("port"),QStringLiteral("7666")));
    arguments.addOption(QCommandLineOption(QStringLiteral("control-port"),QStringLiteral("Loopback visual controls port."),QStringLiteral("port"),QStringLiteral("7668")));
    arguments.addOption(QCommandLineOption(QStringLiteral("shell"), QStringLiteral("Open the native Archie desktop shell.")));
    arguments.addOption(QCommandLineOption(QStringLiteral("theme"), QStringLiteral("Shell theme: dark or light (beta)."), QStringLiteral("mode"), QStringLiteral("dark")));
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
    arguments.addOption(QCommandLineOption(QStringLiteral("ui-walk"), QStringLiteral("Shell or World: click through the steps in this JSON file and record each click (verification)."), QStringLiteral("steps")));
    arguments.addOption(QCommandLineOption(QStringLiteral("ui-walk-out"), QStringLiteral("Where --ui-walk writes its screenshots and report.jsonl."), QStringLiteral("dir"), QStringLiteral("ui-walk")));
    arguments.process(app);
    const bool shellMode = arguments.isSet(QStringLiteral("shell"));
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
        arguments.value(QStringLiteral("lan-file")), arguments.value(QStringLiteral("workspaces-root")));
    QQmlApplicationEngine engine;
    auto *worldImages=new WorldImageProvider;engine.addImageProvider(QStringLiteral("world"),worldImages);bridge.setWorldImageProvider(worldImages);
    auto *globeImages=new WorldImageProvider;engine.addImageProvider(QStringLiteral("globe"),globeImages);globeStream.setImageProvider(globeImages);
    engine.rootContext()->setContextProperty(QStringLiteral("globeStream"), &globeStream);
    engine.rootContext()->setContextProperty(QStringLiteral("archieBridge"), &bridge);
    engine.rootContext()->setContextProperty(QStringLiteral("lumen"), &lumen);
    engine.rootContext()->setContextProperty(QStringLiteral("appCatalog"), &appCatalog);
    engine.rootContext()->setContextProperty(QStringLiteral("displayControl"), &displayControl);
    engine.rootContext()->setContextProperty(QStringLiteral("shellRuntime"), &shellRuntime);
    engine.rootContext()->setContextProperty(QStringLiteral("shellColorMode"), theme);
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
    // One listener, two host surfaces: the shell's World command opens the actual native
    // renderer in this same engine, preserving the shell Theme singleton and all authority.
    if (shellMode) {
        QQmlComponent worldComponent(&engine);
        worldComponent.loadFromModule(QStringLiteral("Archie.Observatory"),QStringLiteral("Main"));
        auto *object=worldComponent.createWithInitialProperties({
            {QStringLiteral("scene"),QStringLiteral("world")},
            {QStringLiteral("worldHost"),true},{QStringLiteral("visible"),false}});
        auto *worldWindow=qobject_cast<QQuickWindow*>(object);
        if(worldWindow) {
            object->setParent(&engine);
            if(presentation) presentation->trackWindow(worldWindow);
            displayControl.setWorldAvailable(true);
            QObject::connect(&displayControl,&DisplayControl::commandReceived,&engine,
                [worldWindow,&bridge](const QString &action,const QString &value,bool) {
                    if(action==QStringLiteral("scene") && value==QStringLiteral("world")) {
                        bridge.enableWorld();bridge.requestMode(QStringLiteral("watcher"));
                        worldWindow->showFullScreen();worldWindow->raise();worldWindow->requestActivate();
                    }
                });
            if(arguments.value(QStringLiteral("scene"))==QStringLiteral("world")) {
                bridge.enableWorld();bridge.requestMode(QStringLiteral("watcher"));worldWindow->showFullScreen();
            }
        } else {
            delete object;
            qWarning("archie-display: native World host unavailable: %s",qPrintable(worldComponent.errorString()));
        }
    }
    if (!displayControl.start(static_cast<quint16>(controlPort)))
        qWarning("archie-display: local visual controls unavailable");
    if (shellMode) {
        shellRuntime.setRunning(true);
        QObject::connect(&app, &QCoreApplication::aboutToQuit, &shellRuntime, [&shellRuntime] { shellRuntime.setRunning(false); });
    }
    if (arguments.isSet(QStringLiteral("ui-walk"))) {
        if (!window || (!shellMode && arguments.value(QStringLiteral("scene"))!=QStringLiteral("world"))) {
            qCritical("archie-display: --ui-walk needs --shell or --scene world");
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
