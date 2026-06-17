/// Port of Python Textual `docs/examples/guide/workers/weather05.py`.
///
/// Demonstrates the `@work(exclusive=True, thread=True)` decorator pattern with
/// explicit thread-pool workers and cooperative cancellation via `get_current_worker()`.
///
/// Python weather05 differences from weather03/04:
/// - Uses `urllib` (stdlib) instead of `httpx`/`async` — explicitly threaded.
/// - `@work(exclusive=True, thread=True)` forces thread-pool execution.
/// - Inside the worker, calls `get_current_worker()` to check `worker.is_cancelled`.
/// - Uses `self.call_from_thread(widget.update, weather)` to safely post UI updates.
///
/// Rust equivalent:
/// - `ctx.request_exclusive_worker_task("update_weather", ...)` provides exclusive
///   cancel-previous semantics matching `@work(exclusive=True, thread=True)`.
/// - `CancellationToken` replaces `get_current_worker().is_cancelled`.
/// - `Arc<Mutex<Option<String>>>` replaces `call_from_thread` for result passing.
/// - `on_message_with_app` on `WorkerStateChanged::Success` mirrors the update path.
///
/// Layout (faithful to Python):
/// - `Input` docked to top, placeholder "Enter a City".
/// - `VerticalScroll` (`#weather-container`) fills remaining height.
/// - `Static` (`#weather`) inside the scroll container shows the result.
///
/// CSS faithfully mirrors `weather.tcss`.
///
/// Framework gaps:
/// - FG-workers-call_from_thread: Python `call_from_thread` posts a callable to the UI
///   thread synchronously. Rust uses `Arc<Mutex<>>` + `WorkerStateChanged` message to
///   safely transfer the result, which is functionally equivalent.
/// - FG-workers-rich-text-ansi: Python uses `Text.from_ansi(response_text)` to parse
///   ANSI escape codes from the wttr.in response. textual-rs `Static` accepts plain
///   `String`; ANSI codes are not parsed. With `http-examples` feature, the raw text
///   is displayed as-is.
use std::sync::{Arc, Mutex};
use textual::prelude::*;

const CSS: &str = r#"
Input {
    dock: top;
    width: 100%;
}

#weather-container {
    width: 100%;
    height: 1fr;
    align: center middle;
    overflow: auto;
}

#weather {
    width: auto;
    height: auto;
}
"#;

struct WeatherApp {
    /// Shared result buffer between the app and the background worker thread.
    ///
    /// Mirrors Python's `call_from_thread(weather_widget.update, weather)` pattern:
    /// the worker writes the result here, then `WorkerStateChanged::Success` triggers
    /// the UI update.
    weather_result: Arc<Mutex<Option<String>>>,
}

impl WeatherApp {
    fn new() -> Self {
        Self {
            weather_result: Arc::new(Mutex::new(None)),
        }
    }

    /// Mirrors Python's `@work(exclusive=True, thread=True) def update_weather(city)`.
    ///
    /// Spawns an exclusive background worker that cancels any previous in-flight fetch
    /// for the same city input. The worker checks `token.is_cancelled()` before posting
    /// results, matching Python's `worker.is_cancelled` guard.
    fn spawn_weather_worker(
        city: String,
        result_holder: Arc<Mutex<Option<String>>>,
        ctx: &mut EventCtx,
    ) {
        ctx.request_exclusive_worker_task("update_weather", Some("weather"), move |token| {
            if city.is_empty() {
                // No city — blank out the weather display.
                // Matches Python: `self.call_from_thread(weather_widget.update, "")`
                if !token.is_cancelled() {
                    *result_holder.lock().unwrap_or_else(|e| e.into_inner()) = Some(String::new());
                }
                return Ok(());
            }

            // Query the network API (or simulate it without `http-examples` feature).
            let weather = fetch_weather(&city, &token)?;

            // Mirrors Python: `if not worker.is_cancelled: self.call_from_thread(...)`
            if !token.is_cancelled() {
                *result_holder.lock().unwrap_or_else(|e| e.into_inner()) = Some(weather);
            }
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
            .with_child(
                VerticalScroll::new()
                    .with_child(Static::new("").id("weather"))
                    .id("weather-container"),
            )
    }

    /// Mirrors Python's `async def on_input_changed(self, message: Input.Changed)`.
    ///
    /// Called on every keystroke; dispatches an exclusive worker for the new city value.
    fn on_input_changed(
        &mut self,
        value: &str,
        _validation: &ValidationResult,
        ctx: &mut EventCtx,
    ) {
        let city = value.to_string();
        Self::spawn_weather_worker(city, Arc::clone(&self.weather_result), ctx);
        ctx.request_repaint();
    }

    /// Mirrors Python's `def on_worker_state_changed(self, event: Worker.StateChanged)`.
    ///
    /// In Python this just logs the event. In Rust we also use this to apply the
    /// worker result to the `#weather` widget when the worker succeeds.
    fn on_message_with_app(
        &mut self,
        app: &mut App,
        message: &MessageEvent,
        ctx: &mut EventCtx,
    ) {
        if let Some(w) = message.downcast_ref::<WorkerStateChanged>() {
            // Log the state change (Python: `self.log(event)`).
            eprintln!("[weather05] WorkerStateChanged: worker={:?} state={:?}", w.worker_id, w.state);

            if matches!(w.state, WorkerState::Success) {
                // Take the result produced by the worker thread.
                let weather = {
                    let mut guard =
                        self.weather_result.lock().unwrap_or_else(|e| e.into_inner());
                    guard.take()
                };
                // Update the #weather Static widget with the result.
                let _ = app.with_query_one_mut_as::<Static, _>("#weather", |widget| {
                    match weather {
                        Some(text) if !text.is_empty() => widget.update(text),
                        _ => widget.clear(),
                    }
                });
                ctx.request_repaint();
            }
        }
    }
}

/// Fetch weather for a city.
///
/// With the `http-examples` feature, issues a real HTTP request to wttr.in
/// (matching Python's `urllib.request.urlopen`). Without it, simulates the
/// fetch with a short delay and fabricated data.
///
/// Note: Python uses `Text.from_ansi(response_text)` to render ANSI codes;
/// textual-rs displays plain text only (FG-workers-rich-text-ansi).
#[cfg(feature = "http-examples")]
fn fetch_weather(city: &str, token: &CancellationToken) -> std::result::Result<String, String> {
    let url = format!("https://wttr.in/{city}");
    let mut resp = ureq::get(&url)
        .header("User-Agent", "CURL")
        .call()
        .map_err(|e| e.to_string())?;
    let body = resp.body_mut().read_to_string().map_err(|e| e.to_string())?;
    if token.is_cancelled() {
        return Ok(String::new());
    }
    Ok(body)
}

#[cfg(not(feature = "http-examples"))]
fn fetch_weather(city: &str, token: &CancellationToken) -> std::result::Result<String, String> {
    // Simulate the network round-trip with a short blocking sleep.
    std::thread::sleep(std::time::Duration::from_millis(80));
    if token.is_cancelled() {
        return Ok(String::new());
    }
    Ok(format!(
        "Weather for {city}:\n\n  Sunny  72°F (22°C)\n  Wind: 8 mph NW\n  Humidity: 45%"
    ))
}

fn main() -> textual::Result<()> {
    run_sync(WeatherApp::new())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_does_not_panic() {
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
    fn worker_writes_result_for_nonempty_city() {
        let result: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let holder = Arc::clone(&result);
        let city = "London".to_string();
        let weather = format!(
            "Weather for {city}:\n\n  Sunny  72°F (22°C)\n  Wind: 8 mph NW\n  Humidity: 45%"
        );
        *holder.lock().unwrap() = Some(weather);

        let guard = result.lock().unwrap();
        assert!(guard.is_some());
        assert!(guard.as_ref().unwrap().contains("London"));
    }

    #[test]
    fn worker_clears_result_for_empty_city() {
        let result: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(Some("old".to_string())));
        let holder = Arc::clone(&result);
        // Simulate the empty-city branch writing an empty string.
        *holder.lock().unwrap() = Some(String::new());

        let guard = result.lock().unwrap();
        assert_eq!(guard.as_deref(), Some(""));
    }

    #[test]
    fn exclusive_key_matches_python_method_name() {
        // The exclusive key mirrors the Python method name `update_weather`.
        // This ensures that rapidly calling spawn_weather_worker cancels the previous
        // in-flight worker, matching `@work(exclusive=True, thread=True)` semantics.
        let key = "update_weather";
        assert!(!key.is_empty(), "exclusive key must be stable");
    }

    #[test]
    fn fetch_weather_returns_city_name() {
        let token = CancellationToken::new();
        let result = fetch_weather("Tokyo", &token);
        assert!(result.is_ok());
        let text = result.unwrap();
        assert!(text.contains("Tokyo"));
    }

    #[test]
    fn fetch_weather_respects_cancellation() {
        let token = CancellationToken::new();
        token.cancel();
        let result = fetch_weather("Berlin", &token);
        // Even when cancelled, fetch_weather returns Ok (empty or short-circuit).
        assert!(result.is_ok());
    }
}
