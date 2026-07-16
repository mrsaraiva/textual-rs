//! Acceptance tests for widget-owned timers (WidgetCtx build, step 4).
//!
//! A widget registers a repeating interval in `on_mount_ctx` via
//! `ctx.set_interval`; the callback mutates a reactive field, whose watcher
//! updates observable state. Because the timer runs on the SAME `TimerRuntime`
//! as app timers, `Pilot::advance_clock` drives it deterministically. The
//! returned `TimerHandle` pauses/resumes it, and unmounting the widget purges it.

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rich_rs::{Console, ConsoleOptions, Segments};
use textual::prelude::*;
use textual::reactive::{
    ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget, RuntimeReactiveEntry,
    enqueue_runtime_reactive_entry,
};
use textual::runtime::{Pilot, TimerHandle};
use textual::widgets::Widget;

/// A widget that counts down once per second on a widget-owned interval.
struct Countdown {
    remaining: i32,
    /// Latest value seen by the reactive watcher (observable from the test).
    observed: Arc<AtomicI32>,
    /// Filled at mount so the test can pause/resume the timer.
    handle_slot: Arc<Mutex<Option<TimerHandle>>>,
    /// Bumped every time the timer callback actually runs (to prove no fire
    /// after unmount).
    fires: Arc<AtomicI32>,
}

impl Countdown {
    fn new(
        start: i32,
        observed: Arc<AtomicI32>,
        handle_slot: Arc<Mutex<Option<TimerHandle>>>,
        fires: Arc<AtomicI32>,
    ) -> Self {
        Self {
            remaining: start,
            observed,
            handle_slot,
            fires,
        }
    }

    fn tick(&mut self, ctx: &mut WidgetCtx) {
        self.fires.fetch_add(1, Ordering::SeqCst);
        let old = self.remaining;
        self.remaining -= 1;
        ctx.record_change(
            "remaining",
            ReactiveFlags::reactive(),
            Box::new(old),
            Box::new(self.remaining),
        );
    }
}

impl Widget for Countdown {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn style_type(&self) -> &'static str {
        "Countdown"
    }

    fn focusable(&self) -> bool {
        true
    }

    fn reactive_widget(&mut self) -> Option<&mut dyn ReactiveWidget> {
        Some(self)
    }

    fn on_mount(&mut self, ctx: &mut WidgetCtx) {
        let handle = ctx.set_interval::<Self, _>(Duration::from_secs(1), false, |w, wctx, _tick| {
            w.tick(wctx);
        });
        *self.handle_slot.lock().unwrap() = Some(handle);
    }
}

impl ReactiveWidget for Countdown {
    fn reactive_dispatch(&mut self, changes: &[ReactiveChange], _ctx: &mut ReactiveCtx) {
        for c in changes {
            if c.field_name == "remaining" {
                if let Some(v) = c.new_value.downcast_ref::<i32>() {
                    self.observed.store(*v, Ordering::SeqCst);
                }
            }
        }
    }
}

struct TimerApp {
    start: i32,
    observed: Arc<AtomicI32>,
    handle_slot: Arc<Mutex<Option<TimerHandle>>>,
    fires: Arc<AtomicI32>,
}

impl TextualApp for TimerApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Countdown::new(
            self.start,
            Arc::clone(&self.observed),
            Arc::clone(&self.handle_slot),
            Arc::clone(&self.fires),
        ))
    }
}

#[test]
fn advance_clock_drives_widget_owned_interval_with_pause_resume() {
    let observed = Arc::new(AtomicI32::new(i32::MIN));
    let handle_slot: Arc<Mutex<Option<TimerHandle>>> = Arc::new(Mutex::new(None));
    let fires = Arc::new(AtomicI32::new(0));
    let app = TimerApp {
        start: 10,
        observed: Arc::clone(&observed),
        handle_slot: Arc::clone(&handle_slot),
        fires: Arc::clone(&fires),
    };

    textual::run_test(app, |pilot: &mut Pilot| {
        // Timer registered in on_mount_ctx (RegisterTimer command drained by the
        // startup pump). No fire yet.
        pilot.pause()?;
        assert_eq!(observed.load(Ordering::SeqCst), i32::MIN, "no tick before clock advances");

        // Deterministic drive: 3 seconds → 3 ticks → remaining 10 → 7.
        pilot.advance_clock(Duration::from_secs(3))?;
        assert_eq!(observed.load(Ordering::SeqCst), 7, "3 clock seconds = 3 ticks (10 -> 7)");

        // pause() halts firing.
        let handle = handle_slot.lock().unwrap().expect("timer registered at mount");
        handle.pause();
        pilot.pause()?; // drain the PauseTimer command
        pilot.advance_clock(Duration::from_secs(5))?;
        assert_eq!(observed.load(Ordering::SeqCst), 7, "paused timer does not fire");

        // resume() continues from where it left off.
        handle.resume();
        pilot.pause()?; // drain the ResumeTimer command
        pilot.advance_clock(Duration::from_secs(2))?;
        assert_eq!(observed.load(Ordering::SeqCst), 5, "resumed timer ticks (7 -> 5)");

        Ok(())
    })
    .expect("headless run_test must succeed");
}

// ===========================================================================
// RA2.0 — headless lifecycle convergence
//
// A widget mounted via DYNAMIC RECOMPOSE (not initial compose) must receive
// `on_mount_ctx` under the headless pump, exactly as under the live loop, so its
// `set_interval` timer registers. Before the fix, `headless_pump` never drained
// tree lifecycle events, so a recompose-mounted timer widget never got
// `on_mount_ctx` and its timer never fired under `advance_clock`.
// ===========================================================================

/// A host whose subtree recomposes when its `show` reactive flips: while `show`
/// is false it composes nothing; once flipped true it composes a `Countdown`
/// (whose `on_mount_ctx` registers a widget-owned interval). `#[reactive(recompose)]`
/// drives the subtree rebuild through the reactive phase.
#[derive(Reactive)]
struct Host {
    #[reactive(recompose)]
    show: bool,
    observed: Arc<AtomicI32>,
    handle_slot: Arc<Mutex<Option<TimerHandle>>>,
    fires: Arc<AtomicI32>,
}

impl Widget for Host {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn style_type(&self) -> &'static str {
        "Host"
    }

    fn focusable(&self) -> bool {
        true
    }

    fn reactive_widget(&mut self) -> Option<&mut dyn ReactiveWidget> {
        Some(self)
    }

    fn compose(&mut self) -> ComposeResult {
        if *self.show() {
            vec![ChildDecl::new(Box::new(Countdown::new(
                10,
                Arc::clone(&self.observed),
                Arc::clone(&self.handle_slot),
                Arc::clone(&self.fires),
            )))]
        } else {
            Vec::new()
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut textual::event::WidgetCtx) {
        if let Event::Key(_) = event {
            // Flip `show` via the widget's own reactive setter and enqueue the
            // change so the reactive phase requests a recompose of this node.
            let node_id = self.node_id();
            let mut reactive = ReactiveCtx::new(node_id);
            self.set_show(true, &mut reactive);
            if reactive.has_changes() {
                enqueue_runtime_reactive_entry(RuntimeReactiveEntry::new(node_id, reactive));
                ctx.set_handled();
            }
        }
    }
}

struct RecomposeApp {
    observed: Arc<AtomicI32>,
    handle_slot: Arc<Mutex<Option<TimerHandle>>>,
    fires: Arc<AtomicI32>,
}

impl TextualApp for RecomposeApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Host {
            show: false,
            observed: Arc::clone(&self.observed),
            handle_slot: Arc::clone(&self.handle_slot),
            fires: Arc::clone(&self.fires),
        })
    }
}

/// Gate: a timer-registering widget mounted via dynamic recompose gets
/// `on_mount_ctx` under the headless pump, so `advance_clock` drives its timer.
#[test]
fn recompose_mounted_widget_registers_timer_headlessly() {
    let observed = Arc::new(AtomicI32::new(i32::MIN));
    let handle_slot: Arc<Mutex<Option<TimerHandle>>> = Arc::new(Mutex::new(None));
    let fires = Arc::new(AtomicI32::new(0));
    let app = RecomposeApp {
        observed: Arc::clone(&observed),
        handle_slot: Arc::clone(&handle_slot),
        fires: Arc::clone(&fires),
    };

    textual::run_test(app, |pilot: &mut Pilot| {
        pilot.pause()?;
        // No Countdown yet (show == false): the timer widget is not mounted.
        assert_eq!(
            observed.load(Ordering::SeqCst),
            i32::MIN,
            "no timer before recompose mounts the widget"
        );
        assert_eq!(fires.load(Ordering::SeqCst), 0, "no fire before recompose");

        // Drive the recompose: focus the Host and press a key → `show` flips true
        // → reactive phase requests a recompose → Countdown mounts.
        pilot.app_mut().action_focus_next();
        pilot.press(&["r"])?;

        // Countdown mounted, but no clock advance yet → still no tick.
        assert_eq!(
            observed.load(Ordering::SeqCst),
            i32::MIN,
            "recompose mounts the widget but no tick until the clock advances"
        );

        // Advance 3 clock seconds → 3 ticks (10 -> 7). This can ONLY happen if
        // the recompose-mounted Countdown got `on_mount_ctx` headlessly and thus
        // registered its interval.
        pilot.advance_clock(Duration::from_secs(3))?;
        assert_eq!(
            observed.load(Ordering::SeqCst),
            7,
            "recompose-mounted timer fired 3 times (proves on_mount_ctx ran headlessly)"
        );
        assert_eq!(
            fires.load(Ordering::SeqCst),
            3,
            "timer callback ran exactly 3 times"
        );
        assert!(
            handle_slot.lock().unwrap().is_some(),
            "timer handle recorded at mount"
        );

        Ok(())
    })
    .expect("headless run_test must succeed");
}

// ===========================================================================
// Public one-shot seam: `WidgetCtx::set_timer` (Python `self.set_timer`).
//
// A widget schedules a one-shot from `on_mount` through the public WidgetCtx
// API (no `event_ctx_mut()` reach-through); the callback runs exactly once,
// `delay` after registration, and never again.
// ===========================================================================

/// A widget that arms a one-shot at mount and counts its fires.
struct OneShot {
    fires: Arc<AtomicI32>,
    handle_slot: Arc<Mutex<Option<TimerHandle>>>,
}

impl Widget for OneShot {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn style_type(&self) -> &'static str {
        "OneShot"
    }

    fn on_mount(&mut self, ctx: &mut WidgetCtx) {
        let handle = ctx.set_timer::<Self, _>(Duration::from_secs(2), |w, _wctx, tick| {
            assert_eq!(tick.fire_count, 1, "one-shot tick reports its single fire");
            w.fires.fetch_add(1, Ordering::SeqCst);
        });
        *self.handle_slot.lock().unwrap() = Some(handle);
    }
}

struct OneShotApp {
    fires: Arc<AtomicI32>,
    handle_slot: Arc<Mutex<Option<TimerHandle>>>,
}

impl TextualApp for OneShotApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(OneShot {
            fires: Arc::clone(&self.fires),
            handle_slot: Arc::clone(&self.handle_slot),
        })
    }
}

#[test]
fn set_timer_one_shot_fires_exactly_once_via_public_widget_ctx_api() {
    let fires = Arc::new(AtomicI32::new(0));
    let handle_slot: Arc<Mutex<Option<TimerHandle>>> = Arc::new(Mutex::new(None));
    let app = OneShotApp {
        fires: Arc::clone(&fires),
        handle_slot: Arc::clone(&handle_slot),
    };

    textual::run_test(app, |pilot: &mut Pilot| {
        pilot.pause()?;
        assert!(
            handle_slot.lock().unwrap().is_some(),
            "set_timer returns a handle immediately at mount"
        );
        assert_eq!(fires.load(Ordering::SeqCst), 0, "no fire before the delay");

        // Not yet due after 1 of the 2 seconds.
        pilot.advance_clock(Duration::from_secs(1))?;
        assert_eq!(fires.load(Ordering::SeqCst), 0, "not due yet at 1s of 2s");

        // Crossing the deadline fires the callback once.
        pilot.advance_clock(Duration::from_secs(1))?;
        assert_eq!(fires.load(Ordering::SeqCst), 1, "one-shot fired at its deadline");

        // Well past several would-be intervals: never fires again.
        pilot.advance_clock(Duration::from_secs(10))?;
        assert_eq!(fires.load(Ordering::SeqCst), 1, "one-shot never fires twice");

        Ok(())
    })
    .expect("headless run_test must succeed");
}

#[test]
fn set_timer_one_shot_can_be_stopped_before_firing() {
    let fires = Arc::new(AtomicI32::new(0));
    let handle_slot: Arc<Mutex<Option<TimerHandle>>> = Arc::new(Mutex::new(None));
    let app = OneShotApp {
        fires: Arc::clone(&fires),
        handle_slot: Arc::clone(&handle_slot),
    };

    textual::run_test(app, |pilot: &mut Pilot| {
        pilot.pause()?;
        let handle = handle_slot.lock().unwrap().expect("timer registered at mount");
        handle.stop();
        pilot.pause()?; // drain the StopTimer command
        pilot.advance_clock(Duration::from_secs(10))?;
        assert_eq!(
            fires.load(Ordering::SeqCst),
            0,
            "a stopped one-shot never fires"
        );
        Ok(())
    })
    .expect("headless run_test must succeed");
}

#[test]
fn unmounting_widget_purges_its_timer_no_fire_after() {
    let observed = Arc::new(AtomicI32::new(i32::MIN));
    let handle_slot: Arc<Mutex<Option<TimerHandle>>> = Arc::new(Mutex::new(None));
    let fires = Arc::new(AtomicI32::new(0));
    let app = TimerApp {
        start: 10,
        observed: Arc::clone(&observed),
        handle_slot: Arc::clone(&handle_slot),
        fires: Arc::clone(&fires),
    };

    textual::run_test(app, |pilot: &mut Pilot| {
        pilot.pause()?;
        pilot.advance_clock(Duration::from_secs(2))?;
        let fires_before = fires.load(Ordering::SeqCst);
        assert_eq!(fires_before, 2, "timer fired twice before unmount");

        // Remove the widget → its node is gone.
        pilot.app_mut()
            .remove("Countdown")
            .map_err(|e| textual::Error::Message(format!("remove Countdown: {e:?}")))?;
        pilot.pause()?;

        // Advance well past several intervals: the timer must not fire the
        // callback again (purged; backstop cancels on the get_mut-None fire).
        pilot.advance_clock(Duration::from_secs(5))?;
        assert_eq!(
            fires.load(Ordering::SeqCst),
            fires_before,
            "no timer callback runs after the owning widget is unmounted"
        );

        Ok(())
    })
    .expect("headless run_test must succeed");
}
