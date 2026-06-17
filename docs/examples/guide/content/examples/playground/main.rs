/// Port of Python Textual `docs/examples/guide/content/playground.py`.
///
/// Demonstrates a markup playground with:
/// - A `TextArea` for entering markup text (editor, id="editor")
/// - A `TextArea` for JSON variables (id="variables", language="json") — F1 toggle
/// - A `VerticalScroll` showing rendered output as `Static` (id="results-container")
/// - A `VerticalScroll` showing span information via `Pretty` (id="spans-container") — F2 toggle
/// - `Footer` with F1/F2 binding hints
///
/// ## Implemented
/// - Markup rendering: `rich_rs::Text::from_markup(&text, false)` + `Static::update_rich(text)`
///   renders the editor markup into #results (on error, `-error` class applied to container).
/// - Span listing: parsed `Text::spans()` are formatted and shown via `Pretty::update_str`.
/// - JSON validation: naive bracket/brace balance check (see note below) toggles `-bad-json`.
/// - Border titles: `Static::with_border_title("Output")` and `Pretty::with_border_title("Spans")`
///   are set on #results and #spans respectively.
/// - Auto-focus: `app.query_mut("#editor").focus()` is called in `on_mount_with_app`.
///
/// ## Framework gaps (genuine blockers)
/// - `Content::from_markup()` with variable substitution: rich-rs offers `Text::from_markup`
///   (parsing + rendering) and `Span` (span list) but no unified Content type that performs
///   named variable substitution in one call (Python: `Content.from_markup(text, **vars)`).
///   The variables TextArea values are parsed but not yet threaded into the markup renderer.
/// - `TextArea` border_title: `TextArea` has no `with_border_title` / `set_border_title`
///   (confirmed absent in src/widgets/text_area.rs), so the editor's 'Markup' title and
///   variables' 'Variables (JSON)' title cannot be reproduced yet.
/// - JSON parsing (serde_json): `serde_json` is not declared in this crate's Cargo.toml
///   (it is available in the top-level workspace but not the docs/examples workspace).
///   A naive bracket-balance heuristic is used instead; it correctly flags obviously
///   malformed text and accepts well-formed objects/arrays.
/// - `@on(Message, "#selector")` declarative routing: Rust stores NodeIds at mount time
///   and compares `message.sender`. Functional but not declarative.
use rich_rs::Text;
use textual::prelude::*;

const CSS: &str = r##"
Screen {
    layout: vertical;
}

#editor {
    width: 1fr;
    height: 1fr;
    border: tab $foreground 50%;
    padding: 1;
    margin: 1 0 0 0;
}

#editor:focus {
    border: tab $primary;
}

#variables {
    width: 1fr;
    height: 1fr;
    border: tab $foreground 50%;
    padding: 1;
    margin: 1 0 0 1;
}

#variables:focus {
    border: tab $primary;
}

#variables.-bad-json {
    border: tab $error;
}

#results-container {
    border: tab $success;
    overflow-y: auto;
}

#results-container.-error {
    border: tab $error;
}

#results {
    padding: 1 1;
    width: 1fr;
}

#spans-container {
    border: tab $success;
    overflow-y: auto;
    margin: 0 0 0 1;
}

#spans {
    padding: 1 1;
    width: 1fr;
}

HorizontalGroup {
    height: 1fr;
}
"##;

struct PlaygroundApp {
    show_variables: bool,
    show_spans: bool,
    /// NodeId of the #editor TextArea, resolved at mount time.
    editor_id: Option<NodeId>,
    /// NodeId of the #variables TextArea, resolved at mount time.
    variables_id: Option<NodeId>,
}

impl PlaygroundApp {
    fn new() -> Self {
        Self {
            show_variables: true,
            show_spans: false,
            editor_id: None,
            variables_id: None,
        }
    }

    /// Naive JSON validity heuristic (used because serde_json is not in this
    /// crate's Cargo.toml).
    ///
    /// Returns `true` when the text is either empty (treated as an empty
    /// variables object) or appears to be a structurally balanced JSON object
    /// or array.  This correctly flags obviously malformed input and accepts
    /// well-formed JSON, but does NOT catch all semantic errors (e.g. duplicate
    /// keys or non-string keys without quotes).
    fn looks_like_valid_json(text: &str) -> bool {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return true;
        }
        // Must start and end with matching brackets.
        let (open, close) = match trimmed.chars().next() {
            Some('{') => ('{', '}'),
            Some('[') => ('[', ']'),
            _ => return false,
        };
        if trimmed.chars().last() != Some(close) {
            return false;
        }
        // Check bracket/brace balance across the whole string.
        let mut depth: i32 = 0;
        let mut in_string = false;
        let mut escape = false;
        for ch in trimmed.chars() {
            if escape {
                escape = false;
                continue;
            }
            if in_string {
                match ch {
                    '\\' => escape = true,
                    '"' => in_string = false,
                    _ => {}
                }
                continue;
            }
            match ch {
                '"' => in_string = true,
                c if c == open => depth += 1,
                c if c == close => {
                    depth -= 1;
                    if depth < 0 {
                        return false;
                    }
                }
                _ => {}
            }
        }
        depth == 0
    }

    /// Format a span list from a parsed `Text` into a human-readable string
    /// suitable for the `Pretty` widget.
    ///
    /// Mirrors Python's `content.spans` accessor which returns a list of
    /// `rich.text.Span` objects (each with start, end, style).
    fn format_spans(text: &Text) -> String {
        let spans = text.spans();
        if spans.is_empty() {
            return "[]".to_string();
        }
        let mut parts = Vec::with_capacity(spans.len());
        for span in spans {
            parts.push(format!(
                "Span({}, {}, {:?})",
                span.start,
                span.end,
                span.style
            ));
        }
        format!("[{}]", parts.join(", "))
    }

    /// Update the results panel from the current editor text.
    ///
    /// Parses the markup with `rich_rs::Text::from_markup`, renders it into
    /// `#results` via `Static::update_rich`, and populates `#spans` with the
    /// span list.  On parse error the `-error` class is added to
    /// `#results-container`.
    fn update_results(app: &mut App, ctx: &mut EventCtx) {
        // Read editor text.
        let text = app
            .with_query_one_mut_as::<TextArea, _>("#editor", |ta| ta.text())
            .ok()
            .unwrap_or_default();

        match Text::from_markup(&text, false) {
            Ok(rich_text) => {
                // Build span representation before consuming rich_text.
                let spans_str = Self::format_spans(&rich_text);

                // Render parsed markup into #results.
                let _ = app.with_query_one_mut_as::<Static, _>("#results", |s| {
                    s.update_rich(rich_text);
                });

                // Populate #spans with the span list.
                let _ = app.with_query_one_mut_as::<Pretty, _>("#spans", |p| {
                    p.update_str(spans_str);
                });

                // Clear error class — markup is valid.
                if let Ok(q) = app.query_mut("#results-container") {
                    q.remove_class("-error");
                }
            }
            Err(_) => {
                // Markup parse error: show raw text and mark container as errored.
                let _ = app.with_query_one_mut_as::<Static, _>("#results", |s| {
                    s.update(text.clone());
                });
                let _ = app.with_query_one_mut_as::<Pretty, _>("#spans", |p| {
                    p.update_str("[]".to_string());
                });
                if let Ok(q) = app.query_mut("#results-container") {
                    q.add_class("-error");
                }
            }
        }

        ctx.request_repaint();
    }
}

impl TextualApp for PlaygroundApp {
    fn title(&self) -> &'static str {
        "Markup Playground"
    }

    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("f1", "toggle_variables", "Variables"),
            BindingDecl::new("f2", "toggle_spans", "Spans"),
        ]
    }

    fn compose(&mut self) -> AppRoot {
        // Top row: editor + variables.
        let top_row = HorizontalGroup::new()
            .with_child(
                Node::new(TextArea::new("").with_soft_wrap(false)).id("editor"),
            )
            .with_child(
                Node::new(TextArea::new("").with_language("json")).id("variables"),
            );

        // Bottom row: results + spans.
        // Static gets border title "Output"; Pretty gets border title "Spans".
        // Note: TextArea has no with_border_title, so editor/variables titles
        // ('Markup' / 'Variables (JSON)') cannot be set until that API exists.
        let results_scroll = Node::new(
            VerticalScroll::new()
                .with_child(Static::new("").with_border_title("Output").id("results")),
        )
        .id("results-container");

        let spans_scroll = Node::new(
            VerticalScroll::new()
                .with_child(Pretty::from_debug_str("[]").with_border_title("Spans").id("spans")),
        )
        .id("spans-container");

        let bottom_row = HorizontalGroup::new()
            .with_child(results_scroll)
            .with_child(spans_scroll);

        AppRoot::new()
            .with_child(top_row)
            .with_child(bottom_row)
            .with_child(Footer::new())
    }

    fn on_mount_with_app(&mut self, app: &mut App, ctx: &mut EventCtx) {
        // Store NodeIds so we can distinguish editor vs variables messages later.
        self.editor_id = app.query_one("#editor").ok();
        self.variables_id = app.query_one("#variables").ok();

        // Auto-focus the editor (mirrors Python AUTO_FOCUS = "#editor").
        if let Ok(q) = app.query_mut("#editor") {
            q.focus();
        }

        // Apply initial visibility state.
        if let Ok(q) = app.query_mut("#variables") {
            q.set_display(self.show_variables);
        }
        if let Ok(q) = app.query_mut("#spans-container") {
            q.set_display(self.show_spans);
        }

        // Run initial markup update.
        Self::update_results(app, ctx);
    }

    fn on_message_with_app(
        &mut self,
        app: &mut App,
        message: &MessageEvent,
        ctx: &mut EventCtx,
    ) {
        if let Some(_changed) = message.downcast_ref::<TextAreaChanged>() {
            let sender = message.sender;

            if Some(sender) == self.editor_id {
                // Editor content changed → re-render markup results.
                Self::update_results(app, ctx);
            } else if Some(sender) == self.variables_id {
                // Variables JSON changed → validate JSON and toggle -bad-json class,
                // then re-render (variable substitution into markup is a confirmed
                // gap: Content::from_markup() with named variables does not exist in
                // rich-rs; full variable threading is deferred).
                let vars_text = app
                    .with_query_one_mut_as::<TextArea, _>("#variables", |ta| ta.text())
                    .ok()
                    .unwrap_or_default();

                let json_ok = Self::looks_like_valid_json(&vars_text);
                if let Ok(q) = app.query_mut("#variables") {
                    if json_ok {
                        q.remove_class("-bad-json");
                    } else {
                        q.add_class("-bad-json");
                    }
                }

                // Re-render results with the (un-substituted) markup.
                Self::update_results(app, ctx);
            }
        }
    }

    fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut EventCtx) {
        match action {
            "toggle_variables" => {
                self.show_variables = !self.show_variables;
                if let Ok(q) = app.query_mut("#variables") {
                    q.set_display(self.show_variables);
                }
                ctx.request_repaint();
            }
            "toggle_spans" => {
                self.show_spans = !self.show_spans;
                if let Ok(q) = app.query_mut("#spans-container") {
                    q.set_display(self.show_spans);
                }
                ctx.request_repaint();
            }
            _ => {}
        }
    }
}

fn main() -> Result<()> {
    run_sync(PlaygroundApp::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playground_composes_without_panic() {
        let mut app = PlaygroundApp::new();
        let _root = app.compose();
    }

    #[test]
    fn has_f1_f2_bindings() {
        let app = PlaygroundApp::new();
        let bindings = app.bindings();
        assert!(
            bindings.iter().any(|b| b.key == "f1"),
            "missing f1 binding"
        );
        assert!(
            bindings.iter().any(|b| b.key == "f2"),
            "missing f2 binding"
        );
    }

    #[test]
    fn initial_state() {
        let app = PlaygroundApp::new();
        assert!(app.show_variables, "variables panel should default to visible");
        assert!(!app.show_spans, "spans panel should default to hidden");
        assert!(app.editor_id.is_none(), "editor_id not set until mount");
    }

    #[test]
    fn json_heuristic_accepts_valid() {
        assert!(PlaygroundApp::looks_like_valid_json(""));
        assert!(PlaygroundApp::looks_like_valid_json("{}"));
        assert!(PlaygroundApp::looks_like_valid_json(r#"{"key": "value"}"#));
        assert!(PlaygroundApp::looks_like_valid_json(r#"{"a": 1, "b": [1, 2]}"#));
        assert!(PlaygroundApp::looks_like_valid_json("[]"));
    }

    #[test]
    fn json_heuristic_rejects_invalid() {
        assert!(!PlaygroundApp::looks_like_valid_json("not json"));
        assert!(!PlaygroundApp::looks_like_valid_json("{unclosed"));
        assert!(!PlaygroundApp::looks_like_valid_json(r#"{"key": "value""#));
    }

    #[test]
    fn format_spans_empty() {
        let text = Text::plain("hello");
        let result = PlaygroundApp::format_spans(&text);
        assert_eq!(result, "[]");
    }

    #[test]
    fn markup_parses_plain_text() {
        let text = Text::from_markup("Hello World", false).unwrap();
        assert_eq!(text.plain_text(), "Hello World");
    }
}
