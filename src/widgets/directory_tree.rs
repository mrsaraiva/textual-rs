use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use rich_rs::{Console, ConsoleOptions, Renderable, Segments};

use crate::event::{Event, EventCtx, MouseDownEvent};
use crate::message::{Message, MessageEvent};

use super::{
    Tree, TreeNode, Widget, WidgetId, WidgetStyles,
    helpers::{empty_classes, fixed_height_from_constraints},
};

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
        let label = if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            name.to_string()
        } else {
            path.display().to_string()
        };
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
        let mut node = TreeNode::new(self.label.clone())
            .expanded(self.expanded)
            .allow_expand(self.is_dir);
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
    id: WidgetId,
    root_path: PathBuf,
    root: DirectoryNode,
    tree: Tree,
    visible_entries: Vec<VisibleEntry>,
    show_hidden: bool,
    focused: bool,
    hovered: bool,
    last_width: u16,
    last_height: u16,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    styles: WidgetStyles,
}

impl DirectoryTree {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let root_path = path.into();
        let show_hidden = false;
        let root = build_root(root_path.clone(), show_hidden, true, &HashSet::new());

        let mut tree = Tree::new(vec![root.to_tree_node()]);
        tree.on_layout(1, 1);

        let mut this = Self {
            id: WidgetId::new(),
            root_path,
            root,
            tree,
            visible_entries: Vec::new(),
            show_hidden,
            focused: false,
            hovered: false,
            last_width: 1,
            last_height: 1,
            classes: vec!["directory-tree".to_string()],
            focused_classes: vec!["directory-tree".to_string(), "focused".to_string()],
            styles: WidgetStyles::default(),
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

    pub fn tree_id(&self) -> WidgetId {
        self.tree.id()
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
        let root = build_root(
            self.root_path.clone(),
            self.show_hidden,
            self.root.expanded,
            &expanded_paths,
        );
        self.root = root;
        self.rebuild_tree(selected);
    }

    fn rebuild_tree(&mut self, preferred_path: Option<PathBuf>) {
        self.visible_entries.clear();
        collect_visible_entries(&self.root, &mut self.visible_entries);

        let mut tree = Tree::new(vec![self.root.to_tree_node()]);
        tree.set_focus(self.focused);
        tree.on_layout(self.last_width, self.last_height);

        if let Some(path) = preferred_path {
            if let Some(index) = self
                .visible_entries
                .iter()
                .position(|entry| entry.path == path)
            {
                tree.set_selected(index);
            }
        }

        self.tree = tree;
    }

    fn update_node_expanded_state(&mut self, index: usize, expanded: bool) {
        let Some(entry) = self.visible_entries.get(index).cloned() else {
            return;
        };

        if let Some(node) = find_node_mut(&mut self.root, &entry.path) {
            node.expanded = expanded;
            if expanded && node.is_dir && !node.loaded {
                node.children = read_children(&node.path, self.show_hidden);
                node.loaded = true;
            }
        }

        self.rebuild_tree(Some(entry.path));
    }
}

impl Widget for DirectoryTree {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
        self.tree.set_focus(focused);
    }

    fn has_focus(&self) -> bool {
        self.focused
    }

    fn is_hovered(&self) -> bool {
        self.hovered
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.last_width = width.max(1);
        self.last_height = height.max(1);
        self.tree.on_layout(self.last_width, self.last_height);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::MouseDown(mouse) if mouse.target == self.id => {
                self.tree.on_event(
                    &Event::MouseDown(MouseDownEvent {
                        target: self.tree.id(),
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
        if message.sender != self.tree.id() {
            return;
        }

        match &message.message {
            Message::TreeNodeSelected { index, .. } => {
                if let Some(entry) = self.visible_entries.get(*index) {
                    ctx.post_message(
                        self.id,
                        Message::TreeNodeSelected {
                            index: *index,
                            label: entry.path.display().to_string(),
                        },
                    );
                    ctx.set_handled();
                }
            }
            Message::TreeNodeToggled {
                index, expanded, ..
            } => {
                let label = self
                    .visible_entries
                    .get(*index)
                    .map(|entry| entry.path.display().to_string())
                    .unwrap_or_default();
                self.update_node_expanded_state(*index, *expanded);
                ctx.post_message(
                    self.id,
                    Message::TreeNodeToggled {
                        index: *index,
                        label,
                        expanded: *expanded,
                    },
                );
                ctx.request_repaint();
                ctx.set_handled();
            }
            _ => {}
        }
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        self.tree.on_mouse_move(x, y)
    }

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        self.tree.on_mouse_scroll(delta_x, delta_y, ctx);
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(&self.tree, console, options)
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(self.tree.layout_height())
    }

    fn content_width(&self) -> Option<usize> {
        self.tree.content_width()
    }

    fn style_type(&self) -> &'static str {
        "DirectoryTree"
    }

    fn style_classes(&self) -> &[String] {
        if self.focused {
            &self.focused_classes
        } else if self.classes.is_empty() {
            empty_classes()
        } else {
            &self.classes
        }
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for DirectoryTree {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

fn read_children(path: &Path, show_hidden: bool) -> Vec<DirectoryNode> {
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

fn build_root(
    root_path: PathBuf,
    show_hidden: bool,
    expanded: bool,
    expanded_paths: &HashSet<PathBuf>,
) -> DirectoryNode {
    let mut root = DirectoryNode::from_path(root_path);
    if !root.is_dir {
        return root;
    }
    root.expanded = expanded;
    populate_expanded_children(&mut root, show_hidden, expanded_paths);
    root
}

fn populate_expanded_children(
    node: &mut DirectoryNode,
    show_hidden: bool,
    expanded_paths: &HashSet<PathBuf>,
) {
    if !node.is_dir {
        return;
    }

    if !node.expanded {
        node.loaded = false;
        node.children.clear();
        return;
    }

    node.children = read_children(&node.path, show_hidden);
    node.loaded = true;
    for child in &mut node.children {
        if child.is_dir {
            child.expanded = expanded_paths.contains(&child.path);
            populate_expanded_children(child, show_hidden, expanded_paths);
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
