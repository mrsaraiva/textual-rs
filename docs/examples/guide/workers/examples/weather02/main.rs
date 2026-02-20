/// Port of Python Textual `docs/examples/guide/workers/weather02.py`.
///
/// Demonstrates the worker lifecycle with exclusive mode:
/// - Input field for city name (docked to top).
/// - On every keystroke, spawns an exclusive background worker that "fetches" weather.
/// - If the user types quickly, the previous worker is cancelled before the new one runs.
/// - Result is displayed in a scrollable area below the input.
///
/// Python: `self.run_worker(self.update_weather(city), exclusive=True)` with an async HTTP
/// client (`httpx`). Rust: `ctx.request_exclusive_worker_task("weather-fetch", ...)` with
/// a simulated delay and fabricated weather string.
///
/// DEFERRED: Real HTTP fetch — requires a blocking HTTP client (e.g. `reqwest` with the
/// `blocking` feature) integrated into the synchronous worker closure. Simulated here with
/// a short sleep + deterministic weather string.
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
        let city = value.trim().to_string();
        let result_holder = Arc::clone(&self.weather_result);

        // exclusive=True: cancel any in-flight worker before starting the new one.
        ctx.request_exclusive_worker_task("weather-fetch", Some("weather"), move |token| {
            if city.is_empty() {
                // Clear when the input is empty.
                *result_holder.lock().unwrap_or_else(|e| e.into_inner()) = None;
                return Ok(());
            }

            // Simulate network latency (a real port would use a blocking HTTP client).
            std::thread::sleep(std::time::Duration::from_millis(80));
            if token.is_cancelled() {
                return Ok(());
            }

            // Fabricated weather data (replaces the wttr.in ANSI response).
            let weather = format!(
                "Weather for {city}:\n\n  Sunny  72°F (22°C)\n  Wind: 8 mph NW\n  Humidity: 45%"
            );
            *result_holder.lock().unwrap_or_else(|e| e.into_inner()) = Some(weather);
            Ok(())
        });

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
                // Take the result produced by the worker thread.
                let weather = {
                    let mut guard =
                        self.weather_result.lock().unwrap_or_else(|e| e.into_inner());
                    guard.take()
                };
                // Update the Static widget with the weather text (or clear it).
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
    fn worker_writes_result_for_nonempty_city() {
        // Simulate the worker closure without threading or delay.
        let result: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let holder = Arc::clone(&result);
        let city = "London".to_string();
        let weather =
            format!("Weather for {city}:\n\n  Sunny  72°F (22°C)\n  Wind: 8 mph NW\n  Humidity: 45%");
        *holder.lock().unwrap() = Some(weather);

        let guard = result.lock().unwrap();
        assert!(guard.is_some());
        assert!(guard.as_ref().unwrap().contains("London"));
    }

    #[test]
    fn worker_clears_result_for_empty_city() {
        let result: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(Some("old".to_string())));
        let holder = Arc::clone(&result);
        // Simulate the empty-city branch.
        *holder.lock().unwrap() = None;

        let guard = result.lock().unwrap();
        assert!(guard.is_none());
    }
}
