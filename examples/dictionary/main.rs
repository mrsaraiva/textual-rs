/// Port of Python Textual `examples/dictionary.py`.
///
/// A word search app that demonstrates workers with async UI updates:
/// - Input field (docked to top) for word lookup.
/// - On every keystroke, an exclusive worker "fetches" a definition.
/// - Results appear in a scrollable Markdown area below.
///
/// Python: `@work(exclusive=True) async def lookup_word(word)` fetches from
/// `https://api.dictionaryapi.dev/api/v2/entries/en/{word}` and calls
/// `self.results.update(markdown)`.
///
/// Rust: `ctx.request_exclusive_worker_task("lookup_word", ...)` with a simulated
/// lookup producing deterministic Markdown output. The worker hands the result
/// back via the message hook, which updates the `Markdown` widget directly —
/// matching Python's `results = query_one("#results", Markdown)` getter +
/// imperative `self.results.update(...)` (reactive state is not the right idiom
/// here; the result pane is an imperative widget API in Python too).
///
/// DEFERRED: Real HTTP lookup — requires a blocking HTTP client (e.g. `reqwest` with the
/// `blocking` feature). Simulated here with a short delay and built-in word list.
use std::sync::{Arc, Mutex};
use textual::prelude::*;

const CSS: &str = r#"
#dictionary-search {
    dock: top;
    margin: 1 0;
    width: 100%;
}

#results-container {
    width: 100%;
    height: 1fr;
    background: $surface;
}

#results {
    width: 100%;
    height: auto;
}
"#;

// ---------------------------------------------------------------------------
// Simulated dictionary data (replaces the real API response).
// ---------------------------------------------------------------------------

fn make_word_markdown(word: &str) -> String {
    // A small built-in word list for demo purposes.
    let entries: &[(&str, &str, &[&str])] = &[
        (
            "hello",
            "exclamation",
            &[
                "Used as a greeting or to begin a telephone conversation.",
                "An expression of surprise.",
            ],
        ),
        (
            "world",
            "noun",
            &[
                "The earth, together with all of its countries and peoples.",
                "A particular region or group of countries.",
            ],
        ),
        (
            "rust",
            "noun",
            &[
                "A reddish-brown flaky coating of iron oxide formed on iron or steel by oxidation.",
                "A programming language focused on safety, speed, and concurrency.",
            ],
        ),
        (
            "textual",
            "adjective",
            &["Of or relating to a text or texts."],
        ),
        (
            "python",
            "noun",
            &[
                "A large heavy-bodied nonvenomous snake.",
                "A high-level general-purpose programming language.",
            ],
        ),
    ];

    let lower = word.to_lowercase();
    let found = entries.iter().find(|(w, _, _)| *w == lower.as_str());

    match found {
        None => format!(
            "# No results for \"{word}\"\n\nNo definition found in the built-in word list.\n\n\
             *(Real port would query https://api.dictionaryapi.dev/api/v2/entries/en/{word})*"
        ),
        Some((w, part_of_speech, definitions)) => {
            let mut lines = vec![
                format!("# {w}"),
                String::new(),
                format!("_{part_of_speech}_"),
                String::new(),
            ];
            for def in *definitions {
                lines.push(format!(" - {def}"));
            }
            lines.push("---".to_string());
            lines.join("\n")
        }
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

struct DictionaryApp {
    /// Shared result buffer between the app and the background worker thread.
    lookup_result: Arc<Mutex<Option<String>>>,
    /// Post-mount typed handle to the Markdown results widget (nested in Node > ScrollView).
    results: Option<Handle<Markdown>>,
}

impl DictionaryApp {
    fn new() -> Self {
        Self {
            lookup_result: Arc::new(Mutex::new(None)),
            results: None,
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
            .with_child(
                Node::new(Input::new().with_placeholder("Search for a word"))
                    .id("dictionary-search"),
            )
            .with_child(
                Node::new(ScrollView::new(Markdown::new("").with_id("results")))
                    .id("results-container"),
            )
    }

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut EventCtx) {
        // The Markdown results widget is nested (Node > ScrollView > Markdown),
        // so use post-mount query_one_typed rather than with_child_handle.
        self.results = app.query_one_typed::<Markdown>("#results").ok();
    }

    fn on_input_changed(
        &mut self,
        value: &str,
        _validation: &ValidationResult,
        ctx: &mut EventCtx,
    ) {
        let word = value.trim().to_string();
        let result_holder = Arc::clone(&self.lookup_result);

        if word.is_empty() {
            // Clear results immediately when the input is empty.
            *result_holder.lock().unwrap_or_else(|e| e.into_inner()) = Some(String::new());
            // Post a synthetic "clear" — reuse the worker result path.
            ctx.request_exclusive_worker_task("lookup_word", Some("clear"), move |_token| Ok(()));
        } else {
            // @work(exclusive=True) semantics: cancel any previous in-flight lookup.
            ctx.request_exclusive_worker_task("lookup_word", Some("lookup"), move |token| {
                // Simulate network latency.
                std::thread::sleep(std::time::Duration::from_millis(80));
                if token.is_cancelled() {
                    return Ok(());
                }

                let markdown = make_word_markdown(&word);
                *result_holder.lock().unwrap_or_else(|e| e.into_inner()) = Some(markdown);
                Ok(())
            });
        }

        ctx.request_repaint();
    }

    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, _ctx: &mut EventCtx) {
        if let Some(w) = message.downcast_ref::<WorkerStateChanged>() {
            if matches!(w.state, WorkerState::Success) {
                let markdown =
                    { self.lookup_result.lock().unwrap_or_else(|e| e.into_inner()).take() };
                if let Some(markdown) = markdown {
                    // Python: `self.results.update(markdown)` on the queried widget.
                    if let Some(h) = self.results {
                        let _ = h.update(app, |w, _ctx| {
                            w.set_markup(markdown.clone());
                        });
                    }
                }
            }
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(DictionaryApp::new())
}

// ---------------------------------------------------------------------------
// Regression tests (DG-02)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_app_composes_without_panic() {
        let mut app = DictionaryApp::new();
        let _root = app.compose();
    }

    #[test]
    fn initial_lookup_result_is_none() {
        let app = DictionaryApp::new();
        let guard = app.lookup_result.lock().unwrap();
        assert!(guard.is_none());
    }

    #[test]
    fn known_word_produces_markdown_with_definition() {
        let md = make_word_markdown("rust");
        assert!(md.contains("# rust"), "expected heading");
        assert!(md.contains("noun"), "expected part of speech");
        assert!(
            md.contains("oxidation") || md.contains("programming"),
            "expected definition"
        );
    }

    #[test]
    fn unknown_word_produces_no_results_markdown() {
        let md = make_word_markdown("xyzzy");
        assert!(md.contains("No results"), "expected no-results message");
        assert!(md.contains("xyzzy"), "expected word echoed in output");
    }
}
