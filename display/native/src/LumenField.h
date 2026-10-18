#pragma once

// Lumenfield, live: the orb scene of the observatory.
//
// The TFOG model in `archie-displayd` composes the scene (`/api/orb/lumen`): which
// entities are folded into which module, where every orb's anchor is, which relations
// were observed. This object asks for the view the operator chose, keeps the physics that
// moves orbs to their anchors and away from the pointer, and turns the result into the
// instance tables and line buffers the QML scene draws. It holds the design's settings,
// profiles and the before/after comparison, and it answers picking.
//
// It reads; it never writes. The only request it makes is a GET for a view, and nothing
// the operator does here reaches Policy, Stage or any producer.

#include <QElapsedTimer>
#include <QHash>
#include <QNetworkAccessManager>
#include <QObject>
#include <QPointer>
#include <QSet>
#include <QTimer>
#include <QVariantList>
#include <QVariantMap>
#include <QVector3D>

#include <utility>
#include <vector>

#include "archie/visual/lumen.hpp"

class LumenGeometry;
class LumenInstancing;

class LumenField final : public QObject
{
    Q_OBJECT
    /// Drawn only while the orb scene is on screen; nothing is fetched or stepped otherwise.
    Q_PROPERTY(bool active READ active WRITE setActive NOTIFY activeChanged)
    Q_PROPERTY(QVariantMap settings READ settings NOTIFY settingsChanged)
    /// Linear colours and light levels the shaders read, derived from the settings.
    Q_PROPERTY(QVariantMap look READ look NOTIFY settingsChanged)
    Q_PROPERTY(QString profile READ profile NOTIFY settingsChanged)
    /// The look presets embedded in the display, by name.
    Q_PROPERTY(QStringList presets READ presets CONSTANT)
    Q_PROPERTY(QString lens READ lens NOTIFY lensChanged)
    Q_PROPERTY(QVariantMap scene READ scene NOTIFY sceneChanged)
    Q_PROPERTY(QVariantList modules READ modules NOTIFY sceneChanged)
    Q_PROPERTY(QVariantList categories READ categories NOTIFY categoriesChanged)
    Q_PROPERTY(QVariantList tree READ tree NOTIFY sceneChanged)
    Q_PROPERTY(QVariantMap selection READ selection NOTIFY selectionChanged)
    Q_PROPERTY(QVariantMap hovered READ hovered NOTIFY hoveredChanged)
    Q_PROPERTY(float spin READ spin NOTIFY frameAdvanced)
    Q_PROPERTY(float time READ time NOTIFY frameAdvanced)
    Q_PROPERTY(bool paused READ paused NOTIFY pausedChanged)
    Q_PROPERTY(int fps READ fps NOTIFY fpsChanged)
    Q_PROPERTY(float fieldRadius READ fieldRadius NOTIFY sceneChanged)
    Q_PROPERTY(float shellRadius READ shellRadius NOTIFY sceneChanged)
    Q_PROPERTY(QVector3D selectionPosition READ selectionPosition NOTIFY frameAdvanced)
    Q_PROPERTY(float selectionScale READ selectionScale NOTIFY frameAdvanced)
    Q_PROPERTY(bool selectionVisible READ selectionVisible NOTIFY frameAdvanced)
    Q_PROPERTY(QString status READ status NOTIFY statusChanged)
    Q_PROPERTY(QString error READ error NOTIFY statusChanged)
    Q_PROPERTY(bool loaded READ loaded NOTIFY sceneChanged)
    /// What the system is using right now, most engaged first: nodes with focus, voice,
    /// a Stage action or high activity. Each entry has id, name, category and score.
    Q_PROPERTY(QVariantList attention READ attention NOTIFY attentionChanged)

public:
    explicit LumenField(QObject *parent = nullptr);

    bool active() const { return m_active; }
    void setActive(bool active);
    QVariantMap settings() const { return m_settings; }
    QVariantMap look() const { return m_look; }
    QString profile() const { return m_profile; }
    QStringList presets() const;
    QString lens() const { return m_lens; }
    QVariantMap scene() const { return m_sceneInfo; }
    QVariantList modules() const { return m_modules; }
    QVariantList categories() const { return m_categoryRows; }
    QVariantList tree() const { return m_tree; }
    QVariantMap selection() const { return m_selectionInfo; }
    QVariantMap hovered() const { return m_hoveredInfo; }
    float spin() const { return m_spin; }
    float time() const { return m_time; }
    bool paused() const { return m_paused; }
    int fps() const { return m_fps; }
    float fieldRadius() const { return m_fieldRadius; }
    float shellRadius() const { return m_shellRadius; }
    QVector3D selectionPosition() const { return m_selectionPosition; }
    float selectionScale() const { return m_selectionScale; }
    bool selectionVisible() const { return m_selectionVisible; }
    QString status() const { return m_status; }
    QString error() const { return m_error; }
    bool loaded() const { return !m_nodes.empty(); }
    QVariantList attention() const { return m_attentionRows; }

    /// Hands a QML-created buffer to a layer: "modules", "entities", "satellites",
    /// "ghosts", "pulses" take a LumenInstancing; "orbMesh", "satelliteMesh", "quad",
    /// "ring", "membrane", "backbone", "highlight", "observed", "semantic", "fibres",
    /// "guides" take a LumenGeometry.
    Q_INVOKABLE void bindLayer(const QString &name, QObject *target);
    Q_INVOKABLE void setSetting(const QString &key, const QVariant &value);
    /// Applies a preset whole, or changes nothing; returns why not, or "" once applied.
    QString loadProfile(const QString &name);
    Q_INVOKABLE bool applyProfile(const QString &name);
    Q_INVOKABLE void resetProfile();
    Q_INVOKABLE void setLens(const QString &lens);
    Q_INVOKABLE void toggleModule(const QString &id);
    Q_INVOKABLE void collapse(const QString &id);
    Q_INVOKABLE void toggleCategory(const QString &key);
    Q_INVOKABLE void select(const QString &id);
    Q_INVOKABLE void clearSelection();
    /// Everything the view shows under `id`, with it at the centre ("Explorar").
    Q_INVOKABLE void explore(const QString &id);
    Q_INVOKABLE void up();
    /// Opens whatever hides `id` and selects it once the scene holds it.
    Q_INVOKABLE void reveal(const QString &id, const QStringList &ancestors, const QString &kind);
    /// A ray in the field's own coordinates; the id of the orb it meets first, or "".
    Q_INVOKABLE QString pick(const QVector3D &origin, const QVector3D &through) const;
    Q_INVOKABLE void hover(const QString &id);
    Q_INVOKABLE void togglePause();
    Q_INVOKABLE void refresh();
    /// The field's position for an id, for the camera to turn towards.
    Q_INVOKABLE QVector3D positionOf(const QString &id) const;

signals:
    void activeChanged();
    void settingsChanged();
    void lensChanged();
    void sceneChanged();
    void categoriesChanged();
    void selectionChanged();
    void hoveredChanged();
    void frameAdvanced();
    void pausedChanged();
    void fpsChanged();
    void statusChanged();
    /// The camera should frame the whole field again.
    void fitRequested();
    /// The camera should turn gently towards `position`.
    void focusRequested(const QVector3D &position);
    void attentionChanged();
    /// The most engaged node moved (`engaged`) or nothing is engaged any more.
    void attentionMoved(const QVector3D &position, bool engaged);
    /// A new field arrived from nothing: its orbs project out of the centre.
    void projectionStarted();

private:
    struct Node {
        QString id;
        QString name;
        QString parent;
        QString kind;
        QString category;
        QString role;
        QString entityKind;
        QString status;
        QStringList signalNames;
        QVector3D anchor;
        int memberCount = 1;
        int depth = 0;
        int parentIndex = -1;
        float size = 0.0F;
        float activity = 0.0F;
        float focus = 0.0F;
        float voice = 0.0F;
        float uncertainty = 0.0F;
        bool selected = false;
        bool expandable = false;
        bool expanded = false;
        bool branch = false;
    };
    struct Edge {
        int source = -1;
        int target = -1;
        QString kind;
        QString evidence;
        QStringList relations;
        int count = 0;
        float strength = 0.0F;
    };
    struct Satellite {
        int centre = -1;
        QVector3D offset;
        float size = 1.0F;
    };

    void request();
    void accept(const QByteArray &body);
    void tick();
    void rebuildStatic();
    void rebuildMembrane();
    void rebuildGhosts();
    void updateLayers();
    void updateHighlight();
    void updatePulses();
    void updateSelectionInfo();
    void updateHoveredInfo();
    void updateLook();
    void updateCategoryRows();
    void setStatus(const QString &status, const QString &error = {});
    void scheduleRequest(int delayMs = 0);
    int indexOf(const QString &id) const;
    bool drawn(int index) const;
    float orbScale(int index) const;
    float attentionOf(int index) const;
    void updateAttention();
    archie::visual::lumen::Rgb colourOf(const QString &category) const;
    float number(const char *key) const;
    bool flag(const char *key) const;
    QVector3D positionAt(int index) const;

    QNetworkAccessManager m_network;
    QTimer m_frameTimer;
    QTimer m_pollTimer;
    QTimer m_requestDelay;
    QElapsedTimer m_clock;
    QElapsedTimer m_fpsClock;
    /// Running while a fresh field projects out of the centre.
    QElapsedTimer m_projection;
    bool m_active = false;
    bool m_requestPending = false;
    bool m_requestAgain = false;
    bool m_paused = false;
    bool m_structureDirty = true;
    bool m_positionsDirty = true;
    int m_framesCounted = 0;
    int m_fps = 0;
    int m_settled = 0;

    QVariantMap m_settings;
    QVariantMap m_look;
    QString m_profile;
    QString m_lens = QStringLiteral("architecture");
    QSet<QString> m_expanded;
    QSet<QString> m_hiddenCategories;
    QString m_focus;
    QString m_selected;
    QString m_hoveredId;
    QString m_pendingReveal;
    QString m_status;
    QString m_error;

    std::vector<Node> m_nodes;
    std::vector<archie::visual::lumen::Body> m_bodies;
    QHash<QString, int> m_index;
    std::vector<Edge> m_relations;
    std::vector<int> m_backbone;
    std::vector<int> m_highlightNodes;
    std::vector<int> m_highlightRelations;
    std::vector<std::pair<int, int>> m_pulseLinks;
    std::vector<Satellite> m_satellites;
    std::vector<archie::visual::Vec3> m_ghostOffsets;
    QVariantMap m_sceneInfo;
    QVariantList m_modules;
    QVariantList m_categoryRows;
    QVariantList m_tree;
    QVariantMap m_selectionInfo;
    QVariantMap m_hoveredInfo;
    QVariantList m_attentionRows;
    std::vector<int> m_attention;
    QString m_attentionLead;
    QVariantMap m_counts;
    QString m_rootId;
    QString m_focusId;
    QStringList m_focusPath;
    float m_fieldRadius = 200.0F;
    float m_shellRadius = 200.0F;
    float m_spin = 0.0F;
    float m_time = 0.0F;
    float m_pulseTime = 0.0F;
    QVector3D m_selectionPosition;
    float m_selectionScale = 1.0F;
    bool m_selectionVisible = false;

    QHash<QString, QPointer<LumenInstancing>> m_tables;
    QHash<QString, QPointer<LumenGeometry>> m_geometry;
};
