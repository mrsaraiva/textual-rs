use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rich_rs::{Console, ConsoleOptions, Segments};
use std::sync::{Arc, Mutex};
use textual::compose;
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

    fn on_layout(&mut self, width: u16, height: u16) {
        self.inner.on_layout(width, height);
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
    let console = Console::new();
    let mut opts = console.options().clone();
    opts.size = (40, 10);
    opts.max_width = 40;
    opts.max_height = 10;
    let _ = root.render(&console, &opts);

    let mut ctx = EventCtx::default();
    root.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: NodeId::default(),
            screen_x: 2,
            screen_y: 2,
            x: 2,
            y: 2,
        }),
        &mut ctx,
    );
    root.on_event(
        &Event::MouseUp(MouseUpEvent {
            target: Some(NodeId::default()),
            screen_x: 2,
            screen_y: 2,
            x: 2,
            y: 2,
        }),
        &mut ctx,
    );

    let descriptions = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert_eq!(
        descriptions,
        vec!["row2".to_string()],
        "P1 gate: Dock->ScrollView wrappers must preserve click row targeting"
    );
}

#[test]
fn p1_gate_dock_scroll_focus_next_descends_to_nested_focusable() {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let mut root = Dock::new().push_fill(ScrollView::new(
        Row::new().with_child(FocusProbe::new("first", sink.clone())),
    ));

    root.set_focus(true);
    let mut ctx = EventCtx::default();
    root.on_event(&Event::Action(Action::FocusNext), &mut ctx);

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
    let console = Console::new();
    let mut opts = console.options().clone();
    opts.size = (60, 12);
    opts.max_width = 60;
    opts.max_height = 12;
    let _ = root.render(&console, &opts);

    let mut ctx = EventCtx::default();
    // Header is y=0, first data row is y=1, second data row is y=2.
    root.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: NodeId::default(),
            screen_x: 2,
            screen_y: 2,
            x: 2,
            y: 2,
        }),
        &mut ctx,
    );

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
    let console = Console::new();
    let mut opts = console.options().clone();
    opts.size = (40, 8);
    opts.max_width = 40;
    opts.max_height = 8;
    let _ = root.render(&console, &opts);

    let mut ctx = EventCtx::default();
    root.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: NodeId::default(),
            screen_x: 1,
            screen_y: 2,
            x: 1,
            y: 2,
        }),
        &mut ctx,
    );
    root.on_event(
        &Event::MouseUp(MouseUpEvent {
            target: Some(NodeId::default()),
            screen_x: 1,
            screen_y: 2,
            x: 1,
            y: 2,
        }),
        &mut ctx,
    );

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

    root.set_focus(true);
    let mut ctx = EventCtx::default();
    root.on_event(&Event::Action(Action::FocusNext), &mut ctx);

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
    let console = Console::new();
    let mut opts = console.options().clone();
    opts.size = (80, 20);
    opts.max_width = 80;
    opts.max_height = 20;
    let _ = root.render(&console, &opts);

    let mut ctx = EventCtx::default();
    root.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: NodeId::default(),
            screen_x: 2,
            screen_y: 1,
            x: 2,
            y: 1,
        }),
        &mut ctx,
    );
    root.on_event(
        &Event::MouseUp(MouseUpEvent {
            target: Some(NodeId::default()),
            screen_x: 2,
            screen_y: 1,
            x: 2,
            y: 1,
        }),
        &mut ctx,
    );

    let descriptions = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert_eq!(
        descriptions,
        vec!["button_like".to_string()],
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
    let console = Console::new();
    let mut opts = console.options().clone();
    opts.size = (80, 20);
    opts.max_width = 80;
    opts.max_height = 20;
    let _ = root.render(&console, &opts);

    let mut ctx = EventCtx::default();
    root.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: NodeId::default(),
            screen_x: 2,
            screen_y: 1,
            x: 2,
            y: 1,
        }),
        &mut ctx,
    );
    root.on_event(
        &Event::MouseUp(MouseUpEvent {
            target: Some(NodeId::default()),
            screen_x: 2,
            screen_y: 1,
            x: 2,
            y: 1,
        }),
        &mut ctx,
    );

    assert!(
        ctx.handled(),
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
    let console = Console::new();
    let mut opts = console.options().clone();
    opts.size = (80, 24);
    opts.max_width = 80;
    opts.max_height = 24;
    let _ = root.render(&console, &opts);

    let mut handled_points = Vec::new();
    for x in 0..20u16 {
        for y in 0..12u16 {
            let mut ctx = EventCtx::default();
            root.on_event(
                &Event::MouseDown(MouseDownEvent {
                    target: NodeId::default(),
                    screen_x: x,
                    screen_y: y,
                    x,
                    y,
                }),
                &mut ctx,
            );
            root.on_event(
                &Event::MouseUp(MouseUpEvent {
                    target: Some(NodeId::default()),
                    screen_x: x,
                    screen_y: y,
                    x,
                    y,
                }),
                &mut ctx,
            );
            if ctx.handled() {
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
fn p1_gate_container_header_plus_button_click_is_handled() {
    let mut root = Container::new()
        .with_child(Static::new("header"))
        .with_child(Button::new("Default"))
        .with_child(Button::primary("Primary!"));
    let mut handled_rows = Vec::new();
    for y in 0..12u16 {
        let mut ctx = EventCtx::default();
        root.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: NodeId::default(),
                screen_x: 2,
                screen_y: y,
                x: 2,
                y,
            }),
            &mut ctx,
        );
        root.on_event(
            &Event::MouseUp(MouseUpEvent {
                target: Some(NodeId::default()),
                screen_x: 2,
                screen_y: y,
                x: 2,
                y,
            }),
            &mut ctx,
        );
        if ctx.handled() {
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
    let console = Console::new();
    let mut opts = console.options().clone();
    opts.size = (80, 20);
    opts.max_width = 80;
    opts.max_height = 20;
    let _ = root.render(&console, &opts);

    let mut ctx = EventCtx::default();
    // Click left column.
    root.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: NodeId::default(),
            screen_x: 2,
            screen_y: 1,
            x: 2,
            y: 1,
        }),
        &mut ctx,
    );
    root.on_event(
        &Event::MouseUp(MouseUpEvent {
            target: Some(NodeId::default()),
            screen_x: 2,
            screen_y: 1,
            x: 2,
            y: 1,
        }),
        &mut ctx,
    );

    // Click right column.
    root.on_event(
        &Event::MouseDown(MouseDownEvent {
            target: NodeId::default(),
            screen_x: 45,
            screen_y: 1,
            x: 45,
            y: 1,
        }),
        &mut ctx,
    );
    root.on_event(
        &Event::MouseUp(MouseUpEvent {
            target: Some(NodeId::default()),
            screen_x: 45,
            screen_y: 1,
            x: 45,
            y: 1,
        }),
        &mut ctx,
    );

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
