//! Speed scoring: a logistic curve, normalised to 0..=1000.
//!
//! `t` is the fraction of the question window still available when the last
//! accepted answer arrives. Wrong or missing answers score 0 (the caller's gate).

pub const MAX_POINTS: i64 = 1000;
/// Steepness of the logistic (plan §6, proposed `k = 8`).
pub const K: f64 = 8.0;
/// Midpoint of the logistic (plan §6, proposed `m = 0.5`).
pub const M: f64 = 0.5;

fn logistic(x: f64) -> f64 {
    1.0 / (1.0 + (-K * (x - M)).exp())
}

/// Points for a correct answer received with `remaining_ms` left out of `limit_ms`.
pub fn points_for(remaining_ms: i64, limit_ms: i64) -> i64 {
    if limit_ms <= 0 {
        return 0;
    }
    let t = (remaining_ms as f64 / limit_ms as f64).clamp(0.0, 1.0);
    let (s0, s1) = (logistic(0.0), logistic(1.0));
    let normalised = (logistic(t) - s0) / (s1 - s0);
    ((MAX_POINTS as f64) * normalised).round() as i64
}

/// Gate: only a correct answer inside the window scores.
pub fn award(is_correct: bool, remaining_ms: i64, limit_ms: i64) -> i64 {
    if is_correct {
        points_for(remaining_ms, limit_ms)
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_are_exact() {
        assert_eq!(points_for(0, 20_000), 0);
        assert_eq!(points_for(20_000, 20_000), 1000);
    }

    #[test]
    fn midpoint_is_half() {
        assert_eq!(points_for(10_000, 20_000), 500);
    }

    #[test]
    fn monotonic_and_bounded() {
        let mut previous = -1;
        for step in 0..=100 {
            let remaining = step * 200;
            let p = points_for(remaining, 20_000);
            assert!((0..=1000).contains(&p));
            assert!(p >= previous, "not monotonic at {remaining}ms");
            previous = p;
        }
    }

    #[test]
    fn clamps_out_of_range_inputs() {
        assert_eq!(points_for(-500, 20_000), 0);
        assert_eq!(points_for(99_000, 20_000), 1000);
        assert_eq!(points_for(1_000, 0), 0);
    }

    #[test]
    fn wrong_answer_scores_zero() {
        assert_eq!(award(false, 20_000, 20_000), 0);
        assert_eq!(award(true, 20_000, 20_000), 1000);
    }
}
