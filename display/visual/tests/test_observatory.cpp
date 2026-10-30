// Integration: the places where two correct modules can still add up to a bug.

#include "archie/visual/observatory.hpp"

#include <algorithm>
#include <cmath>
#include <string>

#include "harness.hpp"

using namespace archie::visual;

namespace {

Value detailed_module(const std::string& id, const char* state, const char* version,
                      const char* explanation, std::vector<Value> dependencies = {}) {
    return Value::object({member("id", Value{id}),
                          member("version", Value{version}),
                          member("state", Value{state}),
                          member("explanation", Value{explanation}),
                          member("dependencies", Value::array(std::move(dependencies)))});
}

Value module_record(const std::string& id, const std::string& state,
                    std::vector<Value> dependencies = {}) {
    return Value::object({
        member("id", Value{id}),
        member("state", Value{state}),
        member("dependencies", Value::array(std::move(dependencies))),
    });
}

/// A DisplaySnapshot always carries both lists, so a fixture that omits one is not a
/// smaller version of the real payload -- it is a malformed one, and testing against it
/// would hide the defect report a real caller depends on.
Value snapshot_of(std::vector<Value> modules, std::vector<Value> peers = {},
                  std::vector<Value> models = {}) {
    return Value::object({member("modules", Value::array(std::move(modules))),
                          member("nodes", Value::array(std::move(peers))),
                          member("models", Value::array(std::move(models)))});
}

Value model_record(const std::string& id, const char* availability, double context_window) {
    return Value::object({member("id", Value{id}),
                          member("provider", Value{"local"}),
                          member("locality", Value{"on-node"}),
                          member("availability", Value{availability}),
                          member("max_privacy_grade", Value{"private"}),
                          member("context_window", Value{context_window})});
}

Value peer_record(const std::string& id, const char* health, bool reachable,
                  std::vector<Value> capabilities = {}) {
    return Value::object({
        member("node_id", Value{id}),
        member("hostname", Value{id + ".local"}),
        member("profile", Value{"workstation"}),
        member("trust", Value{"trusted"}),
        member("health", Value{health}),
        member("reachable", Value{reachable}),
        member("capabilities", Value::array(std::move(capabilities))),
    });
}

Value kernel_of(const char* pressure, const char* verdict) {
    return Value::object({
        member("pressure_availability", Value::object({member("state", Value{pressure})})),
        member("verdict", Value{verdict}),
        member("thermal_availability", Value::object({member("state", Value{"available"})})),
        member("thermal_state", Value{"nominal"}),
    });
}

bool drawn(const Frame& frame, const std::string& id) {
    return std::any_of(frame.nodes.begin(), frame.nodes.end(),
                       [&](const DrawnNode& n) { return n.id == id; });
}

const DrawnNode* node_in(const Frame& frame, const std::string& id) {
    const auto it = std::find_if(frame.nodes.begin(), frame.nodes.end(),
                                 [&](const DrawnNode& n) { return n.id == id; });
    return it == frame.nodes.end() ? nullptr : &*it;
}

Value busy_snapshot(int count) {
    std::vector<Value> modules;
    for (int i = 0; i < count; ++i) {
        modules.push_back(module_record("m" + std::to_string(i), "loaded"));
    }
    return snapshot_of(std::move(modules));
}

// ---------------------------------------------------------------------------

void a_snapshot_becomes_a_drawable_frame() {
    CASE("end to end: payload in, frame out");

    Observatory observatory{};
    observatory.ingest_snapshot(
        snapshot_of({module_record("core", "loaded"),
                     module_record("built.on.core", "loaded", {Value{"core"}})}),
        0, 1'000);

    const Frame frame = observatory.tick(1'000);

    CHECK_EQ(frame.total_nodes, 2);
    CHECK_EQ(frame.nodes.size(), 2U);
    CHECK_EQ(frame.edges.size(), 1U);
    CHECK_EQ(frame.mode, SceneMode::Cerebra);
    CHECK_BECAUSE(frame.events.events.size() == 2,
                  "two nodes appeared, and that is a real change worth a pulse");
    CHECK(frame.ingest.clean());

    // The dependency is legible as distance from the centre.
    const DrawnNode* base = node_in(frame, "core");
    const DrawnNode* built = node_in(frame, "built.on.core");
    CHECK(base != nullptr && built != nullptr);
    const float base_r = std::sqrt(base->position.x * base->position.x +
                                   base->position.z * base->position.z);
    const float built_r = std::sqrt(built->position.x * built->position.x +
                                    built->position.z * built->position.z);
    CHECK(built_r > base_r);
}

void forgetting_a_node_releases_the_camera() {
    CASE("the graph and the camera agree about what exists");

    ObservatoryPolicy policy{};
    policy.graph.departure_grace_ms = 1'000;
    Observatory observatory{policy};

    observatory.ingest_snapshot(snapshot_of({module_record("watched", "loaded"),
                                             module_record("other", "loaded")}),
                                0, 0);
    observatory.tick(0);
    observatory.focus("watched");
    CHECK_EQ(observatory.tick(10).focused, std::string{"watched"});

    // It vanishes from the payload and its grace runs out.
    observatory.ingest_snapshot(snapshot_of({module_record("other", "loaded")}), 0, 100);
    const Frame frame = observatory.tick(5'000);

    CHECK_BECAUSE(frame.focused.empty(),
                  "a camera left on a node the graph has forgotten stares at empty space "
                  "beside an inspector of fields that will never update again, and nothing "
                  "tells the operator that from a node which has merely gone quiet");
    CHECK(frame.camera_state == CameraState::Returning ||
          frame.camera_state == CameraState::Resting);
    CHECK(!drawn(frame, "watched"));
}

void an_unrelated_departure_does_not_interrupt_an_inspection() {
    CASE("nodes come and go constantly; that must not cost the operator their place");

    ObservatoryPolicy policy{};
    policy.graph.departure_grace_ms = 1'000;
    Observatory observatory{policy};

    observatory.ingest_snapshot(
        snapshot_of({module_record("watched", "loaded"), module_record("doomed", "loaded")}), 0,
        0);
    observatory.tick(0);
    observatory.focus("watched");

    observatory.ingest_snapshot(snapshot_of({module_record("watched", "loaded")}), 0, 100);
    const Frame frame = observatory.tick(5'000);

    CHECK_EQ(frame.focused, std::string{"watched"});
    CHECK(drawn(frame, "watched"));
}

void the_tier_caps_what_the_scene_draws() {
    CASE("the budget and the visible set agree, in the same frame");

    Observatory observatory{};
    observatory.ingest_snapshot(busy_snapshot(3'000), 0, 0);

    // A saturated machine. The cap must apply to this frame, not the next one.
    observatory.ingest_kernel_payload(kernel_of("available", "saturated"), 0, true, 0);
    const Frame frame = observatory.tick(0);

    CHECK_EQ(frame.tier, RenderTier::Conserve);
    CHECK_BECAUSE(static_cast<int>(frame.nodes.size()) <= frame.profile.max_visible_nodes,
                  "a scene drawing four thousand nodes on a tier that budgeted four hundred "
                  "is correct in two modules and a bug in the system");
    CHECK_EQ(frame.nodes.size(), static_cast<std::size_t>(frame.profile.max_visible_nodes));
    CHECK_BECAUSE(frame.aggregated == frame.total_nodes -
                                          static_cast<int>(frame.nodes.size()),
                  "and the count of what did not fit still adds up, so the scene can say so");
}

void a_headless_tier_draws_nothing_but_forgets_nothing() {
    CASE("state stays correct while there is nobody to show it to");

    Observatory observatory{};
    observatory.ingest_snapshot(busy_snapshot(20), 0, 0);
    observatory.ingest_kernel_payload(kernel_of("available", "idle"), 0, false, 0);

    const Frame dark = observatory.tick(0);
    CHECK_EQ(dark.tier, RenderTier::Headless);
    CHECK(dark.nodes.empty());
    CHECK_BECAUSE(dark.total_nodes == 20,
                  "the graph is still maintained, so the scene is correct the moment a screen "
                  "returns rather than being rebuilt from nothing");
    CHECK_EQ(dark.aggregated, 20);

    observatory.ingest_kernel_payload(kernel_of("available", "idle"), 0, true, 100);
    const Frame lit = observatory.tick(100);
    CHECK_BECAUSE(lit.nodes.size() == 20U,
                  "and everything is there immediately, with no repopulation to watch");
}

void stale_data_never_moves() {
    CASE("freshness reaches the renderer as a decision, not as a number to interpret");

    Observatory observatory{};
    // A snapshot four minutes old.
    observatory.ingest_snapshot(snapshot_of({module_record("old", "loaded")}), 240'000, 0);
    const Frame frame = observatory.tick(0);

    const DrawnNode* node = node_in(frame, "old");
    CHECK(node != nullptr);
    CHECK_EQ(node->freshness, Freshness::Expired);
    CHECK_BECAUSE(!node->animate,
                  "motion is the strongest 'this is happening now' signal a screen has, and "
                  "no renderer should have to remember to check before using it");
    CHECK(node->confidence < 1.0F);
}

void a_conserving_tier_stops_idle_motion_even_on_live_data() {
    CASE("the two reasons to stop moving are both honoured");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("live", "loaded")}), 0, 0);
    observatory.ingest_kernel_payload(kernel_of("available", "saturated"), 0, true, 0);

    const Frame frame = observatory.tick(0);
    const DrawnNode* node = node_in(frame, "live");
    CHECK(node != nullptr);
    CHECK_EQ(node->freshness, Freshness::Live);
    CHECK_BECAUSE(!node->animate,
                  "the data is current, so it is not a truthfulness problem â€” it is that idle "
                  "motion costs CPU the machine has already said it needs");
}

void a_mode_change_rehomes_and_forgets_incumbency() {
    CASE("importance in the view you left is not importance in the one you entered");

    Observatory observatory{};
    observatory.ingest_snapshot(busy_snapshot(10), 0, 0);
    const Frame before = observatory.tick(0);

    observatory.request_mode(SceneMode::Watcher);
    const Frame during = observatory.tick(16);
    CHECK_EQ(during.incoming_mode, SceneMode::Watcher);
    CHECK(during.morph_progress < 1.0F);

    for (int i = 0; i < 200; ++i) {
        observatory.tick(16 + i * 16);
    }
    const Frame after = observatory.tick(4'000);
    CHECK_EQ(after.mode, SceneMode::Watcher);
    CHECK_BECAUSE(!(after.camera == before.camera),
                  "each view frames the scene for what it is about, so arriving in one must "
                  "actually move the camera");
}

void a_mode_change_actually_discards_incumbency() {
    CASE("a marginal node loses its seat when the operator changes view");

    // The margin only shows up in a scene that is over budget, with two candidates close
    // enough that incumbency alone decides between them. Conserve draws 400.
    const auto scene = [](int alpha_deps, int beta_deps) {
        std::vector<Value> modules;
        for (int i = 0; i < 399; ++i) {
            modules.push_back(module_record("severe" + std::to_string(i), "invalid"));
        }
        const auto with_deps = [](const std::string& id, int count) {
            std::vector<Value> deps;
            for (int i = 0; i < count; ++i) {
                deps.push_back(Value{"phantom" + std::to_string(i)});
            }
            return module_record(id, "loaded", std::move(deps));
        };
        modules.push_back(with_deps("alpha", alpha_deps));
        modules.push_back(with_deps("beta", beta_deps));
        return snapshot_of(std::move(modules));
    };

    Observatory observatory{};
    observatory.ingest_kernel_payload(kernel_of("available", "saturated"), 0, true, 0);

    // Alpha is marginally the more connected of the two, so it takes the last seat.
    observatory.ingest_snapshot(scene(5, 4), 0, 0);
    const Frame first = observatory.tick(0);
    CHECK_EQ(first.nodes.size(), 400U);
    CHECK(drawn(first, "alpha"));
    CHECK(!drawn(first, "beta"));

    // They swap. The difference between them is far smaller than the incumbency margin,
    // so alpha keeps its seat -- which is the anti-flicker rule doing its job.
    observatory.ingest_snapshot(scene(4, 5), 0, 100);
    const Frame held = observatory.tick(100);
    CHECK_BECAUSE(drawn(held, "alpha") && !drawn(held, "beta"),
                  "swapping on a difference this small is exactly the blink the incumbency "
                  "margin exists to prevent");

    // Now the operator changes view. Whatever alpha earned in Cerebra it does not carry
    // into Watcher, so the seat goes to whoever deserves it there.
    observatory.request_mode(SceneMode::Watcher);
    const Frame after = observatory.tick(116);
    CHECK_BECAUSE(drawn(after, "beta") && !drawn(after, "alpha"),
                  "carrying a claim across a mode change privileges nodes that mattered in a "
                  "view the operator has left, and there is nothing on screen explaining why "
                  "the less important of two nodes is the one being shown");
}

void nothing_but_the_operator_changes_the_view() {
    CASE("no arriving payload reorganises the screen");

    Observatory observatory{};
    for (int i = 0; i < 50; ++i) {
        // A stream of arrivals, departures and severity changes.
        observatory.ingest_snapshot(
            snapshot_of({module_record("m" + std::to_string(i), i % 3 == 0 ? "invalid" : "loaded"),
                         module_record("steady", "denied")}),
            0, i * 100);
        observatory.tick(i * 100);
    }

    CHECK_BECAUSE(observatory.tick(10'000).mode == SceneMode::Cerebra,
                  "a display that reorganises itself because something happened elsewhere "
                  "takes the screen away from whoever was reading it, at the moment they were "
                  "most likely reading it for a reason");
}

void an_absurd_magnitude_cannot_swallow_the_scene() {
    CASE("scale is bounded whatever the provider says");

    Observatory observatory{};
    std::vector<Value> deps;
    for (int i = 0; i < 400; ++i) {
        deps.push_back(Value{"d" + std::to_string(i)});
    }
    observatory.ingest_snapshot(snapshot_of({module_record("hub", "loaded", std::move(deps))}), 0,
                                0);

    const Frame frame = observatory.tick(0);
    const DrawnNode* hub = node_in(frame, "hub");
    CHECK(hub != nullptr);
    CHECK_BECAUSE(hub->scale < 2.0F,
                  "multiplying a node's size by an unbounded provider number means one bad "
                  "reading produces a node that swallows the scene and hides everything the "
                  "operator was looking at");
    CHECK(hub->scale > 1.0F);
}

void an_edge_is_never_drawn_into_empty_space() {
    CASE("edges only between nodes that survived the budget");

    ObservatoryPolicy policy{};
    Observatory observatory{policy};

    std::vector<Value> modules;
    modules.push_back(module_record("root", "loaded"));
    for (int i = 0; i < 2'000; ++i) {
        modules.push_back(module_record("leaf" + std::to_string(i), "loaded", {Value{"root"}}));
    }
    observatory.ingest_snapshot(snapshot_of(std::move(modules)), 0, 0);
    observatory.ingest_kernel_payload(kernel_of("available", "saturated"), 0, true, 0);

    const Frame frame = observatory.tick(0);
    CHECK(static_cast<int>(frame.nodes.size()) <= frame.profile.max_visible_nodes);

    std::vector<std::string> visible;
    for (const DrawnNode& node : frame.nodes) {
        visible.push_back(node.id);
    }
    std::sort(visible.begin(), visible.end());

    bool all_anchored = true;
    for (const DrawnEdge& edge : frame.edges) {
        if (!std::binary_search(visible.begin(), visible.end(), edge.from) ||
            !std::binary_search(visible.begin(), visible.end(), edge.to)) {
            all_anchored = false;
        }
    }
    CHECK_BECAUSE(all_anchored,
                  "an edge to a node the budget aggregated away is a line into empty space, "
                  "claiming a relationship with something the operator cannot see or inspect");
}

void focusing_something_that_is_not_there_does_nothing() {
    CASE("the camera does not dive at a point no node occupies");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("real", "loaded")}), 0, 0);
    const Frame before = observatory.tick(0);

    observatory.focus("imaginary");
    const Frame after = observatory.tick(16);

    CHECK(after.focused.empty());
    CHECK_BECAUSE(after.camera_state == before.camera_state,
                  "a focus request for a node that does not exist would otherwise leave the "
                  "inspector naming something the scene cannot show");
}

void a_clock_that_steps_backwards_produces_a_frame_that_does_not_move() {
    CASE("time is handled once, here, rather than in five places");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("a", "loaded")}), 0, 0);
    observatory.tick(1'000'000);
    observatory.request_mode(SceneMode::Fabric);
    observatory.tick(1'000'100);
    const Frame before = observatory.tick(1'000'200);

    const Frame rolled_back = observatory.tick(900'000);
    CHECK_BECAUSE(rolled_back.morph_progress == before.morph_progress,
                  "a backwards step must advance nothing rather than unwinding a transition "
                  "the viewer has already watched");
    CHECK(rolled_back.camera == before.camera);
    CHECK_BECAUSE(!rolled_back.nodes.empty(),
                  "a frame is still produced; the scene does not blank because the clock "
                  "misbehaved");

    // And the next frame measures from the rolled-back now. Keeping the old epoch would
    // freeze every transition until the clock climbed back to where it had been, which
    // after a large correction means an observatory that is visibly stuck for minutes
    // with no indication why.
    const Frame resumed = observatory.tick(900'600);
    CHECK(resumed.morph_progress > rolled_back.morph_progress);
    CHECK_EQ(resumed.incoming_mode, SceneMode::Fabric);
}

void the_first_tick_does_not_jump() {
    CASE("startup has no elapsed time to spend");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("a", "loaded")}), 0, 0);

    // Something has to be in flight for this to mean anything: a first tick that spends
    // its whole timestamp is invisible in a scene where nothing was moving anyway.
    observatory.request_mode(SceneMode::Fabric);
    const Frame frame = observatory.tick(9'999'999);

    CHECK_BECAUSE(frame.morph_progress == 0.0F,
                  "a wall-clock timestamp is not an elapsed time; treating the first one as a "
                  "delta spends nearly three hours of animation in one frame, and every "
                  "transition the observatory opens with completes before it is drawn");
    CHECK_EQ(frame.mode, SceneMode::Cerebra);
    CHECK_EQ(frame.incoming_mode, SceneMode::Fabric);

    // And the second tick measures from the first, not from zero.
    const Frame next = observatory.tick(10'000'599);
    CHECK_EQ(next.morph_progress, 0.5F);
}

void a_tangled_registry_is_surfaced() {
    CASE("a dependency cycle reaches the frame instead of being absorbed");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({
                                    module_record("a", "loaded", {Value{"b"}}),
                                    module_record("b", "loaded", {Value{"a"}}),
                                    module_record("fine", "loaded"),
                                }),
                                0, 0);

    const Frame frame = observatory.tick(0);
    CHECK_EQ(frame.dependency_cycle.size(), 2U);
    CHECK_BECAUSE(frame.nodes.size() == 3U,
                  "and the scene still draws: a registry bug must not cost the operator the "
                  "whole display");
}

void a_defective_payload_reaches_the_frame_as_a_count() {
    CASE("the scene can say it is incomplete");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({
                                    module_record("good", "loaded"),
                                    Value{"not a record"},
                                    Value::object({member("state", Value{"loaded"})}),
                                }),
                                0, 0);

    const Frame frame = observatory.tick(0);
    CHECK_EQ(frame.nodes.size(), 1U);
    CHECK_BECAUSE(frame.ingest.dropped == 2,
                  "a scene missing two of its nodes must say so rather than looking complete");
    CHECK(!frame.ingest.clean());
}

void colour_reaches_the_frame_fully_resolved() {
    CASE("the renderer applies a colour rather than deriving one");

    Observatory observatory{};
    observatory.ingest_snapshot(
        snapshot_of({module_record("broken", "invalid"), module_record("fine", "loaded")}), 0, 0);
    const Frame frame = observatory.tick(0);

    const DrawnNode* broken = node_in(frame, "broken");
    const DrawnNode* fine = node_in(frame, "fine");
    CHECK(broken != nullptr && fine != nullptr);
    CHECK_BECAUSE(luminance(broken->colour.base) > luminance(fine->colour.base),
                  "severity has to be readable without colour vision, and that property is "
                  "worth nothing if it stops at the edge of the palette module");
    CHECK(broken->colour.emissive > fine->colour.emissive);

    // The same node from a snapshot too old to vouch for keeps what it says and loses
    // only the confidence with which it is said.
    Observatory stale{};
    stale.ingest_snapshot(snapshot_of({module_record("broken", "invalid")}), 240'000, 0);
    const DrawnNode* aged = node_in(stale.tick(0), "broken");
    CHECK(aged != nullptr);
    CHECK_BECAUSE(aged->colour.base == broken->colour.base,
                  "a stale critical node is still a critical node all the way to the renderer");
    CHECK(aged->colour.alpha < broken->colour.alpha);
    CHECK_EQ(aged->colour.emissive, 0.0F);
}

void the_timeline_remembers_what_was_true() {
    CASE("scrubbing back finds the moment something broke");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("m", "loaded")}), 0, 1'000);
    observatory.tick(1'000);

    observatory.ingest_snapshot(snapshot_of({module_record("m", "invalid")}), 0, 5'000);
    observatory.tick(5'000);

    observatory.ingest_snapshot(snapshot_of({module_record("m", "loaded")}), 0, 9'000);
    observatory.tick(9'000);

    const History& history = observatory.history();

    const HistoryEntry* before = history.at_or_before(4'000);
    CHECK(before != nullptr);
    CHECK_BECAUSE(before->nodes[0].severity == 0,
                  "at four seconds the module had not broken yet, and the timeline must say so "
                  "rather than reporting the state it reached a second later");

    const HistoryEntry* during = history.at_or_before(7'000);
    CHECK(during != nullptr);
    CHECK_BECAUSE(during->nodes[0].severity == 3,
                  "the whole point of a timeline is that a transient failure is still there to "
                  "be found after it has cleared");

    const HistoryEntry* after = history.at_or_before(20'000);
    CHECK(after != nullptr && after->nodes[0].severity == 0);
    CHECK_EQ(history.at_or_before(0), nullptr);
}

void the_timeline_records_nodes_the_budget_hid() {
    CASE("what was true, not what fitted on screen");

    Observatory observatory{};
    observatory.ingest_snapshot(busy_snapshot(3'000), 0, 0);
    observatory.ingest_kernel_payload(kernel_of("available", "saturated"), 0, true, 0);
    const Frame frame = observatory.tick(0);

    CHECK(frame.nodes.size() < 3'000U);
    const HistoryEntry* entry = observatory.history().at_or_before(0);
    CHECK(entry != nullptr);
    CHECK_BECAUSE(entry->nodes.size() == 3'000U,
                  "a timeline that only remembered the visible nodes would show a quieter past "
                  "than the one that happened, and the aggregated ones are exactly the ones an "
                  "operator scrubs back to look for");
    CHECK_EQ(entry->aggregated, frame.aggregated);
}

Value observation(const std::string& id, const char* grade, double magnitude,
                  double latitude, double longitude, bool authoritative = true) {
    return Value::object({
        member("id", Value{id}),
        member("position", Value::object({member("latitude", Value{latitude}),
                                          member("longitude", Value{longitude})})),
        member("summary", Value{id}),
        member("details", Value::object({member("kind", Value{"earthquake"}),
                                         member("magnitude", Value{magnitude}),
                                         member("severity", Value{grade})})),
        member("provenance", Value::object({member("authoritative", Value{authoritative})})),
    });
}

Value world_of(std::vector<Value> observations) {
    return Value::object({member("observations", Value::array(std::move(observations)))});
}

float distance_from_origin(const Vec3& v) {
    return std::sqrt(v.x * v.x + v.y * v.y + v.z * v.z);
}

void the_two_projections_do_not_evict_each_other() {
    CASE("a world payload is not a statement about modules, or the reverse");

    ObservatoryPolicy policy{};
    policy.graph.departure_grace_ms = 1000;
    Observatory observatory{policy};

    observatory.ingest_snapshot(
        snapshot_of({module_record("core.archie", "loaded"), module_record("core.memory", "loaded")}),
        0, 0);
    observatory.ingest_world_payload(world_of({observation("q1", "strong", 6.2, 35.0, 139.0)}), 0,
                                     100);

    const Frame frame = observatory.tick(100);
    CHECK_BECAUSE(frame.total_nodes == 3,
                  "the world daemon and the control plane describe different parts of one scene "
                  "and arrive on different schedules; treating either payload as the whole truth "
                  "starts a grace period on everything the other one owns");
    CHECK(drawn(frame, "core.archie") && drawn(frame, "core.memory") && drawn(frame, "q1"));

    // And they survive the grace they were never supposed to enter.
    const Frame later = observatory.tick(5000);
    CHECK_EQ(later.total_nodes, 3);
    for (const DrawnNode& node : later.nodes) {
        CHECK_BECAUSE(node.presence != NodePresence::Departing,
                      "nothing here has left, so nothing should be dimming");
    }

    // The other direction, which is the one an unbalanced test would miss: the control
    // plane polls far more often than the world feed, so a module snapshot arriving on
    // top of an established globe is the common case, not the rare one.
    observatory.ingest_snapshot(snapshot_of({module_record("core.archie", "loaded"),
                                             module_record("core.memory", "loaded")}),
                                0, 6000);
    const Frame after_modules = observatory.tick(6000);
    CHECK_EQ(after_modules.total_nodes, 3);
    CHECK(drawn(after_modules, "q1"));
    CHECK_BECAUSE(node_in(after_modules, "q1") != nullptr &&
                      node_in(after_modules, "q1")->presence != NodePresence::Departing,
                  "a snapshot of the control plane says nothing about earthquakes, and reading "
                  "it as though it did would make the globe empty itself every few seconds");

    observatory.tick(9000);
    CHECK_BECAUSE(observatory.graph().find("q1") != nullptr,
                  "and it must still be there once the grace it never entered has elapsed");
}

void an_observation_that_falls_off_the_feed_departs() {
    CASE("the world payload is still authoritative about the world");

    ObservatoryPolicy policy{};
    policy.graph.departure_grace_ms = 1000;
    Observatory observatory{policy};

    observatory.ingest_snapshot(snapshot_of({module_record("core.archie", "loaded")}), 0, 0);
    observatory.ingest_world_payload(world_of({observation("old", "minor", 3.0, 0.0, 0.0),
                                               observation("new", "strong", 6.0, 10.0, 20.0)}),
                                     0, 100);
    observatory.tick(100);

    // The next batch has dropped the older quake, as a rolling feed does.
    observatory.ingest_world_payload(world_of({observation("new", "strong", 6.0, 10.0, 20.0)}), 0,
                                     200);
    const Frame frame = observatory.tick(200);

    const DrawnNode* old_quake = node_in(frame, "old");
    CHECK(old_quake != nullptr);
    CHECK_EQ(old_quake->presence, NodePresence::Departing);
    CHECK_BECAUSE(drawn(frame, "core.archie"),
                  "and the modules are untouched by a change in the weather");

    // A module snapshot arrives while the quake is mid-departure. Carrying every node
    // the other projection owns must not include the ones already on their way out, or a
    // busy control plane would resurrect anything the world feed had dropped and nothing
    // would ever leave the globe.
    observatory.ingest_snapshot(snapshot_of({module_record("core.archie", "loaded")}), 0, 250);
    const Frame during = observatory.tick(250);
    CHECK_BECAUSE(node_in(during, "old")->presence == NodePresence::Departing,
                  "the world feed said it was gone, and an unrelated poll of the control plane "
                  "is not new evidence that it is back");

    observatory.tick(3000);
    CHECK_EQ(observatory.graph().find("old"), nullptr);
    CHECK(observatory.graph().find("core.archie") != nullptr);
}

void observations_are_placed_on_the_globe() {
    CASE("a real place rather than a shell position");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("core.archie", "loaded")}), 0, 0);
    observatory.ingest_world_payload(world_of({observation("north", "minor", 0.0, 90.0, 0.0),
                                               observation("gulf", "minor", 0.0, 0.0, 0.0)}),
                                     0, 0);

    const Frame frame = observatory.tick(0);
    const DrawnNode* north = node_in(frame, "north");
    const DrawnNode* gulf = node_in(frame, "gulf");
    const DrawnNode* module = node_in(frame, "core.archie");
    CHECK(north != nullptr && gulf != nullptr && module != nullptr);

    const ObservatoryPolicy defaults{};
    const float surface = defaults.layout.globe.radius + defaults.layout.globe.base_elevation;
    CHECK(std::fabs(distance_from_origin(north->position) - surface) < 0.05F);
    CHECK_BECAUSE(north->position.y > surface - 0.05F,
                  "the north pole is at the top of the globe, and a shell position would put it "
                  "wherever its slot happened to fall");
    // Longitude 0 faces +Z, matching Qt's native sphere UV map so the texture and the
    // markers share one frame. This check used to read `.x`, from the convention before
    // that alignment; it passed for the wrong reason and would have kept passing with the
    // markers a quarter-turn off the coastline they name.
    CHECK(std::fabs(gulf->position.z - surface) < 0.05F);
    CHECK_BECAUSE(std::fabs(gulf->position.x) < 0.05F,
                  "the Gulf of Guinea is on the prime meridian, so it has no eastward "
                  "component at all");

    CHECK_BECAUSE(!module->location.has_value(),
                  "a module has no latitude, and inventing one would put it somewhere on a globe "
                  "as though it meant something");
    CHECK(north->location.has_value());
}

void inspecting_an_observation_flies_to_where_it_is() {
    CASE("the camera frames a quake at its coordinates, not at a slot");

    Observatory observatory{};
    observatory.ingest_world_payload(world_of({observation("q", "strong", 6.5, -33.87, 151.21)}),
                                     0, 0);
    observatory.tick(0);

    observatory.focus("q");
    for (int i = 1; i <= 100; ++i) {
        observatory.tick(i * 16);
    }
    const Frame frame = observatory.tick(2000);

    CHECK_EQ(frame.camera_state, CameraState::Focused);
    const DrawnNode* node = node_in(frame, "q");
    CHECK(node != nullptr);
    CHECK_BECAUSE(frame.camera.target == node->position,
                  "the camera has to arrive looking at the marker; framing a slot position would "
                  "leave the operator staring at empty space beside a populated inspector");
}

void a_second_hand_report_is_drawn_more_faintly() {
    CASE("authority reaches the renderer as a resolved opacity and a separate fact");

    Observatory observatory{};
    observatory.ingest_world_payload(
        world_of({observation("firsthand", "strong", 6.0, 10.0, 10.0, true),
                  observation("secondhand", "strong", 6.0, 20.0, 20.0, false)}),
        0, 0);

    const Frame frame = observatory.tick(0);
    const DrawnNode* first = node_in(frame, "firsthand");
    const DrawnNode* second = node_in(frame, "secondhand");
    CHECK(first != nullptr && second != nullptr);

    CHECK(first->authoritative && !second->authoritative);
    CHECK(second->colour.alpha < first->colour.alpha);
    CHECK_BECAUSE(second->colour.base == first->colour.base,
                  "both are reports of a strong earthquake, and one of them being second-hand "
                  "does not make it a smaller one");
    CHECK_EQ(second->freshness, first->freshness);
}

void the_two_ingest_reports_stay_separate() {
    CASE("a defect in the world feed is not a defect in the control plane");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("fine", "loaded")}), 0, 0);
    observatory.ingest_world_payload(
        world_of({observation("ok", "minor", 3.0, 0.0, 0.0), Value{"rubbish"}}), 0, 0);

    const Frame frame = observatory.tick(0);
    CHECK_BECAUSE(frame.ingest.clean(),
                  "the control plane sent a perfectly good snapshot, and saying otherwise would "
                  "send an operator to debug the wrong daemon");
    CHECK_EQ(frame.world_ingest.dropped, 1);
    CHECK(!frame.world_ingest.clean());
}

void a_departing_module_is_not_revived_by_the_world_feed() {
    CASE("the mirror case: neither projection resurrects what the other retired");

    ObservatoryPolicy policy{};
    policy.graph.departure_grace_ms = 1000;
    Observatory observatory{policy};

    observatory.ingest_snapshot(snapshot_of({module_record("staying", "loaded"),
                                             module_record("unloading", "loaded")}),
                                0, 0);
    observatory.ingest_world_payload(world_of({observation("q", "minor", 3.0, 0.0, 0.0)}), 0, 0);
    observatory.tick(0);

    // The module is unloaded, so the control plane stops reporting it.
    observatory.ingest_snapshot(snapshot_of({module_record("staying", "loaded")}), 0, 100);
    const Frame departing = observatory.tick(100);
    const DrawnNode* leaving = node_in(departing, "unloading");
    CHECK(leaving != nullptr);
    CHECK_EQ(leaving->presence, NodePresence::Departing);

    // A world batch lands mid-departure. It knows nothing about modules.
    observatory.ingest_world_payload(world_of({observation("q", "minor", 3.0, 0.0, 0.0)}), 0, 200);
    const Frame during = observatory.tick(200);
    const DrawnNode* still_leaving = node_in(during, "unloading");
    CHECK(still_leaving != nullptr);
    CHECK_BECAUSE(still_leaving->presence == NodePresence::Departing,
                  "an earthquake feed is not evidence that a module came back, and carrying "
                  "departing nodes across would mean nothing ever finishes leaving on a node "
                  "where either daemon is polling");

    observatory.tick(3000);
    CHECK_EQ(observatory.graph().find("unloading"), nullptr);
    CHECK(observatory.graph().find("staying") != nullptr);
    CHECK_BECAUSE(observatory.graph().find("q") != nullptr,
                  "and the quake, which nothing retired, is untouched by all of it");
}

void peers_arrive_with_the_modules_they_share_a_payload_with() {
    CASE("one snapshot, two kinds, one reconciliation");

    Observatory observatory{};
    observatory.ingest_snapshot(
        snapshot_of({module_record("core.archie", "loaded")},
                    {peer_record("node-b", "ok", true, {Value{"compute.cuda"}}),
                     peer_record("node-c", "degraded", true)}),
        0, 0);

    const Frame frame = observatory.tick(0);
    CHECK_EQ(frame.total_nodes, 3);
    CHECK(frame.ingest.clean());

    const DrawnNode* peer = node_in(frame, "node-b");
    CHECK(peer != nullptr);
    CHECK_BECAUSE(peer->shape == NodeShape::Octahedron,
                  "a peer has to be tellable from a module without reading the label, since "
                  "colour is already carrying severity and freshness");
    CHECK_BECAUSE(peer->label == std::string{"node-b.local"},
                  "the hostname is what an operator calls the machine; the node id is what "
                  "the fabric calls it, and is the fallback rather than the first choice");
    CHECK_EQ(peer->severity, 0);
    CHECK_EQ(node_in(frame, "node-c")->severity, 1);
}

void an_unreachable_peer_is_not_a_broken_one() {
    CASE("no recent contact is not the same as known to be failing");

    bool recognised = false;
    const int unreachable = severity_for_peer("unreachable", false, recognised);
    const int unhealthy = severity_for_peer("unhealthy", true, recognised);
    const int degraded = severity_for_peer("degraded", true, recognised);

    CHECK_BECAUSE(unreachable > degraded && unreachable < unhealthy,
                  "the upstream enum documents Unreachable as explicitly not Unhealthy, because "
                  "we do not know that it is broken; collapsing them either raises a false "
                  "alarm every time a link flaps or hides a node that is genuinely down");
    CHECK(recognised);
}

void a_peer_that_is_healthy_and_unreachable_is_read_cautiously() {
    CASE("a payload that contradicts itself resolves the safe way");

    bool recognised = true;
    CHECK_BECAUSE(severity_for_peer("ok", false, recognised) >= 2,
                  "the snapshot said healthy and also said we cannot reach it; telling an "
                  "operator a node they cannot talk to is fine is the one reading that is "
                  "definitely wrong");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({}, {peer_record("ghost", "ok", false)}), 0, 0);
    CHECK(node_in(observatory.tick(0), "ghost")->severity >= 2);
}

void a_peer_with_no_reachable_flag_is_not_assumed_reachable() {
    CASE("absent means we were not told, not that the answer is yes");

    Observatory observatory{};
    observatory.ingest_snapshot(
        snapshot_of({}, {Value::object({member("node_id", Value{"quiet"}),
                                        member("health", Value{"ok"})})}),
        0, 0);

    const Frame frame = observatory.tick(0);
    CHECK(node_in(frame, "quiet") != nullptr);
    CHECK(node_in(frame, "quiet")->severity >= 2);
    CHECK_EQ(frame.ingest.count_of(DefectKind::MissingField), 1);
}

void each_view_shows_what_it_is_about() {
    CASE("four readings of the node, not four camera angles on one scene");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("core.archie", "loaded")},
                                            {peer_record("node-b", "ok", true)}),
                                0, 0);
    observatory.ingest_world_payload(world_of({observation("q", "strong", 6.0, 0.0, 0.0)}), 0, 0);

    const Frame everything = observatory.tick(0);
    CHECK_BECAUSE(everything.nodes.size() == 3U,
                  "Cerebra is the view of record: a thing missing from it is missing from the "
                  "observatory");
    CHECK_EQ(everything.filtered_out, 0);

    const auto settle_into = [&](SceneMode mode, std::int64_t from) {
        observatory.request_mode(mode);
        for (std::int64_t t = from; t <= from + 4000; t += 100) {
            observatory.tick(t);
        }
        return observatory.tick(from + 4000);
    };

    const Frame watcher = settle_into(SceneMode::Watcher, 100);
    CHECK_EQ(watcher.mode, SceneMode::Watcher);
    CHECK(drawn(watcher, "q"));
    CHECK_BECAUSE(!drawn(watcher, "core.archie") && !drawn(watcher, "node-b"),
                  "showing the registry alongside the globe drowns the part the view exists to "
                  "isolate");
    CHECK_BECAUSE(watcher.filtered_out == 2,
                  "and it is reported as filtered rather than aggregated: one was never meant "
                  "to be here, the other did not fit, and sending an operator to look for a "
                  "budget problem that does not exist wastes their time");
    CHECK_EQ(watcher.aggregated, 0);

    const Frame fabric = settle_into(SceneMode::Fabric, 5000);
    CHECK(drawn(fabric, "node-b"));
    CHECK(!drawn(fabric, "q") && !drawn(fabric, "core.archie"));

    const Frame brain = settle_into(SceneMode::Brain, 10000);
    CHECK(drawn(brain, "core.archie"));
    CHECK(!drawn(brain, "q") && !drawn(brain, "node-b"));

    const Frame back = settle_into(SceneMode::Cerebra, 15000);
    CHECK_EQ(back.nodes.size(), 3U);
    CHECK_EQ(back.filtered_out, 0);
}

void a_morph_shows_both_ends() {
    CASE("a node arriving in the incoming view is present before the transition ends");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("core.archie", "loaded")},
                                            {peer_record("node-b", "ok", true)}),
                                0, 0);
    observatory.ingest_world_payload(world_of({observation("q", "strong", 6.0, 0.0, 0.0)}), 0, 0);
    observatory.tick(0);

    // Deliberately between two restrictive views. A morph out of Cerebra proves nothing
    // here, because Cerebra already shows everything: the frame would look identical
    // whether both ends were consulted or only the one being left.
    observatory.request_mode(SceneMode::Watcher);
    for (std::int64_t t = 100; t <= 4000; t += 100) {
        observatory.tick(t);
    }
    const Frame settled = observatory.tick(4000);
    CHECK_EQ(settled.mode, SceneMode::Watcher);
    CHECK(drawn(settled, "q") && !drawn(settled, "node-b"));

    observatory.request_mode(SceneMode::Fabric);
    const Frame during = observatory.tick(4016);

    CHECK(during.morph_progress < 1.0F);
    CHECK_EQ(during.mode, SceneMode::Watcher);
    CHECK_EQ(during.incoming_mode, SceneMode::Fabric);
    CHECK_BECAUSE(drawn(during, "node-b"),
                  "the peer belongs to the view being entered, and consulting only the one "
                  "being left would make it appear the instant the transition ended rather "
                  "than fading in over it");
    CHECK_BECAUSE(drawn(during, "q"),
                  "and the quake is still on screen while the view it belongs to is being left, "
                  "so it can fade out instead of vanishing");
    CHECK_BECAUSE(during.filtered_out == 1,
                  "the module belongs to neither end and stays out of both");
}

void the_scene_survives_a_view_with_nothing_in_it() {
    CASE("an empty view is a legitimate answer");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("core.archie", "loaded")}), 0, 0);
    observatory.tick(0);

    observatory.request_mode(SceneMode::Watcher);
    for (std::int64_t t = 100; t <= 4000; t += 100) {
        observatory.tick(t);
    }
    const Frame frame = observatory.tick(4000);

    CHECK(frame.nodes.empty());
    CHECK_BECAUSE(frame.filtered_out == 1,
                  "a globe with no observations is not a broken display, and the count is what "
                  "lets it say so rather than looking like it failed to load");
    CHECK_EQ(frame.total_nodes, 1);
}

void models_arrive_with_the_payload_they_share() {
    CASE("a model is a thing on screen, told apart by its form");

    Observatory observatory{};
    observatory.ingest_snapshot(
        snapshot_of({module_record("core.archie", "loaded")}, {},
                    {model_record("local.reasoner", "ready", 128000),
                     model_record("remote.big", "offline", 200000)}),
        0, 0);

    const Frame frame = observatory.tick(0);
    CHECK_EQ(frame.total_nodes, 3);
    CHECK(frame.ingest.clean());

    const DrawnNode* ready = node_in(frame, "local.reasoner");
    const DrawnNode* offline = node_in(frame, "remote.big");
    CHECK(ready != nullptr && offline != nullptr);
    CHECK_BECAUSE(ready->shape == NodeShape::Capsule,
                  "a model, a module and a peer have to be tellable apart by form alone, since "
                  "colour is already spoken for by severity and freshness");
    CHECK_EQ(ready->severity, 0);
    CHECK_EQ(offline->severity, 2);
}

void model_availability_is_graded_like_everything_else() {
    CASE("deliberate is not a problem, unreachable is not broken");

    bool recognised = false;
    CHECK_BECAUSE(severity_for_model("ready", recognised) == 0 &&
                      severity_for_model("cold", recognised) == 0,
                  "upstream calls both usable: cold is registered but not resident, which costs "
                  "latency on the first call and nothing else");
    CHECK_BECAUSE(severity_for_model("disabled", recognised) == 0,
                  "a model nobody asked to run is behaving correctly, the same as a disabled "
                  "module");
    CHECK_BECAUSE(severity_for_model("offline", recognised) == 2,
                  "temporarily unreachable is a statement about what we cannot see rather than "
                  "about what is broken, and it grades like an unreachable peer");
    CHECK(recognised);

    CHECK_EQ(severity_for_model("quantum-superposed", recognised), 1);
    CHECK(!recognised);
}

void a_context_window_is_a_size_that_stays_comparable() {
    CASE("thousands of tokens, not raw tokens");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({}, {},
                                            {model_record("small", "ready", 4096),
                                             model_record("large", "ready", 200000)}),
                                0, 0);

    const Frame frame = observatory.tick(0);
    const DrawnNode* small = node_in(frame, "small");
    const DrawnNode* large = node_in(frame, "large");
    CHECK(small != nullptr && large != nullptr);
    CHECK_BECAUSE(large->scale - small->scale > 0.05F,
                  "a visible gap, not merely a different float. Raw token counts still compare "
                  "strictly greater, so asserting only that would pass while a 4k model and a "
                  "200k one were drawn at sizes no viewer could tell apart: both land within a "
                  "thousandth of the top of the bounded scale");
    CHECK(large->scale < 2.0F);
}

void a_model_belongs_to_the_cognition_view() {
    CASE("Brain is about the thinking part, and models are part of it");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("core.archie", "loaded")},
                                            {peer_record("node-b", "ok", true)},
                                            {model_record("local.reasoner", "ready", 128000)}),
                                0, 0);
    observatory.tick(0);

    observatory.request_mode(SceneMode::Brain);
    for (std::int64_t t = 100; t <= 4000; t += 100) {
        observatory.tick(t);
    }
    const Frame brain = observatory.tick(4000);

    CHECK_EQ(brain.mode, SceneMode::Brain);
    CHECK(drawn(brain, "local.reasoner") && drawn(brain, "core.archie"));
    CHECK_BECAUSE(!drawn(brain, "node-b"),
                  "the fabric is a different question, and answering it here would drown the "
                  "one this view exists to ask");
    CHECK_EQ(brain.filtered_out, 1);
}

void the_timeline_records_a_departure() {
    CASE("the moment something started leaving is in the timeline");

    ObservatoryPolicy policy{};
    policy.graph.departure_grace_ms = 5000;
    // Long enough that a keyframe cannot be what saves this: if the entry is recorded, it
    // is because the departure itself was noticed.
    policy.history.keyframe_interval_ms = 600000;
    Observatory observatory{policy};

    observatory.ingest_snapshot(
        snapshot_of({module_record("staying", "loaded"), module_record("leaving", "loaded")}), 0,
        0);
    observatory.tick(0);
    observatory.tick(2000);

    // It drops out of the payload. Nothing else about the scene changes: the node count
    // is the same, the tier is the same, the mode is the same, and no spec moved -- the
    // only difference in the world is that one node is now on its way out.
    observatory.ingest_snapshot(snapshot_of({module_record("staying", "loaded")}), 0, 3000);
    const Frame frame = observatory.tick(3000);
    CHECK_EQ(node_in(frame, "leaving")->presence, NodePresence::Departing);
    CHECK_EQ(frame.total_nodes, 2);

    const HistoryEntry* entry = observatory.history().at_or_before(3000);
    CHECK(entry != nullptr);

    bool recorded_as_departing = false;
    for (const NodeSample& sample : entry->nodes) {
        if (sample.id == "leaving" && sample.presence == NodePresence::Departing) {
            recorded_as_departing = true;
        }
    }
    CHECK_BECAUSE(recorded_as_departing,
                  "a node beginning to depart is exactly the kind of moment someone scrubs "
                  "back to find, and it changes nothing the frame reports as a scalar -- so "
                  "any cheap pre-check that decides whether to record has to notice it too");
}

void positions_track_the_graph_after_the_first_frame() {
    CASE("layout is cached, and the cache is not allowed to go stale");

    Observatory observatory{};

    // Everything so far arrives before anything is drawn, which is the case every other
    // test exercises -- and the one case a cache that never invalidates would survive.
    observatory.ingest_snapshot(snapshot_of({module_record("base", "loaded")}), 0, 0);
    const Frame first = observatory.tick(0);
    CHECK_EQ(first.nodes.size(), 1U);
    const Vec3 base_at_first = node_in(first, "base")->position;

    // A node arrives after the scene has already been drawn once.
    observatory.ingest_snapshot(
        snapshot_of({module_record("base", "loaded"),
                     module_record("built.on.base", "loaded", {Value{"base"}})}),
        0, 1000);
    const Frame second = observatory.tick(1000);

    const DrawnNode* newcomer = node_in(second, "built.on.base");
    CHECK(newcomer != nullptr);
    const float radius = std::sqrt(newcomer->position.x * newcomer->position.x +
                                   newcomer->position.z * newcomer->position.z);
    CHECK_BECAUSE(radius > 0.1F,
                  "a node that arrived after the first frame has to be placed; a layout cache "
                  "that never invalidated would leave it at the origin, stacked on the centre "
                  "of the scene with nothing saying why");

    const float base_radius = std::sqrt(base_at_first.x * base_at_first.x +
                                        base_at_first.z * base_at_first.z);
    CHECK_BECAUSE(radius > base_radius,
                  "and placed where its dependency depth puts it, one shell further out than "
                  "the thing it is built on");

    // A node that gets worse has to rise, on a frame where nothing arrived or left.
    observatory.ingest_snapshot(
        snapshot_of({module_record("base", "invalid"),
                     module_record("built.on.base", "loaded", {Value{"base"}})}),
        0, 2000);
    const Frame third = observatory.tick(2000);

    const DrawnNode* worse = node_in(third, "base");
    CHECK(worse != nullptr);
    CHECK_EQ(worse->severity, 3);
    CHECK_BECAUSE(worse->position.y > base_at_first.y,
                  "severity lifts a node out of the plane, and a stale cache would leave a "
                  "module that has just broken sitting exactly where it sat when it was fine");
    CHECK_EQ(third.edges.size(), 1U);

    // Edges are cached against the same counter, and the interesting case is a change
    // from one set of edges to another rather than from none to some: a cache that only
    // filled itself when empty would survive the second and fail here.
    observatory.ingest_snapshot(
        snapshot_of({module_record("base", "invalid"),
                     module_record("built.on.base", "loaded", {Value{"base"}}),
                     module_record("also.on.base", "loaded", {Value{"base"}})}),
        0, 3000);
    const Frame fourth = observatory.tick(3000);
    CHECK_BECAUSE(fourth.edges.size() == 2U,
                  "the new module declares a dependency that already has an edge beside it, so "
                  "a stale edge list still looks plausible -- it is simply missing a "
                  "relationship the payload declared");

    // And an edge that goes away goes away.
    observatory.ingest_snapshot(
        snapshot_of({module_record("base", "invalid"),
                     module_record("built.on.base", "loaded"),
                     module_record("also.on.base", "loaded", {Value{"base"}})}),
        0, 4000);
    CHECK_EQ(observatory.tick(4000).edges.size(), 1U);
}

void a_node_reports_how_far_through_its_phase_it_is() {
    CASE("the renderer is told the progress rather than timing it again");

    ObservatoryPolicy policy{};
    policy.graph.entry_duration_ms = 1000;
    policy.graph.departure_grace_ms = 2000;
    Observatory observatory{policy};

    observatory.ingest_snapshot(snapshot_of({module_record("m", "loaded")}), 0, 0);

    const DrawnNode* arriving = node_in(observatory.tick(0), "m");
    CHECK(arriving != nullptr);
    CHECK_EQ(arriving->presence, NodePresence::Entering);
    CHECK_BECAUSE(arriving->presence_progress == 0.0F,
                  "it has just appeared, so its arrival has not started playing");

    const DrawnNode* halfway = node_in(observatory.tick(500), "m");
    CHECK(halfway != nullptr);
    CHECK_BECAUSE(halfway->presence_progress == 0.5F,
                  "the entry length is graph policy, and a renderer timing it separately would "
                  "be a second clock keeping the same appointment");

    observatory.ingest_snapshot(snapshot_of({module_record("m", "loaded")}), 0, 1000);
    const DrawnNode* settled = node_in(observatory.tick(1200), "m");
    CHECK(settled != nullptr);
    CHECK_EQ(settled->presence, NodePresence::Present);
    CHECK_BECAUSE(settled->presence_progress == 1.0F,
                  "a settled node is not partway through anything");

    // It drops out of the payload. The grace runs from the last sighting at t=1000, not
    // from the poll that noticed, so by the time anyone looks it is already partway
    // through -- which is exactly what a renderer needs, because the window it has left
    // to fade in is the window the model will actually wait.
    observatory.ingest_snapshot(snapshot_of({}), 0, 1200);
    const DrawnNode* leaving = node_in(observatory.tick(2000), "m");
    CHECK(leaving != nullptr);
    CHECK_EQ(leaving->presence, NodePresence::Departing);
    CHECK_BECAUSE(leaving->presence_progress == 0.5F,
                  "half the grace has elapsed, and a renderer fading over a full window from "
                  "here would still be drawing the node after it had been forgotten");

    // Forgotten at the end of that window, not before or after it.
    CHECK(observatory.graph().find("m") != nullptr);
    observatory.tick(3000);
    CHECK_EQ(observatory.graph().find("m"), nullptr);
}

void presence_progress_survives_a_clock_that_misbehaves() {
    CASE("a backwards clock does not interpolate a node through the floor");

    ObservatoryPolicy policy{};
    policy.graph.entry_duration_ms = 1000;
    Observatory observatory{policy};

    observatory.ingest_snapshot(snapshot_of({module_record("m", "loaded")}), 0, 1000000);
    observatory.tick(1000000);

    const Frame rolled_back = observatory.tick(900000);
    const DrawnNode* node = node_in(rolled_back, "m");
    CHECK(node != nullptr);
    CHECK_BECAUSE(node->presence_progress >= 0.0F && node->presence_progress <= 1.0F,
                  "a negative progress is not a phase that has not started, it is a number a "
                  "renderer will happily interpolate past the beginning of the animation");
    CHECK_EQ(node->presence_progress, 0.0F);
}

void changing_view_releases_something_that_view_does_not_contain() {
    CASE("the camera does not keep hold of a node the new view filters out");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("core.archie", "loaded")},
                                            {peer_record("node-b", "ok", true)}),
                                0, 0);
    observatory.tick(0);

    observatory.focus("core.archie");
    for (int i = 1; i <= 80; ++i) {
        observatory.tick(i * 16);
    }
    const Frame held = observatory.tick(2000);
    CHECK_EQ(held.focused, std::string{"core.archie"});
    CHECK(drawn(held, "core.archie"));

    // Fabric is about the other nodes. The module is not in it.
    observatory.request_mode(SceneMode::Fabric);

    // Let go on the request, not on the arrival. The operator has already said they are
    // done with what they were reading, and holding it until the morph finished would
    // fly them into the new view still carrying the old one.
    const Frame during = observatory.tick(2016);
    CHECK(during.morph_progress < 1.0F);
    CHECK_BECAUSE(during.focused.empty(),
                  "the destination decides, not the view being left");

    for (std::int64_t t = 2116; t <= 8000; t += 100) {
        observatory.tick(t);
    }
    const Frame after = observatory.tick(8000);

    CHECK_EQ(after.mode, SceneMode::Fabric);
    CHECK(!drawn(after, "core.archie"));
    CHECK_BECAUSE(after.focused.empty(),
                  "an inspector naming a node the scene is not drawing, beside a camera "
                  "pointed at the empty space where it would have been, is exactly the state "
                  "the removed-node rule exists to prevent -- a filtered node is no more on "
                  "screen than a forgotten one");
    CHECK_BECAUSE(after.camera_state == CameraState::Resting,
                  "and it has arrived somewhere, rather than being left mid-flight toward a "
                  "node it has let go of");
}

void a_node_the_view_does_not_contain_cannot_be_focused() {
    CASE("a click cannot have come from something that is not on screen");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("core.archie", "loaded")},
                                            {peer_record("node-b", "ok", true)}),
                                0, 0);
    observatory.tick(0);
    observatory.request_mode(SceneMode::Fabric);
    for (std::int64_t t = 100; t <= 4000; t += 100) {
        observatory.tick(t);
    }
    const Frame fabric = observatory.tick(4000);
    CHECK(!drawn(fabric, "core.archie"));

    observatory.focus("core.archie");
    const Frame after = observatory.tick(4016);
    CHECK_BECAUSE(after.focused.empty(),
                  "diving at a node this view filters out reaches the same empty patch of "
                  "space as diving at one that does not exist");
    CHECK_EQ(after.camera_state, CameraState::Resting);

    // A node the view does contain is still focusable.
    observatory.focus("node-b");
    CHECK_EQ(observatory.tick(4032).focused, std::string{"node-b"});
}

void an_event_the_view_cannot_show_is_counted_not_dropped() {
    CASE("something breaking elsewhere is neither pulsed nowhere nor forgotten");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("core.archie", "loaded")},
                                            {peer_record("node-b", "ok", true)}),
                                0, 0);
    observatory.tick(0);

    observatory.request_mode(SceneMode::Fabric);
    for (std::int64_t t = 100; t <= 4000; t += 100) {
        observatory.tick(t);
    }
    observatory.tick(4000);

    // A module breaks while the operator is reading the fabric.
    observatory.ingest_snapshot(snapshot_of({module_record("core.archie", "invalid")},
                                            {peer_record("node-b", "ok", true)}),
                                0, 5000);
    const Frame frame = observatory.tick(5000);

    CHECK(!drawn(frame, "core.archie"));
    CHECK_BECAUSE(frame.events.events.empty(),
                  "handing the renderer a pulse for a node it is not drawing gives it nowhere "
                  "to put it");
    CHECK_BECAUSE(frame.events_elsewhere == 1,
                  "and dropping it would mean an operator watching one view never learns that "
                  "something broke in another");

    // Every event whose node is on screen still arrives attached to it.
    observatory.ingest_snapshot(snapshot_of({module_record("core.archie", "invalid")},
                                            {peer_record("node-b", "unhealthy", true)}),
                                0, 6000);
    const Frame local = observatory.tick(6000);
    CHECK_EQ(local.events_elsewhere, 0);
    CHECK_EQ(local.events.events.size(), 1U);
    CHECK_EQ(local.events.events[0].node_id, std::string{"node-b"});
    CHECK(drawn(local, "node-b"));
}

void a_node_that_breaks_earns_its_way_onto_the_screen() {
    CASE("the budget hid it because it was quiet, and it is not quiet any more");

    Observatory observatory{};
    observatory.ingest_snapshot(busy_snapshot(600), 0, 0);
    observatory.ingest_kernel_payload(kernel_of("available", "saturated"), 0, true, 0);
    const Frame first = observatory.tick(0);
    CHECK_EQ(first.tier, RenderTier::Conserve);
    CHECK(static_cast<int>(first.nodes.size()) < first.total_nodes);

    // Find something the budget aggregated away, and break it.
    std::string hidden;
    for (int i = 0; i < 600 && hidden.empty(); ++i) {
        const std::string id = "m" + std::to_string(i);
        if (!drawn(first, id)) {
            hidden = id;
        }
    }
    CHECK(!hidden.empty());

    std::vector<Value> modules;
    for (int j = 0; j < 600; ++j) {
        const std::string id = "m" + std::to_string(j);
        modules.push_back(module_record(id, id == hidden ? "invalid" : "loaded"));
    }
    observatory.ingest_snapshot(snapshot_of(std::move(modules)), 0, 1000);
    const Frame after = observatory.tick(1000);

    CHECK_BECAUSE(drawn(after, hidden),
                  "severity is what the visible set ranks on, so a module that has just broken "
                  "outranks four hundred quiet ones and is promoted into view -- which is the "
                  "whole point of ranking by severity rather than by arrival order");
    CHECK_BECAUSE(after.events_elsewhere == 0,
                  "and because it is now on screen, its pulse has somewhere to land");
    CHECK_EQ(after.events.events.size(), 1U);
}

void an_event_for_a_node_the_budget_still_hides_is_elsewhere() {
    CASE("not on screen is not on screen, whichever reason put it there");

    // A queue large enough to hold the whole storm, so the arithmetic below is about
    // where events went rather than about how many the bound discarded. Six hundred
    // simultaneous changes overflow the default capacity, which is the queue behaving
    // correctly and would obscure the property under test.
    ObservatoryPolicy policy{};
    policy.event_capacity = 1000;
    Observatory observatory{policy};
    observatory.ingest_snapshot(busy_snapshot(600), 0, 0);
    observatory.ingest_kernel_payload(kernel_of("available", "saturated"), 0, true, 0);
    const Frame first = observatory.tick(0);
    const int drawn_count = static_cast<int>(first.nodes.size());
    CHECK(drawn_count < first.total_nodes);

    // The whole snapshot goes stale. Every node changes, and freshness is deliberately
    // not part of what the visible set ranks on -- so the same nodes stay hidden, and
    // their events have nowhere to go.
    observatory.ingest_snapshot(busy_snapshot(600), 240000, 1000);
    const Frame stale = observatory.tick(1000);

    CHECK_EQ(static_cast<int>(stale.nodes.size()), drawn_count);
    CHECK_BECAUSE(stale.events_elsewhere == stale.total_nodes - drawn_count,
                  "the budget aggregated them away rather than a view filtering them out, but "
                  "the renderer has exactly the same nowhere to draw them");
    CHECK_EQ(static_cast<int>(stale.events.events.size()), drawn_count);
    CHECK_BECAUSE(stale.events.dropped == 0,
                  "nothing was lost to the bound here, so every one of the six hundred changes "
                  "is accounted for as either drawable or elsewhere");
}

void a_storm_that_overflowed_the_queue_still_says_so() {
    CASE("the drop count survives being partitioned by what is drawn");

    // Default capacity, and six hundred nodes changing at once. The queue is supposed to
    // discard the oldest and count them; partitioning the survivors afterwards must not
    // lose that count on the way to the frame.
    Observatory observatory{};
    observatory.ingest_snapshot(busy_snapshot(600), 0, 0);
    observatory.tick(0);

    observatory.ingest_snapshot(busy_snapshot(600), 240000, 1000);
    const Frame frame = observatory.tick(1000);

    CHECK_BECAUSE(frame.events.dropped > 0,
                  "six hundred simultaneous changes do not fit a queue of two hundred and "
                  "fifty six, and an interface that showed the survivors without saying some "
                  "were discarded would present a partial history as a complete one");
    // Everything the queue kept is accounted for on one side or the other, and the
    // total cannot exceed what the queue is allowed to hold.
    const std::size_t accounted =
        frame.events.events.size() + static_cast<std::size_t>(frame.events_elsewhere);
    CHECK(accounted > 0U);
    CHECK_BECAUSE(accounted <= 256U,
                  "the partition redistributes what was drained, it does not manufacture "
                  "entries the queue never held");
}

void an_identifier_two_things_claim_is_reported() {
    CASE("a node that vanishes must not leave every count saying the payload was clean");

    Observatory observatory{};
    observatory.ingest_snapshot(
        snapshot_of({module_record("atlas", "loaded")}, {peer_record("atlas", "unhealthy", true)}),
        0, 0);

    const Frame frame = observatory.tick(0);

    CHECK_BECAUSE(frame.total_nodes == 1,
                  "the graph keeps the first and refuses the second, which is the only choice "
                  "that does not make the result depend on which list was walked first");
    CHECK_BECAUSE(frame.ingest.clean(),
                  "and neither record is malformed: each is perfectly valid in its own list, "
                  "which is exactly why the ingest report cannot be the thing that says so");
    CHECK_BECAUSE(frame.colliding_ids.size() == 1U &&
                      frame.colliding_ids[0] == std::string{"atlas"},
                  "an unhealthy peer disappearing while every number in the frame reports a "
                  "clean payload is the failure this library exists to prevent");
}

void an_uncontested_scene_reports_no_collisions() {
    CASE("the collision list is empty when nothing collides");

    Observatory observatory{};
    observatory.ingest_snapshot(
        snapshot_of({module_record("core.archie", "loaded")}, {peer_record("node-b", "ok", true)},
                    {model_record("local.reasoner", "ready", 128000)}),
        0, 0);
    observatory.ingest_world_payload(world_of({observation("q", "minor", 3.0, 0.0, 0.0)}), 0, 0);

    const Frame frame = observatory.tick(0);
    CHECK_EQ(frame.total_nodes, 4);
    CHECK(frame.colliding_ids.empty());
}

void a_collision_across_the_two_projections_is_reported() {
    CASE("an observation and a module can collide as easily as a module and a peer");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("shared.name", "loaded")}), 0, 0);
    observatory.tick(0);
    observatory.ingest_world_payload(
        world_of({observation("shared.name", "strong", 6.0, 10.0, 10.0)}), 0, 1000);

    const Frame frame = observatory.tick(1000);
    CHECK_BECAUSE(!frame.colliding_ids.empty(),
                  "the world daemon and the control plane name things independently, and "
                  "nothing coordinates them");
}

void an_inspection_survives_the_screen_going_away() {
    CASE("Headless is not a change of view, and the operator did not stop reading");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("m", "loaded")}), 0, 0);
    observatory.ingest_kernel_payload(kernel_of("available", "idle"), 0, true, 0);
    observatory.tick(0);

    observatory.focus("m");
    for (int i = 1; i <= 80; ++i) {
        observatory.tick(i * 16);
    }
    CHECK_EQ(observatory.tick(2000).focused, std::string{"m"});

    // The compositor goes away. Nothing is drawn, because nobody is looking.
    observatory.ingest_kernel_payload(kernel_of("available", "idle"), 0, false, 2100);
    const Frame dark = observatory.tick(2100);
    CHECK_EQ(dark.tier, RenderTier::Headless);
    CHECK(dark.nodes.empty());
    CHECK_BECAUSE(dark.focused == std::string{"m"},
                  "releasing here would be treating a blank screen as though the operator had "
                  "navigated away; they have not, and there is no empty space for the camera "
                  "to be pointed at because there is no space at all");

    // And it comes back to exactly what they were reading.
    observatory.ingest_kernel_payload(kernel_of("available", "idle"), 0, true, 2200);
    const Frame back = observatory.tick(2200);
    CHECK_EQ(back.focused, std::string{"m"});
    CHECK(drawn(back, "m"));
    CHECK_BECAUSE(back.camera_state == CameraState::Focused,
                  "and it resumes settled rather than flying back in, since the camera never "
                  "went anywhere");
}

void an_inspection_survives_a_node_leaving_and_coming_back() {
    CASE("a flap under the operator does not cost them their place");

    ObservatoryPolicy policy{};
    policy.graph.departure_grace_ms = 5000;
    Observatory observatory{policy};

    observatory.ingest_snapshot(snapshot_of({module_record("m", "loaded")}), 0, 0);
    observatory.tick(0);
    observatory.focus("m");
    for (int i = 1; i <= 80; ++i) {
        observatory.tick(i * 16);
    }

    // It drops out of one poll. It is still on screen, dimmed, inside its grace.
    observatory.ingest_snapshot(snapshot_of({}), 0, 2000);
    const Frame departing = observatory.tick(2000);
    CHECK_EQ(node_in(departing, "m")->presence, NodePresence::Departing);
    CHECK_BECAUSE(departing.focused == std::string{"m"},
                  "the whole point of the grace is that this may be a flap rather than a "
                  "death, and letting go on the first missing poll would end an inspection "
                  "for something that never left");

    // And it comes back.
    observatory.ingest_snapshot(snapshot_of({module_record("m", "loaded")}), 0, 3000);
    const Frame returned = observatory.tick(3000);
    CHECK_EQ(node_in(returned, "m")->presence, NodePresence::Present);
    CHECK_EQ(returned.focused, std::string{"m"});
    CHECK_EQ(returned.camera_state, CameraState::Focused);
}

void an_inspection_survives_a_change_of_mind_about_the_view() {
    CASE("a morph and its reversal, on a node both views contain");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("m", "loaded")}), 0, 0);
    observatory.tick(0);
    observatory.focus("m");
    for (int i = 1; i <= 80; ++i) {
        observatory.tick(i * 16);
    }

    // A module is in Brain as well as Cerebra, so neither end of this excludes it.
    observatory.request_mode(SceneMode::Brain);
    CHECK_EQ(observatory.tick(2100).focused, std::string{"m"});

    observatory.request_mode(SceneMode::Cerebra);
    CHECK_BECAUSE(observatory.tick(2200).focused == std::string{"m"},
                  "turning back is a change of mind about the view, not about what was being "
                  "read, and the release rule only fires when the destination genuinely does "
                  "not contain it");

    for (std::int64_t t = 2300; t <= 8000; t += 100) {
        observatory.tick(t);
    }
    const Frame settled = observatory.tick(8000);
    CHECK_EQ(settled.mode, SceneMode::Cerebra);
    CHECK_EQ(settled.focused, std::string{"m"});
    CHECK_EQ(settled.camera_state, CameraState::Focused);
}

void data_ages_even_when_nothing_new_arrives() {
    CASE("a daemon going silent does not leave its nodes claiming to be current");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("m", "loaded")}), 0, 0);
    const Frame fresh = observatory.tick(0);
    CHECK_EQ(node_in(fresh, "m")->freshness, Freshness::Live);
    CHECK(node_in(fresh, "m")->animate);

    // Nothing else ever arrives. Only time passes.
    const Frame later = observatory.tick(300000);
    CHECK_BECAUSE(node_in(later, "m")->freshness == Freshness::Expired,
                  "deriving freshness once at ingest and storing it means a daemon that stops "
                  "answering leaves its nodes at full strength and still moving, forever -- "
                  "the exact lie the freshness rules exist to prevent, through the one door "
                  "nobody was watching");
    CHECK_BECAUSE(!node_in(later, "m")->animate,
                  "and motion is what a viewer reads as happening now");
    CHECK(node_in(later, "m")->colour.alpha < node_in(fresh, "m")->colour.alpha);
}

void a_projection_that_keeps_answering_stays_current() {
    CASE("ageing is not a countdown that runs whatever the daemon does");

    Observatory observatory{};
    for (std::int64_t t = 0; t <= 300000; t += 500) {
        observatory.ingest_snapshot(snapshot_of({module_record("m", "loaded")}), 0, t);
        observatory.tick(t);
    }
    const Frame frame = observatory.tick(300000);
    CHECK_EQ(node_in(frame, "m")->freshness, Freshness::Live);
    CHECK(node_in(frame, "m")->animate);
}

void the_two_projections_age_independently() {
    CASE("one daemon going quiet says nothing about the other");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("m", "loaded")}), 0, 0);
    // Deliberately a different age from the snapshot's. Two stamps that agree on both
    // numbers cannot show whether each projection is being read with its own.
    observatory.ingest_world_payload(world_of({observation("q", "strong", 6.0, 10.0, 20.0)}),
                                     4000, 0);
    const Frame start = observatory.tick(0);
    CHECK_BECAUSE(node_in(start, "m")->freshness == Freshness::Live &&
                      node_in(start, "q")->freshness == Freshness::Lagging,
                  "the snapshot arrived current and the world batch arrived four seconds old, "
                  "and each is read against its own arrival rather than a shared one");

    // The control plane keeps answering. The world feed does not.
    for (std::int64_t t = 500; t <= 300000; t += 500) {
        observatory.ingest_snapshot(snapshot_of({module_record("m", "loaded")}), 0, t);
        observatory.tick(t);
    }
    const Frame frame = observatory.tick(300000);

    CHECK_BECAUSE(node_in(frame, "m")->freshness == Freshness::Live,
                  "the module's own daemon is answering every half second");
    CHECK_BECAUSE(node_in(frame, "q")->freshness == Freshness::Expired,
                  "and the quake's has not spoken in five minutes; a snapshot of the control "
                  "plane is not evidence about the weather");
    CHECK(node_in(frame, "m")->animate);
    CHECK(!node_in(frame, "q")->animate);
}

void a_payload_that_arrives_old_is_old_immediately() {
    CASE("the payload's own claim still counts");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("m", "loaded")}), 240000, 0);
    const Frame frame = observatory.tick(0);
    CHECK_BECAUSE(node_in(frame, "m")->freshness == Freshness::Expired,
                  "four minutes old on arrival is four minutes old, and the decay is added to "
                  "that rather than replacing it");

    // And a batch that was already lagging does not become current by sitting there.
    Observatory second{};
    second.ingest_snapshot(snapshot_of({module_record("m", "loaded")}), 3000, 0);
    CHECK_EQ(node_in(second.tick(0), "m")->freshness, Freshness::Lagging);
    // Three seconds old on arrival plus twenty elapsed is twenty three, which is past the
    // stale threshold and nowhere near the expiry one. The point is that the two add up.
    CHECK_EQ(node_in(second.tick(20000), "m")->freshness, Freshness::Stale);
    CHECK_EQ(node_in(second.tick(200000), "m")->freshness, Freshness::Expired);
}

void a_clock_that_goes_backwards_does_not_make_data_younger() {
    CASE("a rollback is not a refresh");

    // Arriving current, so a negative elapsed time would push the age below zero rather
    // than merely making an old payload slightly less old -- which is what makes the two
    // readings tell apart.
    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("m", "loaded")}), 0, 1000000);
    const Frame at_ingest = observatory.tick(1000000);
    CHECK_EQ(node_in(at_ingest, "m")->freshness, Freshness::Live);

    const Frame rolled_back = observatory.tick(900000);
    CHECK_BECAUSE(node_in(rolled_back, "m")->freshness == Freshness::Live,
                  "an unguarded subtraction gives an age a hundred seconds in the future, "
                  "which the freshness rules correctly refuse to call current -- so a clock "
                  "stepping backwards would grey out a scene that is perfectly up to date");

    // And once the clock is moving forwards again from there, ageing resumes normally.
    CHECK_EQ(node_in(observatory.tick(1200000), "m")->freshness, Freshness::Expired);
}

void the_render_tier_backs_off_when_telemetry_goes_quiet() {
    CASE("a rich tier needs current evidence, not evidence that was current once");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("m", "loaded")}), 0, 0);
    for (std::int64_t t = 0; t <= 8000; t += 250) {
        observatory.ingest_kernel_payload(kernel_of("available", "idle"), 0, true, t);
        observatory.tick(t);
    }
    CHECK_EQ(observatory.tick(8000).tier, RenderTier::Ultra);

    // The kernel reader stops answering. Nothing else changes.
    //
    // Ten seconds is the interesting duration: kernel telemetry is a second or two old
    // at most in normal operation, so its policy calls this stale, while the world feed's
    // policy -- written for a provider that publishes every few minutes -- would still
    // call it merely lagging. Waiting long enough to be stale under either would not show
    // which one is being consulted.
    for (std::int64_t t = 8250; t <= 18000; t += 250) {
        observatory.tick(t);
    }
    CHECK_BECAUSE(observatory.tick(18000).tier != RenderTier::Ultra,
                  "a stalled kernel reader has to be noticed on the kernel's timescale; "
                  "reading it against the world feed's patience would leave the display "
                  "spending a cinematic budget for minutes after the machine stopped "
                  "reporting");

    for (std::int64_t t = 18250; t <= 60000; t += 1000) {
        observatory.tick(t);
    }
    const Frame quiet = observatory.tick(60000);

    CHECK_BECAUSE(quiet.tier != RenderTier::Ultra,
                  "the rule that a stale reading cannot buy a rich tier already existed and "
                  "was tested; it simply never ran, because the only thing that called the "
                  "governor was an ingest -- so a reader that stopped left the machine "
                  "spending a cinematic budget on evidence ten minutes old");
    CHECK_EQ(quiet.tier, RenderTier::Normal);
    CHECK_EQ(quiet.tier_reason, TierReason::TelemetryStale);
    CHECK(!quiet.profile.post_processing);
}

void a_kernel_reader_that_keeps_answering_keeps_its_tier() {
    CASE("backing off is a response to silence, not a timer");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("m", "loaded")}), 0, 0);
    for (std::int64_t t = 0; t <= 60000; t += 250) {
        observatory.ingest_kernel_payload(kernel_of("available", "idle"), 0, true, t);
        observatory.tick(t);
    }
    const Frame frame = observatory.tick(60000);
    CHECK_EQ(frame.tier, RenderTier::Ultra);
    CHECK_EQ(frame.tier_reason, TierReason::PressureIdle);
}

void visibility_alone_is_not_a_kernel_reading() {
    CASE("a screen coming and going does not invent telemetry");

    Observatory untouched{};
    untouched.ingest_snapshot(snapshot_of({module_record("m", "loaded")}), 0, 0);
    const Frame expected = untouched.tick(100);

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("m", "loaded")}), 0, 0);
    observatory.set_display_present(false, 0);
    CHECK_EQ(observatory.tick(0).tier, RenderTier::Headless);
    observatory.set_display_present(true, 100);
    const Frame shown = observatory.tick(100);
    CHECK_BECAUSE(shown.tier == expected.tier && shown.tier_reason == expected.tier_reason,
                  "before any kernel reply the tier is the one for no telemetry, not one "
                  "earned by an empty reading stamped as fresh");
}

void an_austere_tier_is_not_relaxed_by_silence() {
    CASE("losing contact with a saturated machine does not mean it recovered");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("m", "loaded")}), 0, 0);
    observatory.ingest_kernel_payload(kernel_of("available", "saturated"), 0, true, 0);
    CHECK_EQ(observatory.tick(0).tier, RenderTier::Conserve);

    for (std::int64_t t = 1000; t <= 60000; t += 1000) {
        observatory.tick(t);
    }
    const Frame quiet = observatory.tick(60000);
    CHECK_BECAUSE(quiet.tier == RenderTier::Conserve,
                  "the staleness rule caps how rich a tier evidence can buy; if it also lifted "
                  "austere ones back to Normal, then losing contact with a machine in trouble "
                  "would make the display spend more on it rather than less");
}

void nothing_is_inspected_until_somebody_asks() {
    CASE("no panel of blanks for a node nobody chose");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("m", "loaded")}), 0, 0);

    const Frame frame = observatory.tick(0);
    CHECK_BECAUSE(!frame.inspection.has_value(),
                  "absent rather than empty, so a renderer cannot draw an inspector full of "
                  "blank fields for something the operator never clicked");
}

void inspecting_a_module_carries_what_the_payload_said_about_it() {
    CASE("the detail the observatory used to read past");

    Observatory observatory{};
    observatory.ingest_snapshot(
        snapshot_of({detailed_module("sentry.vision", "unsatisfied", "0.4.1",
                                     "node does not provide: compute.cuda")}),
        0, 0);
    observatory.tick(0);
    observatory.focus("sentry.vision");
    const Frame frame = observatory.tick(16);

    CHECK(frame.inspection.has_value());
    const Inspection& panel = *frame.inspection;
    CHECK_EQ(panel.id, std::string{"sentry.vision"});
    CHECK_EQ(panel.kind, NodeKind::Module);
    CHECK_EQ(panel.version, std::string{"0.4.1"});
    CHECK_BECAUSE(panel.explanation == std::string{"node does not provide: compute.cuda"},
                  "this is the whole difference between a node coloured amber and a node "
                  "coloured amber because it needs a capability this machine does not have -- "
                  "the first is a prompt to go and find out, the second is the answer");
    CHECK_EQ(panel.severity, 1);
}

void inspecting_a_module_says_what_leans_on_it() {
    CASE("the question an operator asks about the thing that just broke");

    Observatory observatory{};
    observatory.ingest_snapshot(
        snapshot_of({
            detailed_module("core.memory", "loaded", "1.0.0", "loaded"),
            detailed_module("sentry.world", "loaded", "1.0.0", "loaded", {Value{"core.memory"}}),
            detailed_module("cortex.context", "loaded", "1.0.0", "loaded", {Value{"core.memory"}}),
            detailed_module("unrelated", "loaded", "1.0.0", "loaded"),
        }),
        0, 0);
    observatory.tick(0);
    observatory.focus("core.memory");
    const Frame frame = observatory.tick(16);

    CHECK(frame.inspection.has_value());
    const Inspection& panel = *frame.inspection;
    CHECK_BECAUSE(panel.dependents.size() == 2U,
                  "the payload says what each module depends on; the useful question when "
                  "something has gone wrong is the reverse, and nothing was answering it");
    CHECK(std::find(panel.dependents.begin(), panel.dependents.end(), "sentry.world") !=
          panel.dependents.end());
    CHECK(std::find(panel.dependents.begin(), panel.dependents.end(), "cortex.context") !=
          panel.dependents.end());
    CHECK_BECAUSE(std::find(panel.dependents.begin(), panel.dependents.end(), "unrelated") ==
                      panel.dependents.end(),
                  "and a module that declares nothing about this one is not affected by it");
    CHECK(panel.depends_on.empty());
}

void a_node_that_depends_on_things_reports_both_directions() {
    CASE("what it needs and what needs it are different lists");

    Observatory observatory{};
    observatory.ingest_snapshot(
        snapshot_of({
            detailed_module("base", "loaded", "1.0.0", "loaded"),
            detailed_module("middle", "loaded", "1.0.0", "loaded", {Value{"base"}}),
            detailed_module("top", "loaded", "1.0.0", "loaded", {Value{"middle"}}),
        }),
        0, 0);
    observatory.tick(0);
    observatory.focus("middle");
    const Frame frame = observatory.tick(16);

    CHECK(frame.inspection.has_value());
    CHECK_EQ(frame.inspection->depends_on.size(), 1U);
    CHECK_EQ(frame.inspection->depends_on[0], std::string{"base"});
    CHECK_EQ(frame.inspection->dependents.size(), 1U);
    CHECK_EQ(frame.inspection->dependents[0], std::string{"top"});
}

void inspecting_reports_how_long_the_thing_has_been_seen() {
    CASE("a node that appeared four seconds ago on a week-old daemon just restarted");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("old", "loaded")}), 0, 1000);
    observatory.tick(1000);
    observatory.ingest_snapshot(
        snapshot_of({module_record("old", "loaded"), module_record("new", "loaded")}), 0, 9000);
    observatory.tick(9000);

    observatory.focus("new");
    const Frame frame = observatory.tick(9016);
    CHECK(frame.inspection.has_value());
    CHECK_BECAUSE(frame.inspection->first_seen_ms == 9000,
                  "when this observatory first saw the thing, which is not when the thing was "
                  "created -- and the difference is exactly what says it just came back");
    CHECK_EQ(frame.inspection->last_seen_ms, 9000);
}

void the_inspection_follows_the_focus() {
    CASE("releasing or losing the node closes the panel");

    ObservatoryPolicy policy{};
    policy.graph.departure_grace_ms = 1000;
    Observatory observatory{policy};
    observatory.ingest_snapshot(
        snapshot_of({module_record("a", "loaded"), module_record("b", "loaded")}), 0, 0);
    observatory.tick(0);

    observatory.focus("a");
    CHECK_EQ(observatory.tick(16).inspection->id, std::string{"a"});

    observatory.focus("b");
    CHECK_BECAUSE(observatory.tick(32).inspection->id == std::string{"b"},
                  "the panel is about whatever is focused now, not whatever was focused when "
                  "it was first built");

    observatory.release_focus();
    CHECK(!observatory.tick(48).inspection.has_value());

    // And a node that goes away takes its panel with it, rather than leaving a page of
    // fields that will never update again.
    observatory.focus("a");
    CHECK(observatory.tick(64).inspection.has_value());
    observatory.ingest_snapshot(snapshot_of({module_record("b", "loaded")}), 0, 100);
    observatory.tick(5000);
    CHECK(!observatory.tick(5016).inspection.has_value());
}

void an_observation_can_be_inspected_too() {
    CASE("the panel is not only about modules");

    Observatory observatory{};
    observatory.ingest_world_payload(
        world_of({observation("us.tokyo", "strong", 6.4, 35.68, 139.69, true)}), 0, 0);
    observatory.tick(0);
    observatory.focus("us.tokyo");
    const Frame frame = observatory.tick(16);

    CHECK(frame.inspection.has_value());
    CHECK_EQ(frame.inspection->kind, NodeKind::Observation);
    CHECK(frame.inspection->location.has_value());
    CHECK_BECAUSE(frame.inspection->authoritative,
                  "and it carries who said so, which for an observation is half of what the "
                  "operator is deciding about");
    CHECK(frame.inspection->explanation.empty());
}

/// The camera that looks at the globe has to be able to see all of it.
///
/// The globe's size is policy and the camera's distance was a constant, so the two could
/// be configured into incoherence â€” and were: at the shipped distance, the default globe
/// subtended about forty degrees inside a thirty-two degree field and overflowed the
/// frame it was supposed to sit inside. Nothing noticed, because nothing had ever asked
/// the camera whether it could see its own subject.
///
/// Checked at three radii, because a fix that only works at the default is the same bug.
void the_watcher_camera_frames_the_globe_it_is_looking_at() {
    for (const float radius : {4.0F, 20.0F, 75.0F}) {
        ObservatoryPolicy policy;
        policy.layout.globe.radius = radius;
        Observatory observatory(policy);

        observatory.request_mode(SceneMode::Watcher);
        Frame frame{};
        for (std::int64_t t = 0; t <= 4000; t += 100) {
            frame = observatory.tick(t);
        }
        CHECK_EQ(frame.mode, SceneMode::Watcher);

        const float dx = frame.camera.eye.x - frame.camera.target.x;
        const float dy = frame.camera.eye.y - frame.camera.target.y;
        const float dz = frame.camera.eye.z - frame.camera.target.z;
        const float distance = std::sqrt(dx * dx + dy * dy + dz * dz);
        CHECK_BECAUSE(distance > radius,
                      "a camera inside the globe is not looking at it");

        constexpr float kRadiansToDegrees = 180.0F / 3.14159265F;
        const float angular_diameter =
            2.0F * std::asin(radius / distance) * kRadiansToDegrees;

        CHECK_BECAUSE(angular_diameter < frame.camera.fov_degrees,
                      "the globe overflows the frame it is supposed to sit inside");
        CHECK_BECAUSE(angular_diameter > frame.camera.fov_degrees * 0.5F,
                      "a globe that fills a fifth of the screen is not the subject of a "
                      "view that exists to look at it");
    }
}

/// The globe centres on the place the operator said the machine is.
///
/// "Home" is configuration, never measurement: nothing in the machine can see out of the
/// case, and the tempting substitutes â€” geolocating an IP, reading a timezone â€” give a
/// confident answer that is wrong by a country. So the only source is the status payload,
/// and the only correct behaviour without one is to make no claim at all.
void the_globe_centres_on_the_place_the_operator_named() {
    // A place with a distinctive sign in every axis, so a mirrored or swapped mapping
    // cannot pass by symmetry.
    struct Case {
        double latitude;
        double longitude;
    };
    for (const Case place : {Case{-34.6, -58.4}, Case{35.7, 139.7}, Case{64.1, -21.9}}) {
        Observatory observatory;
        observatory.ingest_snapshot(
            Value::object(
                {member("status",
                        Value::object({member(
                            "location",
                            Value::object({member("latitude", Value(place.latitude)),
                                           member("longitude", Value(place.longitude))}))})),
                 member("modules", Value::array({})), member("nodes", Value::array({}))}),
            0, 0);
        observatory.request_mode(SceneMode::Watcher);
        Frame frame{};
        for (std::int64_t t = 0; t <= 4000; t += 100) {
            frame = observatory.tick(t);
        }

        // Where that place actually is on the globe this scene draws.
        const GlobePolicy globe{};
        const Vec3 surface = globe_position(GeoPoint{place.latitude, place.longitude}, 0.0F, globe);
        const float surface_length =
            std::sqrt(surface.x * surface.x + surface.y * surface.y + surface.z * surface.z);
        const float eye_length = std::sqrt(frame.camera.eye.x * frame.camera.eye.x +
                                           frame.camera.eye.y * frame.camera.eye.y +
                                           frame.camera.eye.z * frame.camera.eye.z);
        CHECK(surface_length > 0.0F && eye_length > 0.0F);

        // The camera stands off along the surface normal, so the two directions agree.
        const float dot = (surface.x * frame.camera.eye.x + surface.y * frame.camera.eye.y +
                           surface.z * frame.camera.eye.z) /
                          (surface_length * eye_length);
        CHECK_BECAUSE(dot > 0.999F,
                      "the camera is not above the place the operator named, so home is "
                      "somewhere else on the globe");
        CHECK_BECAUSE(eye_length > globe.radius,
                      "a camera inside the planet is not looking at the surface");
    }
}

/// And with nobody having said, it makes no claim.
void an_unstated_home_leaves_the_globe_where_it_was() {
    Observatory stated;
    Observatory unstated;
    for (Observatory* observatory : {&stated, &unstated}) {
        observatory->request_mode(SceneMode::Watcher);
    }
    stated.ingest_snapshot(
        Value::object(
            {member("status",
                    Value::object({member("location",
                                          Value::object({member("latitude", Value(-34.6)),
                                                         member("longitude", Value(-58.4))}))})),
             member("modules", Value::array({})), member("nodes", Value::array({}))}),
        0, 0);
    unstated.ingest_snapshot(
        Value::object({member("modules", Value::array({})), member("nodes", Value::array({}))}), 0,
        0);

    Frame with{};
    Frame without{};
    for (std::int64_t t = 0; t <= 4000; t += 100) {
        with = stated.tick(t);
        without = unstated.tick(t);
    }
    CHECK_BECAUSE(!(with.camera.eye == without.camera.eye),
                  "a stated home that changes nothing is a setting that does not work");

    // The unstated one is still framing its globe, not sitting at some degenerate pose.
    const float eye_length = std::sqrt(without.camera.eye.x * without.camera.eye.x +
                                       without.camera.eye.y * without.camera.eye.y +
                                       without.camera.eye.z * without.camera.eye.z);
    constexpr float kRadiansToDegrees = 180.0F / 3.14159265F;
    const float angular_diameter =
        2.0F * std::asin(GlobePolicy{}.radius / eye_length) * kRadiansToDegrees;
    CHECK(angular_diameter < without.camera.fov_degrees);
}

/// A location that is not two numbers is reported rather than half-applied.
void a_malformed_home_is_reported_and_not_guessed_at() {
    Observatory observatory;
    observatory.ingest_snapshot(
        Value::object(
            {member("status",
                    Value::object({member("location",
                                          Value::object({member("latitude", Value("here")),
                                                         member("longitude", Value(-58.4))}))})),
             member("modules", Value::array({})), member("nodes", Value::array({}))}),
        0, 0);
    observatory.request_mode(SceneMode::Watcher);
    Frame frame{};
    for (std::int64_t t = 0; t <= 4000; t += 100) {
        frame = observatory.tick(t);
    }
    CHECK_BECAUSE(frame.ingest.count_of(DefectKind::WrongType) > 0,
                  "a place the display could not read must be said, not silently dropped");
    CHECK_BECAUSE(frame.camera.eye.x == 0.0F,
                  "half a coordinate is not a location, and must not move the globe");
}

void ticking_is_deterministic() {
    CASE("the same inputs produce the same frame");

    const auto run = [] {
        Observatory observatory{};
        std::vector<Vec3> path;
        observatory.ingest_snapshot(busy_snapshot(30), 0, 0);
        observatory.ingest_kernel_payload(kernel_of("available", "idle"), 0, true, 0);
        observatory.tick(0);
        observatory.focus("m7");
        for (int i = 1; i <= 60; ++i) {
            path.push_back(observatory.tick(i * 16).camera.eye);
        }
        observatory.request_mode(SceneMode::Fabric);
        for (int i = 61; i <= 160; ++i) {
            path.push_back(observatory.tick(i * 16).camera.eye);
        }
        return path;
    };

    const std::vector<Vec3> first = run();
    const std::vector<Vec3> second = run();

    bool identical = first.size() == second.size();
    for (std::size_t i = 0; i < first.size() && i < second.size(); ++i) {
        if (!(first[i] == second[i])) {
            identical = false;
        }
    }
    CHECK_BECAUSE(identical,
                  "a frame that differs between runs cannot be asserted on, compared across "
                  "screenshots or replayed, and every source of variation is a bug that only "
                  "appears sometimes");
}

void events_only_ever_describe_real_change() {
    CASE("ticking a still scene forever produces nothing");

    Observatory observatory{};
    observatory.ingest_snapshot(snapshot_of({module_record("a", "loaded")}), 0, 0);
    observatory.tick(0);  // drains the arrival

    for (int i = 1; i < 500; ++i) {
        observatory.ingest_snapshot(snapshot_of({module_record("a", "loaded")}), 0, i * 100);
        const Frame frame = observatory.tick(i * 100);
        CHECK_BECAUSE(frame.events.empty(),
                      "a poll reporting the same state is not an event, and a display that "
                      "pulses on every refresh teaches its operator that motion means nothing");
    }
}

}  // namespace

void manual_globe_navigation_stays_bounded_and_can_return_home() {
    CASE("offline globe navigation uses the model and preserves home");
    Observatory model; model.request_mode(SceneMode::Watcher);model.tick(0);
    const auto home=model.tick(2000).camera;
    model.orbit_globe(40.0F,20.0F,2.0F);
    const auto moved=model.tick(2100).camera;
    CHECK(!(moved.eye==home.eye));CHECK(moved.target==Vec3{});
    model.orbit_globe(0.0F,10000.0F,10000.0F);
    for(int i=0;i<20;++i) model.orbit_globe(0.0F,0.0F,10.0F);
    const auto close=model.tick(2200).camera;
    const float r=std::sqrt(close.eye.x*close.eye.x+close.eye.y*close.eye.y+close.eye.z*close.eye.z);
    CHECK(r>20.0F && r<20.001F);CHECK(std::fabs(close.eye.y/r)<0.9999F);
    const auto beforePan=close;
    model.tilt_globe(20);
    const auto tilted=model.tick(2210).camera;
    CHECK(!(tilted.eye==close.eye));CHECK(tilted.target==close.target);
    const auto norm=[](Vec3 p){return std::sqrt(p.x*p.x+p.y*p.y+p.z*p.z);};
    CHECK(norm(tilted.eye)>20.0F);
    model.tilt_globe(std::nanf(""));CHECK(model.tick(2220).camera.eye==tilted.eye);
    model.orbit_globe(40,-20,0);
    const auto panned=model.tick(2250).camera;
    CHECK(!(panned.target==beforePan.target));
    CHECK(!(panned.target==Vec3{}));
    model.orbit_globe(std::nanf(""),0.0F,0.0F);
    CHECK(model.tick(2300).camera.eye==panned.eye);
    model.release_focus();model.tick(2400);
    CHECK(model.tick(4400).camera.eye==home.eye);
    model.request_mode(SceneMode::Cerebra);model.tick(4500);
    const auto core=model.tick(6500).camera;
    model.orbit_globe(90.0F,20.0F,2.0F);
    CHECK(model.tick(6600).camera.eye==core.eye);
}

void exploration_frames_true_radius_and_manual_input_only_moves_the_camera() {
    CASE("exploration frames a near-surface AOI with geographic metres");
    Observatory model;model.request_mode(SceneMode::Watcher);model.tick(0);model.tick(2000);
    CHECK(model.frame_globe(-33.45,-70.66,1000,600,false));
    CHECK(model.exploration_camera_active());
    const auto framed=model.tick(2100).camera;
    const auto distance=[](Vec3 a,Vec3 b){return std::sqrt((a.x-b.x)*(a.x-b.x)+(a.y-b.y)*(a.y-b.y)+(a.z-b.z)*(a.z-b.z));};
    CHECK(std::fabs(distance(framed.eye,framed.target)-3000.0F*20.0F/6371000.0F)<0.00001F);
    CHECK(distance(framed.target,Vec3{})>20.0F);
    CHECK(!model.frame_globe(91,0,1000));CHECK(!model.frame_globe(0,0,-1));
    CHECK(!model.frame_globe(0,std::nan(""),1000));
    CHECK(model.tick(2200).camera.eye==framed.eye);
    model.orbit_globe(20,-15,1);
    const auto manual=model.tick(2300).camera;
    CHECK(!(manual.eye==framed.eye));CHECK(!(manual.target==framed.target));
    model.set_display_present(false,2400);model.tick(2400);
    CHECK(model.tick(2500).camera.eye==manual.eye);
    model.release_focus();model.tick(2600);
    CHECK(model.tick(4600).camera.target==Vec3{});
    CHECK(!model.exploration_camera_active());
    CHECK(model.frame_globe(0,0,1000,-430,false));
    const auto negative=model.tick(4650).camera;
    CHECK(distance(negative.target,Vec3{})<20.0F);
    CHECK(distance(negative.eye,negative.target)>0.009F);
    model.request_mode(SceneMode::Cerebra);model.tick(4700);
    CHECK(!model.frame_globe(0,0,1000));
    model.request_mode(SceneMode::Watcher);model.tick(5000);model.tick(7000);
    CHECK(model.frame_globe(0,180,1000,0,true));
    const auto during=model.tick(7350).camera;
    CHECK(distance(during.eye,Vec3{})>=20.0F);
}

int main() {
    exploration_frames_true_radius_and_manual_input_only_moves_the_camera();
    manual_globe_navigation_stays_bounded_and_can_return_home();
    a_snapshot_becomes_a_drawable_frame();
    forgetting_a_node_releases_the_camera();
    an_unrelated_departure_does_not_interrupt_an_inspection();
    the_tier_caps_what_the_scene_draws();
    a_headless_tier_draws_nothing_but_forgets_nothing();
    stale_data_never_moves();
    a_conserving_tier_stops_idle_motion_even_on_live_data();
    a_mode_change_rehomes_and_forgets_incumbency();
    a_mode_change_actually_discards_incumbency();
    nothing_but_the_operator_changes_the_view();
    an_absurd_magnitude_cannot_swallow_the_scene();
    an_edge_is_never_drawn_into_empty_space();
    focusing_something_that_is_not_there_does_nothing();
    a_clock_that_steps_backwards_produces_a_frame_that_does_not_move();
    the_first_tick_does_not_jump();
    a_tangled_registry_is_surfaced();
    a_defective_payload_reaches_the_frame_as_a_count();
    colour_reaches_the_frame_fully_resolved();
    the_timeline_remembers_what_was_true();
    the_timeline_records_nodes_the_budget_hid();
    the_two_projections_do_not_evict_each_other();
    an_observation_that_falls_off_the_feed_departs();
    observations_are_placed_on_the_globe();
    inspecting_an_observation_flies_to_where_it_is();
    a_second_hand_report_is_drawn_more_faintly();
    the_two_ingest_reports_stay_separate();
    a_departing_module_is_not_revived_by_the_world_feed();
    peers_arrive_with_the_modules_they_share_a_payload_with();
    an_unreachable_peer_is_not_a_broken_one();
    a_peer_that_is_healthy_and_unreachable_is_read_cautiously();
    a_peer_with_no_reachable_flag_is_not_assumed_reachable();
    each_view_shows_what_it_is_about();
    a_morph_shows_both_ends();
    the_scene_survives_a_view_with_nothing_in_it();
    models_arrive_with_the_payload_they_share();
    model_availability_is_graded_like_everything_else();
    a_context_window_is_a_size_that_stays_comparable();
    a_model_belongs_to_the_cognition_view();
    the_timeline_records_a_departure();
    positions_track_the_graph_after_the_first_frame();
    a_node_reports_how_far_through_its_phase_it_is();
    presence_progress_survives_a_clock_that_misbehaves();
    changing_view_releases_something_that_view_does_not_contain();
    a_node_the_view_does_not_contain_cannot_be_focused();
    an_event_the_view_cannot_show_is_counted_not_dropped();
    a_node_that_breaks_earns_its_way_onto_the_screen();
    an_event_for_a_node_the_budget_still_hides_is_elsewhere();
    a_storm_that_overflowed_the_queue_still_says_so();
    an_identifier_two_things_claim_is_reported();
    an_uncontested_scene_reports_no_collisions();
    a_collision_across_the_two_projections_is_reported();
    an_inspection_survives_the_screen_going_away();
    an_inspection_survives_a_node_leaving_and_coming_back();
    an_inspection_survives_a_change_of_mind_about_the_view();
    data_ages_even_when_nothing_new_arrives();
    a_projection_that_keeps_answering_stays_current();
    the_two_projections_age_independently();
    a_payload_that_arrives_old_is_old_immediately();
    a_clock_that_goes_backwards_does_not_make_data_younger();
    the_render_tier_backs_off_when_telemetry_goes_quiet();
    a_kernel_reader_that_keeps_answering_keeps_its_tier();
    visibility_alone_is_not_a_kernel_reading();
    an_austere_tier_is_not_relaxed_by_silence();
    nothing_is_inspected_until_somebody_asks();
    inspecting_a_module_carries_what_the_payload_said_about_it();
    inspecting_a_module_says_what_leans_on_it();
    a_node_that_depends_on_things_reports_both_directions();
    inspecting_reports_how_long_the_thing_has_been_seen();
    the_inspection_follows_the_focus();
    an_observation_can_be_inspected_too();
    the_watcher_camera_frames_the_globe_it_is_looking_at();
    the_globe_centres_on_the_place_the_operator_named();
    an_unstated_home_leaves_the_globe_where_it_was();
    a_malformed_home_is_reported_and_not_guessed_at();
    ticking_is_deterministic();
    events_only_ever_describe_real_change();
    return archie::visual::test::report();
}
