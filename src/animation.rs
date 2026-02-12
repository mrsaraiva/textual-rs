use crate::event::{AnimationEase, AnimationLevel, AnimationRequest};
use crate::node_id::NodeId;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
struct NumericAnimation {
    target: NodeId,
    attribute: String,
    start: f32,
    end: f32,
    start_at: Instant,
    duration: Duration,
    ease: AnimationEase,
    level: AnimationLevel,
    last_value: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnimationUpdate {
    pub target: NodeId,
    pub attribute: String,
    pub value: f32,
    pub done: bool,
}

#[derive(Debug)]
pub struct Animator {
    animations: HashMap<(NodeId, String), NumericAnimation>,
    frame_interval: Duration,
}

impl Default for Animator {
    fn default() -> Self {
        Self::new(60)
    }
}

impl Animator {
    pub fn new(frames_per_second: u32) -> Self {
        let fps = frames_per_second.max(1);
        let frame_interval = Duration::from_secs_f32(1.0 / fps as f32);
        Self {
            animations: HashMap::new(),
            frame_interval,
        }
    }

    pub fn has_animations(&self) -> bool {
        !self.animations.is_empty()
    }

    pub fn is_being_animated(&self, target: NodeId, attribute: &str) -> bool {
        self.animations
            .contains_key(&(target, attribute.to_string()))
    }

    pub fn enqueue(&mut self, request: AnimationRequest, now: Instant) {
        let key = (request.target, request.attribute.clone());
        let start_at = now
            .checked_add(request.delay)
            .unwrap_or_else(|| now + request.delay);
        let animation = NumericAnimation {
            target: request.target,
            attribute: request.attribute,
            start: request.start,
            end: request.end,
            start_at,
            duration: request.duration,
            ease: request.ease,
            level: request.level,
            last_value: None,
        };
        self.animations.insert(key, animation);
    }

    pub fn enqueue_many(&mut self, requests: Vec<AnimationRequest>, now: Instant) {
        for request in requests {
            self.enqueue(request, now);
        }
    }

    pub fn next_timeout(&self, now: Instant) -> Option<Duration> {
        if self.animations.is_empty() {
            return None;
        }
        let mut timeout = self.frame_interval;
        for animation in self.animations.values() {
            if now < animation.start_at {
                let until_start = animation.start_at.saturating_duration_since(now);
                timeout = timeout.min(until_start);
            }
        }
        Some(timeout)
    }

    pub fn step(&mut self, now: Instant, app_level: AnimationLevel) -> Vec<AnimationUpdate> {
        let mut updates = Vec::new();
        let keys: Vec<(NodeId, String)> = self.animations.keys().cloned().collect();
        for key in keys {
            let mut remove = false;
            if let Some(animation) = self.animations.get_mut(&key) {
                if now < animation.start_at {
                    continue;
                }

                let skip_animation = matches!(app_level, AnimationLevel::None)
                    || matches!(app_level, AnimationLevel::Basic)
                        && matches!(animation.level, AnimationLevel::Full);

                let (value, done) = if skip_animation || animation.duration.is_zero() {
                    (animation.end, true)
                } else {
                    let elapsed = now.saturating_duration_since(animation.start_at);
                    let factor =
                        (elapsed.as_secs_f32() / animation.duration.as_secs_f32()).clamp(0.0, 1.0);
                    let eased = apply_easing(animation.ease, factor);
                    let value = animation.start + (animation.end - animation.start) * eased;
                    (value, factor >= 1.0)
                };

                let changed = animation
                    .last_value
                    .map(|previous| (previous - value).abs() > 0.000_1)
                    .unwrap_or(true);
                if changed || done {
                    updates.push(AnimationUpdate {
                        target: animation.target,
                        attribute: animation.attribute.clone(),
                        value,
                        done,
                    });
                    animation.last_value = Some(value);
                }
                if done {
                    remove = true;
                }
            }

            if remove {
                self.animations.remove(&key);
            }
        }
        updates
    }
}

pub fn animation_level_from_env() -> AnimationLevel {
    let value = std::env::var("TEXTUAL_ANIMATIONS")
        .unwrap_or_else(|_| "full".to_string())
        .to_lowercase();
    match value.as_str() {
        "none" => AnimationLevel::None,
        "basic" => AnimationLevel::Basic,
        _ => AnimationLevel::Full,
    }
}

/// Standard bounce-out helper used by InBounce, OutBounce, InOutBounce.
fn bounce_out(x: f32) -> f32 {
    const N1: f32 = 7.5625;
    const D1: f32 = 2.75;
    if x < 1.0 / D1 {
        N1 * x * x
    } else if x < 2.0 / D1 {
        let t = x - 1.5 / D1;
        N1 * t * t + 0.75
    } else if x < 2.5 / D1 {
        let t = x - 2.25 / D1;
        N1 * t * t + 0.9375
    } else {
        let t = x - 2.625 / D1;
        N1 * t * t + 0.984_375
    }
}

fn apply_easing(ease: AnimationEase, x: f32) -> f32 {
    use std::f32::consts::PI;

    match ease {
        // ── Existing ─────────────────────────────────────────────
        AnimationEase::None => 1.0,
        AnimationEase::Round => {
            if x < 0.5 {
                0.0
            } else {
                1.0
            }
        }
        AnimationEase::Linear => x,
        AnimationEase::OutCubic => 1.0 - (1.0 - x).powi(3),
        AnimationEase::InOutCubic => {
            if x < 0.5 {
                4.0 * x * x * x
            } else {
                1.0 - (-2.0 * x + 2.0).powi(3) / 2.0
            }
        }

        // ── Quad ─────────────────────────────────────────────────
        AnimationEase::InQuad => x * x,
        AnimationEase::OutQuad => 1.0 - (1.0 - x) * (1.0 - x),
        AnimationEase::InOutQuad => {
            if x < 0.5 {
                2.0 * x * x
            } else {
                1.0 - (-2.0 * x + 2.0).powi(2) / 2.0
            }
        }

        // ── Cubic (In only — Out and InOut already exist) ────────
        AnimationEase::InCubic => x * x * x,

        // ── Quart ────────────────────────────────────────────────
        AnimationEase::InQuart => x.powi(4),
        AnimationEase::OutQuart => 1.0 - (1.0 - x).powi(4),
        AnimationEase::InOutQuart => {
            if x < 0.5 {
                8.0 * x.powi(4)
            } else {
                1.0 - (-2.0 * x + 2.0).powi(4) / 2.0
            }
        }

        // ── Quint ────────────────────────────────────────────────
        AnimationEase::InQuint => x.powi(5),
        AnimationEase::OutQuint => 1.0 - (1.0 - x).powi(5),
        AnimationEase::InOutQuint => {
            if x < 0.5 {
                16.0 * x.powi(5)
            } else {
                1.0 - (-2.0 * x + 2.0).powi(5) / 2.0
            }
        }

        // ── Expo ─────────────────────────────────────────────────
        AnimationEase::InExpo => {
            if x == 0.0 { 0.0 } else { 2.0_f32.powf(10.0 * x - 10.0) }
        }
        AnimationEase::OutExpo => {
            if x == 1.0 { 1.0 } else { 1.0 - 2.0_f32.powf(-10.0 * x) }
        }
        AnimationEase::InOutExpo => {
            if x == 0.0 {
                0.0
            } else if x == 1.0 {
                1.0
            } else if x < 0.5 {
                2.0_f32.powf(20.0 * x - 10.0) / 2.0
            } else {
                (2.0 - 2.0_f32.powf(-20.0 * x + 10.0)) / 2.0
            }
        }

        // ── Circ ─────────────────────────────────────────────────
        AnimationEase::InCirc => 1.0 - (1.0 - x * x).sqrt(),
        AnimationEase::OutCirc => (1.0 - (x - 1.0).powi(2)).sqrt(),
        AnimationEase::InOutCirc => {
            if x < 0.5 {
                (1.0 - (1.0 - (2.0 * x).powi(2)).sqrt()) / 2.0
            } else {
                ((1.0 - (-2.0 * x + 2.0).powi(2)).sqrt() + 1.0) / 2.0
            }
        }

        // ── Back (overshoot) ─────────────────────────────────────
        AnimationEase::InBack => {
            const C1: f32 = 1.70158;
            const C3: f32 = C1 + 1.0;
            C3 * x * x * x - C1 * x * x
        }
        AnimationEase::OutBack => {
            const C1: f32 = 1.70158;
            const C3: f32 = C1 + 1.0;
            let t = x - 1.0;
            1.0 + C3 * t * t * t + C1 * t * t
        }
        AnimationEase::InOutBack => {
            const C1: f32 = 1.70158;
            const C2: f32 = C1 * 1.525;
            if x < 0.5 {
                ((2.0 * x).powi(2) * ((C2 + 1.0) * 2.0 * x - C2)) / 2.0
            } else {
                ((2.0 * x - 2.0).powi(2) * ((C2 + 1.0) * (2.0 * x - 2.0) + C2) + 2.0) / 2.0
            }
        }

        // ── Bounce ───────────────────────────────────────────────
        AnimationEase::OutBounce => bounce_out(x),
        AnimationEase::InBounce => 1.0 - bounce_out(1.0 - x),
        AnimationEase::InOutBounce => {
            if x < 0.5 {
                (1.0 - bounce_out(1.0 - 2.0 * x)) / 2.0
            } else {
                (1.0 + bounce_out(2.0 * x - 1.0)) / 2.0
            }
        }

        // ── Elastic ──────────────────────────────────────────────
        AnimationEase::InElastic => {
            let c4 = (2.0 * PI) / 3.0;
            if x == 0.0 {
                0.0
            } else if x == 1.0 {
                1.0
            } else {
                -(2.0_f32.powf(10.0 * x - 10.0) * ((10.0 * x - 10.75) * c4).sin())
            }
        }
        AnimationEase::OutElastic => {
            let c4 = (2.0 * PI) / 3.0;
            if x == 0.0 {
                0.0
            } else if x == 1.0 {
                1.0
            } else {
                2.0_f32.powf(-10.0 * x) * ((10.0 * x - 0.75) * c4).sin() + 1.0
            }
        }
        AnimationEase::InOutElastic => {
            let c5 = (2.0 * PI) / 4.5;
            if x == 0.0 {
                0.0
            } else if x == 1.0 {
                1.0
            } else if x < 0.5 {
                -(2.0_f32.powf(20.0 * x - 10.0) * ((20.0 * x - 11.125) * c5).sin()) / 2.0
            } else {
                (2.0_f32.powf(-20.0 * x + 10.0) * ((20.0 * x - 11.125) * c5).sin()) / 2.0 + 1.0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{AnimationEase, AnimationRequest};
    use crate::node_id::node_id_from_ffi;

    #[test]
    fn animator_interpolates_in_out_cubic() {
        let mut animator = Animator::new(60);
        let widget = node_id_from_ffi(1);
        let now = Instant::now();
        animator.enqueue(
            AnimationRequest::new(widget, "x", 0.0, 10.0, Duration::from_millis(300))
                .with_ease(AnimationEase::InOutCubic),
            now,
        );
        let updates = animator.step(now + Duration::from_millis(150), AnimationLevel::Full);
        assert!(!updates.is_empty());
        let value = updates.last().map(|u| u.value).unwrap_or_default();
        assert!(value > 4.0 && value < 6.0);
    }

    #[test]
    fn animator_respects_none_level() {
        let mut animator = Animator::new(60);
        let widget = node_id_from_ffi(2);
        let now = Instant::now();
        animator.enqueue(
            AnimationRequest::new(widget, "x", 0.0, 10.0, Duration::from_millis(300)),
            now,
        );
        let updates = animator.step(now, AnimationLevel::None);
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].value, 10.0);
        assert!(updates[0].done);
        assert!(!animator.has_animations());
    }

    // ── Easing function tests ────────────────────────────────────────

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    /// All "normal" easings (not None/Round) should satisfy f(0)≈0 and f(1)≈1.
    #[test]
    fn easing_boundary_conditions() {
        let normal_easings = [
            AnimationEase::Linear,
            AnimationEase::OutCubic,
            AnimationEase::InOutCubic,
            AnimationEase::InQuad,
            AnimationEase::OutQuad,
            AnimationEase::InOutQuad,
            AnimationEase::InCubic,
            AnimationEase::InQuart,
            AnimationEase::OutQuart,
            AnimationEase::InOutQuart,
            AnimationEase::InQuint,
            AnimationEase::OutQuint,
            AnimationEase::InOutQuint,
            AnimationEase::InExpo,
            AnimationEase::OutExpo,
            AnimationEase::InOutExpo,
            AnimationEase::InCirc,
            AnimationEase::OutCirc,
            AnimationEase::InOutCirc,
            AnimationEase::InBack,
            AnimationEase::OutBack,
            AnimationEase::InOutBack,
            AnimationEase::InBounce,
            AnimationEase::OutBounce,
            AnimationEase::InOutBounce,
            AnimationEase::InElastic,
            AnimationEase::OutElastic,
            AnimationEase::InOutElastic,
        ];
        for ease in &normal_easings {
            let at_0 = apply_easing(*ease, 0.0);
            let at_1 = apply_easing(*ease, 1.0);
            assert!(
                approx_eq(at_0, 0.0),
                "{ease:?} at 0.0 = {at_0}, expected ≈0"
            );
            assert!(
                approx_eq(at_1, 1.0),
                "{ease:?} at 1.0 = {at_1}, expected ≈1"
            );
        }
    }

    #[test]
    fn easing_none_is_instant() {
        assert_eq!(apply_easing(AnimationEase::None, 0.0), 1.0);
        assert_eq!(apply_easing(AnimationEase::None, 0.5), 1.0);
        assert_eq!(apply_easing(AnimationEase::None, 1.0), 1.0);
    }

    #[test]
    fn easing_round_is_step() {
        assert_eq!(apply_easing(AnimationEase::Round, 0.0), 0.0);
        assert_eq!(apply_easing(AnimationEase::Round, 0.49), 0.0);
        assert_eq!(apply_easing(AnimationEase::Round, 0.5), 1.0);
        assert_eq!(apply_easing(AnimationEase::Round, 1.0), 1.0);
    }

    #[test]
    fn easing_linear_is_identity() {
        for &t in &[0.0, 0.25, 0.5, 0.75, 1.0] {
            assert!(approx_eq(apply_easing(AnimationEase::Linear, t), t));
        }
    }

    #[test]
    fn easing_in_quad_midpoint() {
        let mid = apply_easing(AnimationEase::InQuad, 0.5);
        assert!(approx_eq(mid, 0.25)); // 0.5^2
    }

    #[test]
    fn easing_out_quad_midpoint() {
        let mid = apply_easing(AnimationEase::OutQuad, 0.5);
        assert!(approx_eq(mid, 0.75)); // 1 - (0.5)^2
    }

    #[test]
    fn easing_in_out_quad_symmetry() {
        let a = apply_easing(AnimationEase::InOutQuad, 0.25);
        let b = apply_easing(AnimationEase::InOutQuad, 0.75);
        assert!(approx_eq(a + b, 1.0));
    }

    #[test]
    fn easing_in_cubic_midpoint() {
        let mid = apply_easing(AnimationEase::InCubic, 0.5);
        assert!(approx_eq(mid, 0.125)); // 0.5^3
    }

    #[test]
    fn easing_in_quart_midpoint() {
        let mid = apply_easing(AnimationEase::InQuart, 0.5);
        assert!(approx_eq(mid, 0.0625)); // 0.5^4
    }

    #[test]
    fn easing_out_quart_midpoint() {
        let mid = apply_easing(AnimationEase::OutQuart, 0.5);
        assert!(approx_eq(mid, 0.9375)); // 1 - 0.5^4
    }

    #[test]
    fn easing_in_quint_midpoint() {
        let mid = apply_easing(AnimationEase::InQuint, 0.5);
        assert!(approx_eq(mid, 0.03125)); // 0.5^5
    }

    #[test]
    fn easing_out_quint_midpoint() {
        let mid = apply_easing(AnimationEase::OutQuint, 0.5);
        assert!(approx_eq(mid, 0.96875)); // 1 - 0.5^5
    }

    #[test]
    fn easing_in_expo_near_zero() {
        let v = apply_easing(AnimationEase::InExpo, 0.1);
        assert!(v < 0.01); // very slow start
    }

    #[test]
    fn easing_out_expo_near_one() {
        let v = apply_easing(AnimationEase::OutExpo, 0.9);
        assert!(v > 0.99); // nearly done
    }

    #[test]
    fn easing_in_circ_midpoint() {
        let mid = apply_easing(AnimationEase::InCirc, 0.5);
        // 1 - sqrt(1 - 0.25) = 1 - sqrt(0.75) ≈ 0.1340
        assert!(mid > 0.13 && mid < 0.14);
    }

    #[test]
    fn easing_out_circ_midpoint() {
        let mid = apply_easing(AnimationEase::OutCirc, 0.5);
        // sqrt(1 - (-0.5)^2) = sqrt(0.75) ≈ 0.866
        assert!(mid > 0.86 && mid < 0.87);
    }

    #[test]
    fn easing_back_overshoots() {
        // InBack goes negative before 0
        let v = apply_easing(AnimationEase::InBack, 0.1);
        assert!(v < 0.0, "InBack should overshoot below 0 near start");
        // OutBack goes above 1 before settling
        let v = apply_easing(AnimationEase::OutBack, 0.9);
        assert!(v > 1.0, "OutBack should overshoot above 1 near end");
    }

    #[test]
    fn easing_bounce_out_midpoint() {
        let mid = apply_easing(AnimationEase::OutBounce, 0.5);
        assert!(mid > 0.7 && mid < 0.8);
    }

    #[test]
    fn easing_elastic_oscillates() {
        // InElastic goes negative near the start (oscillation)
        let v = apply_easing(AnimationEase::InElastic, 0.3);
        assert!(v < 0.05);
        // OutElastic goes above 1 near the end (oscillation)
        let v = apply_easing(AnimationEase::OutElastic, 0.7);
        assert!(v > 0.95);
    }

    /// Monotonicity check for simple polynomial easings (no overshoot/oscillation).
    #[test]
    fn easing_monotonic_polynomial() {
        let monotonic = [
            AnimationEase::Linear,
            AnimationEase::InQuad,
            AnimationEase::OutQuad,
            AnimationEase::InOutQuad,
            AnimationEase::InCubic,
            AnimationEase::OutCubic,
            AnimationEase::InOutCubic,
            AnimationEase::InQuart,
            AnimationEase::OutQuart,
            AnimationEase::InOutQuart,
            AnimationEase::InQuint,
            AnimationEase::OutQuint,
            AnimationEase::InOutQuint,
            AnimationEase::InCirc,
            AnimationEase::OutCirc,
            AnimationEase::InOutCirc,
            AnimationEase::InExpo,
            AnimationEase::OutExpo,
            AnimationEase::InOutExpo,
        ];
        let steps = 100;
        for ease in &monotonic {
            let mut prev = apply_easing(*ease, 0.0);
            for i in 1..=steps {
                let t = i as f32 / steps as f32;
                let cur = apply_easing(*ease, t);
                assert!(
                    cur >= prev - 1e-5,
                    "{ease:?} not monotonic at t={t}: {prev} -> {cur}"
                );
                prev = cur;
            }
        }
    }
}
