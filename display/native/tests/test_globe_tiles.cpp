#include "GlobeTiles.h"
#include <QCoreApplication>
#include <QBuffer>
#include <QDebug>
#include <QVector3D>
#include <cmath>
#include <cstring>
#include <cstdlib>
void check(bool ok,const char *why){if(!ok){qCritical("%s",why);std::exit(1);}}
QVariantMap tile(int z,int x,int y,int size=256){QImage image(size,size,QImage::Format_RGB32);image.fill(QColor(40+z*7,x%256,y%256));QByteArray png;QBuffer b(&png);b.open(QIODevice::WriteOnly);image.save(&b,"PNG");return {{"zoom",z},{"x",x},{"y",y},{"mime","image/png"},{"width",size},{"height",size},{"data_base64",QString::fromLatin1(png.toBase64())}};}
int main(int argc,char **argv){QCoreApplication app(argc,argv);GlobeTiles tiles;
 check(tiles.ingest("nasa",{tile(0,0,0)}),"real 256 pixel image accepted");
 check(tiles.images().size()==1&&tiles.images().cbegin().value().size()==QSize(256,256),"decoded texture retains native resolution");
 check(tiles.setDesired({QVariantMap{{"zoom",18},{"x",131072},{"y",131072}}}),"new desired footprint updates fallback tessellation before child imagery arrives");
 check(tiles.meshes().size()>1,"coarse imagery tessellates to close camera footprint while the child downloads");
 tiles.setDesired({});
 auto root=tiles.meshes();check(root.size()==1,"root provides global Mercator fallback");
 const auto mesh=root.first().toMap();auto vertices=mesh["vertices"].toByteArray();auto origin=mesh["origin"].value<QVector3D>();
 float v[8];std::memcpy(v,vertices.constData(),sizeof(v));QVector3D point(v[0],v[1],v[2]);point+=origin;
 check(std::abs(point.length()-20.0F)<0.00002F,"tile lies on Earth radius");
 check(std::abs(point.y())>19.0F&&std::abs(v[6])<0.00001F&&std::abs(v[7])<0.00001F,"northern western tile corner is georeferenced with north UV zero");
 tiles.setDesired({QVariantMap{{"zoom",1},{"x",0},{"y",0}}});
 check(tiles.ingest("nasa",{tile(1,0,0)}),"child image accepted");auto refined=tiles.meshes();check(refined.size()==4,"ready child replaces only its quadrant while three ancestor regions remain");
 int native=0,fallback=0;for(auto entry:refined){auto m=entry.toMap();if(m["zoom"]==1)++native;else if(m["zoom"]==0)++fallback;}
 check(native==1&&fallback==3,"parent remains wherever children are pending");
 check(!tiles.ingest("nasa",{tile(1,0,0)}),"unchanged tile does not rebuild geometry or decode again");
 tiles.setDesired({QVariantMap{{"zoom",18},{"x",131072},{"y",131072}}});
 check(tiles.ingest("nasa",{tile(18,131072,131072,512)}),"street tile retains 512 pixels");
 check(tiles.images().size()==3,"previous ancestor images stay resident during refinement");
 bool local=false;for(auto entry:tiles.meshes()){auto m=entry.toMap();if(m["zoom"]==18){auto data=m["vertices"].toByteArray();std::memcpy(v,data.constData(),sizeof(v));local=QVector3D(v[0],v[1],v[2]).length()<0.001F&&m["origin"].value<QVector3D>().length()>19.99F;}}
 check(local,"street geometry uses local origin for ground precision");
 tiles.setDesired({QVariantMap{{"zoom",0},{"x",0},{"y",0}}});
 check(tiles.meshes().size()==1,"cached historical street detail does not inflate the orbital draw list");
 tiles.ingest("other",{tile(0,0,0)});check(tiles.images().size()==1,"source switching cannot expose another source's imagery");
 check(!tiles.ingest("other",{tile(25,0,0),tile(1,2,0),tile(1,0,-1),tile(0,0,0,1024)}),"invalid coordinates and oversized texture rejected");
 qInfo("Progressive globe tile contracts passed");}
