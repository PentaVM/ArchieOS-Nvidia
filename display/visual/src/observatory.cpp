#include "archie/visual/observatory.hpp"

#include <algorithm>
#include <cmath>

namespace archie::visual {
namespace {

/// Scale from a provider's magnitude, bounded into [1, 2).
///
/// The bound is the point. Magnitude is an unbounded number from a source the display
/// does not control, and multiplying a node's size by it directly means one bad reading
/// — or one earthquake an order of magnitude larger than the last — produces a node
/// that swallows the scene and hides everything the operator was looking at.
float scale_from(float magnitude) noexcept {
    const float m = magnitude > 0.0F ? magnitude : 0.0F;
    return 1.0F + m / (1.0F + m);
}

/// How far through its presence phase a node is, in [0, 1].
///
/// Clamped at both ends, and a zero-length policy reads as finished rather than dividing
/// by it. A clock that has gone backwards since the node was last seen yields 0 — the
/// beginning of the phase — because the alternative is a negative progress that a
/// renderer would interpolate through the floor.
float presence_progress_of(const SceneNode& node, std::int64_t now_ms,
                           const GraphPolicy& policy) noexcept {
    const auto fraction = [](std::int64_t elapsed, std::int64_t span) {
        if (span <= 0) {
            return 1.0F;
        }
        if (elapsed <= 0) {
            return 0.0F;
        }
        if (elapsed >= span) {
            return 1.0F;
        }
        return static_cast<float>(elapsed) / static_cast<float>(span);
    };

    switch (node.presence) {
        case NodePresence::Entering:
            return fraction(now_ms - node.first_seen_ms, policy.entry_duration_ms);
        case NodePresence::Departing:
            return fraction(now_ms - node.last_seen_ms, policy.departure_grace_ms);
        case NodePresence::Present:
            break;
    }
    return 1.0F;
}

/// A projection's freshness now, rather than when it arrived.
///
/// A payload that was two seconds old when it landed is two seconds plus however long
/// ago that was. A projection nobody has ever sent is treated as current, because there
/// are no nodes from it to mis-describe.
Freshness freshness_now(bool seen, std::int64_t ingested_at_ms, std::int64_t age_at_ingest_ms,
                        std::int64_t now_ms, const FreshnessPolicy& policy) noexcept {
    if (!seen) {
        return Freshness::Live;
    }
    // A clock that has gone backwards cannot make a payload younger than it was.
    const std::int64_t elapsed = now_ms > ingested_at_ms ? now_ms - ingested_at_ms : 0;
    return classify_age(age_at_ingest_ms + elapsed, policy).tier;
}

float distance_between(const Vec3& a, const Vec3& b) noexcept {
    const float dx = a.x - b.x;
    const float dy = a.y - b.y;
    const float dz = a.z - b.z;
    return std::sqrt(dx * dx + dy * dy + dz * dz);
}

}  // namespace

Observatory::Observatory(ObservatoryPolicy policy)
    : m_policy(policy),
      m_graph(policy.graph),
      m_lod(policy.lod),
      m_camera(policy.camera),
      m_modes(policy.mode),
      m_budget(policy.budget),
      m_events(policy.event_capacity),
      m_history(policy.history) {
    // Placed rather than flown in: the observatory should open looking at something,
    // not swooping from a pose that was never meaningful.
    m_camera.set_home(home_for(SceneMode::Cerebra), false);
}

CameraPose Observatory::home_for(SceneMode mode) const {
    CameraPose pose{};
    pose.target = Vec3{0.0F, 0.0F, 0.0F};

    switch (mode) {
        case SceneMode::Cerebra:
            // High and back, far enough to hold every shell at once.
            pose.eye = Vec3{0.0F, 26.0F, 44.0F};
            pose.fov_degrees = 45.0F;
            break;
        case SceneMode::Brain:
            // Closer and lower: this view is about a part of the structure, so it trades
            // the overview for the ability to read labels.
            pose.eye = Vec3{0.0F, 12.0F, 24.0F};
            pose.fov_degrees = 40.0F;
            break;
        case SceneMode::Watcher: {
            // Standing off the globe, which is centred at the origin like everything
            // else, with a narrower field so the limb does not distort.
            //
            // The distance is derived from the globe's own radius rather than fixed. It
            // used to be a constant 58 while the radius came from policy, so the two
            // could be — and were — configured into incoherence: at 58 units with a
            // 32-degree field, the default radius-20 globe subtends about 40 degrees and
            // overflows the frame it is supposed to sit inside. A camera placed by a
            // constant frames the default and nothing else.
            pose.fov_degrees = 32.0F;
            constexpr float kDegreesToRadians = 3.14159265F / 180.0F;
            const float half_fov = pose.fov_degrees * 0.5F * kDegreesToRadians;
            // Fill about three quarters of the frame: enough that the globe is the
            // subject, with room for markers standing off the surface and for the limb
            // not to touch the edge.
            const float distance =
                m_policy.layout.globe.radius / std::sin(half_fov * 0.75F);

            if (m_home_place.has_value()) {
                // Stand off directly above the place the operator said the machine is, so
                // it sits in the centre of the globe as drawn. Straight out along the
                // surface normal: any other direction would put home off-centre by an
                // amount nobody could predict from the coordinates they typed.
                const Vec3 surface =
                    globe_position(*m_home_place, 0.0F, m_policy.layout.globe);
                const float length = std::sqrt(surface.x * surface.x + surface.y * surface.y +
                                               surface.z * surface.z);
                // A zero-length normal is not reachable through `globe_position`, which
                // always returns a point on or above a positive-radius sphere. Guarded
                // anyway, because the alternative to a guard here is a division by zero
                // that puts the camera at the origin, inside the planet.
                if (length > 0.0F) {
                    const float scale = distance / length;
                    pose.eye = Vec3{surface.x * scale, surface.y * scale, surface.z * scale};
                    break;
                }
            }

            pose.eye = Vec3{0.0F, distance * 0.10F, distance * 0.995F};
            break;
        }
        case SceneMode::Fabric:
            // Looking down at the topology, where the arrangement matters more than the
            // individual node.
            pose.eye = Vec3{0.0F, 52.0F, 18.0F};
            pose.fov_degrees = 50.0F;
            break;
    }
    return pose;
}

CameraPose Observatory::framing_for(const Vec3& position) const {
    CameraPose pose{};
    pose.target = position;
    // Offset outward from the node so it is framed rather than occupied, and lifted so
    // the camera is not inside the plane the node sits in.
    pose.eye = Vec3{position.x * 1.35F + 3.0F, position.y + 4.0F, position.z * 1.35F + 6.0F};
    pose.fov_degrees = 38.0F;
    return pose;
}

void Observatory::ingest_snapshot(const Value& snapshot, std::int64_t age_ms,
                                  std::int64_t now_ms) {
    m_ingest = IngestReport{};
    m_snapshot_stamp = ProjectionStamp{true, now_ms, age_ms};
    const FreshnessReading age = classify_age(age_ms, m_policy.snapshot_freshness);
    std::vector<NodeSpec> specs = ingest_modules(snapshot, age.tier, m_ingest);

    // Peers come from the same payload. One call rather than two, because a caller that
    // updated the modules and forgot the peers would put every peer into a grace period
    // it never earned.
    for (NodeSpec& peer : ingest_peers(snapshot, age.tier, m_ingest)) {
        specs.push_back(std::move(peer));
    }
    for (NodeSpec& model : ingest_models(snapshot, age.tier, m_ingest)) {
        specs.push_back(std::move(model));
    }

    // Carry the observations through. See `ingest_world_payload`: a snapshot that omits
    // them is not a snapshot saying they are gone.
    for (const auto& [id, node] : m_graph.nodes()) {
        if (node.spec.kind == NodeKind::Observation &&
            node.presence != NodePresence::Departing) {
            specs.push_back(node.spec);
        }
    }

    // Where the operator said this machine is, if they have. It is configuration
    // carried on the status payload rather than anything measured, and its absence is a
    // real answer: the globe stays on its default meridian instead of centring somewhere
    // plausible. A place the display invented is worse than no place at all.
    //
    // Read before the graph is applied so that a first payload carrying a location homes
    // the camera on the same tick the nodes arrive, rather than one tick later.
    if (const Value* status = snapshot.find("status"); status != nullptr) {
        if (const Value* location = status->find("location"); location != nullptr) {
            const Value* lat = location->find("latitude");
            const Value* lon = location->find("longitude");
            const std::optional<double> latitude =
                lat == nullptr ? std::nullopt : lat->as_number();
            const std::optional<double> longitude =
                lon == nullptr ? std::nullopt : lon->as_number();
            if (latitude.has_value() && longitude.has_value()) {
                const GeoPoint place{clamp_latitude(*latitude), wrap_longitude(*longitude)};
                if (!m_home_place.has_value() || !m_home_place->same_place_as(place)) {
                    m_home_place = place;
                    // Only move the camera if the operator is looking at the globe. Being
                    // yanked somewhere because a payload arrived is the thing the mode
                    // director exists to prevent.
                    if (m_modes.target() == SceneMode::Watcher) {
                        m_camera.set_home(home_for(SceneMode::Watcher));
                    }
                }
            } else if (!location->is_null()) {
                m_ingest.note(DefectKind::WrongType, "status.location",
                              "expected numeric latitude and longitude");
            }
        }
    }

    const GraphDelta delta = m_graph.apply(specs, now_ms);
    m_collisions = delta.duplicate_ids;
    m_events.observe(delta, now_ms);
}

void Observatory::ingest_world_payload(const Value& world, std::int64_t age_ms,
                                       std::int64_t now_ms) {
    m_world_ingest = IngestReport{};
    m_world_stamp = ProjectionStamp{true, now_ms, age_ms};
    const FreshnessReading age = classify_age(age_ms, m_policy.snapshot_freshness);
    const std::vector<NodeSpec> specs = ingest_world(world, age.tier, m_world_ingest);

    // Deliberately additive rather than a second reconciliation: `apply` treats anything
    // absent from the payload as departing, so handing it only the observations would
    // start a grace period on every module in the scene, and handing it only the modules
    // would do the same to the globe. The two projections describe different parts of
    // one scene and arrive on different schedules.
    std::vector<NodeSpec> combined = specs;
    for (const auto& [id, node] : m_graph.nodes()) {
        if (node.spec.kind != NodeKind::Observation &&
            node.presence != NodePresence::Departing) {
            combined.push_back(node.spec);
        }
    }

    const GraphDelta delta = m_graph.apply(combined, now_ms);
    m_collisions = delta.duplicate_ids;
    m_events.observe(delta, now_ms);
}

void Observatory::ingest_kernel_payload(const Value& kernel, std::int64_t age_ms,
                                        bool display_present, std::int64_t now_ms) {
    IngestReport report{};
    m_kernel_stamp = ProjectionStamp{true, now_ms, age_ms};
    const FreshnessReading age = classify_age(age_ms, m_policy.kernel_freshness);
    m_kernel = ingest_kernel(kernel, age.tier, display_present, report);
    m_budget.observe(m_kernel, now_ms);
}

void Observatory::set_display_present(bool display_present, std::int64_t now_ms) {
    m_kernel.display_present = display_present;
    m_budget.observe(m_kernel, now_ms);
}

void Observatory::request_mode(SceneMode mode) {
    if (mode != m_modes.target()) m_exploration_camera = false;
    if (mode != m_modes.target() && m_camera.manual_navigation()) m_camera.release_focus();
    m_modes.request(mode);
}

void Observatory::orbit_globe(float longitude_delta, float latitude_delta, float zoom_steps) {
    if (m_modes.target() != SceneMode::Watcher || !std::isfinite(longitude_delta) ||
        !std::isfinite(latitude_delta) || !std::isfinite(zoom_steps)) return;
    const CameraPose current = m_camera.pose();
    // Geographic pan and altitude zoom are one continuous navigation model. A
    // close camera moves its ground target; it never becomes trapped orbiting
    // the last captured AOI. Double intermediates keep metre-scale input useful.
    const double earth=m_policy.layout.globe.radius;
    const auto norm=[](Vec3 p){return std::sqrt(double(p.x)*p.x+double(p.y)*p.y+double(p.z)*p.z);};
    const double eyeRadius=norm(current.eye),targetRadius=norm(current.target);
    if(!std::isfinite(eyeRadius)||eyeRadius<=0) return;
    constexpr double degrees=180.0/3.14159265358979323846;
    const double ground=m_exploration_camera&&targetRadius>earth*0.99 ? targetRadius : earth;
    const double altitude=std::max(10.0*earth/6371008.8,targetRadius>earth*0.99 ?
        norm(Vec3{current.eye.x-current.target.x,current.eye.y-current.target.y,current.eye.z-current.target.z}):eyeRadius-ground);
    const double speed=std::clamp(altitude/earth,0.000001,1.0);
    const Vec3 anchor=targetRadius>earth*0.99 ? current.target : current.eye;
    const double anchorRadius=norm(anchor);
    const double oldLatitude=std::asin(std::clamp(double(anchor.y)/anchorRadius,-1.0,1.0))*degrees;
    const double longitude=std::atan2(anchor.x,anchor.z)*degrees+
        std::clamp(double(longitude_delta),-180.0,180.0)*speed/std::max(0.04,std::cos(oldLatitude/degrees));
    const double latitude=std::clamp(oldLatitude+std::clamp(double(latitude_delta),-180.0,180.0)*speed,-89.0,89.0);
    const double nextAltitude=std::clamp(altitude*std::exp(-std::clamp(double(zoom_steps),-10.0,10.0)*0.20),10.0*earth/6371008.8,earth*7.0);
    const auto normal=globe_position({latitude,longitude},0,GlobePolicy{1,0,0});
    CameraPose next=current;
    next.eye={float(normal.x*(ground+nextAltitude)),float(normal.y*(ground+nextAltitude)),float(normal.z*(ground+nextAltitude))};
    // Distant framing looks at Earth's centre. Below one radius a surface
    // target improves the local near plane and keeps geographic pan intuitive.
    next.target=nextAltitude<earth ? Vec3{float(normal.x*ground),float(normal.y*ground),float(normal.z*ground)} : Vec3{};
    m_exploration_camera=nextAltitude<earth;
    m_focused.clear();m_camera.navigate(next);
    if(nextAltitude*6371008.8/earth<100000.) tilt_globe(0);
}

void Observatory::tilt_globe(float delta_degrees) {
    if(m_modes.target()!=SceneMode::Watcher||!std::isfinite(delta_degrees)) return;
    const auto current=m_camera.pose();const double earth=m_policy.layout.globe.radius;
    const auto norm=[](Vec3 p){return std::sqrt(double(p.x)*p.x+double(p.y)*p.y+double(p.z)*p.z);};
    const double radius=norm(current.target);if(radius<earth*0.99||radius>earth*1.01)return;
    const double nx=current.target.x/radius,ny=current.target.y/radius,nz=current.target.z/radius;
    const double horizontal=std::hypot(nx,nz);if(horizontal<1e-5)return;
    m_world_tilt=std::clamp(m_world_tilt+delta_degrees,0.0F,70.0F);
    const double ex=current.eye.x-current.target.x,ey=current.eye.y-current.target.y,ez=current.eye.z-current.target.z;
    const double distance=std::sqrt(ex*ex+ey*ey+ez*ez);
    const double fade=std::clamp((100000.-distance*6371008.8/earth)/80000.,0.,1.);
    const double angle=m_world_tilt*fade*3.14159265358979323846/180.;
    const double northX=-nx*ny/horizontal,northY=horizontal,northZ=-nz*ny/horizontal;
    CameraPose next=current;
    next.eye={float(current.target.x+distance*(nx*std::cos(angle)-northX*std::sin(angle))),
              float(current.target.y+distance*(ny*std::cos(angle)-northY*std::sin(angle))),
              float(current.target.z+distance*(nz*std::cos(angle)-northZ*std::sin(angle)))};
    m_camera.navigate(next);
}

bool Observatory::frame_globe(double latitude, double longitude, double radius_m, double elevation_m, bool animate) {
    if (m_modes.target()!=SceneMode::Watcher || !std::isfinite(latitude) || latitude < -90 || latitude > 90 ||
        !std::isfinite(longitude) || longitude < -180 || longitude > 180 ||
        !std::isfinite(radius_m) || radius_m<=0 || radius_m>6371000 ||
        !std::isfinite(elevation_m) || elevation_m < -12000 || elevation_m > 12000) return false;
    const float scale=m_policy.layout.globe.radius/6371000.0F;
    GlobePolicy surface{m_policy.layout.globe.radius+static_cast<float>(elevation_m)*scale,0,0};
    const auto target=globe_position(GeoPoint{latitude,longitude},0,surface);
    const float standOff=std::max(25.0F*scale,static_cast<float>(radius_m)*scale*3.0F);
    const auto normal=globe_position(GeoPoint{latitude,longitude},0,GlobePolicy{1,0,0});
    CameraPose next=m_camera.pose();next.target=target;
    next.eye={target.x+normal.x*standOff,target.y+normal.y*standOff,target.z+normal.z*standOff};
    m_focused.clear();m_exploration_camera=true;m_camera.navigate(next,animate,true);return true;
}

void Observatory::focus(const std::string& node_id) {
    const SceneNode* node = m_graph.find(node_id);
    if (node == nullptr) {
        // Focusing something that is not there would leave the camera diving at a point
        // no node occupies, and the inspector naming a node the scene cannot show.
        return;
    }
    if (!visible_in(m_modes.target(), node->spec.kind)) {
        // Nor something this view does not contain. A click cannot have come from a node
        // that is not on screen, so this is a caller error rather than an operator one,
        // and diving at it would be the same empty patch of space by another route.
        return;
    }
    const DepthReport depths = compute_depths(m_graph.nodes());
    const auto found = depths.depth.find(node_id);
    const int depth = found == depths.depth.end() ? 0 : found->second;

    m_focused = node_id;
    const Vec3 position =
        node->spec.location.has_value()
            ? globe_position(*node->spec.location, node->spec.magnitude, m_policy.layout.globe)
            : place(node->slot, depth, node->spec.severity, m_policy.layout);
    m_exploration_camera=false;
    m_camera.focus(node_id, framing_for(position));
}

void Observatory::release_focus() {
    m_exploration_camera=false;
    m_focused.clear();
    m_camera.release_focus();
}

Frame Observatory::tick(std::int64_t now_ms) {
    // The delta is worked out here so a caller cannot get it wrong. A backwards step
    // advances nothing and the frame is still produced -- it simply does not move. The
    // directors refuse negative deltas themselves, so the clamp is a second line; what
    // is genuinely owned here is the epoch, which moves to the rolled-back now so later
    // frames resume rather than freezing until the clock climbs back.
    std::int64_t delta_ms = 0;
    if (m_ticked && now_ms > m_last_tick_ms) {
        delta_ms = now_ms - m_last_tick_ms;
    }
    m_last_tick_ms = now_ms;
    m_ticked = true;

    m_modes.advance(delta_ms);
    m_camera.advance(delta_ms);

    // The governor is fed every frame, not only when telemetry arrives.
    //
    // It already refuses to let a stale reading buy a rich tier -- that rule is in
    // `ceiling_for` and it is tested. It simply never ran, because the only thing that
    // called `observe` was an ingest, so a kernel reader that stopped answering left the
    // machine rendering at whatever tier its last word had earned. Ten minutes of silence
    // and the display was still spending a cinematic budget on evidence that old.
    //
    // Feeding it here also puts the promotion dwell on real time rather than on the
    // polling interval, which is what it was always specified in.
    KernelReading aged = m_kernel;
    aged.freshness =
        freshness_now(m_kernel_stamp.seen, m_kernel_stamp.ingested_at_ms,
                      m_kernel_stamp.age_at_ingest_ms, now_ms, m_policy.kernel_freshness);
    m_budget.observe(aged, now_ms);

    // A mode change re-homes the camera and discards incumbency: importance in the view
    // the operator has left is not importance in the one they entered.
    if (m_modes.target() != m_last_target) {
        m_last_target = m_modes.target();
        m_camera.set_home(home_for(m_last_target));
        m_lod.reset();
    }

    // A view the operator has asked for may not contain what they were inspecting. The
    // rule is the same one that applies to a node the graph has forgotten: a camera
    // pointed at something the scene is not drawing shows an empty patch of space beside
    // an inspector naming a node that is not there, and nothing distinguishes that from
    // a node which has gone quiet.
    //
    // The destination decides, not the view being left. An operator on their way to
    // Fabric has already said they are done with the module they were reading, and
    // waiting for the morph to finish before letting go would fly them to the new view
    // still holding the old one.
    if (!m_focused.empty()) {
        const SceneNode* focused = m_graph.find(m_focused);
        if (focused != nullptr && !visible_in(m_modes.target(), focused->spec.kind)) {
            release_focus();
        }
    }

    // Forget nodes whose grace has run out, and let the camera know before anything
    // tries to frame one of them.
    const GraphDelta collected = m_graph.collect(now_ms);
    m_events.observe(collected, now_ms);
    for (const std::string& id : collected.removed) {
        m_camera.note_node_removed(id);
        if (m_focused == id) {
            m_focused.clear();
        }
    }

    Frame frame{};
    frame.mode = m_modes.current();
    frame.incoming_mode = m_modes.target();
    frame.morph_progress = m_modes.progress();
    frame.tier = m_budget.tier();
    frame.tier_reason = m_budget.reason();
    frame.profile = m_budget.profile();
    frame.camera = m_camera.pose();
    frame.camera_state = m_camera.state();
    frame.focused = m_camera.focused_node();
    frame.total_nodes = static_cast<int>(m_graph.size());
    frame.ingest = m_ingest;
    frame.world_ingest = m_world_ingest;

    if (!m_layout_valid || m_graph.generation() != m_layout_generation) {
        const DepthReport depths = compute_depths(m_graph.nodes());
        m_positions = lay_out(m_graph.nodes(), depths, m_policy.layout);
        m_cycle = depths.in_cycle;
        // Edges come from declared dependencies, so they change exactly when the graph
        // does. Rebuilding eight thousand string pairs per frame for a list that had not
        // changed since the last poll was most of what an edge cost.
        m_edges = m_graph.edges();
        m_layout_generation = m_graph.generation();
        m_layout_valid = true;
    }
    frame.dependency_cycle = m_cycle;
    frame.colliding_ids = m_collisions;
    const std::map<std::string, Vec3>& positions = m_positions;

    std::vector<LodCandidate> candidates;
    candidates.reserve(m_graph.size());
    for (const auto& [id, node] : m_graph.nodes()) {
        // Both ends of a morph, so a node arriving in the incoming view is already in
        // the scene when the transition starts and can fade in rather than appearing
        // once it finishes.
        if (!visible_in(frame.mode, node.spec.kind) &&
            !visible_in(frame.incoming_mode, node.spec.kind)) {
            ++frame.filtered_out;
            continue;
        }

        LodCandidate candidate{};
        candidate.id = id;
        candidate.severity = node.spec.severity;
        candidate.magnitude = node.spec.magnitude;
        candidate.freshness = node.spec.freshness;
        const auto position = positions.find(id);
        candidate.distance = position == positions.end()
                                 ? 0.0F
                                 : distance_between(frame.camera.eye, position->second);
        candidates.push_back(std::move(candidate));
    }

    const LodPlan plan =
        m_lod.plan(candidates, frame.profile.max_visible_nodes, m_camera.focused_node());
    frame.aggregated = plan.aggregated;

    // What each projection's data is worth *now*, rather than when it landed.
    const Freshness snapshot_now =
        freshness_now(m_snapshot_stamp.seen, m_snapshot_stamp.ingested_at_ms,
                      m_snapshot_stamp.age_at_ingest_ms, now_ms, m_policy.snapshot_freshness);
    const Freshness world_now =
        freshness_now(m_world_stamp.seen, m_world_stamp.ingested_at_ms,
                      m_world_stamp.age_at_ingest_ms, now_ms, m_policy.snapshot_freshness);

    frame.nodes.reserve(plan.visible.size());
    for (const VisibleNode& visible : plan.visible) {
        const SceneNode* node = m_graph.find(visible.id);
        if (node == nullptr) {
            continue;
        }
        DrawnNode drawn{};
        drawn.id = visible.id;
        drawn.label = node->spec.label;
        const auto position = positions.find(visible.id);
        drawn.position = position == positions.end() ? Vec3{} : position->second;
        drawn.shape = shape_of(node->spec.kind);
        drawn.detail = visible.detail;
        drawn.presence = node->presence;
        drawn.severity = node->spec.severity;
        // Freshness comes from the projection's stamp rather than from the node, because
        // the stamp is the same claim plus the time since. Taking the worse of the two
        // was the first shape of this and a mutation proved it could never differ: the
        // stored value is derived from the age at ingest, and the stamp is that age plus
        // an elapsed time that cannot be negative.
        drawn.freshness =
            node->spec.kind == NodeKind::Observation ? world_now : snapshot_now;
        drawn.scale = scale_from(node->spec.magnitude);
        drawn.confidence = confidence_of(drawn.freshness);
        drawn.authoritative = node->spec.authoritative;
        drawn.location = node->spec.location;
        drawn.colour = colour_for(node->spec.severity, drawn.freshness, node->presence,
                                  node->spec.authoritative, frame.tier, m_policy.palette);
        // Both conditions, decided once. Motion on stale data claims the value is
        // current; motion on a saturated machine spends what the machine needs.
        drawn.animate = may_animate(drawn.freshness) && frame.profile.ambient_motion;
        drawn.presence_progress = presence_progress_of(*node, now_ms, m_policy.graph);
        frame.nodes.push_back(std::move(drawn));
    }

    // Edges only between nodes that are actually being drawn. An edge to a node the
    // budget aggregated away would be a line into empty space.
    std::vector<std::string> drawn_ids;
    drawn_ids.reserve(frame.nodes.size());
    for (const DrawnNode& node : frame.nodes) {
        drawn_ids.push_back(node.id);
    }
    std::sort(drawn_ids.begin(), drawn_ids.end());

    for (const auto& [from, to] : m_edges) {
        if (std::binary_search(drawn_ids.begin(), drawn_ids.end(), from) &&
            std::binary_search(drawn_ids.begin(), drawn_ids.end(), to)) {
            frame.edges.push_back(DrawnEdge{from, to});
        }
    }

    // Recorded from the graph rather than from `frame.nodes`: a node the budget
    // aggregated away was still there, and a timeline that only remembered what fitted
    // on screen would show a quieter past than the one that happened.
    //
    // The cheap facts are checked before the expensive ones are gathered. History refuses
    // most entries — a still scene changes nothing between frames — and building four
    // thousand samples to hand over something that will be dropped is work nobody ever
    // sees. Everything consulted here is a scalar the frame already has, except the
    // graph's generation, which is exactly what says whether any node moved.
    const HistoryEntry* previous = m_history.newest();
    const bool worth_recording =
        previous == nullptr || now_ms > previous->at_ms + m_policy.history.keyframe_interval_ms ||
        m_graph.generation() != m_history_generation || frame.tier != previous->tier ||
        frame.tier_reason != previous->tier_reason || frame.mode != previous->mode ||
        frame.total_nodes != previous->total_nodes || frame.aggregated != previous->aggregated;

    if (worth_recording) {
        HistoryEntry entry{};
        entry.at_ms = now_ms;
        entry.tier = frame.tier;
        entry.tier_reason = frame.tier_reason;
        entry.mode = frame.mode;
        entry.total_nodes = frame.total_nodes;
        entry.aggregated = frame.aggregated;
        entry.nodes.reserve(m_graph.size());
        for (const auto& [id, node] : m_graph.nodes()) {
            entry.nodes.push_back(NodeSample{
                id, node.spec.severity,
                node.spec.kind == NodeKind::Observation ? world_now : snapshot_now,
                node.presence});
        }
        if (m_history.record(std::move(entry))) {
            m_history_generation = m_graph.generation();
        }
    }

    // The detail behind whatever is being inspected. Built once, for one node, and only
    // when there is one — the scene already carries what it needs for the other
    // thousands.
    if (!frame.focused.empty()) {
        if (const SceneNode* node = m_graph.find(frame.focused); node != nullptr) {
            Inspection inspection{};
            inspection.id = frame.focused;
            inspection.label = node->spec.label;
            inspection.kind = node->spec.kind;
            inspection.version = node->spec.version;
            inspection.explanation = node->spec.explanation;
            inspection.severity = node->spec.severity;
            inspection.freshness = node->spec.kind == NodeKind::Observation ? world_now
                                                                           : snapshot_now;
            inspection.presence = node->presence;
            inspection.authoritative = node->spec.authoritative;
            inspection.depends_on = node->spec.depends_on;
            inspection.dependents = m_graph.dependents_of(frame.focused);
            inspection.first_seen_ms = node->first_seen_ms;
            inspection.last_seen_ms = node->last_seen_ms;
            inspection.location = node->spec.location;
            frame.inspection = std::move(inspection);
        }
    }

    // Events are partitioned against what this frame is actually drawing. `drawn_ids` is
    // already sorted for the edge filter above, so this costs a binary search per event
    // rather than a second pass over the scene.
    EventBatch batch = m_events.drain();
    frame.events.dropped = batch.dropped;
    frame.events.events.reserve(batch.events.size());
    for (VisualEvent& event : batch.events) {
        if (std::binary_search(drawn_ids.begin(), drawn_ids.end(), event.node_id)) {
            frame.events.events.push_back(std::move(event));
        } else {
            ++frame.events_elsewhere;
        }
    }

    return frame;
}

}  // namespace archie::visual
