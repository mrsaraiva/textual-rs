//! Opt-in frame ticks for inactive screens (`App::set_tick_inactive_screens`).
//!
//! By default the per-frame widget tick (`Widget::on_tick`) reaches only the
//! active screen's tree; a pushed screen freezes on-tick-driven animation on
//! every tree beneath it. The opt-in delivers the same tick to background
//! trees: the app-root tree under a pushed stack plus every stacked screen
//! below the top. These tests pin both the default (background trees do NOT
//! tick) and the opt-in (they tick exactly once per advanced frame, and stop
//! again when toggled off).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use rich_rs::{Console, ConsoleOptions, Segments};
use textual::prelude::*;

/// A widget that counts every frame tick it receives.
struct TickProbe {
    ticks: Arc<AtomicU64>,
}

impl Widget for TickProbe {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn style_type(&self) -> &'static str {
        "TickProbe"
    }

    // Frame ticks are delivered to active widgets only; opt in.
    fn is_active(&self) -> bool {
        true
    }

    fn on_tick(&mut self, _tick: u64) {
        self.ticks.fetch_add(1, Ordering::Relaxed);
    }
}

/// Base app: one probe on the app-root tree.
struct ProbeApp {
    ticks: Arc<AtomicU64>,
}

impl TextualApp for ProbeApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(TickProbe {
            ticks: self.ticks.clone(),
        })
    }
}

/// A stacked screen carrying its own probe.
struct ProbeScreen {
    ticks: Arc<AtomicU64>,
}

impl Screen for ProbeScreen {
    fn name(&self) -> &str {
        "probe"
    }

    fn compose(&self) -> Box<dyn Widget> {
        Box::new(VerticalGroup::new().with_child(TickProbe {
            ticks: self.ticks.clone(),
        }))
    }
}

/// A plain screen with no probe, used to cover the trees below it.
struct CoverScreen;

impl Screen for CoverScreen {
    fn name(&self) -> &str {
        "cover"
    }

    fn compose(&self) -> Box<dyn Widget> {
        Box::new(VerticalGroup::new().with_child(Static::new("cover")))
    }
}

#[test]
fn background_trees_do_not_tick_by_default() {
    let root_ticks = Arc::new(AtomicU64::new(0));
    let probe = root_ticks.clone();
    run_test(ProbeApp { ticks: probe }, |pilot| {
        assert!(
            !pilot.app().tick_inactive_screens(),
            "inactive-screen ticking must default off"
        );

        // Active app-root tree: the probe ticks once per advanced frame.
        pilot.advance_ticks(2)?;
        let while_active = root_ticks.load(Ordering::Relaxed);
        assert!(
            while_active >= 2,
            "probe on the active tree should tick, got {while_active}"
        );

        // A pushed screen makes the app-root tree inactive: no more ticks.
        pilot.app_mut().push_screen(Box::new(CoverScreen));
        pilot.pause()?;
        let before = root_ticks.load(Ordering::Relaxed);
        pilot.advance_ticks(3)?;
        assert_eq!(
            root_ticks.load(Ordering::Relaxed),
            before,
            "default: background app-root tree must not tick"
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn opt_in_ticks_reach_approot_and_mid_stack_screens() {
    let root_ticks = Arc::new(AtomicU64::new(0));
    let screen_ticks = Arc::new(AtomicU64::new(0));
    let root_probe = root_ticks.clone();
    let screen_probe = screen_ticks.clone();
    run_test(ProbeApp { ticks: root_probe }, |pilot| {
        // Stack: app root (background), probe screen (background), cover (active).
        pilot.app_mut().push_screen(Box::new(ProbeScreen {
            ticks: screen_probe.clone(),
        }));
        pilot.pause()?;
        pilot.app_mut().push_screen(Box::new(CoverScreen));
        pilot.pause()?;

        pilot.app_mut().set_tick_inactive_screens(true);
        let root_before = root_ticks.load(Ordering::Relaxed);
        let screen_before = screen_ticks.load(Ordering::Relaxed);
        pilot.advance_ticks(4)?;
        assert_eq!(
            root_ticks.load(Ordering::Relaxed),
            root_before + 4,
            "opt-in: background app-root tree ticks once per frame"
        );
        assert_eq!(
            screen_ticks.load(Ordering::Relaxed),
            screen_before + 4,
            "opt-in: background mid-stack screen ticks once per frame"
        );

        // Toggling back off stops background delivery again.
        pilot.app_mut().set_tick_inactive_screens(false);
        let root_frozen = root_ticks.load(Ordering::Relaxed);
        let screen_frozen = screen_ticks.load(Ordering::Relaxed);
        pilot.advance_ticks(2)?;
        assert_eq!(root_ticks.load(Ordering::Relaxed), root_frozen);
        assert_eq!(screen_ticks.load(Ordering::Relaxed), screen_frozen);
        Ok(())
    })
    .unwrap();
}
