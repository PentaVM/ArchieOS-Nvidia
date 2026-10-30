#include "EarthMeshes.h"
#include <QBuffer>
#include <QCryptographicHash>
#include <QImageReader>
#include <QSet>
#include <QVector3D>
#include <QtEndian>
#include <cmath>
#include <cstring>

namespace {
constexpr qsizetype maxBytes=128*1024*1024;
constexpr int maxVertices=131072,maxTriangles=262144;
bool keyValid(const QString &key){return !key.isEmpty()&&key.size()<=256&&!key.contains(QLatin1Char('/'))&&!key.contains(QLatin1Char('\\'));}
QByteArray decode(const QVariant &value,qsizetype maximum){
    const auto string=value.toString();if(string.size()>((maximum+2)/3)*4)return {};
    const auto decoded=QByteArray::fromBase64Encoding(string.toLatin1(),QByteArray::AbortOnBase64DecodingErrors);
    return decoded&&decoded.decoded.size()<=maximum?decoded.decoded:QByteArray{};
}
bool dimension(int n){return n>0&&n<=4096;}
}
void EarthMeshes::clear(){m_cache.clear();m_active.clear();m_bytes=0;m_error.clear();}
bool EarthMeshes::ingest(const QVariantList &deltas,const QStringList &activeKeys){
    const auto old=meshes();m_error.clear();m_active.clear();
    for(const auto &key:activeKeys){if(m_active.size()>=64)break;if(keyValid(key)&&!m_active.contains(key))m_active.append(key);}
    const auto trim=[&]{
        while(m_cache.size()>64||m_bytes>maxBytes){
            auto victim=m_cache.end();for(auto it=m_cache.begin();it!=m_cache.end();++it){const bool inactive=!m_active.contains(it.key());if(victim==m_cache.end()||(inactive&&m_active.contains(victim.key()))||(inactive==!m_active.contains(victim.key())&&it->used<victim->used))victim=it;}
            if(victim==m_cache.end())break;m_bytes-=victim->bytes;m_cache.erase(victim);
        }
    };
    for(const auto &entry:deltas.mid(0,64)){
        const auto wire=entry.toMap();const auto key=wire.value(QStringLiteral("key")).toString();
        if(!keyValid(key)||!m_active.contains(key))continue;
        // Mesh keys include epoch/mask/revision and are immutable within the
        // admitted session. Retransmission must not decode or upload them again.
        auto resident=m_cache.find(key);
        if(resident!=m_cache.end()){resident->used=++m_clock;continue;}
        auto fail=[&]{m_error=QStringLiteral("Rejected malformed or oversized 3D mesh");};
        bool vok=false,tok=false;
        const int vc=wire.value(QStringLiteral("vertex_count")).toInt(&vok),tc=wire.value(QStringLiteral("triangle_count")).toInt(&tok);
        if(!vok||!tok||vc<3||vc>maxVertices||tc<1||tc>maxTriangles){fail();continue;}
        const auto origin=wire.value(QStringLiteral("origin")).toList();
        if(origin.size()!=3){fail();continue;}
        bool valid=true;float xyz[3];for(int i=0;i<3;++i){bool ok=false;const double value=origin[i].toDouble(&ok);if(!ok||!std::isfinite(value)||std::abs(value)>22)valid=false;xyz[i]=float(value);}
        if(!valid){fail();continue;}
        auto vertices=decode(wire.value(QStringLiteral("vertices_base64")),qsizetype(vc)*32);
        auto indices=decode(wire.value(QStringLiteral("indices_base64")),qsizetype(tc)*12);
        auto jpeg=decode(wire.value(QStringLiteral("texture_base64")),2*1024*1024);
        if(vertices.size()!=qsizetype(vc)*32||indices.size()!=qsizetype(tc)*12||jpeg.isEmpty()){fail();continue;}
        // Wire buffers are explicitly little-endian. Validate every value and
        // index before Quick3D receives them; malformed geometry never reaches GPU.
        for(qsizetype offset=0;offset<vertices.size();offset+=4){quint32 bits=qFromLittleEndian<quint32>(vertices.constData()+offset);float value;std::memcpy(&value,&bits,4);if(!std::isfinite(value)||std::abs(value)>80){valid=false;break;}std::memcpy(vertices.data()+offset,&value,4);}
        for(qsizetype offset=0;valid&&offset<indices.size();offset+=4){quint32 index=qFromLittleEndian<quint32>(indices.constData()+offset);if(index>=quint32(vc)){valid=false;break;}std::memcpy(indices.data()+offset,&index,4);}
        if(!valid){fail();continue;}
        const auto imageKey=QStringLiteral("earth-")+QString::fromLatin1(QCryptographicHash::hash(jpeg,QCryptographicHash::Sha256).toHex());
        QImage image;
        for(const auto &cached:m_cache)if(cached.imageKey==imageKey){image=cached.image;break;}
        if(image.isNull()){
            QBuffer buffer(&jpeg);buffer.open(QIODevice::ReadOnly);
            QImageReader reader(&buffer,QByteArrayLiteral("jpeg"));reader.setAutoDetectImageFormat(false);reader.setDecideFormatFromContent(false);
            const auto size=reader.size();if(!dimension(size.width())||!dimension(size.height())){fail();continue;}
            image=reader.read();if(image.isNull()){fail();continue;}
        }
        QVariantMap mesh{{QStringLiteral("tile_id"),key},{QStringLiteral("vertices"),vertices},{QStringLiteral("triangleIndices"),indices},{QStringLiteral("origin"),QVariant::fromValue(QVector3D(xyz[0],xyz[1],xyz[2]))},{QStringLiteral("textureSource"),QStringLiteral("image://globe/")+imageKey},{QStringLiteral("vertex_count"),vc},{QStringLiteral("triangle_count"),tc},{QStringLiteral("attribution"),wire.value(QStringLiteral("attribution")).toString().left(2048)}};
        auto previous=m_cache.find(key);if(previous!=m_cache.end()){m_bytes-=previous->bytes;m_cache.erase(previous);}
        const auto cost=vertices.size()+indices.size()+image.sizeInBytes();
        m_cache.insert(key,Mesh{mesh,image,imageKey,cost,++m_clock});m_bytes+=cost;
        // Bound each decoded addition, not only the final frame: one delta
        // batch may contain many large original textures.
        trim();
    }
    for(const auto &key:m_active){auto it=m_cache.find(key);if(it!=m_cache.end())it->used=++m_clock;}
    return old!=meshes();
}
QVariantList EarthMeshes::meshes() const {QVariantList result;for(const auto &key:m_active){auto found=m_cache.constFind(key);if(found!=m_cache.cend())result.append(found->data);}return result;}
QHash<QString,QImage> EarthMeshes::images() const {QHash<QString,QImage> result;for(const auto &mesh:m_cache)result.insert(mesh.imageKey,mesh.image);return result;}
QStringList EarthMeshes::knownKeys() const {return m_cache.keys();}
