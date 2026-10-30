#include "ExplorationPreview.h"
#include "ExplorationSelection.h"
#include "archie/visual/geo.hpp"
#include <QVector3D>
#include <cmath>
#include <cstring>
#include <algorithm>
namespace {
constexpr double earth=6371000.0;
constexpr double scale=20.0/earth;
constexpr double radians=3.14159265358979323846/180.0;
double mercator(double latitude) {return std::log(std::tan(3.14159265358979323846/4.0+latitude*radians/2.0));}
double inverseMercator(double y) {return std::atan(std::sinh(y))/radians;}
bool number(const QVariant &v) {
    const int type=v.metaType().id();
    const bool numeric=type==QMetaType::Double||type==QMetaType::Float||type==QMetaType::Int||type==QMetaType::UInt||type==QMetaType::LongLong||type==QMetaType::ULongLong;
    return numeric && !v.isNull() && std::isfinite(v.toDouble());
}
QVector3D point(double lat,double lon,double height=0) {
    const auto p=archie::visual::globe_position({lat,lon},0,{float(20+height*scale),0,0});return {p.x,p.y,p.z};
}
struct Mesh {
    QByteArray vertices,indices;
    quint32 count=0;
    void triangle(QVector3D a,QVector3D b,QVector3D c) {
        auto n=QVector3D::crossProduct(b-a,c-a).normalized();
        for (auto p:{a,b,c}) { float v[]={p.x(),p.y(),p.z(),n.x(),n.y(),n.z()};vertices.append(reinterpret_cast<const char*>(v),sizeof(v));indices.append(reinterpret_cast<const char*>(&count),sizeof(count));++count; }
    }
    void line(QVector3D a,QVector3D b) {
        // A one-metre display stroke, not a claim about building wall thickness.
        auto side=QVector3D::crossProduct(b-a,(a+b).normalized()).normalized()*float(scale*0.5);
        triangle(a-side,b-side,b+side);triangle(a-side,b+side,a+side);
    }
    QVariantMap pack(const QString &kind) { return {{QStringLiteral("kind"),kind},{QStringLiteral("vertices"),vertices},{QStringLiteral("triangleIndices"),indices}}; }
};
}
QVariantList explorationPreviewMeshes(const QVariantMap &projection) {
    QVariantList result;
    const auto jobs=projection.value(QStringLiteral("jobs")).toList();
    // A read-only selected capture wins; no selector follows the newest AOI.
    if (jobs.isEmpty()) return result;
    const auto job=selectedExplorationJob(projection);
    if(job.isEmpty())return result;
    const auto camera=job.value(QStringLiteral("camera")).toMap();
    const auto preview=job.value(QStringLiteral("preview")).toMap();
    const auto bounds=preview.value(QStringLiteral("bounds")).toMap();
    const auto terrain=preview.value(QStringLiteral("terrain")).toMap();
    const int w=terrain.value(QStringLiteral("width")).toInt(),h=terrain.value(QStringLiteral("height")).toInt();
    const auto heights=terrain.value(QStringLiteral("elevations_m")).toList();
    const double south=bounds.value(QStringLiteral("south")).toDouble(),north=bounds.value(QStringLiteral("north")).toDouble();
    const double west=bounds.value(QStringLiteral("west")).toDouble(),east=bounds.value(QStringLiteral("east")).toDouble();
    const auto rowProjection=preview.value(QStringLiteral("row_projection")).toString();
    const bool rowSupported=rowProjection.isEmpty() || rowProjection==QStringLiteral("web_mercator");
    const bool grid=rowSupported && w>=2&&w<=33&&h>=2&&h<=33&&heights.size()==w*h&&std::isfinite(south)&&std::isfinite(north)&&south>=-85.0511287798066&&north<=85.0511287798066&&south<north&&west>=-180&&east<=180&&west<east;
    Mesh land;
    auto validHeight=[&](int i) {return number(heights[i])&&heights[i].toDouble()>=-12000&&heights[i].toDouble()<=12000;};
    // Rows are uniform in WebMercator y, not latitude (also used by source codecs/exports).
    const double northY=grid ? mercator(north) : 0,southY=grid ? mercator(south) : 0;
    auto vertex=[&](int x,int y) {return point(inverseMercator(northY+(southY-northY)*y/(h-1)),west+(east-west)*x/(w-1),heights[y*w+x].toDouble());};
    if (grid) for (int y=0;y<h-1;++y) for (int x=0;x<w-1;++x) {
        int a=y*w+x,b=a+1,c=a+w,d=c+1;
        if (validHeight(a)&&validHeight(c)&&validHeight(b)) land.triangle(vertex(x,y),vertex(x,y+1),vertex(x+1,y));
        if (validHeight(b)&&validHeight(c)&&validHeight(d)) land.triangle(vertex(x+1,y),vertex(x,y+1),vertex(x+1,y+1));
    }
    if (land.count) result.push_back(land.pack(QStringLiteral("terrain")));
    auto elevation=[&](double lat,double lon) {
        if (!grid || lat<south || lat>north || lon<west || lon>east) return 0.0;
        int x=std::clamp(int(std::round((lon-west)/(east-west)*(w-1))),0,w-1);
        int y=std::clamp(int(std::round((northY-mercator(lat))/(northY-southY)*(h-1))),0,h-1);
        return validHeight(y*w+x)?heights[y*w+x].toDouble():0.0;
    };
    Mesh outlines,walls;
    const auto buildings=preview.value(QStringLiteral("buildings")).toList();
    for (int i=0;i<std::min<qsizetype>(128,buildings.size());++i) {
        const auto building=buildings[i].toMap();const auto ring=building.value(QStringLiteral("ring")).toList();
        if (ring.size()<3 || ring.size()>512) continue;
        bool valid=true;
        for (const auto &v:ring) { const auto p=v.toList();if(p.size()!=2||!number(p[0])||!number(p[1])||std::abs(p[0].toDouble())>180||std::abs(p[1].toDouble())>90){valid=false;break;} }
        if (!valid) continue;
        const auto height=building.value(QStringLiteral("height_m"));
        const bool measured=number(height)&&height.toDouble()>0&&height.toDouble()<=2000;
        for (qsizetype j=0;j<ring.size();++j) {
            const auto a=ring[j].toList(),b=ring[(j+1)%ring.size()].toList();
            const double alon=a[0].toDouble(),alat=a[1].toDouble(),blon=b[0].toDouble(),blat=b[1].toDouble();
            // One metre depth bias avoids z-fighting; unknown ground datum remains explicit in HUD.
            auto lowA=point(alat,alon,elevation(alat,alon)+1),lowB=point(blat,blon,elevation(blat,blon)+1);
            outlines.line(lowA,lowB);
            if(measured) {
                auto highA=point(alat,alon,elevation(alat,alon)+height.toDouble()),highB=point(blat,blon,elevation(blat,blon)+height.toDouble());
                walls.triangle(lowA,lowB,highB);walls.triangle(lowA,highB,highA);outlines.line(highA,highB);
            }
        }
    }
    if(walls.count)result.push_back(walls.pack(QStringLiteral("buildings")));
    if(outlines.count)result.push_back(outlines.pack(QStringLiteral("outlines")));
    Mesh target;
    const auto lat=camera.value(QStringLiteral("latitude")),lon=camera.value(QStringLiteral("longitude")),radius=camera.value(QStringLiteral("radius_m"));
    if(number(lat)&&number(lon)&&number(radius)&&std::abs(lat.toDouble())<=90&&std::abs(lon.toDouble())<=180&&radius.toDouble()>0&&radius.toDouble()<=earth) {
        const double phi=lat.toDouble()*3.14159265358979323846/180,lambda=lon.toDouble()*3.14159265358979323846/180,angular=radius.toDouble()/earth;
        auto circle=[&](int n){double bearing=n*2*3.14159265358979323846/96;double p=std::asin(std::sin(phi)*std::cos(angular)+std::cos(phi)*std::sin(angular)*std::cos(bearing));double l=lambda+std::atan2(std::sin(bearing)*std::sin(angular)*std::cos(phi),std::cos(angular)-std::sin(phi)*std::sin(p));return point(p*180/3.14159265358979323846,l*180/3.14159265358979323846,elevation(p*180/3.14159265358979323846,l*180/3.14159265358979323846)+2);};
        for(int i=0;i<96;++i)target.line(circle(i),circle(i+1));
        result.push_back(target.pack(QStringLiteral("aoi")));
    }
    return result;
}
