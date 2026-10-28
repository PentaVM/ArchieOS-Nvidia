#include "WorldVisuals.h"
#include "archie/visual/geo.hpp"
#include <QCoreApplication>
#include <QBuffer>
#include <QDebug>
#include <QVector3D>
#include <cmath>
#include <cstring>
#include <cstdlib>
void check(bool ok,const char *why){if(!ok){qCritical("%s",why);std::exit(1);}}
QVariantMap bounds(double s,double w,double n,double e){return {{"south",s},{"west",w},{"north",n},{"east",e}};}
int main(int argc,char **argv){
 QCoreApplication app(argc,argv);QImage image(32,32,QImage::Format_RGB32);image.fill(Qt::red);QByteArray png;QBuffer buffer(&png);buffer.open(QIODevice::WriteOnly);image.save(&buffer,"PNG");
 QVariantMap tile{{"bounds",bounds(79.55,-2,80.45,2)},{"mime","image/png"},{"data_base64",QString::fromLatin1(png.toBase64())},{"width",32},{"height",32},{"zoom",10}};
 QVariantMap source{{"source_id","test-source"},{"label","Recorded unit fixture"},{"attribution","Test pixels only"},{"imagery_date","2020-01-01"},{"retrieved_at_ms",1234},{"mode","open_data"},{"state","ready"},{"tiles",QVariantList{tile}}};
 QVariantMap camera{{"latitude",80.0},{"longitude",0},{"radius_m",50000},{"elevation_m",QVariant()}};
 QVariantMap signal{{"id","actual-point"},{"kind","aircraft"},{"label","Reported aircraft"},{"latitude",80.0},{"longitude",0},{"altitude_m",1000},{"observed_at_ms",1234},{"source_id","source"},{"properties",QVariantMap{{"speed",42}}}};
 QVariantMap preview{{"bounds",bounds(79.775,-1,80.225,1)},{"imagery",QVariantList{source}},{"signals",QVariantMap{{"points",QVariantList{signal}}}}};
 QVariantMap job{{"id","job"},{"domain","personal"},{"camera",camera},{"preview",preview}};
 QVariantMap projection{{"jobs",QVariantList{job}}};
 const auto visual=buildWorldVisuals(projection);
 check(visual.imageryMeshes.size()==1&&visual.images.size()==1,"bounded inline pixels become georeferenced texture geometry");
 const auto mesh=visual.imageryMeshes.first().toMap();const auto bytes=mesh["vertices"].toByteArray();float vertex[8];std::memcpy(vertex,bytes.constData(),sizeof(vertex));
 check(std::fabs(vertex[6]-0.25F)<0.00001F,"tile UV clips western edge correctly");
 constexpr double r=3.14159265358979323846/180;
 auto merc=[&](double lat){return std::asinh(std::tan(lat*r));};
 const double expected=(merc(80.45)-merc(80.225))/(merc(80.45)-merc(79.55));
 check(std::fabs(vertex[7]-expected)<0.00001,"UV latitude is WebMercator, north at image v0");
 check(mesh["textureSource"].toString().startsWith("image://world/"),"only memory image provider addresses are emitted");
 check(visual.points.size()==2,"AOI and actual signal create anchored points");
 check(visual.points.first().toMap()["label"].toString()==QString::fromUtf8("AOI · personal"),"AOI label preserves UTF-8 punctuation");
 const auto aircraft=visual.points.last().toMap();check(aircraft["altitudeKnown"].toBool(),"measured aircraft altitude remains known");
 check(aircraft["observed_at_ms"].toLongLong()==1234&&aircraft["properties"].toMap()["speed"].toInt()==42,"point provenance preserved");
 auto newerSource=source;newerSource["source_id"]="newer-source";auto newerPoint=signal;newerPoint["id"]="newer-point";
 auto newerPreview=preview;newerPreview["imagery"]=QVariantList{newerSource};newerPreview["signals"]=QVariantMap{{"points",QVariantList{newerPoint}}};
 auto newerJob=job;newerJob["id"]="newer-job";newerJob["preview"]=newerPreview;
 QVariantMap selectedProjection{{"jobs",QVariantList{job,newerJob}},{"selected_job_id","job"}};
 const auto olderVisual=buildWorldVisuals(selectedProjection);
 check(olderVisual.imageryMeshes.first().toMap()["source_id"]=="test-source"&&olderVisual.points.last().toMap()["id"]=="actual-point","older AOI selection loads its own pixels and observations while newer job remains listed");
 selectedProjection["selected_job_id"]="missing-job";const auto missingVisual=buildWorldVisuals(selectedProjection);
 check(missingVisual.imageryMeshes.isEmpty()&&missingVisual.points.size()==2,"missing selected capture retains only requested AOI nodes and never displays newer observations");
 selectedProjection.remove("selected_job_id");check(buildWorldVisuals(selectedProjection).imageryMeshes.first().toMap()["source_id"]=="newer-source","only absent capture selector follows latest area");
 preview["terrain"]=QVariantMap{{"width",2},{"height",2},{"elevations_m",QVariantList{0,0,0,1000}}};
 preview["row_projection"]="web_mercator";job["preview"]=preview;projection["jobs"]=QVariantList{job};
 const auto draped=buildWorldVisuals(projection).imageryMeshes.first().toMap()["vertices"].toByteArray();
 check(draped.size()==6*8*int(sizeof(float)),"imagery preserves existing terrain triangle topology instead of crossing ridges with a separate grid");
 float high[8];std::memcpy(high,draped.constData()+5*8*sizeof(float),sizeof(high));
 const double highRadius=std::sqrt(double(high[0])*high[0]+double(high[1])*high[1]+double(high[2])*high[2]);
 const auto measuredHigh=archie::visual::globe_position({79.775,1},0,{float(20+1000*20.0/6371000),0,0});
 check(high[0]==measuredHigh.x&&high[1]==measuredHigh.y&&high[2]==measuredHigh.z,"image surface uses measured terrain height without an artificial elevation bias");
 check(buildWorldVisuals(projection).imageryMeshes.first().toMap()["replacesTerrain"].toBool(),"terrain imagery replaces the covered surface rather than overlapping it");
 auto halfTile=tile;halfTile["bounds"]=bounds(79.775,-1,80.225,0);auto halfSource=source;halfSource["tiles"]=QVariantList{halfTile};
 auto halfPreview=preview;halfPreview["imagery"]=QVariantList{halfSource};auto halfJob=job;halfJob["preview"]=halfPreview;
 const auto partial=buildWorldVisuals(QVariantMap{{"jobs",QVariantList{halfJob}}});
 check(partial.imageryMeshes.size()==2&&partial.imageryMeshes.last().toMap()["kind"]=="terrain","partial imagery retains an explicit uncovered measured terrain surface");
 const auto uncovered=partial.imageryMeshes.last().toMap()["vertices"].toByteArray();
 for(qsizetype offset=0;offset<uncovered.size();offset+=8*sizeof(float)) {float p[8];std::memcpy(p,uncovered.constData()+offset,sizeof(p));check(p[6]>=0.5F,"uncovered terrain remains east of the imagery boundary and never overlaps its interior");}
 auto meshArea=[](const QByteArray &data){double area=0;for(qsizetype offset=0;offset<data.size();offset+=24*sizeof(float)){float v[24];std::memcpy(v,data.constData()+offset,sizeof(v));QVector3D a(v[0],v[1],v[2]),b(v[8],v[9],v[10]),c(v[16],v[17],v[18]);area+=QVector3D::crossProduct(b-a,c-a).length()/2;}return area;};
 const auto covered=partial.imageryMeshes.first().toMap()["vertices"].toByteArray();
 check(std::abs(meshArea(covered)+meshArea(uncovered)-meshArea(draped))<meshArea(draped)*0.0001,"imagery and remaining terrain partition the entire measured surface without holes or duplicated area");
 auto surfaceSignal=signal;surfaceSignal["altitude_m"]=QVariant();surfaceSignal["latitude"]=std::atan(std::sinh((merc(80.225)+merc(79.775))/2))/r;
 preview["signals"]=QVariantMap{{"points",QVariantList{surfaceSignal}}};job["preview"]=preview;projection["jobs"]=QVariantList{job};
 const auto surfacePoint=buildWorldVisuals(projection).points.last().toMap();const auto surfacePosition=surfacePoint["position"].value<QVector3D>();
 check(surfacePoint["anchor_mode"]=="terrain_surface"&&std::abs(surfacePosition.length()-(20+20.0/6371000))<0.000004,"surface marker interpolates the rendered terrain triangle, not a different bilinear surface");
 preview.remove("terrain");preview.remove("row_projection");
 signal["latitude"]=999;preview["signals"]=QVariantMap{{"points",QVariantList{signal}}};job["preview"]=preview;projection["jobs"]=QVariantList{job};
 check(buildWorldVisuals(projection).points.size()==1,"invalid or ungeographic contacts never become markers");
 signal["latitude"]=80.0;signal["altitude_m"]=QVariant();preview["signals"]=QVariantMap{{"points",QVariantList{signal}}};job["preview"]=preview;projection["jobs"]=QVariantList{job};
 const auto unknown=buildWorldVisuals(projection).points.last().toMap();check(!unknown["altitudeKnown"].toBool()&&unknown["anchor_mode"]=="map_reference_unknown_elevation","missing aircraft altitude remains explicitly unknown at map reference");
 tile["data_base64"]=QString::fromLatin1(QByteArray("<svg width='32' height='32'><image href='file:///etc/passwd'/></svg>").toBase64());source["tiles"]=QVariantList{tile};preview["imagery"]=QVariantList{source};job["preview"]=preview;projection["jobs"]=QVariantList{job};check(buildWorldVisuals(projection).images.isEmpty(),"declared PNG cannot select an SVG decoder or external image loader");
 tile["data_base64"]="https://example.invalid/not-pixels";source["tiles"]=QVariantList{tile};preview["imagery"]=QVariantList{source};job["preview"]=preview;projection["jobs"]=QVariantList{job};
 check(buildWorldVisuals(projection).images.isEmpty(),"renderer never fetches source URLs");
 tile["data_base64"]=QString::fromLatin1(QByteArray(384*1024+1,'x').toBase64());source["tiles"]=QVariantList{tile};preview["imagery"]=QVariantList{source};job["preview"]=preview;projection["jobs"]=QVariantList{job};
 check(buildWorldVisuals(projection).images.isEmpty(),"inline image aggregate is capped before decode");
 qInfo("World imagery/beacon DTO contracts passed");
}
