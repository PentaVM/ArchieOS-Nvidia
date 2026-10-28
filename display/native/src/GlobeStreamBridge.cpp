#include "GlobeStreamBridge.h"
#include "ExplorationCredential.h"
#include "WorldImageProvider.h"
#include <QTcpSocket>
#include <QJsonDocument>
#include <QJsonArray>
#include <QDateTime>
#include <cmath>
#include <algorithm>
void GlobeTileModel::update(const QVariantList &meshes){
    QHash<QString,QVariant> wanted;for(const auto &mesh:meshes)wanted.insert(mesh.toMap().value(QStringLiteral("tile_id")).toString(),mesh);
    for(int i=m_rows.size()-1;i>=0;--i)if(!wanted.contains(m_rows[i].toMap().value(QStringLiteral("tile_id")).toString())){beginRemoveRows({},i,i);m_rows.removeAt(i);endRemoveRows();}
    for(int i=0;i<m_rows.size();++i){const auto id=m_rows[i].toMap().value(QStringLiteral("tile_id")).toString();const auto next=wanted.take(id);if(next!=m_rows[i]){m_rows[i]=next;emit dataChanged(index(i),index(i),{Qt::UserRole});}}
    for(const auto &mesh:meshes){const auto id=mesh.toMap().value(QStringLiteral("tile_id")).toString();if(!wanted.remove(id))continue;const int row=m_rows.size();beginInsertRows({},row,row);m_rows.push_back(mesh);endInsertRows();}
}
GlobeStreamBridge::GlobeStreamBridge(const QString &runtimeDirectory,QObject *parent):QObject(parent),m_runtime(runtimeDirectory.isEmpty()?explorationRuntimeDirectory():runtimeDirectory){
    // A reopened viewer must supersede generations sent by its previous process.
    m_generation=quint64(QDateTime::currentMSecsSinceEpoch())*1000;
    const std::pair<const char*,const char*> catalog[]={{"blue_marble","NASA Blue Marble"},{"google_satellite","Google Satellite · experimental"},{"sentinel2","Sentinel-2"},{"landsat","Landsat"},{"natural_color","NASA Natural Color"},{"infrared","NASA Infrared"},{"night_lights","NASA Black Marble"},{"thermal","NASA Thermal"}};
    for(const auto &[id,label]:catalog)m_sources.push_back(QVariantMap{{QStringLiteral("source_id"),QString::fromUtf8(id)},{QStringLiteral("label"),QString::fromUtf8(label)},{QStringLiteral("mode"),QString::fromLatin1(id)==QStringLiteral("google_satellite")?QStringLiteral("online_preview_only"):QStringLiteral("open_data")}});
    m_timer.setInterval(250);m_timer.setTimerType(Qt::CoarseTimer);connect(&m_timer,&QTimer::timeout,this,&GlobeStreamBridge::tick);m_timer.start();
    error(QStringLiteral("La voz o la CLI deben autorizar una sesión de navegación"),QStringLiteral("authorization_required"));
}
GlobeStreamBridge::~GlobeStreamBridge(){for(auto *socket:m_requests){socket->disconnect();socket->abort();}}
void GlobeStreamBridge::error(const QString &message,const QString &state){auto next=m_status;next[QStringLiteral("state")]=state;next[QStringLiteral("error")]=message;if(next!=m_status){m_status=next;emit statusChanged();}}
void GlobeStreamBridge::publishImages(){if(m_images){auto images=m_tiles.images();const auto earth=m_earth.images();for(auto it=earth.cbegin();it!=earth.cend();++it)images.insert(it.key(),it.value());m_images->replace(images);}}
void GlobeStreamBridge::clearScene(){m_tiles.ingest(QString{},{});m_earth.clear();m_meshes.clear();m_model.update({});m_earthModel.update({});if(m_images)m_images->replace({});m_points.clear();m_status.clear();emit meshesChanged();emit statusChanged();}
void GlobeStreamBridge::setSourceId(const QString &source){
    if(source==m_source)return;bool found=false;for(const auto &entry:m_sources)if(entry.toMap().value(QStringLiteral("source_id"))==source)found=true;if(!found)return;
    m_source=source;++m_generation;m_dirty=true;m_poll=false;clearScene();emit sourceChanged();
}
void GlobeStreamBridge::setViewport(QVector3D eye,QVector3D target,int width,int height,double fov){
    if(width<16||height<16||width>8192||height>8192||!std::isfinite(fov)||fov<10||fov>120)return;
    constexpr double units=6371008.8/20.0;
    auto vector=[&](QVector3D v){return QJsonArray{double(v.x())*units,double(v.y())*units,double(v.z())*units};};
    for(double value:{double(eye.x()),double(eye.y()),double(eye.z()),double(target.x()),double(target.y()),double(target.z())})if(!std::isfinite(value))return;
    const QJsonObject next{{QStringLiteral("eye"),vector(eye)},{QStringLiteral("target"),vector(target)},{QStringLiteral("width"),width},{QStringLiteral("height"),height},{QStringLiteral("fov_degrees"),fov}};
    if(next==m_viewport)return;m_viewport=next;++m_generation;m_dirty=true;
}
void GlobeStreamBridge::tick(){
    if(m_viewport.isEmpty()||m_requests.size()>=2)return;
    const auto session=worldNavigationSession(m_runtime);
    if(session.isEmpty()){if(!m_session.isEmpty()){m_session.clear();++m_generation;clearScene();}error(QStringLiteral("Sesión de navegación ausente, vencida o sin permiso privado"),QStringLiteral("authorization_required"));return;}
    const auto id=session.value(QStringLiteral("session_id")).toString();if(id!=m_session){if(!m_session.isEmpty()){++m_generation;clearScene();}m_session=id;m_dirty=true;m_poll=false;}
    if(m_dirty){m_dirty=false;request(session,true);}else if((m_poll||QDateTime::currentMSecsSinceEpoch()-m_lastPoll>=5000)&&m_requests.isEmpty())request(session,false);
}
void GlobeStreamBridge::request(const QVariantMap &session,bool viewport){
    m_lastPoll=QDateTime::currentMSecsSinceEpoch();
    const auto generation=m_generation;const auto source=m_source;const auto sessionId=m_session;
    QJsonObject command{{QStringLiteral("action"),viewport?QStringLiteral("viewport"):QStringLiteral("status")},{QStringLiteral("known_keys"),QJsonArray::fromStringList(m_tiles.knownKeys())},{QStringLiteral("known_mesh_keys"),QJsonArray::fromStringList(m_earth.knownKeys())}};
    if(viewport){command[QStringLiteral("generation")]=qint64(generation);command[QStringLiteral("source")]=source;command[QStringLiteral("viewport")]=m_viewport;}
    const auto envelope=QJsonDocument(QJsonObject{{QStringLiteral("token"),session.value(QStringLiteral("token")).toString()},{QStringLiteral("session_id"),sessionId},{QStringLiteral("command"),command}}).toJson(QJsonDocument::Compact)+'\n';
    if(envelope.size()>32*1024){error(QStringLiteral("Navigation request exceeds its bound"));return;}
    auto *socket=new QTcpSocket(this);m_requests.insert(socket);auto bytes=std::make_shared<QByteArray>();auto completed=std::make_shared<bool>(false);
    auto finish=[this,socket,completed]{if(*completed)return;*completed=true;m_requests.remove(socket);socket->abort();socket->deleteLater();};
    connect(socket,&QTcpSocket::connected,socket,[socket,envelope]{socket->write(envelope);});
    connect(socket,&QTcpSocket::readyRead,socket,[this,socket,bytes,completed,finish,generation,source,sessionId]{
        *bytes+=socket->readAll();if(bytes->size()>8*1024*1024){error(QStringLiteral("Navigation response exceeds its bound"));finish();return;}
        if(!bytes->contains('\n'))return;const auto document=QJsonDocument::fromJson(bytes->left(bytes->indexOf('\n')));
        if(generation==m_generation&&source==m_source&&sessionId==m_session){if(document.isObject())accept(document.object(),generation,source,sessionId);else error(QStringLiteral("Invalid navigation JSON"));}
        finish();
    });
    connect(socket,&QTcpSocket::errorOccurred,socket,[this,finish,generation](QAbstractSocket::SocketError){if(generation==m_generation)error(QStringLiteral("Navigation daemon unavailable"));finish();});
    QTimer::singleShot(5000,socket,[this,finish,completed,generation]{if(*completed)return;if(generation==m_generation)error(QStringLiteral("Navigation response timed out"));finish();});
    const auto address=session.value(QStringLiteral("address")).toString();socket->connectToHost(QStringLiteral("127.0.0.1"),quint16(address.section(QLatin1Char(':'),1).toUInt()));
}
void GlobeStreamBridge::accept(const QJsonObject &reply,quint64 generation,const QString &source,const QString &session){
    if(reply.value(QStringLiteral("ok"))!=true){m_poll=false;error(reply.value(QStringLiteral("error")).toString(QStringLiteral("Navigation refused")));return;}
    const auto result=reply.value(QStringLiteral("result")).toObject();
    if(result.value(QStringLiteral("session_id")).toString()!=session||result.value(QStringLiteral("generation")).toDouble()!=double(generation)||result.value(QStringLiteral("source_id")).toString()!=source)return;
    bool changed=m_tiles.ingest(source,result.value(QStringLiteral("tiles")).toArray().toVariantList());
    changed=m_tiles.setDesired(result.value(QStringLiteral("desired_tiles")).toArray().toVariantList())||changed;
    const auto earth=result.value(QStringLiteral("earth")).toObject();QStringList active;
    for(const auto key:earth.value(QStringLiteral("active_keys")).toArray()){if(active.size()>=64)break;active.append(key.toString());}
    const bool earthChanged=m_earth.ingest(source==QStringLiteral("google_satellite")?result.value(QStringLiteral("meshes")).toArray().toVariantList():QVariantList{},source==QStringLiteral("google_satellite")?active:QStringList{});
    // Provider publication must precede rowsInserted/dataChanged: QML's Image
    // requests pixels synchronously when a new delegate is created.
    if(changed||earthChanged){publishImages();if(changed){m_meshes=m_tiles.meshes();m_model.update(m_meshes);}if(earthChanged)m_earthModel.update(m_earth.meshes());emit meshesChanged();}
    const auto metadata=result.value(QStringLiteral("source")).toObject().toVariantMap();
    if(!metadata.isEmpty())for(int i=0;i<m_sources.size();++i)if(m_sources[i].toMap().value(QStringLiteral("source_id"))==source){if(m_sources[i]!=metadata){m_sources[i]=metadata;emit sourceChanged();}break;}
    auto next=result.toVariantMap();next.remove(QStringLiteral("tiles"));next.remove(QStringLiteral("meshes"));int ready=0;const auto desired=result.value(QStringLiteral("desired_tiles")).toArray();
    for(const auto entry:desired){const auto tile=entry.toObject();if(m_tiles.contains(tile.value(QStringLiteral("zoom")).toInt(),tile.value(QStringLiteral("x")).toInt(),tile.value(QStringLiteral("y")).toInt()))++ready;}
    next[QStringLiteral("desired_count")]=desired.size();next[QStringLiteral("loaded_count")]=ready;next[QStringLiteral("fallback_count")]=std::max(0,m_tiles.count()-ready);next[QStringLiteral("pending_count")]=result.value(QStringLiteral("pending_tiles")).toInt();
    next[QStringLiteral("earth_loaded_count")]=m_earthModel.rowCount();next[QStringLiteral("earth_cached_count")]=m_earth.count();next[QStringLiteral("earth_memory_bytes")]=m_earth.bytes();
    qint64 triangles=0;for(const auto &mesh:m_earth.meshes())triangles+=mesh.toMap().value(QStringLiteral("triangle_count")).toLongLong();next[QStringLiteral("earth_triangle_count")]=triangles;
    QStringList errors;for(auto entry:result.value(QStringLiteral("errors")).toArray())errors.push_back(entry.toString());if(!m_tiles.error().isEmpty())errors.push_back(m_tiles.error());if(!m_earth.error().isEmpty())errors.push_back(m_earth.error());if(!earth.value(QStringLiteral("error")).toString().isEmpty())errors.push_back(earth.value(QStringLiteral("error")).toString().left(4096));next[QStringLiteral("error")]=errors.join(QStringLiteral(" · "));
    QVariantList points;
    constexpr double radians=3.14159265358979323846/180.0;
    for(const auto &entry:result.value(QStringLiteral("points")).toArray()){
        if(points.size()>=128)break;auto point=entry.toObject().toVariantMap();
        const auto latitude=point.value(QStringLiteral("latitude")),longitude=point.value(QStringLiteral("longitude"));
        if(!latitude.isValid()||!longitude.isValid())continue;
        const double lat=latitude.toDouble(),lon=longitude.toDouble();
        if(!std::isfinite(lat)||!std::isfinite(lon)||std::abs(lat)>90||std::abs(lon)>180)continue;
        const auto altitude=point.value(QStringLiteral("altitude_m"));
        const bool known=!altitude.isNull()&&std::isfinite(altitude.toDouble());
        const double height=known&&point.value(QStringLiteral("kind"))==QStringLiteral("aircraft")?std::clamp(altitude.toDouble(),-1000.0,40000.0):0;
        const double radius=20.0+(height+1.0)*20.0/6371008.8;
        point[QStringLiteral("position")]=QVariant::fromValue(QVector3D(float(radius*std::cos(lat*radians)*std::sin(lon*radians)),float(radius*std::sin(lat*radians)),float(radius*std::cos(lat*radians)*std::cos(lon*radians))));
        point[QStringLiteral("altitudeKnown")]=known;
        point[QStringLiteral("anchor_mode")]=height!=0?QStringLiteral("reported_barometric_altitude"):QStringLiteral("map_reference_unknown_elevation");
        points.push_back(point);
    }
    m_points=points;
    m_poll=result.value(QStringLiteral("state"))==QStringLiteral("loading")||result.value(QStringLiteral("pending_tiles")).toInt()>0||result.value(QStringLiteral("more_tiles")).toBool()||earth.value(QStringLiteral("state"))==QStringLiteral("loading")||earth.value(QStringLiteral("loading")).toBool()||earth.value(QStringLiteral("more_meshes")).toBool();
    if(next!=m_status){m_status=next;emit statusChanged();}
}
