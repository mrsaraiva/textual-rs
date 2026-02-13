use crate::driver::PointerShape;
use crate::event::{
    Action, ActionMap, ClickEvent, Event, KeyBind, MouseEnterEvent, MouseLeaveEvent,
};
use crate::node_id::NodeId;
use crate::widget_tree::WidgetTree;
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
        Action::HelpQuit,
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
    tree.walk_depth_first(root)
        .into_iter()
        .filter(|&id| {
            tree.get(id)
                .map(|node| {
                    node.display
                        && node.visibility == crate::style::Visibility::Visible
                        && node.widget.focusable()
                })
                .unwrap_or(false)
        })
        .collect()
}

/// Forward `on_mouse_move` to a specific node in the tree.
///
/// Returns `true` if the widget reported a change.
pub(crate) fn call_on_mouse_move_tree(
    tree: &mut WidgetTree,
    target: NodeId,
    x: u16,
    y: u16,
) -> bool {
    if let Some(node) = tree.get_mut(target) {
        node.widget.on_mouse_move(x, y)
    } else {
        false
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
    let disabled = node.widget.is_disabled();

    if !mouse_interactive {
        return PointerShape::Default;
    }

    // Disabled widgets always show not-allowed, regardless of CSS pointer.
    if disabled {
        return PointerShape::NotAllowed;
    }

    // Read the widget's computed CSS `pointer` property.
    if let Some(style) = node.widget.style() {
        if let Some(ptr) = style.pointer {
            return match ptr {
                crate::style::Pointer::Default => PointerShape::Default,
                crate::style::Pointer::Pointer => PointerShape::Pointer,
                crate::style::Pointer::Text => PointerShape::Text,
                crate::style::Pointer::NotAllowed => PointerShape::NotAllowed,
            };
        }
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

#[derive(Debug, Clone, Copy)]
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
    use super::*;
    use crate::node_id::node_id_from_ffi;
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
    fn default_action_map_binds_ctrl_c_to_help_quit() {
        let map = default_action_map();
        let ctrl_c = KeyBind::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(map.lookup(&ctrl_c), Some(Action::HelpQuit));
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
}
