//! The neural sphere's motion: per-frame smoothing with a speed cap, and the live build's schedule (motion.ts).

use std::collections::HashMap;

use super::v3::V3;

/// Distance, in radii, at which a node counts as settled on its target.
pub const SETTLE_DISTANCE: f64 = 0.002;
/// Longest live build, in seconds.
pub const MAX_BUILD_SECONDS: f64 = 12.0;
/// Fastest a node moves, in radii per second (0.05 R per frame at 60 fps).
pub const MAX_SPEED: f64 = 3.0;

/// The fraction of the way covered in `dt` seconds with time constant `response`, whatever the frame rate.
pub fn smooth_factor(dt: f64, response: f64) -> f64 {
    if dt.is_nan() || dt <= 0.0 {
        return 0.0;
    }
    1.0 - (-dt / response).exp()
}

/// Moves `cur` the fraction `factor` of what is left, never more than `max_distance`; within `settle` it lands
/// on the target. Returns the distance covered.
pub fn step_capped(cur: &mut V3, target: V3, factor: f64, max_distance: f64, settle: f64) -> f64 {
    let d = ((target[0] - cur[0]).powi(2)
        + (target[1] - cur[1]).powi(2)
        + (target[2] - cur[2]).powi(2))
    .sqrt();
    if d <= settle {
        *cur = target;
        return d;
    }
    let step = (d * factor).min(max_distance);
    let t = step / d;
    for a in 0..3 {
        cur[a] += (target[a] - cur[a]) * t;
    }
    step
}

/// When each node is born: breadth first from the root (depth, then id), spread evenly over `duration` seconds
/// (at most 12). A parent is always born before its children.
pub fn birth_schedule(
    nodes: &[(String, Option<String>, u32)],
    duration: f64,
) -> HashMap<String, f64> {
    let total = MAX_BUILD_SECONDS.min(duration.max(0.0));
    let mut order: Vec<&(String, Option<String>, u32)> = nodes.iter().collect();
    // The original compares ids with < and >: code-unit order.
    order.sort_by(|a, b| {
        a.2.cmp(&b.2)
            .then_with(|| a.0.encode_utf16().cmp(b.0.encode_utf16()))
    });
    let last = order.len().saturating_sub(1);
    order
        .iter()
        .enumerate()
        .map(|(rank, n)| {
            (
                n.0.clone(),
                if last > 0 {
                    total * rank as f64 / last as f64
                } else {
                    0.0
                },
            )
        })
        .collect()
}

/// How far a node born at `born_at` has appeared (0 to 1), over a linear ramp of `ramp` seconds (0.5).
pub fn born_progress(now: f64, born_at: f64) -> f64 {
    if now <= born_at {
        return 0.0;
    }
    ((now - born_at) / 0.5).min(1.0)
}
