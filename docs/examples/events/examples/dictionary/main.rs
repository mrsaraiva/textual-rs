/// Port of Python Textual `docs/examples/events/dictionary.py`.
///
/// Demonstrates async-style input-driven search:
/// - Input docked at top; user types a word.
/// - On every keystroke a "lookup" is triggered.
/// - Results (simulated JSON-like text) are shown in a scrollable area.
///
/// Python uses `httpx` to query <https://api.dictionaryapi.dev>. The Rust port
/// simulates the lookup with a short worker thread delay and a fabricated
/// response. The layout and CSS are faithful ports of `dictionary.tcss`.
///
/// NON-PROMOTABLE: The initial screen is empty (results appear only after
/// typing), so plain-text PTY parity cannot be scored on the initial frame.
use std::sync::{Arc, Mutex};
use textual::prelude::*;

const CSS: &str = r#"
Screen {
    background: $panel;
}

Input {
    dock: top;
    width: 100%;
    height: 1;
    padding: 0 1;
    margin: 1 1 0 1;
}

#results {
    width: auto;
    min-height: 100%;
}

#results-container {
    background: $background;
    overflow-y: auto;
    margin: 1 2;
    height: 100%;
}
"#;

struct DictionaryApp {
    result: Arc<Mutex<Option<String>>>,
    current_word: Arc<Mutex<String>>,
}

impl DictionaryApp {
    fn new() -> Self {
        Self {
            result: Arc::new(Mutex::new(None)),
            current_word: Arc::new(Mutex::new(String::new())),
        }
    }
}

impl TextualApp for DictionaryApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(Input::new().with_placeholder("Search for a word"))
            .with_child(
                VerticalScroll::new().with_child(Static::new("")),
            )
    }

    fn on_input_changed(
        &mut self,
        value: &str,
        _validation: &ValidationResult,
        ctx: &mut EventCtx,
    ) {
        let word = value.trim().to_string();
        *self.current_word.lock().unwrap() = word.clone();
        let result_holder = Arc::clone(&self.result);
        let current_word = Arc::clone(&self.current_word);

        if word.is_empty() {
            *result_holder.lock().unwrap() = Some(String::new());
        } else {
            ctx.request_exclusive_worker_task("dict-lookup", Some("lookup"), move |token| {
                // Simulate network latency
                std::thread::sleep(std::time::Duration::from_millis(50));
                if token.is_cancelled() {
                    return Ok(());
                }
                // Only write if the word hasn't changed
                let current = current_word.lock().unwrap().clone();
                if current == word {
                    let json = simulate_dictionary_response(&word);
                    *result_holder.lock().unwrap() = Some(json);
                }
                Ok(())
            });
        }
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
                let text = {
                    let mut guard = self.result.lock().unwrap();
                    guard.take()
                };
                let _ = app.with_query_one_mut_as::<Static, _>("Static", |s| {
                    match text {
                        Some(t) => s.update(t),
                        None => s.clear(),
                    }
                });
                ctx.request_repaint();
            }
        }
    }
}

fn simulate_dictionary_response(word: &str) -> String {
    format!(
        r#"[{{"word": "{word}", "phonetic": "/{word}/", "meanings": [{{"partOfSpeech": "noun", "definitions": [{{"definition": "A simulated definition for '{word}'."}}]}}]}}]"#,
    )
}

fn main() -> textual::Result<()> {
    run_sync(DictionaryApp::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_app_composes_without_panic() {
        let mut app = DictionaryApp::new();
        let _root = app.compose();
    }

    #[test]
    fn simulate_response_contains_word() {
        let r = simulate_dictionary_response("hello");
        assert!(r.contains("hello"));
    }

    /// LIVENESS probe (Pilot, headless): focus the search `Input` and type a
    /// word. The keystrokes echo into the input (frame + value change) and drive
    /// `on_input_changed`, which records the current word (shared state) — the
    /// trigger for the lookup. This proves the input-driven search front-end is
    /// live; the asynchronous lookup itself is covered separately (see below).
    #[test]
    fn dictionary_typing_drives_lookup_input_is_live() {
        fn input_value(app: &App) -> Option<String> {
            app.query_one_typed::<Input>("Input")
                .ok()
                .and_then(|h| h.read(app, |i| i.value().to_string()).ok())
        }
        let app = DictionaryApp::new();
        let word = app.current_word.clone();
        run_test(app, |pilot| {
            pilot.click("Input")?; // focus
            assert_eq!(input_value(pilot.app()).as_deref(), Some(""), "input starts empty");
            pilot.press(&["c", "a", "t"])?;
            assert_eq!(input_value(pilot.app()).as_deref(), Some("cat"), "typing must echo into the input");
            // on_input_changed recorded the current word -> the lookup trigger fired.
            assert_eq!(word.lock().unwrap().as_str(), "cat", "on_input_changed must record the searched word");
            Ok(())
        })
        .expect("dictionary input harness should run");
    }

    /// LIVENESS probe (Pilot, headless) for the full lookup → result render.
    ///
    /// UNCLEAR under the headless harness — `#[ignore]`d. ROOT: the actual
    /// dictionary lookup runs in a background *worker thread*
    /// (`ctx.request_exclusive_worker_task(...)`) that sleeps to simulate network
    /// latency, then posts the result back. The headless pump
    /// (`runtime/event_loop.rs:4269`) never spawns worker requests
    /// (`process_worker_requests` runs only in the live loop), so the lookup
    /// never executes headless and the results pane is never populated. This is a
    /// harness limitation, not a demo defect (the input front-end above is live).
    /// TODO: drive worker spawning from the headless pump (or add a Pilot
    /// worker-step), then assert the results render; drop `#[ignore]`.
    #[ignore = "UNCLEAR: headless pump does not spawn the worker thread that performs the lookup"]
    #[test]
    fn dictionary_lookup_result_is_live() {
        run_test(DictionaryApp::new(), |pilot| {
            pilot.click("Input")?;
            let before = pilot.app().frame_fingerprint();
            pilot.press(&["c", "a", "t"])?;
            pilot.pause()?; // would let the worker lookup land + render results
            assert_ne!(before, pilot.app().frame_fingerprint(), "the lookup result must render");
            Ok(())
        })
        .expect("dictionary lookup harness should run");
    }
}
