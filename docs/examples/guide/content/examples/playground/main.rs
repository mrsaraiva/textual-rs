/// Port of Python Textual `docs/examples/guide/content/playground.py`.
///
/// Demonstrates a markup playground with:
/// - A `TextArea` for entering markup text (editor, id="editor")
/// - A `TextArea` for JSON variables (id="variables", language="json") — F1 toggle
/// - A `VerticalScroll` showing rendered output as `Static` (id="results-container")
/// - A `VerticalScroll` showing span information via `Pretty` (id="spans-container") — F2 toggle
/// - `Footer` with F1/F2 binding hints
///
/// ## Template-variable substitution (the playground's core feature)
/// Python's `MarkupPlayground` is built around
/// `Content.from_markup(text, **variables)`, where the variables come from the
/// JSON editor and are substituted into the markup via `string.Template`'s
/// `safe_substitute` (over `$name` / `${name}`) **before** tag parsing.
///
/// This Rust port now reproduces that faithfully:
/// - The variables `TextArea` is parsed as JSON (`serde_json`) into a string map.
/// - `Content::from_markup_with_vars(text, &vars)` performs the same
///   `safe_substitute` over **text tokens only** (tag bodies like `[$primary]`
///   are left intact), exactly like Python.
/// - The resulting `Content` (with variables already substituted) is rendered by
///   `Static::update_content(content)` — a value that carries its own spans, so a
///   variable value containing literal `[red]` is shown verbatim, not re-parsed
///   as markup (matching Python).
/// - The `Content.spans` list is shown in the `#spans` `Pretty` panel.
///
/// ## Remaining smaller gaps (not core to this feature)
/// - `TextArea` border_title: `TextArea` has no `with_border_title` /
///   `set_border_title`, so the editor's 'Markup' and variables' 'Variables (JSON)'
///   titles are not reproduced yet.
/// - `@on(Message, "#selector")` declarative routing: Rust stores NodeIds at mount
///   time and compares `message.sender`. Functional but not declarative.
use std::collections::HashMap;

use textual::content::{Content, SpanStyle};
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
    /// Current template variables (last successfully-parsed JSON object).
    /// Mirrors Python `MarkupPlayground.variables` (a reactive dict).
    variables: HashMap<String, String>,
}

impl PlaygroundApp {
    fn new() -> Self {
        Self {
            show_variables: true,
            show_spans: false,
            editor_id: None,
            variables_id: None,
            variables: HashMap::new(),
        }
    }

    /// Parse the JSON variables text into a `{name: value-string}` map.
    ///
    /// Mirrors Python `json.loads(text)` followed by `**variables` expansion:
    /// each top-level key becomes a template variable. JSON values are converted
    /// to the string form that `safe_substitute` would interpolate (string values
    /// use their raw contents; other JSON scalars/containers use their JSON text).
    ///
    /// Returns `Err` when the text is not a valid JSON object (Python flags this
    /// by adding the `-bad-json` class and clearing the variables).
    fn parse_variables(text: &str) -> std::result::Result<HashMap<String, String>, ()> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            // Empty editor → empty variables (Python's reactive default is `{}`).
            return Ok(HashMap::new());
        }
        let value: serde_json::Value = serde_json::from_str(trimmed).map_err(|_| ())?;
        let serde_json::Value::Object(map) = value else {
            // Python passes the parsed object via `**variables`; a non-object
            // (array / scalar) cannot be expanded as keyword arguments.
            return Err(());
        };
        let mut vars = HashMap::with_capacity(map.len());
        for (key, val) in map {
            let s = match val {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            };
            vars.insert(key, s);
        }
        Ok(vars)
    }

    /// Format the `Content.spans` list into a human-readable string for the
    /// `Pretty` widget, mirroring Python's `content.spans` (a list of
    /// `Span(start, end, style)` repr).
    fn format_spans(content: &Content) -> String {
        let spans = content.spans();
        if spans.is_empty() {
            return "[]".to_string();
        }
        let mut parts = Vec::with_capacity(spans.len());
        for span in spans {
            let style_repr = match &span.span_style {
                SpanStyle::Raw(raw) => format!("{raw:?}"),
                SpanStyle::Parsed(style) => format!("{style:?}"),
            };
            parts.push(format!(
                "Span({}, {}, {})",
                span.start, span.end, style_repr
            ));
        }
        format!("[{}]", parts.join(", "))
    }

    /// Update the results panel from the current editor text + variables.
    ///
    /// Mirrors Python `MarkupPlayground.update_markup`:
    /// `content = Content.from_markup(editor.text, **self.variables)` then
    /// `results.update(content)` and `spans.update(content.spans)`.
    fn update_results(app: &mut App, ctx: &mut textual::event::WidgetCtx, variables: &HashMap<String, String>) {
        let text = app
            .with_query_one_mut_as::<TextArea, _>("#editor", |ta| ta.text())
            .ok()
            .unwrap_or_default();

        // Build Content with template-variable substitution. Substitution applies
        // to text tokens only; tag bodies (`[$primary]`) are left intact, and a
        // variable value containing literal brackets is NOT re-parsed as markup.
        let content = Content::from_markup_with_vars(&text, variables);
        let spans_str = Self::format_spans(&content);

        let _ = app.with_query_one_mut_as::<Static, _>("#results", |s| {
            s.update_content(content.clone());
        });
        let _ = app.with_query_one_mut_as::<Pretty, _>("#spans", |p| {
            p.update_str(spans_str);
        });

        // Our markup parser is total (never errors), matching Textual's behaviour
        // for well-formed-enough input; clear any error class.
        if let Ok(q) = app.query_mut("#results-container") {
            q.remove_class("-error");
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
            .with_child(Node::new(TextArea::new("").with_soft_wrap(false)).id("editor"))
            .with_child(Node::new(TextArea::new("").with_language("json")).id("variables"));

        // Bottom row: results + spans.
        let results_scroll = Node::new(
            VerticalScroll::new()
                .with_child(Static::new("").with_border_title("Output").id("results")),
        )
        .id("results-container");

        let spans_scroll = Node::new(
            VerticalScroll::new().with_child(
                Pretty::from_debug_str("[]")
                    .with_border_title("Spans")
                    .id("spans"),
            ),
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

    fn on_mount_with_app(&mut self, app: &mut App, ctx: &mut textual::event::WidgetCtx) {
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
        let vars = self.variables.clone();
        Self::update_results(app, ctx, &vars);
    }

    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut textual::event::WidgetCtx) {
        if message.downcast_ref::<TextAreaChanged>().is_some() {
            let sender = message.sender;

            if Some(sender) == self.editor_id {
                // Editor content changed → re-render markup results.
                let vars = self.variables.clone();
                Self::update_results(app, ctx, &vars);
            } else if Some(sender) == self.variables_id {
                // Variables JSON changed → parse JSON, toggle -bad-json class,
                // update the stored variables, then re-render with substitution.
                let vars_text = app
                    .with_query_one_mut_as::<TextArea, _>("#variables", |ta| ta.text())
                    .ok()
                    .unwrap_or_default();

                match Self::parse_variables(&vars_text) {
                    Ok(vars) => {
                        if let Ok(q) = app.query_mut("#variables") {
                            q.remove_class("-bad-json");
                        }
                        self.variables = vars;
                    }
                    Err(()) => {
                        if let Ok(q) = app.query_mut("#variables") {
                            q.add_class("-bad-json");
                        }
                        // Python sets `self.variables = {}` on bad JSON.
                        self.variables = HashMap::new();
                    }
                }

                let vars = self.variables.clone();
                Self::update_results(app, ctx, &vars);
            }
        }
    }

    fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut textual::event::WidgetCtx) {
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
        assert!(bindings.iter().any(|b| b.key == "f1"), "missing f1 binding");
        assert!(bindings.iter().any(|b| b.key == "f2"), "missing f2 binding");
    }

    #[test]
    fn initial_state() {
        let app = PlaygroundApp::new();
        assert!(
            app.show_variables,
            "variables panel should default to visible"
        );
        assert!(!app.show_spans, "spans panel should default to hidden");
        assert!(app.editor_id.is_none(), "editor_id not set until mount");
        assert!(app.variables.is_empty(), "variables default empty");
    }

    #[test]
    fn parse_variables_accepts_object() {
        let vars = PlaygroundApp::parse_variables(r#"{"name": "Will", "age": 42}"#).unwrap();
        assert_eq!(vars.get("name").map(String::as_str), Some("Will"));
        // Non-string JSON values are stringified (Python str()-equivalent here is
        // the JSON scalar text).
        assert_eq!(vars.get("age").map(String::as_str), Some("42"));
    }

    #[test]
    fn parse_variables_empty_is_ok() {
        assert!(PlaygroundApp::parse_variables("").unwrap().is_empty());
        assert!(PlaygroundApp::parse_variables("   ").unwrap().is_empty());
    }

    #[test]
    fn parse_variables_rejects_invalid_and_non_objects() {
        assert!(PlaygroundApp::parse_variables("not json").is_err());
        assert!(PlaygroundApp::parse_variables("{unclosed").is_err());
        assert!(PlaygroundApp::parse_variables("[1, 2, 3]").is_err());
        assert!(PlaygroundApp::parse_variables("42").is_err());
    }

    /// The core feature: `$name` in the markup text is replaced by the variable,
    /// faithfully mirroring Python `Content.from_markup("Hello, [b]$name[/b]!", name="Will")`.
    #[test]
    fn from_markup_with_vars_substitutes_in_text() {
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "Will".to_string());
        let content = Content::from_markup_with_vars("Hello, [b]$name[/b]!", &vars);
        assert_eq!(content.plain(), "Hello, Will!");
        // The `[b]` tag span survives substitution and covers the substituted name.
        assert_eq!(content.spans().len(), 1);
        assert_eq!(content.spans()[0].start, 7);
        assert_eq!(content.spans()[0].end, 11); // "Will"
    }

    /// Faithful to Python: a variable VALUE containing markup-like brackets is
    /// inserted as literal text, NOT re-parsed as a tag.
    #[test]
    fn variable_value_with_brackets_is_literal() {
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), "[red]BIG[/red]".to_string());
        let content = Content::from_markup_with_vars("Hello $x world", &vars);
        assert_eq!(content.plain(), "Hello [red]BIG[/red] world");
        assert!(
            content.spans().is_empty(),
            "value brackets must not become spans"
        );
    }

    /// Tag bodies are NOT substituted (only text tokens), matching Python.
    #[test]
    fn tag_bodies_are_not_substituted() {
        let mut vars = HashMap::new();
        vars.insert("primary".to_string(), "red".to_string());
        // `[$primary]` is a tag body → left intact; only the text `$primary` is subbed.
        let content = Content::from_markup_with_vars("[$primary]$primary[/]", &vars);
        assert_eq!(content.plain(), "red");
        assert_eq!(content.spans().len(), 1);
        // Raw tag body remains `$primary` (deferred theme-token resolution).
        match &content.spans()[0].span_style {
            SpanStyle::Raw(raw) => assert_eq!(raw, "$primary"),
            other => panic!("expected raw span, got {other:?}"),
        }
    }
}
