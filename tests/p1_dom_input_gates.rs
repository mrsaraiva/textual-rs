use rich_rs::{Console, ConsoleOptions, Segments};
use textual::event::{Event, EventCtx, MouseDownEvent, MouseUpEvent};
use textual::node_id::NodeId;
use textual::prelude::*;
use std::sync::{Arc, Mutex};

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
            Event::MouseUp(mouse)
                if self.pressed && mouse.target == Some(NodeId::default()) =>
            {
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
