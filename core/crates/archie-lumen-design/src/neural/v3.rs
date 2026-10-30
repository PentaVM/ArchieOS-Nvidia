//! Three-vectors in doubles with three.js's `Vector3` arithmetic, operation by operation, so the layout and the
//! curves come out as the original computed them.

pub type V3 = [f64; 3];

pub fn scale(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

/// `a.addScaledVector(v, s)`.
pub fn add_scaled(a: V3, v: V3, s: f64) -> V3 {
    [a[0] + v[0] * s, a[1] + v[1] * s, a[2] + v[2] * s]
}

pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn length_sq(a: V3) -> f64 {
    a[0] * a[0] + a[1] * a[1] + a[2] * a[2]
}

pub fn length(a: V3) -> f64 {
    length_sq(a).sqrt()
}

/// `a.normalize()`: divides by the length, or by 1 for the zero vector.
pub fn normalize(a: V3) -> V3 {
    let l = length(a);
    scale(a, 1.0 / if l == 0.0 { 1.0 } else { l })
}

/// `a.distanceTo(b)`.
pub fn distance(a: V3, b: V3) -> f64 {
    let (dx, dy, dz) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// `new Vector3().crossVectors(a, b)`.
pub fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// `a.clone().lerp(b, t)`.
pub fn lerp(a: V3, b: V3, t: f64) -> V3 {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// model.ts `basis`: two vectors across `dir`.
pub fn basis(dir: V3) -> (V3, V3) {
    let up = if dir[1].abs() < 0.9 {
        [0.0, 1.0, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let u = normalize(cross(dir, up));
    (u, cross(dir, u))
}

/// `new CubicBezierCurve3(p0, p1, p2, p3).getPoints(divisions)`.
pub fn bezier_points(p0: V3, p1: V3, p2: V3, p3: V3, divisions: usize) -> Vec<V3> {
    (0..=divisions)
        .map(|d| {
            let t = d as f64 / divisions as f64;
            let k = 1.0 - t;
            let at = |i: usize| {
                k * k * k * p0[i]
                    + 3.0 * k * k * t * p1[i]
                    + 3.0 * (1.0 - t) * t * t * p2[i]
                    + t * t * t * p3[i]
            };
            [at(0), at(1), at(2)]
        })
        .collect()
}
