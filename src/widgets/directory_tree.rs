use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::event::{Event, EventCtx, MouseDownEvent};
use crate::message::*;
use crate::widgets::delegate::{delegate_renderable, delegate_widget_method};

use crate::node_id::NodeId;

use super::{NodeSeed, Tree, TreeNode, Widget};

/// Icon for an expanded folder (Python: `ICON_NODE_EXPANDED`).
const ICON_FOLDER_OPEN: &str = "📂 ";
/// Icon for a collapsed folder (Python: `ICON_NODE`).
const ICON_FOLDER: &str = "📁 ";
/// Icon for a file (Python: `ICON_FILE`).
const ICON_FILE: &str = "📄 ";

#[derive(Debug, Clone)]
struct DirectoryNode {
    path: PathBuf,
    label: String,
    is_dir: bool,
    expanded: bool,
    loaded: bool,
    children: Vec<DirectoryNode>,
}

impl DirectoryNode {
    fn from_path(path: PathBuf) -> Self {
        let is_dir = path.is_dir();
        // Match Python's `DirectoryTree`, whose root label is `path.name`
        // (the basename). For a path like `"./"` the basename is empty, so the
        // root renders as just the folder emoji rather than the full path text.
        let label = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.to_string())
            .unwrap_or_default();
        Self {
            path,
            label,
            is_dir,
            expanded: is_dir,
            loaded: !is_dir,
            children: Vec::new(),
        }
    }

    fn to_tree_node(&self) -> TreeNode {
        // Prepend directory/file icon to the label, matching Python's render_label().
        let icon_label = if self.is_dir {
            if self.expanded {
                format!("{}{}", ICON_FOLDER_OPEN, self.label)
            } else {
                format!("{}{}", ICON_FOLDER, self.label)
            }
        } else {
            format!("{}{}", ICON_FILE, self.label)
        };
        let mut node = TreeNode::new(icon_label)
            .expanded(self.expanded)
            .allow_expand(self.is_dir);

        // Component classes (QW-24)
        if self.is_dir {
            node = node.with_component_class("directory-tree--folder");
        } else {
            node = node.with_component_class("directory-tree--file");
            if let Some(ext) = Path::new(&self.label).extension().and_then(|e| e.to_str()) {
                node = node.with_component_class("directory-tree--extension");
                node = node.with_component_class(format!(
                    "directory-tree--extension-{}",
                    ext.to_lowercase()
                ));
            }
        }
        if self.label.starts_with('.') {
            node = node.with_component_class("directory-tree--hidden");
        }

        for child in &self.children {
            node = node.with_child(child.to_tree_node());
        }
        node
    }
}

#[derive(Debug, Clone)]
struct VisibleEntry {
    path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct DirectoryTree {
    root_path: PathBuf,
    root: DirectoryNode,
    tree: Tree,
    visible_entries: Vec<VisibleEntry>,
    inflight_loads_by_path: HashMap<PathBuf, u64>,
    inflight_loads_by_task: HashMap<u64, PathBuf>,
    next_task_id: u64,
    show_hidden: bool,
    /// Optional path filter predicate; paths for which the predicate returns false are excluded.
    filter: Option<fn(&Path) -> bool>,
    last_width: u16,
    last_height: u16,
    pub(crate) seed: NodeSeed,
}

impl DirectoryTree {
    crate::seed_ident_methods!();

    pub fn new(path: impl Into<PathBuf>) -> Self {
        let root_path = path.into();
        let show_hidden = false;
        let filter: Option<fn(&Path) -> bool> = None;
        let root = build_root(
            root_path.clone(),
            show_hidden,
            true,
            &HashSet::new(),
            filter,
        );

        let mut tree = Tree::new(vec![root.to_tree_node()]);
        // DirectoryTree replaces the twisty with the folder/file emoji prefix
        // (see Python `DirectoryTree.render_label`).
        tree.set_hide_twisty(true);
        tree.on_layout(1, 1);

        let mut this = Self {
            root_path,
            root,
            tree,
            visible_entries: Vec::new(),
            inflight_loads_by_path: HashMap::new(),
            inflight_loads_by_task: HashMap::new(),
            next_task_id: 1,
            show_hidden,
            filter,
            last_width: 1,
            last_height: 1,
            seed: NodeSeed::default(),
        };
        this.rebuild_tree(None);
        this
    }

    pub fn show_hidden(mut self, show_hidden: bool) -> Self {
        self.show_hidden = show_hidden;
        self.refresh();
        self
    }

    pub fn set_show_hidden(&mut self, show_hidden: bool) {
        if self.show_hidden == show_hidden {
            return;
        }
        self.show_hidden = show_hidden;
        self.refresh();
    }

    pub fn showing_hidden(&self) -> bool {
        self.show_hidden
    }

    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    pub fn tree_id(&self) -> NodeId {
        self.node_id()
    }

    pub fn selected_path(&self) -> Option<&Path> {
        self.visible_entries
            .get(self.tree.selected())
            .map(|entry| entry.path.as_path())
    }

    pub fn refresh(&mut self) {
        let selected = self.selected_path().map(Path::to_path_buf);
        let mut expanded_paths = HashSet::new();
        collect_expanded_paths(&self.root, &mut expanded_paths);
        self.inflight_loads_by_path.clear();
        self.inflight_loads_by_task.clear();
        let root = build_root(
            self.root_path.clone(),
            self.show_hidden,
            self.root.expanded,
            &expanded_paths,
            self.filter,
        );
        self.root = root;
        self.rebuild_tree(selected);
    }

    fn rebuild_tree(&mut self, preferred_path: Option<PathBuf>) {
        self.visible_entries.clear();
        collect_visible_entries(&self.root, &mut self.visible_entries);

        let mut tree = Tree::new(vec![self.root.to_tree_node()]);
        tree.set_hide_twisty(true);
        tree.on_layout(self.last_width, self.last_height);

        if let Some(path) = preferred_path {
            if let Some(index) = self
                .visible_entries
                .iter()
                .position(|entry| entry.path == path)
            {
                // Use set_selected with throwaway ctx (tree is pre-mount, changes discarded).
                let mut rctx = crate::reactive::ReactiveCtx::new(crate::node_id::NodeId::default());
                tree.set_selected(index, &mut rctx);
            }
        }

        self.tree = tree;
    }

    fn update_node_expanded_state(&mut self, index: usize, expanded: bool, ctx: &mut EventCtx) {
        let Some(entry) = self.visible_entries.get(index).cloned() else {
            return;
        };

        let mut queue_load_for: Option<PathBuf> = None;
        let mut cancel_pending_for: Option<PathBuf> = None;
        if let Some(node) = find_node_mut(&mut self.root, &entry.path) {
            node.expanded = expanded;
            if node.is_dir {
                if expanded && !node.loaded {
                    queue_load_for = Some(node.path.clone());
                } else if !expanded {
                    node.children.clear();
                    node.loaded = false;
                    cancel_pending_for = Some(node.path.clone());
                }
            }
        }
        if let Some(path) = cancel_pending_for.as_deref() {
            self.cancel_inflight_loads_for(path, ctx);
        }
        if let Some(path) = queue_load_for.as_deref() {
            self.spawn_directory_load(path, ctx);
        }

        self.rebuild_tree(Some(entry.path));
    }

    fn spawn_directory_load(&mut self, path: &Path, ctx: &mut EventCtx) {
        if self.inflight_loads_by_path.contains_key(path) {
            return;
        }
        let path_buf = path.to_path_buf();
        let task_id = self.next_task_id;
        self.next_task_id += 1;
        self.inflight_loads_by_path
            .insert(path_buf.clone(), task_id);
        self.inflight_loads_by_task
            .insert(task_id, path_buf.clone());
        ctx.post_message(AsyncTaskSpawn {
            task_id,
            target: self.node_id(),
            request: AsyncTaskRequest::ReadDirectory {
                path: path_buf.display().to_string(),
                show_hidden: self.show_hidden,
            },
        });
    }

    fn cancel_inflight_loads_for(&mut self, path: &Path, ctx: &mut EventCtx) {
        let task_ids = self
            .inflight_loads_by_path
            .iter()
            .filter_map(|(pending_path, task_id)| {
                is_same_or_descendant(pending_path, path).then_some(*task_id)
            })
            .collect::<Vec<_>>();

        for task_id in task_ids {
            if let Some(pending_path) = self.inflight_loads_by_task.remove(&task_id) {
                self.inflight_loads_by_path.remove(&pending_path);
            }
            ctx.post_message(AsyncTaskCancel { task_id });
        }
    }

    fn apply_directory_load_result(
        &mut self,
        task_id: u64,
        result: &AsyncTaskResult,
        ctx: &mut EventCtx,
    ) {
        let Some(path) = self.inflight_loads_by_task.remove(&task_id) else {
            return;
        };
        self.inflight_loads_by_path.remove(&path);

        match result {
            AsyncTaskResult::DirectoryEntries { entries, .. } => {
                let selected = self.selected_path().map(Path::to_path_buf);
                if let Some(node) = find_node_mut(&mut self.root, &path) {
                    if node.is_dir && node.expanded && !node.loaded {
                        node.children = entries
                            .iter()
                            .filter(|e| self.filter.is_none_or(|pred| pred(Path::new(&e.path))))
                            .map(directory_node_from_async_entry)
                            .collect::<Vec<_>>();
                        node.loaded = true;
                        self.rebuild_tree(selected);
                        ctx.request_repaint();
                    }
                }
            }
            AsyncTaskResult::Failed { .. } => {
                if let Some(node) = find_node_mut(&mut self.root, &path) {
                    node.children.clear();
                    node.loaded = false;
                }
            }
            AsyncTaskResult::SleepFinished { .. } => {
                // DirectoryTree only consumes directory-entry task payloads.
            }
        }
    }

    fn clear_inflight_task(&mut self, task_id: u64) {
        let Some(path) = self.inflight_loads_by_task.remove(&task_id) else {
            return;
        };
        self.inflight_loads_by_path.remove(&path);
    }

    // ── QW-25: DirectoryTree APIs ────────────────────────────────────────

    /// Set a path filter predicate. Paths for which the predicate returns `false`
    /// are excluded from the tree. Call [`refresh`](Self::refresh) after to apply.
    pub fn filter_paths(&mut self, predicate: fn(&Path) -> bool) {
        self.filter = Some(predicate);
        self.refresh();
    }

    /// Remove the path filter, showing all paths again.
    pub fn clear_filter(&mut self) {
        if self.filter.is_some() {
            self.filter = None;
            self.refresh();
        }
    }

    /// Reload a specific directory node's children. If the node at `node_index`
    /// is a directory and was expanded, its children are cleared and re-read
    /// (spawning an async load if needed).
    pub fn reload_node(&mut self, node_index: usize, ctx: &mut EventCtx) {
        let Some(entry) = self.visible_entries.get(node_index) else {
            return;
        };
        let entry_path = entry.path.clone();

        let was_expanded = {
            let Some(node) = find_node_mut(&mut self.root, &entry_path) else {
                return;
            };
            if !node.is_dir {
                return;
            }
            let expanded = node.expanded;
            node.children.clear();
            node.loaded = false;
            expanded
        };

        self.rebuild_tree(Some(entry_path.clone()));
        if was_expanded {
            self.spawn_directory_load(&entry_path, ctx);
        }
    }
}

impl Widget for DirectoryTree {
    fn focusable(&self) -> bool {
        true
    }

    fn on_node_state_changed(
        &mut self,
        _old: crate::widgets::NodeState,
        new: crate::widgets::NodeState,
    ) {
        // Propagate hover state changes to the inner Tree so hover highlighting clears
        // when DirectoryTree loses hover.
        if !new.hovered {
            self.tree.on_node_state_changed(_old, new);
        }
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.last_width = width.max(1);
        self.last_height = height.max(1);
        self.tree.on_layout(self.last_width, self.last_height);
    }

    fn on_unmount(&mut self) {
        self.inflight_loads_by_path.clear();
        self.inflight_loads_by_task.clear();
        self.tree.on_unmount();
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.last_width = width.max(1);
        self.last_height = height.max(1);
        self.tree.on_resize(self.last_width, self.last_height);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::MouseDown(mouse) if mouse.target == self.node_id() => {
                self.tree.on_event(
                    &Event::MouseDown(MouseDownEvent {
                        target: self.node_id(),
                        screen_x: mouse.screen_x,
                        screen_y: mouse.screen_y,
                        x: mouse.x,
                        y: mouse.y,
                    }),
                    ctx,
                );
            }
            _ => self.tree.on_event(event, ctx),
        }
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Some(m) = message.downcast_ref::<AsyncTaskCompleted>() {
            if m.target == self.node_id() {
                self.apply_directory_load_result(m.task_id, &m.result, ctx);
                ctx.set_handled();
                return;
            }
        }
        if let Some(m) = message.downcast_ref::<AsyncTaskCancelled>() {
            if m.target == self.node_id() {
                self.clear_inflight_task(m.task_id);
                ctx.set_handled();
                return;
            }
        }
        if let Some(m) = message.downcast_ref::<TreeNodeSelected>() {
            if message.sender != self.node_id() {
                return;
            }
            if let Some(entry) = self.visible_entries.get(m.index) {
                let path = entry.path.display().to_string();
                if entry.path.is_dir() {
                    ctx.post_message(DirectoryTreeDirectorySelected {
                        index: m.index,
                        path,
                    });
                } else {
                    ctx.post_message(DirectoryTreeFileSelected {
                        index: m.index,
                        path,
                    });
                }
                ctx.set_handled();
            }
            return;
        }
        if let Some(m) = message.downcast_ref::<TreeNodeToggled>() {
            if message.sender != self.node_id() {
                return;
            }
            self.update_node_expanded_state(m.index, m.expanded, ctx);
            ctx.request_repaint();
            ctx.set_handled();
        }
    }

    fn layout_height(&self) -> Option<usize> {
        self.tree.layout_height()
    }

    fn content_width(&self) -> Option<usize> {
        let content_width = self.tree.content_width().unwrap_or(1);
        let meta = crate::css::selector_meta_generic(self);
        let resolved = crate::css::resolve_style(self, &meta);
        let padding = resolved.effective_padding();
        let (_, _, border_left, border_right) =
            super::helpers::border_spacing_from_style(&resolved);
        let chrome_lr =
            usize::from(padding.left.saturating_add(padding.right)) + border_left + border_right;
        Some(content_width.saturating_add(chrome_lr).max(1))
    }

    fn style_type(&self) -> &'static str {
        "DirectoryTree"
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
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
            take_composed_children,
            can_focus,
            can_focus_children,
            on_mount,
            on_tick,
            set_virtual_content_size,
            on_event_capture,
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

delegate_renderable!(DirectoryTree);

fn read_children(
    path: &Path,
    show_hidden: bool,
    filter: Option<fn(&Path) -> bool>,
) -> Vec<DirectoryNode> {
    let mut entries = Vec::new();
    let Ok(read_dir) = fs::read_dir(path) else {
        return entries;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let label = name.to_string();
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        if filter.is_some_and(|pred| !pred(&path)) {
            continue;
        }

        let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        entries.push(DirectoryNode {
            path,
            label,
            is_dir,
            expanded: false,
            loaded: !is_dir,
            children: Vec::new(),
        });
    }

    entries.sort_by(|left, right| {
        right
            .is_dir
            .cmp(&left.is_dir)
            .then_with(|| left.label.to_lowercase().cmp(&right.label.to_lowercase()))
            .then_with(|| left.label.cmp(&right.label))
    });

    entries
}

fn directory_node_from_async_entry(entry: &AsyncDirectoryEntry) -> DirectoryNode {
    DirectoryNode {
        path: PathBuf::from(&entry.path),
        label: entry.label.clone(),
        is_dir: entry.is_dir,
        expanded: false,
        loaded: !entry.is_dir,
        children: Vec::new(),
    }
}

fn build_root(
    root_path: PathBuf,
    show_hidden: bool,
    expanded: bool,
    expanded_paths: &HashSet<PathBuf>,
    filter: Option<fn(&Path) -> bool>,
) -> DirectoryNode {
    let mut root = DirectoryNode::from_path(root_path);
    if !root.is_dir {
        return root;
    }
    root.expanded = expanded;
    populate_expanded_children(&mut root, show_hidden, expanded_paths, filter);
    root
}

fn populate_expanded_children(
    node: &mut DirectoryNode,
    show_hidden: bool,
    expanded_paths: &HashSet<PathBuf>,
    filter: Option<fn(&Path) -> bool>,
) {
    if !node.is_dir {
        return;
    }

    if !node.expanded {
        node.loaded = false;
        node.children.clear();
        return;
    }

    node.children = read_children(&node.path, show_hidden, filter);
    node.loaded = true;
    for child in &mut node.children {
        if child.is_dir {
            child.expanded = expanded_paths.contains(&child.path);
            populate_expanded_children(child, show_hidden, expanded_paths, filter);
        }
    }
}

fn collect_expanded_paths(node: &DirectoryNode, out: &mut HashSet<PathBuf>) {
    if !node.is_dir || !node.expanded {
        return;
    }
    out.insert(node.path.clone());
    for child in &node.children {
        collect_expanded_paths(child, out);
    }
}

fn collect_visible_entries(node: &DirectoryNode, out: &mut Vec<VisibleEntry>) {
    out.push(VisibleEntry {
        path: node.path.clone(),
    });

    if !node.is_dir || !node.expanded {
        return;
    }

    for child in &node.children {
        collect_visible_entries(child, out);
    }
}

fn find_node_mut<'a>(node: &'a mut DirectoryNode, path: &Path) -> Option<&'a mut DirectoryNode> {
    if node.path == path {
        return Some(node);
    }

    for child in &mut node.children {
        if let Some(found) = find_node_mut(child, path) {
            return Some(found);
        }
    }

    None
}

fn is_same_or_descendant(path: &Path, maybe_parent: &Path) -> bool {
    path == maybe_parent || path.starts_with(maybe_parent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempTreeDir {
        path: PathBuf,
    }

    impl TempTreeDir {
        fn new(label: &str) -> Self {
            let mut path = std::env::temp_dir();
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock before epoch")
                .as_nanos();
            path.push(format!("textual-rs-{label}-{}-{stamp}", std::process::id()));
            fs::create_dir_all(&path).expect("create temp test directory");
            Self { path }
        }
    }

    impl Drop for TempTreeDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn directory_tree_selection_forwards_file_specific_message() {
        let temp = TempTreeDir::new("directory-tree-file-selected");
        fs::write(temp.path.join("alpha.txt"), "alpha").expect("write file");

        let mut tree = DirectoryTree::new(&temp.path);
        tree.on_layout(40, 4);

        let mut ctx = EventCtx::default();
        tree.on_message(
            &MessageEvent::new(
                tree.tree_id(),
                TreeNodeSelected {
                    index: 1,
                    label: "alpha.txt".to_string(),
                    data: None,
                },
            ),
            &mut ctx,
        );

        assert!(ctx.handled());
        let emitted = ctx.take_messages();
        assert!(emitted.iter().any(|event| {
            event
                .downcast_ref::<DirectoryTreeFileSelected>()
                .is_some_and(|m| m.index == 1 && m.path.ends_with("alpha.txt"))
        }));
    }

    #[test]
    fn directory_tree_selection_forwards_directory_specific_message() {
        let temp = TempTreeDir::new("directory-tree-dir-selected");
        fs::create_dir_all(temp.path.join("nested")).expect("create nested dir");

        let mut tree = DirectoryTree::new(&temp.path);
        tree.on_layout(40, 4);

        let mut ctx = EventCtx::default();
        tree.on_message(
            &MessageEvent::new(
                tree.tree_id(),
                TreeNodeSelected {
                    index: 1,
                    label: "nested".to_string(),
                    data: None,
                },
            ),
            &mut ctx,
        );

        assert!(ctx.handled());
        let emitted = ctx.take_messages();
        assert!(emitted.iter().any(|event| {
            event
                .downcast_ref::<DirectoryTreeDirectorySelected>()
                .is_some_and(|m| m.index == 1 && m.path.ends_with("nested"))
        }));
    }

    #[test]
    fn directory_tree_expand_emits_async_task_spawn_message() {
        let temp = TempTreeDir::new("directory-tree-expand-spawn");
        fs::create_dir_all(temp.path.join("nested")).expect("create nested dir");

        let mut tree = DirectoryTree::new(&temp.path);
        tree.on_layout(40, 4);

        let mut ctx = EventCtx::default();
        tree.on_message(
            &MessageEvent::new(
                tree.tree_id(),
                TreeNodeToggled {
                    index: 1,
                    label: "nested".to_string(),
                    expanded: true,
                },
            ),
            &mut ctx,
        );

        let emitted = ctx.take_messages();
        assert!(emitted.iter().any(|event| {
            event.downcast_ref::<AsyncTaskSpawn>().is_some_and(|m| {
                m.target == NodeId::default()
                    && matches!(m.request, AsyncTaskRequest::ReadDirectory { .. })
            })
        }));
    }

    #[test]
    fn async_load_result_applies_filter_predicate() {
        let temp = TempTreeDir::new("directory-tree-async-filter");
        fs::create_dir_all(temp.path.join("nested")).expect("create nested dir");
        fs::write(temp.path.join("nested/keep.rs"), "").expect("write keep.rs");
        fs::write(temp.path.join("nested/skip.txt"), "").expect("write skip.txt");

        let mut tree = DirectoryTree::new(&temp.path);
        tree.filter_paths(|p| {
            p.is_dir()
                || p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|ext| ext == "rs")
        });
        tree.on_layout(40, 10);

        // Simulate expanding "nested" — first collapse it (the sync build expanded it) then expand.
        let mut ctx = EventCtx::default();
        tree.on_message(
            &MessageEvent::new(
                tree.tree_id(),
                TreeNodeToggled {
                    index: 1,
                    label: "nested".to_string(),
                    expanded: false,
                },
            ),
            &mut ctx,
        );
        let _ = ctx.take_messages();

        let mut ctx = EventCtx::default();
        tree.on_message(
            &MessageEvent::new(
                tree.tree_id(),
                TreeNodeToggled {
                    index: 1,
                    label: "nested".to_string(),
                    expanded: true,
                },
            ),
            &mut ctx,
        );
        let spawn_msgs = ctx.take_messages();
        let task_id = spawn_msgs
            .iter()
            .find_map(|event| event.downcast_ref::<AsyncTaskSpawn>().map(|m| m.task_id));
        assert!(task_id.is_some(), "should have spawned async task");

        // Simulate async result with both files arriving.
        let nested_path = temp.path.join("nested");
        let mut ctx = EventCtx::default();
        tree.apply_directory_load_result(
            task_id.unwrap(),
            &AsyncTaskResult::DirectoryEntries {
                path: nested_path.display().to_string(),
                entries: vec![
                    AsyncDirectoryEntry {
                        path: temp.path.join("nested/keep.rs").display().to_string(),
                        label: "keep.rs".to_string(),
                        is_dir: false,
                    },
                    AsyncDirectoryEntry {
                        path: temp.path.join("nested/skip.txt").display().to_string(),
                        label: "skip.txt".to_string(),
                        is_dir: false,
                    },
                ],
            },
            &mut ctx,
        );

        // The filter should have excluded skip.txt.
        let nested_node = find_node_mut(&mut tree.root, &nested_path).expect("nested node");
        assert_eq!(
            nested_node.children.len(),
            1,
            "filter should exclude skip.txt"
        );
        assert_eq!(nested_node.children[0].label, "keep.rs");
    }

    #[test]
    fn directory_tree_collapse_emits_async_task_cancel_message() {
        let temp = TempTreeDir::new("directory-tree-collapse-cancel");
        fs::create_dir_all(temp.path.join("nested")).expect("create nested dir");

        let mut tree = DirectoryTree::new(&temp.path);
        tree.on_layout(40, 4);

        let mut expand_ctx = EventCtx::default();
        tree.on_message(
            &MessageEvent::new(
                tree.tree_id(),
                TreeNodeToggled {
                    index: 1,
                    label: "nested".to_string(),
                    expanded: true,
                },
            ),
            &mut expand_ctx,
        );
        let _ = expand_ctx.take_messages();

        let mut collapse_ctx = EventCtx::default();
        tree.on_message(
            &MessageEvent::new(
                tree.tree_id(),
                TreeNodeToggled {
                    index: 1,
                    label: "nested".to_string(),
                    expanded: false,
                },
            ),
            &mut collapse_ctx,
        );

        let emitted = collapse_ctx.take_messages();
        assert!(emitted.iter().any(|event| {
            event
                .downcast_ref::<AsyncTaskCancel>()
                .is_some_and(|m| m.task_id == 1)
        }));
    }
}
