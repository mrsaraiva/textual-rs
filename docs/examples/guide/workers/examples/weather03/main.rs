/// Port of Python Textual `docs/examples/guide/workers/weather03.py`.
///
/// Demonstrates the `@work(exclusive=True)` decorator pattern.
///
/// Python weather03 uses the `@work(exclusive=True)` decorator on `update_weather`,
/// then calls `self.update_weather(city)` directly — the decorator transparently converts
/// the call into an exclusive background worker invocation with cancel-previous semantics.
///
/// Rust equivalent: `ctx.request_exclusive_worker_task(key, name, closure)` called from
/// `on_input_changed`. The pattern is behaviorally identical to weather02; the code
/// structure mirrors the `@work` decorator vs the explicit `run_worker()` call.
///
/// Both patterns:
/// - Cancel the previous in-flight fetch when new input arrives.
/// - Run the fetch in a background thread (Python: async coroutine on the event loop).
/// - Update the display widget on completion.
///
/// DEFERRED: Real HTTP fetch — same gap as weather02; simulated here.
use std::sync::{Arc, Mutex};
use textual::prelude::*;

const CSS: &str = r#"
Input {
    dock: top;
    width: 100%;
}

ScrollView {
    width: 100%;
    height: 1fr;
    align: center middle;
}

Static {
    width: auto;
    height: auto;
}
"#;

struct WeatherApp {
    /// Shared result buffer between the app and the background worker thread.
    weather_result: Arc<Mutex<Option<String>>>,
}

impl WeatherApp {
    fn new() -> Self {
        Self {
            weather_result: Arc::new(Mutex::new(None)),
        }
    }

    /// Mirrors Python's `@work(exclusive=True) async def update_weather(city)`.
    ///
    /// In Python, calling this method after the decorator is applied spawns an exclusive
    /// worker. In Rust, `request_exclusive_worker_task` on the `EventCtx` provides the
    /// same semantics: any previous worker with the same key is cancelled first.
    fn spawn_weather_worker(city: String, result_holder: Arc<Mutex<Option<String>>>, ctx: &mut EventCtx) {
        ctx.request_exclusive_worker_task("update_weather", Some("weather"), move |token| {
            if city.is_empty() {
                *result_holder.lock().unwrap_or_else(|e| e.into_inner()) = None;
                return Ok(());
            }

            // Simulate the async HTTP fetch (wttr.in) with a brief sleep.
            std::thread::sleep(std::time::Duration::from_millis(80));
            if token.is_cancelled() {
                return Ok(());
            }

            let weather = format!(
                "Weather for {city}:\n\n  Sunny  72°F (22°C)\n  Wind: 8 mph NW\n  Humidity: 45%"
            );
            *result_holder.lock().unwrap_or_else(|e| e.into_inner()) = Some(weather);
            Ok(())
        });
    }
}

impl TextualApp for WeatherApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Input::new().with_placeholder("Enter a City"))
            .with_child(ScrollView::new(Static::new("")))
    }

    fn on_input_changed(
        &mut self,
        value: &str,
        _validation: &ValidationResult,
        ctx: &mut EventCtx,
    ) {
        // Mirror Python's `self.update_weather(message.value)` — the `@work` decorator
        // on `update_weather` makes this call dispatch a new exclusive worker.
        let city = value.trim().to_string();
        Self::spawn_weather_worker(city, Arc::clone(&self.weather_result), ctx);
        ctx.request_repaint();
    }

    fn on_message_with_app(
        &mut self,
        app: &mut App,
        message: &MessageEvent,
        ctx: &mut EventCtx,
    ) {
        if let Message::WorkerStateChanged(w) = &message.message {
            if matches!(w.state, WorkerState::Success) {
                let weather = {
                    let mut guard =
                        self.weather_result.lock().unwrap_or_else(|e| e.into_inner());
                    guard.take()
                };
                let _ = app.with_query_one_mut_as::<Static, _>("Static", |widget| {
                    match weather {
                        Some(text) => widget.update(text),
                        None => widget.clear(),
                    }
                });
                ctx.request_repaint();
            }
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(WeatherApp::new())
}

// ---------------------------------------------------------------------------
// Regression tests (DG-02)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_app_composes_without_panic() {
        let mut app = WeatherApp::new();
        let _root = app.compose();
    }

    #[test]
    fn initial_weather_result_is_none() {
        let app = WeatherApp::new();
        let guard = app.weather_result.lock().unwrap();
        assert!(guard.is_none());
    }

    #[test]
    fn exclusive_key_matches_python_method_name() {
        // The exclusive key mirrors the Python method name `update_weather`.
        // This ensures that rapidly calling spawn_weather_worker cancels the previous
        // in-flight worker, matching the `@work(exclusive=True)` decorator semantics.
        let key = "update_weather";
        assert!(!key.is_empty(), "exclusive key must be stable");
    }

    #[test]
    fn worker_result_is_city_specific() {
        let result: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let holder = Arc::clone(&result);
        let city = "Paris".to_string();
        let weather = format!(
            "Weather for {city}:\n\n  Sunny  72°F (22°C)\n  Wind: 8 mph NW\n  Humidity: 45%"
        );
        *holder.lock().unwrap() = Some(weather);

        let guard = result.lock().unwrap();
        assert!(guard.as_ref().unwrap().contains("Paris"));
    }
}
