#pragma once

// Design: Xavi. Native Rust/C++ implementation: PentaVM.
// Xavi's neural sphere (Lumenfield's "Esfera neuronal", API 2), as the host sees it.
//
// The model, the live build, the motion and the sparks are Rust (`archie-lumen-design`,
// `neural_*`). This object owns one opaque Rust sphere, lists the folder off the GUI thread,
// steps the sphere on the GUI thread and tells `NeuralGeometry` when its buffers changed.
// It reads the folder; it never writes and never opens a file.

#include <QHash>
#include <QMap>
#include <QObject>
#include <QPair>
#include <QQuaternion>
#include <QString>
#include <QStringList>
#include <QVariantList>
#include <QVariantMap>
#include <QVector3D>
#include <QtQml/qqmlregistration.h>

#include <vector>

#include "archie_lumen_design.h"

class NeuralField : public QObject
{
    Q_OBJECT
    QML_ELEMENT
    /// The folder drawn. Listed again when it changes; empty draws nothing.
    Q_PROPERTY(QString root READ root WRITE setRoot NOTIFY rootChanged)
    Q_PROPERTY(bool active READ active WRITE setActive NOTIFY activeChanged)
    Q_PROPERTY(bool loading READ loading NOTIFY sourceChanged)
    Q_PROPERTY(QString sourceRoot READ sourceRoot NOTIFY sourceChanged)
    Q_PROPERTY(QString scannedAt READ scannedAt NOTIFY sourceChanged)
    /// The original's deterministic frame (`?still`): no motion, no live build.
    Q_PROPERTY(bool still READ still WRITE setStill NOTIFY stillChanged)
    Q_PROPERTY(int limit READ limit WRITE setLimit NOTIFY rootChanged)
    Q_PROPERTY(int filesPerFolder READ filesPerFolder WRITE setFilesPerFolder NOTIFY rootChanged)
    Q_PROPERTY(bool ready READ ready NOTIFY updated)
    Q_PROPERTY(bool building READ building NOTIFY updated)
    Q_PROPERTY(QString error READ error NOTIFY errorChanged)
    Q_PROPERTY(double time READ time NOTIFY updated)
    /// Bumps whenever any buffer other than the sparks changed.
    Q_PROPERTY(qulonglong revision READ revision NOTIFY updated)
    /// The original's live settings, by its names.
    Q_PROPERTY(QVariantMap settings READ settings NOTIFY settingsChanged)
    Q_PROPERTY(QString background READ background NOTIFY settingsChanged)
    /// The background in linear light, for the shaders' fog.
    Q_PROPERTY(QVector3D backgroundLinear READ backgroundLinear NOTIFY settingsChanged)

public:
    /// The field's radius: the model's, fixed at 100 (the decoration is built for it).
    static constexpr float kRadius = 100.0F;

    explicit NeuralField(QObject *parent = nullptr);
    ~NeuralField() override;
    NeuralField(const NeuralField &) = delete;
    NeuralField &operator=(const NeuralField &) = delete;

    QString root() const { return m_root; }
    void setRoot(const QString &root);
    bool active() const { return m_active; }
    void setActive(bool active);
    bool loading() const { return m_loading; }
    QString sourceRoot() const { return m_sourceRoot; }
    QString scannedAt() const { return m_scannedAt; }
    bool still() const { return m_still; }
    void setStill(bool still);
    int limit() const { return m_limit; }
    void setLimit(int limit);
    int filesPerFolder() const { return m_filesPerFolder; }
    void setFilesPerFolder(int files);
    bool ready() const { return m_sphere != nullptr; }
    bool building() const;
    QString error() const { return m_error; }
    double time() const { return m_time; }
    qulonglong revision() const { return m_revision; }
    QVariantMap settings() const { return m_settings; }
    QString background() const { return m_background; }
    QVector3D backgroundLinear() const { return m_backgroundLinear; }

    /// Steps the sphere by `seconds` of wall time (the original steps once a frame).
    Q_INVOKABLE double advance(double seconds);
    /// Explicitly refresh the bounded filesystem snapshot; no synthetic live graph is inferred.
    Q_INVOKABLE void refresh();
    /// Waits for the listing and draws the original's deterministic frame; for captures.
    Q_INVOKABLE bool loadNow();
    Q_INVOKABLE bool setNumber(const QString &key, double value);
    Q_INVOKABLE bool setFlag(const QString &key, bool value);
    Q_INVOKABLE bool setBackground(const QString &hex);
    /// `on`: 0, 1 or anything else to keep; `opacity`: NaN keeps.
    Q_INVOKABLE bool setLink(int kind, int on, double opacity);
    Q_INVOKABLE bool select(const QString &id);
    /// Selects, and opens a folded program.
    Q_INVOKABLE bool activate(const QString &id);
    Q_INVOKABLE bool toggleFold(const QString &id);
    /// A ray in the field's own coordinates; the id of the node it meets first, or "".
    Q_INVOKABLE QString pick(const QVector3D &origin, const QVector3D &direction, float threshold = 3.0F) const;
    Q_INVOKABLE QVector3D positionOf(const QString &id) const;
    /// Checked coordinates for camera following. Missing is {found:false}; a real
    /// node at the origin remains {found:true,position:QVector3D(0,0,0)}.
    Q_INVOKABLE QVariantMap lookupPosition(const QString &id) const;
    /// Labels for the HUD: [{kind: "p"|"f", x, y, z, id, text}] in field coordinates.
    Q_INVOKABLE QVariantList labels();
    /// The HUD's selection card, one string a line.
    Q_INVOKABLE QStringList selection(const QString &rootLabel);
    Q_INVOKABLE QVariantMap stats() const;
    Q_INVOKABLE QVariantList linkCounts();
    /// The tilted orbit's pose (three's Euler XYZ order) and the base's turn, at `time`.
    Q_INVOKABLE QQuaternion orbitRotation(double time) const;
    Q_INVOKABLE QQuaternion baseRotation(double time) const;

    // For NeuralGeometry: copies of what Rust holds, empty when there is no sphere.
    std::vector<NeuralPoint> points() const;
    std::vector<NeuralVertex> lines(int layer) const;
    std::vector<NeuralSpark> sparks() const;

signals:
    void rootChanged();
    void activeChanged();
    void sourceChanged();
    void stillChanged();
    void errorChanged();
    void settingsChanged();
    /// Positions, colours or the sparks changed: geometries upload again.
    void updated();
    void selectionChanged();

private:
    void load();
    void accept(NeuralIndex *index);
    void release();
    void publish();
    void readSettings();

    ::NeuralSphere *m_sphere = nullptr;
    QString m_root;
    QString m_sourceRoot;
    QString m_scannedAt;
    QString m_error;
    QString m_background = QStringLiteral("#01060b");
    QVector3D m_backgroundLinear;
    QVariantMap m_settings;
    // What the operator set, kept so a sphere made after (or instead of) the last one gets it.
    QHash<QString, double> m_numbers;
    QHash<QString, bool> m_flags;
    QMap<int, QPair<int, double>> m_links;
    quint64 m_generation = 0;
    qulonglong m_revision = 0;
    qulonglong m_modelRevision = ~0ULL;
    double m_time = 0.0;
    int m_limit = 2500;
    int m_filesPerFolder = 30;
    bool m_still = false;
    bool m_active = true;
    bool m_loading = false;
};
