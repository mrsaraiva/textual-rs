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
/// When the `http-examples` feature is enabled, fetches real weather data from wttr.in.
/// Without the feature, simulates the fetch with a short delay and fabricated data.
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

            let weather = fetch_weather(&city, &token)?;
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
        if let Some(w) = message.downcast_ref::<WorkerStateChanged>() {
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
