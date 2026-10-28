#pragma once
#include <QVariantMap>
#include <QVariantList>

// A selector never falls through to another capture. An absent selector follows
// the latest job; a missing selected job means that no capture is available.
inline QVariantMap selectedExplorationJob(const QVariantMap &projection) {
    const auto jobs=projection.value(QStringLiteral("jobs")).toList();
    if(!projection.contains(QStringLiteral("selected_job_id")))
        return jobs.isEmpty()?QVariantMap{}:jobs.last().toMap();
    const auto id=projection.value(QStringLiteral("selected_job_id")).toString();
    if(id.isEmpty())return {};
    for(const auto &entry:jobs){const auto job=entry.toMap();if(job.value(QStringLiteral("id")).toString()==id)return job;}
    return {};
}
