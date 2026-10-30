#include "archie/visual/camera.hpp"
#include <cmath>
#include <algorithm>

namespace archie::visual {
namespace {

constexpr float lerp(float a, float b, float t) noexcept { return a + (b - a) * t; }

constexpr Vec3 lerp(const Vec3& a, const Vec3& b, float t) noexcept {
    return Vec3{lerp(a.x, b.x, t), lerp(a.y, b.y, t), lerp(a.z, b.z, t)};
}

}  // namespace

CameraPose interpolate(const CameraPose& from, const CameraPose& to, float t) noexcept {
    CameraPose result{};
    result.eye = lerp(from.eye, to.eye, t);
    result.target = lerp(from.target, to.target, t);
    result.fov_degrees = lerp(from.fov_degrees, to.fov_degrees, t);
    return result;
}

CameraPose CameraDirector::pose() const noexcept {
    if (!moving()) {
        return m_to;
    }
    const float t=ease(progress());
    auto result=interpolate(m_from,m_to,t);
    if(m_spherical_navigation) {
        const auto radius=[](Vec3 p){return std::sqrt(p.x*p.x+p.y*p.y+p.z*p.z);};
        const float a=radius(m_from.eye),b=radius(m_to.eye);
        if(a>0 && b>0) {
            constexpr float pi=3.14159265358979323846F;
            const float lonA=std::atan2(m_from.eye.x,m_from.eye.z),lonB=std::atan2(m_to.eye.x,m_to.eye.z);
            const float latA=std::asin(std::clamp(m_from.eye.y/a,-1.0F,1.0F)),latB=std::asin(std::clamp(m_to.eye.y/b,-1.0F,1.0F));
            const float lon=lonA+std::remainder(lonB-lonA,2*pi)*t,lat=lerp(latA,latB,t),r=lerp(a,b,t);
            result.eye={r*std::cos(lat)*std::sin(lon),r*std::sin(lat),r*std::cos(lat)*std::cos(lon)};
        }
    }
    return result;
}

float CameraDirector::progress() const noexcept {
    // The duration test guards the division, not a reachable state: `begin` settles a
    // zero-length move outright, so a moving camera always has a positive duration. There
    // is deliberately no upper clamp on the ratio — `advance` settles the moment elapsed
    // reaches duration, so a moving camera is always strictly before its destination, and
    // a clamp here would be an unreachable branch implying a hazard that does not exist.
    if (!moving() || m_duration_ms <= 0) {
        return 1.0F;
    }
    return static_cast<float>(m_elapsed_ms) / static_cast<float>(m_duration_ms);
}

void CameraDirector::begin(const CameraPose& to, std::int64_t duration_ms, CameraState state) {
    // The whole point: the move starts from where the camera *is*, not from where the
    // previous move started. Without this, interrupting a transition snaps the camera
    // back to the old origin before setting off again, which reads as a glitch rather
    // than a change of mind.
    m_from = pose();
    m_spherical_navigation = false;
    m_to = to;
    m_elapsed_ms = 0;

    if (duration_ms <= 0) {
        // A zero-length move is a cut. Honour it rather than dividing by it.
        m_duration_ms = 0;
        settle_now();
        return;
    }

    m_duration_ms = duration_ms;
    m_state = state;
}

void CameraDirector::settle_now() {
    // `m_from` is deliberately not touched: it is only read while moving, and the next
    // `begin` overwrites it with the live pose anyway.
    m_elapsed_ms = m_duration_ms;
    m_state = m_settling_into;
}

void CameraDirector::set_home(const CameraPose& home, bool animate) {
    m_home = home;

    if (holding_focus() || m_manual) {
        // The operator is inspecting something. Re-homing the camera underneath them
        // would end that inspection without them asking, so the new home simply waits
        // until they release it.
        return;
    }

    m_settling_into = CameraState::Resting;
    begin(home, animate ? m_policy.mode_transition_ms : 0, CameraState::Transitioning);
}

void CameraDirector::focus(const std::string& node_id, const CameraPose& pose_) {
    m_manual = false;
    // Set immediately rather than on arrival: the inspector should be populating while
    // the camera dives, and an operator who has clicked a node has already committed.
    m_focused = node_id;
    m_settling_into = CameraState::Focused;
    begin(pose_, m_policy.focus_transition_ms, CameraState::Transitioning);
}

void CameraDirector::release_focus() {
    m_manual = false;
    m_focused.clear();
    m_settling_into = CameraState::Resting;
    begin(m_home, m_policy.return_transition_ms, CameraState::Returning);
}

void CameraDirector::navigate(const CameraPose& pose_, bool animate, bool spherical) {
    m_manual = true;
    m_focused.clear();
    m_settling_into = CameraState::Resting;
    begin(pose_, animate ? m_policy.focus_transition_ms : 0, CameraState::Transitioning);
    m_spherical_navigation=spherical;
}

void CameraDirector::note_node_removed(const std::string& node_id) {
    if (!m_focused.empty() && m_focused == node_id) {
        // A camera left staring at a node that no longer exists shows an empty patch of
        // space beside an inspector full of fields that will never update again, and
        // nothing distinguishes that from a node which has merely gone quiet.
        release_focus();
    }
}

void CameraDirector::advance(std::int64_t delta_ms) {
    if (delta_ms <= 0) {
        // Ignored rather than rewound. A frame that took no measurable time, or a clock
        // that stepped backwards between two frames, must not move the camera at all —
        // least of all backwards along a path the viewer already watched.
        return;
    }
    if (!moving()) {
        return;
    }

    m_elapsed_ms += delta_ms;
    if (m_elapsed_ms >= m_duration_ms) {
        settle_now();
    }
}

bool CameraDirector::holding_focus() const noexcept {
    return m_state == CameraState::Focused ||
           (m_state == CameraState::Transitioning && m_settling_into == CameraState::Focused);
}

}  // namespace archie::visual
