#pragma once
#include <QHash>
#include <QImage>
#include <QVariantList>

// Private online-viewing buffers only. No URLs, disk cache or export interface.
class EarthMeshes final {
public:
    bool ingest(const QVariantList &deltas,const QStringList &activeKeys);
    void clear();
    QVariantList meshes() const;
    QHash<QString,QImage> images() const;
    QStringList knownKeys() const;
    QString error() const {return m_error;}
    qsizetype bytes() const {return m_bytes;}
    int count() const {return m_cache.size();}
private:
    struct Mesh {QVariantMap data;QImage image;QString imageKey;qsizetype bytes;quint64 used;};
    QHash<QString,Mesh> m_cache;
    QStringList m_active;
    QString m_error;
    qsizetype m_bytes=0;
    quint64 m_clock=0;
};
