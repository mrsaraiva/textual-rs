use rich_rs::{Console, ConsoleOptions, Renderable, Segments};

use crate::compose::ComposeResult;
use crate::event::{Event, EventCtx};
use crate::message::MessageEvent;

use super::{
    Markdown, ScrollView, Tree, TreeNode, Widget, WidgetStyles,
    core::LayoutConstraints,
};

// ---------------------------------------------------------------------------
// MarkdownTableOfContents
// ---------------------------------------------------------------------------

/// A sidebar widget showing the headings of a Markdown document as a tree.
///
/// Mirrors Python's `MarkdownTableOfContents` (which extends `Tree`).
/// In Rust, it wraps a `Tree` and rebuilds it whenever `set_headings` is called.
pub struct MarkdownTableOfContents {
    headings: Vec<(usize, String)>,
    styles: WidgetStyles,
}

impl MarkdownTableOfContents {
    pub fn new(headings: Vec<(usize, String)>) -> Self {
        Self {
            headings,
            styles: WidgetStyles::default(),
        }
    }

    pub fn set_headings(&mut self, headings: Vec<(usize, String)>) {
        self.headings = headings;
    }

    /// Build a Tree from the current headings, nesting by heading level.
    ///
    /// Python builds H1 → root children, H2 → under last H1, H3 → under last H2, etc.
    fn build_tree(&self) -> Tree {
        if self.headings.is_empty() {
            return Tree::new(vec![TreeNode::new("Contents")]);
        }

        let root = TreeNode::new("Contents")
            .expanded(true)
            .allow_expand(true);

        // Build hierarchical structure: use a stack of (level, TreeNode) pairs.
        // Each heading is nested under the last heading with a lower level number.
        let nodes = build_heading_nodes(&self.headings);
        let mut root = root;
        for node in nodes {
            root = root.with_child(node);
        }
        Tree::new(vec![root])
    }
}

impl Widget for MarkdownTableOfContents {
    fn style_type(&self) -> &'static str {
        "MarkdownTableOfContents"
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(&self.build_tree(), console, options)
    }

    fn layout_height(&self) -> Option<usize> {
        None // Fill available space
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }

    fn layout_constraints(&self) -> LayoutConstraints {
        LayoutConstraints {
            min_width: Some(20),
            max_width: Some(40),
            ..Default::default()
        }
    }

    fn on_event_capture(&mut self, _event: &Event, _ctx: &mut EventCtx) {}
    fn on_event(&mut self, _event: &Event, _ctx: &mut EventCtx) {}
    fn on_message(&mut self, _message: &MessageEvent, _ctx: &mut EventCtx) {}
}

impl Renderable for MarkdownTableOfContents {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

// ---------------------------------------------------------------------------
// Heading hierarchy builder
// ---------------------------------------------------------------------------

/// Build hierarchical `TreeNode`s from a flat list of `(level, title)` headings.
///
/// Mirrors Python's algorithm: for each heading at level N, walk from the root
/// down into the last child N-1 times, then add the heading as a leaf there.
/// H1 → root children, H2 → under last H1, H3 → under last H2, etc.
fn build_heading_nodes(headings: &[(usize, String)]) -> Vec<TreeNode> {
    // We build the tree by accumulating nodes into a mutable structure,
    // then convert to TreeNode at the end.
    struct TocNode {
        label: String,
        children: Vec<TocNode>,
    }

    let mut roots: Vec<TocNode> = Vec::new();

    for (level, title) in headings {
        let depth = level.saturating_sub(1); // H1=0 deep, H2=1 deep, etc.
        let new_node = TocNode {
            label: title.clone(),
            children: Vec::new(),
        };

        // Walk down `depth` levels into the last child at each step.
        let mut target = &mut roots;
        for _ in 0..depth {
            if target.is_empty() {
                break;
            }
            let last = target.last_mut().unwrap();
            target = &mut last.children;
        }
        target.push(new_node);
    }

    // Convert TocNode tree to TreeNode tree.
    fn to_tree_node(toc: &TocNode) -> TreeNode {
        let has_children = !toc.children.is_empty();
        let mut node = TreeNode::new(&toc.label)
            .expanded(true)
            .allow_expand(has_children);
        for child in &toc.children {
            node = node.with_child(to_tree_node(child));
        }
        node
    }

    roots.iter().map(to_tree_node).collect()
}

// ---------------------------------------------------------------------------
// MarkdownViewer
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Navigator
// ---------------------------------------------------------------------------

/// Browser-like navigation history for Markdown documents.
///
/// Mirrors Python's `Navigator` class. Maintains a stack of content strings
/// with a cursor position. `go()` pushes new content, `back()` and `forward()`
/// move the cursor, discarding forward history on new `go()` calls.
pub struct Navigator {
    history: Vec<String>,
    cursor: usize,
}

impl Navigator {
    fn new() -> Self {
        Self {
            history: Vec::new(),
            cursor: 0,
        }
    }

    /// Push new content, discarding any forward history.
    fn go(&mut self, content: String) {
        // Truncate forward history.
        self.history.truncate(self.cursor + if self.history.is_empty() { 0 } else { 1 });
        self.history.push(content);
        self.cursor = self.history.len() - 1;
    }

    /// Move back in history. Returns the content if possible.
    fn back(&mut self) -> Option<&str> {
        if self.cursor > 0 {
            self.cursor -= 1;
            Some(&self.history[self.cursor])
        } else {
            None
        }
    }

    /// Move forward in history. Returns the content if possible.
    fn forward(&mut self) -> Option<&str> {
        if self.cursor + 1 < self.history.len() {
            self.cursor += 1;
            Some(&self.history[self.cursor])
        } else {
            None
        }
    }

    /// True if at the start of history (can't go back).
    pub fn at_start(&self) -> bool {
        self.cursor == 0
    }

    /// True if at the end of history (can't go forward).
    pub fn at_end(&self) -> bool {
        self.history.is_empty() || self.cursor >= self.history.len() - 1
    }

    /// Current content, if any.
    pub fn current(&self) -> Option<&str> {
        self.history.get(self.cursor).map(String::as_str)
    }
}

// ---------------------------------------------------------------------------
// MarkdownViewer
// ---------------------------------------------------------------------------

/// A composite viewer for Markdown content with an optional Table of Contents sidebar.
///
/// Mirrors Python's `MarkdownViewer` (which extends `VerticalScroll` and composes
/// a `Markdown` widget and a `MarkdownTableOfContents` sidebar).
///
/// ## Layout
/// - The `MarkdownTableOfContents` is docked to the left (via CSS `dock: left`).
///   When `show_table_of_contents` is false, the child is hidden via `child_display_for_tree`.
/// - The main content is a `ScrollView` wrapping the `Markdown` renderer.
///
/// ## CSS class `-show-table-of-contents`
/// Added when `show_table_of_contents` is true; the default CSS uses this class
/// to toggle `MarkdownTableOfContents` visibility.
///
/// ## Navigation history
/// Uses a [`Navigator`] with `go()`, `back()`, and `forward()` methods for
/// browser-like content history, matching Python's `MarkdownViewer.navigator`.
pub struct MarkdownViewer {
    content: String,
    show_table_of_contents: bool,
    /// CSS classes on this widget (e.g. `-show-table-of-contents`).
    classes: Vec<String>,
    styles: WidgetStyles,
    children_extracted: bool,
    /// Navigation history for back/forward.
    pub navigator: Navigator,
}

impl MarkdownViewer {
    pub fn new(content: impl Into<String>) -> Self {
        let content = content.into();
        let mut navigator = Navigator::new();
        navigator.go(content.clone());
        Self {
            content,
            show_table_of_contents: true,
            classes: vec!["-show-table-of-contents".to_string()],
            styles: WidgetStyles::default(),
            children_extracted: false,
            navigator,
        }
    }

    pub fn show_table_of_contents(mut self, show: bool) -> Self {
        self.set_show_table_of_contents(show);
        self
    }

    pub fn set_show_table_of_contents(&mut self, show: bool) {
        self.show_table_of_contents = show;
        const CLASS: &str = "-show-table-of-contents";
        if show {
            if !self.classes.iter().any(|c| c == CLASS) {
                self.classes.push(CLASS.to_string());
            }
        } else {
            self.classes.retain(|c| c != CLASS);
        }
    }

    pub fn is_showing_table_of_contents(&self) -> bool {
        self.show_table_of_contents
    }

    pub fn set_content(&mut self, content: impl Into<String>) {
        self.content = content.into();
    }

    /// Navigate to new content, pushing it onto the history stack.
    ///
    /// Mirrors Python's `MarkdownViewer.go()`. Discards any forward history.
    pub fn go(&mut self, content: impl Into<String>) {
        let content = content.into();
        self.navigator.go(content.clone());
        self.content = content;
    }

    /// Navigate back in history. Returns `true` if navigation occurred.
    pub fn back(&mut self) -> bool {
        if let Some(content) = self.navigator.back() {
            self.content = content.to_string();
            true
        } else {
            false
        }
    }

    /// Navigate forward in history. Returns `true` if navigation occurred.
    pub fn forward(&mut self) -> bool {
        if let Some(content) = self.navigator.forward() {
            self.content = content.to_string();
            true
        } else {
            false
        }
    }

    /// Extract headings from the current content.
    ///
    /// Returns `(level, title)` pairs; used to populate the TOC sidebar.
    pub fn extract_headings(&self) -> Vec<(usize, String)> {
        self.headings()
    }

    fn headings(&self) -> Vec<(usize, String)> {
        self.content
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim_start();
                let marker_len = trimmed.chars().take_while(|ch| *ch == '#').count();
                if marker_len == 0 || marker_len > 6 {
                    return None;
                }
                let title = trimmed[marker_len..].trim();
                if title.is_empty() {
                    return None;
                }
                Some((marker_len, title.to_string()))
            })
            .collect()
    }

    fn build_children(&self) -> Vec<Box<dyn Widget>> {
        let scroll = ScrollView::new(Markdown::new(self.content.clone()));
        let toc = MarkdownTableOfContents::new(self.headings());
        vec![Box::new(scroll), Box::new(toc)]
    }
}

impl Widget for MarkdownViewer {
    fn style_type(&self) -> &'static str {
        "MarkdownViewer"
    }

    fn compose(&self) -> ComposeResult {
        Vec::new()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        self.children_extracted = true;
        self.build_children()
    }

    /// Show main content always; show TOC only when `show_table_of_contents` is true.
    fn child_display_for_tree(&self, child_index: usize) -> Option<bool> {
        if !self.children_extracted {
            return None;
        }
        let visible = match child_index {
            0 => true, // main scroll view
            1 => self.show_table_of_contents, // TOC
            _ => false,
        };
        Some(visible)
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        // In arena-tree mode, children are rendered by the tree.
        // In flat mode (unit tests), render main content directly.
        let scroll = ScrollView::new(Markdown::new(self.content.clone()));
        Widget::render(&scroll, console, options)
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }

    fn style_classes(&self) -> &[String] {
        &self.classes
    }
}

impl Renderable for MarkdownViewer {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_viewer_default_shows_toc() {
        let viewer = MarkdownViewer::new("# Heading");
        assert!(viewer.is_showing_table_of_contents());
    }

    #[test]
    fn markdown_viewer_hide_toc() {
        let viewer = MarkdownViewer::new("# Heading").show_table_of_contents(false);
        assert!(!viewer.is_showing_table_of_contents());
    }

    #[test]
    fn markdown_viewer_extracts_headings() {
        let viewer = MarkdownViewer::new("# H1\n## H2\n### H3");
        let h = viewer.headings();
        assert_eq!(h.len(), 3);
        assert_eq!(h[0], (1, "H1".to_string()));
        assert_eq!(h[1], (2, "H2".to_string()));
        assert_eq!(h[2], (3, "H3".to_string()));
    }

    #[test]
    fn markdown_viewer_child_display_for_tree_before_extraction_returns_none() {
        let viewer = MarkdownViewer::new("# Test");
        assert_eq!(viewer.child_display_for_tree(0), None);
        assert_eq!(viewer.child_display_for_tree(1), None);
    }

    #[test]
    fn markdown_viewer_child_display_after_extraction_hides_toc_when_disabled() {
        let mut viewer = MarkdownViewer::new("# Test").show_table_of_contents(false);
        let _ = viewer.take_composed_children();
        assert_eq!(viewer.child_display_for_tree(0), Some(true));
        assert_eq!(viewer.child_display_for_tree(1), Some(false));
    }

    #[test]
    fn markdown_viewer_child_display_after_extraction_shows_toc_when_enabled() {
        let mut viewer = MarkdownViewer::new("# Test").show_table_of_contents(true);
        let _ = viewer.take_composed_children();
        assert_eq!(viewer.child_display_for_tree(0), Some(true));
        assert_eq!(viewer.child_display_for_tree(1), Some(true));
    }

    #[test]
    fn toc_set_headings_updates_content() {
        let mut toc = MarkdownTableOfContents::new(vec![(1, "H1".to_string())]);
        toc.set_headings(vec![(1, "H1".to_string()), (2, "H2".to_string())]);
        assert_eq!(toc.headings.len(), 2);
    }
}
