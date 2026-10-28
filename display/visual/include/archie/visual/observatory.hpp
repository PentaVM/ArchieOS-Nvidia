#pragma once

// The whole model behind one object, and one frame out of it.
//
// Every other header here answers one question well. This one is where they have to
// agree, and integration is where the interesting failures live: a camera left focused
// on a node the graph has forgotten, a scene drawing four thousand nodes on a tier that
// budgeted four hundred, a node still animating from a snapshot that went stale two
// minutes ago. Each of those is correct behaviour in one module and a bug in the
// system.
//
// So the wiring is here, in one place, tested as its own thing:
//
//  - the graph forgets a node   -> the camera lets go of it
//  - the tier drops             -> the visible set shrinks to match, that frame
//  - the mode changes           -> the camera re-homes and incumbency is discarded
//  - the data goes stale        -> motion stops, without anyone having to remember to
//                                  check
//
// `tick` takes the current time and works out its own delta, so a caller cannot get the
// bookkeeping wrong. It normalises a backwards clock step to a zero delta at the
// boundary; the directors downstream refuse negative deltas independently, so this is a
// second line rather than the only one, and what is genuinely owned here is the *epoch*
// — after a rollback, later ticks measure from the new now rather than waiting out the
// gap.

#include <cstdint>
#include <map>
#include <string>
#include <utility>
#include <vector>

#include "archie/visual/budget.hpp"
#include "archie/visual/camera.hpp"
#include "archie/visual/events.hpp"
#include "archie/visual/freshness.hpp"
#include "archie/visual/graph.hpp"
#include "archie/visual/history.hpp"
#include "archie/visual/ingest.hpp"
#include "archie/visual/layout.hpp"
#include "archie/visual/lod.hpp"
#include "archie/visual/palette.hpp"
#include "archie/visual/scene.hpp"
#include "archie/visual/value.hpp"

namespace archie::visual {

/// One node, as the renderer receives it. Everything needed to draw it and nothing that
/// would let the renderer make a decision of its own.
struct DrawnNode {
    std::string id;
    std::string label;
    Vec3 position{};
    NodeShape shape = NodeShape::Placeholder;
    DetailLevel detail = DetailLevel::Simplified;
    NodePresence presence = NodePresence::Present;
    int severity = 0;
    Freshness freshness = Freshness::Live;

    /// Bounded, always. An unbounded provider magnitude must not be able to produce a
    /// node that fills the screen.
    float scale = 1.0F;

    /// How strongly to draw it, from freshness.
    float confidence = 1.0F;

    /// Whether the source that reported this was the authority for it. Carried
    /// separately from the colour so an inspector can say which of the two reasons a
    /// node is drawn faintly: it is old, or it is second-hand.
    bool authoritative = true;

    /// Where in the world it is, when that is a meaningful question. Present for
    /// observations, absent for everything else.
    std::optional<GeoPoint> location{};

    /// Fully resolved colour: hue and luminance from severity, opacity from freshness
    /// and presence, glow from severity and what the tier can afford. The renderer
    /// applies it rather than deriving anything of its own.
    NodeColour colour{};

    /// Whether this node may move at all. False for stale data and on tiers with no
    /// budget for idle motion — decided here so no renderer has to remember to ask.
    bool animate = true;

    /// How far through its current presence phase, in [0, 1].
    ///
    /// Entering runs 0 to 1 as the arrival plays; Departing runs 0 to 1 as the grace
    /// period elapses toward being forgotten; Present is always 1.
    ///
    /// It is here because the model owns the durations — entry length and departure
    /// grace are graph policy — and a renderer timing its own animations against them
    /// would be a second clock keeping the same appointment. Two clocks disagree: an
    /// entry that finished visually before the node settled, or a node erased mid-fade
    /// because its grace ran out while the renderer thought it had further to go.
    float presence_progress = 1.0F;
};

struct DrawnEdge {
    std::string from;
    std::string to;
};

/// Everything worth reading about the one node an operator is looking at.
///
/// Only ever populated for the focused node, and deliberately so. The scene carries a
/// few facts about thousands of things; this carries every fact about one, and building
/// it for all of them would be the same waste as drawing labels nobody can read.
///
/// Nothing here is computed for the panel's benefit. `explanation` is what the control
/// plane said, `dependents` is the reverse of the dependencies the payload declared, and
/// `first_seen_ms` is when this observatory first saw the thing rather than when it was
/// created — a distinction worth keeping, because a node that appeared four seconds ago
/// on a daemon that has been up for a week is a node that just restarted.
struct Inspection {
    std::string id;
    std::string label;
    NodeKind kind = NodeKind::Unknown;

    /// The version the payload reported, empty when it reported none.
    std::string version;

    /// Why the thing is in the state it is in, in the control plane's own words.
    std::string explanation;

    int severity = 0;
    Freshness freshness = Freshness::Live;
    NodePresence presence = NodePresence::Present;
    bool authoritative = true;

    /// What it declares it needs.
    std::vector<std::string> depends_on;

    /// What declares it needs this. The question an operator actually asks when they
    /// click something that has gone wrong.
    std::vector<std::string> dependents;

    /// How long this observatory has known about it, and when it was last in a payload.
    std::int64_t first_seen_ms = 0;
    std::int64_t last_seen_ms = 0;

    /// Where in the world, when that is a meaningful question.
    std::optional<GeoPoint> location{};
};

/// One frame's worth of instruction. Pure data: the renderer draws exactly this.
struct Frame {
    SceneMode mode = SceneMode::Cerebra;
    SceneMode incoming_mode = SceneMode::Cerebra;
    float morph_progress = 1.0F;

    RenderTier tier = RenderTier::Normal;
    TierReason tier_reason = TierReason::TelemetryUnavailable;
    RenderProfile profile{};

    CameraPose camera{};
    CameraState camera_state = CameraState::Resting;
    std::string focused;

    /// The detail behind `focused`, when there is one.
    ///
    /// Absent rather than empty when nothing is being inspected, so a renderer cannot
    /// draw a panel full of blanks for a node nobody chose.
    std::optional<Inspection> inspection{};

    std::vector<DrawnNode> nodes;
    std::vector<DrawnEdge> edges;

    /// Events the renderer can attach to something it is drawing.
    EventBatch events;

    /// Real changes to nodes this frame is not showing.
    ///
    /// Handing the renderer a pulse for a node it is not drawing gives it nowhere to put
    /// it; dropping the event instead would mean an operator watching the globe never
    /// learns that a module broke. So it is counted, and the interface can say that
    /// something happened somewhere else without inventing a place to show it.
    ///
    /// A removal always lands here, because by then the node is gone and there is
    /// nothing left to attach to. That is not a loss: the renderer learns a node has been
    /// removed from its absence in `nodes`, which is the same signal a frame earlier.
    int events_elsewhere = 0;

    /// Nodes in the scene that did not fit the tier's budget.
    int aggregated = 0;

    /// Nodes the scene knows about, drawn or not.
    int total_nodes = 0;

    /// Nodes this view is not about.
    ///
    /// Distinct from `aggregated`, and the difference matters to an operator: an
    /// aggregated node did not fit, and a filtered one was never meant to be here. A
    /// scene reporting "and 47 more" for things it deliberately excluded would send
    /// someone looking for a budget problem that does not exist.
    int filtered_out = 0;

    /// Modules that declare a dependency cycle. A registry bug, surfaced rather than
    /// silently absorbed by the layout.
    std::vector<std::string> dependency_cycle;

    /// Identifiers claimed by more than one thing in the same scene.
    ///
    /// The projections are separate lists from separate subsystems and nothing stops a
    /// module and a peer sharing a name. The scene graph keeps the first and refuses the
    /// second, which is the only choice that does not depend on ordering — but left
    /// unreported it means a node simply is not there, while every count in this frame
    /// says the payload was clean. A thing that vanishes with nothing said is the exact
    /// failure this library exists to prevent, so the collision travels with the frame.
    std::vector<std::string> colliding_ids;

    /// What the last snapshot ingest made of the payload.
    IngestReport ingest{};

    /// What the last world ingest made of its payload. Kept separate because the two
    /// arrive from different daemons on different schedules, and merging them would make
    /// a defect in one look like a defect in the other.
    IngestReport world_ingest{};
};

struct ObservatoryPolicy {
    GraphPolicy graph{};
    LodPolicy lod{};
    LayoutPolicy layout{};
    CameraPolicy camera{};
    ModePolicy mode{};
    BudgetPolicy budget{};
    Palette palette{};
    HistoryPolicy history{};
    FreshnessPolicy snapshot_freshness{2'000, 15'000, 120'000, 2'000};
    FreshnessPolicy kernel_freshness{1'500, 5'000, 30'000, 2'000};
    std::size_t event_capacity = 256;
};

class Observatory final {
public:
    Observatory() : Observatory(ObservatoryPolicy{}) {}
    explicit Observatory(ObservatoryPolicy policy);

    /// Feed a `DisplaySnapshot`. `age_ms` is how old the payload is.
    ///
    /// Produces both modules and peers: they arrive in one payload from one daemon, so
    /// splitting them across two calls would let a caller update half the scene and
    /// leave the other half looking as though it had gone quiet.
    void ingest_snapshot(const Value& snapshot, std::int64_t age_ms, std::int64_t now_ms);

    /// Feed a `WorldSnapshot`.
    ///
    /// Observations live in the same scene graph as everything else, so they get the
    /// same identity, grace and event handling; what makes them different is that they
    /// carry a real place, and the layout puts them there instead of on a shell.
    void ingest_world_payload(const Value& world, std::int64_t age_ms, std::int64_t now_ms);

    /// Feed a `KernelSnapshot`.
    void ingest_kernel_payload(const Value& kernel, std::int64_t age_ms, bool display_present,
                               std::int64_t now_ms);

    /// Whether anything is on screen. Only the budget changes: no kernel reading is
    /// received by this, so freshness still says "none yet" until one arrives.
    void set_display_present(bool display_present, std::int64_t now_ms);

    /// Operator input. Nothing else changes the mode or the camera's intent.
    void request_mode(SceneMode mode);
    void focus(const std::string& node_id);
    void release_focus();
    /// Earth-fixed orbit/zoom. Invalid input and requests outside Watcher are ignored.
    void orbit_globe(float longitude_delta, float latitude_delta, float zoom_steps);
    void tilt_globe(float delta_degrees);
    /// Read-only AOI framing. radius_m is geographic extent, never radial distance.
    bool frame_globe(double latitude, double longitude, double radius_m, double elevation_m = 0, bool animate = true);

    [[nodiscard]] bool exploration_camera_active() const noexcept { return m_exploration_camera; }

    /// Advance to `now_ms` and produce the frame to draw.
    ///
    /// Deliberately not [[nodiscard]]: advancing without drawing is a legitimate call —
    /// catching up after a stall, or stepping the model in a test — and marking it would
    /// force every such caller to invent a variable to throw away.
    Frame tick(std::int64_t now_ms);

    [[nodiscard]] const SceneGraph& graph() const noexcept { return m_graph; }

    /// What the scene looked like earlier, for scrubbing back.
    ///
    /// Records what was *true*, not what was drawn: a node the budget aggregated away
    /// was still there, and a timeline that only remembered the visible ones would show
    /// a quieter past than actually happened.
    [[nodiscard]] const History& history() const noexcept { return m_history; }
    [[nodiscard]] RenderTier tier() const noexcept { return m_budget.tier(); }

private:
    [[nodiscard]] CameraPose home_for(SceneMode mode) const;
    [[nodiscard]] CameraPose framing_for(const Vec3& position) const;

    ObservatoryPolicy m_policy{};
    SceneGraph m_graph;
    LodPlanner m_lod;
    CameraDirector m_camera;
    ModeDirector m_modes;
    BudgetGovernor m_budget;
    EventQueue m_events;
    History m_history;

    /// Layout, cached against the graph's generation.
    ///
    /// Recomputed when the graph changes shape and not otherwise: depth resolution and
    /// placement are pure functions of it, and at the Ultra tier's own node cap doing
    /// them every frame cost more than the entire frame budget.
    std::uint64_t m_layout_generation = 0;

    /// The graph generation the newest history entry was taken at, so a frame can tell
    /// whether any node has moved since without walking them all to find out.
    std::uint64_t m_history_generation = 0;
    bool m_layout_valid = false;
    std::map<std::string, Vec3> m_positions{};
    std::vector<std::string> m_cycle{};
    std::vector<std::pair<std::string, std::string>> m_edges{};

    /// When each projection last arrived, and how old it already was.
    ///
    /// Freshness has to keep ageing after the last payload. Deriving it once at ingest
    /// and storing it means a daemon that goes silent leaves its nodes claiming to be
    /// current forever — still at full strength, still moving — which is precisely the
    /// lie the freshness rules exist to prevent, arriving through the one door nobody
    /// was watching.
    ///
    /// The payload's own claim stays on the node; the passage of time is applied when the
    /// frame is built. Keeping them separate is what stops the graph changing on every
    /// tick, which would defeat the layout cache and make every node look freshly
    /// modified to the timeline.
    struct ProjectionStamp {
        bool seen = false;
        std::int64_t ingested_at_ms = 0;
        std::int64_t age_at_ingest_ms = 0;
    };
    ProjectionStamp m_snapshot_stamp{};
    ProjectionStamp m_world_stamp{};
    ProjectionStamp m_kernel_stamp{};

    /// Where the operator said this machine is, if they have said.
    ///
    /// Configuration carried on the status payload, never measured and never inferred.
    /// Absent is a real answer: the globe keeps its default framing rather than centring
    /// on a place nobody stated.
    std::optional<GeoPoint> m_home_place{};

    KernelReading m_kernel{};
    IngestReport m_ingest{};
    IngestReport m_world_ingest{};
    std::vector<std::string> m_collisions{};
    SceneMode m_last_target = SceneMode::Cerebra;
    std::string m_focused{};
    std::int64_t m_last_tick_ms = 0;
    bool m_ticked = false;
    bool m_exploration_camera = false;
    float m_world_tilt = 35.0F;
};

}  // namespace archie::visual
