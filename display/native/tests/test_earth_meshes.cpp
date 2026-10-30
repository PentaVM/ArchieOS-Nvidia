#include "EarthMeshes.h"
#include <QCoreApplication>
#include <QBuffer>
#include <QVector3D>
#include <QtEndian>
#include <cmath>
#include <cstring>
#include <cstdlib>
#include <limits>

void check(bool ok,const char *why){if(!ok){qCritical("%s",why);std::exit(1);}}
QVariantMap mesh(const QString &key,int size=256,int height=-1){
    QImage image(size,height<0?size:height,QImage::Format_RGB32);image.fill(Qt::red);QByteArray jpeg;QBuffer buffer(&jpeg);buffer.open(QIODevice::WriteOnly);image.save(&buffer,"JPEG");
    // Local height variation is preserved instead of flattening to a raster patch.
    const float values[]={0,0,0, 0,0,1, 0,0, .001F,0,0, 0,0,1, 1,0, 0,.001F,.0001F, 0,0,1, 0,1};
    QByteArray vertices;for(float value:values){quint32 bits;std::memcpy(&bits,&value,4);bits=qToLittleEndian(bits);vertices.append(reinterpret_cast<const char*>(&bits),4);}
    const quint32 indices[]={qToLittleEndian(quint32(0)),qToLittleEndian(quint32(1)),qToLittleEndian(quint32(2))};
    return {{QStringLiteral("key"),key},{QStringLiteral("vertex_count"),3},{QStringLiteral("triangle_count"),1},{QStringLiteral("origin"),QVariantList{0,0,20}},{QStringLiteral("vertices_base64"),QString::fromLatin1(vertices.toBase64())},{QStringLiteral("indices_base64"),QString::fromLatin1(QByteArray(reinterpret_cast<const char*>(indices),sizeof(indices)).toBase64())},{QStringLiteral("texture_base64"),QString::fromLatin1(jpeg.toBase64())}};
}
int main(int argc,char **argv){QCoreApplication app(argc,argv);EarthMeshes cache;const auto a=QStringLiteral("earth:123:0"),b=QStringLiteral("earth:1234:0");
    EarthMeshes rectangular;check(rectangular.ingest({mesh(a,128,256)},{a}),"original rectangular Earth JPEG is retained");
    check(rectangular.images().cbegin().value().size()==QSize(128,256),"no invented square texture restriction");
    check(cache.ingest({mesh(a)},{a}),"original JPEG and indexed 3D mesh accepted");
    check(cache.meshes().size()==1&&cache.images().size()==1,"one active geometry and private image published");
    auto data=cache.meshes().first().toMap();const auto vertices=data.value(QStringLiteral("vertices")).toByteArray();float height;std::memcpy(&height,vertices.constData()+18*4,4);
    check(std::abs(height-.0001F)<1e-9F&&data.value(QStringLiteral("origin")).value<QVector3D>().z()==20,"local building height and native Earth origin retained");
    check(cache.images().cbegin().value().size()==QSize(256,256),"original texture pixels retained");
    const auto decodedTexture=cache.images().cbegin().value().cacheKey();
    check(!cache.ingest({mesh(a)},{a}),"duplicate immutable mesh delta preserves the draw model");
    check(cache.images().cbegin().value().cacheKey()==decodedTexture,"duplicate delta must not JPEG-decode the same texture again");
    const auto revised=QStringLiteral("earth:123:new-mask");
    cache.ingest({mesh(revised)},{revised});
    check(cache.images().cbegin().value().cacheKey()==decodedTexture,"new geometry mask reuses its unchanged original JPEG");
    cache.clear();cache.ingest({mesh(a)},{a});
    check(!cache.ingest({},{a}),"unchanged delta preserves stable geometry data");
    check(cache.ingest({mesh(b,512)},{b}),"camera's active child replaces draw plan");
    check(cache.knownKeys().size()==2&&cache.meshes().size()==1,"cache history remains reusable but never becomes a draw list");
    check(cache.ingest({},{a}),"parent fallback can return without retransmitting bytes");
    const auto bad=QStringLiteral("earth:bad");auto invalid=mesh(bad);invalid[QStringLiteral("indices_base64")]=QString::fromLatin1(QByteArray(12,char(255)).toBase64());
    cache.ingest({invalid},{bad});check(cache.meshes().isEmpty()&&!cache.error().isEmpty(),"out-of-range indices rejected before GPU allocation");
    invalid=mesh(bad);auto bytes=QByteArray::fromBase64(invalid.value(QStringLiteral("vertices_base64")).toString().toLatin1());const quint32 nan=qToLittleEndian(quint32(0x7fc00000));std::memcpy(bytes.data(),&nan,4);invalid[QStringLiteral("vertices_base64")]=QString::fromLatin1(bytes.toBase64());
    cache.ingest({invalid},{bad});check(!cache.knownKeys().contains(bad),"non-finite vertices rejected");
    invalid=mesh(bad);invalid[QStringLiteral("vertex_count")]=131073;cache.ingest({invalid},{bad});check(!cache.knownKeys().contains(bad),"vertex count is bounded before base64 allocation");
    invalid=mesh(bad,64);cache.ingest({invalid},{bad});check(cache.knownKeys().contains(bad),"small original 3D JPEGs are supported");
    const auto huge=QStringLiteral("earth:huge");invalid=mesh(huge);auto jpeg=QByteArray::fromBase64(invalid.value(QStringLiteral("texture_base64")).toString().toLatin1());const auto sof=jpeg.indexOf(QByteArray::fromHex("ffc0"));check(sof>=0,"synthetic JPEG has baseline size header");jpeg[sof+5]=char(0x20);jpeg[sof+6]=char(0);invalid[QStringLiteral("texture_base64")]=QString::fromLatin1(jpeg.toBase64());cache.ingest({invalid},{huge});check(!cache.knownKeys().contains(huge),"oversized JPEG header rejected before image allocation");
    const auto badOrigin=QStringLiteral("earth:bad-origin");invalid=mesh(badOrigin);invalid[QStringLiteral("origin")]=QVariantList{0,0,999};cache.ingest({invalid},{badOrigin});check(!cache.knownKeys().contains(badOrigin),"invalid Earth origin rejected");
    for(int i=0;i<70;++i){const auto key=QStringLiteral("earth:cache:")+QString::number(i);cache.ingest({mesh(key)},{key});}
    check(cache.count()==64&&cache.bytes()<=128*1024*1024,"separate mesh RAM cache has hard entry and byte limits");
    cache.clear();QVariantList large;QStringList active;
    for(int i=0;i<10;++i){const auto key=QStringLiteral("earth:large:")+QString::number(i);large.append(mesh(key,2048));active.append(key);}
    cache.ingest(large,active);check(cache.count()==7&&cache.bytes()<=128*1024*1024,"decoded original 2048px textures trigger byte eviction within a delta batch");
    cache.clear();check(cache.count()==0&&cache.images().isEmpty()&&cache.meshes().isEmpty(),"private texture and geometry buffers clear together");
    qInfo("Earth JPEG/geometry/active-plan/bounds contracts passed");
}
