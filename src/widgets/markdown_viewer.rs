use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use rich_rs::{Console, ConsoleOptions, Renderable, Segments};

use crate::action::ParsedAction;
use crate::compose::ComposeResult;
use crate::debug::DebugLayout;
use crate::event::{Event, EventCtx};
use crate::message::{
    MarkdownTableOfContentsSelected, Message, MessageEvent, TreeNodeActivated,
};

use super::containers::ScrollableContainer;
use super::{
    BindingDecl, Markdown, Tree, TreeNode, Widget, WidgetStyles,
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
    tree: Tree,
    /// Cached intrinsic content width (from the Tree) so `content_width()` works
    /// even after the Tree is consumed by `take_composed_children()`.
    cached_content_width: Option<usize>,
    styles: WidgetStyles,
}

impl MarkdownTableOfContents {
    pub fn new(headings: Vec<(usize, String)>) -> Self {
        let tree = Self::build_tree_from_headings(&headings);
        let cached_content_width = tree.content_width();
        Self {
            headings,
            tree,
            cached_content_width,
            styles: WidgetStyles::default(),
        }
    }

    pub fn set_headings(&mut self, headings: Vec<(usize, String)>) {
        self.headings = headings;
        self.tree = Self::build_tree_from_headings(&self.headings);
        self.cached_content_width = self.tree.content_width();
    }

    /// Build a Tree from the current headings, nesting by heading level.
    ///
    /// Python builds H1 → root children, H2 → under last H1, H3 → under last H2, etc.
    /// Python sets `show_root = False` and `auto_expand = False`.
    fn build_tree_from_headings(headings: &[(usize, String)]) -> Tree {
        if headings.is_empty() {
            let mut tree = Tree::new(vec![TreeNode::new("Contents")]);
            tree.set_show_root_plain(false);
            return tree;
        }

        let root = TreeNode::new("Contents")
            .expanded(true)
            .allow_expand(true);

        // Build hierarchical structure: use a stack of (level, TreeNode) pairs.
        // Each heading is nested under the last heading with a lower level number.
        let nodes = build_heading_nodes(headings);
        let mut root = root;
        for node in nodes {
            root = root.with_child(node);
        }
        let mut tree = Tree::new(vec![root]);
        // Python parity: hide the "Contents" root, don't auto-expand children.
        tree.set_show_root_plain(false);
        tree
    }
}

impl Widget for MarkdownTableOfContents {
    fn style_type(&self) -> &'static str {
        "MarkdownTableOfContents"
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        let tree = std::mem::replace(
            &mut self.tree,
            Self::build_tree_from_headings(&[]),
        );
        vec![Box::new(tree)]
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        // Arena-tree mode: children render themselves.
        // Fallback: delegate to the internal Tree.
        Widget::render(&self.tree, console, options)
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

    fn content_width(&self) -> Option<usize> {
        self.cached_content_width
    }

    fn layout_constraints(&self) -> LayoutConstraints {
        LayoutConstraints {
            min_width: Some(16),
            ..Default::default()
        }
    }

    fn on_event_capture(&mut self, _event: &Event, _ctx: &mut EventCtx) {}
    fn on_event(&mut self, _event: &Event, _ctx: &mut EventCtx) {}

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        // When the child Tree fires TreeNodeActivated, extract the heading block_id
        // from node data and post MarkdownTableOfContentsSelected for the viewer.
        if let Message::TreeNodeActivated(TreeNodeActivated { data: Some(block_id), .. }) =
            &message.message
        {
            ctx.post_message(Message::MarkdownTableOfContentsSelected(
                MarkdownTableOfContentsSelected {
                    block_id: block_id.clone(),
                },
            ));
            ctx.set_handled();
        }
    }
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
/// Roman numeral prefixes for TOC labels, indexed by heading level (1-6).
/// Mirrors Python's `NUMERALS = " ⅠⅡⅢⅣⅤⅥ"`.
const NUMERALS: [char; 7] = [' ', 'Ⅰ', 'Ⅱ', 'Ⅲ', 'Ⅳ', 'Ⅴ', 'Ⅵ'];

fn build_heading_nodes(headings: &[(usize, String)]) -> Vec<TreeNode> {
    // We build the tree by accumulating nodes into a mutable structure,
    // then convert to TreeNode at the end.
    struct TocNode {
        label: String,
        /// Heading level (1-6) for numeral prefix.
        level: usize,
        /// Original flat heading index (used as block_id for click-to-scroll).
        heading_index: usize,
        children: Vec<TocNode>,
    }

    let mut roots: Vec<TocNode> = Vec::new();

    for (flat_idx, (level, title)) in headings.iter().enumerate() {
        let depth = level.saturating_sub(1); // H1=0 deep, H2=1 deep, etc.
        let new_node = TocNode {
            label: title.clone(),
            level: *level,
            heading_index: flat_idx,
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
    // Python parity: parent nodes start expanded (Python expands as it walks down
    // to place child headings), leaf nodes have allow_expand=false.
    // Each node carries its heading_index as data for click-to-scroll.
    // Labels are prefixed with Roman numeral by heading level.
    fn to_tree_node(toc: &TocNode) -> TreeNode {
        let has_children = !toc.children.is_empty();
        let numeral = NUMERALS.get(toc.level).copied().unwrap_or(' ');
        let prefixed_label = format!("{} {}", numeral, toc.label);
        let mut node = TreeNode::new(prefixed_label)
            .expanded(has_children)
            .allow_expand(has_children)
            .with_data(toc.heading_index.to_string());
        for child in &toc.children {
            node = node.with_child(to_tree_node(child));
        }
        node
    }

    roots.iter().map(to_tree_node).collect()
}

// ---------------------------------------------------------------------------
// Navigator
// ---------------------------------------------------------------------------

/// Browser-like navigation history for Markdown documents.
///
/// Mirrors Python's `Navigator` class. Maintains a stack of path keys
/// with a cursor position. `go()` pushes a new location, `back()` and
/// `forward()` move the cursor, discarding forward history on new `go()` calls.
///
/// Path keys are resolved to content via the `MarkdownViewer`'s content registry.
pub struct Navigator {
    /// Stack of path keys (e.g. "demo.md", "example.md").
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

    /// Push a new location (path key), discarding any forward history.
    pub fn go(&mut self, location: impl Into<String>) -> bool {
        let location = location.into();
        // Truncate forward history.
        self.history.truncate(self.cursor + if self.history.is_empty() { 0 } else { 1 });
        self.history.push(location);
        self.cursor = self.history.len() - 1;
        true
    }

    /// Move back in history. Returns the path key if possible.
    pub fn back(&mut self) -> Option<&str> {
        if self.cursor > 0 {
            self.cursor -= 1;
            Some(&self.history[self.cursor])
        } else {
            None
        }
    }

    /// Move forward in history. Returns the path key if possible.
    pub fn forward(&mut self) -> Option<&str> {
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

    /// Current path key, if any.
    pub fn location(&self) -> Option<&str> {
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
/// ## Architecture
/// Internally delegates to a [`ScrollableContainer`], making this widget a scroll
/// host. Children are composed as:
/// - `Markdown` — the rendered content (scrollable)
/// - `MarkdownTableOfContents` — docked left via CSS
/// - Scrollbar widgets (from ScrollableContainer)
///
/// ## CSS class `-show-table-of-contents`
/// Added when `show_table_of_contents` is true; the default CSS uses this class
/// to toggle `MarkdownTableOfContents` visibility via `display: none/block`.
///
/// ## Navigation history
/// Uses a [`Navigator`] with `go()`, `back()`, and `forward()` methods for
/// browser-like content history, matching Python's `MarkdownViewer.navigator`.
pub struct MarkdownViewer {
    /// Scroll container that owns the Markdown + TOC children and scrollbar widgets.
    inner: ScrollableContainer,
    /// Shared content state between this viewer and its Markdown child.
    /// When `go()`/`back()`/`forward()` update content, the Markdown child picks it
    /// up during `on_layout()` via this shared reference.
    shared_markup: Arc<RwLock<String>>,
    content: String,
    /// CSS classes on this widget (e.g. `-show-table-of-contents`).
    classes: Vec<String>,
    /// Navigation history for back/forward (stores path keys).
    pub navigator: Navigator,
    /// Content registry: path key → markdown content.
    content_map: HashMap<String, String>,
}

impl MarkdownViewer {
    /// Create a new MarkdownViewer with initial content.
    ///
    /// For path-based navigation, use `register_content()` and `go()`.
    /// For simple single-document display, pass content directly.
    pub fn new(content: impl Into<String>) -> Self {
        let content = content.into();
        let mut navigator = Navigator::new();
        navigator.go("__initial__");
        let mut content_map = HashMap::new();
        content_map.insert("__initial__".to_string(), content.clone());

        let shared_markup = Arc::new(RwLock::new(content.clone()));
        let headings = Self::parse_headings(&content);
        let inner = ScrollableContainer::new()
            .with_child(Markdown::with_shared_markup(shared_markup.clone()))
            .with_child(MarkdownTableOfContents::new(headings));

        Self {
            inner,
            shared_markup,
            content,
            classes: vec!["-show-table-of-contents".to_string()],
            navigator,
            content_map,
        }
    }

    /// Register content for a path key.
    pub fn register_content(&mut self, path: impl Into<String>, content: impl Into<String>) {
        self.content_map.insert(path.into(), content.into());
    }

    pub fn show_table_of_contents(mut self, show: bool) -> Self {
        self.set_show_table_of_contents(show);
        self
    }

    pub fn set_show_table_of_contents(&mut self, show: bool) {
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
        self.classes.iter().any(|c| c == "-show-table-of-contents")
    }

    pub fn set_content(&mut self, content: impl Into<String>) {
        self.content = content.into();
        if let Ok(mut shared) = self.shared_markup.write() {
            *shared = self.content.clone();
        }
    }

    /// Navigate to a registered path key, pushing it onto the history stack.
    pub fn go(&mut self, path: impl Into<String>) -> bool {
        let path = path.into();
        if let Some(content) = self.content_map.get(&path).cloned() {
            self.navigator.go(&path);
            self.content = content.clone();
            if let Ok(mut shared) = self.shared_markup.write() {
                *shared = content;
            }
            true
        } else {
            false
        }
    }

    /// Navigate back in history. Returns `true` if navigation occurred.
    pub fn back(&mut self) -> bool {
        if let Some(location) = self.navigator.back() {
            if let Some(content) = self.content_map.get(location).cloned() {
                self.content = content.clone();
                if let Ok(mut shared) = self.shared_markup.write() {
                    *shared = content;
                }
                return true;
            }
        }
        false
    }

    /// Navigate forward in history. Returns `true` if navigation occurred.
    pub fn forward(&mut self) -> bool {
        if let Some(location) = self.navigator.forward() {
            if let Some(content) = self.content_map.get(location).cloned() {
                self.content = content.clone();
                if let Ok(mut shared) = self.shared_markup.write() {
                    *shared = content;
                }
                return true;
            }
        }
        false
    }

    /// Extract headings from the current content.
    pub fn extract_headings(&self) -> Vec<(usize, String)> {
        Self::parse_headings(&self.content)
    }

    /// Compute the approximate line offset of the Nth heading in the content.
    ///
    /// Scans markdown content counting lines until the heading at `heading_index`
    /// is found. Used by click-to-scroll to position the scroll offset.
    fn heading_line_offset(&self, heading_index: usize) -> usize {
        let mut found = 0usize;
        for (line_idx, line) in self.content.lines().enumerate() {
            let trimmed = line.trim_start();
            let marker_len = trimmed.chars().take_while(|ch| *ch == '#').count();
            if marker_len > 0 && marker_len <= 6 {
                let title = trimmed[marker_len..].trim();
                if !title.is_empty() {
                    if found == heading_index {
                        return line_idx;
                    }
                    found += 1;
                }
            }
        }
        0
    }

    fn parse_headings(content: &str) -> Vec<(usize, String)> {
        content
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
}

// ---------------------------------------------------------------------------
// Widget impl — delegates scroll behavior to inner ScrollableContainer,
// overrides identity (style_type, style_classes) for CSS resolution.
// ---------------------------------------------------------------------------

impl Widget for MarkdownViewer {
    fn style_type(&self) -> &'static str {
        "MarkdownViewer"
    }

    fn style_classes(&self) -> &[String] {
        &self.classes
    }

    fn compose(&self) -> ComposeResult {
        self.inner.compose()
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        self.inner.take_composed_children()
    }

    fn focusable(&self) -> bool {
        self.inner.focusable()
    }

    fn can_focus(&self) -> bool {
        self.inner.can_focus()
    }

    fn can_focus_children(&self) -> bool {
        self.inner.can_focus_children()
    }

    fn set_focus(&mut self, focused: bool) {
        self.inner.set_focus(focused);
    }

    fn has_focus(&self) -> bool {
        self.inner.has_focus()
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.inner.render(console, options)
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        self.inner.render_with_debug(console, options, debug)
    }

    fn on_mount(&mut self) {
        self.inner.on_mount();
    }

    fn on_unmount(&mut self) {
        self.inner.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.inner.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.inner.on_resize(width, height);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.inner.on_layout(width, height);
    }

    fn set_virtual_content_size(&mut self, width: usize, height: usize) {
        self.inner.set_virtual_content_size(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.inner.on_event(event, ctx);
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        // Handle TOC heading selection: scroll to the heading in the document.
        if let Message::MarkdownTableOfContentsSelected(
            MarkdownTableOfContentsSelected { block_id },
        ) = &message.message
        {
            if let Ok(heading_index) = block_id.parse::<usize>() {
                // Estimate the heading position: each heading's vertical offset
                // depends on the Markdown rendering. For now, use the heading index
                // to compute an approximate line position by scanning the content.
                let target_line = self.heading_line_offset(heading_index);
                self.inner.scroll_to(target_line);
                ctx.request_repaint();
            }
            ctx.set_handled();
            return;
        }
        self.inner.on_message(message, ctx);
    }

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        self.inner.on_mouse_scroll(delta_x, delta_y, ctx);
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        self.inner.on_mouse_move(x, y)
    }

    fn scroll_offset(&self) -> (usize, usize) {
        self.inner.scroll_offset()
    }

    fn clips_descendants_to_content(&self) -> bool {
        self.inner.clips_descendants_to_content()
    }

    fn scroll_viewport_size(&self) -> Option<(usize, usize)> {
        self.inner.scroll_viewport_size()
    }

    fn layout_height(&self) -> Option<usize> {
        self.inner.layout_height()
    }

    fn content_width(&self) -> Option<usize> {
        self.inner.content_width()
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        self.inner.bindings()
    }

    fn execute_action(&mut self, action: &ParsedAction, ctx: &mut EventCtx) -> bool {
        self.inner.execute_action(action, ctx)
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        self.inner.styles()
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        self.inner.styles_mut()
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
        let h = viewer.extract_headings();
        assert_eq!(h.len(), 3);
        assert_eq!(h[0], (1, "H1".to_string()));
        assert_eq!(h[1], (2, "H2".to_string()));
        assert_eq!(h[2], (3, "H3".to_string()));
    }

    #[test]
    fn markdown_viewer_is_scroll_host() {
        // Verify that MarkdownViewer delegates scroll behavior.
        let viewer = MarkdownViewer::new("# Test");
        assert_eq!(viewer.scroll_offset(), (0, 0));
        assert!(viewer.clips_descendants_to_content());
    }

    #[test]
    fn markdown_viewer_children_include_scrollbars() {
        // take_composed_children() should return user children + scrollbar widgets.
        let mut viewer = MarkdownViewer::new("# Test");
        let children = viewer.take_composed_children();
        // At minimum: Markdown, MarkdownTableOfContents, + scrollbar widgets.
        assert!(
            children.len() >= 2,
            "expected at least 2 children (Markdown + TOC), got {}",
            children.len()
        );
        // First child should be Markdown (or its contents from flattening).
        // Scrollbar widgets should be present.
        let has_scrollbar = children.iter().any(|c| {
            let st = c.style_type();
            st.contains("Scrollbar") || st.contains("ScrollBar")
        });
        assert!(
            has_scrollbar || children.len() >= 3,
            "expected scrollbar widgets in children"
        );
    }

    #[test]
    fn markdown_viewer_style_type() {
        let viewer = MarkdownViewer::new("# Test");
        assert_eq!(viewer.style_type(), "MarkdownViewer");
    }

    #[test]
    fn markdown_viewer_style_classes_include_toc_class() {
        let viewer = MarkdownViewer::new("# Test");
        assert!(
            viewer.style_classes().iter().any(|c| c == "-show-table-of-contents"),
            "expected -show-table-of-contents class"
        );
    }

    #[test]
    fn markdown_viewer_toggle_toc_removes_class() {
        let mut viewer = MarkdownViewer::new("# Test");
        viewer.set_show_table_of_contents(false);
        assert!(
            !viewer.style_classes().iter().any(|c| c == "-show-table-of-contents"),
            "class should be removed when TOC is hidden"
        );
    }

    #[test]
    fn toc_set_headings_updates_content() {
        let mut toc = MarkdownTableOfContents::new(vec![(1, "H1".to_string())]);
        toc.set_headings(vec![(1, "H1".to_string()), (2, "H2".to_string())]);
        assert_eq!(toc.headings.len(), 2);
    }

    #[test]
    fn toc_composes_tree_child() {
        let mut toc = MarkdownTableOfContents::new(vec![
            (1, "Chapter".to_string()),
            (2, "Section".to_string()),
        ]);
        let children = toc.take_composed_children();
        assert_eq!(children.len(), 1, "TOC should compose exactly 1 Tree child");
        assert_eq!(children[0].style_type(), "Tree");
    }

    // ── Navigator path-based, TOC settings ───────────────────────────────

    #[test]
    fn navigator_path_based_go_back_forward() {
        let mut nav = Navigator::new();
        nav.go("page1.md");
        nav.go("page2.md");
        assert_eq!(nav.location(), Some("page2.md"));

        assert_eq!(nav.back(), Some("page1.md"));
        assert_eq!(nav.location(), Some("page1.md"));

        assert_eq!(nav.forward(), Some("page2.md"));
        assert_eq!(nav.location(), Some("page2.md"));
    }

    #[test]
    fn navigator_start_end_properties() {
        let mut nav = Navigator::new();
        assert!(nav.at_start());
        assert!(nav.at_end());

        nav.go("a.md");
        assert!(nav.at_start());
        assert!(nav.at_end());

        nav.go("b.md");
        assert!(!nav.at_start());
        assert!(nav.at_end());

        nav.back();
        assert!(nav.at_start());
        assert!(!nav.at_end());
    }

    #[test]
    fn viewer_register_content_and_navigate() {
        let mut viewer = MarkdownViewer::new("initial");
        viewer.register_content("demo.md", "# Demo");
        viewer.register_content("example.md", "# Example");

        assert!(viewer.go("demo.md"));
        assert_eq!(viewer.content, "# Demo");

        assert!(viewer.go("example.md"));
        assert_eq!(viewer.content, "# Example");

        assert!(viewer.back());
        assert_eq!(viewer.content, "# Demo");

        assert!(viewer.forward());
        assert_eq!(viewer.content, "# Example");
    }

    #[test]
    fn viewer_go_unknown_path_returns_false() {
        let mut viewer = MarkdownViewer::new("initial");
        assert!(!viewer.go("nonexistent.md"));
        assert_eq!(viewer.content, "initial");
    }

    #[test]
    fn toc_tree_hides_root() {
        let toc = MarkdownTableOfContents::new(vec![(1, "H1".to_string())]);
        let tree = MarkdownTableOfContents::build_tree_from_headings(&toc.headings);
        assert!(!tree.showing_root());
    }

    #[test]
    fn toc_tree_parent_nodes_start_expanded() {
        let toc = MarkdownTableOfContents::new(vec![
            (1, "Chapter".to_string()),
            (2, "Section".to_string()),
        ]);
        let tree = MarkdownTableOfContents::build_tree_from_headings(&toc.headings);
        // Python parity: parent nodes start expanded (Python expands as it walks
        // down to place child headings). The root "Contents" is always expanded.
        if let Some(root) = tree.root() {
            assert!(
                root.is_expanded(),
                "Root 'Contents' node should start expanded"
            );
        }
    }

    #[test]
    fn toc_content_width_reflects_tree() {
        let toc = MarkdownTableOfContents::new(vec![
            (1, "A Long Chapter Title".to_string()),
            (2, "Section".to_string()),
        ]);
        let w = toc.content_width();
        assert!(w.is_some(), "TOC should report content_width from inner Tree");
        // Width should be at least as wide as the longest label.
        assert!(w.unwrap() >= 20, "content_width should reflect label length");
    }

    #[test]
    fn tree_node_data_carries_heading_index() {
        let toc = MarkdownTableOfContents::new(vec![
            (1, "Chapter".to_string()),
            (2, "Section".to_string()),
        ]);
        let tree = MarkdownTableOfContents::build_tree_from_headings(&toc.headings);
        // Root is "Contents" (data="0" would be wrong since root isn't a heading).
        // The first real heading is "Chapter" at index 0 in the flat headings list.
        if let Some(root) = tree.root() {
            // Root children = H1 nodes
            let h1 = root.children_slice();
            assert!(!h1.is_empty());
            assert_eq!(h1[0].data(), Some("0")); // Chapter is heading index 0
            let h2_children = h1[0].children_slice();
            assert!(!h2_children.is_empty());
            assert_eq!(h2_children[0].data(), Some("1")); // Section is heading index 1
        }
    }

    #[test]
    fn heading_line_offset_finds_correct_line() {
        let content = "Some preamble\n\n# First Heading\n\nText\n\n## Second Heading\n";
        let viewer = MarkdownViewer::new(content);
        // Heading 0: "# First Heading" is on line 2
        assert_eq!(viewer.heading_line_offset(0), 2);
        // Heading 1: "## Second Heading" is on line 6
        assert_eq!(viewer.heading_line_offset(1), 6);
    }

    #[test]
    fn toc_on_message_posts_toc_selected() {
        let mut toc = MarkdownTableOfContents::new(vec![
            (1, "Chapter".to_string()),
        ]);
        let msg = MessageEvent {
            sender: crate::node_id::NodeId::default(),
            message: Message::TreeNodeActivated(TreeNodeActivated {
                index: 0,
                label: "Chapter".to_string(),
                data: Some("0".to_string()),
            }),
            control: None,
        };
        let mut ctx = crate::event::EventCtx::default();
        toc.on_message(&msg, &mut ctx);
        assert!(ctx.handled());
        let messages = ctx.take_messages();
        assert!(
            messages.iter().any(|m| matches!(
                &m.message,
                Message::MarkdownTableOfContentsSelected(
                    MarkdownTableOfContentsSelected { block_id }
                ) if block_id == "0"
            )),
            "TOC should post MarkdownTableOfContentsSelected with block_id"
        );
    }

    // ── Shared markup + content propagation tests ─────────────────────────

    #[test]
    fn shared_markup_syncs_on_layout() {
        let shared = Arc::new(RwLock::new("# Hello".to_string()));
        let mut md = Markdown::with_shared_markup(shared.clone());
        assert_eq!(md.layout_height(), Some(1));

        // Update shared content to something taller.
        *shared.write().unwrap() = "Line1\nLine2\nLine3\nLine4\nLine5".to_string();

        // Before on_layout, markup is still old.
        assert_eq!(md.layout_height(), Some(1));

        // on_layout triggers sync.
        md.on_layout(40, 10);
        assert_eq!(md.layout_height(), Some(5));
    }

    #[test]
    fn viewer_go_updates_shared_markup() {
        let mut viewer = MarkdownViewer::new("initial");
        viewer.register_content("demo.md", "# Demo\n\nParagraph\n\nMore text");

        viewer.go("demo.md");

        // The shared markup should now contain the new content.
        let shared_content = viewer.shared_markup.read().unwrap().clone();
        assert_eq!(shared_content, "# Demo\n\nParagraph\n\nMore text");
    }

    #[test]
    fn viewer_back_forward_updates_shared_markup() {
        let mut viewer = MarkdownViewer::new("initial");
        viewer.register_content("a.md", "# Page A");
        viewer.register_content("b.md", "# Page B");

        viewer.go("a.md");
        viewer.go("b.md");
        assert_eq!(*viewer.shared_markup.read().unwrap(), "# Page B");

        viewer.back();
        assert_eq!(*viewer.shared_markup.read().unwrap(), "# Page A");

        viewer.forward();
        assert_eq!(*viewer.shared_markup.read().unwrap(), "# Page B");
    }

    #[test]
    fn viewer_scroll_viewport_size_delegates() {
        let viewer = MarkdownViewer::new("# Test");
        // Before any layout, viewport is 0×0 → None.
        assert_eq!(viewer.scroll_viewport_size(), None);
    }
}
