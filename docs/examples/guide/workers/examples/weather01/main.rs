/// Port of Python Textual `docs/examples/guide/workers/weather01.py`.
///
/// The "before workers" weather app: fetches weather data directly on every
/// keystroke (blocking the UI in Python; synchronous here).  This is the
/// baseline shown before the workers chapter introduces `run_worker`.
///
/// Layout:
/// - `Input` docked to top for city name.
/// - `VerticalScroll` container (`#weather-container`) fills remaining space.
/// - `Static` widget (`#weather`) inside the scroll view shows the result.
///
/// Framework gap (FG-weather01-async): Python uses `async def update_weather`
/// with `httpx.AsyncClient` for a non-blocking HTTP call.  textual-rs has no
/// async HTTP primitive; the fetch is simulated with a short blocking call.
/// When the `http-examples` feature is enabled, `ureq` is used for a real
/// HTTP request to wttr.in.
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

Static {
    width: auto;
    height: auto;
}
"#;

struct WeatherApp;

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
                    .with_child(Static::new(""))
                    .id("weather-container"),
            )
    }

    fn on_message_with_app(
        &mut self,
        app: &mut App,
        message: &MessageEvent,
        ctx: &mut textual::event::WidgetCtx,
    ) {
        if let Some(m) = message.downcast_ref::<InputChanged>() {
            let city = m.value.trim().to_string();
            let text = if city.is_empty() {
                String::new()
            } else {
                fetch_weather(&city)
            };
            let _ = app.with_query_one_mut_as::<Static, _>("Static", |widget| {
                if text.is_empty() {
                    widget.clear();
                } else {
                    widget.update(text);
                }
            });
            ctx.request_repaint();
        }
    }
}

/// Fetch weather for a city.
///
/// With the `http-examples` feature, issues a real HTTP request to wttr.in.
/// Without it, simulates a response with fabricated data.
#[cfg(feature = "http-examples")]
fn fetch_weather(city: &str) -> String {
    // Python uses bare https://wttr.in/{city} (no query string), match that.
    let url = format!("https://wttr.in/{city}");
    match ureq::get(&url).call() {
        Ok(mut resp) => resp
            .body_mut()
            .read_to_string()
            .unwrap_or_else(|e| format!("Error reading response: {e}")),
        Err(e) => format!("Error fetching weather: {e}"),
    }
}

#[cfg(not(feature = "http-examples"))]
fn fetch_weather(city: &str) -> String {
    // Simulate a brief fetch delay (as Python's httpx would have).
    std::thread::sleep(std::time::Duration::from_millis(50));
    format!("Weather for {city}:\n\n  Sunny  72°F (22°C)\n  Wind: 8 mph NW\n  Humidity: 45%")
}

fn main() -> textual::Result<()> {
    run_sync(WeatherApp)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_does_not_panic() {
        let mut app = WeatherApp;
        let _root = app.compose();
    }

    #[test]
    fn fetch_weather_returns_city_name() {
        let result = fetch_weather("London");
        assert!(result.contains("London"));
    }

    #[test]
    fn fetch_weather_nonempty_for_nonempty_city() {
        let result = fetch_weather("Paris");
        assert!(!result.is_empty());
    }

    /// LIVENESS PROBE — the "before workers" baseline fetches synchronously on
    /// each keystroke (InputChanged) and updates the `Static`. We type a city and
    /// assert the Static's own text became the (fabricated) weather. A dead demo
    /// (unwired InputChanged) leaves the Static empty and fails this gate.
    #[test]
    fn liveness_typing_city_updates_weather() {
        textual::run_test(WeatherApp, |pilot| {
            pilot.click("Input")?;
            pilot.press(&["L", "o", "n"])?;
            let text = pilot
                .app_mut()
                .with_query_one_mut_as::<Static, _>("Static", |s| s.text().to_string())
                .unwrap_or_default();
            assert!(
                text.contains("Lon"),
                "typing a city must populate the weather Static (got {text:?})"
            );
            Ok(())
        })
        .unwrap();
    }
}
