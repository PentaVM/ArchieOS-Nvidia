#pragma once
#include "GlobeTiles.h"
#include "EarthMeshes.h"
#include <QObject>
#include <QAbstractListModel>
#include <QTimer>
#include <QVector3D>
#include <QJsonObject>
#include <QSet>
class QTcpSocket;
class WorldImageProvider;
// Stable rows preserve GPU texture/geometry objects while sibling tiles arrive.
class GlobeTileModel final : public QAbstractListModel {
    Q_OBJECT
public:
    using QAbstractListModel::QAbstractListModel;
    int rowCount(const QModelIndex &parent={}) const override {return parent.isValid()?0:m_rows.size();}
    QVariant data(const QModelIndex &index,int role) const override {return index.isValid()&&index.row()<m_rows.size()&&role==Qt::UserRole ? m_rows[index.row()] : QVariant{};}
    QHash<int,QByteArray> roleNames() const override {return {{Qt::UserRole,"meshData"}};}
    void update(const QVariantList &meshes);
private:
    QVariantList m_rows;
};
// Owner-preauthorized, bounded camera navigation only. Saved captures remain in
// ArchieBridge; neither bridge obtains Core commands or opens its own session.
class GlobeStreamBridge final : public QObject {
    Q_OBJECT
    Q_PROPERTY(QString sourceId READ sourceId WRITE setSourceId NOTIFY sourceChanged)
    Q_PROPERTY(QVariantList sources READ sources NOTIFY sourceChanged)
    Q_PROPERTY(QVariantList meshes READ meshes NOTIFY meshesChanged)
    Q_PROPERTY(QAbstractItemModel* tileModel READ tileModel CONSTANT)
    Q_PROPERTY(QAbstractItemModel* earthModel READ earthModel CONSTANT)
    Q_PROPERTY(bool proprietaryActive READ proprietaryActive NOTIFY meshesChanged)
    Q_PROPERTY(QVariantMap status READ status NOTIFY statusChanged)
    Q_PROPERTY(QVariantList worldPoints READ worldPoints NOTIFY statusChanged)
public:
    explicit GlobeStreamBridge(const QString &runtimeDirectory={},QObject *parent=nullptr);
    ~GlobeStreamBridge() override;
    QString sourceId() const {return m_source;}
    void setSourceId(const QString &source);
    QVariantList sources() const {return m_sources;}
    QVariantList meshes() const {return m_meshes;}
    QAbstractItemModel *tileModel(){return &m_model;}
    QAbstractItemModel *earthModel(){return &m_earthModel;}
    bool proprietaryActive() const {return m_earthModel.rowCount()>0;}
    QVariantMap status() const {return m_status;}
    QVariantList worldPoints() const {return m_points;}
    void setImageProvider(WorldImageProvider *provider){m_images=provider;}
    Q_INVOKABLE void setViewport(QVector3D eye,QVector3D target,int width,int height,double fov);
Q_SIGNALS:
    void sourceChanged();void meshesChanged();void statusChanged();
private:
    void tick();
    void request(const QVariantMap &session,bool viewport);
    void accept(const QJsonObject &reply,quint64 generation,const QString &source,const QString &session);
    void error(const QString &message,const QString &state=QStringLiteral("unavailable"));
    void clearScene();
    void publishImages();
    QString m_runtime,m_source=QStringLiteral("blue_marble"),m_session;
    QVariantList m_sources,m_meshes,m_points;
    QVariantMap m_status;
    QJsonObject m_viewport;
    GlobeTiles m_tiles;
    GlobeTileModel m_model;
    EarthMeshes m_earth;
    GlobeTileModel m_earthModel;
    QTimer m_timer;
    QSet<QTcpSocket*> m_requests;
    quint64 m_generation=0;
    bool m_dirty=false,m_poll=false;
    qint64 m_lastPoll=0;
    WorldImageProvider *m_images=nullptr;
};
