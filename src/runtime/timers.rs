//! Timer subsystem — faithful port of Python Textual `timer.py` +
//! `message_pump.set_interval` / `set_timer`.
//!
//! A [`TimerRuntime`] tracks any number of named timers, each of which fires a
//! [`TimerFired`] message to its target node at a fixed wall-clock cadence.
//! Timers may be one-shot (`set_timer`, `repeat = Some(1)`) or repeating
//! (`set_interval`, `repeat = None` for forever or `Some(n)` for n ticks), and
//! can be paused, resumed, reset, and stopped — mirroring Python's
//! `Timer.pause/resume/reset/stop`.
//!
//! Scheduling is anchored to a [`Clock`] so the event loop drives timers off
//! real wall-clock time while tests can `advance` a deterministic clock and
//! assert exactly how many times a callback fired.
//!
//! Python parity notes (`timer.py`):
//! - `_run` computes `next_timer = start + (count + 1) * interval` and, when
//!   `skip` is enabled, fast-forwards `count` past any deadlines that already
//!   elapsed — so a stalled loop fires once, not a burst (`Timer._run`).
//! - a paused timer (`_active` cleared) does not advance or fire until resumed.
//! - `reset` restarts the schedule from "now" with `count = 0`.

use crate::event::EventCtx;
use crate::message::{MessageEvent, TimerCancelled, TimerFired};
use crate::node_id::NodeId;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// A Rust callback invoked when an app-level timer fires.
///
/// Mirrors Python's `TimerCallback`: a function called on each tick of a
/// `set_interval` / `set_timer` timer. It receives the runtime [`App`] and an
/// [`EventCtx`] so it can mutate reactive state, query widgets, request
/// repaint/recompose, and post messages — exactly what a Python timer callback
/// (e.g. `self.time = datetime.now()`) does.
pub type TimerCallback = Box<dyn FnMut(&mut super::App, &mut EventCtx) + Send>;

/// A monotonic clock the timer runtime schedules against.
///
/// In production the runtime uses [`Clock::wall`], reading `Instant::now()`.
/// Tests use [`Clock::manual`] and call [`Clock::advance`] to move time forward
/// deterministically, so timer behavior can be asserted without sleeping.
#[derive(Debug, Clone)]
pub(crate) struct Clock {
    base: Instant,
    /// When `Some`, the clock is manual: `now()` returns `base + offset` and
    /// only `advance` moves it. When `None`, `now()` reads the wall clock.
    manual_offset: Option<Duration>,
}

impl Clock {
    /// A clock anchored to real wall-clock time.
    pub(crate) fn wall() -> Self {
        Self {
            base: Instant::now(),
            manual_offset: None,
        }
    }

    /// A deterministic clock starting at offset zero, advanced only by
    /// [`Clock::advance`]. Used by tests and the headless [`Pilot`] harness
    /// (`App::run_test`), where it makes time-driven behavior deterministic.
    pub(crate) fn manual() -> Self {
        Self {
            base: Instant::now(),
            manual_offset: Some(Duration::ZERO),
        }
    }

    pub(crate) fn now(&self) -> Instant {
        match self.manual_offset {
            Some(offset) => self.base + offset,
            None => Instant::now(),
        }
    }

    /// Whether this clock is manual (deterministic), as opposed to wall-clock.
    pub(crate) fn is_manual(&self) -> bool {
        self.manual_offset.is_some()
    }

    /// Advance a manual clock by `delta`. No-op on a wall clock.
    pub(crate) fn advance(&mut self, delta: Duration) {
        if let Some(offset) = self.manual_offset.as_mut() {
            *offset += delta;
        }
    }
}

#[derive(Debug, Clone)]
struct RunningTimer {
    target: NodeId,
    /// Time between fires.
    interval: Duration,
    /// Anchor for the schedule. `next due = start + (count + 1) * interval`.
    start: Instant,
    /// Number of times this timer has fired so far.
    count: u64,
    /// `None` = repeat forever, `Some(n)` = fire at most `n` times total.
    repeat: Option<u64>,
    /// When true the timer is paused: it neither advances nor fires until
    /// resumed (mirrors Python `_active` being cleared).
    paused: bool,
    /// When `skip` is enabled a timer that fell behind fires once and
    /// fast-forwards, rather than firing a backlog burst.
    skip: bool,
}

impl RunningTimer {
    /// Wall-clock time at which the timer's next fire is due.
    fn next_due(&self) -> Instant {
        self.start + self.interval * ((self.count + 1) as u32)
    }

    /// Whether the timer has exhausted its repeat budget and should be removed.
    fn finished(&self) -> bool {
        matches!(self.repeat, Some(limit) if self.count >= limit)
    }
}

/// Tracks all live timers and turns elapsed deadlines into [`TimerFired`]
/// messages. Renamed from the earlier one-shot-only runtime; one-shot timers
/// are now just `repeat = Some(1)`.
#[derive(Debug)]
pub(crate) struct TimerRuntime {
    running: HashMap<u64, RunningTimer>,
    clock: Clock,
}

impl Default for TimerRuntime {
    fn default() -> Self {
        Self {
            running: HashMap::new(),
            clock: Clock::wall(),
        }
    }
}

impl TimerRuntime {
    /// Construct a runtime backed by a deterministic manual clock (lib tests
    /// drive the clock directly; the headless [`Pilot`] harness instead uses the
    /// in-place [`TimerRuntime::switch_to_manual`] to preserve startup timers).
    #[cfg(test)]
    pub(crate) fn manual() -> Self {
        Self {
            running: HashMap::new(),
            clock: Clock::manual(),
        }
    }

    /// Current scheduling time.
    pub(crate) fn now(&self) -> Instant {
        self.clock.now()
    }

    /// True if this runtime is driven by a deterministic manual clock.
    pub(crate) fn clock_is_manual(&self) -> bool {
        self.clock.is_manual()
    }

    /// Switch this runtime to a deterministic manual clock **in place**,
    /// preserving every already-scheduled timer.
    ///
    /// Each running timer is re-anchored so the wall-clock time remaining until
    /// its next fire is carried over to the manual timeline (manual `now` starts
    /// at offset zero). This lets the headless harness (`App::run_test`) flip to
    /// deterministic time after startup-scheduled timers already exist, without
    /// dropping them — unlike a fresh [`TimerRuntime::manual`] which would.
    ///
    /// No-op if the clock is already manual.
    pub(crate) fn switch_to_manual(&mut self) {
        if self.clock.is_manual() {
            return;
        }
        let old_now = self.clock.now();
        let manual = Clock::manual();
        let new_now = manual.now();
        for timer in self.running.values_mut() {
            // Preserve "time remaining until next_due" across the swap.
            let next_due = timer.next_due();
            let remaining = next_due.saturating_duration_since(old_now);
            // Re-anchor: start so that next_due == new_now + remaining, with
            // count reset to 0 (one interval ahead from the new start).
            timer.start = (new_now + remaining)
                .checked_sub(timer.interval)
                .unwrap_or(new_now);
            timer.count = 0;
        }
        self.clock = manual;
    }

    /// Advance the (manual) clock — deterministic driver. No-op on wall.
    pub(crate) fn advance(&mut self, delta: Duration) {
        self.clock.advance(delta);
    }

    /// Schedule a one-shot timer (`set_timer`): fires once after `delay`.
    ///
    /// Returns a [`TimerCancelled`] event if an existing timer with the same id
    /// was replaced (matching the prior one-shot replacement semantics).
    pub(crate) fn schedule(
        &mut self,
        timer_id: u64,
        target: NodeId,
        delay: Duration,
    ) -> Option<MessageEvent> {
        self.schedule_full(timer_id, target, delay, Some(1), false)
    }

    /// Schedule a repeating timer (`set_interval`).
    ///
    /// `repeat = None` repeats forever; `Some(n)` fires at most `n` times.
    pub(crate) fn schedule_interval(
        &mut self,
        timer_id: u64,
        target: NodeId,
        interval: Duration,
        repeat: Option<u64>,
        paused: bool,
    ) -> Option<MessageEvent> {
        self.schedule_full(timer_id, target, interval, repeat, paused)
    }

    fn schedule_full(
        &mut self,
        timer_id: u64,
        target: NodeId,
        interval: Duration,
        repeat: Option<u64>,
        paused: bool,
    ) -> Option<MessageEvent> {
        let now = self.clock.now();
        let replaced = self.running.insert(
            timer_id,
            RunningTimer {
                target,
                interval,
                start: now,
                count: 0,
                repeat,
                paused,
                skip: true,
            },
        );
        replaced.map(|timer| self.cancelled_event(timer_id, timer.target))
    }

    /// Stop and remove a timer (`Timer.stop`). Returns a cancellation event if
    /// the timer existed.
    pub(crate) fn cancel(&mut self, timer_id: u64) -> Option<MessageEvent> {
        let timer = self.running.remove(&timer_id)?;
        Some(self.cancelled_event(timer_id, timer.target))
    }

    /// Pause a timer (`Timer.pause`): it stops advancing/firing until resumed.
    /// Returns `true` if a timer with that id existed.
    pub(crate) fn pause(&mut self, timer_id: u64) -> bool {
        if let Some(timer) = self.running.get_mut(&timer_id) {
            timer.paused = true;
            true
        } else {
            false
        }
    }

    /// Resume a paused timer (`Timer.resume`). The schedule is re-anchored to
    /// "now" so a long pause doesn't release a backlog of fires.
    pub(crate) fn resume(&mut self, timer_id: u64) -> bool {
        let now = self.clock.now();
        if let Some(timer) = self.running.get_mut(&timer_id) {
            if timer.paused {
                timer.paused = false;
                timer.start = now;
                timer.count = 0;
            }
            true
        } else {
            false
        }
    }

    /// Reset a timer (`Timer.reset`): restart its schedule from "now".
    pub(crate) fn reset(&mut self, timer_id: u64) -> bool {
        let now = self.clock.now();
        if let Some(timer) = self.running.get_mut(&timer_id) {
            timer.start = now;
            timer.count = 0;
            true
        } else {
            false
        }
    }

    /// True if a timer with `timer_id` is currently registered.
    pub(crate) fn contains(&self, timer_id: u64) -> bool {
        self.running.contains_key(&timer_id)
    }

    /// Time until the soonest non-paused timer is due, relative to `now`.
    pub(crate) fn next_timeout(&self, now: Instant) -> Option<Duration> {
        self.running
            .values()
            .filter(|timer| !timer.paused)
            .map(|timer| timer.next_due().saturating_duration_since(now))
            .min()
    }

    /// Collect every timer whose deadline has elapsed at `now`, advancing or
    /// removing them, and emit a [`TimerFired`] for each fire.
    ///
    /// A repeating timer that fell behind is fast-forwarded (Python `skip`):
    /// `count` jumps past elapsed deadlines so a single fire is emitted instead
    /// of a backlog.
    pub(crate) fn drain_ready(&mut self, now: Instant) -> Vec<MessageEvent> {
        // Deterministic order by timer id for reproducible message sequencing.
        let mut ready_ids = self
            .running
            .iter()
            .filter(|(_, timer)| !timer.paused && timer.next_due() <= now)
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        ready_ids.sort_unstable();

        let mut ready = Vec::new();
        for timer_id in ready_ids {
            let Some(timer) = self.running.get_mut(&timer_id) else {
                continue;
            };

            // Count this fire.
            timer.count += 1;
            let target = timer.target;

            // Skip: fast-forward past any further deadlines already elapsed,
            // counting each skipped deadline toward the repeat budget but only
            // emitting one message. Mirrors Python `_run`'s skip branch.
            if timer.skip {
                while timer.next_due() <= now && !timer.finished() {
                    timer.count += 1;
                }
            }

            let finished = timer.finished();
            if finished {
                self.running.remove(&timer_id);
            }

            let sender = super::App::runtime_message_sender();
            ready.push(
                MessageEvent::new(sender, TimerFired { timer_id, target }).with_control(sender),
            );
        }
        ready
    }

    fn cancelled_event(&self, timer_id: u64, target: NodeId) -> MessageEvent {
        let sender = super::App::runtime_message_sender();
        MessageEvent::new(sender, TimerCancelled { timer_id, target }).with_control(sender)
    }
}

#[cfg(test)]
mod tests {
    use super::TimerRuntime;
    use crate::message::{
        AsyncTaskCompleted, AsyncTaskRequest, AsyncTaskResult, TimerCancelled, TimerFired,
    };
    use crate::node_id::node_id_from_ffi;
    use crate::runtime::tasks::AsyncTaskRuntime;
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn schedule_then_drain_ready_emits_timer_fired() {
        let mut runtime = TimerRuntime::manual();
        let target_id = node_id_from_ffi(88);
        runtime.schedule(4, target_id, Duration::from_millis(1));
        runtime.advance(Duration::from_millis(2));
        let ready = runtime.drain_ready(runtime.now());
        assert_eq!(ready.len(), 1);
        {
            let m = ready[0].downcast_ref::<TimerFired>().unwrap();
            assert_eq!(m.timer_id, 4);
            assert_eq!(m.target, target_id);
        }
    }

    #[test]
    fn one_shot_fires_exactly_once() {
        let mut runtime = TimerRuntime::manual();
        let target = node_id_from_ffi(1);
        runtime.schedule(7, target, Duration::from_secs(1));
        // First advance crosses the deadline -> one fire.
        runtime.advance(Duration::from_secs(1));
        assert_eq!(runtime.drain_ready(runtime.now()).len(), 1);
        // Subsequent advances never fire again.
        runtime.advance(Duration::from_secs(5));
        assert!(runtime.drain_ready(runtime.now()).is_empty());
        assert!(!runtime.contains(7));
    }

    #[test]
    fn interval_fires_repeatedly_n_times_after_n_advances() {
        let mut runtime = TimerRuntime::manual();
        let target = node_id_from_ffi(2);
        runtime.schedule_interval(3, target, Duration::from_secs(1), None, false);

        let mut fires = 0;
        for _ in 0..5 {
            runtime.advance(Duration::from_secs(1));
            fires += runtime.drain_ready(runtime.now()).len();
        }
        assert_eq!(fires, 5, "interval should fire once per advance");
        assert!(runtime.contains(3), "forever interval stays registered");
    }

    #[test]
    fn bounded_repeat_stops_after_limit() {
        let mut runtime = TimerRuntime::manual();
        let target = node_id_from_ffi(9);
        runtime.schedule_interval(11, target, Duration::from_secs(1), Some(3), false);

        let mut fires = 0;
        for _ in 0..6 {
            runtime.advance(Duration::from_secs(1));
            fires += runtime.drain_ready(runtime.now()).len();
        }
        assert_eq!(fires, 3, "bounded interval fires exactly repeat times");
        assert!(!runtime.contains(11), "bounded interval removed after limit");
    }

    #[test]
    fn skip_collapses_backlog_to_single_fire() {
        let mut runtime = TimerRuntime::manual();
        let target = node_id_from_ffi(4);
        runtime.schedule_interval(5, target, Duration::from_secs(1), None, false);
        // Jump far ahead: 10 deadlines elapse, but skip collapses to one fire.
        runtime.advance(Duration::from_secs(10));
        let ready = runtime.drain_ready(runtime.now());
        assert_eq!(ready.len(), 1, "skip collapses backlog to a single fire");
    }

    #[test]
    fn paused_timer_does_not_fire_until_resumed() {
        let mut runtime = TimerRuntime::manual();
        let target = node_id_from_ffi(6);
        runtime.schedule_interval(8, target, Duration::from_secs(1), None, true);
        runtime.advance(Duration::from_secs(3));
        assert!(
            runtime.drain_ready(runtime.now()).is_empty(),
            "paused timer must not fire"
        );
        runtime.resume(8);
        runtime.advance(Duration::from_secs(1));
        assert_eq!(
            runtime.drain_ready(runtime.now()).len(),
            1,
            "resumed timer fires on the next interval"
        );
    }

    #[test]
    fn pause_then_resume_does_not_release_backlog() {
        let mut runtime = TimerRuntime::manual();
        let target = node_id_from_ffi(7);
        runtime.schedule_interval(2, target, Duration::from_secs(1), None, false);
        runtime.pause(2);
        runtime.advance(Duration::from_secs(100));
        assert!(runtime.drain_ready(runtime.now()).is_empty());
        runtime.resume(2);
        // Right after resume, nothing is due yet (re-anchored to now).
        assert!(runtime.drain_ready(runtime.now()).is_empty());
        runtime.advance(Duration::from_secs(1));
        assert_eq!(runtime.drain_ready(runtime.now()).len(), 1);
    }

    #[test]
    fn reset_restarts_schedule() {
        let mut runtime = TimerRuntime::manual();
        let target = node_id_from_ffi(8);
        runtime.schedule_interval(12, target, Duration::from_secs(2), None, false);
        runtime.advance(Duration::from_millis(1500));
        runtime.reset(12);
        // After reset, the original 0.5s remaining is discarded.
        runtime.advance(Duration::from_millis(1500));
        assert!(
            runtime.drain_ready(runtime.now()).is_empty(),
            "reset re-anchors; not yet due"
        );
        runtime.advance(Duration::from_millis(500));
        assert_eq!(runtime.drain_ready(runtime.now()).len(), 1);
    }

    #[test]
    fn scheduling_existing_timer_replaces_and_cancels_previous_target() {
        let mut runtime = TimerRuntime::manual();
        let first = node_id_from_ffi(1);
        let second = node_id_from_ffi(2);
        let replaced = runtime.schedule(9, first, Duration::from_secs(10));
        assert!(replaced.is_none());

        let replaced = runtime
            .schedule(9, second, Duration::from_secs(10))
            .expect("replacement should emit cancellation");
        {
            let m = replaced.downcast_ref::<TimerCancelled>().unwrap();
            assert_eq!(m.timer_id, 9);
            assert_eq!(m.target, first);
        }
    }

    #[test]
    fn cancel_removes_timer_and_prevents_fire() {
        let mut runtime = TimerRuntime::manual();
        let target_id = node_id_from_ffi(5);
        runtime.schedule(17, target_id, Duration::from_millis(1));
        let cancelled = runtime.cancel(17).expect("cancelled event");
        {
            let m = cancelled.downcast_ref::<TimerCancelled>().unwrap();
            assert_eq!(m.timer_id, 17);
            assert_eq!(m.target, target_id);
        }
        runtime.advance(Duration::from_millis(2));
        assert!(runtime.drain_ready(runtime.now()).is_empty());
    }

    #[test]
    fn switch_to_manual_preserves_scheduled_timers() {
        // A timer scheduled on the wall clock survives the in-place switch to a
        // manual clock with its remaining time intact (the Pilot startup case).
        let mut runtime = TimerRuntime::default();
        assert!(!runtime.clock_is_manual());
        let target = node_id_from_ffi(21);
        runtime.schedule_interval(31, target, Duration::from_secs(1), None, false);

        runtime.switch_to_manual();
        assert!(runtime.clock_is_manual());
        assert!(runtime.contains(31), "timer preserved across clock switch");

        // Not yet due immediately after the switch (≈1s remaining).
        assert!(runtime.drain_ready(runtime.now()).is_empty());
        // Advancing the manual clock by the interval fires it deterministically.
        runtime.advance(Duration::from_secs(1));
        assert_eq!(runtime.drain_ready(runtime.now()).len(), 1);
    }

    #[test]
    fn switch_to_manual_is_idempotent() {
        let mut runtime = TimerRuntime::manual();
        assert!(runtime.clock_is_manual());
        let target = node_id_from_ffi(22);
        runtime.schedule_interval(32, target, Duration::from_secs(1), None, false);
        runtime.advance(Duration::from_millis(500));
        // A no-op switch must not disturb the in-flight schedule.
        runtime.switch_to_manual();
        assert!(runtime.drain_ready(runtime.now()).is_empty());
        runtime.advance(Duration::from_millis(500));
        assert_eq!(runtime.drain_ready(runtime.now()).len(), 1);
    }

    #[test]
    fn next_timeout_ignores_paused_timers() {
        let mut runtime = TimerRuntime::manual();
        let target = node_id_from_ffi(3);
        runtime.schedule_interval(1, target, Duration::from_secs(5), None, true);
        let now = runtime.now();
        assert!(
            runtime.next_timeout(now).is_none(),
            "paused timer should not drive the loop timeout"
        );
        runtime.resume(1);
        let now = runtime.now();
        assert_eq!(runtime.next_timeout(now), Some(Duration::from_secs(5)));
    }

    #[test]
    fn timer_and_async_task_complete_without_blocking_each_other() {
        let mut timers = TimerRuntime::default();
        let mut tasks = AsyncTaskRuntime::default();
        let widget_id = node_id_from_ffi(42);
        tasks.spawn(
            1,
            widget_id,
            AsyncTaskRequest::Sleep {
                duration: Duration::from_millis(25),
                label: "load".to_string(),
            },
        );
        timers.schedule(2, widget_id, Duration::from_millis(2));

        let mut saw_timer = false;
        let mut saw_task = false;
        for _ in 0..200 {
            let timer_events = timers.drain_ready(Instant::now());
            saw_timer |= timer_events.iter().any(|event| event.is::<TimerFired>());

            let task_events = tasks.drain_completed();
            saw_task |= task_events.iter().any(|event| {
                event.downcast_ref::<AsyncTaskCompleted>().is_some_and(|m| {
                    m.task_id == 1
                        && m.target == widget_id
                        && matches!(m.result, AsyncTaskResult::SleepFinished { .. })
                })
            });

            if saw_timer && saw_task {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }

        assert!(saw_timer, "timer should fire while async work is running");
        assert!(saw_task, "sleep task should complete");
    }
}
