#include "DisplayControl.h"
#include "LumenField.h"
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonArray>
#include <QSignalSpy>
#include <QTcpSocket>
#include <QtTest>

class DisplayControlContracts final : public QObject {
    Q_OBJECT
    QByteArray request(DisplayControl &control, const QByteArray &payload) {
        QTcpSocket socket;
        socket.connectToHost(QHostAddress::LocalHost, control.port());
        if (!QTest::qWaitFor([&socket] { return socket.state() == QAbstractSocket::ConnectedState; }, 1000))
            return {};
        socket.write(payload);
        if (!QTest::qWaitFor([&socket] { return socket.bytesAvailable() > 0; }, 1000))
            return {};
        return socket.readAll();
    }
private slots:
    void catalogFollowsResourcesAndActualHostScenes() {
        LumenField field;
        DisplayControl observatory(&field);
        QVERIFY(observatory.start(0));
        const auto catalog = QJsonDocument::fromJson(request(observatory, "{\"action\":\"list\"}\n")).object();
        QCOMPARE(catalog.value(QStringLiteral("presets")).toArray(), QJsonArray::fromStringList(field.presets()));
        QCOMPARE(catalog.value(QStringLiteral("scenes")).toArray(), QJsonArray::fromStringList(observatory.scenes()));
        QVERIFY(observatory.scenes().contains(QStringLiteral("world")));
        DisplayControl shell(&field, nullptr, true);
        QVERIFY(shell.start(0));
        const auto shellCatalog = QJsonDocument::fromJson(request(shell, "{\"action\":\"list\"}\n")).object();
        QVERIFY(!shellCatalog.value(QStringLiteral("scenes")).toArray().contains(QStringLiteral("world")));
        QCOMPARE(shellCatalog.value(QStringLiteral("presets")).toArray(), QJsonArray::fromStringList(field.presets()));
    }
    void presetSuccessMeansAppliedAndFailurePreservesState() {
        LumenField field;
        DisplayControl control(&field);
        QVERIFY(control.start(0));
        QSignalSpy delivered(&control, &DisplayControl::commandReceived);
        QString profileWhenNotified;
        connect(&control, &DisplayControl::commandReceived, this, [&] { profileWhenNotified = field.profile(); });
        QCOMPARE(request(control, "{\"action\":\"preset\",\"value\":\"neural\"}\n"), QByteArray("ok\n"));
        QCOMPARE(profileWhenNotified, QStringLiteral("neural"));
        const auto before = field.settings();
        QVERIFY(request(control, "{\"action\":\"preset\",\"value\":\"../field\"}\n").startsWith("error:"));
        QCOMPARE(field.settings(), before);
        QCOMPARE(field.profile(), QStringLiteral("neural"));
        QCOMPARE(delivered.count(), 1);
        QCOMPARE(request(control, "{\"action\":\"scene\",\"value\":\" CORE \"}\n"), QByteArray("ok\n"));
        QCOMPARE(delivered.last()[1].toString(), QStringLiteral("forge"));
    }
    void invalidRequestsNeverDeliverCommands() {
        LumenField field;
        DisplayControl control(&field);
        QVERIFY(control.start(0));
        QSignalSpy delivered(&control, &DisplayControl::commandReceived);
        for (const auto &payload : QList<QByteArray>{"[]\n", "broken\n", "{\"action\":\"execute\"}\n", QByteArray(4097, 'x')})
            QVERIFY(request(control, payload).startsWith("error:"));
        QCOMPARE(delivered.count(), 0);
    }
    void existingVoiceCommandsAreDelivered() {
        LumenField field;
        DisplayControl control(&field);
        QVERIFY(control.start(0));
        QSignalSpy delivered(&control, &DisplayControl::commandReceived);
        const QList<QByteArray> commands{
            "{\"action\":\"scene\",\"value\":\"world\"}\n",
            "{\"action\":\"cinematic\",\"value\":\"crown\"}\n",
            "{\"action\":\"overlay\",\"value\":\"pulses\",\"enabled\":false}\n"};
        for (const auto &command : commands) {
            QTcpSocket socket;
            socket.connectToHost(QHostAddress::LocalHost, control.port());
            QTRY_COMPARE(socket.state(), QAbstractSocket::ConnectedState);
            socket.write(command);
            QTRY_VERIFY(socket.bytesAvailable() > 0);
            QCOMPARE(socket.readAll(), QByteArray("ok\n"));
        }
        QCOMPARE(delivered.count(), 3);
        QCOMPARE(delivered[2][2].toBool(), false);
    }
    void shellDoesNotAcknowledgeAnUnavailableWorldView() {
        LumenField field;
        DisplayControl control(&field, nullptr, true);
        QVERIFY(control.start(0));
        QSignalSpy delivered(&control, &DisplayControl::commandReceived);
        QTcpSocket socket;
        socket.connectToHost(QHostAddress::LocalHost, control.port());
        QTRY_COMPARE(socket.state(), QAbstractSocket::ConnectedState);
        socket.write("{\"action\":\"scene\",\"value\":\"world\"}\n");
        QTRY_VERIFY(socket.bytesAvailable() > 0);
        QVERIFY(socket.readAll().startsWith("error:"));
        QCOMPARE(delivered.count(), 0);
    }
    void shellAcknowledgesWorldOnlyAfterNativeHostIsAvailable() {
        LumenField field;
        DisplayControl control(&field,nullptr,true);
        QVERIFY(!control.scenes().contains(QStringLiteral("world")));
        control.setWorldAvailable(true);
        QVERIFY(control.scenes().contains(QStringLiteral("world")));
        QVERIFY(control.start(0));
        QSignalSpy delivered(&control,&DisplayControl::commandReceived);
        QTcpSocket socket;socket.connectToHost(QHostAddress::LocalHost,control.port());
        QTRY_COMPARE(socket.state(),QAbstractSocket::ConnectedState);
        socket.write("{\"action\":\"scene\",\"value\":\"world\"}\n");
        QTRY_VERIFY(socket.bytesAvailable()>0);
        QCOMPARE(socket.readAll(),QByteArray("ok\n"));QCOMPARE(delivered.count(),1);
    }
    void overlaysRequireAnActualBoolean() {
        LumenField field;
        DisplayControl control(&field);
        QVERIFY(control.start(0));
        QSignalSpy delivered(&control, &DisplayControl::commandReceived);
        QTcpSocket socket;
        socket.connectToHost(QHostAddress::LocalHost, control.port());
        QTRY_COMPARE(socket.state(), QAbstractSocket::ConnectedState);
        socket.write("{\"action\":\"overlay\",\"value\":\"shell\",\"enabled\":\"true\"}\n");
        QTRY_VERIFY(socket.bytesAvailable() > 0);
        QVERIFY(socket.readAll().startsWith("error:"));
        QCOMPARE(delivered.count(), 0);
    }
    void shellPanelsAreAllowlistedLocalViews() {
        LumenField field;
        DisplayControl control(&field, nullptr, true);
        QVERIFY(control.start(0));
        QSignalSpy delivered(&control, &DisplayControl::commandReceived);
        QTcpSocket socket;
        socket.connectToHost(QHostAddress::LocalHost, control.port());
        QTRY_COMPARE(socket.state(), QAbstractSocket::ConnectedState);
        socket.write("{\"action\":\"panel\",\"value\":\"inspector\"}\n");
        QTRY_VERIFY(socket.bytesAvailable() > 0);
        QCOMPARE(socket.readAll(), QByteArray("ok\n"));
        QCOMPARE(delivered.count(), 1);
        QTcpSocket invalid;
        invalid.connectToHost(QHostAddress::LocalHost, control.port());
        QTRY_COMPARE(invalid.state(), QAbstractSocket::ConnectedState);
        invalid.write("{\"action\":\"panel\",\"value\":\"execute\"}\n");
        QTRY_VERIFY(invalid.bytesAvailable() > 0);
        QVERIFY(invalid.readAll().startsWith("error:"));
        QCOMPARE(delivered.count(), 1);
    }
};
QTEST_GUILESS_MAIN(DisplayControlContracts)
#include "test_display_control.moc"
