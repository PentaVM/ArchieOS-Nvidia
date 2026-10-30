#include "ShellRuntime.h"
#include <QCoreApplication>
#include <QFile>
#include <QFileInfo>
#include <QDir>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QtTest>

class ShellRuntimeContracts final : public QObject {
    Q_OBJECT
private slots:
    void framesAreCompleteAndArgumentsStayLiteral() {
        ShellRuntime runtime(QCoreApplication::applicationFilePath(),
                             QStringLiteral("fixture:frames & literal"), QStringLiteral("/safe workspaces"));
        QSignalSpy frames(&runtime, &ShellRuntime::frameReady);
        runtime.setRunning(true);
        QTRY_VERIFY_WITH_TIMEOUT(frames.count() >= 2, 4000);
        QCOMPARE(frames[0][0].toString(), QStringLiteral("{\"schema\":1}"));
        const auto args = QJsonDocument::fromJson(frames[1][0].toString().toUtf8()).array();
        QCOMPARE(args[0].toString(), QStringLiteral("watch"));
        QCOMPARE(args[2].toString(), QStringLiteral("fixture:frames & literal"));
        QCOMPARE(args[4].toString(), QStringLiteral("/safe workspaces"));
        QStringList childArguments;
        for (const auto &arg : args) childArguments.append(arg.toString());
        const int displaydAt = childArguments.indexOf(QStringLiteral("--displayd"));
        QVERIFY2(displaydAt >= 0, "the child must receive the host's explicit projection endpoint");
        QCOMPARE(childArguments.value(displaydAt + 1), QStringLiteral("127.0.0.1:7666"));
        runtime.setRunning(false);
        QTRY_VERIFY(!runtime.running());
        const int count = frames.count();
        QTest::qWait(150);
        QCOMPARE(frames.count(), count);
    }
    void anOversizedUnterminatedFrameIsRejected() {
        ShellRuntime runtime(QCoreApplication::applicationFilePath(), QStringLiteral("fixture:oversize"), QString());
        QSignalSpy frames(&runtime, &ShellRuntime::frameReady);
        runtime.setRunning(true);
        QTRY_VERIFY_WITH_TIMEOUT(!runtime.error().isEmpty(), 4000);
        QCOMPARE(frames.count(), 0);
        runtime.setRunning(false);
    }
    void customProjectionPortReachesTheBridge() {
        ShellRuntime runtime(QCoreApplication::applicationFilePath(), QStringLiteral("fixture:frames"),
                             QString(), 17666);
        QSignalSpy frames(&runtime, &ShellRuntime::frameReady);
        runtime.setRunning(true);
        QTRY_VERIFY_WITH_TIMEOUT(frames.count() >= 2, 4000);
        const auto args = QJsonDocument::fromJson(frames[1][0].toString().toUtf8()).array();
        QStringList childArguments;
        for (const auto &arg : args) childArguments.append(arg.toString());
        const int displaydAt = childArguments.indexOf(QStringLiteral("--displayd"));
        QVERIFY(displaydAt >= 0);
        QCOMPARE(childArguments.value(displaydAt + 1), QStringLiteral("127.0.0.1:17666"));
        runtime.setRunning(false);
        QTRY_VERIFY(!runtime.running());
    }
    void missingProgramIsReportedWithoutBlocking() {
        ShellRuntime runtime(QStringLiteral("/missing/archie-shell-bridge"), QString(), QString());
        runtime.setRunning(true);
        QTRY_VERIFY_WITH_TIMEOUT(!runtime.error().isEmpty(), 2000);
        QVERIFY(!runtime.running());
        runtime.setRunning(false);
    }
    // Settings > Modelos lists archie-local's models and switches to one of them, never to a
    // name the list did not give. The archie-local here is a stand-in that records its arguments.
    void localModelsAreListedAndOnlyAListedOneIsUsed() {
        QTemporaryDir bin;
        QVERIFY(bin.isValid());
#ifdef Q_OS_WIN
        QVERIFY(QFile::copy(QCoreApplication::applicationFilePath(), bin.filePath(QStringLiteral("archie-local.exe"))));
#else
        QFile tool(bin.filePath(QStringLiteral("archie-local")));
        QVERIFY(tool.open(QIODevice::WriteOnly));
        tool.write("#!/bin/sh\n"
                   "echo \"$*\" >> \"$(dirname \"$0\")/calls\"\n"
                   "case \"$1\" in\n"
                   "  list) echo '[{\"name\":\"27b\",\"label\":\"Bonsai 2 27B\",\"port\":8082,\"here\":true,\"answering\":false,\"chosen\":false},"
                   "{\"name\":\"9b\",\"label\":\"Bonsai 2 9B\",\"port\":8081,\"here\":true,\"answering\":true,\"chosen\":true}]' ;;\n"
                   "  use) [ \"$2\" = 27b ] || { echo 'archie-local: no' >&2; exit 64; } ;;\n"
                   "esac\n");
        tool.close();
        QVERIFY(tool.setPermissions(QFile::ReadOwner | QFile::WriteOwner | QFile::ExeOwner));
#endif
        const QByteArray path = qgetenv("PATH");
        qputenv("PATH", bin.path().toUtf8() + QDir::listSeparator().toLatin1() + path);

        ShellRuntime runtime(QStringLiteral("/missing/archie-shell-bridge"), QString(), QString());
        runtime.localModelsRefresh();
        QTRY_COMPARE_WITH_TIMEOUT(runtime.localModels().size(), 2, 4000);
        QTRY_VERIFY(!runtime.localModelBusy());
        QCOMPARE(runtime.localModels()[1].toMap().value(QStringLiteral("answering")).toBool(), true);

        runtime.useLocalModel(QStringLiteral("70b; rm -rf /"));
        QVERIFY(!runtime.localModelBusy());
        QVERIFY(!runtime.localModelNote().isEmpty());

        runtime.useLocalModel(QStringLiteral("27b"));
        QVERIFY(runtime.localModelBusy());
        // The switch, then the list read back.
        QTRY_VERIFY_WITH_TIMEOUT(!runtime.localModelBusy() && runtime.localModelNote().isEmpty(), 4000);
        qputenv("PATH", path);
        const auto calls = [&bin] {
            QFile file(bin.filePath(QStringLiteral("calls")));
            return file.open(QIODevice::ReadOnly) ? file.readAll() : QByteArray();
        };
        QTRY_COMPARE_WITH_TIMEOUT(calls(), QByteArray("list --json\nuse 27b\nlist --json\n"), 4000);
    }
    void unexpectedExitReconnectsAndStopCancelsReconnect() {
        ShellRuntime runtime(QCoreApplication::applicationFilePath(), QStringLiteral("fixture:exit"), QString());
        QSignalSpy frames(&runtime, &ShellRuntime::frameReady);
        runtime.setRunning(true);
        QTRY_VERIFY_WITH_TIMEOUT(frames.count() >= 2, 6500);
        runtime.setRunning(false);
        const int count = frames.count();
        QTest::qWait(3200);
        QCOMPARE(frames.count(), count);
    }
};

int main(int argc, char **argv) {
    QCoreApplication app(argc, argv);
    const auto args = app.arguments();
#ifdef Q_OS_WIN
    // The real child-process boundary is the fixture, without requiring /bin/sh.
    if (QFileInfo(app.applicationFilePath()).baseName() == QStringLiteral("archie-local")) {
        QFile calls(QDir(app.applicationDirPath()).filePath(QStringLiteral("calls")));
        if (!calls.open(QIODevice::WriteOnly | QIODevice::Append)) return 2;
        calls.write(args.mid(1).join(QLatin1Char(' ')).toUtf8() + '\n');
        if (args.value(1) == QStringLiteral("list")) {
            QFile output;
            if (!output.open(stdout,QIODevice::WriteOnly)) return 2;
            output.write(R"([{"name":"27b","label":"Bonsai 2 27B","port":8082,"here":true,"answering":false,"chosen":false},{"name":"9b","label":"Bonsai 2 9B","port":8081,"here":true,"answering":true,"chosen":true}])");
            output.flush();
            return 0;
        }
        return args.value(1) == QStringLiteral("use") && args.value(2) == QStringLiteral("27b") ? 0 : 64;
    }
#endif
    if (args.contains(QStringLiteral("watch"))) {
        QFile output;
        if (!output.open(stdout, QIODevice::WriteOnly)) return 2;
        const int at = args.indexOf(QStringLiteral("--lan-file"));
        const QString fixture = at >= 0 ? args.value(at + 1) : QString();
        if (fixture == QStringLiteral("fixture:oversize")) {
            output.write(QByteArray(1024 * 1024 + 1, 'x'));
            output.flush();
            QTest::qWait(2000);
        } else if (fixture == QStringLiteral("fixture:exit")) {
            output.write("{\"schema\":1}\n");
            output.flush();
        } else {
            output.write("{\"schema\""); output.flush();
            QTest::qWait(20);
            output.write(":1}\n");
            QJsonArray literal;
            for (const auto &arg : args.mid(1)) literal.append(arg);
            output.write(QJsonDocument(literal).toJson(QJsonDocument::Compact) + '\n');
            output.flush();
            QTest::qWait(2000);
        }
        return 0;
    }
    ShellRuntimeContracts tests;
    return QTest::qExec(&tests, argc, argv);
}
#include "test_shell_runtime.moc"
