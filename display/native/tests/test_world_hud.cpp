#include <QGuiApplication>
#include <QQuickItem>
#include <QQuickWindow>
#include <QSignalSpy>
#include <QTest>
#include <QVector3D>
#include <QQmlEngine>
#include <QQmlComponent>
#include <QVariantMap>
#include <QDebug>
#include <cstdlib>
class ProjectionReader : public QObject {
    Q_OBJECT
    Q_PROPERTY(QVariantMap exploration MEMBER exploration NOTIFY explorationChanged)
    Q_PROPERTY(QVariantMap snapshot MEMBER snapshot NOTIFY snapshotChanged)
    Q_PROPERTY(QVariantMap kernel MEMBER kernel NOTIFY kernelChanged)
    Q_PROPERTY(QVariantMap frame MEMBER frame NOTIFY explorationChanged)
    Q_PROPERTY(QVariantList imagerySources MEMBER imagerySources NOTIFY explorationChanged)
    Q_PROPERTY(QVariantList signalSources MEMBER signalSources NOTIFY explorationChanged)
    Q_PROPERTY(QVariantList worldPoints MEMBER worldPoints NOTIFY explorationChanged)
    Q_PROPERTY(QString selectedImagery MEMBER selectedImagery NOTIFY explorationChanged)
    Q_PROPERTY(QString selectedExplorationJob MEMBER selectedExplorationJob NOTIFY explorationChanged)
public:
    QVariantMap exploration,snapshot,kernel,frame;QVariantList imagerySources,signalSources,worldPoints;QString selectedImagery,selectedExplorationJob;int plans=0;bool animated=true;double lastElevation=0;
    Q_INVOKABLE bool frameExploration(double,double,double,double elevation,bool motion){++plans;animated=motion;lastElevation=elevation;return true;}
signals:
    void explorationChanged();void snapshotChanged();void kernelChanged();
};
class ProjectionView : public QObject {
    Q_OBJECT
public:
    Q_INVOKABLE QVector3D mapFrom3DScene(const QVector3D &) {return {100,100,1};}
};
void check(bool condition,const char *message){if(!condition){qCritical("%s",message);std::exit(1);}}
QQuickItem *findNotice(QQuickItem *item,const QString &name) {
    if(!item) return nullptr;
    if(item->objectName()==name) return item;
    for(auto *child:item->childItems()) if(auto *found=findNotice(child,name)) return found;
    return nullptr;
}
QVariantMap job(QString id,QVariant elevation=QVariant()) {return {{QStringLiteral("id"),id},{QStringLiteral("domain"),QStringLiteral("personal")},{QStringLiteral("state"),QStringLiteral("running")},{QStringLiteral("camera"),QVariantMap{{QStringLiteral("latitude"),0},{QStringLiteral("longitude"),0},{QStringLiteral("radius_m"),1000},{QStringLiteral("elevation_m"),elevation}}}};}
int main(int argc,char **argv) {
    QQuickWindow::setGraphicsApi(QSGRendererInterface::Software);
    QGuiApplication app(argc,argv);QQmlEngine engine;ProjectionReader reader;
    reader.exploration={{QStringLiteral("source"),QStringLiteral("live")},{QStringLiteral("jobs"),QVariantList{job(QStringLiteral("first"))}}};
    // The minimal HUD: search, rail, layers, ruler, console, and the reader as its drawer.
    QQmlComponent minimal(&engine,QUrl(QStringLiteral("qrc:/world-test/WorldHud.qml")));
    auto *overlay=minimal.createWithInitialProperties({{QStringLiteral("controller"),QVariant::fromValue(&reader)},{QStringLiteral("worldActive"),true}});
    if(!overlay)qCritical()<<minimal.errors();
    check(overlay,"production minimal WorldHud loads with actual shared shell resources");
    for(const auto *name:{"world.search","world.rail","world.layers","world.ruler","world.console"})
        check(findNotice(qobject_cast<QQuickItem*>(overlay),QString::fromLatin1(name)),"minimal HUD carries its search, rail and on-demand panels");
    check(overlay->property("panel").toString().isEmpty()&&!overlay->property("measuring").toBool(),"World opens with every panel closed");
    QMetaObject::invokeMethod(overlay,"search",Q_ARG(QVariant,QVariant(QStringLiteral("-33.45, -70.66"))));
    check(reader.plans==1,"coordinates in the search box frame the camera there");
    QMetaObject::invokeMethod(overlay,"search",Q_ARG(QVariant,QVariant(QStringLiteral("Santiago"))));
    check(reader.plans==1&&!overlay->property("searchNote").toString().isEmpty(),"a place name without the console says what it needs instead of guessing");
    delete overlay;reader.plans=0;
    QQmlComponent component(&engine,QUrl(QStringLiteral("qrc:/world-test/WorldDetails.qml")));
    auto *hud=component.createWithInitialProperties({{QStringLiteral("controller"),QVariant::fromValue(&reader)},{QStringLiteral("worldActive"),true}});
    if(!hud)qCritical()<<component.errors();
    check(hud,"production WorldDetails loads with actual shared shell resources");
    check(reader.plans==0,"World startup retains global home even when saved jobs exist");
    emit reader.explorationChanged();check(reader.plans==0,"poll cannot reclaim the navigation camera");
    reader.exploration.insert(QStringLiteral("jobs"),QVariantList{job(QStringLiteral("first"),4200)});
    emit reader.explorationChanged();check(reader.plans==0,"background terrain readiness cannot auto-focus a saved AOI");
    QVariant accepted;
    QMetaObject::invokeMethod(hud,"applyCamera",Q_RETURN_ARG(QVariant,accepted),Q_ARG(QVariant,QVariant(job(QStringLiteral("first"),4200))));
    check(reader.plans==1&&reader.lastElevation==4200,"explicit capture navigation preserves measured elevation");
    QMetaObject::invokeMethod(hud,"cedeCamera");
    hud->setProperty("worldActive",false);hud->setProperty("worldActive",true);
    reader.exploration.insert(QStringLiteral("jobs"),QVariantList{job(QStringLiteral("first")),job(QStringLiteral("next"))});
    emit reader.explorationChanged();check(reader.plans==2,"a newly admitted voice/CLI exploration can navigate after startup history is seeded");
    reader.exploration.insert(QStringLiteral("source"),QStringLiteral("unavailable"));
    emit reader.explorationChanged();check(reader.plans==2,"unavailable snapshot cannot frame");
    const auto lightFill=hud->property("fill");
    QQmlComponent theme(&engine);
    theme.setData("import QtQuick\nimport \"qrc:/archie/shell/prototype\" as Shell\nQtObject { property string mode: \"dark\"; onModeChanged: Shell.Theme.colorMode=mode; Component.onCompleted: { Shell.Theme.colorMode=mode;Shell.Theme.reducedMotion=true } }",QUrl());
    auto *themeSwitch=theme.create();check(themeSwitch,"shared Theme can be switched at runtime");
    check(hud->property("fill")!=lightFill,"World HUD follows actual shared dark theme");
    themeSwitch->setProperty("mode",QStringLiteral("light"));
    check(hud->property("fill")==lightFill,"World HUD returns to actual shared light theme");
    reader.exploration.insert(QStringLiteral("source"),QStringLiteral("live"));
    reader.exploration.insert(QStringLiteral("jobs"),QVariantList{job(QStringLiteral("reduced-motion"))});
    emit reader.explorationChanged();
    check(reader.plans==3,"new admitted plan remains available in reduced motion");
    check(!reader.animated,"reduced motion disables spatial camera interpolation");
    auto bounded=job(QStringLiteral("bounded-view"));
    bounded.insert(QStringLiteral("projection_truncated"),true);
    bounded.insert(QStringLiteral("preview"),QVariantMap{{QStringLiteral("omitted_buildings"),7}});
    reader.exploration.insert(QStringLiteral("jobs"),QVariantList{bounded});
    emit reader.explorationChanged();QCoreApplication::processEvents();
    auto *truncated=findNotice(qobject_cast<QQuickItem*>(hud),QStringLiteral("projectionTruncationNotice"));
    auto *omitted=findNotice(qobject_cast<QQuickItem*>(hud),QStringLiteral("omittedBuildingsNotice"));
    check(truncated && truncated->property("visible").toBool(),"bounded projection explicitly identifies truncated plan/artifacts");
    check(omitted && omitted->property("visible").toBool() && omitted->property("text").toString().contains(QStringLiteral("7")),"omitted building count is visible");
    const int selectedPlans=reader.plans;reader.selectedExplorationJob=QStringLiteral("first");
    reader.exploration[QStringLiteral("selected_job_id")]=QStringLiteral("first");reader.exploration[QStringLiteral("jobs")]=QVariantList{job(QStringLiteral("first")),job(QStringLiteral("new-worker-job"))};emit reader.explorationChanged();
    check(reader.plans==selectedPlans&&hud->property("selectedJob").toMap().value(QStringLiteral("id"))==QStringLiteral("first"),"selected older area persists while new worker job appears without camera takeover");
    reader.exploration[QStringLiteral("selected_job_id")]=QStringLiteral("missing");emit reader.explorationChanged();
    check(hud->property("selectedJob").isNull(),"HUD identifies missing selected capture rather than showing latest job");
    ProjectionView view;
    reader.frame={{QStringLiteral("cameraEye"),QVariant::fromValue(QVector3D(0,0,60))}};
    reader.worldPoints={QVariantMap{{QStringLiteral("id"),QStringLiteral("reported-point")},{QStringLiteral("label"),QStringLiteral("Reported aircraft")},{QStringLiteral("kind"),QStringLiteral("aircraft")},{QStringLiteral("position"),QVariant::fromValue(QVector3D(0,0,20))}}};
    hud->setProperty("selectedPointId",QStringLiteral("reported-point"));emit reader.explorationChanged();
    check(hud->property("selectedPoint").toMap().value(QStringLiteral("label"))==QStringLiteral("Reported aircraft"),"inspector resolves selected ID against current projection");
    auto refreshedPoint=reader.worldPoints.first().toMap();refreshedPoint[QStringLiteral("label")]=QStringLiteral("Updated aircraft observation");refreshedPoint[QStringLiteral("observed_at_ms")]=5678;reader.worldPoints={refreshedPoint};emit reader.explorationChanged();
    check(hud->property("selectedPoint").toMap().value(QStringLiteral("observed_at_ms")).toLongLong()==5678,"inspector updates measured details and observation time from the new projection");
    reader.worldPoints.clear();emit reader.explorationChanged();QCoreApplication::processEvents();
    check(hud->property("selectedPoint").isNull(),"removed observation cannot leave frozen inspector measurements");
    auto *expired=findNotice(qobject_cast<QQuickItem*>(hud),QStringLiteral("selectedPointExpired"));check(expired&&expired->property("visible").toBool(),"removed selected observation is explicitly unavailable");
    reader.worldPoints={refreshedPoint};emit reader.explorationChanged();
    QQmlComponent beaconComponent(&engine,QUrl(QStringLiteral("qrc:/world-test/WorldBeacons.qml")));
    auto *beacons=beaconComponent.createWithInitialProperties({{QStringLiteral("controller"),QVariant::fromValue(&reader)},{QStringLiteral("view"),QVariant::fromValue(&view)},{QStringLiteral("width"),400},{QStringLiteral("height"),300}});
    if(!beacons)qCritical()<<beaconComponent.errors();
    check(beacons,"production beacon layer loads with bounded real-location marker");
    const auto *beaconRoot=qobject_cast<QQuickItem*>(beacons);check(beaconRoot && !beaconRoot->childItems().isEmpty(),"beacon layer creates visual children");
    QQuickWindow beaconWindow;beaconWindow.resize(400,300);qobject_cast<QQuickItem*>(beacons)->setParentItem(beaconWindow.contentItem());beaconWindow.show();
    check(QTest::qWaitForWindowExposed(&beaconWindow),"production beacon interaction surface is exposed");
    QSignalSpy chosen(beacons,SIGNAL(chosen(QVariant)));check(chosen.isValid(),"production point selection signal is observable");
    QTest::mouseClick(&beaconWindow,Qt::LeftButton,Qt::NoModifier,QPoint(100,100));QCoreApplication::processEvents();
    check(chosen.count()==1&&chosen.first().first().toMap().value(QStringLiteral("id"))==QStringLiteral("reported-point"),"actual pointer click picks the sourced beacon through its production MouseArea");
    auto areaPoint=refreshedPoint;areaPoint[QStringLiteral("id")]=QStringLiteral("aoi:older");areaPoint[QStringLiteral("kind")]=QStringLiteral("aoi");
    reader.worldPoints={areaPoint,refreshedPoint};emit reader.explorationChanged();QCoreApplication::processEvents();chosen.clear();
    QTest::mouseClick(&beaconWindow,Qt::LeftButton,Qt::NoModifier,QPoint(100,100));QCoreApplication::processEvents();
    check(chosen.count()==1&&chosen.first().first().toMap().value(QStringLiteral("id"))==QStringLiteral("aoi:older"),"AOI navigation beacon remains clickable when a later aircraft overlaps its marker");
    auto hidden=reader.worldPoints.first().toMap();hidden.insert(QStringLiteral("position"),QVariant::fromValue(QVector3D(0,0,-20)));reader.worldPoints={hidden};emit reader.explorationChanged();QCoreApplication::processEvents();
    QTest::mouseClick(&beaconWindow,Qt::LeftButton,Qt::NoModifier,QPoint(100,100));QCoreApplication::processEvents();
    check(chosen.count()==1,"globe-occluded beacon cannot receive a pointer selection");
    delete beacons;delete themeSwitch;delete hud;qInfo("World HUD/beacon QML contracts passed");
}
#include "test_world_hud.moc"
