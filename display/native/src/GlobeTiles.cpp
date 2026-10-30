#include "GlobeTiles.h"
#include <QBuffer>
#include <QImageReader>
#include <QCryptographicHash>
#include <QVector3D>
#include <QSet>
#include <cmath>
#include <algorithm>
#include <functional>
namespace {
constexpr double pi=3.14159265358979323846;
QString key(int z,int x,int y){return QString::number(z)+QStringLiteral("/")+QString::number(x)+QStringLiteral("/")+QString::number(y);}
double latitude(double y,double n){return std::atan(std::sinh(pi*(1-2*y/n)));}
QVariantMap patch(int z,int x,int y,int imageZ,int imageX,int imageY,const QString &image){
    const double n=std::ldexp(1.0,z),imageN=std::ldexp(1.0,imageZ);
    const double lon=(double(x)+0.5)/n*2*pi-pi,lat=latitude(double(y)+0.5,n);
    const double ox=20*std::cos(lat)*std::sin(lon),oy=20*std::sin(lat),oz=20*std::cos(lat)*std::cos(lon);
    QByteArray vertices,indices;quint32 count=0;
    // Coarse patches follow the globe rather than drawing floating rectangles.
    // The dense root is only built once; street patches need four subdivisions.
    const int segments=std::clamp(int(std::ceil(128.0/n)),4,128);
    auto vertex=[&](int ix,int iy){
        const double tx=(x+double(ix)/segments)/n,ty=(y+double(iy)/segments)/n;
        const double a=tx*2*pi-pi,b=latitude(ty,1.0),nx=std::cos(b)*std::sin(a),ny=std::sin(b),nz=std::cos(b)*std::cos(a);
        const float v[]={float(20*nx-ox),float(20*ny-oy),float(20*nz-oz),float(nx),float(ny),float(nz),float(tx*imageN-imageX),float(ty*imageN-imageY)};
        vertices.append(reinterpret_cast<const char*>(v),sizeof(v));indices.append(reinterpret_cast<const char*>(&count),sizeof(count));++count;
    };
    for(int iy=0;iy<segments;++iy)for(int ix=0;ix<segments;++ix){vertex(ix,iy);vertex(ix,iy+1);vertex(ix+1,iy);vertex(ix+1,iy);vertex(ix,iy+1);vertex(ix+1,iy+1);}
    return {{QStringLiteral("vertices"),vertices},{QStringLiteral("triangleIndices"),indices},{QStringLiteral("origin"),QVariant::fromValue(QVector3D(float(ox),float(oy),float(oz)))},{QStringLiteral("textureSource"),QStringLiteral("image://globe/")+image},{QStringLiteral("zoom"),imageZ},{QStringLiteral("region_zoom"),z},{QStringLiteral("tile_id"),key(z,x,y)}};
}
}
bool GlobeTiles::ingest(const QString &source,const QVariantList &entries){
    bool changed=false;
    if(source!=m_source){m_source=source;m_tiles.clear();m_geometry.clear();m_desired.clear();changed=true;}
    m_error.clear();
    for(const auto &entry:entries){
        const auto tile=entry.toMap();bool zok=false,xok=false,yok=false;
        const int z=tile.value(QStringLiteral("zoom")).toInt(&zok),x=tile.value(QStringLiteral("x")).toInt(&xok),y=tile.value(QStringLiteral("y")).toInt(&yok);
        if(!zok||!xok||!yok||z<0||z>22||x<0||y<0||x>=std::ldexp(1.0,z)||y>=std::ldexp(1.0,z))continue;
        const auto id=key(z,x,y);auto existing=m_tiles.find(id);
        if(existing!=m_tiles.end()){existing->used=++m_clock;continue;}
        const auto mime=tile.value(QStringLiteral("mime")).toString();const auto encoded=tile.value(QStringLiteral("data_base64")).toString().toLatin1();
        if((mime!=QStringLiteral("image/jpeg")&&mime!=QStringLiteral("image/png"))||encoded.size()>2*1024*1024){m_error=QStringLiteral("Unsupported or oversized imagery");continue;}
        auto bytes=QByteArray::fromBase64Encoding(encoded,QByteArray::AbortOnBase64DecodingErrors);
        if(!bytes||bytes.decoded.isEmpty())continue;
        QBuffer buffer(&bytes.decoded);buffer.open(QIODevice::ReadOnly);
        QImageReader reader(&buffer,mime==QStringLiteral("image/png")?QByteArrayLiteral("png"):QByteArrayLiteral("jpeg"));reader.setAutoDetectImageFormat(false);reader.setDecideFormatFromContent(false);
        const auto size=reader.size();
        if(size.width()!=size.height()||(size.width()!=256&&size.width()!=512)||size.width()!=tile.value(QStringLiteral("width")).toInt()||size.height()!=tile.value(QStringLiteral("height")).toInt()){m_error=QStringLiteral("Stream tile must retain its 256 or 512 source pixels");continue;}
        auto image=reader.read();if(image.isNull()){m_error=QStringLiteral("Image decode failed");continue;}
        const auto digest=QString::fromLatin1(QCryptographicHash::hash(bytes.decoded,QCryptographicHash::Sha256).toHex());
        m_tiles.insert(id,Tile{z,x,y,digest,tile.value(QStringLiteral("key")).toString(),image,++m_clock});changed=true;
    }
    // Keep the root and recent detail bounded, including 512px NASA products.
    auto memory=[&]{qsizetype bytes=0;for(const auto &tile:m_tiles)bytes+=tile.image.sizeInBytes();return bytes;};
    while(m_tiles.size()>256||memory()>128*1024*1024){
        auto victim=m_tiles.end();for(auto it=m_tiles.begin();it!=m_tiles.end();++it)if(it->z>0&&(victim==m_tiles.end()||it->used<victim->used))victim=it;
        if(victim==m_tiles.end())break;m_tiles.erase(victim);changed=true;
    }
    return changed;
}
QHash<QString,QImage> GlobeTiles::images() const {QHash<QString,QImage> result;for(const auto &tile:m_tiles)result.insert(tile.imageKey,tile.image);return result;}
QVariantList GlobeTiles::meshes() const {
    QVariantList result;QSet<QString> branches;
    for(const auto &entry:m_desired){const auto tile=entry.toMap();const int zoom=tile.value(QStringLiteral("zoom")).toInt(),x=tile.value(QStringLiteral("x")).toInt(),y=tile.value(QStringLiteral("y")).toInt();
        if(zoom<0||zoom>22||x<0||y<0||x>=std::ldexp(1.0,zoom)||y>=std::ldexp(1.0,zoom))continue;
        for(int z=zoom-1;z>=0;--z)branches.insert(key(z,x>>(zoom-z),y>>(zoom-z)));
    }
    // Cache history is not a render plan: only today's visible footprint refines
    // the globe. Distant street tiles remain reusable without adding draw calls.
    std::function<void(int,int,int,const Tile*)> visit=[&](int z,int x,int y,const Tile *ancestor){
        auto found=m_tiles.constFind(key(z,x,y));if(found!=m_tiles.cend())ancestor=&found.value();
        if(branches.contains(key(z,x,y))){for(int dy=0;dy<2;++dy)for(int dx=0;dx<2;++dx)visit(z+1,x*2+dx,y*2+dy,ancestor);return;}
        if(!ancestor)return;
        const auto geometryKey=key(z,x,y)+QStringLiteral(":")+key(ancestor->z,ancestor->x,ancestor->y)+QStringLiteral(":")+ancestor->imageKey;
        auto *geometry=m_geometry.object(geometryKey);
        if(!geometry){geometry=new QVariantMap(patch(z,x,y,ancestor->z,ancestor->x,ancestor->y,ancestor->imageKey));const auto cost=(geometry->value(QStringLiteral("vertices")).toByteArray().size()+geometry->value(QStringLiteral("triangleIndices")).toByteArray().size())/1024+1;m_geometry.insert(geometryKey,geometry,int(cost));}
        result.push_back(*geometry);
    };
    visit(0,0,0,nullptr);return result;
}

QStringList GlobeTiles::knownKeys() const {QStringList result;for(const auto &tile:m_tiles)if(!tile.wireKey.isEmpty())result.push_back(tile.wireKey);return result;}
bool GlobeTiles::contains(int z,int x,int y) const {return m_tiles.contains(key(z,x,y));}

bool GlobeTiles::setDesired(const QVariantList &tiles) {const auto bounded=tiles.mid(0,96);if(bounded==m_desired)return false;m_desired=bounded;return true;}
