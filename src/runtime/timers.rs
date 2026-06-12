use crate::message::{MessageEvent, TimerCancelled, TimerFired};
use crate::node_id::NodeId;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
struct RunningTimer {
    target: NodeId,
    due_at: Instant,
}

#[derive(Debug, Default)]
pub(crate) struct OneShotTimerRuntime {
    running: HashMap<u64, RunningTimer>,
}

impl OneShotTimerRuntime {
    pub(crate) fn schedule(
        &mut self,
        timer_id: u64,
        target: NodeId,
        delay: Duration,
    ) -> Option<MessageEvent> {
        let replaced = self.running.insert(
            timer_id,
            RunningTimer {
                target,
                due_at: Instant::now() + delay,
            },
        );
        replaced.map(|timer| self.cancelled_event(timer_id, timer.target))
    }

    pub(crate) fn cancel(&mut self, timer_id: u64) -> Option<MessageEvent> {
        let timer = self.running.remove(&timer_id)?;
        Some(self.cancelled_event(timer_id, timer.target))
    }

    pub(crate) fn next_timeout(&self, now: Instant) -> Option<Duration> {
        self.running
            .values()
            .map(|timer| timer.due_at.saturating_duration_since(now))
            .min()
    }

    pub(crate) fn drain_ready(&mut self, now: Instant) -> Vec<MessageEvent> {
        let mut ready_ids = self
            .running
            .iter()
            .filter_map(|(timer_id, timer)| (timer.due_at <= now).then_some(*timer_id))
            .collect::<Vec<_>>();
        ready_ids.sort_unstable();

        let mut ready = Vec::with_capacity(ready_ids.len());
        for timer_id in ready_ids {
            let Some(timer) = self.running.remove(&timer_id) else {
                continue;
            };
            let sender = super::App::runtime_message_sender();
            ready.push(
                MessageEvent::new(
                    sender,
                    TimerFired {
                        timer_id,
                        target: timer.target,
                    },
                )
                .with_control(sender),
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
    use super::OneShotTimerRuntime;
    use crate::message::{
        AsyncTaskCompleted, AsyncTaskRequest, AsyncTaskResult, TimerCancelled, TimerFired,
    };
    use crate::node_id::node_id_from_ffi;
    use crate::runtime::tasks::AsyncTaskRuntime;
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn schedule_then_drain_ready_emits_timer_fired() {
        let mut runtime = OneShotTimerRuntime::default();
        let target_id = node_id_from_ffi(88);
        runtime.schedule(4, target_id, Duration::from_millis(1));
        thread::sleep(Duration::from_millis(2));
        let ready = runtime.drain_ready(Instant::now());
        assert_eq!(ready.len(), 1);
        {
            let m = ready[0].downcast_ref::<TimerFired>().unwrap();
            assert_eq!(m.timer_id, 4);
            assert_eq!(m.target, target_id);
        }
    }

    #[test]
    fn scheduling_existing_timer_replaces_and_cancels_previous_target() {
        let mut runtime = OneShotTimerRuntime::default();
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
        let mut runtime = OneShotTimerRuntime::default();
        let target_id = node_id_from_ffi(5);
        runtime.schedule(17, target_id, Duration::from_millis(1));
        let cancelled = runtime.cancel(17).expect("cancelled event");
        {
            let m = cancelled.downcast_ref::<TimerCancelled>().unwrap();
            assert_eq!(m.timer_id, 17);
            assert_eq!(m.target, target_id);
        }
        thread::sleep(Duration::from_millis(2));
        assert!(runtime.drain_ready(Instant::now()).is_empty());
    }

    #[test]
    fn timer_and_async_task_complete_without_blocking_each_other() {
        let mut timers = OneShotTimerRuntime::default();
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
