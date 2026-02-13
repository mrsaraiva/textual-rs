use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rich_rs::{Console, ConsoleOptions, Segments};
use std::sync::{Arc, Mutex};
use textual::event::{Action, Event, EventCtx, MouseDownEvent, MouseUpEvent};
use textual::keys::KeyEventData;
use textual::node_id::NodeId;
use textual::prelude::*;

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

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::MouseDown(mouse) if mouse.target == NodeId::default() => {
                self.pressed = true;
                ctx.set_handled();
            }
            Event::MouseUp(mouse) if self.pressed && mouse.target == Some(NodeId::default()) => {
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
}

impl Widget for HoverProbe {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }

    fn is_hovered(&self) -> bool {
        self.hovered
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
}

impl Widget for FocusProbe {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn focusable(&self) -> bool {
        true
    }

    fn has_focus(&self) -> bool {
        self.focused
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

struct DataTableNavProbe {
    inner: DataTable,
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
            sink,
        }
    }
}

impl Widget for DataTableNavProbe {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.inner.render(console, options)
    }

    fn focusable(&self) -> bool {
        self.inner.focusable()
    }

    fn has_focus(&self) -> bool {
        self.inner.has_focus()
    }

    fn set_focus(&mut self, focused: bool) {
        self.inner.set_focus(focused);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        let before = self.inner.selected();
        self.inner.on_event(event, ctx);
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
    let mut ctx = EventCtx::default();

    root.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: NodeId::default(),
            screen_x: 0,
            screen_y: 1,
            x: 0,
            y: 1,
        }),
        &mut ctx,
    );
    root.on_event(
        &Event::MouseUp(MouseUpEvent {
            target: Some(NodeId::default()),
            screen_x: 0,
            screen_y: 1,
            x: 0,
            y: 1,
        }),
        &mut ctx,
    );

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
    let mut ctx = EventCtx::default();

    // Click row 0.
    root.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: NodeId::default(),
            screen_x: 0,
            screen_y: 0,
            x: 0,
            y: 0,
        }),
        &mut ctx,
    );
    root.on_event(
        &Event::MouseUp(MouseUpEvent {
            target: Some(NodeId::default()),
            screen_x: 0,
            screen_y: 0,
            x: 0,
            y: 0,
        }),
        &mut ctx,
    );

    // Click row 1.
    root.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: NodeId::default(),
            screen_x: 0,
            screen_y: 1,
            x: 0,
            y: 1,
        }),
        &mut ctx,
    );
    root.on_event(
        &Event::MouseUp(MouseUpEvent {
            target: Some(NodeId::default()),
            screen_x: 0,
            screen_y: 1,
            x: 0,
            y: 1,
        }),
        &mut ctx,
    );

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

    assert!(root.on_mouse_move(0, 1));
    assert!(root.on_mouse_move(0, 0));

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
    let mut ctx = EventCtx::default();

    root.on_event(&Event::Action(Action::FocusNext), &mut ctx);
    root.on_event(&Event::Action(Action::FocusNext), &mut ctx);
    root.on_event(&Event::Action(Action::FocusPrev), &mut ctx);

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
    let mut ctx = EventCtx::default();

    for _ in 0..2 {
        root.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: NodeId::default(),
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut ctx,
        );
        root.on_event(
            &Event::MouseUp(MouseUpEvent {
                target: Some(NodeId::default()),
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut ctx,
        );
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
