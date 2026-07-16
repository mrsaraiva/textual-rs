//! Gap 6 regression: build-time `on_mount` side effects reach the runtime.
//!
//! A widget mounted during the INITIAL tree build that posts a message AND
//! requests a worker from `on_mount` (the canonical Python `on_mount` + `@work`
//! startup idiom) must have BOTH take effect by the first settled frame,
//! exactly as when the same widget is mounted dynamically via recompose.
//! Pre-fix, `WidgetTree::fire_mount_callbacks` salvaged only the messages and
//! silently dropped every other synth-`EventCtx` side effect (worker requests,
//! animation requests, run_action, stop). These tests pin the repro from
//! `docs/devel/DESIGN_mount_and_cross_screen.md` section 1.
//!
//! NOTE: worker completion is timing-sensitive under heavy load (same PTY
//! flakiness class as any worker test); run idle.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use rich_rs::{Console, ConsoleOptions, Segments};
use textual::message::MessageEvent;
use textual::prelude::*;
use textual::reactive::{ReactiveCtx, RuntimeReactiveEntry, enqueue_runtime_reactive_entry};
use textual::runtime::Pilot;
use textual::widgets::Widget;

// ---------------------------------------------------------------------------
// Shared fixture: a widget that posts a message and requests a worker at mount.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct StartupPing;
textual::impl_message!(StartupPing);

struct StartupWorker {
    worker_ran: Arc<AtomicBool>,
    post_ping: bool,
}

impl Widget for StartupWorker {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn style_type(&self) -> &'static str {
        "StartupWorker"
    }

    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }

    fn on_mount(&mut self, ctx: &mut WidgetCtx) {
        let ran = Arc::clone(&self.worker_ran);
        ctx.request_worker_task(Some("startup-scan"), move |_cancel| {
            ran.store(true, Ordering::SeqCst);
            Ok(())
        });
        if self.post_ping {
            ctx.post_message(StartupPing);
        }
    }
}

// ---------------------------------------------------------------------------
// The repro: initial compose, on_mount posts a message AND requests a worker.
// ---------------------------------------------------------------------------

struct StartupApp {
    worker_ran: Arc<AtomicBool>,
    pings: Arc<AtomicUsize>,
}

impl TextualApp for StartupApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(StartupWorker {
            worker_ran: Arc::clone(&self.worker_ran),
            post_ping: true,
        })
    }

    fn on_message(&mut self, message: &MessageEvent, _ctx: &mut WidgetCtx) {
        if message.is::<StartupPing>() {
            self.pings.fetch_add(1, Ordering::SeqCst);
        }
    }
}

/// The design-note repro, verbatim: pre-fix this failed on the worker half
/// ("mount message delivered: true / mount worker ran after startup: false").
#[test]
fn initial_mount_worker_and_message_both_land_by_first_settled_frame() {
    let worker_ran = Arc::new(AtomicBool::new(false));
    let pings = Arc::new(AtomicUsize::new(0));
    let app = StartupApp {
        worker_ran: Arc::clone(&worker_ran),
        pings: Arc::clone(&pings),
    };

    textual::run_test(app, |pilot: &mut Pilot| {
        pilot.pause()?;
        assert!(
            worker_ran.load(Ordering::SeqCst),
            "worker requested from a build-time on_mount must run by the first \
             settled frame (was silently dropped pre-fix)"
        );
        assert!(
            pings.load(Ordering::SeqCst) >= 1,
            "message posted from the same on_mount must reach the app handler"
        );
        Ok(())
    })
    .expect("headless run_test must succeed");
}

// ---------------------------------------------------------------------------
// Initial-vs-dynamic parity: the SAME widget, mounted via compose and via
// recompose, must run its mount-time worker in both cases.
// ---------------------------------------------------------------------------

#[derive(Reactive)]
struct Host {
    #[reactive(recompose)]
    show: bool,
    worker_ran: Arc<AtomicBool>,
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

    fn reactive_widget(&mut self) -> Option<&mut dyn textual::reactive::ReactiveWidget> {
        Some(self)
    }

    fn compose(&mut self) -> ComposeResult {
        if *self.show() {
            vec![ChildDecl::new(Box::new(StartupWorker {
                worker_ran: Arc::clone(&self.worker_ran),
                post_ping: false,
            }))]
        } else {
            Vec::new()
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut WidgetCtx) {
        if let Event::Key(_) = event {
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

struct ParityApp {
    initial: bool,
    worker_ran: Arc<AtomicBool>,
}

impl TextualApp for ParityApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Host {
            show: self.initial,
            worker_ran: Arc::clone(&self.worker_ran),
        })
    }
}

#[test]
fn mount_worker_runs_when_mounted_in_initial_build() {
    let worker_ran = Arc::new(AtomicBool::new(false));
    let app = ParityApp {
        initial: true,
        worker_ran: Arc::clone(&worker_ran),
    };
    textual::run_test(app, |pilot: &mut Pilot| {
        pilot.pause()?;
        assert!(
            worker_ran.load(Ordering::SeqCst),
            "initial-build mount must run the worker (the pre-fix asymmetry)"
        );
        Ok(())
    })
    .expect("headless run_test must succeed");
}

#[test]
fn mount_worker_runs_when_mounted_via_dynamic_recompose() {
    let worker_ran = Arc::new(AtomicBool::new(false));
    let app = ParityApp {
        initial: false,
        worker_ran: Arc::clone(&worker_ran),
    };
    textual::run_test(app, |pilot: &mut Pilot| {
        pilot.pause()?;
        assert!(
            !worker_ran.load(Ordering::SeqCst),
            "no worker before the recompose mounts the widget"
        );
        // Flip `show` -> recompose mounts StartupWorker -> its on_mount worker
        // request rides the lifecycle drain (the already-working dynamic path).
        pilot.app_mut().action_focus_next();
        pilot.press(&["r"])?;
        assert!(
            worker_ran.load(Ordering::SeqCst),
            "recompose-mounted widget runs its mount-time worker"
        );
        Ok(())
    })
    .expect("headless run_test must succeed");
}

// ---------------------------------------------------------------------------
// ctx.request_stop() from a build-time on_mount now stops the app (newly
// reachable behavior made possible by the bundle; matches Python
// `self.app.exit()` in `on_mount`). Pinned deliberately.
// ---------------------------------------------------------------------------

struct StopOnMount;

impl Widget for StopOnMount {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn style_type(&self) -> &'static str {
        "StopOnMount"
    }

    fn on_mount(&mut self, ctx: &mut WidgetCtx) {
        ctx.request_stop();
    }
}

struct StopApp;

impl TextualApp for StopApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(StopOnMount)
    }
}

#[test]
fn request_stop_from_build_time_mount_stops_the_app() {
    textual::run_test(StopApp, |pilot: &mut Pilot| {
        pilot.pause()?;
        assert!(
            pilot.app().headless_stop_requested(),
            "stop requested from a build-time on_mount must be recorded \
             (the live loop exits on it; headless records it stickily)"
        );
        Ok(())
    })
    .expect("headless run_test must succeed");
}
