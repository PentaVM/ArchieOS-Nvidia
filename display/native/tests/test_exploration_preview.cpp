#include "ExplorationPreview.h"
#include <QCoreApplication>
#include <QDebug>
#include <cstdlib>
#include <cmath>
#include <cstring>
namespace {
void check(bool condition,const char *message) {if(!condition){qCritical("%s",message);std::exit(1);}}
QVariantMap projection(QVariantMap preview) {
    return {{QStringLiteral("jobs"),QVariantList{QVariantMap{{QStringLiteral("id"),QStringLiteral("test")},{QStringLiteral("camera"),QVariantMap{{QStringLiteral("latitude"),0},{QStringLiteral("longitude"),0},{QStringLiteral("radius_m"),1000}}},{QStringLiteral("preview"),preview}}}}};
}
int kindCount(const QVariantList &meshes,QString kind) {int result=0;for(const auto &m:meshes)if(m.toMap().value(QStringLiteral("kind"))==kind)++result;return result;}
}
int main(int argc,char **argv) {
    QCoreApplication app(argc,argv);
    const QVariantMap bounds{{QStringLiteral("south"),-0.01},{QStringLiteral("north"),0.01},{QStringLiteral("west"),-0.01},{QStringLiteral("east"),0.01}};
    QVariantMap terrain{{QStringLiteral("width"),2},{QStringLiteral("height"),2},{QStringLiteral("elevations_m"),QVariantList{0,10,20,30}}};
    const QVariantList ring{QVariantList{0,0},QVariantList{0.001,0},QVariantList{0.001,0.001},QVariantList{0,0.001}};
    QVariantMap building{{QStringLiteral("ring"),ring},{QStringLiteral("height_m"),QVariant()}};
    QVariantMap preview{{QStringLiteral("row_projection"),QStringLiteral("web_mercator")},{QStringLiteral("bounds"),bounds},{QStringLiteral("terrain"),terrain},{QStringLiteral("buildings"),QVariantList{building}}};
    auto meshes=explorationPreviewMeshes(projection(preview));
    check(kindCount(meshes,QStringLiteral("terrain"))==1,"real grid produces terrain");
    check(kindCount(meshes,QStringLiteral("outlines"))==1,"unknown height remains an outline");
    check(kindCount(meshes,QStringLiteral("buildings"))==0,"unknown height cannot become an extrusion");
    check(kindCount(meshes,QStringLiteral("aoi"))==1,"requested AOI produces geodesic circle");
    building.insert(QStringLiteral("height_m"),40);preview.insert(QStringLiteral("buildings"),QVariantList{building});
    check(kindCount(explorationPreviewMeshes(projection(preview)),QStringLiteral("buildings"))==1,"known height produces walls");
    terrain.insert(QStringLiteral("elevations_m"),QVariantList{QVariant(),QVariant(),QVariant(),QVariant()});preview.insert(QStringLiteral("terrain"),terrain);
    check(kindCount(explorationPreviewMeshes(projection(preview)),QStringLiteral("terrain"))==0,"missing elevation samples do not become zero-height terrain");
    terrain.insert(QStringLiteral("elevations_m"),QVariantList{-430,-420,-410,-400});preview.insert(QStringLiteral("terrain"),terrain);
    const auto belowSea=explorationPreviewMeshes(projection(preview));
    check(kindCount(belowSea,QStringLiteral("terrain"))==1,"negative grid remains drawable");
    for(const auto &entry:belowSea) {
        const auto mesh=entry.toMap();if(mesh.value(QStringLiteral("kind"))!=QStringLiteral("terrain")) continue;
        const auto data=mesh.value(QStringLiteral("vertices")).toByteArray();float vertex[6];std::memcpy(vertex,data.constData(),sizeof(vertex));
        check(std::sqrt(vertex[0]*vertex[0]+vertex[1]*vertex[1]+vertex[2]*vertex[2])<20.0F,"negative terrain remains below reference sphere; renderer masks basemap instead of lifting data");
    }
    terrain.insert(QStringLiteral("width"),1000000);preview.insert(QStringLiteral("terrain"),terrain);
    check(kindCount(explorationPreviewMeshes(projection(preview)),QStringLiteral("terrain"))==0,"oversized grid rejected");
    building.insert(QStringLiteral("ring"),QVariantList{QVariantList{900,0},QVariantList{1,0},QVariantList{1,1}});preview.insert(QStringLiteral("buildings"),QVariantList{building});
    check(kindCount(explorationPreviewMeshes(projection(preview)),QStringLiteral("buildings"))==0,"invalid coordinate rejected");
    // Roughly 100 km north/south at 80 N: the interior Mercator row is not 80.0 N.
    const QVariantMap polarBounds{{QStringLiteral("south"),79.55},{QStringLiteral("north"),80.45},{QStringLiteral("west"),-0.01},{QStringLiteral("east"),0.01}};
    const QVariantMap polarTerrain{{QStringLiteral("width"),2},{QStringLiteral("height"),3},{QStringLiteral("elevations_m"),QVariantList{0,0,1000,1000,2000,2000}}};
    const QVariantList polarRing{QVariantList{0,80.228},QVariantList{0.001,80.228},QVariantList{0.001,80.2281}};
    const QVariantMap polarBuilding{{QStringLiteral("ring"),polarRing},{QStringLiteral("height_m"),40}};
    const QVariantMap polarPreview{{QStringLiteral("bounds"),polarBounds},{QStringLiteral("terrain"),polarTerrain},{QStringLiteral("row_projection"),QStringLiteral("web_mercator")},{QStringLiteral("buildings"),QVariantList{polarBuilding}}};
    const auto polar=explorationPreviewMeshes(projection(polarPreview));
    check(kindCount(polar,QStringLiteral("terrain"))==1,"high latitude grid remains drawable");
    constexpr double radians=3.14159265358979323846/180.0;
    const double rowLatitude=std::atan(std::sinh((std::asinh(std::tan(80.45*radians))+std::asinh(std::tan(79.55*radians)))/2.0))/radians;
    check(std::fabs(rowLatitude-80.0)>0.005,"regression fixture distinguishes Mercator from linear latitude");
    for(const auto &entry:polar) {
        const auto mesh=entry.toMap();const auto data=mesh.value(QStringLiteral("vertices")).toByteArray();
        if(mesh.value(QStringLiteral("kind"))==QStringLiteral("terrain")) {
            float vertex[6];std::memcpy(vertex,data.constData()+6*sizeof(float),sizeof(vertex));
            const double radius=std::sqrt(double(vertex[0])*vertex[0]+double(vertex[1])*vertex[1]+double(vertex[2])*vertex[2]);
            const double actual=std::asin(vertex[1]/radius)/radians;
            check(std::fabs(actual-rowLatitude)<0.00003,"interior grid vertex uses inverse WebMercator row position");
        }
        if(mesh.value(QStringLiteral("kind"))==QStringLiteral("buildings")) {
            float vertex[6];std::memcpy(vertex,data.constData(),sizeof(vertex));
            const double radius=std::sqrt(double(vertex[0])*vertex[0]+double(vertex[1])*vertex[1]+double(vertex[2])*vertex[2]);
            check(std::fabs((radius-20.0)*6371000.0/20.0-1001.0)<1.0,"building ground sampling uses Mercator row inverse index");
        }
    }
    auto selected=projection(polarPreview);auto selectedJobs=selected.value(QStringLiteral("jobs")).toList();auto newer=selectedJobs.first().toMap();newer[QStringLiteral("id")]=QStringLiteral("newer");newer[QStringLiteral("preview")]=QVariantMap{};selectedJobs.append(newer);selected[QStringLiteral("jobs")]=selectedJobs;
    selected[QStringLiteral("selected_job_id")]=QStringLiteral("test");
    check(kindCount(explorationPreviewMeshes(selected),QStringLiteral("terrain"))==1,"selected older AOI uses its own recorded terrain instead of latest job");
    selected[QStringLiteral("selected_job_id")]=QStringLiteral("missing");check(explorationPreviewMeshes(selected).isEmpty(),"missing selected AOI never falls back to another terrain capture");
    selected.remove(QStringLiteral("selected_job_id"));check(kindCount(explorationPreviewMeshes(selected),QStringLiteral("terrain"))==0,"absent selector follows latest job even if an older capture has terrain");
    qInfo("Exploration preview contracts passed");
}
