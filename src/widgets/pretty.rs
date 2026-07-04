use std::fmt::Debug;
use std::sync::{Arc, Mutex};

use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use super::{NodeSeed, Widget, helpers::adjust_line_length_no_bg};

/// Internal data source for Pretty.
#[derive(Clone)]
enum PrettySource {
    /// Static debug string captured at construction.
    Static(String),
    /// Shared debug string for live updates.
    Shared(Arc<Mutex<String>>),
}

impl PrettySource {
    fn read(&self) -> String {
        match self {
            PrettySource::Static(s) => s.clone(),
            PrettySource::Shared(s) => s.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        }
    }
}

/// A widget that pretty-prints Rust data structures.
///
/// `Pretty` is a thin wrapper around [`rich_rs::pretty::Pretty`] that integrates
/// pretty-printing into the widget tree. It accepts any type implementing `Debug`,
/// renders it with proper indentation, line wrapping, and syntax highlighting.
///
/// For live updates, use [`Pretty::shared`] with an `Arc<Mutex<String>>` — the
/// widget reads from the mutex on each render, so external code can update the
/// displayed value without needing mutable access to the widget.
///
/// # Example
///
/// ```rust
/// use textual::prelude::*;
///
/// let data = vec![1, 2, 3, 4, 5];
/// let pretty = Pretty::new(&data);
/// ```
///
/// # Default CSS
///
/// ```css
/// Pretty { fg: $foreground; }
/// ```
#[derive(Clone)]
pub struct Pretty {
    source: PrettySource,
    layout_width: usize,
    seed: NodeSeed,
    border_title_text: Option<String>,
}

impl Pretty {
    crate::seed_ident_methods!();

    /// Create a new `Pretty` widget from any `Debug` value.
    ///
    /// The value's `Debug` representation is captured at construction time.
    pub fn new<T: Debug>(value: &T) -> Self {
        Self {
            source: PrettySource::Static(format!("{:?}", value)),
            layout_width: 1,
            seed: NodeSeed::default(),
            border_title_text: None,
        }
    }

    /// Create a `Pretty` widget from a pre-formatted debug string.
    pub fn from_debug_str(debug_str: impl Into<String>) -> Self {
        Self {
            source: PrettySource::Static(debug_str.into()),
            layout_width: 1,
            seed: NodeSeed::default(),
            border_title_text: None,
        }
    }

    /// Create a `Pretty` widget backed by a shared debug string.
    ///
    /// The widget reads from the mutex on each render, so external code can
    /// update the displayed value by writing to the mutex and requesting a
    /// repaint.
    pub fn shared(debug_str: Arc<Mutex<String>>) -> Self {
        Self {
            source: PrettySource::Shared(debug_str),
            layout_width: 1,
            seed: NodeSeed::default(),
            border_title_text: None,
        }
    }

    /// Set a border title for this widget.
    pub fn with_border_title(mut self, title: impl Into<String>) -> Self {
        self.border_title_text = Some(title.into());
        self
    }

    /// Update the displayed value.
    ///
    /// For shared sources, writes the new debug string to the shared mutex.
    /// For static sources, replaces the stored string directly.
    pub fn update<T: Debug>(&mut self, value: &T) {
        let s = format!("{:?}", value);
        match &self.source {
            PrettySource::Shared(arc) => {
                *arc.lock().unwrap_or_else(|e| e.into_inner()) = s;
            }
            PrettySource::Static(_) => {
                self.source = PrettySource::Static(s);
            }
        }
    }

    /// Update the displayed value from a raw debug string.
    ///
    /// For shared sources, writes the new string to the shared mutex.
    /// For static sources, replaces the stored string directly.
    pub fn update_str(&mut self, debug_str: impl Into<String>) {
        let s = debug_str.into();
        match &self.source {
            PrettySource::Shared(arc) => {
                *arc.lock().unwrap_or_else(|e| e.into_inner()) = s;
            }
            PrettySource::Static(_) => {
                self.source = PrettySource::Static(s);
            }
        }
    }

    /// Get the current debug string.
    ///
    /// String literals are rendered Python-`repr` style (single quotes) by the
    /// pretty printer in `rich-rs` (>= 1.1.1), so no quote normalization is
    /// needed here.
    fn debug_str(&self) -> String {
        self.source.read()
    }

    /// Build a `rich_rs::pretty::Pretty` renderable for the current state.
    fn rich_pretty(&self) -> rich_rs::pretty::Pretty {
        rich_rs::pretty::Pretty::from_str(self.debug_str())
    }
}

impl Debug for Pretty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pretty")
            .field("debug_str", &self.debug_str())
            .finish()
    }
}

impl Widget for Pretty {
    fn border_title(&self) -> Option<&str> {
        self.border_title_text.as_deref()
    }

    fn style_type(&self) -> &'static str {
        "Pretty"
    }

    fn on_layout(&mut self, width: u16, _height: u16) {
        self.layout_width = usize::from(width).max(1);
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);

        // Delegate to rich_rs::Pretty for rendering (syntax highlighting, indentation, etc.)
        let rich = self.rich_pretty();
        let mut render_opts = options.clone();
        render_opts.max_width = width;

        let segments = rich_rs::Renderable::render(&rich, console, &render_opts);

        // Collect into lines for width adjustment
        let raw: Vec<Segment> = segments.into_iter().collect();
        let lines =
            Segment::split_and_crop_lines(Segments::from_iter(raw), width, None, true, false);

        let mut out = Segments::new();
        let line_count = lines.len();
        for (idx, line) in lines.into_iter().enumerate() {
            out.extend(adjust_line_length_no_bg(&line, width));
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn content_width(&self) -> Option<usize> {
        let debug_str = self.debug_str();
        if debug_str.is_empty() {
            return Some(1);
        }
        // Measure via rich_rs::Pretty
        let console = Console::new();
        let options = ConsoleOptions::default();
        let rich = rich_rs::pretty::Pretty::from_str(debug_str);
        let measurement = rich_rs::Renderable::measure(&rich, &console, &options);
        Some(measurement.maximum.max(1))
    }

    fn layout_height(&self) -> Option<usize> {
        // PURE content height (line count). The flow layout adds the CSS-resolved
        // vertical chrome (e.g. `Pretty { border: solid }`) with ancestor context,
        // symmetric with the width axis.
        let debug_str = self.debug_str();
        let content_lines = if debug_str.is_empty() {
            1
        } else {
            let text = rich_rs::pretty::pretty_repr(
                &debug_str,
                self.layout_width,
                4,
                None,
                None,
                None,
                false,
            );
            text.lines().count().max(1)
        };
        Some(content_lines)
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

impl Renderable for Pretty {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pretty_new_captures_debug() {
        let data = vec![1, 2, 3];
        let pretty = Pretty::new(&data);
        assert_eq!(pretty.debug_str(), "[1, 2, 3]");
    }

    #[test]
    fn pretty_from_debug_str() {
        let pretty = Pretty::from_debug_str("hello world");
        assert_eq!(pretty.debug_str(), "hello world");
    }

    #[test]
    fn pretty_shared() {
        let shared = Arc::new(Mutex::new("[1, 2]".to_string()));
        let pretty = Pretty::shared(shared.clone());
        assert_eq!(pretty.debug_str(), "[1, 2]");

        // Update shared data
        *shared.lock().unwrap() = "[3, 4, 5]".to_string();
        assert_eq!(pretty.debug_str(), "[3, 4, 5]");
    }

    #[test]
    fn pretty_update() {
        let mut pretty = Pretty::new(&42);
        assert_eq!(pretty.debug_str(), "42");
        pretty.update(&"hello");
        // `debug_str()` stores the raw Rust `Debug` output; the pretty printer
        // (rich-rs >= 1.1.1) converts strings to Python `repr` single quotes at
        // render time, so the render path is what shows `'hello'`.
        assert_eq!(pretty.debug_str(), "\"hello\"");
    }

    #[test]
    fn pretty_update_str() {
        let mut pretty = Pretty::new(&42);
        pretty.update_str("[updated]");
        assert_eq!(pretty.debug_str(), "[updated]");
    }

    #[test]
    fn pretty_empty_struct() {
        #[derive(Debug)]
        struct Empty;
        let pretty = Pretty::new(&Empty);
        assert_eq!(pretty.debug_str(), "Empty");
        assert_eq!(pretty.content_width(), Some(5));
        assert_eq!(pretty.layout_height(), Some(1));
    }

    #[test]
    fn pretty_simple_struct() {
        #[derive(Debug)]
        #[allow(dead_code)]
        struct Point {
            x: i32,
            y: i32,
        }
        let p = Point { x: 10, y: 20 };
        let pretty = Pretty::new(&p);
        assert!(pretty.debug_str().contains("Point"));
        assert!(pretty.debug_str().contains("x: 10"));
        assert!(pretty.debug_str().contains("y: 20"));
    }

    #[test]
    fn pretty_nested_struct() {
        #[derive(Debug)]
        #[allow(dead_code)]
        struct Inner {
            val: i32,
        }
        #[derive(Debug)]
        #[allow(dead_code)]
        struct Outer {
            name: String,
            inner: Inner,
        }
        let data = Outer {
            name: "test".to_string(),
            inner: Inner { val: 42 },
        };
        let pretty = Pretty::new(&data);
        assert!(pretty.debug_str().contains("Outer"));
        assert!(pretty.debug_str().contains("Inner"));
        assert!(pretty.debug_str().contains("42"));
    }

    #[test]
    fn pretty_content_width_non_empty() {
        let data = vec![1, 2, 3];
        let pretty = Pretty::new(&data);
        let w = pretty.content_width().unwrap();
        assert!(w >= 9); // "[1, 2, 3]" is 9 chars
    }

    #[test]
    fn pretty_layout_height_single_line() {
        let data = vec![1, 2, 3];
        let mut pretty = Pretty::new(&data);
        pretty.on_layout(80, 10);
        assert_eq!(pretty.layout_height(), Some(1));
    }

    #[test]
    fn pretty_layout_height_multiline() {
        let data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let mut pretty = Pretty::new(&data);
        pretty.on_layout(10, 20);
        let h = pretty.layout_height().unwrap();
        assert!(h > 1);
    }

    #[test]
    fn pretty_style_type() {
        let pretty = Pretty::new(&42);
        assert_eq!(pretty.style_type(), "Pretty");
    }

    #[test]
    fn pretty_debug_impl() {
        let pretty = Pretty::new(&vec![1, 2]);
        let dbg = format!("{:?}", pretty);
        assert!(dbg.contains("Pretty"));
        assert!(dbg.contains("[1, 2]"));
    }

    #[test]
    fn pretty_empty_value() {
        let data: Vec<i32> = vec![];
        let pretty = Pretty::new(&data);
        assert_eq!(pretty.debug_str(), "[]");
        assert_eq!(pretty.layout_height(), Some(1));
    }

    #[test]
    fn pretty_update_on_shared_preserves_sharing() {
        let shared = Arc::new(Mutex::new("[1]".to_string()));
        let mut pretty = Pretty::shared(shared.clone());
        assert_eq!(pretty.debug_str(), "[1]");

        // update() should write to the shared mutex, not replace with Static
        pretty.update(&vec![2, 3]);
        assert_eq!(pretty.debug_str(), "[2, 3]");
        // External readers should also see the update
        assert_eq!(*shared.lock().unwrap(), "[2, 3]");

        // External update should still work (sharing preserved)
        *shared.lock().unwrap() = "[99]".to_string();
        assert_eq!(pretty.debug_str(), "[99]");
    }

    #[test]
    fn pretty_update_str_on_shared_preserves_sharing() {
        let shared = Arc::new(Mutex::new("old".to_string()));
        let mut pretty = Pretty::shared(shared.clone());

        pretty.update_str("new");
        assert_eq!(*shared.lock().unwrap(), "new");
        assert_eq!(pretty.debug_str(), "new");
    }
}
