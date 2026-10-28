#include "GlobeStreamBridge.h"
#include "WorldImageProvider.h"
#include <QGuiApplication>
#include <QTcpServer>
#include <QTcpSocket>
#include <QTemporaryDir>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonArray>
#include <QDateTime>
#include <QBuffer>
#include <QTest>
#include <QSignalSpy>
#include <cstdlib>
void check(bool ok,const char *why){if(!ok){qCritical("%s",why);std::exit(1);}}
int main(int argc,char **argv){QGuiApplication app(argc,argv);QTemporaryDir runtime;QTcpServer server;check(server.listen(QHostAddress::LocalHost),"loopback fixture listens");
 const auto descriptorPath=runtime.filePath("navigation.json");QFile descriptor(descriptorPath);check(descriptor.open(QIODevice::WriteOnly),"session fixture created");
 auto object=QJsonObject{{"schema_version",1},{"address",QString("127.0.0.1:%1").arg(server.serverPort())},{"token",QString(64,'a')},{"session_id",QString(32,'b')},{"domain","personal"},{"expires_at_ms",QDateTime::currentMSecsSinceEpoch()+60000}};
 descriptor.write(QJsonDocument(object).toJson(QJsonDocument::Compact));descriptor.close();descriptor.setPermissions(QFileDevice::ReadOwner|QFileDevice::WriteOwner);
 QImage image(256,256,QImage::Format_RGB32);image.fill(Qt::green);QByteArray png;QBuffer encoded(&png);encoded.open(QIODevice::WriteOnly);image.save(&encoded,"PNG");
 const auto tile=QJsonObject{{"key","blue_marble:0/0/0"},{"zoom",0},{"x",0},{"y",0},{"mime","image/png"},{"width",256},{"height",256},{"data_base64",QString::fromLatin1(png.toBase64())}};
 QByteArray jpeg;QBuffer jpegBuffer(&jpeg);jpegBuffer.open(QIODevice::WriteOnly);image.save(&jpegBuffer,"JPEG");
 const float earthVertices[]={0,0,0,0,0,1,0,0, .001F,0,0,0,0,1,1,0, 0,.001F,.0001F,0,0,1,0,1};const quint32 earthIndices[]={0,1,2};
 const auto earthMesh=QJsonObject{{"key","earth:0123:0:0:0"},{"origin",QJsonArray{0,0,20}},{"vertex_count",3},{"triangle_count",1},{"vertices_base64",QString::fromLatin1(QByteArray(reinterpret_cast<const char*>(earthVertices),sizeof(earthVertices)).toBase64())},{"indices_base64",QString::fromLatin1(QByteArray(reinterpret_cast<const char*>(earthIndices),sizeof(earthIndices)).toBase64())},{"texture_base64",QString::fromLatin1(jpeg.toBase64())}};
 QList<QJsonObject> requests;QList<QPointer<QTcpSocket>> held;bool delay=false;
 QObject::connect(&server,&QTcpServer::newConnection,&app,[&]{while(server.hasPendingConnections()){auto *socket=server.nextPendingConnection();auto bytes=std::make_shared<QByteArray>();QObject::connect(socket,&QTcpSocket::readyRead,socket,[&,socket,bytes]{*bytes+=socket->readAll();if(!bytes->contains('\n'))return;const auto request=QJsonDocument::fromJson(bytes->trimmed()).object();requests.append(request);const auto command=request["command"].toObject();if(delay){held.append(socket);return;}auto result=QJsonObject{{"session_id",QString(32,'b')},{"generation",command["generation"]},{"source_id","blue_marble"},{"state","ready"},{"tiles",QJsonArray{tile}},{"desired_tiles",QJsonArray{QJsonObject{{"zoom",0},{"x",0},{"y",0}}}},{"pending_tiles",0},{"min_zoom",0},{"max_zoom",0},{"source",QJsonObject{{"source_id","blue_marble"},{"label","NASA Blue Marble"},{"mode","open_data"}}}};if(command["source"].toString()=="google_satellite"){result["source_id"]="google_satellite";result["earth"]=QJsonObject{{"state","ready"},{"active_keys",QJsonArray{"earth:0123:0:0:0"}}};result["meshes"]=QJsonArray{earthMesh};}socket->write(QJsonDocument(QJsonObject{{"ok",true},{"result",result}}).toJson(QJsonDocument::Compact)+'\n');socket->disconnectFromHost();});}});
 WorldImageProvider provider;GlobeStreamBridge bridge(runtime.path());bridge.setImageProvider(&provider);
 check(bridge.property("earthModel").isValid(),"photogrammetry has a dedicated stable mesh model");
 bool imagePublishedFirst=false;
 QObject::connect(bridge.tileModel(),&QAbstractItemModel::rowsInserted,&app,[&]{const auto row=bridge.tileModel()->data(bridge.tileModel()->index(0,0),Qt::UserRole).toMap();const auto url=row.value("textureSource").toString();QSize size;imagePublishedFirst=!provider.requestImage(url.mid(QString("image://globe/").size()),&size,{}).isNull();});
 bridge.setViewport({0,0,44},{0,0,0},1200,800,45);
 QTest::qWait(700);check(requests.size()==1,"settled viewport sends once and does not poll when ready");check(bridge.meshes().size()==1,"actual TCP delta decodes into native tile mesh");
 check(imagePublishedFirst,"provider pixels are published before QML creates image-bearing model rows");
 const auto first=requests.first();check(first["token"].toString()==QString(64,'a')&&first["session_id"].toString()==QString(32,'b'),"dedicated navigation capability authenticates session");
 const auto eye=first["command"].toObject()["viewport"].toObject()["eye"].toArray();check(std::abs(eye[2].toDouble()-44.0*6371008.8/20)<0.01,"viewport coordinates converted to Earth-centred metres");
 QSignalSpy inserted(bridge.tileModel(),&QAbstractItemModel::rowsInserted);QSignalSpy removed(bridge.tileModel(),&QAbstractItemModel::rowsRemoved);
 for(int i=0;i<100;++i)bridge.setViewport({float(i)*0.01F,0,44},{0,0,0},1200,800,45);
 QTest::qWait(650);check(requests.size()==2,"camera burst coalesces to a single subsequent viewport");
 check(inserted.isEmpty()&&removed.isEmpty(),"unchanged cached tiles preserve native GPU model rows");
 check(requests.last()["command"].toObject()["known_keys"].toArray().contains("blue_marble:0/0/0"),"resident tile keys suppress repeated bytes");
 {
  WorldImageProvider earthProvider;GlobeStreamBridge earthBridge(runtime.path());earthBridge.setImageProvider(&earthProvider);earthBridge.setSourceId("google_satellite");
  bool earthPublishedFirst=false;QObject::connect(earthBridge.earthModel(),&QAbstractItemModel::rowsInserted,&app,[&]{auto row=earthBridge.earthModel()->data(earthBridge.earthModel()->index(0,0),Qt::UserRole).toMap();QSize size;earthPublishedFirst=!earthProvider.requestImage(row["textureSource"].toString().mid(QString("image://globe/").size()),&size,{}).isNull();});
  earthBridge.setViewport({0,0,20.001F},{0,0,20},1200,800,45);QTest::qWait(650);
  check(earthBridge.earthModel()->rowCount()==1&&earthBridge.proprietaryActive(),"private mesh delta activates indexed photogrammetry model and capture guard");
  check(earthPublishedFirst,"original mesh JPEG publishes before native QML rows");
  check(earthBridge.status()["earth_triangle_count"].toInt()==1&&!earthBridge.status().contains("meshes"),"status retains counts without duplicating private base64 payloads");
  QSignalSpy changedRows(earthBridge.earthModel(),&QAbstractItemModel::rowsInserted);earthBridge.setViewport({0,0,20.002F},{0,0,20},1200,800,45);QTest::qWait(650);
  check(changedRows.isEmpty(),"unchanged mesh retains native GPU rows");check(requests.last()["command"].toObject()["known_mesh_keys"].toArray().contains("earth:0123:0:0:0"),"resident 3D mesh keys suppress repeated payloads");
  const auto privateUrl=earthBridge.earthModel()->data(earthBridge.earthModel()->index(0,0),Qt::UserRole).toMap()["textureSource"].toString().mid(QString("image://globe/").size());earthBridge.setSourceId("blue_marble");QSize cleared;check(!earthBridge.proprietaryActive()&&earthBridge.earthModel()->rowCount()==0&&earthProvider.requestImage(privateUrl,&cleared,{}).isNull(),"NASA switch clears private mesh rows and image namespace");
 }
 delay=true;for(int i=0;i<8;++i){bridge.setViewport({float(i)+1,0,44},{0,0,0},1200,800,45);QTest::qWait(270);}check(held.size()==2,"at most two asynchronous requests can remain in flight");
 const auto stale=QJsonObject{{"session_id",QString(32,'b')},{"generation",0},{"source_id","google_satellite"},{"state","ready"},{"pending_tiles",0}};
 for(auto socket:held)if(socket){socket->write(QJsonDocument(QJsonObject{{"ok",true},{"result",stale}}).toJson(QJsonDocument::Compact)+'\n');socket->disconnectFromHost();}
 delay=false;QTest::qWait(650);check(bridge.sourceId()=="blue_marble"&&bridge.status()["source_id"].toString()=="blue_marble","stale camera/source replies cannot replace current view");
 const double oldGeneration=requests.last()["command"].toObject()["generation"].toDouble();
 GlobeStreamBridge reopened(runtime.path());reopened.setViewport({0,0,44},{0,0,0},1200,800,45);QTest::qWait(600);
 check(requests.last()["command"].toObject()["generation"].toDouble()>oldGeneration,"reopened viewer generations supersede the previous process");
 QFile::setPermissions(descriptorPath,QFileDevice::ReadOwner|QFileDevice::WriteOwner|QFileDevice::ReadGroup);
 const int before=requests.size();bridge.setViewport({10,0,44},{0,0,0},1200,800,45);QTest::qWait(600);check(requests.size()==before,"group-readable capability cannot dispatch navigation");
 qInfo("Globe stream TCP/coalescing/capability contracts passed");}
