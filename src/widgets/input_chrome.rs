use std::time::{Duration, Instant};

use crate::style::{Color, parse_color_like};

/// The input widget's painted surface background: the live composited
/// background during a tree render (state-aware, including the `:focus`
/// `background-tint: $foreground 5%`), or a stateless off-tree fallback with
/// any `background-tint` applied by hand. Mirrors Python `dom.py`:
/// `background += styles.background.tint(styles.background_tint)`.
///
/// Shared by `Input` and `MaskedInput` so their `input--*` component colours
/// (which may carry alpha or an `auto <pct>%` contrast token) composite over
/// the same, correct surface.
pub(super) fn composited_surface_bg<W: crate::widgets::Widget + ?Sized>(widget: &W) -> Color {
    let fallback_bg = parse_color_like("$background").unwrap_or(Color::rgb(0, 0, 0));
    crate::css::current_composited_background().unwrap_or_else(|| {
        // Off-tree callers (unit tests without a live style stack): resolve
        // statelessly and apply any `background-tint` by hand.
        let base_meta = crate::css::selector_meta_generic(widget);
        let base_style = crate::css::resolve_style(widget, &base_meta);
        let mut bg = match base_style.bg {
            Some(bg) if bg.a <= 0.0 => fallback_bg,
            Some(bg) => bg,
            None => fallback_bg,
        };
        if let Some(tint) = base_style.background_tint {
            bg = crate::renderables::Tint::<()>::blend_color_with_percent(
                bg,
                tint.color,
                tint.percent,
            );
        }
        bg
    })
}

/// Resolve an `input--*` component class (`input--cursor` / `input--selection`
/// / `input--placeholder` / `input--suggestion`) as a ready-to-paint
/// `rich_rs::Style` composited over `base_bg` (see [`composited_surface_bg`]).
///
/// `widget_own_bg` is the widget's OWN (untinted) background from the live
/// style stack: the base `Input { background: $surface }` rule also matches
/// the component selector meta, so a component background EQUAL to the widget
/// surface is that leaked base rule, not a genuine override — the segment is
/// left transparent so the compositor paints (and tints) it against the real
/// surface. An `auto <pct>%` foreground (e.g. `$text-disabled` = `auto 38%`)
/// resolves the contrast colour of the under-background at fractional alpha,
/// matching Python's `background.get_contrast_text(alpha)`.
pub(super) fn resolve_input_component_rich<W: crate::widgets::Widget + ?Sized>(
    widget: &W,
    class: &str,
    base_bg: Color,
    widget_own_bg: Option<Color>,
) -> rich_rs::Style {
    let style = crate::css::resolve_component_style(widget, &[class]);
    let mut rich = style.to_rich_without_colors().unwrap_or_default();
    let mut under_bg = base_bg;

    if let Some(bg) = style.bg {
        if bg.a <= 0.0 {
            return rich;
        }
        if Some(bg) != widget_own_bg {
            let flat = bg.flatten_over(under_bg);
            under_bg = flat;
            rich = rich.with_bgcolor(flat.to_simple_opaque());
        }
    }
    if let Some(fg) = style.fg {
        let flat = fg.flatten_over(under_bg);
        rich = rich.with_color(flat.to_simple_opaque());
    } else if let Some(auto) = style.fg_auto {
        let contrast = crate::style::contrast_text(under_bg);
        let flat = contrast.blend_over_float(under_bg, auto.alpha());
        rich = rich.with_color(flat.to_simple_opaque());
    }
    rich
}

#[derive(Debug, Clone)]
pub(super) struct InputChrome {
    focused: bool,
    mouse_down: bool,
    cursor_visible: bool,
    cursor_blink_next_at: Option<Instant>,
    app_active: bool,
}

impl InputChrome {
    const CURSOR_BLINK_PERIOD: Duration = Duration::from_millis(500);

    pub(super) fn new() -> Self {
        Self {
            focused: false,
            mouse_down: false,
            cursor_visible: false,
            cursor_blink_next_at: None,
            app_active: true,
        }
    }

    fn next_blink_deadline() -> Instant {
        let now = Instant::now();
        now.checked_add(Self::CURSOR_BLINK_PERIOD).unwrap_or(now)
    }

    pub(super) fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
        if !focused {
            self.mouse_down = false;
            self.cursor_visible = false;
            self.cursor_blink_next_at = None;
            return;
        }
        self.reset_blink();
    }

    pub(super) fn set_mouse_down(&mut self, down: bool) {
        self.mouse_down = down;
    }

    pub(super) fn is_mouse_down(&self) -> bool {
        self.mouse_down
    }

    pub(super) fn is_active(&self) -> bool {
        self.mouse_down
    }

    pub(super) fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    pub(super) fn reset_blink(&mut self) {
        if !self.focused || !self.app_active {
            return;
        }
        self.cursor_visible = true;
        self.cursor_blink_next_at = Some(Self::next_blink_deadline());
    }

    pub(super) fn handle_app_focus(&mut self, active: bool) {
        self.app_active = active;
        if !active {
            self.cursor_visible = false;
            self.cursor_blink_next_at = None;
            return;
        }
        self.reset_blink();
    }

    pub(super) fn handle_tick(&mut self, now: Instant) -> bool {
        if !self.focused || !self.app_active {
            return false;
        }
        let Some(next_at) = self.cursor_blink_next_at else {
            return false;
        };
        if now < next_at {
            return false;
        }
        self.cursor_visible = !self.cursor_visible;
        self.cursor_blink_next_at = now.checked_add(Self::CURSOR_BLINK_PERIOD).or(Some(now));
        true
    }
}
