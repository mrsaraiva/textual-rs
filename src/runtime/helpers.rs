use crate::driver::PointerShape;
use crate::event::{Action, ActionMap, KeyBind};
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
                .map(|node| node.display && node.widget.focusable())
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
    let ty = node.widget.style_type();

    if !mouse_interactive {
        return PointerShape::Default;
    }

    if ty == "Input" {
        return PointerShape::Text;
    }

    if disabled {
        PointerShape::NotAllowed
    } else {
        PointerShape::Pointer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
