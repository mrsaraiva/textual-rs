//! Pomodoro multi-timer board — the "AFTER" cold-user app.
//!
//! This is the rewrite of the app documented in
//! `docs/devel/COLDAPP_POMODORO_FRICTION.md` (the "before": ~250 lines of
//! non-test code, five ranked friction points, an app-global tick loop, a
//! `CardCommand` message type, and hand-built `ReactiveCtx` /
//! `enqueue_runtime_reactive_entry` plumbing). It is authored against ONLY
//! `std` + `textual::prelude::*` (+ `rich-rs`, which the `Widget::render`
//! signature forces on any custom widget) — no runtime/reactive/css internals.
//!
//! What the accumulated 1.0 fundamentals closed, friction-point by point:
//!
//! * **#2 delegation** — `PomodoroCard` is `#[widget(base = VerticalGroup)]`;
//!   the ~15-method hand-forwarded `impl Widget` is gone.
//! * **#3 widget-owned timer** — each card owns its countdown via
//!   `WidgetCtx::set_interval` (deterministic under `Pilot`, purged on unmount).
//!   The app-global tick loop, the `PomodoroCard` type-query, and the
//!   `NEXT_CARD`/`card_id` identity plumbing are all deleted.
//! * **#1 scoped DOM** — a card refreshes its own `Digits` child with
//!   `ctx.query_one::<Digits>().update_via(...)`, rooted at self, by type — no
//!   ids, no `with_widget_mut_as`, no downcast.
//! * **#5 no runtime leaks** — reactive state is mutated with the derive's
//!   `set_*(value, ctx)`; class flips with `ctx.add_class`/`remove_class`. None
//!   of `enqueue_runtime_reactive_entry` / `RuntimeReactiveEntry` /
//!   `ReactiveCtx::new` appear.
//! * **the card owns its own completion** — `PomodoroFinished` is posted BY the
//!   card that hit zero (`ctx.post_message`), not synthesized by the app.
//!
//! The ONE residual gap (honest gate-III finding): a reactive `watch` handler is
//! still handed a DOM-blind `&mut ReactiveCtx`, so the Python idiom of
//! refreshing a child from `watch_remaining` is not available. Here the refresh
//! rides the `WidgetCtx` handlers that CAN reach the subtree (the countdown tick
//! and the buttons). See `sync_digits`. A 1.x "watch handlers get a WidgetCtx"
//! change would let the refresh move into the watch and remove the two explicit
//! `sync_digits` calls.

use std::time::Duration;

use textual::prelude::*;

/// One pomodoro = 25:00.
const POMODORO_SECS: f64 = 1500.0;
/// The countdown ticks 4×/second.
const TICK: Duration = Duration::from_millis(250);

fn format_mmss(secs: f64) -> String {
    let s = secs.max(0.0).round() as u64;
    format!("{:02}:{:02}", s / 60, s % 60)
}

// ===========================================================================
// Message — posted BY a card when its countdown reaches zero
// ===========================================================================

/// Bubbles from the `PomodoroCard` that finished up to the app, which bumps the
/// completed counter. Mirrors Python `self.post_message(self.Finished())` — the
/// message genuinely originates from the widget it is about.
#[derive(Debug, Clone)]
struct PomodoroFinished;
textual::impl_message!(PomodoroFinished);

// ===========================================================================
// PomodoroCard — delegation compound + widget-owned timer + reactive state
// ===========================================================================

#[textual::widget(
    base = VerticalGroup,
    reactive,
    override(on_mount),
    on(on_button),
    style_type = "PomodoroCard"
)]
#[derive(textual::Reactive)]
struct PomodoroCard {
    base: VerticalGroup,
    /// Seconds left on this card's timer.
    #[reactive]
    remaining: f64,
    /// Whether the countdown is currently running.
    #[reactive]
    running: bool,
    /// The card's own repeating countdown timer (auto-stopped on unmount).
    timer: Option<TimerHandle>,
}

impl PomodoroCard {
    fn new(title: &str) -> Self {
        let base = VerticalGroup::new().with_compose(vec![
            ChildDecl::new(Box::new(Label::new(title))).with_classes(&["card-title"]),
            ChildDecl::from(Digits::new(format_mmss(POMODORO_SECS))),
            // Controls stack vertically: three buttons do not fit across a
            // one-third-width card at 80 columns (the rightmost would clip and
            // become unclickable), so each control gets its own full-width row.
            ChildDecl::from(VerticalGroup::new().with_compose(vec![
                ChildDecl::from(Button::success("Start").id("start")),
                ChildDecl::from(Button::new("Pause").id("pause")),
                ChildDecl::from(Button::error("Reset").id("reset")),
            ])),
        ]);
        Self { base, remaining: POMODORO_SECS, running: false, timer: None }
    }

    /// Register the card's own countdown timer. Replaces the app-global tick loop
    /// entirely — the card owns its clock and addresses only itself.
    fn on_mount(&mut self, ctx: &mut WidgetCtx) {
        self.timer = Some(ctx.set_interval(TICK, false, |card: &mut Self, c, _tick| {
            card.tick(c);
        }));
    }

    /// One countdown step. Runs with a `WidgetCtx` (from the widget-owned timer),
    /// so it can both mutate its own reactive state AND reach its `Digits` child.
    fn tick(&mut self, ctx: &mut WidgetCtx) {
        if !self.running {
            return;
        }
        let next = (self.remaining - 0.25).max(0.0);
        self.set_remaining(next, ctx);
        self.sync_digits(ctx);
        if next == 0.0 {
            self.set_running(false, ctx);
            ctx.remove_class("running");
            // The finished message really comes from THIS card.
            ctx.post_message(PomodoroFinished);
        }
    }

    /// Refresh the owned `Digits` child from `remaining`.
    ///
    /// In Python this lives in `watch_remaining`; here a reactive watch is handed
    /// only a DOM-blind `ReactiveCtx`, so the refresh rides the `WidgetCtx`
    /// handlers (the tick above and the buttons below) that CAN reach the
    /// subtree. `query_one::<Digits>()` is rooted at this card and matches by
    /// type — no id plumbing.
    fn sync_digits(&self, ctx: &mut WidgetCtx) {
        let text = format_mmss(self.remaining);
        ctx.query_one::<Digits>().update_via(ctx, move |d, _| d.update(text));
    }

    /// The card handles its own Start/Pause/Reset buttons via bubbling — no app
    /// round-trip, no `CardCommand`, no per-card id addressing. `button_id` only
    /// distinguishes the three buttons WITHIN this card.
    #[textual::on(ButtonPressed)]
    fn on_button(&mut self, event: &ButtonPressed, ctx: &mut WidgetCtx) {
        match event.button_id.as_deref() {
            Some("start") => {
                self.set_running(true, ctx);
                ctx.add_class("running");
            }
            Some("pause") => {
                self.set_running(false, ctx);
                ctx.remove_class("running");
            }
            Some("reset") => {
                self.set_running(false, ctx);
                self.set_remaining(POMODORO_SECS, ctx);
                ctx.remove_class("running");
                self.sync_digits(ctx);
            }
            _ => {}
        }
    }
}

// ===========================================================================
// PomodoroApp — owns the completed counter + dark-mode toggle
// ===========================================================================

const CSS: &str = r#"
Screen { layout: vertical; background: $surface; }

#done {
    height: 1;
    background: $primary;
    color: $text;
    text-style: bold;
    padding: 0 1;
}

#board { layout: horizontal; height: 1fr; }

PomodoroCard {
    width: 1fr;
    height: 100%;
    border: round $panel;
    margin: 0 1;
    padding: 1 1;
    align: center middle;
}
PomodoroCard.running { border: round $success; }

.card-title { text-style: bold; text-align: center; width: 100%; color: $accent; }
Digits { width: auto; color: $text; }
PomodoroCard.running Digits { color: $success; }

PomodoroCard VerticalGroup { height: auto; width: auto; }
PomodoroCard Button { margin: 1 0 0 0; }
"#;

#[derive(textual::Reactive)]
struct PomodoroApp {
    /// Tracks dark/light for the `d` toggle.
    dark: bool,
    /// Completed-pomodoro counter; the watcher repaints the top-bar label.
    #[reactive(watch_with_app, init = false)]
    completed: u32,
}

impl PomodoroApp {
    fn new() -> Self {
        Self { dark: true, completed: 0 }
    }

    /// Reactive watch (`#[reactive(watch_with_app)] completed`): repaint the
    /// top-bar "Completed pomodoros" label whenever a card finishes.
    fn watch_completed(&mut self, app: &mut App, _old: &u32, new: &u32, _ctx: &mut ReactiveCtx) {
        let text = format!("Completed pomodoros: {new}");
        let _ = app.with_query_one_mut_as::<Label, _>("#done", |l| l.set_text(text));
    }
}

impl TextualApp for PomodoroApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("d", "dark", "Toggle dark")]
    }

    fn reactive_widget_mut(&mut self) -> Option<&mut dyn ReactiveWidget> {
        Some(self)
    }

    fn compose(&mut self) -> AppRoot {
        let done = self.completed;
        let board = HorizontalGroup::new().with_compose(vec![
            ChildDecl::from(PomodoroCard::new("Focus")),
            ChildDecl::from(PomodoroCard::new("Write")),
            ChildDecl::from(PomodoroCard::new("Review")),
        ]);
        AppRoot::new().with_compose(vec![
            ChildDecl::new(Box::new(Label::new(format!("Completed pomodoros: {done}"))))
                .with_id("done"),
            ChildDecl::new(Box::new(board)).with_id("board"),
            ChildDecl::from(Footer::new()),
        ])
    }

    fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut WidgetCtx) {
        if action == "dark" {
            self.dark = !self.dark;
            app.set_theme_by_name(if self.dark { "textual-dark" } else { "textual-light" });
            ctx.set_handled();
        }
    }

    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut WidgetCtx) {
        if message.downcast_ref::<PomodoroFinished>().is_some() {
            let next = self.completed + 1;
            self.set_completed(next, app.reactive_ctx());
            ctx.set_handled();
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(PomodoroApp::new())
}

// ===========================================================================
// Tests — deterministic manual clock via Pilot
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Read the first `PomodoroCard`'s `remaining` (seconds).
    fn first_remaining(pilot: &mut Pilot) -> f64 {
        pilot
            .app_mut()
            .with_query_one_mut_as::<PomodoroCard, _>("PomodoroCard", |c| c.remaining)
            .unwrap_or(-1.0)
    }

    fn first_running(pilot: &mut Pilot) -> bool {
        pilot
            .app_mut()
            .with_query_one_mut_as::<PomodoroCard, _>("PomodoroCard", |c| c.running)
            .unwrap_or(false)
    }

    fn completed(pilot: &mut Pilot) -> u32 {
        pilot
            .app_mut()
            .with_app_struct::<PomodoroApp, _>(
                |a, _app, _ctx| a.completed,
                &mut textual::event::EventCtx::default(),
            )
            .unwrap_or(0)
    }

    #[test]
    fn format_mmss_is_zero_padded() {
        assert_eq!(format_mmss(1500.0), "25:00");
        assert_eq!(format_mmss(1496.0), "24:56");
        assert_eq!(format_mmss(0.0), "00:00");
        assert_eq!(format_mmss(-5.0), "00:00");
    }

    #[test]
    fn app_composes_without_panic() {
        let mut app = PomodoroApp::new();
        let _root = app.compose();
    }

    /// A fresh card is stopped at 25:00.
    #[test]
    fn card_starts_stopped_at_full() {
        run_test(PomodoroApp::new(), |pilot| {
            assert!(pilot.clock_is_manual());
            assert_eq!(first_remaining(pilot), POMODORO_SECS);
            assert!(!first_running(pilot));
            Ok(())
        })
        .unwrap();
    }

    /// Starting a card and advancing the clock 4s decrements its own timer
    /// (16 ticks × 0.25s) via the widget-owned interval — 25:00 → 24:56.
    #[test]
    fn start_then_advance_decrements_the_card() {
        run_test(PomodoroApp::new(), |pilot| {
            pilot.click("#start")?;
            assert!(first_running(pilot), "clicking Start must run the card");
            pilot.advance_clock(Duration::from_secs(4))?;
            assert_eq!(
                first_remaining(pilot),
                POMODORO_SECS - 4.0,
                "4s at 4 ticks/sec must decrement the card's own timer to 24:56"
            );
            Ok(())
        })
        .unwrap();
    }

    /// A stopped card does not tick.
    #[test]
    fn stopped_card_does_not_tick() {
        run_test(PomodoroApp::new(), |pilot| {
            pilot.advance_clock(Duration::from_secs(4))?;
            assert_eq!(
                first_remaining(pilot),
                POMODORO_SECS,
                "a card that was never started must stay at 25:00"
            );
            Ok(())
        })
        .unwrap();
    }

    /// Pause halts the countdown mid-run.
    #[test]
    fn pause_halts_the_countdown() {
        run_test(PomodoroApp::new(), |pilot| {
            pilot.click("#start")?;
            pilot.advance_clock(Duration::from_secs(2))?;
            let mid = first_remaining(pilot);
            pilot.click("#pause")?;
            assert!(!first_running(pilot), "Pause must stop the card");
            pilot.advance_clock(Duration::from_secs(4))?;
            assert_eq!(first_remaining(pilot), mid, "a paused card must not decrement");
            Ok(())
        })
        .unwrap();
    }

    /// Reset restores a running card to 25:00 and stops it.
    #[test]
    fn reset_restores_full_and_stops() {
        run_test(PomodoroApp::new(), |pilot| {
            pilot.click("#start")?;
            pilot.advance_clock(Duration::from_secs(4))?;
            assert!(first_remaining(pilot) < POMODORO_SECS);
            pilot.click("#reset")?;
            assert_eq!(first_remaining(pilot), POMODORO_SECS, "Reset must restore 25:00");
            assert!(!first_running(pilot), "Reset must stop the card");
            Ok(())
        })
        .unwrap();
    }

    /// A card that reaches zero posts `PomodoroFinished` ITSELF, which the app
    /// counts — proving the message originates from the card and drives the
    /// app's reactive `completed` watch.
    #[test]
    fn finishing_a_card_bumps_the_app_counter() {
        run_test(PomodoroApp::new(), |pilot| {
            assert_eq!(completed(pilot), 0);
            pilot.click("#start")?;
            // Run the full 25:00 out (plus a hair) so the card crosses zero.
            pilot.advance_clock(Duration::from_secs(1501))?;
            assert_eq!(first_remaining(pilot), 0.0, "the card must bottom out at 00:00");
            assert!(!first_running(pilot), "a finished card must stop itself");
            assert_eq!(
                completed(pilot),
                1,
                "the card's own PomodoroFinished must bump the app counter"
            );
            Ok(())
        })
        .unwrap();
    }
}
