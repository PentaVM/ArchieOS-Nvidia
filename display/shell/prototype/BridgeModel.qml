import QtQuick
import "Lang.js" as Tr

// The shell's view of the node, fed by `archie-shell-bridge watch` (one JSON line per poll,
// schema 1; core/crates/archie-shell-bridge/src/protocol.rs). Pure QML: it neither reads
// files nor starts processes. A host feeds it lines with ingest(): in the shell that is
// quickshell/BridgeProcess.qml, in tests a recorded frame.
//
// Rule: the model never claims more than the last frame proved. A frame that is too old
// makes the link "bridge_silent"; a line it does not understand is counted and dropped,
// never half-applied.
QtObject {
    id: m
    readonly property int schema: 1
    // How often the bridge polls (seconds); silence is 3 polls without a frame.
    property int pollSeconds: 2
    // Wall clock in ms, advanced by `tick()`; injected so tests are deterministic.
    property real nowMs: Date.now()

    property var frame: null
    property real receivedAtMs: 0
    property int rejected: 0
    property string lastRejection: ""

    readonly property bool received: frame !== null
    readonly property real ageMs: received ? Math.max(0, nowMs - receivedAtMs) : 0
    readonly property bool silent: received && ageMs > pollSeconds * 3000

    // waiting | live | fixture | daemon_unreachable | displayd_offline | invalid | bridge_silent
    readonly property string link: !received ? "waiting" : (silent ? "bridge_silent" : frame.link.state)
    readonly property string reason: silent ? Tr.t("El bridge no ha enviado una trama reciente") : (received && frame.link.reason ? frame.link.reason : "")
    readonly property real staleMs: ageMs + (received && frame.link.stale_ms !== undefined ? frame.link.stale_ms : 0)

    readonly property var system: received && !silent ? frame.system : null
    readonly property var kernel: received && !silent ? frame.kernel : null
    // LAN inventory file read by the bridge. Frames from before this field existed count as not configured.
    readonly property var lan: localSource("lan", { devices: [], total: 0, age_seconds: 0, stale: false, path: "", reason: "" })
    // Long bridge jobs, read-only. Same rule as lan for older frames.
    readonly property var jobs: localSource("jobs", { jobs: [], skipped: 0, unreadable: [] })
    readonly property int runningJobs: jobs.state === "ok" ? jobs.jobs.filter(function (j) { return j.state === "running" || j.state === "starting" }).length : 0
    readonly property var catalog: projection("catalog", { modules: [], models: [], nodes: [], capabilities: {}, daemon_reachable: false, staleness_ms: 0 })
    readonly property var inspector: projection("inspector", { providers: [], cortex_trajectories: [] })
    readonly property var workflows: projection("workflows", { workflows: [] })

    function localSource(name, defaults) {
        var value = Object.assign({ state: "not_configured" }, defaults, received && frame[name] ? frame[name] : {})
        if (silent) { value.state = "bridge_silent"; value.reason = reason; value.stale = true }
        if (name === "lan" && value.state === "ok") {
            value.age_seconds += Math.floor(ageMs / 1000)
            value.stale = value.stale || value.age_seconds > 900
        }
        return value
    }
    function projection(name, defaults) {
        var raw = received && frame[name] ? frame[name] : null
        var value = Object.assign({ state: "unavailable", source: "unavailable", reason: Tr.t("Sin lectura del backend"), generated_at_ms: 0 }, defaults, raw || {})
        if (name === "catalog" && raw) value.state = raw.daemon_reachable ? "ready" : "stale"
        if (silent && raw) { value.state = "stale"; value.reason = reason }
        return value
    }

    function tick() { nowMs = Date.now() }

    // Returns true when the line was a valid frame of a schema this model understands.
    function ingest(line) {
        var f
        try { f = JSON.parse(line) } catch (e) { return reject("not JSON") }
        if (!object(f)) return reject("not an object")
        if (f.schema !== schema) return reject("schema " + f.schema + " (expected " + schema + ")")
        if (!object(f.link) || typeof f.link.state !== "string") return reject("no link state")
        if (["live", "fixture", "daemon_unreachable", "displayd_offline", "invalid"].indexOf(f.link.state) < 0) return reject("unknown link state")
        if (f.link.state !== "live" && f.link.state !== "fixture" && typeof f.link.reason !== "string") return reject("no link reason")
        if (f.link.state === "daemon_unreachable" && !number(f.link.stale_ms)) return reject("invalid stale age")
        if (!number(f.at_ms)) return reject("invalid frame time")
        if (f.system !== null && f.system !== undefined && !validSystem(f.system)) return reject("invalid system")
        if (f.kernel !== null && f.kernel !== undefined && !validKernel(f.kernel)) return reject("invalid kernel")
        if (f.lan !== undefined && !validLan(f.lan)) return reject("invalid lan")
        if (f.jobs !== undefined && !validJobs(f.jobs)) return reject("invalid jobs")
        if (f.catalog !== undefined && f.catalog !== null && !validCatalog(f.catalog)) return reject("invalid catalog")
        if (f.inspector !== undefined && !validInspector(f.inspector)) return reject("invalid inspector")
        if (f.workflows !== undefined && !validWorkflows(f.workflows)) return reject("invalid workflows")
        if (f.link.state === "fixture" && ((f.system && f.system.source !== "fixture") || (f.catalog && f.catalog.source !== "fixture") || (f.inspector && f.inspector.source === "live") || (f.workflows && f.workflows.source === "live"))) return reject("fixture provenance mismatch")
        if (f.system === undefined) f.system = null
        if (f.kernel === undefined) f.kernel = null
        frame = f
        receivedAtMs = nowMs
        return true
    }
    function reject(why) { rejected += 1; lastRejection = why; return false }
    function object(v) { return v !== null && typeof v === "object" && !Array.isArray(v) }
    function number(v) { return typeof v === "number" && isFinite(v) && v >= 0 }
    function strings(v) { return Array.isArray(v) && v.every(function(x) { return typeof x === "string" }) }
    function fields(v, keys) { return object(v) && keys.every(function(k) { return typeof v[k] === "string" }) }
    function numbers(v, keys) { return object(v) && keys.every(function(k) { return number(v[k]) }) }
    function counters(v, keys) { return numbers(v, keys) && keys.every(function(k) { return Math.floor(v[k]) === v[k] }) }
    function nullableString(v) { return v === null || typeof v === "string" }
    function validSystem(v) {
        return fields(v, ["hostname", "archieos_version", "health", "source"]) && ["live", "fixture"].indexOf(v.source) >= 0
            && numbers(v, ["modules_loaded", "modules_installed", "models_registered", "active_tasks", "peers", "reachable_peers"])
    }
    function availability(v) { return object(v) && (v.state === "available" || (v.state === "unavailable" && typeof v.reason === "string")) }
    function validKernel(v) {
        if (!object(v) || v.schema_version !== 1 || !number(v.read_at) || !availability(v.pressure_availability) || !availability(v.thermal_availability) || typeof v.display_budget !== "string") return false
        if (v.pressure_availability.state === "available") {
            if (!object(v.pressure)) return false
            if (!["cpu", "memory", "io"].every(function(k) { return object(v.pressure[k]) && numbers(v.pressure[k].some, ["avg10", "avg60", "avg300", "total_us"]) })) return false
        }
        if (v.thermal_availability.state === "available" && (!Array.isArray(v.zones) || !v.zones.every(function(z) { return fields(z,["zone_type", "state"]) && typeof z.celsius === "number" && isFinite(z.celsius) && (z.headroom_fraction === null || (number(z.headroom_fraction) && z.headroom_fraction <= 1)) }))) return false
        return true
    }
    function validLan(v) {
        if (!object(v)) return false
        if (v.state === "not_configured") return true
        if (v.state === "missing") return typeof v.path === "string"
        if (v.state === "unreadable" || v.state === "invalid") return typeof v.reason === "string"
        return v.state === "ok" && numbers(v,["scanned_unix", "age_seconds", "total"]) && typeof v.stale === "boolean"
            && Array.isArray(v.devices) && v.total >= v.devices.length && v.devices.length <= 512 && v.devices.every(function(d) {
                return fields(d,["address","name","manufacturer","model"]) && nullableString(d.class) && strings(d.services) && strings(d.seen_by)
                    && object(d.state) && Object.keys(d.state).every(function(k) { return typeof d.state[k] === "string" })
            })
    }
    function validJobs(v) {
        if (!object(v)) return false
        if (v.state === "not_configured") return true
        return v.state === "ok" && number(v.skipped) && Array.isArray(v.unreadable) && v.unreadable.every(function(x) { return strings(x) && x.length === 2 })
            && Array.isArray(v.jobs) && v.jobs.length <= 64 && v.jobs.every(function(j) {
                return fields(j,["workspace","id","bridge","kind","state"]) && ["starting","running","done","cancelled","failed"].indexOf(j.state) >= 0
                    && counters(j,["done","total","started_ms","updated_ms","idle_ms"]) && nullableString(j.error)
            })
    }
    function source(v) { return object(v) && ["live","fixture","unavailable"].indexOf(v.source) >= 0 }
    function validCatalog(v) {
        return source(v) && v.source !== "unavailable" && number(v.generated_at) && typeof v.daemon_reachable === "boolean" && number(v.staleness_ms) && nullableString(v.error)
            && Array.isArray(v.modules) && v.modules.every(function(x) { return fields(x,["id","version","state","explanation"]) && (x.dependencies === undefined || strings(x.dependencies)) })
            && Array.isArray(v.models) && v.models.every(function(x) { return fields(x,["id","provider","locality","availability","max_privacy_grade"]) && number(x.context_window) })
            && Array.isArray(v.nodes) && v.nodes.every(function(x) { return fields(x,["node_id","hostname","profile","trust","health"]) && typeof x.reachable === "boolean" && strings(x.capabilities) && number(x.last_seen) })
            && object(v.capabilities) && Object.keys(v.capabilities).every(function(k) { return strings(v.capabilities[k]) })
    }
    function projectionHeader(v, states) { return source(v) && states.indexOf(v.state) >= 0 && number(v.generated_at_ms) && nullableString(v.reason) && (v.source !== "unavailable" || v.state === "unavailable") }
    function validInspector(v) {
        const half = function(state, reason) { return (state === undefined || ["ready","unavailable","stale"].indexOf(state) >= 0) && (reason === undefined || reason === null || typeof reason === "string") }
        return projectionHeader(v,["ready","unavailable","stale"]) && half(v.providers_state, v.providers_reason) && half(v.trajectories_state, v.trajectories_reason) && Array.isArray(v.providers) && v.providers.every(function(p) { return fields(p,["id","title","capability","status"]) && strings(p.records) && strings(p.known_gaps) })
            && Array.isArray(v.cortex_trajectories) && v.cortex_trajectories.every(function(c) { return fields(c,["trajectory_id","correction_kind","source_graph_snapshot_id","resulting_graph_snapshot_id","model_id","payload_sha256_fingerprint"]) && numbers(c,["selected_context_count","omitted_candidate_count","captured_at"]) })
    }
    function validWorkflows(v) {
        return projectionHeader(v,["ready","unavailable","stale"]) && Array.isArray(v.workflows) && v.workflows.length <= 4 && v.workflows.every(function(w) {
            return fields(w,["component","phase"]) && ["wanda","cortex","vision","iris"].indexOf(w.component) >= 0
                && ["waiting","observe","consent","score","hypothesis","graph","evidence","capture","reflex","ground","verify","error"].indexOf(w.phase) >= 0
                && typeof w.active === "boolean" && typeof w.observed === "boolean" && number(w.count) && (w.age_ms === null || number(w.age_ms))
        })
    }
}
