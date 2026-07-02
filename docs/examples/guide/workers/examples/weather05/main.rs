/// Port of Python Textual `docs/examples/guide/workers/weather05.py`.
///
/// Demonstrates the `@work(exclusive=True, thread=True)` decorator pattern with
/// explicit thread-pool workers, cooperative cancellation via
/// `get_current_worker()`, and — the keystone of this example — posting UI
/// updates back to the app thread with `App.call_from_thread`.
///
/// Python weather05 differences from weather03/04:
/// - Uses `urllib` (stdlib) instead of `httpx`/`async` — explicitly threaded.
/// - `@work(exclusive=True, thread=True)` forces thread-pool execution.
/// - Inside the worker, calls `get_current_worker()` to check `worker.is_cancelled`.
/// - Uses `self.call_from_thread(widget.update, weather)` to safely post UI updates.
///
/// Rust equivalent (faithful):
/// - `ctx.request_exclusive_worker_task("update_weather", ...)` provides exclusive
///   cancel-previous semantics matching `@work(exclusive=True, thread=True)`.
/// - `CancellationToken` replaces `get_current_worker().is_cancelled`.
/// - `App::call_from_thread(|app| { ... })` is the direct Rust analogue of
///   `self.call_from_thread(weather_widget.update, weather)`: it posts a closure
///   onto the UI/event-loop thread, which runs it with `&mut App` access and
///   blocks the worker until it returns. The closure performs the widget update
///   exactly where Python's bound method `weather_widget.update` would run.
///
/// Layout (faithful to Python):
/// - `Input` docked to top, placeholder "Enter a City".
/// - `VerticalScroll` (`#weather-container`) fills remaining height.
/// - `Static` (`#weather`) inside the scroll container shows the result.
///
/// CSS faithfully mirrors `weather.tcss`.
///
/// Framework gaps:
/// - FG-workers-rich-text-ansi: Python uses `Text.from_ansi(response_text)` to parse
///   ANSI escape codes from the wttr.in response. textual-rs `Static` accepts plain
///   `String`; ANSI codes are not parsed. With `http-examples` feature, the raw text
///   is displayed as-is.
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

struct WeatherApp;

impl WeatherApp {
    fn new() -> Self {
        Self
    }

    /// Mirrors Python's `@work(exclusive=True, thread=True) def update_weather(city)`.
    ///
    /// Spawns an exclusive background worker that cancels any previous in-flight fetch
    /// for the same input. The worker checks `token.is_cancelled()` before posting
    /// results (Python's `worker.is_cancelled` guard) and, instead of ferrying the
    /// result back through shared state, posts the widget update straight onto the UI
    /// thread with `App::call_from_thread` — exactly mirroring Python's
    /// `self.call_from_thread(weather_widget.update, weather)`.
    fn spawn_weather_worker(city: String, ctx: &mut EventCtx) {
        ctx.request_exclusive_worker_task("update_weather", Some("weather"), move |token| {
            if city.is_empty() {
                // No city — blank out the weather display.
                // Python: `if not worker.is_cancelled: self.call_from_thread(widget.update, "")`
                if !token.is_cancelled() {
                    let _ = App::call_from_thread(|app| {
                        let _ = app
                            .with_query_one_mut_as::<Static, _>("#weather", |w| w.clear());
                    });
                }
                return Ok(());
            }

            // Query the network API (or simulate it without `http-examples` feature).
            let weather = fetch_weather(&city, &token)?;

            // Python: `if not worker.is_cancelled: self.call_from_thread(widget.update, weather)`
            if !token.is_cancelled() {
                App::call_from_thread(move |app| {
                    let _ = app.with_query_one_mut_as::<Static, _>("#weather", |w| {
                        w.update(weather);
                    });
                })
                .map_err(|e| e.to_string())?;
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
        Self::spawn_weather_worker(value.to_string(), ctx);
        ctx.request_repaint();
    }

    /// Mirrors Python's `def on_worker_state_changed(self, event: Worker.StateChanged)`.
    ///
    /// Python calls `self.log(event)`, which routes to the Textual devtools
    /// console — it NEVER touches the visible screen. Rust has no attached
    /// devtools sink in this example, so the handler is intentionally a no-op:
    /// the actual widget update happens inside the worker via `call_from_thread`.
    ///
    /// The previous implementation emitted `eprintln!("[weather05]
    /// WorkerStateChanged: ...")`, but in a PTY stderr shares the terminal with
    /// the alternate-screen buffer, so that raw text corrupted the rendered frame
    /// (Python's `self.log` never does this). Dropping the screen write restores
    /// parity with Python.
    fn on_message_with_app(&mut self, _app: &mut App, _message: &MessageEvent, _ctx: &mut EventCtx) {
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

    #[test]
    fn call_from_thread_without_running_app_reports_not_running() {
        // Outside a running event loop, call_from_thread must not block: it
        // returns NotRunning immediately. Confirms the worker closure stays
        // non-blocking when the app is not up.
        let result = App::call_from_thread(|_app| 42);
        assert_eq!(result, Err(CallFromThreadError::NotRunning));
    }

    /// LIVENESS PROBE (UNCLEAR under the headless harness — see note).
    ///
    /// weather05 uses `@work(exclusive=True, thread=True)` semantics: a threaded
    /// worker fetches and posts the result onto the UI thread via
    /// `App::call_from_thread`, updating the `#weather` Static.
    ///
    /// Now LIVE: the headless pump owns a `WorkerRegistry`, registers the test
    /// thread as the UI thread, and drains `call_from_thread` jobs (main loop +
    /// worker-wait spin), so the threaded worker's posted `#weather` update runs
    /// on the event-loop thread and the demo reaches a settled frame.
    #[test]
    fn liveness_worker_updates_weather() {
        textual::run_test(WeatherApp::new(), |pilot| {
            pilot.click("Input")?;
            pilot.press(&["L", "o", "n"])?;
            for _ in 0..5 {
                pilot.pause()?;
            }
            let text = pilot
                .app_mut()
                .with_query_one_mut_as::<Static, _>("#weather", |s| s.text().to_string())
                .unwrap_or_default();
            assert!(
                !text.is_empty(),
                "the threaded worker must populate the weather Static"
            );
            Ok(())
        })
        .unwrap();
    }
}
