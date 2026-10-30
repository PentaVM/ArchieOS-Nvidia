#include "WorldVisuals.h"
#include "ExplorationSelection.h"
#include "archie/visual/geo.hpp"
#include <QVector3D>
#include <QBuffer>
#include <QImageReader>
#include <QCryptographicHash>
#include <cmath>
#include <algorithm>
namespace {
QString k(const char *s){return QString::fromUtf8(s);}
QVariant get(const QVariantMap &m,const char *s){return m.value(k(s));}
bool number(const QVariant &v){const int t=v.metaType().id();return !v.isNull()&&(t==QMetaType::Double||t==QMetaType::Float||t==QMetaType::Int||t==QMetaType::UInt||t==QMetaType::LongLong||t==QMetaType::ULongLong)&&std::isfinite(v.toDouble());}
constexpr double earth=6371000.0,scale=20.0/earth,rad=3.14159265358979323846/180;
double merc(double lat){return std::asinh(std::tan(lat*rad));}
double inv(double y){return std::atan(std::sinh(y))/rad;}
struct Bounds{double s,w,n,e;bool valid;};
Bounds bounds(QVariantMap b){Bounds p{get(b,"south").toDouble(),get(b,"west").toDouble(),get(b,"north").toDouble(),get(b,"east").toDouble(),false};p.valid=number(get(b,"south"))&&number(get(b,"west"))&&number(get(b,"north"))&&number(get(b,"east"))&&p.s>=-85.0511287798066&&p.n<=85.0511287798066&&p.s<p.n&&p.w>=-180&&p.e<=180&&p.w<p.e;return p;}
QVector3D point(double lat,double lon,double h){auto p=archie::visual::globe_position({lat,lon},0,{float(20+h*scale),0,0});return {p.x,p.y,p.z};}
struct Ground {
 Bounds b;int w,h;QVariantList heights;
 explicit Ground(const QVariantMap &preview):b(bounds(get(preview,"bounds").toMap())){auto t=get(preview,"terrain").toMap();w=get(t,"width").toInt();h=get(t,"height").toInt();heights=get(t,"elevations_m").toList();auto rows=get(preview,"row_projection").toString();b.valid=b.valid&&(rows.isEmpty()||rows==k("web_mercator"))&&w>=2&&w<=33&&h>=2&&h<=33&&heights.size()==w*h;}
 std::pair<double,bool> at(double lat,double lon)const{
  if(!b.valid||lat<b.s||lat>b.n||lon<b.w||lon>b.e)return {0,false};
  double x=std::clamp((lon-b.w)/(b.e-b.w)*(w-1),0.0,double(w-1));double y=std::clamp((merc(b.n)-merc(lat))/(merc(b.n)-merc(b.s))*(h-1),0.0,double(h-1));
  int ix=std::min(int(x),w-2),iy=std::min(int(y),h-2);double fx=x-ix,fy=y-iy;
  int a=iy*w+ix,b=a+1,c=a+w,d=c+1;
  int indices[]={a,b,c};double weights[]={1-fx-fy,fx,fy};
  if(fx+fy>1){indices[0]=b;indices[1]=c;indices[2]=d;weights[0]=1-fy;weights[1]=1-fx;weights[2]=fx+fy-1;}
  double value=0;
  for(int i=0;i<3;++i){if(!number(heights[indices[i]])||std::abs(heights[indices[i]].toDouble())>12000)return {0,false};value+=heights[indices[i]].toDouble()*weights[i];}
  return {value,true};
 }
};
struct TexturedMesh{
 QByteArray vertices,indices;quint32 count=0;
 void triangle(QVector3D a,QVector3D b,QVector3D c,QPointF ua,QPointF ub,QPointF uc){auto normal=QVector3D::crossProduct(b-a,c-a).normalized();QVector3D positions[]={a,b,c};QPointF uv[]={ua,ub,uc};for(int i=0;i<3;++i){auto p=positions[i];float v[]={p.x(),p.y(),p.z(),normal.x(),normal.y(),normal.z(),float(uv[i].x()),float(uv[i].y())};vertices.append(reinterpret_cast<const char*>(v),sizeof(v));indices.append(reinterpret_cast<const char*>(&count),sizeof(count));++count;}}
};
// Clip each existing terrain triangle rather than bridging a different image grid
// across ridges. Position interpolation keeps pixels on the actual rendered mesh.
struct SurfaceVertex {double lon,y;QVector3D position;};
using Polygon=QList<SurfaceVertex>;
Polygon clipSurface(Polygon polygon,double boundary,bool longitude,bool greater){
 Polygon output;if(polygon.isEmpty())return output;
 auto value=[&](const SurfaceVertex &v){return longitude?v.lon:v.y;};
 auto inside=[&](const SurfaceVertex &v){return greater?value(v)>=boundary:value(v)<=boundary;};
 auto previous=polygon.last();bool previousInside=inside(previous);
 for(const auto &current:polygon){bool currentInside=inside(current);if(currentInside!=previousInside){double t=(boundary-value(previous))/(value(current)-value(previous));output.push_back({previous.lon+(current.lon-previous.lon)*t,previous.y+(current.y-previous.y)*t,previous.position+(current.position-previous.position)*float(t)});}if(currentInside)output.push_back(current);previous=current;previousInside=currentInside;}
 return output;
}
void drapeTerrain(TexturedMesh &mesh,const Ground &ground,const Bounds &area,const Bounds &tile){
 const double north=merc(ground.b.n),south=merc(ground.b.s),tileNorth=merc(tile.n),tileSouth=merc(tile.s);
 auto vertex=[&](int x,int y){double lon=ground.b.w+(ground.b.e-ground.b.w)*x/(ground.w-1),my=north+(south-north)*y/(ground.h-1);return SurfaceVertex{lon,my,point(inv(my),lon,ground.heights[y*ground.w+x].toDouble())};};
 auto valid=[&](int x,int y){auto v=ground.heights[y*ground.w+x];return number(v)&&std::abs(v.toDouble())<=12000;};
 auto emitTriangle=[&](int ax,int ay,int bx,int by,int cx,int cy){
  if(!valid(ax,ay)||!valid(bx,by)||!valid(cx,cy))return;
  Polygon polygon{vertex(ax,ay),vertex(bx,by),vertex(cx,cy)};
  polygon=clipSurface(polygon,area.w,true,true);polygon=clipSurface(polygon,area.e,true,false);polygon=clipSurface(polygon,merc(area.s),false,true);polygon=clipSurface(polygon,merc(area.n),false,false);
  auto uv=[&](const SurfaceVertex &v){return QPointF((v.lon-tile.w)/(tile.e-tile.w),(tileNorth-v.y)/(tileNorth-tileSouth));};
  for(int i=1;i+1<polygon.size();++i)mesh.triangle(polygon[0].position,polygon[i].position,polygon[i+1].position,uv(polygon[0]),uv(polygon[i]),uv(polygon[i+1]));
 };
 for(int y=0;y<ground.h-1;++y)for(int x=0;x<ground.w-1;++x){emitTriangle(x,y,x,y+1,x+1,y);emitTriangle(x+1,y,x,y+1,x+1,y+1);}
}
// The imagery IS the measured terrain where it is present. Render uncovered
// pieces once, instead of z-fighting two almost coincident global-coordinate
// surfaces (a half-metre offset is only one float ULP at this globe radius).
TexturedMesh uncoveredTerrain(const Ground &ground,const QList<Bounds> &coverage) {
 TexturedMesh mesh;const double north=merc(ground.b.n),south=merc(ground.b.s);
 auto vertex=[&](int x,int y){double lon=ground.b.w+(ground.b.e-ground.b.w)*x/(ground.w-1),my=north+(south-north)*y/(ground.h-1);return SurfaceVertex{lon,my,point(inv(my),lon,ground.heights[y*ground.w+x].toDouble())};};
 auto valid=[&](int x,int y){auto v=ground.heights[y*ground.w+x];return number(v)&&std::abs(v.toDouble())<=12000;};
 auto emitTriangle=[&](int ax,int ay,int bx,int by,int cx,int cy){
  if(!valid(ax,ay)||!valid(bx,by)||!valid(cx,cy))return;
  QList<Polygon> pieces{Polygon{vertex(ax,ay),vertex(bx,by),vertex(cx,cy)}};
  for(const auto &area:coverage) {
   QList<Polygon> remaining;
   for(auto polygon:pieces) {
    auto keep=[&](double edge,bool longitude,bool greater){auto outside=clipSurface(polygon,edge,longitude,greater);if(outside.size()>=3)remaining.push_back(outside);polygon=clipSurface(polygon,edge,longitude,!greater);};
    keep(area.w,true,false);keep(area.e,true,true);keep(merc(area.s),false,false);keep(merc(area.n),false,true);
   }
   pieces=remaining;if(pieces.isEmpty())break;
  }
  for(const auto &polygon:pieces)for(int i=1;i+1<polygon.size();++i) {
   auto a=polygon[0].position,b=polygon[i].position,c=polygon[i+1].position;
   auto uv=[&](const SurfaceVertex &v){return QPointF((v.lon-ground.b.w)/(ground.b.e-ground.b.w),(north-v.y)/(north-south));};
   if(QVector3D::crossProduct(b-a,c-a).lengthSquared()>1e-24F)mesh.triangle(a,b,c,uv(polygon[0]),uv(polygon[i]),uv(polygon[i+1]));
  }
 };
 for(int y=0;y<ground.h-1;++y)for(int x=0;x<ground.w-1;++x){emitTriangle(x,y,x,y+1,x+1,y);emitTriangle(x+1,y,x,y+1,x+1,y+1);}
 return mesh;
}
}
WorldVisualData buildWorldVisuals(const QVariantMap &projection){
 WorldVisualData result;const auto jobs=get(projection,"jobs").toList();if(jobs.isEmpty())return result;
 // AOI beacons identify actual requests; none are invented from system nodes.
 for(const auto &entry:jobs){auto job=entry.toMap(),camera=get(job,"camera").toMap();auto lat=get(camera,"latitude"),lon=get(camera,"longitude"),radius=get(camera,"radius_m");if(!number(lat)||!number(lon)||!number(radius)||std::abs(lat.toDouble())>90||std::abs(lon.toDouble())>180||radius.toDouble()<=0)continue;auto elevation=get(camera,"elevation_m");double z=number(elevation)&&std::abs(elevation.toDouble())<=12000?elevation.toDouble():0;result.points.push_back(QVariantMap{{k("id"),k("aoi:")+get(job,"id").toString()},{k("kind"),k("aoi")},{k("label"),k("AOI · ")+get(job,"domain").toString()},{k("latitude"),lat},{k("longitude"),lon},{k("radius_m"),radius},{k("altitude_m"),elevation},{k("altitudeKnown"),number(elevation)},{k("position"),QVariant::fromValue(point(lat.toDouble(),lon.toDouble(),z+1))},{k("job"),job},{k("source_id"),k("GeoFabric · requested AOI")},{k("observed_at_ms"),get(job,"created_at_ms")}});}
 auto job=selectedExplorationJob(projection);if(job.isEmpty())return result;auto preview=get(job,"preview").toMap();Ground ground(preview);auto clip=bounds(get(preview,"bounds").toMap());qsizetype imageBytes=0;int totalTiles=0;
 for(const auto &entry:get(preview,"imagery").toList()){
  if(result.imagerySources.size()>=12)break;auto source=entry.toMap();QVariantMap meta;
  for(auto key:{"source_id","label","attribution","imagery_date","requested_date","date_selection","retrieved_at_ms","mode","state","error"})meta.insert(k(key),get(source,key));
  int accepted=0;QString decodeError;QVariant sourceResolution,previewResolution;QList<Bounds> coverage;
  for(const auto &tileEntry:get(source,"tiles").toList()){
   if(totalTiles>=32){decodeError=k("Renderer tile bound reached");break;}auto tile=tileEntry.toMap();auto b=bounds(get(tile,"bounds").toMap());if(!clip.valid||!b.valid)continue;
   Bounds area{std::max(clip.s,b.s),std::max(clip.w,b.w),std::min(clip.n,b.n),std::min(clip.e,b.e),true};if(area.s>=area.n||area.w>=area.e)continue;
   const auto encoded=get(tile,"data_base64").toString().toLatin1();if(encoded.size()>524292){decodeError=k("Inline imagery byte bound exceeded");continue;}
   auto decoded=QByteArray::fromBase64Encoding(encoded,QByteArray::AbortOnBase64DecodingErrors);if(!decoded||decoded.decoded.isEmpty()){decodeError=k("Invalid inline image encoding");continue;}
   if(imageBytes+decoded.decoded.size()>384*1024){decodeError=k("Inline imagery byte bound exceeded");continue;}imageBytes+=decoded.decoded.size();
   const auto mime=get(tile,"mime").toString();if(mime!=k("image/jpeg")&&mime!=k("image/png")){decodeError=k("Unsupported image format");continue;}
   QBuffer buffer(&decoded.decoded);buffer.open(QIODevice::ReadOnly);QImageReader reader(&buffer,mime==k("image/png")?QByteArrayLiteral("png"):QByteArrayLiteral("jpeg"));reader.setAutoDetectImageFormat(false);reader.setDecideFormatFromContent(false);const auto size=reader.size();if(size.width()<1||size.height()<1||size.width()>1024||size.height()>1024||size.width()!=get(tile,"width").toInt()||size.height()!=get(tile,"height").toInt()){decodeError=k("Invalid image dimensions");continue;}
   auto image=reader.read();if(image.isNull()){decodeError=k("Image decode failed");continue;}
   const auto imageKey=QString::fromLatin1(QCryptographicHash::hash(decoded.decoded,QCryptographicHash::Sha256).toHex());result.images.insert(imageKey,image);sourceResolution=get(tile,"source_resolution_m");previewResolution=get(tile,"preview_resolution_m");
   TexturedMesh mesh;const double northY=merc(area.n),southY=merc(area.s),tileNorth=merc(b.n),tileSouth=merc(b.s);
   auto vertex=[&](int x,int y){double lat=inv(northY+(southY-northY)*y/8.0),lon=area.w+(area.e-area.w)*x/8.0;return point(lat,lon,ground.at(lat,lon).first);};
   auto uv=[&](int x,int y){double lat=inv(northY+(southY-northY)*y/8.0),lon=area.w+(area.e-area.w)*x/8.0;return QPointF((lon-b.w)/(b.e-b.w),(tileNorth-merc(lat))/(tileNorth-tileSouth));};
   if(ground.b.valid)drapeTerrain(mesh,ground,area,b);
   else for(int y=0;y<8;++y)for(int x=0;x<8;++x){mesh.triangle(vertex(x,y),vertex(x,y+1),vertex(x+1,y),uv(x,y),uv(x,y+1),uv(x+1,y));mesh.triangle(vertex(x+1,y),vertex(x,y+1),vertex(x+1,y+1),uv(x+1,y),uv(x,y+1),uv(x+1,y+1));}
   result.imageryMeshes.push_back(QVariantMap{{k("source_id"),get(source,"source_id")},{k("kind"),k("imagery")},{k("vertices"),mesh.vertices},{k("triangleIndices"),mesh.indices},{k("hasUv"),true},{k("replacesTerrain"),ground.b.valid},{k("textureSource"),k("image://world/")+imageKey}});coverage.push_back(area);++accepted;++totalTiles;
  }
  if(ground.b.valid&&!coverage.isEmpty()) {const auto remainder=uncoveredTerrain(ground,coverage);if(remainder.count)result.imageryMeshes.push_back(QVariantMap{{k("source_id"),get(source,"source_id")},{k("kind"),k("terrain")},{k("vertices"),remainder.vertices},{k("triangleIndices"),remainder.indices},{k("hasUv"),true}});}
  meta.insert(k("source_resolution_m"),sourceResolution);meta.insert(k("preview_resolution_m"),previewResolution);meta.insert(k("rendered_tiles"),accepted);meta.insert(k("renderer_error"),decodeError);result.imagerySources.push_back(meta);
 }
 const auto signalData=get(preview,"signals").toMap();result.signalSources=get(signalData,"sources").toList();
 for(const auto &entry:get(signalData,"points").toList()){
  if(result.points.size()>=288)break;auto p=entry.toMap();auto lat=get(p,"latitude"),lon=get(p,"longitude");if(get(p,"id").toString().isEmpty()||!number(lat)||!number(lon)||std::abs(lat.toDouble())>90||std::abs(lon.toDouble())>180)continue;
  auto altitude=get(p,"altitude_m");bool known=number(altitude)&&altitude.toDouble()>=-12000&&altitude.toDouble()<=100000;bool airborne=get(p,"kind")==k("aircraft");auto groundReading=ground.at(lat.toDouble(),lon.toDouble());double z=airborne&&known?altitude.toDouble():groundReading.first;
  p.insert(k("altitudeKnown"),known);p.insert(k("anchor_mode"),airborne&&known?k("measured_altitude"):groundReading.second?k("terrain_surface"):k("map_reference_unknown_elevation"));p.insert(k("position"),QVariant::fromValue(point(lat.toDouble(),lon.toDouble(),z+1)));result.points.push_back(p);
 }
 return result;
}
