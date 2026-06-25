//! In-process headless test harness (`App::run_test` + [`Pilot`]).
//!
//! This is the Rust analogue of Python Textual's `Pilot` (see
//! `textual/src/textual/pilot.py`) and headless driver. It runs the real app
//! event-dispatch engine in-process, fed from injected input events instead of
//! a terminal, and rendering into the in-memory [`FrameBuffer`] instead of a
//! TTY (see [`App::headless`] seam in `runtime/mod.rs`).
//!
//! Each driver call (`press`, `click`, `pause`, …) injects the event(s) and
//! advances the loop until idle (no pending invalidation, no active animations,
//! no elapsed timers), so the test body can read app/widget state and rendered
//! output between calls — mirroring `await pilot.press(...)`.
//!
//! ```no_run
//! use textual::prelude::*;
//!
//! struct MyApp;
//! impl TextualApp for MyApp {
//!     fn compose(&mut self) -> AppRoot { AppRoot::new() }
//! }
//!
//! MyApp.run_test(|pilot| {
//!     pilot.press(&["tab"])?;
//!     assert!(pilot.app().query_one("Button").is_ok());
//!     Ok(())
//! }).unwrap();
//! ```

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::Result;
use crate::runtime::App;
use crate::widgets::Widget;

/// Drives a headless app in tests. Mirrors Python Textual's `Pilot`.
///
/// Borrows the running [`App`] and its root widget; created and passed to the
/// closure given to [`App::run_test`] / the `TextualApp::run_test` extension.
pub struct Pilot<'a> {
    app: &'a mut App,
    root: &'a mut dyn Widget,
}

impl<'a> Pilot<'a> {
    pub(crate) fn new(app: &'a mut App, root: &'a mut dyn Widget) -> Self {
        // Install a deterministic manual clock for the duration of the test so
        // time-driven behavior is reproducible and explicitly driven by
        // `advance_clock`, rather than racing the wall clock. Timers scheduled
        // during `headless_startup` (e.g. from `on_mount`) are preserved
        // (re-anchored) by the in-place switch.
        app.enable_manual_timer_clock();
        Self { app, root }
    }

    /// Immutable access to the running app, for assertions (`query_one`, state).
    pub fn app(&self) -> &App {
        self.app
    }

    /// Mutable access to the running app (advanced cases).
    pub fn app_mut(&mut self) -> &mut App {
        self.app
    }

    /// Simulate key-presses, then advance to idle.
    ///
    /// Each key is a Textual key name: a single character (`"r"`), a named key
    /// (`"enter"`, `"tab"`, `"escape"`, `"up"`, `"f5"`), or a modified key
    /// (`"ctrl+a"`, `"shift+tab"`). Mirrors `pilot.press(*keys)`.
    pub fn press(&mut self, keys: &[&str]) -> Result<()> {
        for key in keys {
            let event = parse_key(key)
                .ok_or_else(|| crate::Error::Message(format!("unknown key spec: {key}")))?;
            self.app.headless_inject_key(self.root, event)?;
        }
        Ok(())
    }

    /// Convenience: press a single key.
    pub fn press_key(&mut self, key: &str) -> Result<()> {
        self.press(&[key])
    }

    /// Simulate a left-click on the widget matched by `selector`, at the centre
    /// of its rendered region. Mirrors `pilot.click(selector)`.
    pub fn click(&mut self, selector: &str) -> Result<()> {
        let node = self
            .app
            .query_one(selector)
            .map_err(|e| crate::Error::Message(format!("click selector {selector}: {e:?}")))?;
        let rect = self
            .app
            .node_screen_rect(node)
            .ok_or_else(|| crate::Error::Message(format!("no rendered region for {selector}")))?;
        let cx = rect.0 + (rect.2.saturating_sub(rect.0)) / 2;
        let cy = rect.1 + (rect.3.saturating_sub(rect.1)) / 2;
        self.app.headless_inject_click(self.root, cx, cy)
    }

    /// Click at an absolute screen coordinate.
    pub fn click_at(&mut self, x: u16, y: u16) -> Result<()> {
        self.app.headless_inject_click(self.root, x, y)
    }

    /// Advance the app to idle (process queued messages/timers/animations and
    /// render). Mirrors `pilot.pause()`.
    pub fn pause(&mut self) -> Result<()> {
        self.app.headless_pause(self.root)
    }

    /// Alias for [`Pilot::pause`] — wait until the app is idle.
    pub fn wait_for_idle(&mut self) -> Result<()> {
        self.pause()
    }

    /// Advance the deterministic test clock by `delta`, firing every timer whose
    /// deadline elapses along the way, pumping the headless loop to idle and
    /// re-rendering after each fire.
    ///
    /// This is the deterministic analogue of Python's `await pilot.pause(delay)`
    /// (which sleeps real time so wall-clock timers fire). Inside `run_test` the
    /// timer subsystem runs on a manual clock (installed in [`Pilot::new`]), so
    /// time-driven demos — clocks, stopwatches, progress timers — become fully
    /// deterministic with no sleeping and no flakiness.
    ///
    /// Crucially, the clock is advanced **deadline-by-deadline**, exactly as the
    /// real event loop wakes once per timer timeout: a `advance_clock(3s)` over a
    /// 1s interval fires three discrete ticks (1s, 2s, 3s), not a single
    /// backlog-collapsed fire. The remaining sub-deadline time is then consumed
    /// so the clock ends exactly `delta` ahead.
    pub fn advance_clock(&mut self, delta: Duration) -> Result<()> {
        let mut remaining = delta;
        // Bound iterations defensively (a fast interval over a long delta still
        // terminates; this only guards against a pathological zero-interval).
        const MAX_STEPS: usize = 1_000_000;
        // Walk to each timer deadline that falls within `remaining`, advancing
        // and pumping (which drains ready timers, runs app-level timer
        // callbacks, processes messages/recompositions, and re-renders — exactly
        // the housekeeping a live frame performs at each wake).
        for _ in 0..MAX_STEPS {
            if remaining.is_zero() {
                break;
            }
            match self.app.next_timer_timeout() {
                // Advance at least 1ns past a same-instant deadline so a
                // repeating timer cannot wedge the loop on a zero timeout.
                Some(step) if step <= remaining => {
                    let advanced = step.max(Duration::from_nanos(1)).min(remaining);
                    self.app.advance_timers(advanced);
                    remaining -= advanced;
                    self.app.headless_pause(self.root)?;
                }
                _ => break,
            }
        }
        // Consume any sub-deadline remainder so the clock ends exactly `delta`
        // ahead, then pump once more to settle.
        if !remaining.is_zero() {
            self.app.advance_timers(remaining);
            self.app.headless_pause(self.root)?;
        }
        Ok(())
    }

    /// True while the harness is running on the deterministic manual clock
    /// (always the case inside `run_test`). Lets tests assert the foundation is
    /// active before relying on [`Pilot::advance_clock`] determinism.
    pub fn clock_is_manual(&self) -> bool {
        self.app.timer_clock_is_manual()
    }

    /// Resize the virtual terminal and advance to idle.
    pub fn resize(&mut self, width: u16, height: u16) -> Result<()> {
        self.app.headless_resize(self.root, width, height)
    }
}

/// Parse a Textual key name (e.g. `"r"`, `"enter"`, `"ctrl+a"`, `"shift+tab"`)
/// into a crossterm [`KeyEvent`].
///
/// Returns `None` for unrecognised specs.
pub fn parse_key(spec: &str) -> Option<KeyEvent> {
    let mut modifiers = KeyModifiers::NONE;
    let parts: Vec<&str> = spec.split('+').collect();
    let (mod_parts, key_part) = parts.split_at(parts.len() - 1);
    for m in mod_parts {
        match m.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= KeyModifiers::CONTROL,
            "shift" => modifiers |= KeyModifiers::SHIFT,
            "alt" | "meta" | "option" => modifiers |= KeyModifiers::ALT,
            "super" | "cmd" | "command" => modifiers |= KeyModifiers::SUPER,
            _ => return None,
        }
    }
    let key = key_part[0];
    let code = match key.to_ascii_lowercase().as_str() {
        "enter" | "return" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "backtab" => KeyCode::BackTab,
        "escape" | "esc" => KeyCode::Esc,
        "space" => KeyCode::Char(' '),
        "backspace" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" | "page_up" => KeyCode::PageUp,
        "pagedown" | "page_down" => KeyCode::PageDown,
        other => {
            if let Some(stripped) = other.strip_prefix('f') {
                if let Ok(n) = stripped.parse::<u8>() {
                    KeyCode::F(n)
                } else {
                    return single_char(key);
                }
            } else {
                return single_char_with_mods(key, modifiers);
            }
        }
    };
    Some(KeyEvent::new(code, modifiers))
}

fn single_char(key: &str) -> Option<KeyEvent> {
    single_char_with_mods(key, KeyModifiers::NONE)
}

fn single_char_with_mods(key: &str, modifiers: KeyModifiers) -> Option<KeyEvent> {
    let mut chars = key.chars();
    let ch = chars.next()?;
    if chars.next().is_some() {
        return None; // multi-char unknown name
    }
    Some(KeyEvent::new(KeyCode::Char(ch), modifiers))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventCtx;
    use crate::message::ButtonPressed;
    use crate::style::{Color, parse_color_like};
    use crate::widgets::{AppRoot, BindingDecl, Button, Horizontal};
    use crate::{App, TextualApp};

    const CSS: &str = r#"
Screen { align: center middle; }
Horizontal { width: auto; height: auto; }
"#;

    /// Port of Python `docs/examples/guide/testing/rgb.py` + `test_rgb.py`,
    /// driven through the real Pilot harness.
    struct RgbApp;

    impl TextualApp for RgbApp {
        fn bindings(&self) -> Vec<BindingDecl> {
            vec![
                BindingDecl::new("r", "switch_color('red')", "Go Red"),
                BindingDecl::new("g", "switch_color('green')", "Go Green"),
                BindingDecl::new("b", "switch_color('blue')", "Go Blue"),
            ]
        }

        fn compose(&mut self) -> AppRoot {
            AppRoot::new().with_child(Horizontal::new().with_compose(crate::compose![
                Button::new("Red").id("red"),
                Button::new("Green").id("green"),
                Button::new("Blue").id("blue"),
            ]))
        }

        fn configure(&mut self, app: &mut App) -> crate::Result<()> {
            app.load_stylesheet(CSS);
            Ok(())
        }

        fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut EventCtx) {
            if let Some(parsed) = crate::action::parse_action(action) {
                if parsed.name == "switch_color" {
                    if let Some(name) = parsed.arguments.first() {
                        if let Some(color) = parse_color_like(name) {
                            let _ = app
                                .query_mut("Screen")
                                .map(|q| q.set_styles(|s| s.set_bg(color)));
                            ctx.set_handled();
                            ctx.request_repaint();
                        }
                    }
                }
            }
        }

        fn on_message_with_app(
            &mut self,
            app: &mut App,
            message: &crate::message::MessageEvent,
            ctx: &mut EventCtx,
        ) {
            if let Some(bp) = message.downcast_ref::<ButtonPressed>() {
                if let Some(name) = &bp.button_id {
                    if let Some(color) = parse_color_like(name) {
                        let _ = app
                            .query_mut("Screen")
                            .map(|q| q.set_styles(|s| s.set_bg(color)));
                        ctx.set_handled();
                        ctx.request_repaint();
                    }
                }
            }
        }
    }

    /// Read the explicit screen background, mirroring Python's
    /// `app.screen.styles.background`. `AppRoot::style_type()` is `"Screen"`.
    fn screen_bg(app: &App) -> Option<Color> {
        let node = app.query_one("Screen").ok()?;
        app.node_explicit_bg(node)
    }

    #[test]
    fn pilot_press_changes_rendered_state() {
        crate::run_test(RgbApp, |pilot| {
            let initial = screen_bg(pilot.app());

            pilot.press(&["r"])?;
            let red = screen_bg(pilot.app());
            assert_eq!(
                red,
                parse_color_like("red"),
                "pressing 'r' must set the screen background to red"
            );
            assert_ne!(red, initial, "pressing 'r' must change the rendered state");

            pilot.press(&["g"])?;
            assert_eq!(
                screen_bg(pilot.app()),
                parse_color_like("green"),
                "pressing 'g' must set the screen background to green"
            );

            pilot.press(&["b"])?;
            assert_eq!(
                screen_bg(pilot.app()),
                parse_color_like("blue"),
                "pressing 'b' must set the screen background to blue"
            );

            // Unmapped key must not change anything.
            let before_x = screen_bg(pilot.app());
            pilot.press(&["x"])?;
            assert_eq!(
                screen_bg(pilot.app()),
                before_x,
                "pressing an unmapped key must not change state"
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn pilot_click_changes_rendered_state() {
        crate::run_test(RgbApp, |pilot| {
            pilot.click("#red")?;
            assert_eq!(
                screen_bg(pilot.app()),
                parse_color_like("red"),
                "clicking #red must set the screen background to red"
            );

            pilot.click("#green")?;
            assert_eq!(
                screen_bg(pilot.app()),
                parse_color_like("green"),
                "clicking #green must set the screen background to green"
            );

            pilot.click("#blue")?;
            assert_eq!(
                screen_bg(pilot.app()),
                parse_color_like("blue"),
                "clicking #blue must set the screen background to blue"
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn pilot_press_changes_rendered_output() {
        // Tab cycles focus between the three buttons; the focused button renders
        // a distinct (focused) appearance, so the rendered frame must change.
        crate::run_test(RgbApp, |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.press(&["tab"])?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "pressing Tab must change the rendered frame (focus moved)"
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn parse_key_handles_names_and_modifiers() {
        assert!(parse_key("r").is_some());
        assert!(parse_key("enter").is_some());
        assert!(parse_key("ctrl+a").is_some());
        assert!(parse_key("shift+tab").is_some());
        assert!(parse_key("f5").is_some());
        assert!(parse_key("boguskey").is_none());
        assert!(parse_key("ctrl+r").is_some());
    }
}
