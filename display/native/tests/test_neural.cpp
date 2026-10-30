#include "NeuralField.h"

#include <QDir>
#include <QFile>
#include <QTemporaryDir>
#include <QtTest>

#include <cmath>
#include <cstring>

namespace {
const QStringList folders{QStringLiteral("src"), QStringLiteral("src/core"), QStringLiteral("docs"), QStringLiteral("assets/img")};

// A small project: folders with files, enough for several layers of the sphere.
bool populate(const QString &root)
{
    for (const QString &folder : folders)
        if (!QDir().mkpath(root + QLatin1Char('/') + folder))
            return false;
    for (int i = 0; i < 24; ++i) {
        for (const QString &folder : folders) {
            QFile file(root + QLatin1Char('/') + folder + QStringLiteral("/file%1.%2").arg(i).arg(i % 3 ? QStringLiteral("rs") : QStringLiteral("md")));
            if (!file.open(QIODevice::WriteOnly))
                return false;
            file.write(QByteArray(i * 100, 'x'));
        }
    }
    return true;
}

bool settle(NeuralField &field)
{
    for (int i = 0; i < 800 && field.building(); ++i)
        field.advance(0.05);
    return !field.building();
}
} // namespace

class TestNeural : public QObject
{
    Q_OBJECT
private slots:
    void listsDrawsAndSettles();
    void stillFrameIsDeterministic();
    void settingsApplyBeforeAndAfterTheListing();
    void unreadableFolderIsReported();
    void rotationsFollowThreeJsEulerOrder();
    void pickingUsesTheHostsTwoCallContract();
    void failedRootClearsThePreviousDomain();
    void refreshWaitsForTheNewSnapshot();
    void modelAndDisplayUseTheSameStep();
    void inactiveViewsDeferFilesystemScanning();
    void lowerNodeLimitAndParticleLimitApplyImmediately();
    void positionLookupDistinguishesMissingNodes();
};

void TestNeural::listsDrawsAndSettles()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    QVERIFY(populate(dir.path()));
    NeuralField field;
    field.setRoot(dir.path());
    QVERIFY(field.loadNow());
    QVERIFY(field.ready());
    QVERIFY(field.error().isEmpty());
    // The live build starts with part of the sphere and grows to all of it.
    const std::size_t first = field.points().size();
    QVERIFY(first > 0);
    QVERIFY(settle(field));
    const std::size_t all = field.points().size();
    QVERIFY(all >= first);
    QVERIFY2(all > 90, "four folders of 24 files, plus the folders and the root, should be drawn");
    QVERIFY(!field.lines(0).empty());
    QCOMPARE(field.lines(0).size() % 2, std::size_t(0));
    QVERIFY(field.lines(99).empty());
    QVERIFY(field.stats().value(QStringLiteral("revision")).toULongLong() > 0);
    for (const NeuralPoint &p : field.points()) {
        QVERIFY(std::isfinite(p.position[0]) && std::isfinite(p.position[1]) && std::isfinite(p.position[2]));
        QVERIFY(p.born >= 0.0F && p.born <= 1.0F);
    }
    QVERIFY(!field.labels().isEmpty());
    // Selecting is the sphere's own: unknown ids are refused, known ones read back.
    QVERIFY(!field.select(QStringLiteral("no/such/node")));
    QVERIFY(field.select(QStringLiteral("src")));
    const QStringList card = field.selection(QStringLiteral("proyecto"));
    QVERIFY(card.size() >= 2);
    QCOMPARE(card.first(), QStringLiteral("src"));
    QCOMPARE(field.linkCounts().size(), 7);
}

void TestNeural::stillFrameIsDeterministic()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    QVERIFY(populate(dir.path()));
    NeuralField a;
    NeuralField b;
    for (NeuralField *field : {&a, &b}) {
        field->setStill(true);
        field->setRoot(dir.path());
        QVERIFY(field->loadNow());
    }
    const auto pa = a.points();
    const auto pb = b.points();
    QVERIFY(!pa.empty());
    QCOMPARE(pa.size(), pb.size());
    QCOMPARE(std::memcmp(pa.data(), pb.data(), pa.size() * sizeof(NeuralPoint)), 0);
    const auto sa = a.sparks();
    const auto sb = b.sparks();
    QCOMPARE(sa.size(), sb.size());
    QCOMPARE(std::memcmp(sa.data(), sb.data(), sa.size() * sizeof(NeuralSpark)), 0);
    // A still sphere does not move.
    a.advance(1.0);
    const auto after = a.points();
    QCOMPARE(std::memcmp(pa.data(), after.data(), pa.size() * sizeof(NeuralPoint)), 0);
}

void TestNeural::settingsApplyBeforeAndAfterTheListing()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    QVERIFY(populate(dir.path()));
    NeuralField field;
    QVERIFY(field.setNumber(QStringLiteral("brightness"), 1.5));
    QVERIFY(field.setFlag(QStringLiteral("base"), false));
    QVERIFY(field.setLink(2, 0, std::nan("")));
    QVERIFY(field.setBackground(QStringLiteral("#112233")));
    QVERIFY(!field.setBackground(QStringLiteral("red")));
    field.setRoot(dir.path());
    QVERIFY(field.loadNow());
    // What was set before the listing is what the new sphere starts with.
    QCOMPARE(field.settings().value(QStringLiteral("brightness")).toDouble(), 1.5);
    QCOMPARE(field.settings().value(QStringLiteral("base")).toBool(), false);
    QCOMPARE(field.settings().value(QStringLiteral("background")).toString(), QStringLiteral("#112233"));
    QVERIFY(field.backgroundLinear().x() < field.backgroundLinear().z());
    // Invalid values change nothing.
    QVERIFY(!field.setNumber(QStringLiteral("no-such-setting"), 1.0));
    QVERIFY(!field.setNumber(QStringLiteral("brightness"), std::nan("")));
    QVERIFY(!field.setLink(9, 1, 1.0));
    QCOMPARE(field.settings().value(QStringLiteral("brightness")).toDouble(), 1.5);
    // Limits are the sphere's: 3 is the ceiling.
    QVERIFY(field.setNumber(QStringLiteral("brightness"), 50.0));
    QCOMPARE(field.settings().value(QStringLiteral("brightness")).toDouble(), 3.0);
}

void TestNeural::unreadableFolderIsReported()
{
    NeuralField field;
    field.setRoot(QDir::tempPath() + QStringLiteral("/archie-neural-no-such-folder-0449"));
    QVERIFY(!field.loadNow());
    QVERIFY(!field.ready());
    QVERIFY(!field.error().isEmpty());
    QVERIFY(field.points().empty());
    // A sphere that does not exist answers every query with nothing, not with a crash.
    field.advance(0.1);
    QVERIFY(field.labels().isEmpty());
    QVERIFY(field.selection(QString()).isEmpty());
    QVERIFY(field.pick(QVector3D(0, 0, 300), QVector3D(0, 0, -1)).isEmpty());
}

void TestNeural::rotationsFollowThreeJsEulerOrder()
{
    NeuralField field;
    // scene.ts: orbit.rotation.set(0.3, 0, -0.1), then .y = t * 0.05, Euler XYZ (R = Rx Ry Rz).
    const QVector3D v(1, 0, 0);
    const QVector3D at0 = field.orbitRotation(0.0).rotatedVector(v);
    // Rz(-0.1) first: (cos .1, -sin .1, 0); Rx(0.3) then leaves x alone and tilts y into z.
    QVERIFY(qAbs(at0.x() - std::cos(0.1)) < 1e-5);
    QVERIFY(qAbs(at0.y() - (-std::sin(0.1) * std::cos(0.3))) < 1e-5);
    QVERIFY(qAbs(at0.z() - (-std::sin(0.1) * std::sin(0.3))) < 1e-5);
    // The base turns about Y at 0.04 rad/s.
    const QVector3D turned = field.baseRotation(10.0).rotatedVector(v);
    QVERIFY(qAbs(turned.x() - std::cos(0.4)) < 1e-5);
    QVERIFY(qAbs(turned.z() - (-std::sin(0.4))) < 1e-5);
}

void TestNeural::pickingUsesTheHostsTwoCallContract()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    NeuralField field;
    field.setStill(true);
    field.setLimit(1);
    field.setRoot(dir.path());
    QVERIFY(field.loadNow());
    QCOMPARE(field.pick(QVector3D(0, 0, 300), QVector3D(0, 0, -1)), QStringLiteral("."));
}

void TestNeural::failedRootClearsThePreviousDomain()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    NeuralField field;
    field.setRoot(dir.path());
    QVERIFY(field.loadNow());
    QCOMPARE(field.sourceRoot(), QDir::cleanPath(dir.path()));
    field.setRoot(dir.path() + QStringLiteral("/missing-work-domain"));
    QVERIFY(!field.loadNow());
    QVERIFY(!field.ready());
    QVERIFY(field.sourceRoot().isEmpty());
    QVERIFY(field.points().empty());
    QVERIFY(!field.error().isEmpty());
}

void TestNeural::refreshWaitsForTheNewSnapshot()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    NeuralField field;
    field.setRoot(dir.path());
    QVERIFY(field.loadNow());
    const int before = field.stats().value(QStringLiteral("total")).toInt();
    QFile added(dir.path() + QStringLiteral("/new.rs"));
    QVERIFY(added.open(QIODevice::WriteOnly));
    added.write("// a real new filesystem entry\n");
    added.close();
    field.refresh();
    QVERIFY(field.loadNow());
    QVERIFY(!field.loading());
    QVERIFY(!field.scannedAt().isEmpty());
    QCOMPARE(field.stats().value(QStringLiteral("total")).toInt(), before + 1);
}

void TestNeural::modelAndDisplayUseTheSameStep()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    NeuralField field;
    field.setRoot(dir.path());
    QVERIFY(field.loadNow());
    const double accepted = field.advance(0.1);
    QVERIFY(qAbs(accepted - 0.05) < 1e-6);
    QVERIFY(qAbs(field.time() - field.stats().value(QStringLiteral("time")).toDouble()) < 1e-6);
    const double before = field.time();
    QCOMPARE(field.advance(std::nan("")), 0.0);
    QCOMPARE(field.time(), before);
}

void TestNeural::inactiveViewsDeferFilesystemScanning()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    NeuralField field;
    field.setActive(false);
    field.setRoot(dir.path());
    QVERIFY(!field.loading());
    QVERIFY(!field.ready());
    field.setActive(true);
    QVERIFY(field.loadNow());
    QCOMPARE(field.sourceRoot(), QDir::cleanPath(dir.path()));
}

void TestNeural::lowerNodeLimitAndParticleLimitApplyImmediately()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    QVERIFY(populate(dir.path()));
    NeuralField field;
    field.setRoot(dir.path());
    QVERIFY(field.loadNow());
    QVERIFY(settle(field));
    field.setLimit(8);
    QVERIFY(!field.ready());
    QVERIFY(field.loadNow());
    QVERIFY(field.points().size() <= std::size_t(8));
    QVERIFY(field.setNumber(QStringLiteral("pulses"), 3));
    QCOMPARE(field.sparks().size(), std::size_t(15));
    QVERIFY(field.setNumber(QStringLiteral("pulses"), 0));
    QVERIFY(field.sparks().empty());
}

void TestNeural::positionLookupDistinguishesMissingNodes()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    QVERIFY(populate(dir.path()));
    NeuralField field;
    QCOMPARE(field.lookupPosition(QStringLiteral("src")).value(QStringLiteral("found")).toBool(), false);
    field.setRoot(dir.path());
    QVERIFY(field.loadNow());
    QVERIFY(settle(field));
    const QVariantMap present = field.lookupPosition(QStringLiteral("src"));
    QVERIFY(present.value(QStringLiteral("found")).toBool());
    QCOMPARE(present.value(QStringLiteral("position")).value<QVector3D>(), field.positionOf(QStringLiteral("src")));
    const QVariantMap absent = field.lookupPosition(QStringLiteral("not/in/snapshot"));
    QVERIFY(!absent.value(QStringLiteral("found")).toBool());
    QVERIFY(!absent.contains(QStringLiteral("position")));
}

QTEST_GUILESS_MAIN(TestNeural)
#include "test_neural.moc"
