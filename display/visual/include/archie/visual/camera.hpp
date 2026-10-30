#pragma once

// Where the camera is, and how it gets somewhere else.
//
// Camera movement is the most expensive thing an observatory can get wrong. A cut
// where the viewer expected a move loses them completely — after a hard jump nobody
// knows whether they are looking at the same scene from a new angle or at a different
// scene entirely, and they have to re-read every label to find out. A move that
// restarts halfway through, or snaps back to a pose the camera left two seconds ago,
// does the same damage more slowly.
//
// So the camera is a state machine with one rule above the others: **a new instruction
// is obeyed from wherever the camera actually is**, never from where the previous
// instruction started. That single property is what makes an interrupted move look
// like a change of mind rather than a glitch, and it is checkable to the bit.
//
// There is no clock here and no randomness. `advance` takes a delta, poses are pure
// functions of the accumulated time, and the same sequence of calls always produces the
// same numbers — which is what makes a camera path something a test can assert on and
// two runs something that can be compared.

#include <cstdint>
#include <string>

namespace archie::visual {

struct Vec3 {
    float x = 0.0F;
    float y = 0.0F;
    float z = 0.0F;

    friend bool operator==(const Vec3& a, const Vec3& b) noexcept {
        return a.x == b.x && a.y == b.y && a.z == b.z;
    }
};

/// Everything needed to place a camera. Deliberately no up-vector or quaternion: the
/// scene is world-up and the renderer derives orientation from eye and target, so there
/// is one representation of a pose and no way for two of them to disagree.
struct CameraPose {
    Vec3 eye{};
    Vec3 target{};
    float fov_degrees = 45.0F;

    friend bool operator==(const CameraPose& a, const CameraPose& b) noexcept {
        return a.eye == b.eye && a.target == b.target && a.fov_degrees == b.fov_degrees;
    }
};

enum class CameraState {
    /// Settled at a pose. The overwhelming majority of the time.
    Resting,
    /// Moving to a pose that is not a node — a mode's home, or an operator's framing.
    Transitioning,
    /// Settled on a node the operator is inspecting.
    Focused,
    /// On its way back to the mode's home after a focus was released.
    Returning,
};

struct CameraPolicy {
    /// Moving between the major views. Long enough to follow, short enough not to
    /// become a loading screen.
    std::int64_t mode_transition_ms = 1200;
    /// Diving onto a node. Faster, because the operator asked for it and is waiting.
    std::int64_t focus_transition_ms = 700;
    /// Coming back out. Slightly slower than going in: the viewer needs a moment to
    /// re-acquire the wider scene, which they were not looking at on the way down.
    std::int64_t return_transition_ms = 900;
};

/// Smoothstep. Zero velocity at both ends, so a move neither jerks into motion nor
/// stops dead — the two things that read as "broken" rather than "fast".
[[nodiscard]] constexpr float ease(float t) noexcept {
    const float clamped = t < 0.0F ? 0.0F : (t > 1.0F ? 1.0F : t);
    return clamped * clamped * (3.0F - 2.0F * clamped);
}

[[nodiscard]] CameraPose interpolate(const CameraPose& from, const CameraPose& to, float t) noexcept;

/// Drives the camera. Holds no clock: `advance` takes a delta.
class CameraDirector final {
public:
    CameraDirector() = default;
    explicit CameraDirector(CameraPolicy policy) : m_policy(policy) {}

    /// Set the pose the camera returns to when nothing is being inspected.
    ///
    /// Changing the home while resting or returning moves the camera there; changing it
    /// while the operator is focused on a node does not yank them out of it.
    void set_home(const CameraPose& home, bool animate = true);

    /// Frame a node. `id` is remembered so the camera can be told if it goes away.
    void focus(const std::string& node_id, const CameraPose& pose);

    /// Let go of whatever is focused and head home.
    void release_focus();
    /// Direct operator navigation keeps the configured home available for Escape.
    void navigate(const CameraPose& pose, bool animate = false, bool spherical = false);
    [[nodiscard]] bool manual_navigation() const noexcept { return m_manual; }

    /// The focused node no longer exists.
    ///
    /// A camera left staring at a node that has been removed shows an empty patch of
    /// space with an inspector full of stale fields, and the operator has no way to tell
    /// that from a node that is simply dark. It goes home instead.
    void note_node_removed(const std::string& node_id);

    /// Advance by a time delta. Negative deltas are ignored rather than rewinding.
    void advance(std::int64_t delta_ms);

    [[nodiscard]] CameraPose pose() const noexcept;
    [[nodiscard]] CameraState state() const noexcept { return m_state; }
    [[nodiscard]] const std::string& focused_node() const noexcept { return m_focused; }

    /// How far through the current move, in [0, 1]. 1 when settled.
    [[nodiscard]] float progress() const noexcept;

    [[nodiscard]] bool moving() const noexcept {
        return m_state == CameraState::Transitioning || m_state == CameraState::Returning;
    }

private:
    /// Begin a move to `to`, starting from wherever the camera is right now.
    void begin(const CameraPose& to, std::int64_t duration_ms, CameraState state);

    /// Arrive: the move is over and the camera adopts the state it was heading for.
    void settle_now();

    /// Whether the operator is inspecting a node, including while still diving onto it.
    [[nodiscard]] bool holding_focus() const noexcept;

    CameraPolicy m_policy{};
    CameraState m_state = CameraState::Resting;

    /// The state to adopt on arrival. Kept separate from `m_state` because the movement
    /// and the destination are different facts: a dive onto a node and a move to a
    /// mode's home look identical while they are happening and mean different things
    /// the moment they stop.
    CameraState m_settling_into = CameraState::Resting;

    CameraPose m_home{};
    CameraPose m_from{};
    CameraPose m_to{};
    std::string m_focused{};
    bool m_manual = false;
    bool m_spherical_navigation = false;

    std::int64_t m_duration_ms = 0;
    std::int64_t m_elapsed_ms = 0;
};

}  // namespace archie::visual
