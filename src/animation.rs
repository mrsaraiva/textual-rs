use crate::event::{
    AnimationEase, AnimationLevel, AnimationRequest, StyleAnimationRequest, StyleValue,
};
use crate::node_id::NodeId;
use crate::style::{Color, Scalar, Spacing, Style, Tint};
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

#[derive(Debug, Clone)]
struct StyleAnimation {
    target: NodeId,
    property: String,
    from: StyleValue,
    to: StyleValue,
    start_at: Instant,
    duration: Duration,
    ease: AnimationEase,
    level: AnimationLevel,
    last_value: Option<StyleValue>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnimationUpdate {
    pub target: NodeId,
    pub attribute: String,
    pub value: f32,
    pub done: bool,
}

/// Output of a style property animation tick.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleAnimationUpdate {
    pub target: NodeId,
    pub property: String,
    pub value: StyleValue,
    pub done: bool,
}

#[derive(Debug)]
pub struct Animator {
    animations: HashMap<(NodeId, String), NumericAnimation>,
    style_animations: HashMap<(NodeId, String), StyleAnimation>,
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
            style_animations: HashMap::new(),
            frame_interval,
        }
    }

    pub fn has_animations(&self) -> bool {
        !self.animations.is_empty() || !self.style_animations.is_empty()
    }

    pub fn is_being_animated(&self, target: NodeId, attribute: &str) -> bool {
        let key = (target, attribute.to_string());
        self.animations.contains_key(&key) || self.style_animations.contains_key(&key)
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
        if self.animations.is_empty() && self.style_animations.is_empty() {
            return None;
        }
        let mut timeout = self.frame_interval;
        for animation in self.animations.values() {
            if now < animation.start_at {
                let until_start = animation.start_at.saturating_duration_since(now);
                timeout = timeout.min(until_start);
            }
        }
        for animation in self.style_animations.values() {
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

    // ── Style property animation ──────────────────────────────────────

    pub fn enqueue_style(&mut self, request: StyleAnimationRequest, now: Instant) {
        let key = (request.target, request.property.clone());
        let start_at = now
            .checked_add(request.delay)
            .unwrap_or_else(|| now + request.delay);
        let animation = StyleAnimation {
            target: request.target,
            property: request.property,
            from: request.from,
            to: request.to,
            start_at,
            duration: request.duration,
            ease: request.ease,
            level: request.level,
            last_value: None,
        };
        self.style_animations.insert(key, animation);
    }

    pub fn enqueue_style_many(&mut self, requests: Vec<StyleAnimationRequest>, now: Instant) {
        for request in requests {
            self.enqueue_style(request, now);
        }
    }

    pub fn step_style(
        &mut self,
        now: Instant,
        app_level: AnimationLevel,
    ) -> Vec<StyleAnimationUpdate> {
        let mut updates = Vec::new();
        let keys: Vec<(NodeId, String)> = self.style_animations.keys().cloned().collect();
        for key in keys {
            let mut remove = false;
            if let Some(animation) = self.style_animations.get_mut(&key) {
                if now < animation.start_at {
                    continue;
                }

                let skip_animation = matches!(app_level, AnimationLevel::None)
                    || matches!(app_level, AnimationLevel::Basic)
                        && matches!(animation.level, AnimationLevel::Full);

                let (value, done) = if skip_animation || animation.duration.is_zero() {
                    (animation.to.clone(), true)
                } else {
                    let elapsed = now.saturating_duration_since(animation.start_at);
                    let factor =
                        (elapsed.as_secs_f32() / animation.duration.as_secs_f32()).clamp(0.0, 1.0);
                    let eased = apply_easing(animation.ease, factor);
                    let value = interpolate_style_value(&animation.from, &animation.to, eased)
                        .unwrap_or_else(|| animation.to.clone());
                    (value, factor >= 1.0)
                };

                let changed = animation
                    .last_value
                    .as_ref()
                    .map(|previous| *previous != value)
                    .unwrap_or(true);
                if changed || done {
                    updates.push(StyleAnimationUpdate {
                        target: animation.target,
                        property: animation.property.clone(),
                        value: value.clone(),
                        done,
                    });
                    animation.last_value = Some(value);
                }
                if done {
                    remove = true;
                }
            }

            if remove {
                self.style_animations.remove(&key);
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
            if x == 0.0 {
                0.0
            } else {
                2.0_f32.powf(10.0 * x - 10.0)
            }
        }
        AnimationEase::OutExpo => {
            if x == 1.0 {
                1.0
            } else {
                1.0 - 2.0_f32.powf(-10.0 * x)
            }
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

// ── Style property interpolation ─────────────────────────────────────────

/// Linearly interpolate between two colors (RGBA channels independently).
pub fn interpolate_color(from: Color, to: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let lerp = |a: u8, b: u8| -> u8 {
        let v = a as f32 + (b as f32 - a as f32) * t;
        v.round().clamp(0.0, 255.0) as u8
    };
    Color::rgba(
        lerp(from.r, to.r),
        lerp(from.g, to.g),
        lerp(from.b, to.b),
        lerp(from.a, to.a),
    )
}

/// Linearly interpolate two f32 values.
pub fn interpolate_f32(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t.clamp(0.0, 1.0)
}

/// Interpolate two Scalar values. Only same-unit interpolation is supported.
/// Returns `None` if units differ or either value is `Auto`.
pub fn interpolate_scalar(from: &Scalar, to: &Scalar, t: f32) -> Option<Scalar> {
    let t = t.clamp(0.0, 1.0);
    match (from, to) {
        (Scalar::Cells(a), Scalar::Cells(b)) => {
            let v = *a as f32 + (*b as f32 - *a as f32) * t;
            Some(Scalar::Cells(v.round().clamp(0.0, u16::MAX as f32) as u16))
        }
        (Scalar::Percent(a), Scalar::Percent(b)) => {
            Some(Scalar::Percent(interpolate_f32(*a, *b, t)))
        }
        (Scalar::Fraction(a), Scalar::Fraction(b)) => {
            Some(Scalar::Fraction(interpolate_f32(*a, *b, t)))
        }
        (Scalar::ViewWidth(a), Scalar::ViewWidth(b)) => {
            Some(Scalar::ViewWidth(interpolate_f32(*a, *b, t)))
        }
        (Scalar::ViewHeight(a), Scalar::ViewHeight(b)) => {
            Some(Scalar::ViewHeight(interpolate_f32(*a, *b, t)))
        }
        _ => None,
    }
}

/// Interpolate two Spacing values (per-side independently).
pub fn interpolate_spacing(from: &Spacing, to: &Spacing, t: f32) -> Spacing {
    let t = t.clamp(0.0, 1.0);
    let lerp_u16 = |a: u16, b: u16| -> u16 {
        let v = a as f32 + (b as f32 - a as f32) * t;
        v.round().clamp(0.0, u16::MAX as f32) as u16
    };
    Spacing::new(
        lerp_u16(from.top, to.top),
        lerp_u16(from.right, to.right),
        lerp_u16(from.bottom, to.bottom),
        lerp_u16(from.left, to.left),
    )
}

/// Interpolate two Tint values (color + percent).
pub fn interpolate_tint(from: &Tint, to: &Tint, t: f32) -> Tint {
    let color = interpolate_color(from.color, to.color, t);
    let percent = interpolate_f32(from.percent as f32, to.percent as f32, t)
        .round()
        .clamp(0.0, 100.0) as u8;
    Tint::new(color, percent)
}

/// Interpolate a `StyleValue`. Returns `None` if the variant types differ
/// or the values cannot be interpolated (e.g. different `Scalar` units).
pub fn interpolate_style_value(from: &StyleValue, to: &StyleValue, t: f32) -> Option<StyleValue> {
    match (from, to) {
        (StyleValue::Color(a), StyleValue::Color(b)) => {
            Some(StyleValue::Color(interpolate_color(*a, *b, t)))
        }
        (StyleValue::Float(a), StyleValue::Float(b)) => {
            Some(StyleValue::Float(interpolate_f32(*a, *b, t)))
        }
        (StyleValue::Scalar(a), StyleValue::Scalar(b)) => {
            interpolate_scalar(a, b, t).map(StyleValue::Scalar)
        }
        (StyleValue::Spacing(a), StyleValue::Spacing(b)) => {
            Some(StyleValue::Spacing(interpolate_spacing(a, b, t)))
        }
        (StyleValue::Tint(a), StyleValue::Tint(b)) => {
            Some(StyleValue::Tint(interpolate_tint(a, b, t)))
        }
        _ => None,
    }
}

/// Check if a CSS property name is animatable.
pub fn is_animatable_property(property: &str) -> bool {
    matches!(
        property,
        "fg" | "bg"
            | "opacity"
            | "text_opacity"
            | "width"
            | "height"
            | "min_width"
            | "max_width"
            | "min_height"
            | "max_height"
            | "margin"
            | "padding"
            | "tint"
            | "background_tint"
    )
}

/// Interpolate a single CSS property between two `Style` objects.
///
/// Returns a `Style` with only the interpolated property set, or `None`
/// if the property is non-animatable or the values are missing/incompatible.
pub fn interpolate_style_property(
    property: &str,
    from: &Style,
    to: &Style,
    t: f32,
) -> Option<Style> {
    let mut result = Style::new();
    match property {
        "fg" => {
            let a = from.fg?;
            let b = to.fg?;
            result.fg = Some(interpolate_color(a, b, t));
        }
        "bg" => {
            let a = from.bg?;
            let b = to.bg?;
            result.bg = Some(interpolate_color(a, b, t));
        }
        "opacity" => {
            let a = from.opacity? as f32;
            let b = to.opacity? as f32;
            result.opacity = Some(interpolate_f32(a, b, t).round().clamp(0.0, 100.0) as u8);
        }
        "text_opacity" => {
            let a = from.text_opacity? as f32;
            let b = to.text_opacity? as f32;
            result.text_opacity = Some(interpolate_f32(a, b, t).round().clamp(0.0, 100.0) as u8);
        }
        "width" => {
            result.width = Some(interpolate_scalar(&from.width?, &to.width?, t)?);
        }
        "height" => {
            result.height = Some(interpolate_scalar(&from.height?, &to.height?, t)?);
        }
        "min_width" => {
            result.min_width = Some(interpolate_scalar(&from.min_width?, &to.min_width?, t)?);
        }
        "max_width" => {
            result.max_width = Some(interpolate_scalar(&from.max_width?, &to.max_width?, t)?);
        }
        "min_height" => {
            result.min_height = Some(interpolate_scalar(&from.min_height?, &to.min_height?, t)?);
        }
        "max_height" => {
            result.max_height = Some(interpolate_scalar(&from.max_height?, &to.max_height?, t)?);
        }
        "margin" => {
            let a = from.margin.as_ref()?;
            let b = to.margin.as_ref()?;
            result.margin = Some(interpolate_spacing(a, b, t));
        }
        "padding" => {
            let a = from.padding.as_ref()?;
            let b = to.padding.as_ref()?;
            result.padding = Some(interpolate_spacing(a, b, t));
        }
        "tint" => {
            let a = from.tint.as_ref()?;
            let b = to.tint.as_ref()?;
            result.tint = Some(interpolate_tint(a, b, t));
        }
        "background_tint" => {
            let a = from.background_tint.as_ref()?;
            let b = to.background_tint.as_ref()?;
            result.background_tint = Some(interpolate_tint(a, b, t));
        }
        _ => return None,
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{AnimationEase, AnimationRequest, StyleAnimationRequest, StyleValue};
    use crate::node_id::node_id_from_ffi;
    use crate::style::{Color, Scalar, Spacing, Tint};

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

    // ── Style interpolation tests ─────────────────────────────────────

    #[test]
    fn interpolate_color_midpoint() {
        let from = Color::rgba(0, 0, 0, 255);
        let to = Color::rgba(100, 200, 50, 128);
        let mid = interpolate_color(from, to, 0.5);
        assert_eq!(mid.r, 50);
        assert_eq!(mid.g, 100);
        assert_eq!(mid.b, 25);
        // lerp(255, 128, 0.5) = 255 + (128 - 255) * 0.5 = 191.5 → 192
        assert_eq!(mid.a, 192);
    }

    #[test]
    fn interpolate_color_at_boundaries() {
        let from = Color::rgb(10, 20, 30);
        let to = Color::rgb(200, 100, 50);
        let at_0 = interpolate_color(from, to, 0.0);
        assert_eq!(at_0, Color::rgba(10, 20, 30, 255));
        let at_1 = interpolate_color(from, to, 1.0);
        assert_eq!(at_1, Color::rgba(200, 100, 50, 255));
    }

    #[test]
    fn interpolate_color_clamps_t() {
        let from = Color::rgb(0, 0, 0);
        let to = Color::rgb(100, 100, 100);
        // t < 0 should clamp to 0
        let v = interpolate_color(from, to, -0.5);
        assert_eq!(v, Color::rgba(0, 0, 0, 255));
        // t > 1 should clamp to 1
        let v = interpolate_color(from, to, 1.5);
        assert_eq!(v, Color::rgba(100, 100, 100, 255));
    }

    #[test]
    fn interpolate_f32_opacity() {
        let v = interpolate_f32(0.0, 100.0, 0.5);
        assert!((v - 50.0).abs() < 0.01);
        assert!((interpolate_f32(0.0, 100.0, 0.0)).abs() < 0.001);
        assert!((interpolate_f32(0.0, 100.0, 1.0) - 100.0).abs() < 0.001);
    }

    #[test]
    fn interpolate_scalar_cells() {
        let from = Scalar::Cells(10);
        let to = Scalar::Cells(30);
        let mid = interpolate_scalar(&from, &to, 0.5).unwrap();
        assert_eq!(mid, Scalar::Cells(20));
    }

    #[test]
    fn interpolate_scalar_cells_boundaries() {
        let from = Scalar::Cells(10);
        let to = Scalar::Cells(30);
        assert_eq!(
            interpolate_scalar(&from, &to, 0.0).unwrap(),
            Scalar::Cells(10)
        );
        assert_eq!(
            interpolate_scalar(&from, &to, 1.0).unwrap(),
            Scalar::Cells(30)
        );
    }

    #[test]
    fn interpolate_scalar_percent() {
        let from = Scalar::Percent(0.0);
        let to = Scalar::Percent(100.0);
        if let Some(Scalar::Percent(v)) = interpolate_scalar(&from, &to, 0.5) {
            assert!((v - 50.0).abs() < 0.01);
        } else {
            panic!("expected Percent variant");
        }
    }

    #[test]
    fn interpolate_scalar_fraction() {
        let from = Scalar::Fraction(1.0);
        let to = Scalar::Fraction(3.0);
        if let Some(Scalar::Fraction(v)) = interpolate_scalar(&from, &to, 0.5) {
            assert!((v - 2.0).abs() < 0.01);
        } else {
            panic!("expected Fraction variant");
        }
    }

    #[test]
    fn interpolate_scalar_view_width() {
        let from = Scalar::ViewWidth(10.0);
        let to = Scalar::ViewWidth(90.0);
        if let Some(Scalar::ViewWidth(v)) = interpolate_scalar(&from, &to, 0.25) {
            assert!((v - 30.0).abs() < 0.01);
        } else {
            panic!("expected ViewWidth variant");
        }
    }

    #[test]
    fn interpolate_scalar_view_height() {
        let from = Scalar::ViewHeight(0.0);
        let to = Scalar::ViewHeight(50.0);
        if let Some(Scalar::ViewHeight(v)) = interpolate_scalar(&from, &to, 0.5) {
            assert!((v - 25.0).abs() < 0.01);
        } else {
            panic!("expected ViewHeight variant");
        }
    }

    #[test]
    fn interpolate_scalar_different_units_returns_none() {
        assert!(interpolate_scalar(&Scalar::Cells(10), &Scalar::Percent(50.0), 0.5).is_none());
        assert!(interpolate_scalar(&Scalar::Auto, &Scalar::Cells(10), 0.5).is_none());
        assert!(interpolate_scalar(&Scalar::Cells(10), &Scalar::Auto, 0.5).is_none());
        assert!(
            interpolate_scalar(&Scalar::ViewWidth(10.0), &Scalar::ViewHeight(10.0), 0.5).is_none()
        );
    }

    #[test]
    fn interpolate_spacing_midpoint() {
        let from = Spacing::new(0, 0, 0, 0);
        let to = Spacing::new(10, 20, 30, 40);
        let mid = interpolate_spacing(&from, &to, 0.5);
        assert_eq!(mid.top, 5);
        assert_eq!(mid.right, 10);
        assert_eq!(mid.bottom, 15);
        assert_eq!(mid.left, 20);
    }

    #[test]
    fn interpolate_spacing_boundaries() {
        let from = Spacing::new(2, 4, 6, 8);
        let to = Spacing::new(10, 20, 30, 40);
        let at_0 = interpolate_spacing(&from, &to, 0.0);
        assert_eq!(at_0, from);
        let at_1 = interpolate_spacing(&from, &to, 1.0);
        assert_eq!(at_1, to);
    }

    #[test]
    fn interpolate_tint_midpoint() {
        let from = Tint::new(Color::rgb(0, 0, 0), 0);
        let to = Tint::new(Color::rgb(100, 100, 100), 50);
        let mid = interpolate_tint(&from, &to, 0.5);
        assert_eq!(mid.color, Color::rgba(50, 50, 50, 255));
        assert_eq!(mid.percent, 25);
    }

    #[test]
    fn interpolate_style_value_color() {
        let from = StyleValue::Color(Color::rgb(0, 0, 0));
        let to = StyleValue::Color(Color::rgb(100, 100, 100));
        let mid = interpolate_style_value(&from, &to, 0.5).unwrap();
        assert_eq!(mid, StyleValue::Color(Color::rgba(50, 50, 50, 255)));
    }

    #[test]
    fn interpolate_style_value_float() {
        let from = StyleValue::Float(0.0);
        let to = StyleValue::Float(100.0);
        if let Some(StyleValue::Float(v)) = interpolate_style_value(&from, &to, 0.75) {
            assert!((v - 75.0).abs() < 0.01);
        } else {
            panic!("expected Float");
        }
    }

    #[test]
    fn interpolate_style_value_scalar() {
        let from = StyleValue::Scalar(Scalar::Cells(0));
        let to = StyleValue::Scalar(Scalar::Cells(100));
        let mid = interpolate_style_value(&from, &to, 0.5).unwrap();
        assert_eq!(mid, StyleValue::Scalar(Scalar::Cells(50)));
    }

    #[test]
    fn interpolate_style_value_spacing() {
        let from = StyleValue::Spacing(Spacing::all(0));
        let to = StyleValue::Spacing(Spacing::all(20));
        if let Some(StyleValue::Spacing(s)) = interpolate_style_value(&from, &to, 0.5) {
            assert_eq!(s, Spacing::all(10));
        } else {
            panic!("expected Spacing");
        }
    }

    #[test]
    fn interpolate_style_value_tint() {
        let from = StyleValue::Tint(Tint::new(Color::rgb(0, 0, 0), 0));
        let to = StyleValue::Tint(Tint::new(Color::rgb(200, 200, 200), 100));
        if let Some(StyleValue::Tint(t)) = interpolate_style_value(&from, &to, 0.5) {
            assert_eq!(t.color, Color::rgba(100, 100, 100, 255));
            assert_eq!(t.percent, 50);
        } else {
            panic!("expected Tint");
        }
    }

    #[test]
    fn interpolate_style_value_mismatched_types_returns_none() {
        let color = StyleValue::Color(Color::rgb(0, 0, 0));
        let float = StyleValue::Float(1.0);
        let scalar = StyleValue::Scalar(Scalar::Cells(10));
        assert!(interpolate_style_value(&color, &float, 0.5).is_none());
        assert!(interpolate_style_value(&float, &scalar, 0.5).is_none());
        assert!(interpolate_style_value(&scalar, &color, 0.5).is_none());
    }

    #[test]
    fn is_animatable_property_positive() {
        for prop in &[
            "fg",
            "bg",
            "opacity",
            "text_opacity",
            "width",
            "height",
            "min_width",
            "max_width",
            "min_height",
            "max_height",
            "margin",
            "padding",
            "tint",
            "background_tint",
        ] {
            assert!(is_animatable_property(prop), "{prop} should be animatable");
        }
    }

    #[test]
    fn is_animatable_property_negative() {
        for prop in &[
            "display",
            "visibility",
            "layout",
            "dock",
            "overflow",
            "bold",
            "italic",
            "underline",
            "border",
            "pointer",
            "layer",
            "layers",
            "nonexistent",
        ] {
            assert!(
                !is_animatable_property(prop),
                "{prop} should NOT be animatable"
            );
        }
    }

    #[test]
    fn interpolate_style_property_bg_at_boundaries() {
        let from = Style::new().bg(Color::rgb(0, 0, 0));
        let to = Style::new().bg(Color::rgb(100, 200, 50));

        let at_0 = interpolate_style_property("bg", &from, &to, 0.0).unwrap();
        assert_eq!(at_0.bg, Some(Color::rgba(0, 0, 0, 255)));

        let at_05 = interpolate_style_property("bg", &from, &to, 0.5).unwrap();
        assert_eq!(at_05.bg, Some(Color::rgba(50, 100, 25, 255)));

        let at_1 = interpolate_style_property("bg", &from, &to, 1.0).unwrap();
        assert_eq!(at_1.bg, Some(Color::rgba(100, 200, 50, 255)));
    }

    #[test]
    fn interpolate_style_property_fg() {
        let from = Style::new().fg(Color::rgb(255, 0, 0));
        let to = Style::new().fg(Color::rgb(0, 255, 0));
        let mid = interpolate_style_property("fg", &from, &to, 0.5).unwrap();
        assert_eq!(mid.fg, Some(Color::rgba(128, 128, 0, 255)));
    }

    #[test]
    fn interpolate_style_property_opacity() {
        let from = Style::new().opacity(0);
        let to = Style::new().opacity(100);
        let mid = interpolate_style_property("opacity", &from, &to, 0.5).unwrap();
        assert_eq!(mid.opacity, Some(50));
    }

    #[test]
    fn interpolate_style_property_width_cells() {
        let from = Style::new().width(Scalar::Cells(10));
        let to = Style::new().width(Scalar::Cells(50));
        let mid = interpolate_style_property("width", &from, &to, 0.5).unwrap();
        assert_eq!(mid.width, Some(Scalar::Cells(30)));
    }

    #[test]
    fn interpolate_style_property_margin() {
        let mut from = Style::new();
        from.margin = Some(Spacing::all(0));
        let mut to = Style::new();
        to.margin = Some(Spacing::new(10, 20, 30, 40));
        let mid = interpolate_style_property("margin", &from, &to, 0.5).unwrap();
        assert_eq!(mid.margin, Some(Spacing::new(5, 10, 15, 20)));
    }

    #[test]
    fn interpolate_style_property_non_animatable_returns_none() {
        let from = Style::new();
        let to = Style::new();
        assert!(interpolate_style_property("display", &from, &to, 0.5).is_none());
        assert!(interpolate_style_property("layout", &from, &to, 0.5).is_none());
        assert!(interpolate_style_property("dock", &from, &to, 0.5).is_none());
        assert!(interpolate_style_property("bold", &from, &to, 0.5).is_none());
    }

    #[test]
    fn interpolate_style_property_missing_from_returns_none() {
        let from = Style::new(); // no bg set
        let to = Style::new().bg(Color::rgb(255, 0, 0));
        assert!(interpolate_style_property("bg", &from, &to, 0.5).is_none());
    }

    #[test]
    fn interpolate_style_property_missing_to_returns_none() {
        let from = Style::new().bg(Color::rgb(0, 0, 0));
        let to = Style::new(); // no bg set
        assert!(interpolate_style_property("bg", &from, &to, 0.5).is_none());
    }

    // ── Animator style animation integration tests ────────────────────

    #[test]
    fn animator_style_animation_basic() {
        let mut animator = Animator::new(60);
        let widget = node_id_from_ffi(10);
        let now = Instant::now();

        animator.enqueue_style(
            StyleAnimationRequest::new(
                widget,
                "bg",
                StyleValue::Color(Color::rgb(0, 0, 0)),
                StyleValue::Color(Color::rgb(100, 100, 100)),
                Duration::from_millis(300),
            ),
            now,
        );

        assert!(animator.has_animations());

        // Midpoint
        let updates = animator.step_style(now + Duration::from_millis(150), AnimationLevel::Full);
        assert!(!updates.is_empty());
        assert_eq!(updates[0].target, widget);
        assert_eq!(updates[0].property, "bg");
        assert!(!updates[0].done);

        // End
        let updates = animator.step_style(now + Duration::from_millis(300), AnimationLevel::Full);
        assert!(!updates.is_empty());
        assert!(updates[0].done);
        assert_eq!(
            updates[0].value,
            StyleValue::Color(Color::rgba(100, 100, 100, 255))
        );

        // Should be removed
        assert!(!animator.has_animations());
    }

    #[test]
    fn animator_style_animation_respects_none_level() {
        let mut animator = Animator::new(60);
        let widget = node_id_from_ffi(11);
        let now = Instant::now();

        animator.enqueue_style(
            StyleAnimationRequest::new(
                widget,
                "opacity",
                StyleValue::Float(0.0),
                StyleValue::Float(100.0),
                Duration::from_millis(500),
            ),
            now,
        );

        // AnimationLevel::None should skip to end immediately
        let updates = animator.step_style(now, AnimationLevel::None);
        assert_eq!(updates.len(), 1);
        assert!(updates[0].done);
        assert_eq!(updates[0].value, StyleValue::Float(100.0));
        assert!(!animator.has_animations());
    }

    #[test]
    fn animator_style_animation_with_delay() {
        let mut animator = Animator::new(60);
        let widget = node_id_from_ffi(12);
        let now = Instant::now();

        animator.enqueue_style(
            StyleAnimationRequest::new(
                widget,
                "bg",
                StyleValue::Color(Color::rgb(0, 0, 0)),
                StyleValue::Color(Color::rgb(200, 200, 200)),
                Duration::from_millis(200),
            )
            .with_delay(Duration::from_millis(100)),
            now,
        );

        // Before delay: no updates
        let updates = animator.step_style(now + Duration::from_millis(50), AnimationLevel::Full);
        assert!(updates.is_empty());
        assert!(animator.has_animations());

        // After delay, midway: should have update
        let updates = animator.step_style(now + Duration::from_millis(200), AnimationLevel::Full);
        assert!(!updates.is_empty());
        assert!(!updates[0].done);
    }

    #[test]
    fn animator_style_replaces_same_key() {
        let mut animator = Animator::new(60);
        let widget = node_id_from_ffi(13);
        let now = Instant::now();

        // Enqueue first animation
        animator.enqueue_style(
            StyleAnimationRequest::new(
                widget,
                "bg",
                StyleValue::Color(Color::rgb(0, 0, 0)),
                StyleValue::Color(Color::rgb(100, 100, 100)),
                Duration::from_millis(300),
            ),
            now,
        );

        // Enqueue second with same (node, property) — should replace
        animator.enqueue_style(
            StyleAnimationRequest::new(
                widget,
                "bg",
                StyleValue::Color(Color::rgb(50, 50, 50)),
                StyleValue::Color(Color::rgb(200, 200, 200)),
                Duration::from_millis(500),
            ),
            now,
        );

        // Complete the animation
        let updates = animator.step_style(now + Duration::from_millis(500), AnimationLevel::Full);
        assert_eq!(updates.len(), 1);
        assert!(updates[0].done);
        // Should reach the second animation's target
        assert_eq!(
            updates[0].value,
            StyleValue::Color(Color::rgba(200, 200, 200, 255))
        );
    }

    #[test]
    fn animator_enqueue_style_many() {
        let mut animator = Animator::new(60);
        let widget = node_id_from_ffi(14);
        let now = Instant::now();

        animator.enqueue_style_many(
            vec![
                StyleAnimationRequest::new(
                    widget,
                    "bg",
                    StyleValue::Color(Color::rgb(0, 0, 0)),
                    StyleValue::Color(Color::rgb(255, 255, 255)),
                    Duration::from_millis(100),
                ),
                StyleAnimationRequest::new(
                    widget,
                    "opacity",
                    StyleValue::Float(0.0),
                    StyleValue::Float(100.0),
                    Duration::from_millis(100),
                ),
            ],
            now,
        );

        assert!(animator.has_animations());
        let updates = animator.step_style(now + Duration::from_millis(100), AnimationLevel::Full);
        assert_eq!(updates.len(), 2);
        assert!(updates.iter().all(|u| u.done));
    }

    #[test]
    fn animator_next_timeout_includes_style_animations() {
        let mut animator = Animator::new(60);
        let widget = node_id_from_ffi(15);
        let now = Instant::now();

        // No animations: None
        assert!(animator.next_timeout(now).is_none());

        animator.enqueue_style(
            StyleAnimationRequest::new(
                widget,
                "bg",
                StyleValue::Color(Color::rgb(0, 0, 0)),
                StyleValue::Color(Color::rgb(255, 255, 255)),
                Duration::from_millis(300),
            ),
            now,
        );

        // Should return Some
        assert!(animator.next_timeout(now).is_some());
    }
}
