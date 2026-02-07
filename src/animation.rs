use crate::event::{AnimationEase, AnimationLevel, AnimationRequest};
use crate::widgets::WidgetId;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
struct NumericAnimation {
    target: WidgetId,
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
    pub target: WidgetId,
    pub attribute: String,
    pub value: f32,
    pub done: bool,
}

#[derive(Debug)]
pub struct Animator {
    animations: HashMap<(WidgetId, String), NumericAnimation>,
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

    pub fn is_being_animated(&self, target: WidgetId, attribute: &str) -> bool {
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
        let keys: Vec<(WidgetId, String)> = self.animations.keys().cloned().collect();
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

fn apply_easing(ease: AnimationEase, x: f32) -> f32 {
    match ease {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{AnimationEase, AnimationRequest};

    #[test]
    fn animator_interpolates_in_out_cubic() {
        let mut animator = Animator::new(60);
        let widget = WidgetId::new();
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
        let widget = WidgetId::new();
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
}
