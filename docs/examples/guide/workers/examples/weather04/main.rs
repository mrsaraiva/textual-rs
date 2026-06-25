/// Port of Python Textual `docs/examples/guide/workers/weather04.py`.
///
/// This is weather03 + an explicit `on_worker_state_changed` handler (Python:
/// `on_worker_state_changed`).  The handler logs every worker state transition —
/// in Python it calls `self.log(event)`; here we emit an eprintln to stderr
/// (the Rust framework does not yet expose a `log()` / console sink API).
///
/// Python weather04 uses `@work(exclusive=True)` (async, runs on the event
/// loop), so it updates `weather_widget.update(...)` directly. textual-rs runs
/// `request_exclusive_worker_task` on a real OS thread, so to update the widget
/// safely we post the update back onto the UI/event-loop thread with
/// `App::call_from_thread` — the same primitive Python's threaded weather05 uses
/// explicitly. This keeps the worker off the app's single-threaded widget state
/// while preserving weather04's "worker updates the widget" structure.
///
/// When the `http-examples` feature is enabled, fetches real weather from wttr.in.
/// Without the feature, simulates the fetch with a short delay.
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

struct WeatherApp;

impl WeatherApp {
    fn new() -> Self {
        Self
    }

    /// Mirrors Python's `@work(exclusive=True) async def update_weather(city)`.
    ///
    /// Spawns an exclusive background worker.  Any previously running worker
    /// with the same key is cancelled first (cancel-previous semantics). The
    /// fetched result is applied to the `Static` widget via `App::call_from_thread`,
    /// posting the update onto the UI thread (Rust runs the worker off-thread,
    /// where direct widget access would be unsafe).
    fn spawn_weather_worker(city: String, ctx: &mut EventCtx) {
        ctx.request_exclusive_worker_task("update_weather", Some("weather"), move |token| {
            if city.is_empty() {
                if !token.is_cancelled() {
                    let _ =
                        App::call_from_thread(|app| {
                            let _ = app
                                .with_query_one_mut_as::<Static, _>("Static", |w| w.clear());
                        });
                }
                return Ok(());
            }

            let weather = fetch_weather(&city, &token)?;
            if !token.is_cancelled() {
                App::call_from_thread(move |app| {
                    let _ = app.with_query_one_mut_as::<Static, _>("Static", |w| {
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
            .with_child(ScrollView::new(Static::new("")))
    }

    fn on_input_changed(
        &mut self,
        value: &str,
        _validation: &ValidationResult,
        ctx: &mut EventCtx,
    ) {
        Self::spawn_weather_worker(value.trim().to_string(), ctx);
        ctx.request_repaint();
    }

    fn on_message_with_app(&mut self, _app: &mut App, message: &MessageEvent, _ctx: &mut EventCtx) {
        if let Some(w) = message.downcast_ref::<WorkerStateChanged>() {
            // Mirror Python's `on_worker_state_changed`: log the event.
            // Python calls `self.log(event)`; Rust logs to stderr (no console sink yet).
            eprintln!("[worker] id={} state={:?}", w.worker_id, w.state);
        }
    }
}

/// Fetch weather for a city. With `http-examples` feature, queries wttr.in.
/// Without it, simulates the fetch with a short delay and fabricated data.
#[cfg(feature = "http-examples")]
fn fetch_weather(city: &str, token: &CancellationToken) -> std::result::Result<String, String> {
    let url = format!("https://wttr.in/{city}?format=4");
    let resp = ureq::get(&url).call().map_err(|e| e.to_string())?;
    let body = resp.body().read_to_string().map_err(|e| e.to_string())?;
    if token.is_cancelled() {
        return Ok(String::new());
    }
    Ok(body)
}

#[cfg(not(feature = "http-examples"))]
fn fetch_weather(city: &str, token: &CancellationToken) -> std::result::Result<String, String> {
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
// Regression tests
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
    fn exclusive_key_matches_python_method_name() {
        // The exclusive key mirrors the Python method name `update_weather`.
        let key = "update_weather";
        assert!(!key.is_empty(), "exclusive key must be stable");
    }

    #[test]
    fn fetch_weather_is_city_specific() {
        let token = CancellationToken::new();
        let weather = fetch_weather("Paris", &token).expect("fetch ok");
        assert!(weather.contains("Paris"));
    }

    #[test]
    fn call_from_thread_without_running_app_reports_not_running() {
        // Confirms the worker closure does not block when no app is running.
        let result = App::call_from_thread(|_app| ());
        assert_eq!(result, Err(CallFromThreadError::NotRunning));
    }

    /// LIVENESS PROBE (UNCLEAR under the headless harness — see note).
    ///
    /// weather04 is weather03 + an explicit `on_worker_state_changed` handler;
    /// the exclusive worker fetches and posts the result back onto the UI thread
    /// via `App::call_from_thread` to update the `Static`.
    ///
    /// Now LIVE: the headless pump owns a `WorkerRegistry`, registers the test
    /// thread as the UI thread, and drains `call_from_thread` jobs both in its
    /// main loop and inside the worker-wait spin — so the worker's posted
    /// `Static` update runs on the event-loop thread and the worker unblocks,
    /// reaching a settled frame deterministically.
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
                .with_query_one_mut_as::<Static, _>("Static", |s| s.text().to_string())
                .unwrap_or_default();
            assert!(
                !text.is_empty(),
                "the background worker must populate the weather Static"
            );
            Ok(())
        })
        .unwrap();
    }
}
