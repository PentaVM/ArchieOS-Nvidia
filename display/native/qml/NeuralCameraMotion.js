.pragma library

// Presentation only. Poses and checked node positions carry no OS input authority.
// Design: Xavi. Native implementation: PentaVM.
// Time is explicit; each state belongs to one view, even though this library is shared.
function finite(value) { return typeof value === "number" && isFinite(value) }
function clamp(value, low, high) { return Math.max(low, Math.min(high, value)) }
function vector(value) {
    return value && finite(value.x) && finite(value.y) && finite(value.z)
        ? {x:value.x, y:value.y, z:value.z} : null
}
function copy(p) {
    return {target:vector(p.target), yaw:p.yaw, elevation:p.elevation, distance:p.distance}
}
function bounded(s, p) {
    if (!p || !vector(p.target) || !finite(p.yaw) || !finite(p.elevation) || !finite(p.distance)) return null
    return {target:vector(p.target), yaw:p.yaw,
        elevation:clamp(p.elevation, -89, 89), distance:clamp(p.distance,s.minDistance,s.maxDistance)}
}
function create(home, options) {
    options = options || {}
    const s = {minDistance:finite(options.minDistance) ? Math.max(0.01,options.minDistance) : 40,
        maxDistance:finite(options.maxDistance) ? Math.max(0.01,options.maxDistance) : 1200,
        focusMs:finite(options.focusMs) ? Math.max(0,options.focusMs) : 700,
        returnMs:finite(options.returnMs) ? Math.max(0,options.returnMs) : 900,
        followMs:finite(options.followMs) ? Math.max(0,options.followMs) : 180,
        enabled:true, lastMs:null, focusId:"", elapsed:0, duration:0}
    s.maxDistance = Math.max(s.minDistance,s.maxDistance)
    const p = bounded(s,home)
    if (!p) throw new Error("A camera home must be a finite pose")
    s.home = copy(p); s.current = copy(p); s.from = copy(p); s.to = copy(p)
    return s
}
function pose(s) { return copy(s.current) }
function moving(s) { return s.elapsed < s.duration }
function shortestYaw(from, to) { return from + ((to-from+180)%360+360)%360-180 }
function begin(s, destination, duration, reducedMotion) {
    const p = bounded(s,destination)
    if (!p) return false
    p.yaw = shortestYaw(s.current.yaw,p.yaw)
    s.from = copy(s.current); s.to = p; s.elapsed = 0
    s.duration = reducedMotion ? 0 : Math.max(0,duration)
    if (!s.duration) s.current = copy(s.to)
    return true
}
// lookup is NeuralField.lookupPosition(id): {found:true,position:{x,y,z}}.
// A missing node is distinct from a real node at the origin.
function focus(s, id, lookup, desired, reducedMotion) {
    if (typeof id !== "string" || !id.length || !lookup || lookup.found !== true || !vector(lookup.position)) return false
    desired = desired || {}
    const p = {target:vector(lookup.position),
        yaw:desired.yaw === undefined ? s.current.yaw : desired.yaw,
        elevation:desired.elevation === undefined ? s.current.elevation : desired.elevation,
        distance:desired.distance === undefined ? s.current.distance : desired.distance}
    if (!begin(s,p,s.focusMs,reducedMotion)) return false
    s.focusId = id
    return true
}
function follow(s, id, lookup, reducedMotion) {
    if (!s.focusId || s.focusId !== id) return false
    if (!lookup || lookup.found !== true || !vector(lookup.position)) {
        goHome(s,reducedMotion)
        return false
    }
    const p = copy(s.to), at = vector(lookup.position)
    if (p.target.x === at.x && p.target.y === at.y && p.target.z === at.z) return true
    p.target = at
    // Keep the dive's original deadline. Once settled, short tracking moves start at
    // the live pose; updates never restart the full node transition.
    return begin(s,p,moving(s) ? s.duration-s.elapsed : s.followMs,reducedMotion)
}
function goHome(s, reducedMotion) {
    s.focusId = ""
    return begin(s,s.home,s.returnMs,reducedMotion)
}
// Call before direct drag/wheel changes. Pass the current view pose when synchronizing
// manual navigation back into this state before a later explicit focus request.
function cancel(s, currentPose) {
    const p = currentPose ? bounded(s,currentPose) : copy(s.current)
    if (!p) return false
    s.current = p; s.from = copy(p); s.to = copy(p)
    s.focusId = ""; s.elapsed = 0; s.duration = 0
    return true
}
function reset(s, home) {
    const p = bounded(s,home)
    if (!p) return false
    s.home = copy(p); cancel(s,p); s.lastMs = null
    return true
}
// Invoke on visibility/freeze/governor changes, even when the frame timer is stopped.
function setEnabled(s, enabled) {
    if (s.enabled !== !!enabled) { s.enabled = !!enabled; s.lastMs = null }
}
function advance(s, nowMs, reducedMotion) {
    if (!finite(nowMs)) return pose(s)
    if (!s.enabled) { s.lastMs = null; return pose(s) }
    const delta = s.lastMs === null ? 0 : Math.max(0,nowMs-s.lastMs)
    // A backward clock is ignored, including its baseline, until time catches up.
    if (s.lastMs === null || nowMs >= s.lastMs) s.lastMs = nowMs
    if (!moving(s)) return pose(s)
    s.elapsed = reducedMotion ? s.duration : Math.min(s.duration,s.elapsed+delta)
    const fraction = s.duration > 0 ? s.elapsed/s.duration : 1
    const t = fraction*fraction*(3-2*fraction), a=s.from, b=s.to
    s.current = {target:{x:a.target.x+(b.target.x-a.target.x)*t,
        y:a.target.y+(b.target.y-a.target.y)*t,z:a.target.z+(b.target.z-a.target.z)*t},
        yaw:a.yaw+(b.yaw-a.yaw)*t,elevation:a.elevation+(b.elevation-a.elevation)*t,
        distance:Math.exp(Math.log(a.distance)+(Math.log(b.distance)-Math.log(a.distance))*t)}
    if (!moving(s)) s.current = copy(s.to)
    return pose(s)
}
