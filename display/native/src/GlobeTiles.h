#pragma once
#include <QVariantList>
#include <QHash>
#include <QCache>
#include <QImage>
#include <QString>
// The streaming scene is separate from saved AOI captures. Encoded bytes are
// validated once; images and immutable curved patches are reused across polls.
class GlobeTiles final {
public:
    bool ingest(const QString &source, const QVariantList &tiles);
    QVariantList meshes() const;
    bool setDesired(const QVariantList &tiles);
    QHash<QString,QImage> images() const;
    int count() const { return m_tiles.size(); }
    QStringList knownKeys() const;
    bool contains(int z,int x,int y) const;
    QString error() const { return m_error; }
private:
    struct Tile {int z,x,y;QString imageKey,wireKey;QImage image;quint64 used;};
    QString m_source,m_error;
    QVariantList m_desired;
    QHash<QString,Tile> m_tiles;
    mutable QCache<QString,QVariantMap> m_geometry{64*1024};
    quint64 m_clock=0;
};
