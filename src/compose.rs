//! Compose API types for declarative widget tree construction.
//!
//! Widgets declare their children via `compose()` → `ComposeResult`. Each entry
//! is a lightweight `ChildDecl` descriptor — **not** a live widget. The runtime
//! materializes declarations into arena nodes during mount (P1-05).
//!
//! # Example
//!
//! ```ignore
//! fn compose(&mut self) -> ComposeResult {
//!     compose![
//!         Header::new(),
//!         Button::new("Click me").with_id("btn"),
//!     ]
//! }
//! ```

use crate::widgets::Widget;

// ---------------------------------------------------------------------------
// ComposeResult
// ---------------------------------------------------------------------------

/// The return type of a widget's `compose()` method.
///
/// A `Vec` of child declarations that the runtime materializes into arena
/// nodes. Order is preserved: the first element becomes the first child.
pub type ComposeResult = Vec<ChildDecl>;

// ---------------------------------------------------------------------------
// WidgetBuilder
// ---------------------------------------------------------------------------

/// Type-erased widget constructor.
///
/// Currently only supports an already-constructed widget (`Ready`). Future
/// variants could support lazy construction or factory closures.
pub enum WidgetBuilder {
    /// A fully constructed widget, ready to be inserted into the arena.
    Ready(Box<dyn Widget>),
}

impl std::fmt::Debug for WidgetBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ready(_) => f.write_str("WidgetBuilder::Ready(…)"),
        }
    }
}

// ---------------------------------------------------------------------------
// ChildDecl
// ---------------------------------------------------------------------------

/// A lightweight descriptor for a widget to be mounted in the tree.
///
/// `ChildDecl` is a *declaration*, not a live node. The runtime consumes these
/// during mount to create actual `WidgetNode` entries in the `WidgetTree` arena.
///
/// Declarations can be nested: `children` holds sub-declarations that become
/// children of the widget produced by `builder`.
pub struct ChildDecl {
    /// Type-erased widget constructor.
    pub(crate) builder: WidgetBuilder,
    /// Nested child declarations (mounted under this widget).
    pub(crate) children: Vec<ChildDecl>,
    /// Optional CSS id (set via `.with_id()`).
    pub(crate) id: Option<String>,
    /// Initial CSS classes (set via `.with_classes()`).
    pub(crate) classes: Vec<String>,
    /// Sink fired with the mounted node's identity (set via `HandleSlot::bind`).
    pub(crate) handle_sink: Option<crate::handle::HandleSink>,
}

impl ChildDecl {
    /// Create a new declaration from an already-constructed widget.
    pub fn new(widget: Box<dyn Widget>) -> Self {
        Self {
            builder: WidgetBuilder::Ready(widget),
            children: Vec::new(),
            id: None,
            classes: Vec::new(),
            handle_sink: None,
        }
    }

    /// Set the CSS id for this declaration.
    pub fn with_id(mut self, id: &str) -> Self {
        self.id = Some(id.to_string());
        self
    }

    /// Set initial CSS classes for this declaration.
    pub fn with_classes(mut self, classes: &[&str]) -> Self {
        self.classes = classes.iter().map(|c| (*c).to_string()).collect();
        self
    }

    /// Append nested child declarations.
    ///
    /// These children will be mounted under the widget produced by this
    /// declaration's builder.
    pub fn with_children(mut self, children: Vec<ChildDecl>) -> Self {
        self.children = children;
        self
    }

    /// The CSS id declared for this child (via [`with_id`](Self::with_id)), if any.
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    /// The CSS classes declared for this child (via
    /// [`with_classes`](Self::with_classes)).
    pub fn classes(&self) -> &[String] {
        &self.classes
    }

    /// Borrow the declared widget (the `Ready` builder's payload).
    ///
    /// Used by tests that assert on a freshly-composed child's pre-mount state
    /// (e.g. its `style_type()`) without draining it into an arena tree.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn widget(&self) -> &dyn Widget {
        let WidgetBuilder::Ready(w) = &self.builder;
        w.as_ref()
    }

    /// Mutably borrow the declared widget (the `Ready` builder's payload).
    ///
    /// Used by tests that assert on a freshly-composed child's pre-mount state
    /// (e.g. its seed css-id) without draining it into an arena tree.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn widget_mut(&mut self) -> &mut dyn Widget {
        let WidgetBuilder::Ready(w) = &mut self.builder;
        w.as_mut()
    }

    /// Consume the declaration, yielding just the boxed widget (dropping any
    /// nested decls/id/classes). Used by test helpers that mount a single
    /// leaf child directly into an arena tree.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn into_widget(self) -> Box<dyn Widget> {
        let WidgetBuilder::Ready(w) = self.builder;
        w
    }
}

/// Re-assemble a widget's declared children into self-describing [`ChildDecl`]s
/// from the legacy parallel arrays (`children` + index-keyed decl-meta +
/// index-keyed handle sinks).
///
/// RA2.1 makes `compose(&mut self)` the single child-declaration path. Widgets
/// that historically stored children plus `with_compose`/`with_child_handle`
/// side metadata drain all three here and fold them back into one `ChildDecl`
/// per child (id + classes + handle sink bundled), so the runtime never has to
/// consult a parallel side channel again.
pub(crate) fn zip_child_decls(
    children: Vec<Box<dyn Widget>>,
    meta: Vec<crate::widgets::ChildDeclMeta>,
    sinks: Vec<(usize, crate::handle::HandleSink)>,
) -> ComposeResult {
    let mut meta_map: std::collections::HashMap<usize, (Option<String>, Vec<String>)> =
        meta.into_iter().map(|(i, id, c)| (i, (id, c))).collect();
    let mut sink_map: std::collections::HashMap<usize, crate::handle::HandleSink> =
        sinks.into_iter().collect();
    children
        .into_iter()
        .enumerate()
        .map(|(i, w)| {
            let mut decl = ChildDecl::new(w);
            if let Some((id, classes)) = meta_map.remove(&i) {
                decl.id = id;
                decl.classes = classes;
            }
            if let Some(sink) = sink_map.remove(&i) {
                decl.handle_sink = Some(sink);
            }
            decl
        })
        .collect()
}

impl std::fmt::Debug for ChildDecl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChildDecl")
            .field("builder", &self.builder)
            .field("children", &self.children.len())
            .field("id", &self.id)
            .field("classes", &self.classes)
            .field("handle_sink", &self.handle_sink.is_some())
            .finish()
    }
}

// ---------------------------------------------------------------------------
// From<W: Widget> → ChildDecl
// ---------------------------------------------------------------------------

/// Wrap a live widget in a `ChildDecl` with no decl-channel id/classes.
///
/// The resulting decl's own `id`/`classes` are empty **by design**: a widget's
/// declared identity (set via its native `.id()`/`.class()` builders) lives in
/// the widget's own [`NodeSeed`](crate::widgets::NodeSeed) and is harvested at
/// mount by `take_node_seed`, on every mount path (app root and pushed-screen
/// root share the same `mount_declarations` recursion). So `ChildDecl::from(w)`
/// leaving `id: None` is not an id drop — the id simply travels inside `w`. Use
/// [`with_id`](ChildDecl::with_id)/[`with_classes`](ChildDecl::with_classes) only
/// to attach identity that the widget does not already carry itself.
impl<W: Widget + 'static> From<W> for ChildDecl {
    fn from(widget: W) -> Self {
        Self::new(Box::new(widget))
    }
}

// ---------------------------------------------------------------------------
// compose![] macro
// ---------------------------------------------------------------------------

/// Declarative macro for building a `ComposeResult` from widget expressions.
///
/// Each expression must evaluate to something that implements `Into<ChildDecl>`
/// (any `Widget + 'static` qualifies via the blanket `From` impl).
///
/// # Example
///
/// ```ignore
/// compose![
///     Header::new(),
///     Button::new("OK").with_id("ok-btn"),
/// ]
/// ```
#[macro_export]
macro_rules! compose {
    ( $( $widget:expr ),* $(,)? ) => {
        vec![ $( <_ as ::std::convert::Into<$crate::compose::ChildDecl>>::into($widget) ),* ]
    };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rich_rs::{Console, ConsoleOptions, Segments};

    /// Minimal widget for testing.
    struct Stub;

    impl Stub {
        fn new() -> Self {
            Self
        }
    }

    impl Widget for Stub {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn style_type(&self) -> &'static str {
            "Stub"
        }
    }

    #[test]
    fn child_decl_from_widget() {
        let decl = ChildDecl::from(Stub::new());
        assert!(decl.id.is_none());
        assert!(decl.classes.is_empty());
        assert!(decl.children.is_empty());
    }

    #[test]
    fn child_decl_builder_methods() {
        let decl = ChildDecl::from(Stub::new())
            .with_id("my-id")
            .with_classes(&["foo", "bar"]);
        assert_eq!(decl.id.as_deref(), Some("my-id"));
        assert_eq!(decl.classes, vec!["foo", "bar"]);
    }

    #[test]
    fn child_decl_with_children() {
        let decl = ChildDecl::from(Stub::new()).with_children(vec![
            ChildDecl::from(Stub::new()),
            ChildDecl::from(Stub::new()),
        ]);
        assert_eq!(decl.children.len(), 2);
    }

    #[test]
    fn compose_macro_empty() {
        let result: ComposeResult = compose![];
        assert!(result.is_empty());
    }

    #[test]
    fn compose_macro_single() {
        let result: ComposeResult = compose![Stub::new()];
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn compose_macro_multiple() {
        let result: ComposeResult = compose![Stub::new(), Stub::new(), Stub::new(),];
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn compose_macro_with_builder_methods() {
        let result: ComposeResult = compose![
            ChildDecl::from(Stub::new()).with_id("header"),
            ChildDecl::from(Stub::new()).with_classes(&["primary"]),
        ];
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id.as_deref(), Some("header"));
        assert_eq!(result[1].classes, vec!["primary"]);
    }

    #[test]
    fn compose_macro_trailing_comma() {
        let a: ComposeResult = compose![Stub::new(), Stub::new()];
        let b: ComposeResult = compose![Stub::new(), Stub::new(),];
        assert_eq!(a.len(), b.len());
    }

    #[test]
    fn widget_builder_debug() {
        let builder = WidgetBuilder::Ready(Box::new(Stub::new()));
        let dbg = format!("{:?}", builder);
        assert!(dbg.contains("Ready"));
    }

    #[test]
    fn child_decl_debug() {
        let decl = ChildDecl::from(Stub::new()).with_id("x");
        let dbg = format!("{:?}", decl);
        assert!(dbg.contains("ChildDecl"));
        assert!(dbg.contains("x"));
    }
}
