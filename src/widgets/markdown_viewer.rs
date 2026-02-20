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

    /// Build a Tree from the current headings.
    fn build_tree(&self) -> Tree {
        // Build a flat list of `TreeNode`s, indented by heading level.
        // Level 1 → root nodes; levels 2-6 → nested under preceding lower-level node.
        // For simplicity, build one root "Table of Contents" with all headings as children.
        let mut children: Vec<TreeNode> = Vec::new();
        for (level, title) in &self.headings {
            let indent = "  ".repeat(level.saturating_sub(1));
            children.push(TreeNode::new(format!("{indent}{title}")));
        }
        let root = if children.is_empty() {
            TreeNode::new("Contents")
        } else {
            let mut r = TreeNode::new("Contents")
                .expanded(true)
                .allow_expand(true);
            for child in children {
                r = r.with_child(child);
            }
            r
        };
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
/// Python's `go()`, `back()`, `forward()` and `Navigator` are not yet implemented.
/// DEFERRED: navigation history (go/back/forward/Navigator) — will be added in a
/// subsequent iteration once async document loading is wired into the runtime.
pub struct MarkdownViewer {
    content: String,
    show_table_of_contents: bool,
    /// CSS classes on this widget (e.g. `-show-table-of-contents`).
    classes: Vec<String>,
    styles: WidgetStyles,
    children_extracted: bool,
}

impl MarkdownViewer {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            show_table_of_contents: true,
            classes: vec!["-show-table-of-contents".to_string()],
            styles: WidgetStyles::default(),
            children_extracted: false,
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
