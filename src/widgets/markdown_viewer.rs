use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use rich_rs::{Console, ConsoleOptions, Segments};

use crate::action::{ActionDecl, ParsedAction};
use crate::compose::ComposeResult;
use crate::event::{Event, EventCtx};
use crate::message::{
    MarkdownTableOfContentsSelected, MarkdownTableOfContentsUpdated, MessageEvent,
    NavigatorUpdated, ScrollbarAxis, ScrollbarScrollTo, TreeNodeActivated,
};

use super::containers::VerticalScroll;
use super::delegate::{delegate_renderable, delegate_widget_method};
use super::markdown_model::parse_markdown_headings_with_lines;
use super::{Markdown, NodeSeed, Tree, TreeNode, Widget};

// ---------------------------------------------------------------------------
// MarkdownTableOfContents
// ---------------------------------------------------------------------------

type HeadingEntry = (usize, String, String);

const MARKDOWN_VIEWER_ACTIONS: &[ActionDecl] = &[ActionDecl {
    name: "link",
    namespace: "markdown_viewer",
    description: "Follow a markdown link",
    default_binding: None,
}];

/// A sidebar widget showing the headings of a Markdown document as a tree.
///
/// Mirrors Python's `MarkdownTableOfContents` (which composes a real `Tree` child).
pub struct MarkdownTableOfContents {
    shared_headings: Arc<RwLock<Vec<HeadingEntry>>>,
}

impl MarkdownTableOfContents {
    pub fn new(headings: Vec<HeadingEntry>) -> Self {
        Self {
            shared_headings: Arc::new(RwLock::new(headings)),
        }
    }

    pub fn with_shared_headings(shared: Arc<RwLock<Vec<HeadingEntry>>>) -> Self {
        Self {
            shared_headings: shared,
        }
    }

    pub fn set_headings(&mut self, headings: Vec<HeadingEntry>) {
        if let Ok(mut data) = self.shared_headings.write() {
            *data = headings;
        }
    }

    /// Build a Tree from the current headings, nesting by heading level.
    ///
    /// Python builds H1 → root children, H2 → under last H1, H3 → under last H2, etc.
    /// Python sets `show_root = False` and `auto_expand = False`.
    fn build_tree_from_headings(headings: &[HeadingEntry]) -> Tree {
        if headings.is_empty() {
            let mut tree = Tree::new(vec![TreeNode::new("Contents")]);
            tree.set_show_root_plain(false);
            return tree;
        }

        let root = TreeNode::new("Contents").expanded(true).allow_expand(true);
        let nodes = build_heading_nodes(headings);
        let mut root = root;
        for node in nodes {
            root = root.with_child(node);
        }
        let mut tree = Tree::new(vec![root]);
        tree.set_show_root_plain(false);
        tree.set_auto_expand(false);
        tree
    }
}

struct MarkdownTableOfContentsTree {
    headings: Vec<HeadingEntry>,
    shared_headings: Arc<RwLock<Vec<HeadingEntry>>>,
    tree: Tree,
}

impl MarkdownTableOfContentsTree {
    fn with_shared_headings(shared: Arc<RwLock<Vec<HeadingEntry>>>) -> Self {
        let initial = shared.read().map(|h| h.clone()).unwrap_or_default();
        let tree = MarkdownTableOfContents::build_tree_from_headings(&initial);
        Self {
            headings: initial,
            shared_headings: shared,
            tree,
        }
    }

    fn sync_headings(&mut self) {
        let next = self.shared_headings.read().ok().map(|h| h.clone());
        if let Some(headings) = next
            && headings != self.headings
        {
            self.headings = headings;
            self.tree = MarkdownTableOfContents::build_tree_from_headings(&self.headings);
        }
    }
}

impl Widget for MarkdownTableOfContentsTree {
    fn style_type(&self) -> &'static str {
        // Keep selector compatibility with Python and default CSS:
        // `MarkdownTableOfContents > Tree`.
        "Tree"
    }

    fn content_width(&self) -> Option<usize> {
        // In Python, the composed Tree fills the TOC pane; pane width is driven by
        // the parent `MarkdownTableOfContents` dock/intrinsic sizing. Returning `None`
        // avoids a second intrinsic-width clamp on the child Tree.
        None
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.sync_headings();
        self.tree.on_layout(width, height);
    }

    // delegate-audit: 70 methods as of 2026-02-26
    delegate_widget_method!(
        tree,
        [
            render,
            render_with_debug,
            render_line,
            render_lines,
            compose,
            focusable,
            can_focus,
            can_focus_children,
            on_mount,
            on_unmount,
            on_tick,
            on_resize,
            set_virtual_content_size,
            on_event_capture,
            on_event,
            on_message,
            on_mouse_scroll,
            on_mouse_move,
            on_app_key,
            on_app_action,
            on_app_message,
            on_app_tick,
            on_app_mount,
            scroll_offset,
            scroll_offset_f32,
            scroll_viewport_size,
            scroll_virtual_content_size,
            clips_descendants_to_content,
            child_display_for_tree,
            tree_child_content_inset,
            layout_height,
            preserve_underlay,
            bindings,
            binding_hints,
            execute_action,
            action_namespace,
            action_registry,
            style_type_aliases,
            border_title,
            border_subtitle,
            is_active,
            mouse_interactive,
            tooltip,
            tooltip_anchor,
            help_markup,
            allow_select,
            selection_at,
            selection_word_range_at,
            selection_all_range,
            update_selection,
            clear_selection,
            get_selection,
            selection_updated,
            reactive_widget,
        ]
    );
}

delegate_renderable!(MarkdownTableOfContentsTree);

impl Widget for MarkdownTableOfContents {
    fn style_type(&self) -> &'static str {
        "MarkdownTableOfContents"
    }

    fn compose(&mut self) -> ComposeResult {
        vec![MarkdownTableOfContentsTree::with_shared_headings(self.shared_headings.clone()).into()]
    }

    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn layout_height(&self) -> Option<usize> {
        None
    }

    fn content_width(&self) -> Option<usize> {
        let headings = self.shared_headings.read().ok().map(|h| h.clone())?;
        let tree = MarkdownTableOfContents::build_tree_from_headings(&headings);
        let base = tree.content_width().unwrap_or(1);

        // `MarkdownTableOfContents > Tree` contributes horizontal padding in default CSS.
        let toc_meta = crate::css::selector_meta_generic(self);
        let toc_resolved = crate::css::resolve_style(self, &toc_meta);
        let tree_meta = crate::css::selector_meta_generic(&tree);
        let tree_resolved = crate::css::with_style_stack(toc_meta, toc_resolved, || {
            crate::css::resolve_style(&tree, &tree_meta)
        });
        let padding = tree_resolved.effective_padding();
        Some(
            base.saturating_add(usize::from(padding.left))
                .saturating_add(usize::from(padding.right))
                .max(1),
        )
    }

    fn can_focus_children(&self) -> bool {
        true
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Some(m) = message.downcast_ref::<MarkdownTableOfContentsUpdated>() {
            if let Ok(mut shared) = self.shared_headings.write() {
                *shared = m.headings.clone();
            }
            // TOC width is content-driven (`width: auto` with dock). Heading changes
            // must invalidate layout so the sidebar width can be recomputed.
            ctx.request_layout_invalidation();
            ctx.request_repaint();
            return;
        }

        if let Some(m) = message.downcast_ref::<TreeNodeActivated>() {
            if let Some(block_id) = &m.data {
                ctx.post_message(MarkdownTableOfContentsSelected {
                    block_id: block_id.clone(),
                });
                ctx.set_handled();
            }
        }
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

fn build_heading_nodes(headings: &[HeadingEntry]) -> Vec<TreeNode> {
    // We build the tree by accumulating nodes into a mutable structure,
    // then convert to TreeNode at the end.
    struct TocNode {
        label: String,
        /// Heading level (1-6) for numeral prefix.
        level: usize,
        /// Stable heading block id.
        block_id: String,
        children: Vec<TocNode>,
    }

    let mut roots: Vec<TocNode> = Vec::new();

    for (level, title, block_id) in headings {
        let depth = level.saturating_sub(1); // H1=0 deep, H2=1 deep, etc.
        let new_node = TocNode {
            label: title.clone(),
            level: *level,
            block_id: block_id.clone(),
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
    // Each node carries its block_id as data for click-to-scroll.
    // Labels are prefixed with Roman numeral by heading level.
    fn to_tree_node(toc: &TocNode) -> TreeNode {
        let has_children = !toc.children.is_empty();
        let numeral = NUMERALS.get(toc.level).copied().unwrap_or(' ');
        let prefixed_label = format!("{} {}", numeral, toc.label);
        let mut node = TreeNode::new(prefixed_label)
            .expanded(has_children)
            .allow_expand(has_children)
            .with_data(toc.block_id.clone());
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
        self.history
            .truncate(self.cursor + if self.history.is_empty() { 0 } else { 1 });
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
    /// Python-parity scroll host (`VerticalScroll`) that owns Markdown + TOC children.
    inner: VerticalScroll,
    /// Shared content state between this viewer and its Markdown child.
    /// When `go()`/`back()`/`forward()` update content, the Markdown child picks it
    /// up during `on_layout()` via this shared reference.
    shared_markup: Arc<RwLock<String>>,
    /// Shared heading metadata used by TOC to stay synchronized with document updates.
    shared_headings: Arc<RwLock<Vec<HeadingEntry>>>,
    content: String,
    /// CSS classes on this widget (e.g. `-show-table-of-contents`).
    classes: Vec<String>,
    /// Navigation history for back/forward (stores path keys).
    pub navigator: Navigator,
    /// Content registry: path key → markdown content.
    content_map: HashMap<String, String>,
    /// Whether a TOC-updated message should be emitted on the next event turn.
    toc_dirty: bool,
    /// Pending TOC visibility class change to apply on the next event turn.
    /// Some(true) means add `-show-table-of-contents`; Some(false) means remove it.
    toc_class_pending: Option<bool>,
    /// One-shot identity/style payload consumed at mount.
    seed: NodeSeed,
}

impl MarkdownViewer {
    crate::seed_ident_methods!();

    /// Create a new MarkdownViewer with initial content.
    ///
    /// For path-based navigation, use `register_content()` and `go()`.
    /// For simple single-document display, pass content directly.
    pub fn new(content: impl Into<String>) -> Self {
        let content = content.into();
        let navigator = Navigator::new();
        let content_map = HashMap::new();

        let shared_markup = Arc::new(RwLock::new(content.clone()));
        let shared_headings = Arc::new(RwLock::new(Self::parse_headings(&content)));
        let inner = VerticalScroll::new()
            .scroll_step(2)
            .with_child(Markdown::with_shared_markup(shared_markup.clone()).with_can_focus(true))
            .with_child(MarkdownTableOfContents::with_shared_headings(
                shared_headings.clone(),
            ));

        Self {
            inner,
            shared_markup,
            shared_headings,
            content,
            classes: vec!["-show-table-of-contents".to_string()],
            navigator,
            content_map,
            toc_dirty: true,
            toc_class_pending: None,
            seed: NodeSeed::default(),
        }
    }

    /// Set a CSS id for this viewer (for query routing via `#id` selectors).
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        let id = id.into();
        self.seed.css_id = Some(id);
        self
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
        let was_showing = self.classes.iter().any(|c| c == CLASS);
        if show {
            if !was_showing {
                self.classes.push(CLASS.to_string());
            }
        } else {
            self.classes.retain(|c| c != CLASS);
        }
        // Queue a class op to propagate to the arena node on the next event turn.
        // (The widget struct's `classes` field is consumed by take_node_seed at mount;
        // after mount, CSS resolution reads from the arena node record, so we must
        // push class changes through EventCtx on the next event.)
        if show != was_showing {
            self.toc_class_pending = Some(show);
        }
    }

    pub fn is_showing_table_of_contents(&self) -> bool {
        self.classes.iter().any(|c| c == "-show-table-of-contents")
    }

    pub fn set_content(&mut self, content: impl Into<String>) {
        self.apply_content_update(content.into());
    }

    /// Navigate to a registered path key, pushing it onto the history stack.
    pub fn go(&mut self, path: impl Into<String>) -> bool {
        let path = path.into();
        if let Some(content) = self.content_map.get(&path).cloned() {
            self.navigator.go(&path);
            self.apply_content_update(content);
            true
        } else {
            false
        }
    }

    /// Navigate back in history. Returns `true` if navigation occurred.
    pub fn back(&mut self) -> bool {
        if let Some(location) = self.navigator.back() {
            if let Some(content) = self.content_map.get(location).cloned() {
                self.apply_content_update(content);
                return true;
            }
        }
        false
    }

    /// Navigate forward in history. Returns `true` if navigation occurred.
    pub fn forward(&mut self) -> bool {
        if let Some(location) = self.navigator.forward() {
            if let Some(content) = self.content_map.get(location).cloned() {
                self.apply_content_update(content);
                return true;
            }
        }
        false
    }

    fn follow_link(&mut self, href: &str) -> bool {
        let href = href.trim();
        let path_part = href.split('#').next().unwrap_or_default().trim();
        if path_part.is_empty() {
            return false;
        }

        let mut candidates = vec![path_part.to_string()];
        if let Some(stripped) = path_part.strip_prefix("./") {
            candidates.push(stripped.to_string());
        }

        for candidate in candidates {
            if self.content_map.contains_key(&candidate) && self.go(candidate) {
                return true;
            }
        }
        false
    }

    /// Extract headings from the current content.
    pub fn extract_headings(&self) -> Vec<(usize, String)> {
        Self::parse_headings(&self.content)
            .into_iter()
            .map(|(level, title, _)| (level, title))
            .collect()
    }

    fn apply_content_update(&mut self, content: String) {
        self.content = content;
        if let Ok(mut shared) = self.shared_markup.write() {
            *shared = self.content.clone();
        }
        let headings = Self::parse_headings(&self.content);
        if let Ok(mut shared_headings) = self.shared_headings.write() {
            *shared_headings = headings;
        }
        self.toc_dirty = true;
    }

    fn flush_toc_message(&mut self, ctx: &mut EventCtx) {
        // Flush any pending TOC class change into the arena node record.
        if let Some(show) = self.toc_class_pending.take() {
            const CLASS: &str = "-show-table-of-contents";
            ctx.set_class(show, CLASS);
        }
        if !self.toc_dirty {
            return;
        }
        let headings = self
            .shared_headings
            .read()
            .ok()
            .map(|h| h.clone())
            .unwrap_or_default();
        ctx.post_message(MarkdownTableOfContentsUpdated { headings });
        self.toc_dirty = false;
    }

    /// Compute the approximate line offset for a heading block id in the content.
    fn heading_line_offset(&self, block_id: &str) -> usize {
        let viewport_width = self
            .inner
            .scroll_viewport_size()
            .map(|(w, _)| w)
            .unwrap_or(80)
            .max(1);

        let toc_width = if self.is_showing_table_of_contents() {
            MarkdownTableOfContents::with_shared_headings(self.shared_headings.clone())
                .content_width()
                .unwrap_or(0)
        } else {
            0
        };

        let markdown_width = viewport_width.saturating_sub(toc_width).max(1);
        // Default Markdown CSS has left/right padding of 2 cells.
        let content_width = markdown_width.saturating_sub(4).max(1);

        let headings = Self::parse_heading_lines(&self.content);
        let mut by_source_line: HashMap<usize, (usize, String)> = HashMap::new();
        for (level, _title, id, source_line) in &headings {
            by_source_line.insert(*source_line, (*level, id.clone()));
        }

        let mut visual_row = 0usize;
        for (source_line, line) in self.content.lines().enumerate() {
            let wraps = rich_rs::cell_len(line).div_ceil(content_width).max(1);
            if let Some((level, id)) = by_source_line.get(&source_line) {
                let (top, bottom) = heading_margins(*level);
                if id == block_id {
                    // Python parity: `scroll_to_widget(..., top=True)` aligns to the
                    // heading block region (which includes heading top margin). Our
                    // source-line approximation compensates by backing off one context
                    // row plus the heading top margin so the viewport lands just before
                    // the heading text, matching Python's visual position.
                    return visual_row.saturating_sub(1 + top);
                }
                visual_row = visual_row
                    .saturating_add(top)
                    .saturating_add(wraps)
                    .saturating_add(bottom);
            } else {
                visual_row = visual_row.saturating_add(wraps);
            }
        }
        0
    }

    fn parse_headings(content: &str) -> Vec<HeadingEntry> {
        parse_markdown_heading_lines(content)
            .into_iter()
            .map(|(level, title, block_id, _)| (level, title, block_id))
            .collect()
    }

    fn parse_heading_lines(content: &str) -> Vec<(usize, String, String, usize)> {
        parse_markdown_heading_lines(content)
    }
}

fn slugify_heading(title: &str) -> String {
    let mut slug = String::new();
    let mut prev_dash = false;
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if ch == '_' || ch == '-' {
            if !prev_dash && !slug.is_empty() {
                slug.push(ch);
                prev_dash = true;
            }
        } else if ch.is_ascii_whitespace() && !prev_dash && !slug.is_empty() {
            slug.push('-');
            prev_dash = true;
        }
    }
    let slug = slug.trim_end_matches('-').to_string();
    if slug.is_empty() {
        "section".to_string()
    } else {
        slug
    }
}

fn heading_margins(level: usize) -> (usize, usize) {
    if level <= 2 { (2, 1) } else { (1, 1) }
}

pub(crate) fn parse_markdown_heading_lines(content: &str) -> Vec<(usize, String, String, usize)> {
    let mut out = Vec::new();
    let mut slug_counts: HashMap<String, usize> = HashMap::new();
    for (marker_len, title, line_idx) in parse_markdown_headings_with_lines(content) {
        let base = slugify_heading(&title);
        let seen = slug_counts.entry(base.clone()).or_insert(0);
        let block_id = if *seen == 0 {
            base
        } else {
            format!("{base}-{}", *seen)
        };
        *seen += 1;
        out.push((marker_len, title, block_id, line_idx));
    }
    out
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

    fn drain_pending_class_ops(&mut self) -> Vec<(String, bool)> {
        if let Some(show) = self.toc_class_pending.take() {
            vec![("-show-table-of-contents".to_string(), show)]
        } else {
            Vec::new()
        }
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        let mut seed = std::mem::take(&mut self.seed);
        // Push runtime-accumulated classes into the seed so the tree node gets them at mount.
        for class in &self.classes {
            if !seed.classes.contains(class) {
                seed.classes.push(class.clone());
            }
        }
        seed
    }

    fn focusable(&self) -> bool {
        false
    }

    fn can_focus(&self) -> bool {
        false
    }

    fn can_focus_children(&self) -> bool {
        true
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.flush_toc_message(ctx);
        self.inner.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.flush_toc_message(ctx);
        self.inner.on_event(event, ctx);
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        self.flush_toc_message(ctx);
        if let Some(m) = message.downcast_ref::<MarkdownTableOfContentsUpdated>() {
            if let Ok(mut shared_headings) = self.shared_headings.write() {
                *shared_headings = m.headings.clone();
            }
            // MarkdownViewer docks TOC with `width:auto`; heading updates must trigger
            // a relayout so the dock width tracks the rebuilt TOC tree width.
            ctx.request_layout_invalidation();
            ctx.request_repaint();
            ctx.set_handled();
            return;
        }
        // Handle TOC heading selection: scroll to the heading in the document.
        if let Some(m) = message.downcast_ref::<MarkdownTableOfContentsSelected>() {
            let block_id = m.block_id.clone();
            let target_line = self.heading_line_offset(&block_id);
            // Python `scroll_to_widget(..., top=True)` defaults to a fixed 0.2s
            // duration when no explicit speed/duration is provided.
            let scroll_duration = Some(Duration::from_millis(200));
            ctx.post_message(ScrollbarScrollTo {
                axis: ScrollbarAxis::Vertical,
                offset: target_line as f32,
                animate: true,
                scroll_duration,
            });
            ctx.set_handled();
            return;
        }
        self.inner.on_message(message, ctx);
    }

    fn action_namespace(&self) -> &str {
        "markdown_viewer"
    }

    fn action_registry(&self) -> &[ActionDecl] {
        MARKDOWN_VIEWER_ACTIONS
    }

    fn execute_action(&mut self, action: &ParsedAction, ctx: &mut EventCtx) -> bool {
        if action.name == "link"
            && let Some(href) = action.arguments.first()
            && self.follow_link(href)
        {
            ctx.post_message(NavigatorUpdated);
            ctx.request_layout_invalidation();
            ctx.request_repaint();
            return true;
        }
        self.inner.execute_action(action, ctx)
    }

    // delegate-audit: 67 methods as of 2026-02-26
    delegate_widget_method!(
        inner,
        [
            render,
            render_with_debug,
            render_line,
            render_lines,
            compose,
            on_mount,
            on_unmount,
            on_tick,
            on_resize,
            on_layout,
            set_virtual_content_size,
            on_mouse_scroll,
            on_mouse_move,
            on_app_key,
            on_app_action,
            on_app_message,
            on_app_tick,
            on_app_mount,
            scroll_offset,
            scroll_offset_f32,
            scroll_viewport_size,
            scroll_virtual_content_size,
            clips_descendants_to_content,
            child_display_for_tree,
            tree_child_content_inset,
            layout_height,
            content_width,
            preserve_underlay,
            bindings,
            binding_hints,
            style_type_aliases,
            border_title,
            border_subtitle,
            is_active,
            mouse_interactive,
            tooltip,
            tooltip_anchor,
            help_markup,
            allow_select,
            selection_at,
            selection_word_range_at,
            selection_all_range,
            update_selection,
            clear_selection,
            get_selection,
            selection_updated,
            reactive_widget,
        ]
    );
}

delegate_renderable!(MarkdownViewer);

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
        // compose() should return user children + scrollbar widgets.
        let mut viewer = MarkdownViewer::new("# Test");
        let children = viewer.compose();
        // At minimum: Markdown, MarkdownTableOfContents, + scrollbar widgets.
        assert!(
            children.len() >= 2,
            "expected at least 2 children (Markdown + TOC), got {}",
            children.len()
        );
        // First child should be Markdown (or its contents from flattening).
        // Scrollbar widgets should be present.
        let has_scrollbar = children.iter().any(|c| {
            let st = c.widget().style_type();
            st.contains("Scrollbar") || st.contains("ScrollBar")
        });
        assert!(
            has_scrollbar || children.len() >= 3,
            "expected scrollbar widgets in children"
        );
    }

    #[test]
    fn markdown_viewer_composes_focusable_markdown_document_child() {
        let mut viewer = MarkdownViewer::new("# Test");
        let children = viewer.compose();
        let markdown_child = children
            .iter()
            .find(|child| child.widget().style_type() == "Markdown")
            .expect("expected Markdown child in MarkdownViewer composition");
        assert!(
            markdown_child.widget().focusable(),
            "MarkdownViewer should compose a focusable Markdown child (Python parity)"
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
            viewer
                .classes
                .iter()
                .any(|c| c == "-show-table-of-contents"),
            "expected -show-table-of-contents class"
        );
    }

    #[test]
    fn markdown_viewer_toggle_toc_removes_class() {
        let mut viewer = MarkdownViewer::new("# Test");
        viewer.set_show_table_of_contents(false);
        assert!(
            !viewer
                .classes
                .iter()
                .any(|c| c == "-show-table-of-contents"),
            "class should be removed when TOC is hidden"
        );
    }

    #[test]
    fn toc_set_headings_updates_content() {
        let mut toc = MarkdownTableOfContents::new(vec![(1, "H1".to_string(), "h1".to_string())]);
        toc.set_headings(vec![
            (1, "H1".to_string(), "h1".to_string()),
            (2, "H2".to_string(), "h2".to_string()),
        ]);
        let headings = toc.shared_headings.read().unwrap().clone();
        assert_eq!(headings.len(), 2);
    }

    #[test]
    fn toc_compose_returns_tree_child() {
        let mut toc = MarkdownTableOfContents::new(vec![
            (1, "Chapter".to_string(), "chapter".to_string()),
            (2, "Section".to_string(), "section".to_string()),
        ]);
        let children = toc.compose();
        assert!(
            children.len() == 1,
            "TOC should compose exactly one Tree child, got {}",
            children.len()
        );
        match &children[0].builder {
            crate::compose::WidgetBuilder::Ready(widget) => {
                assert_eq!(widget.style_type(), "Tree");
            }
        }
    }

    #[test]
    fn toc_child_tree_css_padding_and_bg_resolve_with_parent_context() {
        let _guard = crate::css::set_style_context(crate::css::default_widget_stylesheet());
        let toc = MarkdownTableOfContents::new(vec![(1, "H1".to_string(), "h1".to_string())]);
        let tree = MarkdownTableOfContentsTree::with_shared_headings(toc.shared_headings.clone());
        let toc_meta = crate::css::selector_meta_generic(&toc);
        let toc_resolved = crate::css::resolve_style(&toc, &toc_meta);
        let tree_meta = crate::css::selector_meta_generic(&tree);
        let tree_resolved = crate::css::with_style_stack(toc_meta, toc_resolved, || {
            crate::css::resolve_style(&tree, &tree_meta)
        });
        let padding = tree_resolved.effective_padding();
        assert_eq!(
            padding,
            crate::style::Spacing::all(1),
            "MarkdownTableOfContents > Tree should resolve padding: 1 from default CSS"
        );
        assert!(
            tree_resolved.bg.is_some(),
            "MarkdownTableOfContents > Tree should resolve background from default CSS"
        );
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
    fn viewer_link_action_resolves_relative_registered_path() {
        let mut viewer = MarkdownViewer::new("initial");
        viewer.register_content("demo.md", "# Demo");
        viewer.register_content("example.md", "# Example");
        assert!(viewer.go("demo.md"));

        let action =
            crate::action::parse_action("link('./example.md')").expect("link action should parse");
        let mut ctx = EventCtx::default();
        assert!(viewer.execute_action(&action, &mut ctx));
        assert_eq!(viewer.content, "# Example");
        assert!(
            ctx.take_messages()
                .into_iter()
                .any(|msg| msg.is::<NavigatorUpdated>()),
            "link navigation should emit NavigatorUpdated"
        );
    }

    #[test]
    fn toc_tree_hides_root() {
        let toc = MarkdownTableOfContents::new(vec![(1, "H1".to_string(), "h1".to_string())]);
        let headings = toc.shared_headings.read().unwrap().clone();
        let tree = MarkdownTableOfContents::build_tree_from_headings(&headings);
        assert!(!tree.showing_root());
    }

    #[test]
    fn toc_tree_parent_nodes_start_expanded() {
        let toc = MarkdownTableOfContents::new(vec![
            (1, "Chapter".to_string(), "chapter".to_string()),
            (2, "Section".to_string(), "section".to_string()),
        ]);
        let headings = toc.shared_headings.read().unwrap().clone();
        let tree = MarkdownTableOfContents::build_tree_from_headings(&headings);
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
    fn toc_composed_tree_defers_intrinsic_width_to_toc_wrapper() {
        let toc = MarkdownTableOfContents::new(vec![
            (
                1,
                "A Long Chapter Title".to_string(),
                "a-long-chapter-title".to_string(),
            ),
            (2, "Section".to_string(), "section".to_string()),
        ]);
        let tree = MarkdownTableOfContentsTree::with_shared_headings(toc.shared_headings.clone());
        let w = tree.content_width();
        assert_eq!(
            w, None,
            "Composed TOC Tree should fill parent pane; TOC wrapper computes intrinsic width"
        );
    }

    #[test]
    fn toc_wrapper_reports_intrinsic_width_for_dock_auto_layout() {
        let _guard = crate::css::set_style_context(crate::css::default_widget_stylesheet());
        let toc = MarkdownTableOfContents::new(vec![
            (
                1,
                "A Long Chapter Title".to_string(),
                "a-long-chapter-title".to_string(),
            ),
            (2, "Section".to_string(), "section".to_string()),
        ]);
        let w = toc.content_width();
        assert!(
            w.is_some() && w.unwrap() > 10,
            "TOC wrapper should expose intrinsic width so dock:auto doesn't consume full viewport"
        );
    }

    #[test]
    fn tree_node_data_carries_heading_block_id() {
        let toc = MarkdownTableOfContents::new(vec![
            (1, "Chapter".to_string(), "chapter".to_string()),
            (2, "Section".to_string(), "section".to_string()),
        ]);
        let headings = toc.shared_headings.read().unwrap().clone();
        let tree = MarkdownTableOfContents::build_tree_from_headings(&headings);
        if let Some(root) = tree.root() {
            let h1 = root.children_slice();
            assert!(!h1.is_empty());
            assert_eq!(h1[0].data(), Some("chapter"));
            let h2_children = h1[0].children_slice();
            assert!(!h2_children.is_empty());
            assert_eq!(h2_children[0].data(), Some("section"));
        }
    }

    #[test]
    fn heading_line_offset_finds_visual_row_by_block_id() {
        let content = "Some preamble\n\n# First Heading\n\nText\n\n## Second Heading\n";
        let mut viewer = MarkdownViewer::new(content);
        viewer.on_layout(80, 24);
        let first = viewer.heading_line_offset("first-heading");
        let second = viewer.heading_line_offset("second-heading");
        assert!(first < second);
        assert_eq!(first, 0);
        assert_eq!(second, 6);
    }

    #[test]
    fn toc_on_message_posts_toc_selected() {
        let mut toc =
            MarkdownTableOfContents::new(vec![(1, "Chapter".to_string(), "chapter".to_string())]);
        let msg = MessageEvent::new(
            crate::node_id::NodeId::default(),
            TreeNodeActivated {
                index: 0,
                label: "Chapter".to_string(),
                data: Some("chapter".to_string()),
            },
        );
        let mut ctx = crate::event::EventCtx::default();
        toc.on_message(&msg, &mut ctx);
        assert!(ctx.handled());
        let messages = ctx.take_messages();
        assert!(
            messages.iter().any(|m| m
                .downcast_ref::<MarkdownTableOfContentsSelected>()
                .map_or(false, |s| s.block_id == "chapter")),
            "TOC should post MarkdownTableOfContentsSelected with block_id"
        );
    }

    #[test]
    fn toc_on_message_updated_requests_layout_invalidation() {
        let mut toc =
            MarkdownTableOfContents::new(vec![(1, "Chapter".to_string(), "chapter".to_string())]);
        let msg = MessageEvent::new(
            crate::node_id::NodeId::default(),
            MarkdownTableOfContentsUpdated {
                headings: vec![
                    (1, "Chapter".to_string(), "chapter".to_string()),
                    (2, "Section".to_string(), "section".to_string()),
                ],
            },
        );
        let mut ctx = crate::event::EventCtx::default();
        toc.on_message(&msg, &mut ctx);
        assert!(
            ctx.invalidation().layout,
            "TOC updates should invalidate layout so dock:auto width can grow"
        );
    }

    #[test]
    fn toc_on_selected_does_not_post_toc_selected() {
        let mut toc =
            MarkdownTableOfContents::new(vec![(1, "Chapter".to_string(), "chapter".to_string())]);
        let msg = MessageEvent::new(
            crate::node_id::NodeId::default(),
            crate::message::TreeNodeSelected {
                index: 0,
                label: "Chapter".to_string(),
                data: Some("chapter".to_string()),
            },
        );
        let mut ctx = crate::event::EventCtx::default();
        toc.on_message(&msg, &mut ctx);
        assert!(!ctx.handled());
        assert!(ctx.take_messages().is_empty());
    }

    #[test]
    fn parse_headings_generates_stable_slug_ids() {
        let headings = MarkdownViewer::parse_headings("# Hello World\n## Hello World\n## !!!\n");
        assert_eq!(headings[0].2, "hello-world");
        assert_eq!(headings[1].2, "hello-world-1");
        assert_eq!(headings[2].2, "section");
    }

    #[test]
    fn toc_tree_width_handles_long_h2_titles() {
        let content = "# Markdown Viewer\n\n## Features\n\n## Tables\n\n## Code Blocks\n\n## Litany Against Fear\n";
        let headings = MarkdownViewer::parse_headings(content);
        let tree = MarkdownTableOfContents::build_tree_from_headings(&headings);
        let expected = rich_rs::cell_len("└── Ⅱ Litany Against Fear");
        assert_eq!(tree.content_width(), Some(expected.max(1)));
    }

    #[test]
    fn viewer_toc_update_requests_layout_invalidation() {
        let mut viewer = MarkdownViewer::new("# Chapter");
        let msg = MessageEvent::new(
            crate::node_id::NodeId::default(),
            MarkdownTableOfContentsUpdated {
                headings: vec![
                    (1, "Chapter".to_string(), "chapter".to_string()),
                    (2, "Section".to_string(), "section".to_string()),
                    (
                        2,
                        "Litany Against Fear".to_string(),
                        "litany-against-fear".to_string(),
                    ),
                ],
            },
        );
        let mut ctx = crate::event::EventCtx::default();
        viewer.on_message(&msg, &mut ctx);
        assert!(
            ctx.invalidation().layout,
            "MarkdownViewer must invalidate layout when TOC headings change"
        );
    }

    #[test]
    fn viewer_toc_selected_posts_scrollbar_scroll_to() {
        let mut viewer = MarkdownViewer::new("# First\n\n## Second");
        // Ensure heading offsets are initialized from current content/layout assumptions.
        viewer.on_layout(80, 24);

        let msg = MessageEvent::new(
            crate::node_id::NodeId::default(),
            MarkdownTableOfContentsSelected {
                block_id: "second".to_string(),
            },
        );
        let mut ctx = crate::event::EventCtx::default();
        viewer.on_message(&msg, &mut ctx);
        assert!(ctx.handled());
        let messages = ctx.take_messages();
        assert!(
            messages.iter().any(|m| m
                .downcast_ref::<ScrollbarScrollTo>()
                .map_or(false, |p| p.axis == ScrollbarAxis::Vertical && p.animate)),
            "TOC selection should route through ScrollbarScrollTo for synchronized content+thumb scroll"
        );
    }

    // ── Shared markup + content propagation tests ─────────────────────────

    #[test]
    fn shared_markup_syncs_on_layout() {
        let shared = Arc::new(RwLock::new("# Hello".to_string()));
        let mut md = Markdown::with_shared_markup(shared.clone());
        let initial_height = md.layout_height().unwrap_or_default();
        assert!(initial_height > 0);

        // Update shared content to something taller.
        *shared.write().unwrap() = "# Line1\n\n# Line2\n\n# Line3".to_string();

        // Before on_layout, markup is still old.
        assert_eq!(md.layout_height(), Some(initial_height));
        assert_eq!(md.extract_headings().len(), 1);

        // on_layout triggers sync.
        md.on_layout(40, 10);
        assert_eq!(md.extract_headings().len(), 3);
        assert!(md.layout_height().unwrap_or_default() > 0);
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
    fn viewer_go_updates_shared_headings() {
        let mut viewer = MarkdownViewer::new("initial");
        viewer.register_content("demo.md", "# Demo\n\n## Child");

        viewer.go("demo.md");

        let headings = viewer.shared_headings.read().unwrap().clone();
        assert_eq!(headings.len(), 2);
        assert_eq!(headings[0].2, "demo");
        assert_eq!(headings[1].2, "child");
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
