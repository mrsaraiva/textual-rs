use crate::css::{node_selector_meta, resolve_node_style, with_style_stack};
use crate::driver::PointerShape;
use crate::event::{
    Action, ActionMap, ClickEvent, Event, KeyBind, MouseEnterEvent, MouseLeaveEvent,
};
use crate::node_id::NodeId;
use crate::widget_tree::WidgetTree;
use crate::widgets::{
    APP_ROOT_HSCROLLBAR_ID, APP_ROOT_SCROLLBAR_CORNER_ID, APP_ROOT_VSCROLLBAR_ID,
    DATA_TABLE_HSCROLLBAR_ID, KEY_PANEL_VSCROLLBAR_ID, LOG_VSCROLLBAR_ID, RICH_LOG_VSCROLLBAR_ID,
    SCROLL_VIEW_HSCROLLBAR_ID, SCROLL_VIEW_SCROLLBAR_CORNER_ID, SCROLL_VIEW_VSCROLLBAR_ID,
    SYSTEM_TOOLTIP_STYLE_ID,
};
use crossterm::event::{KeyCode, KeyModifiers, MouseEventKind};
use rich_rs::ConsoleOptions;

use crate::driver::Size;

pub(crate) fn apply_size(options: &mut ConsoleOptions, size: Size) {
    let width = size.width as usize;
    let height = size.height as usize;
    options.size = (width, height);
    options.max_width = width;
    options.max_height = height;
}

pub(crate) fn mouse_scroll_deltas(kind: MouseEventKind, modifiers: KeyModifiers) -> (i32, i32) {
    let (mut delta_x, mut delta_y) = match kind {
        MouseEventKind::ScrollUp => (0, -1),
        MouseEventKind::ScrollDown => (0, 1),
        MouseEventKind::ScrollLeft => (-1, 0),
        MouseEventKind::ScrollRight => (1, 0),
        _ => (0, 0),
    };

    // Common TUI convention: Shift + vertical wheel scrolls horizontally.
    if modifiers.contains(KeyModifiers::SHIFT) && delta_x == 0 && delta_y != 0 {
        delta_x = delta_y;
        delta_y = 0;
    }

    (delta_x, delta_y)
}

pub(crate) fn should_quit_key(key: &crossterm::event::KeyEvent, quit_keys: &[KeyBind]) -> bool {
    let bind = KeyBind::new(key.code, key.modifiers);
    quit_keys.iter().any(|candidate| *candidate == bind)
}

pub(crate) fn default_action_map() -> ActionMap {
    let mut map = ActionMap::new();
    map.bind(
        KeyBind::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        Action::CopySelectedText,
    );
    map.bind(
        KeyBind::new(KeyCode::Tab, KeyModifiers::empty()),
        Action::FocusNext,
    );
    map.bind(
        KeyBind::new(KeyCode::BackTab, KeyModifiers::SHIFT),
        Action::FocusPrev,
    );
    map.bind(
        KeyBind::new(KeyCode::Home, KeyModifiers::empty()),
        Action::ScrollHome,
    );
    map.bind(
        KeyBind::new(KeyCode::End, KeyModifiers::empty()),
        Action::ScrollEnd,
    );
    map.bind(
        KeyBind::new(KeyCode::Up, KeyModifiers::empty()),
        Action::ScrollUp,
    );
    map.bind(
        KeyBind::new(KeyCode::Down, KeyModifiers::empty()),
        Action::ScrollDown,
    );
    map.bind(
        KeyBind::new(KeyCode::PageUp, KeyModifiers::empty()),
        Action::ScrollPageUp,
    );
    map.bind(
        KeyBind::new(KeyCode::PageDown, KeyModifiers::empty()),
        Action::ScrollPageDown,
    );
    map.bind(
        KeyBind::new(KeyCode::Char('k'), KeyModifiers::empty()),
        Action::ScrollUp,
    );
    map.bind(
        KeyBind::new(KeyCode::Char('j'), KeyModifiers::empty()),
        Action::ScrollDown,
    );
    map.bind(
        KeyBind::new(KeyCode::Left, KeyModifiers::empty()),
        Action::ScrollLeft,
    );
    map.bind(
        KeyBind::new(KeyCode::Right, KeyModifiers::empty()),
        Action::ScrollRight,
    );
    map.bind(
        KeyBind::new(KeyCode::Char('h'), KeyModifiers::empty()),
        Action::ScrollLeft,
    );
    map.bind(
        KeyBind::new(KeyCode::Char('l'), KeyModifiers::empty()),
        Action::ScrollRight,
    );
    map.bind(
        KeyBind::new(KeyCode::PageUp, KeyModifiers::CONTROL),
        Action::ScrollPageLeft,
    );
    map.bind(
        KeyBind::new(KeyCode::PageDown, KeyModifiers::CONTROL),
        Action::ScrollPageRight,
    );
    map.bind(
        KeyBind::new(KeyCode::Char(' '), KeyModifiers::empty()),
        Action::Toggle,
    );
    map.bind(
        KeyBind::new(KeyCode::Enter, KeyModifiers::empty()),
        Action::Toggle,
    );
    map.bind(
        KeyBind::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
        Action::CommandPalette,
    );
    map
}

// ---------------------------------------------------------------------------
// Arena-tree-based focus/hover/binding helpers
// ---------------------------------------------------------------------------

/// Collect the focus chain: all focusable, visible nodes in depth-first order.
pub(crate) fn collect_focus_chain_tree(tree: &WidgetTree) -> Vec<NodeId> {
    let root = match tree.root() {
        Some(r) => r,
        None => return Vec::new(),
    };
    let mut focus_chain = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let Some(node) = tree.get(id) else {
            continue;
        };
        if !node.display || node.visibility != crate::style::Visibility::Visible {
            continue;
        }

        if node.widget.focusable() {
            focus_chain.push(id);
        }

        if node.widget.can_focus_children() {
            for &child in tree.children(id).iter().rev() {
                stack.push(child);
            }
        }
    }
    focus_chain
}

/// Forward `on_mouse_move` through the target bubble path (target → root).
///
/// Coordinates are provided in screen space and translated per node to
/// content-local coordinates before invoking `on_mouse_move`.
///
/// Returns `true` when any widget on the path reports a change.
pub fn call_on_mouse_move_tree(
    tree: &mut WidgetTree,
    target: NodeId,
    screen_x: u16,
    screen_y: u16,
) -> bool {
    let mut changed = false;
    let path = build_path_to_node_local(tree, target);
    for &node_id in path.iter().rev() {
        let (x, y) = tree_content_local_coords(tree, node_id, screen_x, screen_y);
        if let Some(node) = tree.get_mut(node_id) {
            changed |= node.widget.on_mouse_move(x, y);
        }
    }
    changed
}

fn build_path_to_node_local(tree: &WidgetTree, target: NodeId) -> Vec<NodeId> {
    let mut path = Vec::new();
    let mut cur = Some(target);
    while let Some(id) = cur {
        path.push(id);
        cur = tree.parent(id);
    }
    path.reverse();
    path
}

/// Find the deepest visible node at a screen coordinate using tree layout
/// geometry, independent of rendered segment metadata.
pub fn widget_at_tree_layout(tree: &WidgetTree, x: u16, y: u16) -> Option<NodeId> {
    let root = tree.root()?;
    let mut hit_any: Option<NodeId> = None;
    let mut hit_interactive: Option<NodeId> = None;
    for node_id in tree.walk_depth_first(root) {
        let Some(node) = tree.get(node_id) else {
            continue;
        };
        if !node.display || node.visibility != crate::style::Visibility::Visible {
            continue;
        }
        let node_css_id = node.css_id.as_deref().or_else(|| node.widget.style_id());
        if node_css_id == Some(SYSTEM_TOOLTIP_STYLE_ID) {
            continue;
        }
        let mut render_shift_x: i32 = 0;
        let mut render_shift_y: i32 = 0;
        let apply_root_scroll = root_scroll_applies_to_subtree(tree, node_id);
        for ancestor_id in tree.ancestors(node_id) {
            let Some(ancestor) = tree.get(ancestor_id) else {
                continue;
            };
            if ancestor_id == root && !apply_root_scroll {
                continue;
            }
            if !descendant_uses_ancestor_scroll(tree, ancestor_id, node_id) {
                continue;
            }
            let (ox, oy) = ancestor.widget.scroll_offset();
            render_shift_x -= ox as i32;
            render_shift_y -= oy as i32;
        }
        let rect = node.layout_rect;
        let x0 = i32::from(rect.x0) + render_shift_x;
        let x1 = i32::from(rect.x1) + render_shift_x;
        let y0 = i32::from(rect.y0) + render_shift_y;
        let y1 = i32::from(rect.y1) + render_shift_y;
        let inside =
            i32::from(x) >= x0 && i32::from(x) < x1 && i32::from(y) >= y0 && i32::from(y) < y1;
        if !inside {
            continue;
        }
        hit_any = Some(node_id);
        if node.widget.mouse_interactive() {
            hit_interactive = Some(node_id);
        }
    }
    hit_interactive.or(hit_any).or(Some(root))
}

/// Translate screen coordinates to content-local coordinates using tree node
/// geometry (prefers `content_rect`, falls back to `layout_rect`).
pub fn tree_content_local_coords(
    tree: &WidgetTree,
    target: NodeId,
    screen_x: u16,
    screen_y: u16,
) -> (u16, u16) {
    let Some(node) = tree.get(target) else {
        return (0, 0);
    };
    let content = node.content_rect;
    let rect = if content.x1 > content.x0 && content.y1 > content.y0 {
        content
    } else {
        node.layout_rect
    };
    // Tree rendering may shift descendants via scroll containers. Mirror that
    // translation here so pointer coordinates map to rendered positions.
    let mut render_shift_x: i32 = 0;
    let mut render_shift_y: i32 = 0;
    let root = tree.root();
    let apply_root_scroll = root_scroll_applies_to_subtree(tree, target);
    for ancestor_id in tree.ancestors(target) {
        let Some(ancestor) = tree.get(ancestor_id) else {
            continue;
        };
        if Some(ancestor_id) == root && !apply_root_scroll {
            continue;
        }
        if !descendant_uses_ancestor_scroll(tree, ancestor_id, target) {
            continue;
        }
        let (ox, oy) = ancestor.widget.scroll_offset();
        render_shift_x -= ox as i32;
        render_shift_y -= oy as i32;
    }

    let origin_x = i32::from(rect.x0) + render_shift_x;
    let origin_y = i32::from(rect.y0) + render_shift_y;
    let local_x = i32::from(screen_x).saturating_sub(origin_x).max(0) as u16;
    let local_y = i32::from(screen_y).saturating_sub(origin_y).max(0) as u16;
    (local_x, local_y)
}

fn root_scroll_applies_to_subtree(tree: &WidgetTree, node_id: NodeId) -> bool {
    let Some(root_id) = tree.root() else {
        return true;
    };
    if node_id == root_id {
        return false;
    }
    let mut cursor = node_id;
    let mut direct_child = None;
    while let Some(parent) = tree.parent(cursor) {
        if parent == root_id {
            direct_child = Some(cursor);
            break;
        }
        cursor = parent;
    }
    let Some(child_id) = direct_child else {
        return true;
    };
    descendant_uses_ancestor_scroll(tree, root_id, child_id)
}

fn descendant_uses_ancestor_scroll(
    tree: &WidgetTree,
    ancestor_id: NodeId,
    descendant_id: NodeId,
) -> bool {
    let mut cursor = descendant_id;
    let mut child_on_path = None;
    while let Some(parent) = tree.parent(cursor) {
        if parent == ancestor_id {
            child_on_path = Some(cursor);
            break;
        }
        cursor = parent;
    }
    let Some(child_id) = child_on_path else {
        return true;
    };
    !node_is_docked(tree, child_id) && !node_is_dedicated_scrollbar(tree, child_id)
}

fn node_is_docked(tree: &WidgetTree, node_id: NodeId) -> bool {
    resolve_style_in_tree(tree, node_id).is_some_and(|style| style.dock.is_some())
}

fn node_is_dedicated_scrollbar(tree: &WidgetTree, node_id: NodeId) -> bool {
    let Some(node) = tree.get(node_id) else {
        return false;
    };
    let css_id = node.css_id.as_deref().or_else(|| node.widget.style_id());
    matches!(
        css_id,
        Some(
            APP_ROOT_VSCROLLBAR_ID
                | APP_ROOT_HSCROLLBAR_ID
                | APP_ROOT_SCROLLBAR_CORNER_ID
                | SCROLL_VIEW_VSCROLLBAR_ID
                | SCROLL_VIEW_HSCROLLBAR_ID
                | SCROLL_VIEW_SCROLLBAR_CORNER_ID
                | LOG_VSCROLLBAR_ID
                | RICH_LOG_VSCROLLBAR_ID
                | KEY_PANEL_VSCROLLBAR_ID
                | DATA_TABLE_HSCROLLBAR_ID
        )
    )
}

pub(crate) fn resolve_style_in_tree(
    tree: &WidgetTree,
    node_id: NodeId,
) -> Option<crate::style::Style> {
    let mut path = Vec::new();
    let mut cursor = Some(node_id);
    while let Some(id) = cursor {
        path.push(id);
        cursor = tree.parent(id);
    }
    if path.is_empty() {
        return None;
    }
    path.reverse(); // root -> target
    resolve_style_along_path(tree, &path, 0)
}

fn resolve_style_along_path(
    tree: &WidgetTree,
    path: &[NodeId],
    index: usize,
) -> Option<crate::style::Style> {
    let id = *path.get(index)?;
    let meta = node_selector_meta(tree, id);
    let resolved = resolve_node_style(tree, id, &meta);
    if index + 1 == path.len() {
        Some(resolved)
    } else {
        with_style_stack(meta, resolved, || {
            resolve_style_along_path(tree, path, index + 1)
        })
    }
}

/// Check whether any widget in the tree reports `is_active() == true`.
pub(crate) fn any_widget_active_tree(tree: &WidgetTree) -> bool {
    let root = match tree.root() {
        Some(r) => r,
        None => return false,
    };
    for node_id in tree.walk_depth_first(root) {
        if let Some(node) = tree.get(node_id) {
            if node.widget.is_active() {
                return true;
            }
        }
    }
    false
}

/// Determine the pointer shape for a hovered node.
///
/// Reads the widget's CSS `pointer` property first. Falls back to
/// `PointerShape::Pointer` for interactive widgets (or `NotAllowed` if disabled).
pub(crate) fn pointer_shape_for_hover_tree(
    tree: &WidgetTree,
    hovered: Option<NodeId>,
) -> PointerShape {
    let Some(id) = hovered else {
        return PointerShape::Default;
    };

    let Some(node) = tree.get(id) else {
        return PointerShape::Default;
    };

    let mouse_interactive = node.widget.mouse_interactive();
    // Dual-write phase: merge node.state.disabled with legacy widget getter.
    let disabled = node.state.disabled || node.widget.is_disabled();

    if !mouse_interactive {
        return PointerShape::Default;
    }

    // Disabled widgets always show not-allowed, regardless of CSS pointer.
    if disabled {
        return PointerShape::NotAllowed;
    }

    // Read the node's resolved CSS `pointer` property via node record path.
    let meta = node_selector_meta(tree, id);
    let resolved = resolve_node_style(tree, id, &meta);
    if let Some(ptr) = resolved.pointer {
        return match ptr {
            crate::style::Pointer::Default => PointerShape::Default,
            crate::style::Pointer::Pointer => PointerShape::Pointer,
            crate::style::Pointer::Text => PointerShape::Text,
            crate::style::Pointer::NotAllowed => PointerShape::NotAllowed,
        };
    }

    // Default for interactive widgets with no explicit CSS pointer.
    PointerShape::Pointer
}

// ---------------------------------------------------------------------------
// Mouse enter/leave event generation
// ---------------------------------------------------------------------------

/// Generate Enter/Leave events when the hovered widget changes.
///
/// Returns a list of `(NodeId, Event)` pairs to dispatch. At most one Leave
/// (for `old_hover`) and one Enter (for `new_hover`) are emitted.
pub(crate) fn generate_enter_leave_events(
    old_hover: Option<NodeId>,
    new_hover: Option<NodeId>,
    x: u16,
    y: u16,
    screen_x: u16,
    screen_y: u16,
) -> Vec<(NodeId, Event)> {
    if old_hover == new_hover {
        return Vec::new();
    }
    let mut events = Vec::with_capacity(2);
    if let Some(old) = old_hover {
        events.push((
            old,
            Event::Leave(MouseLeaveEvent {
                x,
                y,
                screen_x,
                screen_y,
            }),
        ));
    }
    if let Some(new) = new_hover {
        events.push((
            new,
            Event::Enter(MouseEnterEvent {
                x,
                y,
                screen_x,
                screen_y,
            }),
        ));
    }
    events
}

// ---------------------------------------------------------------------------
// Click detection
// ---------------------------------------------------------------------------

/// Tracks mousedown target to detect click (down+up on same widget).
#[derive(Debug, Default)]
pub(crate) struct ClickTracker {
    /// The widget that received the most recent mousedown, plus coordinates.
    down: Option<ClickDownState>,
}

/// Coordinates are stored for future drag-distance thresholds / long-press detection.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
struct ClickDownState {
    target: NodeId,
    screen_x: u16,
    screen_y: u16,
    x: u16,
    y: u16,
    button: u8,
}

impl ClickTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a mousedown. `button`: 0=left, 1=middle, 2=right.
    pub fn on_mouse_down(
        &mut self,
        target: NodeId,
        x: u16,
        y: u16,
        screen_x: u16,
        screen_y: u16,
        button: u8,
    ) {
        self.down = Some(ClickDownState {
            target,
            screen_x,
            screen_y,
            x,
            y,
            button,
        });
    }

    /// Current mousedown target, used as drag-capture owner while a button is held.
    pub fn down_target(&self) -> Option<NodeId> {
        self.down.map(|down| down.target)
    }

    /// Record a mouseup. If the target matches the previous mousedown target,
    /// returns a `(NodeId, Event::Click)` pair.
    pub fn on_mouse_up(
        &mut self,
        target: Option<NodeId>,
        x: u16,
        y: u16,
        screen_x: u16,
        screen_y: u16,
    ) -> Option<(NodeId, Event)> {
        let down = self.down.take()?;
        let up_target = target?;
        if up_target == down.target {
            Some((
                up_target,
                Event::Click(ClickEvent {
                    x,
                    y,
                    screen_x,
                    screen_y,
                    button: down.button,
                }),
            ))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::any::Any;

    use super::*;
    use crate::css::{default_widget_stylesheet, set_style_context};
    use crate::event::EventCtx;
    use crate::message::{MessageEvent, ScrollbarAxis, ScrollbarScrollTo};
    use crate::node_id::node_id_from_ffi;
    use crate::widget_tree::Rect;
    use crate::widget_tree::WidgetTree;
    use crate::widgets::{AppRoot, Footer, Label, Widget};
    use crossterm::event::KeyEvent;

    #[test]
    fn shift_wheel_maps_vertical_to_horizontal() {
        assert_eq!(
            mouse_scroll_deltas(MouseEventKind::ScrollUp, KeyModifiers::SHIFT),
            (-1, 0)
        );
        assert_eq!(
            mouse_scroll_deltas(MouseEventKind::ScrollDown, KeyModifiers::SHIFT),
            (1, 0)
        );
        assert_eq!(
            mouse_scroll_deltas(MouseEventKind::ScrollLeft, KeyModifiers::SHIFT),
            (-1, 0)
        );
        assert_eq!(
            mouse_scroll_deltas(MouseEventKind::ScrollRight, KeyModifiers::SHIFT),
            (1, 0)
        );
        assert_eq!(
            mouse_scroll_deltas(MouseEventKind::ScrollDown, KeyModifiers::empty()),
            (0, 1)
        );
    }

    #[test]
    fn quit_key_matches_defaults() {
        let quit_keys = vec![KeyBind::new(KeyCode::Char('q'), KeyModifiers::CONTROL)];
        let ctrl_q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL);
        let q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty());
        let x = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::empty());

        assert!(should_quit_key(&ctrl_q, &quit_keys));
        assert!(!should_quit_key(&q, &quit_keys));
        assert!(!should_quit_key(&x, &quit_keys));
    }

    #[test]
    fn quit_key_can_require_modifiers() {
        let quit_keys = vec![KeyBind::new(KeyCode::Char('q'), KeyModifiers::CONTROL)];
        let ctrl_q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL);
        let plain_q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty());

        assert!(should_quit_key(&ctrl_q, &quit_keys));
        assert!(!should_quit_key(&plain_q, &quit_keys));
    }

    #[test]
    fn default_action_map_binds_ctrl_c_to_copy_selected_text() {
        let map = default_action_map();
        let ctrl_c = KeyBind::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(map.lookup(&ctrl_c), Some(Action::CopySelectedText));
    }

    #[test]
    fn app_root_scrollbar_child_does_not_inherit_root_scroll_transform() {
        let mut root = AppRoot::new();
        let extracted = root.take_composed_children();

        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(root));
        for child in extracted {
            tree.mount(root_id, child);
        }

        let vbar_id = tree
            .children(root_id)
            .iter()
            .copied()
            .find(|id| {
                tree.css_id(*id) == Some(crate::widgets::APP_ROOT_VSCROLLBAR_ID)
            })
            .expect("app root vertical scrollbar child should exist");

        assert!(
            !descendant_uses_ancestor_scroll(&tree, root_id, vbar_id),
            "app root scrollbar lane must stay in unscrolled screen space"
        );
    }

    #[test]
    fn app_root_scrollbar_local_coords_stay_stable_when_root_is_scrolled() {
        let mut root = AppRoot::new();
        let extracted = root.take_composed_children();

        let mut tree = WidgetTree::new();
        let root_id = tree.set_root(Box::new(root));
        for child in extracted {
            tree.mount(root_id, child);
        }

        let vbar_id = tree
            .children(root_id)
            .iter()
            .copied()
            .find(|id| {
                tree.css_id(*id) == Some(crate::widgets::APP_ROOT_VSCROLLBAR_ID)
            })
            .expect("app root vertical scrollbar child should exist");

        if let Some(root_node) = tree.get_mut(root_id) {
            root_node.layout_rect = Rect {
                x0: 0,
                y0: 0,
                x1: 114,
                y1: 34,
            };
            root_node.content_rect = root_node.layout_rect;
            let any = root_node.widget.as_mut() as &mut dyn Any;
            let app_root = any
                .downcast_mut::<AppRoot>()
                .expect("root widget should be AppRoot");
            app_root.on_layout(114, 34);
            app_root.set_virtual_content_size(114, 50);
            let mut ctx = EventCtx::default();
            app_root.on_message(
                &MessageEvent::new(node_id_from_ffi(0), ScrollbarScrollTo {
                    axis: ScrollbarAxis::Vertical,
                    offset: 16.0,
                    animate: false,
                    scroll_duration: None,
                }),
                &mut ctx,
            );
        }

        if let Some(vbar) = tree.get_mut(vbar_id) {
            vbar.layout_rect = Rect {
                x0: 112,
                y0: 0,
                x1: 114,
                y1: 34,
            };
            vbar.content_rect = vbar.layout_rect;
        }

        let (lx, ly) = tree_content_local_coords(&tree, vbar_id, 113, 8);
        assert_eq!(
            (lx, ly),
            (1, 8),
            "root scroll offset must not shift app-root scrollbar local pointer mapping"
        );
    }

    // ── Enter/Leave generation tests ─────────────────────────────────

    #[test]
    fn enter_leave_same_hover_emits_nothing() {
        let id = node_id_from_ffi(1);
        let events = generate_enter_leave_events(Some(id), Some(id), 0, 0, 0, 0);
        assert!(events.is_empty());
    }

    #[test]
    fn enter_leave_none_to_none_emits_nothing() {
        let events = generate_enter_leave_events(None, None, 0, 0, 0, 0);
        assert!(events.is_empty());
    }

    #[test]
    fn enter_leave_none_to_widget_emits_enter() {
        let new_id = node_id_from_ffi(5);
        let events = generate_enter_leave_events(None, Some(new_id), 10, 20, 30, 40);
        assert_eq!(events.len(), 1);
        let (target, ref ev) = events[0];
        assert_eq!(target, new_id);
        assert!(matches!(
            ev,
            Event::Enter(MouseEnterEvent {
                x: 10,
                y: 20,
                screen_x: 30,
                screen_y: 40
            })
        ));
    }

    #[test]
    fn enter_leave_widget_to_none_emits_leave() {
        let old_id = node_id_from_ffi(3);
        let events = generate_enter_leave_events(Some(old_id), None, 1, 2, 3, 4);
        assert_eq!(events.len(), 1);
        let (target, ref ev) = events[0];
        assert_eq!(target, old_id);
        assert!(matches!(
            ev,
            Event::Leave(MouseLeaveEvent { x: 1, y: 2, .. })
        ));
    }

    #[test]
    fn enter_leave_widget_to_widget_emits_both() {
        let old_id = node_id_from_ffi(1);
        let new_id = node_id_from_ffi(2);
        let events = generate_enter_leave_events(Some(old_id), Some(new_id), 5, 6, 7, 8);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].0, old_id);
        assert!(matches!(events[0].1, Event::Leave(_)));
        assert_eq!(events[1].0, new_id);
        assert!(matches!(events[1].1, Event::Enter(_)));
    }

    // ── ClickTracker tests ───────────────────────────────────────────

    #[test]
    fn click_tracker_emits_click_on_same_target() {
        let mut tracker = ClickTracker::new();
        let id = node_id_from_ffi(10);
        tracker.on_mouse_down(id, 5, 5, 50, 50, 0);
        let result = tracker.on_mouse_up(Some(id), 5, 5, 50, 50);
        assert!(result.is_some());
        let (target, ev) = result.unwrap();
        assert_eq!(target, id);
        assert!(matches!(
            ev,
            Event::Click(ClickEvent {
                button: 0,
                x: 5,
                y: 5,
                ..
            })
        ));
    }

    #[test]
    fn click_tracker_no_click_on_different_target() {
        let mut tracker = ClickTracker::new();
        let a = node_id_from_ffi(1);
        let b = node_id_from_ffi(2);
        tracker.on_mouse_down(a, 0, 0, 0, 0, 0);
        let result = tracker.on_mouse_up(Some(b), 0, 0, 0, 0);
        assert!(result.is_none());
    }

    #[test]
    fn click_tracker_no_click_without_mousedown() {
        let mut tracker = ClickTracker::new();
        let id = node_id_from_ffi(1);
        let result = tracker.on_mouse_up(Some(id), 0, 0, 0, 0);
        assert!(result.is_none());
    }

    #[test]
    fn click_tracker_no_click_on_none_target() {
        let mut tracker = ClickTracker::new();
        let id = node_id_from_ffi(1);
        tracker.on_mouse_down(id, 0, 0, 0, 0, 0);
        let result = tracker.on_mouse_up(None, 0, 0, 0, 0);
        assert!(result.is_none());
    }

    #[test]
    fn click_tracker_preserves_button() {
        let mut tracker = ClickTracker::new();
        let id = node_id_from_ffi(1);
        tracker.on_mouse_down(id, 0, 0, 0, 0, 2); // right click
        let result = tracker.on_mouse_up(Some(id), 0, 0, 0, 0);
        assert!(result.is_some());
        let (_, ev) = result.unwrap();
        assert!(matches!(ev, Event::Click(ClickEvent { button: 2, .. })));
    }

    #[test]
    fn click_tracker_resets_after_mouseup() {
        let mut tracker = ClickTracker::new();
        let id = node_id_from_ffi(1);
        tracker.on_mouse_down(id, 0, 0, 0, 0, 0);
        let _ = tracker.on_mouse_up(Some(id), 0, 0, 0, 0);
        // Second mouseup without new mousedown → no click
        let result = tracker.on_mouse_up(Some(id), 0, 0, 0, 0);
        assert!(result.is_none());
    }

    #[test]
    fn root_scroll_skips_docked_root_child_subtrees() {
        let _guard = set_style_context(default_widget_stylesheet());
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(AppRoot::new()));
        let label = tree.mount(root, Box::new(Label::new("content")));
        let footer = tree.mount(root, Box::new(Footer::new()));

        // Emulate a docked layout result: root content excludes a bottom footer band.
        if let Some(root_node) = tree.get_mut(root) {
            root_node.content_rect = crate::widget_tree::Rect {
                x0: 0,
                y0: 0,
                x1: 80,
                y1: 23,
            };
        }
        if let Some(label_node) = tree.get_mut(label) {
            label_node.layout_rect = crate::widget_tree::Rect {
                x0: 0,
                y0: 0,
                x1: 80,
                y1: 23,
            };
        }
        if let Some(footer_node) = tree.get_mut(footer) {
            footer_node.layout_rect = crate::widget_tree::Rect {
                x0: 0,
                y0: 23,
                x1: 80,
                y1: 24,
            };
        }

        assert!(
            root_scroll_applies_to_subtree(&tree, label),
            "non-docked root children should move with root scroll"
        );
        assert!(
            !root_scroll_applies_to_subtree(&tree, footer),
            "docked root children should ignore root scroll translation"
        );
    }

    #[test]
    fn markdown_viewer_toc_docked_child_ignores_parent_scroll_transform() {
        let _guard = set_style_context(default_widget_stylesheet());
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(crate::widgets::MarkdownViewer::new(
            "# One\n## Two",
        )));
        let toc = tree.mount(
            root,
            Box::new(crate::widgets::MarkdownTableOfContents::new(vec![
                (1, "One".to_string(), "one".to_string()),
                (2, "Two".to_string(), "two".to_string()),
            ])),
        );
        assert!(
            !descendant_uses_ancestor_scroll(&tree, root, toc),
            "MarkdownViewer > MarkdownTableOfContents should resolve as docked and stay unscrolled"
        );
    }
}
