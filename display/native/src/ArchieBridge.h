#pragma once

#include <QByteArray>
#include <QNetworkAccessManager>
#include <QObject>
#include <QTimer>
#include <QUrl>
#include <QVariantList>
#include <QVariantMap>

#include "archie/visual/observatory.hpp"

class WorldImageProvider;

class ArchieBridge final : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QVariantMap snapshot READ snapshot NOTIFY snapshotChanged)
    Q_PROPERTY(QVariantMap world READ world NOTIFY worldChanged)
    Q_PROPERTY(QVariantMap exploration READ exploration NOTIFY explorationChanged)
    Q_PROPERTY(QVariantList explorationMeshes READ explorationMeshes NOTIFY explorationChanged)
    Q_PROPERTY(QVariantList imageryMeshes READ imageryMeshes NOTIFY explorationChanged)
    Q_PROPERTY(QVariantList imagerySources READ imagerySources NOTIFY explorationChanged)
    Q_PROPERTY(QVariantList worldPoints READ worldPoints NOTIFY explorationChanged)
    Q_PROPERTY(QVariantList signalSources READ signalSources NOTIFY explorationChanged)
    Q_PROPERTY(QString selectedImagery READ selectedImagery WRITE setSelectedImagery NOTIFY selectedImageryChanged)
    Q_PROPERTY(QString selectedExplorationJob READ selectedExplorationJobId NOTIFY explorationSelectionChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)
    Q_PROPERTY(bool connected READ connected NOTIFY connectedChanged)
    Q_PROPERTY(QString error READ error NOTIFY errorChanged)
    Q_PROPERTY(int snapshotRevision READ snapshotRevision NOTIFY snapshotRevisionChanged)
    Q_PROPERTY(int worldRevision READ worldRevision NOTIFY worldRevisionChanged)
    Q_PROPERTY(float earthAngle READ earthAngle NOTIFY earthAngleChanged)
    // Machine telemetry, read by displayd from /proc and /sys rather than from archied.
    // It stays true while the daemon is unreachable, which is exactly when it matters.
    Q_PROPERTY(QVariantMap kernel READ kernel NOTIFY kernelChanged)
    Q_PROPERTY(int kernelRevision READ kernelRevision NOTIFY kernelRevisionChanged)
    // What GeoFabric's provider selection would draw, at a handful of named scales.
    // Fetched once, not on a timer: the plan is a constant of the daemon's build, not a
    // reading of anything that changes while the process runs, so polling it again
    // would only ever return the same bytes.
    Q_PROPERTY(QVariantMap geoFabric READ geoFabric NOTIFY geoFabricChanged)

    // Real OSM building footprints, read once from whatever `.osm.pbf` extract is
    // configured (see loadBuildings). `buildings` is the honest status -- whether one is
    // configured, how many were found, how many could actually be drawn and why the rest
    // could not -- and `buildingMeshes` is the drawable geometry itself, already placed on
    // the globe in scene units, one entry per building the QML side turns into a
    // MeshGeometry. No demo or placeholder shape appears in either when nothing is
    // configured: the honest state is an empty list, not an invented building.
    Q_PROPERTY(QVariantMap buildings READ buildings NOTIFY buildingsChanged)
    Q_PROPERTY(QVariantList buildingMeshes READ buildingMeshes NOTIFY buildingsChanged)
    // The read-only TFOG frame as it arrived. The orb is drawn by LumenField from the
    // composed Lumenfield scene; this frame is what search looks through.
    Q_PROPERTY(QVariantMap orb READ orb NOTIFY orbChanged)
    Q_PROPERTY(int orbRevision READ orbRevision NOTIFY orbChanged)
    Q_PROPERTY(bool presented READ presented WRITE setPresented NOTIFY presentedChanged)

    // Everything below comes out of `archie::visual::Observatory` rather than from this
    // class. The bridge used to derive positions from a hash of the node name, colours
    // from a table of state strings, and scale from a magnitude — three separate
    // inventions that no test could see and that disagreed with the model the rest of
    // the display is specified against. Now the bridge fetches bytes and the model
    // decides, which is the only arrangement in which what is drawn can be checked.
    Q_PROPERTY(QVariantList sceneNodes READ sceneNodes NOTIFY frameChanged)
    Q_PROPERTY(QVariantList sceneEdges READ sceneEdges NOTIFY frameChanged)
    /// Tier, counts, mode and the ingest verdicts: what the frame says about itself.
    Q_PROPERTY(QVariantMap frame READ frame NOTIFY frameChanged)

public:
    explicit ArchieBridge(QObject *parent = nullptr, bool shellMode = false, quint16 projectionPort = 7666);
    void setWorldImageProvider(WorldImageProvider *provider) {m_imageProvider=provider;}
    bool presented() const { return m_presented; }
    void setPresented(bool presented);

    QVariantMap snapshot() const { return m_snapshot; }
    QVariantMap world() const { return m_world; }
    QVariantList imageryMeshes() const {return m_imageryMeshes;}
    QVariantList imagerySources() const {return m_imagerySources;}
    QVariantList worldPoints() const {return m_worldPoints;}
    QVariantList signalSources() const {return m_signalSources;}
    QString selectedImagery() const {return m_selectedImagery;}
    QString selectedExplorationJobId() const {return m_selectedExplorationJob;}
    void setSelectedImagery(const QString &id);
    QVariantMap exploration() const { return m_exploration; }
    QVariantList explorationMeshes() const { return m_explorationMeshes; }
    QVariantList sceneNodes() const { return m_sceneNodes; }
    QVariantList sceneEdges() const { return m_sceneEdges; }
    bool busy() const { return m_pendingRequests > 0; }
    bool connected() const { return m_connected; }
    QString error() const { return m_error; }
    int snapshotRevision() const { return m_snapshotRevision; }
    int worldRevision() const { return m_worldRevision; }
    float earthAngle() const { return m_earthAngle; }
    QVariantMap kernel() const { return m_kernel; }
    int kernelRevision() const { return m_kernelRevision; }
    QVariantMap frame() const { return m_frame; }
    QVariantMap geoFabric() const { return m_geoFabric; }
    QVariantMap buildings() const { return m_buildings; }
    QVariantList buildingMeshes() const { return m_buildingMeshes; }
    QVariantMap orb() const { return m_orb; }
    int orbRevision() const { return m_orbRevision; }

    Q_INVOKABLE void refreshSnapshot();
    Q_INVOKABLE void refreshWorld();
    Q_INVOKABLE void refreshExploration();
    Q_INVOKABLE bool selectExplorationJob(const QString &id);
    Q_INVOKABLE void latestExplorationJob();
    Q_INVOKABLE void enableWorld();
    Q_INVOKABLE bool frameExploration(double latitude, double longitude, double radiusM, double elevationM, bool animate);
    Q_INVOKABLE void refreshKernel();
    Q_INVOKABLE void refreshOrb();
    Q_INVOKABLE void refreshGeoFabric();
    /// Reads `ARCHIE_OSM_EXTRACT` and, if it names a readable `.osm.pbf` file, parses it
    /// and rebuilds `buildingMeshes`. Called once at construction, like refreshGeoFabric:
    /// the extract does not change while the process runs, so re-reading it on a timer
    /// would only ever repeat the same parse. Invokable so an operator who has just set
    /// the variable and does not want to restart the observatory can ask for it directly.
    ///
    /// The read, parse and mesh-building all happen on a background thread (a real
    /// Geofabrik extract's tens of thousands of ways are enough work to freeze a frame):
    /// this call returns immediately, `buildings.loading` becomes true, and
    /// `buildingsChanged` fires again once the result is ready.
    Q_INVOKABLE void loadBuildings();

    /// Operator input. The mode is the model's to hold, not the interface's: it owns the
    /// morph between scenes and re-homes the camera when it changes, and a QML toggle
    /// keeping its own copy would be a second answer to the same question.
    Q_INVOKABLE void requestMode(const QString &mode);
    Q_INVOKABLE void focusNode(const QString &id);
    Q_INVOKABLE void releaseFocus();
    Q_INVOKABLE void orbitGlobe(float longitudeDelta, float latitudeDelta, float zoomSteps);
    Q_INVOKABLE void tiltGlobe(float deltaDegrees);

signals:
    void snapshotChanged();
    void worldChanged();
    void explorationChanged();
    void selectedImageryChanged();
    void explorationSelectionChanged();
    void busyChanged();
    void connectedChanged();
    void errorChanged();
    void snapshotRevisionChanged();
    void worldRevisionChanged();
    void earthAngleChanged();
    void kernelChanged();
    void kernelRevisionChanged();
    void frameChanged();
    void geoFabricChanged();
    void buildingsChanged();
    void orbChanged();
    void presentedChanged();

private:
    enum class Projection { Snapshot, World, Kernel, GeoFabric, Orb, Exploration };

    void request(Projection projection, const QUrl &url);
    void accept(Projection projection, const QByteArray &body);
    void fail(const QString &message);
    void setConnected(bool value);
    void updateEarthAngle();

    /// Advance the model and publish what it says to draw.
    void tick();
    void clearExplorationVisuals();

    /// The model. Everything drawn comes out of this, and nothing about how to draw it
    /// is decided anywhere else in this process.
    archie::visual::Observatory m_observatory;

    QNetworkAccessManager m_network;
    QTimer m_snapshotTimer;
    QTimer m_worldTimer;
    QTimer m_explorationTimer;
    QTimer m_kernelTimer;
    QTimer m_orbTimer;
    QTimer m_clockTimer;
    /// Drives `tick`. Advancing is not the same as a payload arriving: freshness decays,
    /// entries and departures play out, and the camera moves, all between snapshots.
    QTimer m_frameTimer;
    QVariantMap m_snapshot;
    QVariantMap m_world;
    QVariantMap m_exploration{{QStringLiteral("source"), QStringLiteral("unavailable")}};
    QVariantList m_explorationMeshes,m_imageryMeshes,m_imagerySources,m_worldPoints,m_signalSources;
    QString m_selectedImagery,m_imageryPreference;
    QString m_selectedExplorationJob;
    quint64 m_explorationSelectionRevision=0;
    WorldImageProvider *m_imageProvider=nullptr;
    quint16 m_projectionPort=7666;
    QByteArray m_explorationGeometryDigest;
    bool m_explorationPending = false;
    QVariantMap m_kernel;
    QVariantList m_sceneNodes;
    QVariantList m_sceneEdges;
    QVariantMap m_frame;
    QVariantMap m_geoFabric;
    QVariantMap m_buildings;
    QVariantList m_buildingMeshes;
    QVariantMap m_orb;
    /// Bumped on every `loadBuildings` call and captured by the background task; a result
    /// that arrives for an older generation is discarded rather than applied, which is
    /// what keeps a stale parse from overwriting a fresher one if `loadBuildings` is ever
    /// invoked again (an operator retry) before the first run finishes.
    int m_buildingsGeneration = 0;
    QByteArray m_snapshotDigest;
    QByteArray m_worldDigest;
    QByteArray m_kernelDigest;
    QString m_error;
    int m_pendingRequests = 0;
    int m_snapshotRevision = 0;
    int m_worldRevision = 0;
    int m_kernelRevision = 0;
    int m_orbRevision = 0;
    bool m_orbRequestPending = false;
    bool m_connected = false;
    bool m_presented = true;
    qint64 m_kernelReceivedAt = 0;
    float m_earthAngle = 0.0F;
};
