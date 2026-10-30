#include "AppCatalog.h"

#include <QDir>
#include <QFile>
#include <QGuiApplication>
#include <QImage>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QUrl>
#include <QtTest>

#include <utility>

namespace {
QVariantMap row(const QVariantList &rows, const QString &id)
{
    for (const QVariant &value : rows) {
        const QVariantMap app = value.toMap();
        if (app.value(QStringLiteral("id")).toString() == id)
            return app;
    }
    return {};
}

QStringList ids(const QVariantList &rows)
{
    QStringList result;
    for (const QVariant &value : rows)
        result << value.toMap().value(QStringLiteral("id")).toString();
    return result;
}
}

class AppCatalogContracts final : public QObject {
    Q_OBJECT
public:
    explicit AppCatalogContracts(QString root) : m_root(std::move(root)) {}

private slots:
    void initTestCase()
    {
        QVERIFY(QFile::exists(QStringLiteral(":/archie/apps/archie-stage.desktop")));
        const QString apps = m_root + QStringLiteral("/data/applications");
        QVERIFY(QDir().mkpath(apps));
        const auto desktop = [&apps](const QString &id, const QByteArray &extra) {
            QFile file(apps + QLatin1Char('/') + id + QStringLiteral(".desktop"));
            if (!file.open(QIODevice::WriteOnly))
                return false;
            const QByteArray content = "[Desktop Entry]\nType=Application\nName=" + id.toUtf8()
                + "\nExec=\"" + QGuiApplication::applicationFilePath().toUtf8()
                + "\"\n" + extra;
            return file.write(content) == content.size();
        };
        QVERIFY(desktop(QStringLiteral("fixture-ide"), "Categories=Development;IDE;\n"));
        QVERIFY(desktop(QStringLiteral("fixture-browser"), "Categories=Network;WebBrowser;\n"));
        // End-user editors/CAD remain visible even when their metadata mentions development.
        QVERIFY(desktop(QStringLiteral("fixture-cad"), "Categories=Development;Engineering;\n"));
        QVERIFY(desktop(QStringLiteral("fixture-preview-editor"), "Categories=Development;\nComment=Preview, debug and training notes\nX-Archie-Internal=false\n"));
        QVERIFY(desktop(QStringLiteral("fixture-training"), "Categories=X-Archie;Development;\nX-Archie-Internal=true\n"));
        QVERIFY(desktop(QStringLiteral("fixture-debug"), "Name[es]=Herramienta de pruebas\nX-Archie-Internal=true\n"));
        QVERIFY(desktop(QStringLiteral("fixture-preview"), "X-Archie-Internal=true\n"));
        QVERIFY(desktop(QStringLiteral("fixture-hidden"), "Hidden=true\n"));
        QVERIFY(desktop(QStringLiteral("fixture-no-display"), "NoDisplay=true\n"));
    }

    void forgeExcludesInternalToolsWithoutRemovingStageFromGlobalCatalog()
    {
        AppCatalog catalog(QStringList{m_root + QStringLiteral("/data/applications")});
        const QVariant projection = catalog.property("forgeApps");
        QVERIFY2(projection.isValid(), "Forge needs its own application projection; the global catalog serves the Stage dock");
        const QVariantList forge = projection.toList();
        for (const char *name : {"archie-stage", "archie-voice", "archie-lienzo", "fixture-training", "fixture-debug", "fixture-preview"}) {
            const QString id = QString::fromLatin1(name);
            QVERIFY2(!row(catalog.apps(), id).isEmpty(), qPrintable(id + QStringLiteral(" must remain in the global catalog")));
            QVERIFY2(row(forge, id).isEmpty(), qPrintable(id + QStringLiteral(" must stay out of Forge")));
        }
        QSignalSpy launched(&catalog, &AppCatalog::launched);
        QVERIFY(!catalog.canLaunch());
        QVERIFY(!catalog.launch(QStringLiteral("archie-stage")));
        QCOMPARE(launched.count(), 1);
        QCOMPARE(launched.first().at(2).toString(), catalog.launchNote());
    }

    void normalApplicationsAndSuggestionsRemainInForge()
    {
        AppCatalog catalog(QStringList{m_root + QStringLiteral("/data/applications")});
        const QVariantList forge = catalog.property("forgeApps").toList();
        for (const char *name : {"archie-cli", "archie-editor", "fixture-ide", "fixture-browser", "fixture-cad", "fixture-preview-editor"}) {
            const QString id = QString::fromLatin1(name);
            QVERIFY2(!row(forge, id).isEmpty(), qPrintable(id + QStringLiteral(" is an end-user application")));
            QCOMPARE(row(forge, id), row(catalog.apps(), id));
        }
        QCOMPARE(row(forge, QStringLiteral("org.kicad.kicad")).value(QStringLiteral("section")).toString(), QStringLiteral("suggested"));
        for (const char *name : {"fixture-hidden", "fixture-no-display"}) {
            const QString id = QString::fromLatin1(name);
            QVERIFY(row(catalog.apps(), id).isEmpty());
            QVERIFY(row(forge, id).isEmpty());
        }
    }

    void stagePolicyChangesOperabilityWithoutChangingForgeVisibility()
    {
        AppCatalog catalog(QStringList{m_root + QStringLiteral("/data/applications")});
        const QStringList before = ids(catalog.property("forgeApps").toList());
        QVERIFY(!before.isEmpty());
        QVERIFY(!row(catalog.apps(), QStringLiteral("fixture-ide")).value(QStringLiteral("operable")).toBool());
        QFile policy(m_root + QStringLiteral("/apps.toml"));
        QVERIFY(policy.open(QIODevice::WriteOnly));
        policy.write("[[app]]\napp_id = \"fixture-ide\"\n\n[[app]]\napp_id = \"fixture-training\"\n");
        policy.close();
        catalog.refresh();
        QCOMPARE(ids(catalog.property("forgeApps").toList()), before);
        QVERIFY(row(catalog.apps(), QStringLiteral("fixture-ide")).value(QStringLiteral("operable")).toBool());
        QVERIFY(row(catalog.apps(), QStringLiteral("fixture-training")).value(QStringLiteral("operable")).toBool());
        QVERIFY(row(catalog.property("forgeApps").toList(), QStringLiteral("fixture-training")).isEmpty());
    }

    void allNineSuggestionsHaveReadableOfficialFallbackIcons()
    {
        AppCatalog catalog(QStringList{});
        for (const char *name : {"org.kicad.kicad", "firefox", "org.kde.kdenlive", "org.freecad.FreeCAD",
                                "blender", "org.inkscape.Inkscape", "gimp", "com.obsproject.Studio", "libreoffice-startcenter"}) {
            const auto app = row(catalog.forgeApps(), QString::fromLatin1(name));
            QCOMPARE(app.value(QStringLiteral("section")).toString(), QStringLiteral("suggested"));
            const QString icon = app.value(QStringLiteral("icon")).toString();
            QVERIFY2(icon.startsWith(QLatin1String("qrc:/archie/assets/app-icons/")), qPrintable(icon));
            QVERIFY(QFile::exists(QLatin1Char(':') + QUrl(icon).path()));
            QVERIFY2(!QImage(QLatin1Char(':') + QUrl(icon).path()).isNull(), qPrintable(icon));
            QCOMPARE(app.value(QStringLiteral("iconSource")).toString(), QStringLiteral("official_bundle"));
            QVERIFY(!app.value(QStringLiteral("available")).toBool());
        }
    }

    void installedDesktopIconWinsAndMissingKnownIconFallsBackWithoutChangingAvailability()
    {
        QTemporaryDir directory;
        QVERIFY(directory.isValid());
        const QString localIcon = directory.path() + QStringLiteral("/installed.png");
        QImage image(32, 32, QImage::Format_ARGB32);
        image.fill(Qt::green);
        QVERIFY(image.save(localIcon));
        const auto desktop = [&directory](const QString &icon) {
            QFile file(directory.path() + QStringLiteral("/firefox.desktop"));
            if (!file.open(QIODevice::WriteOnly | QIODevice::Truncate)) return false;
            return file.write((QStringLiteral("[Desktop Entry]\nType=Application\nName=Firefox\nExec=not-an-installed-executable\nIcon=") + icon + QLatin1Char('\n')).toUtf8()) > 0;
        };
        QVERIFY(desktop(localIcon));
        AppCatalog catalog(QStringList{directory.path()});
        auto app = row(catalog.forgeApps(), QStringLiteral("firefox"));
        QCOMPARE(app.value(QStringLiteral("icon")).toString(), QUrl::fromLocalFile(localIcon).toString());
        QCOMPARE(app.value(QStringLiteral("iconSource")).toString(), QStringLiteral("installed"));
        QVERIFY(!app.value(QStringLiteral("available")).toBool());
        QVERIFY(desktop(QStringLiteral("missing-fixture-icon")));
        catalog.refresh();
        app = row(catalog.forgeApps(), QStringLiteral("firefox"));
        QCOMPARE(app.value(QStringLiteral("icon")).toString(), QStringLiteral("qrc:/archie/assets/app-icons/firefox.svg"));
        QCOMPARE(app.value(QStringLiteral("iconSource")).toString(), QStringLiteral("official_bundle"));
        QVERIFY(!app.value(QStringLiteral("available")).toBool());
    }

private:
    QString m_root;
};

int main(int argc, char **argv)
{
    QTemporaryDir root;
    if (!root.isValid())
        return 2;
    // Keep all incidental Qt data lookups local; catalog fixtures use explicit roots.
    qputenv("XDG_DATA_HOME", (root.path() + QStringLiteral("/data")).toUtf8());
    qputenv("XDG_DATA_DIRS", (root.path() + QStringLiteral("/system-data")).toUtf8());
    qputenv("ARCHIE_STAGE_POLICY", (root.path() + QStringLiteral("/apps.toml")).toUtf8());
    qputenv("ARCHIE_SHARE", (root.path() + QStringLiteral("/share")).toUtf8());
    qputenv("QT_QPA_PLATFORM", "offscreen");
    QGuiApplication app(argc, argv);
    AppCatalogContracts tests(root.path());
    return QTest::qExec(&tests, argc, argv);
}

#include "test_app_catalog.moc"
