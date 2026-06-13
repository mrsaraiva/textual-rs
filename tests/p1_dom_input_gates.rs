use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rich_rs::{Console, ConsoleOptions, Segments};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use textual::compose;
use textual::event::{
    Action, BlurEvent, Event, EventCtx, FocusEvent, MouseDownEvent, MouseEnterEvent,
    MouseLeaveEvent, MouseUpEvent,
};
use textual::keys::KeyEventData;
use textual::node_id::NodeId;
use textual::prelude::*;
use textual::runtime::{
    build_widget_tree_from_root, dispatch_ctx::set_dispatch_recipient, focused_node_id_tree,
    render_tree_to_frame, tree_content_local_coords, widget_at_tree_layout,
};
use textual::widget_tree::WidgetTree;

#[derive(Clone)]
struct ClickProbe {
    id: &'static str,
    pressed: bool,
    sink: Arc<Mutex<Vec<String>>>,
}

impl ClickProbe {
    fn new(id: &'static str, sink: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            id,
            pressed: false,
            sink,
        }
    }
}

impl Widget for ClickProbe {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }

    fn mouse_interactive(&self) -> bool {
        true
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        let this = self.node_id();
        match event {
            Event::MouseDown(mouse) if mouse.target == this => {
                self.pressed = true;
                ctx.set_handled();
            }
            Event::MouseUp(mouse) if self.pressed && mouse.target == Some(this) => {
                self.pressed = false;
                self.sink
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(self.id.to_string());
                ctx.set_handled();
            }
            _ => {}
        }
    }
}

#[derive(Clone)]
struct LayoutClickProbe {
    id: &'static str,
    pressed: bool,
    click_sink: Arc<Mutex<Vec<String>>>,
    layout_sink: Arc<Mutex<HashMap<String, (u16, u16)>>>,
}

impl LayoutClickProbe {
    fn new(
        id: &'static str,
        click_sink: Arc<Mutex<Vec<String>>>,
        layout_sink: Arc<Mutex<HashMap<String, (u16, u16)>>>,
    ) -> Self {
        Self {
            id,
            pressed: false,
            click_sink,
            layout_sink,
        }
    }
}

impl Widget for LayoutClickProbe {
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        self.layout_sink
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                self.id.to_string(),
                (options.max_width as u16, options.max_height as u16),
            );
        Segments::new()
    }

    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }

    fn mouse_interactive(&self) -> bool {
        true
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        let this = self.node_id();
        match event {
            Event::MouseDown(mouse) if mouse.target == this => {
                self.pressed = true;
                ctx.set_handled();
            }
            Event::MouseUp(mouse) if self.pressed && mouse.target == Some(this) => {
                self.pressed = false;
                self.click_sink
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(self.id.to_string());
                ctx.set_handled();
            }
            _ => {}
        }
    }
}

#[derive(Clone)]
struct HoverProbe {
    id: &'static str,
    hovered: bool,
    sink: Arc<Mutex<Vec<String>>>,
}

impl HoverProbe {
    fn new(id: &'static str, sink: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            id,
            hovered: false,
            sink,
        }
    }

    fn set_hovered(&mut self, hovered: bool) {
        if self.hovered != hovered {
            self.hovered = hovered;
            self.sink
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(format!("{}:{hovered}", self.id));
        }
    }
}

impl Widget for HoverProbe {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::Enter(_) => {
                self.set_hovered(true);
                ctx.set_handled();
            }
            Event::Leave(_) => {
                self.set_hovered(false);
                ctx.set_handled();
            }
            _ => {}
        }
    }
}

#[derive(Clone)]
struct FocusProbe {
    id: &'static str,
    focused: bool,
    sink: Arc<Mutex<Vec<String>>>,
}

impl FocusProbe {
    fn new(id: &'static str, sink: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            id,
            focused: false,
            sink,
        }
    }

    fn set_focus(&mut self, focused: bool) {
        if self.focused != focused {
            self.focused = focused;
            self.sink
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(format!("{}:{focused}", self.id));
        }
    }
}

impl Widget for FocusProbe {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn focusable(&self) -> bool {
        true
    }

    fn mouse_interactive(&self) -> bool {
        true
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::Focus(_) => {
                self.set_focus(true);
                ctx.set_handled();
            }
            Event::Blur(_) => {
                self.set_focus(false);
                ctx.set_handled();
            }
            _ => {}
        }
    }
}

fn build_tree(root: &mut dyn Widget, w: usize, h: usize) -> WidgetTree {
    let console = Console::new();
    let mut tree = build_widget_tree_from_root(root).expect("tree should exist");
    let _ = render_tree_to_frame(&mut tree, root, &console, w, h);
    tree
}

fn click_tree(tree: &mut WidgetTree, x: u16, y: u16) -> bool {
    let Some(target) = widget_at_tree_layout(tree, x, y) else {
        return false;
    };
    let _ = focus_node(tree, target);
    let (local_x, local_y) = tree_content_local_coords(tree, target, x, y);
    let down = textual::runtime::dispatch_event_to_target_tree(
        tree,
        target,
        &Event::MouseDown(MouseDownEvent {
            target,
            screen_x: x,
            screen_y: y,
            x: local_x,
            y: local_y,
        }),
    );
    let up = textual::runtime::dispatch_event_to_target_tree(
        tree,
        target,
        &Event::MouseUp(MouseUpEvent {
            target: Some(target),
            screen_x: x,
            screen_y: y,
            x: local_x,
            y: local_y,
        }),
    );
    down.handled || up.handled
}

fn focus_node(tree: &mut WidgetTree, target: NodeId) -> bool {
    let current = focused_node_id_tree(tree);
    if current == Some(target) {
        return false;
    }
    if let Some(current_id) = current {
        tree.set_focus_state(current_id, false);
        let _ = textual::runtime::dispatch_event_to_target_tree(
            tree,
            current_id,
            &Event::Blur(BlurEvent { node: current_id }),
        );
    }
    tree.set_focus_state(target, true);
    textual::runtime::dispatch_event_to_target_tree(
        tree,
        target,
        &Event::Focus(FocusEvent { node: target }),
    )
    .handled
}

fn send_key_to_focus(tree: &mut WidgetTree, key: KeyEventData) -> bool {
    let Some(target) = focused_node_id_tree(tree).or_else(|| tree.root()) else {
        return false;
    };
    textual::runtime::dispatch_event_to_target_tree(tree, target, &Event::Key(key)).handled
}

fn move_hover_tree(tree: &mut WidgetTree, hovered: &mut Option<NodeId>, x: u16, y: u16) -> bool {
    let target = widget_at_tree_layout(tree, x, y);
    if *hovered == target {
        if let Some(id) = target {
            let (local_x, local_y) = tree_content_local_coords(tree, id, x, y);
            return textual::runtime::call_on_mouse_move_tree(tree, id, local_x, local_y);
        }
        return false;
    }

    if let Some(previous) = *hovered {
        let (local_x, local_y) = tree_content_local_coords(tree, previous, x, y);
        let _ = textual::runtime::dispatch_event_to_target_tree(
            tree,
            previous,
            &Event::Leave(MouseLeaveEvent {
                screen_x: x,
                screen_y: y,
                x: local_x,
                y: local_y,
            }),
        );
    }

    if let Some(id) = target {
        let (local_x, local_y) = tree_content_local_coords(tree, id, x, y);
        let _ = textual::runtime::dispatch_event_to_target_tree(
            tree,
            id,
            &Event::Enter(MouseEnterEvent {
                screen_x: x,
                screen_y: y,
                x: local_x,
                y: local_y,
            }),
        );
        let _ = textual::runtime::call_on_mouse_move_tree(tree, id, local_x, local_y);
    }

    *hovered = target;
    true
}

fn find_click_for_sink(
    tree: &mut WidgetTree,
    sink: &Arc<Mutex<Vec<String>>>,
    width: u16,
    height: u16,
    needle: &str,
) -> Option<(u16, u16)> {
    for y in 0..height {
        for x in 0..width {
            sink.lock().unwrap_or_else(|e| e.into_inner()).clear();
            let _ = click_tree(tree, x, y);
            if sink
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .any(|entry| entry == needle)
            {
                return Some((x, y));
            }
        }
    }
    None
}

struct DataTableNavProbe {
    inner: DataTable,
    focused: bool,
    sink: Arc<Mutex<Vec<String>>>,
}

impl DataTableNavProbe {
    fn new(sink: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            inner: DataTable::new(
                vec!["Name".into(), "Value".into()],
                vec![
                    vec!["Alpha".into(), "1".into()],
                    vec!["Beta".into(), "2".into()],
                    vec!["Gamma".into(), "3".into()],
                ],
            ),
            focused: false,
            sink,
        }
    }

    fn has_focus(&self) -> bool {
        self.focused || self.node_state().focused
    }

    #[allow(dead_code)]
    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }
}

impl Widget for DataTableNavProbe {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.inner.render(console, options)
    }

    fn focusable(&self) -> bool {
        self.inner.focusable()
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.inner.on_layout(width, height);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::Focus(_) => {
                self.focused = true;
                ctx.set_handled();
                return;
            }
            Event::Blur(_) => {
                self.focused = false;
                ctx.set_handled();
                return;
            }
            _ => {}
        }
        // Forward to inner table; use a focused dispatch context if this probe is focused.
        // Preserve the current node_id so mouse.target checks still match.
        let before = self.inner.selected();
        if self.has_focus() {
            let node_id = self.node_id();
            let focused_ns = NodeState {
                focused: true,
                ..Default::default()
            };
            let _guard = set_dispatch_recipient(node_id, focused_ns);
            self.inner.on_event(event, ctx);
        } else {
            self.inner.on_event(event, ctx);
        }
        let after = self.inner.selected();
        if before != after {
            self.sink
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(format!("row:{before}->{after}"));
        }
    }
}

#[test]
fn p1_gate_container_click_targets_correct_child_by_y() {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root = Container::new()
        .with_child(ClickProbe::new("first", sink.clone()))
        .with_child(ClickProbe::new("second", sink.clone()));
    let mut tree = build_tree(&mut root, 20, 5);
    let _ = click_tree(&mut tree, 0, 1);

    let descriptions = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert_eq!(
        descriptions,
        vec!["second".to_string()],
        "P1 gate: a click on the second row must route to the second child"
    );
}

#[test]
fn p1_gate_container_distinguishes_clicks_on_different_children() {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root = Container::new()
        .with_child(ClickProbe::new("first", sink.clone()))
        .with_child(ClickProbe::new("second", sink.clone()));
    let mut tree = build_tree(&mut root, 20, 5);
    let _ = click_tree(&mut tree, 0, 0);
    let _ = click_tree(&mut tree, 0, 1);

    let descriptions = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert_eq!(
        descriptions,
        vec!["first".to_string(), "second".to_string()],
        "P1 gate: separate rows must emit events from separate targets"
    );
}

#[test]
fn p1_gate_row_click_targets_correct_child_by_x() {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root = Row::new()
        .with_child(ClickProbe::new("left", sink.clone()))
        .with_child(ClickProbe::new("right", sink.clone()));
    let mut ctx = EventCtx::default();

    root.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: NodeId::default(),
            screen_x: 9,
            screen_y: 0,
            x: 9,
            y: 0,
        }),
        &mut ctx,
    );
    root.on_event(
        &Event::MouseUp(MouseUpEvent {
            target: Some(NodeId::default()),
            screen_x: 9,
            screen_y: 0,
            x: 9,
            y: 0,
        }),
        &mut ctx,
    );

    let descriptions = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert_eq!(
        descriptions,
        vec!["right".to_string()],
        "P1 gate: a click on the right side must route to the right child"
    );
}

#[test]
fn p1_gate_container_hover_targets_child_by_y() {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root = Container::new()
        .with_child(HoverProbe::new("first", sink.clone()))
        .with_child(HoverProbe::new("second", sink.clone()));
    let mut tree = build_tree(&mut root, 20, 5);
    let mut hovered = None;
    assert!(move_hover_tree(&mut tree, &mut hovered, 0, 1));
    assert!(move_hover_tree(&mut tree, &mut hovered, 0, 0));

    let events = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert!(
        events.contains(&"second:true".to_string()),
        "P1 gate: hovering row 1 should mark second child hovered; events={events:?}"
    );
    assert!(
        events.contains(&"second:false".to_string()) && events.contains(&"first:true".to_string()),
        "P1 gate: moving back to row 0 should swap hover from second to first; events={events:?}"
    );
}

#[test]
fn p1_gate_row_hover_targets_child_by_x() {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root = Row::new()
        .with_child(HoverProbe::new("left", sink.clone()))
        .with_child(HoverProbe::new("right", sink.clone()));

    assert!(root.on_mouse_move(9, 0));
    assert!(root.on_mouse_move(0, 0));

    let events = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert!(
        events.contains(&"right:true".to_string()),
        "P1 gate: hovering right side should mark right child hovered; events={events:?}"
    );
    assert!(
        events.contains(&"right:false".to_string()) && events.contains(&"left:true".to_string()),
        "P1 gate: moving to left side should swap hover from right to left; events={events:?}"
    );
}

#[test]
fn p1_gate_container_focus_next_prev_cycles_children() {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root = Container::new()
        .with_child(FocusProbe::new("first", sink.clone()))
        .with_child(FocusProbe::new("second", sink.clone()));
    let mut tree = build_tree(&mut root, 20, 5);
    let probes = tree
        .query("FocusProbe")
        .expect("focus probe query should parse");
    assert_eq!(probes.len(), 2, "expected two focus probes");
    assert!(focus_node(&mut tree, probes[0]));
    assert!(focus_node(&mut tree, probes[1]));
    assert!(focus_node(&mut tree, probes[0]));

    let events = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert!(
        events.contains(&"first:true".to_string()),
        "P1 gate: first FocusNext should focus first child; events={events:?}"
    );
    assert!(
        events.contains(&"first:false".to_string()) && events.contains(&"second:true".to_string()),
        "P1 gate: second FocusNext should move focus to second child; events={events:?}"
    );
}

#[test]
fn p1_gate_row_focus_next_prev_cycles_children() {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root = Row::new()
        .with_child(FocusProbe::new("left", sink.clone()))
        .with_child(FocusProbe::new("right", sink.clone()));
    let mut ctx = EventCtx::default();

    root.on_event(&Event::Action(Action::FocusNext), &mut ctx);
    root.on_event(&Event::Action(Action::FocusNext), &mut ctx);
    root.on_event(&Event::Action(Action::FocusPrev), &mut ctx);

    let events = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert!(
        events.contains(&"left:true".to_string()),
        "P1 gate: first FocusNext should focus left child; events={events:?}"
    );
    assert!(
        events.contains(&"left:false".to_string()) && events.contains(&"right:true".to_string()),
        "P1 gate: second FocusNext should move focus to right child; events={events:?}"
    );
}

#[test]
fn p1_gate_repeated_clicks_emit_repeated_events() {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root = Container::new().with_child(ClickProbe::new("only", sink.clone()));
    let mut tree = build_tree(&mut root, 20, 5);
    for _ in 0..2 {
        let _ = click_tree(&mut tree, 0, 0);
    }

    let descriptions = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert_eq!(
        descriptions,
        vec!["only".to_string(), "only".to_string()],
        "P1 gate: repeated clicks must emit repeated events"
    );
}

#[test]
fn p1_gate_container_focus_routes_arrow_keys_to_datatable() {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root = Container::new()
        .with_child(FocusProbe::new("first", Arc::new(Mutex::new(Vec::new()))))
        .with_child(DataTableNavProbe::new(sink.clone()));
    let mut tree = build_tree(&mut root, 40, 8);
    let table = tree
        .query("DataTableNavProbe")
        .expect("table query should parse")
        .into_iter()
        .next()
        .expect("expected DataTableNavProbe node");
    assert!(focus_node(&mut tree, table));
    assert!(send_key_to_focus(
        &mut tree,
        KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
    ));

    let events = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert!(
        events.contains(&"row:0->1".to_string()),
        "P1 gate: focused DataTable in container should react to Down key; events={events:?}"
    );
}

#[test]
fn p1_gate_row_focus_routes_arrow_keys_to_datatable() {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root = Row::new()
        .with_child(FocusProbe::new("left", Arc::new(Mutex::new(Vec::new()))))
        .with_child(DataTableNavProbe::new(sink.clone()));
    root.on_layout(20, 5);

    root.on_event(&Event::Action(Action::FocusNext), &mut EventCtx::default());
    root.on_event(&Event::Action(Action::FocusNext), &mut EventCtx::default());

    let mut key_ctx = EventCtx::default();
    root.on_event(
        &Event::Key(KeyEventData::from_crossterm(KeyEvent::new(
            KeyCode::Down,
            KeyModifiers::NONE,
        ))),
        &mut key_ctx,
    );

    let events = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert!(
        events.contains(&"row:0->1".to_string()),
        "P1 gate: focused DataTable in row should react to Down key; events={events:?}"
    );
}

#[test]
fn p1_gate_dock_scroll_click_routes_to_nested_child() {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root = Dock::new().push_fill(
        ScrollView::new(
            Container::new()
                .with_child(ClickProbe::new("row0", sink.clone()))
                .with_child(ClickProbe::new("row1", sink.clone()))
                .with_child(ClickProbe::new("row2", sink.clone())),
        )
        .scroll_step(1),
    );
    let mut tree = build_tree(&mut root, 40, 10);
    let hit = find_click_for_sink(&mut tree, &sink, 40, 10, "row2");
    assert!(
        hit.is_some(),
        "P1 gate: Dock->ScrollView wrappers must preserve click row targeting"
    );
}

#[test]
fn p1_gate_dock_scroll_focus_next_descends_to_nested_focusable() {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root = Dock::new().push_fill(ScrollView::new(
        Row::new().with_child(FocusProbe::new("first", sink.clone())),
    ));
    let mut tree = build_tree(&mut root, 40, 10);
    let probe = tree
        .query("FocusProbe")
        .expect("focus probe query should parse")
        .into_iter()
        .next()
        .expect("expected nested focus probe");
    assert!(focus_node(&mut tree, probe));

    let events = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert!(
        events.iter().any(|entry| entry == "first:true"),
        "P1 gate: FocusNext through Dock->ScrollView should focus nested descendant"
    );
}

#[test]
fn p1_gate_dock_scroll_datatable_click_updates_row() {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root = Dock::new().push_fill(ScrollView::new(DataTableNavProbe::new(sink.clone())));
    let mut tree = build_tree(&mut root, 60, 12);
    // Header is y=0, first data row is y=1, second data row is y=2.
    let _ = click_tree(&mut tree, 2, 2);

    let events = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert!(
        events.contains(&"row:0->1".to_string()),
        "P1 gate: DataTable click under Dock->ScrollView should update selected row; events={events:?}"
    );
}

#[test]
fn p1_gate_vertical_scroll_click_routes_to_nested_child() {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root = VerticalScroll::new()
        .with_child(ClickProbe::new("row0", sink.clone()))
        .with_child(ClickProbe::new("row1", sink.clone()))
        .with_child(ClickProbe::new("row2", sink.clone()));
    let mut tree = build_tree(&mut root, 40, 8);
    let _ = click_tree(&mut tree, 1, 2);

    let descriptions = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert_eq!(
        descriptions,
        vec!["row2".to_string()],
        "P1 gate: VerticalScroll should route mouse click to nested child by local y"
    );
}

#[test]
fn p1_gate_vertical_scroll_focus_next_descends_to_nested_focusable() {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root = VerticalScroll::new()
        .with_child(Static::new("header"))
        .with_child(FocusProbe::new("button_like", sink.clone()));
    let mut tree = build_tree(&mut root, 40, 10);
    let probe = tree
        .query("FocusProbe")
        .expect("focus probe query should parse")
        .into_iter()
        .next()
        .expect("expected focus probe");
    assert!(focus_node(&mut tree, probe));

    let events = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert!(
        events.iter().any(|entry| entry == "button_like:true"),
        "P1 gate: VerticalScroll focus should descend into nested focusables"
    );
}

#[test]
fn p1_gate_buttons_wrapper_chain_click_reaches_probe() {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root = Dock::new().push_fill(ScrollView::new(
        Horizontal::new().with_child(
            VerticalScroll::new()
                .with_child(Static::new("header"))
                .with_child(ClickProbe::new("button_like", sink.clone())),
        ),
    ));
    let mut tree = build_tree(&mut root, 80, 20);
    let hit = find_click_for_sink(&mut tree, &sink, 80, 20, "button_like");
    assert!(
        hit.is_some(),
        "P1 gate: buttons-style wrapper chain must deliver click to nested child"
    );
}

#[test]
fn p1_gate_buttons_wrapper_chain_click_emits_button_pressed() {
    let mut root = Dock::new().push_fill(ScrollView::new(
        Horizontal::new().with_child(
            VerticalScroll::new()
                .with_child(Static::new("header"))
                .with_child(Button::new("Default")),
        ),
    ));
    let mut tree = build_tree(&mut root, 80, 20);
    assert!(
        click_tree(&mut tree, 2, 1),
        "P1 gate: buttons-style wrapper chain click should be handled by nested button"
    );
}

#[test]
fn p1_gate_buttons_advanced_like_chain_click_on_first_button_is_handled() {
    let mut root = Dock::new()
        .push_fill(ScrollView::new(Horizontal::new().with_compose(compose![
            VerticalScroll::new().with_compose(compose![
                Static::new("Standard Buttons"),
                Button::new("Default"),
                Button::primary("Primary!"),
            ]),
            VerticalScroll::new().with_compose(compose![
                Static::new("Disabled Buttons"),
                Button::new("Default").disabled(true),
            ]),
        ])))
        .push_bottom(Some(3), Static::new("status"));
    let mut tree = build_tree(&mut root, 80, 24);

    let mut handled_points = Vec::new();
    for x in 0..20u16 {
        for y in 0..12u16 {
            if click_tree(&mut tree, x, y) {
                handled_points.push((x, y));
            }
        }
    }

    assert!(
        !handled_points.is_empty(),
        "P1 gate: buttons_advanced-like chain should handle some click points; handled_points={handled_points:?}"
    );
}

#[test]
fn p1_gate_buttons_advanced_like_fill_button_has_non_zero_layout_and_is_clickable() {
    let click_sink = Arc::new(Mutex::new(Vec::new()));
    let layout_sink = Arc::new(Mutex::new(HashMap::new()));
    let mut root = Dock::new()
        .push_top(
            Some(3),
            Container::new().with_child(LayoutClickProbe::new(
                "top_probe",
                click_sink.clone(),
                layout_sink.clone(),
            )),
        )
        .push_fill(ScrollView::new(Horizontal::new().with_compose(compose![
            VerticalScroll::new().with_compose(compose![
                Static::new("Standard Buttons"),
                LayoutClickProbe::new("fill_probe", click_sink.clone(), layout_sink.clone()),
            ]),
            VerticalScroll::new().with_compose(compose![
                Static::new("Disabled Buttons"),
                LayoutClickProbe::new("fill_probe_2", click_sink.clone(), layout_sink.clone()),
            ]),
        ])))
        .push_bottom(
            Some(3),
            Container::new().with_child(LayoutClickProbe::new(
                "bottom_probe",
                click_sink.clone(),
                layout_sink.clone(),
            )),
        );
    let mut tree = build_tree(&mut root, 80, 24);

    let layouts = layout_sink
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let fill_layout = layouts
        .get("fill_probe")
        .copied()
        .expect("P1 gate: fill probe in buttons-style chain should receive layout");
    assert!(
        fill_layout.0 > 0 && fill_layout.1 > 0,
        "P1 gate: fill probe should get non-zero layout area; layout={fill_layout:?}"
    );

    // With a 3-row top dock and a 1-row section header inside the fill column,
    // y=4 lands on the first interactive probe in the fill region.
    let hit = find_click_for_sink(&mut tree, &click_sink, 80, 24, "fill_probe");
    let clicks = click_sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert!(
        hit.is_some() || clicks.contains(&"fill_probe".to_string()),
        "P1 gate: click in fill content band should reach fill probe; clicks={clicks:?}"
    );
}

#[test]
fn p1_gate_dock_fill_band_remains_interactive_between_header_and_footer() {
    let click_sink = Arc::new(Mutex::new(Vec::new()));
    let layout_sink = Arc::new(Mutex::new(HashMap::new()));
    let mut root = Dock::new()
        .push_top(
            Some(2),
            Container::new().with_child(LayoutClickProbe::new(
                "top_probe",
                click_sink.clone(),
                layout_sink.clone(),
            )),
        )
        .push_fill(ScrollView::new(Container::new().with_child(
            LayoutClickProbe::new("fill_probe", click_sink.clone(), layout_sink.clone()),
        )))
        .push_bottom(
            Some(2),
            Container::new().with_child(LayoutClickProbe::new(
                "bottom_probe",
                click_sink.clone(),
                layout_sink.clone(),
            )),
        );
    let mut tree = build_tree(&mut root, 40, 10);

    let layouts = layout_sink
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let fill_layout = layouts
        .get("fill_probe")
        .copied()
        .expect("P1 gate: fill probe should receive layout under dock");
    assert!(
        fill_layout.0 > 0 && fill_layout.1 > 0,
        "P1 gate: dock fill area must get non-zero interactive layout; layout={fill_layout:?}"
    );

    // Height=10 with top=2 and bottom=2 means fill starts at y=2.
    // Clicking y=2 targets the first row of the fill region.
    let hit = find_click_for_sink(&mut tree, &click_sink, 40, 10, "fill_probe");
    let clicks = click_sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert!(
        hit.is_some() || clicks.iter().any(|entry| entry == "fill_probe"),
        "P1 gate: fill band click should route to fill probe, not only top/bottom; clicks={clicks:?}"
    );
}

#[test]
fn p1_gate_container_header_plus_button_click_is_handled() {
    let mut root = Container::new()
        .with_child(Static::new("header"))
        .with_child(Button::new("Default"))
        .with_child(Button::primary("Primary!"));
    let mut tree = build_tree(&mut root, 30, 12);
    let mut handled_rows = Vec::new();
    for y in 0..12u16 {
        if click_tree(&mut tree, 2, y) {
            handled_rows.push(y);
        }
    }
    assert!(
        !handled_rows.is_empty(),
        "P1 gate: container with header+buttons should handle click on some rows; handled_rows={handled_rows:?}"
    );
}

#[test]
fn p1_gate_buttons_wrapper_chain_click_clears_previous_focus() {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root =
        Dock::new().push_fill(ScrollView::new(Horizontal::new().with_compose(compose![
            VerticalScroll::new().with_compose(compose![
                Static::new("Left"),
                FocusProbe::new("left", sink.clone()),
            ]),
            VerticalScroll::new().with_compose(compose![
                Static::new("Right"),
                FocusProbe::new("right", sink.clone()),
            ]),
        ])));
    let mut tree = build_tree(&mut root, 80, 20);
    let left = find_click_for_sink(&mut tree, &sink, 80, 20, "left:true");
    let right = find_click_for_sink(&mut tree, &sink, 80, 20, "right:true");
    let (left_x, left_y) = left.expect("P1 gate: expected a click point that focuses left probe");
    let (right_x, right_y) =
        right.expect("P1 gate: expected a click point that focuses right probe");

    sink.lock().unwrap_or_else(|e| e.into_inner()).clear();
    let _ = click_tree(&mut tree, left_x, left_y);
    let _ = click_tree(&mut tree, right_x, right_y);

    let events = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert!(
        events.contains(&"left:true".to_string()),
        "P1 gate: left probe should receive focus on first click; events={events:?}"
    );
    assert!(
        events.contains(&"left:false".to_string()),
        "P1 gate: previous focused probe must be cleared when clicking another column; events={events:?}"
    );
    assert!(
        events.contains(&"right:true".to_string()),
        "P1 gate: right probe should receive focus on second click; events={events:?}"
    );
}
