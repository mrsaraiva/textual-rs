use crate::message::{
    AsyncDirectoryEntry, AsyncTaskRequest, AsyncTaskResult, Message, MessageEvent,
};
use crate::widgets::WidgetId;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};

#[derive(Debug)]
struct RunningTask {
    generation: u64,
    target: WidgetId,
    cancel_flag: Arc<AtomicBool>,
}

#[derive(Debug)]
struct TaskCompletion {
    task_id: u64,
    generation: u64,
    target: WidgetId,
    result: AsyncTaskResult,
}

#[derive(Debug)]
pub(crate) struct AsyncTaskRuntime {
    running: HashMap<u64, RunningTask>,
    completion_tx: Sender<TaskCompletion>,
    completion_rx: Receiver<TaskCompletion>,
}

impl Default for AsyncTaskRuntime {
    fn default() -> Self {
        let (completion_tx, completion_rx) = mpsc::channel();
        Self {
            running: HashMap::new(),
            completion_tx,
            completion_rx,
        }
    }
}

impl AsyncTaskRuntime {
    pub(crate) fn spawn(
        &mut self,
        task_id: u64,
        target: WidgetId,
        request: AsyncTaskRequest,
    ) -> Option<MessageEvent> {
        let (previous_generation, replaced) = if let Some(previous) = self.running.remove(&task_id)
        {
            previous.cancel_flag.store(true, Ordering::Relaxed);
            (
                previous.generation,
                Some(MessageEvent {
                    sender: super::App::runtime_message_sender(),
                    message: Message::AsyncTaskCancelled {
                        task_id,
                        target: previous.target,
                    },
                }),
            )
        } else {
            (0, None)
        };
        let generation = previous_generation + 1;
        let cancel_flag = Arc::new(AtomicBool::new(false));

        self.running.insert(
            task_id,
            RunningTask {
                generation,
                target,
                cancel_flag: Arc::clone(&cancel_flag),
            },
        );

        let tx = self.completion_tx.clone();
        std::thread::spawn(move || {
            if cancel_flag.load(Ordering::Relaxed) {
                return;
            }
            let result = execute_request(request);
            if cancel_flag.load(Ordering::Relaxed) {
                return;
            }
            let _ = tx.send(TaskCompletion {
                task_id,
                generation,
                target,
                result,
            });
        });
        replaced
    }

    pub(crate) fn cancel(&mut self, task_id: u64) -> Option<MessageEvent> {
        let task = self.running.remove(&task_id)?;
        task.cancel_flag.store(true, Ordering::Relaxed);
        Some(MessageEvent {
            sender: super::App::runtime_message_sender(),
            message: Message::AsyncTaskCancelled {
                task_id,
                target: task.target,
            },
        })
    }

    pub(crate) fn cancel_for_target(&mut self, target: WidgetId) -> Vec<MessageEvent> {
        let ids = self
            .running
            .iter()
            .filter_map(|(task_id, task)| (task.target == target).then_some(*task_id))
            .collect::<Vec<_>>();
        let mut cancelled = Vec::new();
        for task_id in ids {
            if let Some(event) = self.cancel(task_id) {
                cancelled.push(event);
            }
        }
        cancelled
    }

    pub(crate) fn drain_completed(&mut self) -> Vec<MessageEvent> {
        let mut out = Vec::new();
        loop {
            let completion = match self.completion_rx.try_recv() {
                Ok(completion) => completion,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            };

            let Some(active) = self.running.get(&completion.task_id) else {
                continue;
            };
            if active.generation != completion.generation {
                continue;
            }

            self.running.remove(&completion.task_id);
            out.push(MessageEvent {
                sender: super::App::runtime_message_sender(),
                message: Message::AsyncTaskCompleted {
                    task_id: completion.task_id,
                    target: completion.target,
                    result: completion.result,
                },
            });
        }
        out
    }
}

fn execute_request(request: AsyncTaskRequest) -> AsyncTaskResult {
    match request {
        AsyncTaskRequest::ReadDirectory { path, show_hidden } => {
            read_directory_request(path, show_hidden)
        }
        AsyncTaskRequest::Sleep { duration, label } => {
            let start = std::time::Instant::now();
            std::thread::sleep(duration);
            AsyncTaskResult::SleepFinished {
                label,
                elapsed: start.elapsed(),
            }
        }
    }
}

fn read_directory_request(path: String, show_hidden: bool) -> AsyncTaskResult {
    let path_buf = PathBuf::from(&path);
    let read_dir = match fs::read_dir(&path_buf) {
        Ok(read_dir) => read_dir,
        Err(error) => {
            return AsyncTaskResult::Failed {
                path,
                error: error.to_string(),
            };
        }
    };

    let mut entries = Vec::new();
    for entry in read_dir.flatten() {
        let entry_path = entry.path();
        let Some(name) = entry_path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        entries.push(AsyncDirectoryEntry {
            path: entry_path.display().to_string(),
            label: name.to_string(),
            is_dir,
        });
    }

    entries.sort_by(|left, right| {
        right
            .is_dir
            .cmp(&left.is_dir)
            .then_with(|| left.label.to_lowercase().cmp(&right.label.to_lowercase()))
            .then_with(|| left.label.cmp(&right.label))
    });

    AsyncTaskResult::DirectoryEntries { path, entries }
}

#[cfg(test)]
mod tests {
    use super::AsyncTaskRuntime;
    use crate::message::{AsyncTaskRequest, Message};
    use crate::widgets::WidgetId;
    use std::fs;
    use std::path::PathBuf;
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    struct TempTreeDir {
        path: PathBuf,
    }

    impl TempTreeDir {
        fn new(label: &str) -> Self {
            let mut path = std::env::temp_dir();
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock before epoch")
                .as_nanos();
            path.push(format!("textual-rs-{label}-{}-{stamp}", std::process::id()));
            fs::create_dir_all(&path).expect("create temp test directory");
            Self { path }
        }
    }

    impl Drop for TempTreeDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn wait_for_messages(runtime: &mut AsyncTaskRuntime) -> Vec<crate::message::MessageEvent> {
        for _ in 0..200 {
            let drained = runtime.drain_completed();
            if !drained.is_empty() {
                return drained;
            }
            thread::sleep(Duration::from_millis(1));
        }
        Vec::new()
    }

    #[test]
    fn read_directory_task_completes_with_directory_entries() {
        let temp = TempTreeDir::new("async-task-complete");
        fs::create_dir_all(temp.path.join("nested")).expect("create nested dir");
        fs::write(temp.path.join("alpha.txt"), "alpha").expect("write file");

        let mut runtime = AsyncTaskRuntime::default();
        runtime.spawn(
            7,
            WidgetId::from_u64(10),
            AsyncTaskRequest::ReadDirectory {
                path: temp.path.display().to_string(),
                show_hidden: false,
            },
        );

        let messages = wait_for_messages(&mut runtime);
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            &messages[0].message,
            Message::AsyncTaskCompleted { task_id, target, .. }
                if *task_id == 7 && *target == WidgetId::from_u64(10)
        ));
    }

    #[test]
    fn cancelling_running_task_emits_cancelled_and_suppresses_completion() {
        let temp = TempTreeDir::new("async-task-cancel");
        fs::write(temp.path.join("alpha.txt"), "alpha").expect("write file");

        let mut runtime = AsyncTaskRuntime::default();
        runtime.spawn(
            5,
            WidgetId::from_u64(22),
            AsyncTaskRequest::ReadDirectory {
                path: temp.path.display().to_string(),
                show_hidden: false,
            },
        );

        let cancelled = runtime.cancel(5).expect("cancelled message");
        assert!(matches!(
            cancelled.message,
            Message::AsyncTaskCancelled { task_id: 5, target } if target == WidgetId::from_u64(22)
        ));

        let messages = wait_for_messages(&mut runtime);
        assert!(messages.is_empty());
    }
}
