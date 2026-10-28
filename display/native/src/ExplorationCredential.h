#pragma once
#include <QByteArray>
#include <QString>
// Read-only World projection credential. Never reads the command descriptor.
QString explorationRuntimeDirectory();
QByteArray explorationProjectionToken(const QString &runtimeDirectory);

#include <QVariantMap>
// Navigation has its own short-lived owner capability; never reads control.json.
QVariantMap worldNavigationSession(const QString &runtimeDirectory);
