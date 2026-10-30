#pragma once
#include <QVariantList>
#include <QVariantMap>
// Bounded, read-only decoded data from displayd. Never opens artifact paths.
QVariantList explorationPreviewMeshes(const QVariantMap &projection);
