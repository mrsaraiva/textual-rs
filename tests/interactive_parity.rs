//! Interactive-parity harness — the framework that proves interactive demos
//! actually *do something*.
//!
//! Python Textual ships dozens of interactive examples (buttons that mutate
//! state, clocks that tick, stopwatches that count). A plain snapshot only
//! captures one frame; it cannot tell a *responsive* demo from a *dead* one
//! whose handler was never wired. This harness closes that gap by driving each
//! demo through the real headless [`Pilot`] and asserting that the scripted
//! interaction changed observable state.
//!
//! Two assertion modes (Python `pilot`-style):
//!
//! * [`Assert::Liveness`] — fingerprint the rendered frame *before* the script,
//!   run the script, fingerprint *after*, and require the frame **changed**.
//!   This is the dead-demo detector: an inert app (handler missing / never
//!   fires) produces an identical frame and **fails** the check.
//! * [`Assert::Exact`] — the script ends by querying a concrete value/text and
//!   asserting it equals an expected value. Made deterministic for time-driven
//!   demos by [`Pilot::advance_clock`], which advances the manual test clock by
//!   an exact duration and fires precisely the timers due in that window.
//!
//! The seed entries below demonstrate the harness both ways: a responsive
//! button-counter and a deterministic timer-counter **pass**, while a
//! deliberately inert ("dead") demo is shown to **fail** the Liveness gate.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use textual::prelude::*;
use textual::runtime::Pilot;

// ───────────────────────────── framework ──────────────────────────────────

/// How a harness entry decides whether the scripted interaction "did something".
enum Assert {
    /// The rendered frame must change between the pre- and post-script snapshot.
    /// Catches dead demos whose interaction is inert.
    Liveness,
    /// After the script runs, this predicate must hold (e.g. a queried value
    /// equals the expected, deterministic result). Returns `(ok, detail)`.
    /// Mirrors Python asserting `app.count == 3` after a scripted interaction —
    /// here the demo publishes its observable state through a shared counter the
    /// test inspects, made deterministic by [`Pilot::advance_clock`].
    Exact(Box<dyn Fn() -> (bool, String)>),
}

/// One interactive-parity entry: build an app, script an interaction, assert it
/// did something. Mirrors a `{ name, build, script, assert }` row.
struct Entry<T: TextualApp + 'static> {
    name: &'static str,
    build: Box<dyn Fn() -> T>,
    script: Box<dyn Fn(&mut Pilot) -> textual::Result<()>>,
    assert: Assert,
}

/// Run one entry under the headless harness, returning `Ok(())` if the
/// interaction is verified live/correct, or `Err(detail)` describing why the
/// demo appears dead/wrong. (The harness itself never panics on a dead demo —
/// it *reports* — so callers can assert both pass and fail outcomes.)
fn run_entry<T: TextualApp + 'static>(entry: Entry<T>) -> std::result::Result<(), String> {
    let Entry {
        name,
        build,
        script,
        assert,
    } = entry;

    let outcome = std::cell::RefCell::new(Ok::<(), String>(()));
    textual::run_test(build(), |pilot| {
        // The harness foundation: time is deterministic for the whole run.
        assert!(
            pilot.clock_is_manual(),
            "run_test must install the deterministic manual clock"
        );

        match &assert {
            Assert::Liveness => {
                let before = pilot.app().frame_fingerprint();
                script(pilot)?;
                let after = pilot.app().frame_fingerprint();
                if before == after {
                    *outcome.borrow_mut() = Err(format!(
                        "[{name}] DEAD: rendered frame unchanged after interaction \
                         (fingerprint {before:#018x}); the demo did nothing"
                    ));
                }
            }
            Assert::Exact(predicate) => {
                script(pilot)?;
                let (ok, detail) = predicate();
                if !ok {
                    *outcome.borrow_mut() =
                        Err(format!("[{name}] WRONG: {detail}"));
                }
            }
        }
        Ok(())
    })
    .map_err(|e| format!("[{name}] harness error: {e:?}"))?;

    outcome.into_inner()
}

// ───────────────────────── demos under test ───────────────────────────────

/// A responsive counter: clicking the button bumps a counter and rewrites the
/// label. This is the canonical *live* interaction. The counter is published
/// through a shared [`AtomicU32`] so the test can assert exact state — the
/// analogue of Python asserting `app.count == 3`.
struct CounterApp {
    count: Arc<AtomicU32>,
}

const COUNTER_CSS: &str = r#"
Screen { align: center middle; }
Vertical { width: auto; height: auto; }
"#;

fn count_label(count: u32) -> String {
    format!("Count: {count}")
}

impl TextualApp for CounterApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(COUNTER_CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Vertical::new().with_compose(textual::compose![
            Static::new(count_label(0)).id("readout"),
            Button::new("Increment").id("inc"),
        ]))
    }

    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Some(bp) = message.downcast_ref::<ButtonPressed>() {
            if bp.button_id.as_deref() == Some("inc") {
                let next = self.count.fetch_add(1, Ordering::SeqCst) + 1;
                let label = count_label(next);
                let _ = app.with_query_one_mut_as::<Static, _>("#readout", |s| s.update(label));
                ctx.set_handled();
                ctx.request_repaint();
            }
        }
    }
}

/// A time-driven counter: a 1s interval timer increments a tick count and
/// rewrites the label — the deterministic analogue of a clock/stopwatch. With
/// the manual clock, `advance_clock(N s)` produces exactly N ticks.
struct TickApp {
    ticks: Arc<AtomicU32>,
}

const TICK_CSS: &str = r#"
Screen { align: center middle; }
"#;

fn tick_label(ticks: u32) -> String {
    format!("Ticks: {ticks}")
}

impl TextualApp for TickApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(TICK_CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Static::new(tick_label(0)).id("ticks"))
    }

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut EventCtx) {
        // Python: self.set_interval(1, self.tick). Each fire increments the
        // tick counter and rewrites the readout — exactly a clock's update.
        let ticks = Arc::clone(&self.ticks);
        app.set_interval(
            Duration::from_secs(1),
            None,
            false,
            Box::new(move |app, ctx| {
                let next = ticks.fetch_add(1, Ordering::SeqCst) + 1;
                let label = tick_label(next);
                let _ = app.with_query_one_mut_as::<Static, _>("#ticks", |s| s.update(label));
                ctx.request_repaint();
            }),
        );
    }
}

/// A deliberately *dead* demo: it has a button but no handler, so clicking it
/// changes nothing. The harness must flag this under the Liveness gate.
#[derive(Default)]
struct DeadApp;

const DEAD_CSS: &str = r#"
Screen { align: center middle; }
"#;

impl TextualApp for DeadApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(DEAD_CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        // A Static (no focus, no handler). Pressing an unmapped key cannot
        // change anything — the frame is inert.
        AppRoot::new().with_child(Static::new("I do nothing").id("inert"))
    }
}

// ───────────────────────────── seed tests ─────────────────────────────────

#[test]
fn seed_button_counter_is_live() {
    // Clicking the increment button changes the rendered frame -> Liveness pass.
    let count = Arc::new(AtomicU32::new(0));
    let entry = Entry {
        name: "button_counter",
        build: Box::new({
            let count = Arc::clone(&count);
            move || CounterApp {
                count: Arc::clone(&count),
            }
        }),
        script: Box::new(|pilot| pilot.click("#inc")),
        assert: Assert::Liveness,
    };
    run_entry(entry).expect("responsive button-counter must pass the Liveness gate");
}

#[test]
fn seed_button_counter_exact_state() {
    // Three clicks -> exactly count == 3, deterministically.
    let count = Arc::new(AtomicU32::new(0));
    let entry = Entry {
        name: "button_counter_exact",
        build: Box::new({
            let count = Arc::clone(&count);
            move || CounterApp {
                count: Arc::clone(&count),
            }
        }),
        script: Box::new(|pilot| {
            pilot.click("#inc")?;
            pilot.click("#inc")?;
            pilot.click("#inc")?;
            Ok(())
        }),
        assert: Assert::Exact(Box::new({
            let count = Arc::clone(&count);
            move || {
                let got = count.load(Ordering::SeqCst);
                (got == 3, format!("count = {got}, expected 3"))
            }
        })),
    };
    run_entry(entry).expect("three clicks must yield exactly count == 3");
}

#[test]
fn seed_timer_counter_exact_via_advance_clock() {
    // Advancing the deterministic clock by 3s fires exactly 3 interval ticks.
    let ticks = Arc::new(AtomicU32::new(0));
    let entry = Entry {
        name: "timer_counter_exact",
        build: Box::new({
            let ticks = Arc::clone(&ticks);
            move || TickApp {
                ticks: Arc::clone(&ticks),
            }
        }),
        script: Box::new(|pilot| pilot.advance_clock(Duration::from_secs(3))),
        assert: Assert::Exact(Box::new({
            let ticks = Arc::clone(&ticks);
            move || {
                let got = ticks.load(Ordering::SeqCst);
                (got == 3, format!("ticks = {got}, expected 3"))
            }
        })),
    };
    run_entry(entry).expect("advancing 3s must produce exactly 3 ticks");
}

#[test]
fn seed_timer_counter_is_live() {
    // A single second of (deterministic) time changes the rendered frame.
    let ticks = Arc::new(AtomicU32::new(0));
    let entry = Entry {
        name: "timer_counter_live",
        build: Box::new({
            let ticks = Arc::clone(&ticks);
            move || TickApp {
                ticks: Arc::clone(&ticks),
            }
        }),
        script: Box::new(|pilot| pilot.advance_clock(Duration::from_secs(1))),
        assert: Assert::Liveness,
    };
    run_entry(entry).expect("a time-driven demo must change its frame as time advances");
}

#[test]
fn seed_dead_demo_fails_liveness_gate() {
    // THE proof the harness works: a non-responsive interaction (pressing an
    // unmapped key on a handler-less app) leaves the frame unchanged, so the
    // Liveness gate REPORTS a dead demo. We assert the harness returns Err —
    // demonstrating it FAILS for dead demos and PASSES for live ones.
    let entry = Entry {
        name: "dead_demo",
        build: Box::new(|| DeadApp),
        script: Box::new(|pilot| pilot.press(&["x"])),
        assert: Assert::Liveness,
    };
    let result = run_entry(entry);
    assert!(
        result.is_err(),
        "the harness MUST flag a dead demo: a non-responsive interaction left \
         the frame unchanged, but the Liveness gate reported success"
    );
    let detail = result.unwrap_err();
    assert!(
        detail.contains("DEAD"),
        "dead-demo failure should explain the demo did nothing, got: {detail}"
    );
}
