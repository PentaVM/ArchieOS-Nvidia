#pragma once
#include <QVariantMap>
#include <QVariantList>
#include <QHash>
#include <QImage>
struct WorldVisualData {
    QVariantList imageryMeshes,imagerySources,points,signalSources;
    QHash<QString,QImage> images;
};
WorldVisualData buildWorldVisuals(const QVariantMap &projection);
