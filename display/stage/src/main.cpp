// `archie-stage` — the compositor Archie works in.
//
// A Wayland compositor built on Qt Wayland Compositor. Applications connect to its socket
// like any desktop; Archie operates them through sealed commands on its control socket;
// a person watches, and takes over with a hand on the mouse or the keyboard.
//
// Runs on DRM/KMS through EGLFS on a node, or as a window inside another Wayland session
// (WSLg, a development desktop), where the applications it hosts appear inside that
// window. The behaviour is the same in both.

#include <QCommandLineParser>
#include <QFile>
#include <QFileInfo>
#include <QGuiApplication>
#include <QIcon>
#include <QScreen>
#include <QQmlApplicationEngine>
#include <QQuickWindow>
#include <QStandardPaths>
#include <QTimer>
#include <QUrl>

#include "CommandCodec.h"
#include "StageController.h"

int main(int argc, char *argv[])
{
    QGuiApplication app(argc, argv);
    QGuiApplication::setApplicationName(QStringLiteral("archie-stage"));
    QGuiApplication::setWindowIcon(QIcon(QStringLiteral(":/archie/brand/archieos-app-512.png")));
    QGuiApplication::setDesktopFileName(QStringLiteral("org.archieos.Stage"));

    QCommandLineParser parser;
    parser.setApplicationDescription(QStringLiteral("The Archie Stage compositor"));
    parser.addHelpOption();
    const QCommandLineOption socketName(QStringLiteral("socket-name"),
                                        QStringLiteral("Wayland socket applications connect to."),
                                        QStringLiteral("name"), QStringLiteral("archie-stage-0"));
    const QCommandLineOption control(QStringLiteral("control"), QStringLiteral("Control socket path."),
                                     QStringLiteral("path"));
    const QCommandLineOption keyFile(QStringLiteral("key-file"),
                                     QStringLiteral("File holding the 32-byte command key as hex."),
                                     QStringLiteral("path"));
    const QCommandLineOption reducedMotion(QStringLiteral("reduced-motion"),
                                           QStringLiteral("Cut instead of easing; no ripples."));
    const QCommandLineOption tier(QStringLiteral("tier"), QStringLiteral("normal, conserve or ultra."),
                                  QStringLiteral("tier"), QStringLiteral("normal"));
    const QCommandLineOption seats(QStringLiteral("seats"),
                                   QStringLiteral("exclusive: one seat, and a person's input takes the stage from the agent; "
                                                  "shared: the agent has a pointer and keyboard of its own beside the person's"),
                                   QStringLiteral("mode"), QStringLiteral("exclusive"));
    const QCommandLineOption tempo(QStringLiteral("tempo"),
                                   QStringLiteral("person, to be watched; agent, four actions a second; video; or live, input at once with the pointer's travel drawn over it."),
                                   QStringLiteral("tempo"), QStringLiteral("person"));
    const QCommandLineOption strokeSpeed(
        QStringLiteral("stroke-speed"),
        QStringLiteral("How fast a line is drawn, in pixels a second. An application sees the pointer at its "
                       "own frame rate, so a line drawn faster than about ten pixels a frame reaches it as "
                       "a polygon of its corners."),
        QStringLiteral("px/s"));
    const QCommandLineOption presenceFile(
        QStringLiteral("presence-file"),
        QStringLiteral("A file rewritten when a person touches the mouse or keyboard; its age is how long "
                       "the machine has been untouched. Empty writes none."),
        QStringLiteral("path"), QStringLiteral("/run/archie/person-input"));
    const QCommandLineOption desktopApp(
        QStringLiteral("desktop-app"),
        QStringLiteral("The app id of the desktop: its window titled --desktop-title is laid under every "
                       "other one, unframed, across the whole screen, and the others are framed between its "
                       "top bar and its dock. Empty: no desktop."),
        QStringLiteral("app-id"), QStringLiteral("archie-display"));
    const QCommandLineOption desktopTitle(QStringLiteral("desktop-title"),
                                          QStringLiteral("The title of the desktop's window."),
                                          QStringLiteral("title"), QStringLiteral("ArchieOS"));
    const QCommandLineOption narrationFile(
        QStringLiteral("narration-file"),
        QStringLiteral("A JSON file ({\"text\", \"lang\", \"at_ms\"}) Archie's voice rewrites with each line "
                       "it says on its own while it works; the stage shows it as a subtitle."),
        QStringLiteral("path"));
    const QCommandLineOption windowed(QStringLiteral("windowed"), QStringLiteral("Do not go full screen."));
    const QCommandLineOption screen(
        QStringLiteral("screen"),
        QStringLiteral("Which screen to take, counting from zero. Left out, Archie takes the last "
                       "one when there is more than one, so the person keeps the first."),
        QStringLiteral("n"));
    const QCommandLineOption width(QStringLiteral("width"), QStringLiteral("Window width."), QStringLiteral("px"),
                                   QStringLiteral("1600"));
    const QCommandLineOption height(QStringLiteral("height"), QStringLiteral("Window height."),
                                    QStringLiteral("px"), QStringLiteral("900"));
    const QCommandLineOption grab(QStringLiteral("grab"), QStringLiteral("Save a PNG of the stage."),
                                  QStringLiteral("path"));
    const QCommandLineOption grabAfter(QStringLiteral("grab-after-ms"), QStringLiteral("When to grab."),
                                       QStringLiteral("ms"), QStringLiteral("3000"));
    const QCommandLineOption grabEvery(QStringLiteral("grab-every-ms"),
                                       QStringLiteral("Keep grabbing to numbered files at this interval."),
                                       QStringLiteral("ms"));
    const QCommandLineOption quitAfter(QStringLiteral("quit-after-ms"), QStringLiteral("Exit after this long."),
                                       QStringLiteral("ms"));
    parser.addOptions({socketName, control, keyFile, reducedMotion, tier, tempo, seats, strokeSpeed, presenceFile,
                       desktopApp, desktopTitle, narrationFile, windowed, screen, width, height, grab, grabAfter,
                       grabEvery, quitAfter});
    parser.process(app);
    // The subtitle is read by the scene from a local file; Qt Quick reads local files only
    // when told it may. The scene is the stage's own, and reads nothing else.
    QString narrationUrl;
    if (parser.isSet(narrationFile) && !parser.value(narrationFile).isEmpty()) {
        qputenv("QML_XHR_ALLOW_FILE_READ", "1");
        narrationUrl = QUrl::fromLocalFile(QFileInfo(parser.value(narrationFile)).absoluteFilePath()).toString();
    }

    StageController::Options options;
    options.control_socket = parser.value(control);
    if (options.control_socket.isEmpty()) {
        const QString runtime = QStandardPaths::writableLocation(QStandardPaths::RuntimeLocation);
        options.control_socket = runtime + QStringLiteral("/archie-stage/control.sock");
    }
    if (parser.isSet(keyFile)) {
        QFile file(parser.value(keyFile));
        if (file.open(QIODevice::ReadOnly)) {
            if (const auto key = archie::stage::parse_key(file.readAll())) {
                options.key = *key;
            } else {
                qWarning("archie-stage: %s is not a 64-hex-character key", qUtf8Printable(parser.value(keyFile)));
            }
        } else {
            qWarning("archie-stage: cannot read %s", qUtf8Printable(parser.value(keyFile)));
        }
    }
    options.reduced_motion = parser.isSet(reducedMotion);
    options.presence_file = parser.value(presenceFile);
    if (parser.value(seats) == QLatin1String("shared")) {
        options.shared_seats = true;
    } else if (parser.value(seats) != QLatin1String("exclusive")) {
        qWarning("archie-stage: --seats is exclusive or shared");
    }
    if (parser.value(tempo) == QLatin1String("agent")) {
        // What IRIS needs to work an application: the same checks, the same cinematics,
        // on a clock that fits four actions in a second.
        options.playback = archie::visual::PlaybackPolicy::agent_tempo();
    } else if (parser.value(tempo) == QLatin1String("video")) {
        options.playback = archie::visual::PlaybackPolicy::video_tempo();
    } else if (parser.value(tempo) == QLatin1String("live")) {
        // A game or IRIS acting frame by frame: input at once, the pointer's travel drawn over.
        options.playback = archie::visual::PlaybackPolicy::live_tempo();
    } else if (parser.value(tempo) != QLatin1String("person")) {
        qWarning("archie-stage: --tempo is person, agent, video or live");
    }
    if (parser.isSet(strokeSpeed)) {
        bool read = false;
        const float speed = parser.value(strokeSpeed).toFloat(&read);
        if (read && speed > 0.0F) {
            options.playback.stroke_speed_px_per_s = speed;
        } else {
            qWarning("archie-stage: --stroke-speed wants pixels a second, above nothing");
        }
    }
    if (parser.value(tier) == QLatin1String("conserve")) {
        options.tier = archie::visual::RenderTier::Conserve;
    } else if (parser.value(tier) == QLatin1String("ultra")) {
        options.tier = archie::visual::RenderTier::Ultra;
    }

    // Which screen Archie works on. A stage that is half a screen wide, or cut off at the
    // edge of one, is a stage nobody can watch: when Archie takes a screen it takes all of
    // it. Left to itself it takes the last screen, so a person with two keeps the first.
    const QList<QScreen *> screens = QGuiApplication::screens();
    int taken = screens.size() > 1 ? screens.size() - 1 : 0;
    if (parser.isSet(screen)) {
        bool read = false;
        const int asked = parser.value(screen).toInt(&read);
        if (read && asked >= 0 && asked < screens.size()) {
            taken = asked;
        } else {
            qWarning("archie-stage: --screen is 0 to %lld", static_cast<long long>(screens.size() - 1));
        }
    }
    for (int at = 0; at < screens.size(); ++at) {
        const QRect where = screens.at(at)->geometry();
        qInfo("archie-stage: screen %d is %dx%d at %d,%d%s", at, where.width(), where.height(),
              where.x(), where.y(), at == taken ? " <- taken" : "");
    }
    const QRect stand = screens.isEmpty() ? QRect(0, 0, 1280, 800) : screens.at(taken)->geometry();

    StageController controller(options);
    QQmlApplicationEngine engine;
    // The stage's words follow ARCHIE_UI_LANG, as the shell's do (Lang.js); Spanish otherwise.
    engine.setUiLanguage(qEnvironmentVariable("ARCHIE_UI_LANG"));
    engine.setInitialProperties({
        {QStringLiteral("stage"), QVariant::fromValue(&controller)},
        {QStringLiteral("stageSocketName"), parser.value(socketName)},
        {QStringLiteral("fullscreen"), !parser.isSet(windowed)},
        {QStringLiteral("screenIndex"), taken},
        // Windowed, the size asked for; otherwise the whole screen, so the stage is never
        // three quarters of one with its edge off the side.
        {QStringLiteral("initialWidth"),
         parser.isSet(windowed) ? parser.value(width).toInt() : stand.width()},
        {QStringLiteral("initialHeight"),
         parser.isSet(windowed) ? parser.value(height).toInt() : stand.height()},
        {QStringLiteral("desktopApp"), parser.value(desktopApp)},
        {QStringLiteral("desktopTitle"), parser.value(desktopTitle)},
        {QStringLiteral("narrationUrl"), narrationUrl},
    });
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app,
                     [] { QCoreApplication::exit(1); }, Qt::QueuedConnection);
    engine.loadFromModule("Archie.Stage", "Main");

    if (parser.isSet(grab)) {
        const QString path = parser.value(grab);
        const auto save = [&controller](const QString &target) {
            if (QQuickWindow *window = controller.window()) {
                if (!window->grabWindow().save(target)) {
                    qWarning("archie-stage: could not save %s", qUtf8Printable(target));
                }
            }
        };
        if (parser.isSet(grabEvery)) {
            const int every = parser.value(grabEvery).toInt();
            auto *timer = new QTimer(&app);
            QObject::connect(timer, &QTimer::timeout, &app, [save, path, count = 0]() mutable {
                QString numbered = path;
                numbered.replace(QStringLiteral(".png"), QStringLiteral("-%1.png").arg(++count, 3, 10, QLatin1Char('0')));
                save(numbered);
            });
            QTimer::singleShot(parser.value(grabAfter).toInt(), timer, [timer, every] { timer->start(every); });
        } else {
            QTimer::singleShot(parser.value(grabAfter).toInt(), &app, [save, path] { save(path); });
        }
    }
    if (parser.isSet(quitAfter)) {
        QTimer::singleShot(parser.value(quitAfter).toInt(), &app, &QCoreApplication::quit);
    }
    return app.exec();
}
