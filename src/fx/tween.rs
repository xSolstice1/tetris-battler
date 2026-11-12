//! Easing curves. Input `t` is clamped to 0..=1.

/// Fast start, gentle stop.
pub fn out_cubic(t: f32) -> f32 {
    let u = 1.0 - t.clamp(0.0, 1.0);
    1.0 - u * u * u
}

/// Overshoots past 1.0 then settles. `overshoot` ~1.7 is a lively pop (~10% past),
/// ~0.9 a subtle settle (~3% past).
fn out_back_by(t: f32, overshoot: f32) -> f32 {
    let c3 = overshoot + 1.0;
    let u = t.clamp(0.0, 1.0) - 1.0;
    1.0 + c3 * u * u * u + overshoot * u * u
}

/// Lively pop, for text and UI.
pub fn out_back(t: f32) -> f32 {
    out_back_by(t, 1.70158)
}

/// Subtle settle, for things that must not visibly overlap (falling rows).
pub fn out_back_soft(t: f32) -> f32 {
    out_back_by(t, 0.9)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_hit_endpoints() {
        for f in [out_cubic, out_back, out_back_soft] {
            assert!(f(0.0).abs() < 1e-5);
            assert!((f(1.0) - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn back_curves_overshoot_by_expected_amount() {
        let peak = |f: fn(f32) -> f32| (0..=1000).map(|i| f(i as f32 / 1000.0)).fold(0.0, f32::max);
        assert!(peak(out_back) > 1.08);
        let soft = peak(out_back_soft);
        assert!(soft > 1.0 && soft < 1.05, "soft peak {soft}");
    }
}
