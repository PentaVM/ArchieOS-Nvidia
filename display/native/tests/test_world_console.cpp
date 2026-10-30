#include "WorldConsole.h"
#include <QCoreApplication>
#include <QFile>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QSignalSpy>
#include <QTcpServer>
#include <QTcpSocket>
#include <QtTest>

class WorldConsoleContracts final : public QObject {
    Q_OBJECT
    WorldConsole::Config config() const {
        WorldConsole::Config c;
        c.program = QCoreApplication::applicationFilePath();
        c.contextJson = QStringLiteral("\"work/acme/maps\"");
        return c;
    }
private slots:
    void quotedQueriesStayLiteralAndHostScopeIsFixed() {
        WorldConsole console(config());
        QSignalSpy results(&console, &WorldConsole::resultReady);
        console.submit(QStringLiteral("world resolve \"Santiago de Chile\""));
        QVERIFY(console.busy());
        QTRY_VERIFY_WITH_TIMEOUT(!console.busy(), 4000);
        QVERIFY2(console.error().isEmpty(), qPrintable(console.error()));
        QCOMPARE(results.size(), 1);
        const auto result = console.lastResult().value(QStringLiteral("result")).toMap();
        const auto args = result.value(QStringLiteral("argv")).toStringList();
        QCOMPARE(args.mid(0, 5), QStringList({QStringLiteral("--json"), QStringLiteral("world"),
            QStringLiteral("operator"), QStringLiteral("--context"), QStringLiteral("\"work/acme/maps\"")}));
        QCOMPARE(args.value(5), QStringLiteral("--request"));
        const auto request = QJsonDocument::fromJson(args.value(6).toUtf8()).object();
        QCOMPARE(request.value(QStringLiteral("op")).toString(), QStringLiteral("resolve"));
        QCOMPARE(request.value(QStringLiteral("query")).toString(), QStringLiteral("Santiago de Chile"));
        QVERIFY(!request.contains(QStringLiteral("domain")));
        QVERIFY(!request.contains(QStringLiteral("privacy")));
        QCOMPARE(result.value(QStringLiteral("api_key")).toString(), QStringLiteral("[redacted]"));
        QVERIFY(!QJsonDocument::fromVariant(console.entries()).toJson().contains("private-fixture-token"));
    }
    void rejectsShellScopeAndMalformedCommands() {
        const QStringList invalid = {
            QStringLiteral("world status; os ping"), QStringLiteral("world status | cat"),
            QStringLiteral("world resolve $(whoami)"), QStringLiteral("world resolve `whoami`"),
            QStringLiteral("world resolve place --domain personal"), QStringLiteral("world status --root /tmp"),
            QStringLiteral("world resolve \"unterminated"), QStringLiteral("world plan nan 0 500"),
            QStringLiteral("world plan 91 0 500"), QStringLiteral("world explore 0 0 -1"),
            QStringLiteral("world plan 0 0 500 signals"), QStringLiteral("world cancel ../escape"),
            QStringLiteral("os reboot"), QStringLiteral("sh anything"), QStringLiteral("os status\nos ping")};
        for (const auto &command : invalid) {
            WorldConsole console(config());
            console.submit(command);
            QVERIFY2(!console.busy(), qPrintable(command));
            QVERIFY2(!console.error().isEmpty(), qPrintable(command));
            QVERIFY(console.lastResult().isEmpty());
        }
    }
    void trustedPrefixesReachOnlyTheChildAsLiteralArguments() {
        auto c = config();
        c.programPrefixArguments = {QStringLiteral("trusted prefix with spaces & ; $"), QStringLiteral("--host-only")};
        WorldConsole console(c);
        console.submit(QStringLiteral("os ping"));
        QTRY_VERIFY_WITH_TIMEOUT(!console.busy(), 4000);
        QVERIFY2(console.error().isEmpty(), qPrintable(console.error()));
        const auto args = console.lastResult().value(QStringLiteral("result")).toMap().value(QStringLiteral("argv")).toStringList();
        QCOMPARE(args.mid(0, 2), c.programPrefixArguments);
        QCOMPARE(args.value(2), QStringLiteral("--json"));
    }
    void regionCarriesOnlyBoundedTypedInputs() {
        WorldConsole console(config());
        console.submit(QStringLiteral("world plan -33.4 -70.6 750 terrain,buildings"));
        QTRY_VERIFY_WITH_TIMEOUT(!console.busy(), 4000);
        QVERIFY2(console.error().isEmpty(), qPrintable(console.error()));
        const auto result = console.lastResult().value(QStringLiteral("result")).toMap();
        const auto args = result.value(QStringLiteral("argv")).toStringList();
        const auto request = QJsonDocument::fromJson(args.value(6).toUtf8()).object().value(QStringLiteral("request")).toObject();
        QCOMPARE(request.size(), 4);
        QCOMPARE(request.value(QStringLiteral("latitude")).toDouble(), -33.4);
        QCOMPARE(request.value(QStringLiteral("radius_m")).toDouble(), 750.0);
        QCOMPARE(request.value(QStringLiteral("layers")).toArray().size(), 2);
    }
    void invalidCoreOutputAndRefusalAreNotSuccess() {
        for (const auto &fixture : {QStringLiteral("invalid"), QStringLiteral("refused")}) {
            auto c = config();
            c.contextJson = QStringLiteral("\"personal/") + fixture + QStringLiteral("\"");
            WorldConsole console(c);
            QSignalSpy results(&console, &WorldConsole::resultReady);
            console.submit(QStringLiteral("world status"));
            QTRY_VERIFY_WITH_TIMEOUT(!console.busy(), 4000);
            QVERIFY(!console.error().isEmpty());
            QCOMPARE(results.size(), 0);
        }
    }
    void outputOverflowFailedStartAndDeadlineAreBounded() {
        auto c = config();
        c.contextJson = QStringLiteral("\"personal/overflow\"");
        c.maxOutputBytes = 4096;
        WorldConsole overflow(c);
        overflow.submit(QStringLiteral("world status"));
        QTRY_VERIFY_WITH_TIMEOUT(!overflow.busy(), 4000);
        QVERIFY(overflow.error().contains(QStringLiteral("bound")));
        c.program = QStringLiteral("/missing/archie-world-console-fixture");
        WorldConsole missing(c);
        missing.submit(QStringLiteral("os ping"));
        QTRY_VERIFY_WITH_TIMEOUT(!missing.busy(), 4000);
        QVERIFY(!missing.error().isEmpty());
        c = config();
        c.contextJson = QStringLiteral("\"personal/wait\"");
        c.processTimeoutMs = 100;
        WorldConsole deadline(c);
        deadline.submit(QStringLiteral("world status"));
        QTRY_VERIFY_WITH_TIMEOUT(!deadline.busy(), 4000);
        QVERIFY(deadline.error().contains(QStringLiteral("deadline")));
    }
    void cancellationDoesNotClaimToCancelAcceptedCoreWork() {
        auto c = config();
        c.contextJson = QStringLiteral("\"personal/wait\"");
        WorldConsole console(c);
        console.submit(QStringLiteral("world status"));
        QVERIFY(console.busy());
        console.cancel();
        QTRY_VERIFY_WITH_TIMEOUT(!console.busy(), 4000);
        QVERIFY(console.error().contains(QStringLiteral("may still be running")));
    }
    void historyAndUnconfiguredIrisAreBounded() {
        WorldConsole console(config());
        for (int i = 0; i < 200; ++i) console.submit(QStringLiteral("help"));
        QVERIFY(console.entries().size() <= 120);
        console.submit(QStringLiteral("iris open the map"));
        QVERIFY(!console.busy());
        QVERIFY(console.error().contains(QStringLiteral("Stage")));
        console.submit(QStringLiteral("iris status"));
        QVERIFY(!console.busy());
        QVERIFY(console.error().contains(QStringLiteral("configured")));
    }
    void irisUsesFixedTrustedStageArgumentsAndRedactsSensitiveFields() {
        auto c = config();
        c.voiceProgram = c.program;
        c.stageSocket = QStringLiteral("/trusted/stage.sock");
        c.stageKeyFile = QStringLiteral("/private/key");
        c.stagePolicy = QStringLiteral("/trusted/apps.toml");
        c.groundModel = QStringLiteral("local-grounder");
        WorldConsole console(c);
        console.submit(QStringLiteral("iris open the map"));
        QTRY_VERIFY_WITH_TIMEOUT(!console.busy(), 4000);
        QVERIFY2(console.error().isEmpty(), qPrintable(console.error()));
        const auto text = console.lastResult().value(QStringLiteral("output")).toString();
        const auto args = QJsonDocument::fromJson(text.toUtf8()).array();
        QCOMPARE(args[0].toString(), QStringLiteral("do"));
        QCOMPARE(args[1].toString(), QStringLiteral("open the map"));
        QCOMPARE(args[2].toString(), QStringLiteral("--grade"));
        QCOMPARE(args[3].toString(), QStringLiteral("public"));
        QCOMPARE(args[10].toString(), QStringLiteral("--steps"));
        QCOMPARE(args[11].toString(), QStringLiteral("6"));
        QCOMPARE(args[12].toString(), QStringLiteral("--ground-model"));
        QCOMPARE(args[13].toString(), QStringLiteral("local-grounder"));
        QVERIFY(!text.contains(c.stageKeyFile));
    }
    void healthIsReadOnlyLoopbackAndReportsActualJson() {
        QTcpServer server;
        QVERIFY(server.listen(QHostAddress::LocalHost));
        connect(&server, &QTcpServer::newConnection, &server, [&] {
            auto *socket = server.nextPendingConnection();
            connect(socket, &QTcpSocket::readyRead, socket, [socket] {
                const auto request = socket->readAll();
                QVERIFY(request.startsWith("GET /health "));
                socket->write("HTTP/1.1 200 OK\r\nContent-Length: 19\r\nConnection: close\r\n\r\n{\"model\":\"fixture\"}");
                socket->disconnectFromHost();
            });
            connect(socket, &QTcpSocket::disconnected, socket, &QObject::deleteLater);
        });
        auto c = config();
        c.irisHealthUrl = QUrl(QStringLiteral("http://127.0.0.1:%1/health").arg(server.serverPort()));
        WorldConsole console(c);
        console.submit(QStringLiteral("iris status"));
        QTRY_VERIFY_WITH_TIMEOUT(!console.busy(), 4000);
        QVERIFY2(console.error().isEmpty(), qPrintable(console.error()));
        QCOMPARE(console.lastResult().value(QStringLiteral("health")).toMap().value(QStringLiteral("model")).toString(), QStringLiteral("fixture"));
        c.irisHealthUrl = QUrl(QStringLiteral("http://example.com/health"));
        WorldConsole remote(c);
        remote.submit(QStringLiteral("iris status"));
        QVERIFY(!remote.busy());
        QVERIFY(!remote.error().isEmpty());
    }
};

int main(int argc, char **argv) {
    QCoreApplication app(argc, argv);
    const auto args = app.arguments();
    const int jsonAt = args.indexOf(QStringLiteral("--json"));
    if (jsonAt > 0 || args.value(1) == QStringLiteral("do")) {
        QFile out;
        if (!out.open(stdout, QIODevice::WriteOnly)) return 2;
        if (args.value(1) == QStringLiteral("do")) {
            out.write(QJsonDocument(QJsonArray::fromStringList(args.mid(1))).toJson(QJsonDocument::Compact));
            return 0;
        }
        const auto scope = args.value(jsonAt + 4);
        if (scope.contains(QStringLiteral("overflow"))) out.write(QByteArray(300000, 'x'));
        else if (scope.contains(QStringLiteral("wait"))) QTest::qWait(10000);
        else if (scope.contains(QStringLiteral("invalid"))) out.write("not JSON");
        else if (scope.contains(QStringLiteral("refused"))) {
            out.write("{\"ok\":false,\"error\":\"Core refused fixture\"}");
            return 1;
        } else out.write(QJsonDocument(QJsonObject{{QStringLiteral("ok"), true}, {QStringLiteral("result"),
            QJsonObject{{QStringLiteral("argv"), QJsonArray::fromStringList(args.mid(1))},
                {QStringLiteral("api_key"), QStringLiteral("private-fixture-token")}}}}).toJson(QJsonDocument::Compact));
        return 0;
    }
    WorldConsoleContracts tests;
    return QTest::qExec(&tests, argc, argv);
}
#include "test_world_console.moc"
