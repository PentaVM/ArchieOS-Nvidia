import QtQuick
import "WorldStars.js" as Stars

// World's sky: the real stars and constellations behind the globe, where they are now.
// Each star is a catalogue position turned by Greenwich sidereal time into the globe's
// Earth-fixed frame, the same frame the real sun already uses, then projected with the
// globe camera's own pose. Anything the Earth (with its atmosphere) hides stays hidden.
// The sky fades out as the camera comes down, so streets and terrain are never starred.
Canvas {
    id: sky
    property var controller: null
    property bool active: true
    property real globeRadius: 20
    // Earth plus the atmosphere shell drawn around it.
    property real occluderRadius: 20.4
    property var placed: Stars.sky(Date.now())
    property string painted: ""
    readonly property var frame: controller && controller.frame ? controller.frame : ({})
    readonly property real altitudeKm: {
        const e = frame.cameraEye
        return e ? Math.max(0, (Math.sqrt(e.x * e.x + e.y * e.y + e.z * e.z) - globeRadius) * 6371.0088 / globeRadius) : 0
    }
    opacity: Math.max(0, Math.min(1, (altitudeKm - 300) / 1700))
    visible: active && opacity > 0
    renderStrategy: Canvas.Cooperative

    // The sky turns a quarter of a degree a minute.
    Timer { interval: 60000; repeat: true; running: sky.visible; onTriggered: { sky.placed = Stars.sky(Date.now()); sky.requestPaint() } }
    Connections {
        target: sky.controller
        ignoreUnknownSignals: true
        function onFrameChanged() { sky.repaintIfMoved() }
    }
    onWidthChanged: requestPaint()
    onHeightChanged: requestPaint()
    onVisibleChanged: if (visible) requestPaint()

    function repaintIfMoved() {
        const e = frame.cameraEye, t = frame.cameraTarget
        if (!visible || !e || !t) return
        const key = [e.x, e.y, e.z, t.x, t.y, t.z, frame.cameraFov].map(function(v) { return Number(v).toFixed(4) }).join()
        if (key !== painted) requestPaint()
    }
    function normalize(v) {
        const l = Math.sqrt(v.x * v.x + v.y * v.y + v.z * v.z)
        return l > 0 ? {x: v.x / l, y: v.y / l, z: v.z / l} : null
    }
    function cross(a, b) { return {x: a.y * b.z - a.z * b.y, y: a.z * b.x - a.x * b.z, z: a.x * b.y - a.y * b.x} }
    function dot(a, b) { return a.x * b.x + a.y * b.y + a.z * b.z }

    onPaint: {
        const ctx = getContext("2d")
        ctx.clearRect(0, 0, width, height)
        const e = frame.cameraEye, t = frame.cameraTarget
        if (!e || !t || width <= 0 || height <= 0) return
        painted = [e.x, e.y, e.z, t.x, t.y, t.z, frame.cameraFov].map(function(v) { return Number(v).toFixed(4) }).join()
        // The camera's basis, as Node.lookAt builds it: forward to the target, Y up.
        const f = normalize({x: t.x - e.x, y: t.y - e.y, z: t.z - e.z})
        if (!f) return
        let r = normalize(cross(f, {x: 0, y: 1, z: 0}))
        if (!r) r = {x: 1, y: 0, z: 0}
        const u = cross(r, f)
        const scale = (height / 2) / Math.tan((frame.cameraFov || 45) * Math.PI / 360)
        const eye2 = dot(e, e) - occluderRadius * occluderRadius
        function project(d) {
            const z = dot(d, f)
            if (z <= 0.02) return null
            // Behind the Earth: the ray from the eye meets the globe first.
            const b = dot(e, d), disc = b * b - eye2
            if (disc >= 0 && -b - Math.sqrt(disc) > 0) return null
            return {x: width / 2 + dot(d, r) / z * scale, y: height / 2 - dot(d, u) / z * scale}
        }
        // The faint field first, as single soft pixels.
        placed.faint.forEach(function(s) {
            const p = project(s)
            if (!p || p.x < 0 || p.y < 0 || p.x > width || p.y > height) return
            const alpha = Math.max(0.2, Math.min(0.75, 1.45 - 0.18 * s.mag))
            const size = s.mag < 4.6 ? 1.6 : 1.1
            ctx.fillStyle = "rgba(220, 230, 255, " + alpha.toFixed(2) + ")"
            ctx.fillRect(p.x - size / 2, p.y - size / 2, size, size)
        })
        // Then the figures, faint, so the named stars sit on top of their own lines.
        ctx.lineWidth = 1
        ctx.strokeStyle = "rgba(150, 185, 255, 0.28)"
        ctx.beginPath()
        const labels = []
        placed.figures.forEach(function(figure) {
            figure.segments.forEach(function(s) {
                const a = project(s[0]), b = project(s[1])
                if (a && b) { ctx.moveTo(a.x, a.y); ctx.lineTo(b.x, b.y) }
            })
            const c = project(figure.centre)
            if (c) labels.push({text: figure.name, x: c.x, y: c.y})
        })
        ctx.stroke()
        // Names only from far out, where whole figures fit on the screen.
        if (altitudeKm > 6000) {
            ctx.fillStyle = "rgba(170, 200, 255, 0.42)"
            ctx.font = "500 11px sans-serif"
            ctx.textAlign = "center"
            labels.forEach(function(l) { ctx.fillText(l.text.toUpperCase(), l.x, l.y + 18) })
        }
        // Stars: size and brightness from magnitude; the brightest get a small halo.
        placed.stars.forEach(function(s) {
            const p = project(s)
            if (!p || p.x < -4 || p.y < -4 || p.x > width + 4 || p.y > height + 4) return
            const radius = Math.max(0.7, Math.min(2.6, 2.3 - 0.42 * s.mag))
            const alpha = Math.max(0.35, Math.min(1, 1.05 - 0.17 * s.mag))
            if (s.mag < 1.0) {
                const halo = ctx.createRadialGradient(p.x, p.y, 0, p.x, p.y, radius * 4)
                halo.addColorStop(0, "rgba(200, 220, 255, 0.35)")
                halo.addColorStop(1, "rgba(200, 220, 255, 0)")
                ctx.fillStyle = halo
                ctx.beginPath(); ctx.arc(p.x, p.y, radius * 4, 0, 2 * Math.PI); ctx.fill()
            }
            ctx.fillStyle = "rgba(235, 242, 255, " + alpha.toFixed(2) + ")"
            ctx.beginPath(); ctx.arc(p.x, p.y, radius, 0, 2 * Math.PI); ctx.fill()
        })
    }
}
