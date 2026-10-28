#pragma once
#include <QQuickImageProvider>
#include <QReadWriteLock>
#include <QHash>
class WorldImageProvider final : public QQuickImageProvider {
public:
    WorldImageProvider():QQuickImageProvider(QQuickImageProvider::Image){}
    void replace(const QHash<QString,QImage> &images){QWriteLocker lock(&m_lock);m_images=images;}
    QImage requestImage(const QString &id,QSize *size,const QSize &) override {QReadLocker lock(&m_lock);const auto image=m_images.value(id);if(size)*size=image.size();return image;}
private:
    QReadWriteLock m_lock;QHash<QString,QImage> m_images;
};
