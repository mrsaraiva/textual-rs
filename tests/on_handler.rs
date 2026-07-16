//! Integration tests for the `#[on(MessageType)]` attribute macro.

use textual::event::{EventCtx, WidgetCtx};
use textual::message::{ButtonPressed, CheckboxChanged, MessageEvent};
use textual::node_id::node_id_from_ffi;
use textual::on;

// ---------------------------------------------------------------------------
// Test struct with `#[on]`-annotated handler methods
// ---------------------------------------------------------------------------

struct MyApp {
    button_count: u32,
    last_checkbox: Option<bool>,
    save_count: u32,
}

impl MyApp {
    fn new() -> Self {
        Self {
            button_count: 0,
            last_checkbox: None,
            save_count: 0,
        }
    }

    // Type-only handler: matches any ButtonPressed message.
    #[on(ButtonPressed)]
    fn handle_button(&mut self, event: &ButtonPressed, ctx: &mut WidgetCtx) {
        let _ = (event, ctx);
        self.button_count += 1;
    }

    // Type-only handler for CheckboxChanged.
    #[on(CheckboxChanged)]
    fn handle_checkbox(&mut self, event: &CheckboxChanged, ctx: &mut WidgetCtx) {
        let _ = ctx;
        self.last_checkbox = Some(event.checked);
    }

    // Selector handler: matches ButtonPressed from widget matching "#save".
    #[on(ButtonPressed, selector = "#save")]
    fn handle_save(&mut self, event: &ButtonPressed, ctx: &mut WidgetCtx) {
        let _ = (event, ctx);
        self.save_count += 1;
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn test_ctx() -> EventCtx {
    EventCtx::default()
}

fn dummy_sender() -> textual::node_id::NodeId {
    node_id_from_ffi(1)
}

fn button_event() -> MessageEvent {
    MessageEvent::new(
        dummy_sender(),
        ButtonPressed {
            description: "ok".into(),
            button_id: None,
        },
    )
}

fn checkbox_event(checked: bool) -> MessageEvent {
    MessageEvent::new(dummy_sender(), CheckboxChanged { checked })
}

// ---------------------------------------------------------------------------
// Tests — type-only dispatch (new signature: &MessageEvent, &mut textual::event::WidgetCtx)
// ---------------------------------------------------------------------------

#[test]
fn dispatch_matches_correct_message_type() {
    let mut app = MyApp::new();
    let event = button_event();
    let mut ectx = test_ctx();
    let mut ctx = WidgetCtx::__from_dispatch(dummy_sender(), &mut ectx);

    let matched = app.__on_dispatch_handle_button(&event, &mut ctx);
    assert!(matched);
    assert_eq!(app.button_count, 1);
}

#[test]
fn dispatch_ignores_wrong_message_type() {
    let mut app = MyApp::new();
    let event = checkbox_event(true);
    let mut ectx = test_ctx();
    let mut ctx = WidgetCtx::__from_dispatch(dummy_sender(), &mut ectx);

    let matched = app.__on_dispatch_handle_button(&event, &mut ctx);
    assert!(!matched);
    assert_eq!(app.button_count, 0);
}

#[test]
fn dispatch_checkbox_handler() {
    let mut app = MyApp::new();
    let event = checkbox_event(true);
    let mut ectx = test_ctx();
    let mut ctx = WidgetCtx::__from_dispatch(dummy_sender(), &mut ectx);

    let matched = app.__on_dispatch_handle_checkbox(&event, &mut ctx);
    assert!(matched);
    assert_eq!(app.last_checkbox, Some(true));
}

#[test]
fn dispatch_checkbox_ignores_button() {
    let mut app = MyApp::new();
    let event = button_event();
    let mut ectx = test_ctx();
    let mut ctx = WidgetCtx::__from_dispatch(dummy_sender(), &mut ectx);

    let matched = app.__on_dispatch_handle_checkbox(&event, &mut ctx);
    assert!(!matched);
    assert!(app.last_checkbox.is_none());
}

// ---------------------------------------------------------------------------
// Tests — selector dispatch (same signature)
// ---------------------------------------------------------------------------

#[test]
fn selector_dispatch_matches_matching_control() {
    let mut app = MyApp::new();
    let sender = node_id_from_ffi(42);
    let event = MessageEvent::new(
        sender,
        ButtonPressed {
            description: "save".into(),
            button_id: Some("save".into()),
        },
    );
    let mut ectx = test_ctx();
    let mut ctx = WidgetCtx::__from_dispatch(dummy_sender(), &mut ectx);

    let matched = app.__on_dispatch_handle_save(&event, &mut ctx);
    assert!(matched);
    assert_eq!(app.save_count, 1);
}

#[test]
fn selector_dispatch_skips_message_without_control_identity() {
    // A message with no control id must NOT satisfy a `selector = "#save"`
    // handler. Mirrors Python where `@on(Message, selector)` matches against
    // `message.control` and a non-matching control skips the handler.
    let mut app = MyApp::new();
    let event = button_event(); // button_id: None
    let mut ectx = test_ctx();
    let mut ctx = WidgetCtx::__from_dispatch(dummy_sender(), &mut ectx);

    let matched = app.__on_dispatch_handle_save(&event, &mut ctx);
    assert!(!matched);
    assert_eq!(app.save_count, 0);
}

#[test]
fn selector_dispatch_ignores_wrong_type() {
    let mut app = MyApp::new();
    let sender = node_id_from_ffi(42);
    let event = MessageEvent::new(sender, CheckboxChanged { checked: false });
    let mut ectx = test_ctx();
    let mut ctx = WidgetCtx::__from_dispatch(dummy_sender(), &mut ectx);

    let matched = app.__on_dispatch_handle_save(&event, &mut ctx);
    assert!(!matched);
    assert_eq!(app.save_count, 0);
}

#[test]
fn selector_const_is_generated() {
    assert_eq!(MyApp::__ON_SELECTOR_HANDLE_SAVE, "#save");
}

/// Port of Python `test_on.py::test_on_button_pressed`: with two controls, a
/// selector-filtered handler fires ONLY for the message whose originating
/// control matches the selector, while a type-only handler fires for every
/// message of that type.
#[test]
fn selector_filters_between_two_controls() {
    let mut app = MyApp::new();
    let save_pressed = MessageEvent::new(
        node_id_from_ffi(10),
        ButtonPressed {
            description: "save".into(),
            button_id: Some("save".into()),
        },
    );
    let cancel_pressed = MessageEvent::new(
        node_id_from_ffi(11),
        ButtonPressed {
            description: "cancel".into(),
            button_id: Some("cancel".into()),
        },
    );
    let mut ectx = test_ctx();
    let mut ctx = WidgetCtx::__from_dispatch(dummy_sender(), &mut ectx);

    // Press #save: both the type-only and the "#save" handler fire.
    app.__on_dispatch_handle_button(&save_pressed, &mut ctx);
    app.__on_dispatch_handle_save(&save_pressed, &mut ctx);
    // Press #cancel: only the type-only handler fires.
    app.__on_dispatch_handle_button(&cancel_pressed, &mut ctx);
    let cancel_matched_save = app.__on_dispatch_handle_save(&cancel_pressed, &mut ctx);

    assert!(!cancel_matched_save, "#save handler must skip #cancel");
    assert_eq!(app.button_count, 2, "type-only handler fires for both");
    assert_eq!(app.save_count, 1, "selector handler fires only for #save");
}

// ---------------------------------------------------------------------------
// Tests — multiple dispatches accumulate
// ---------------------------------------------------------------------------

#[test]
fn multiple_dispatches_accumulate() {
    let mut app = MyApp::new();
    let event = button_event();
    let mut ectx = test_ctx();
    let mut ctx = WidgetCtx::__from_dispatch(dummy_sender(), &mut ectx);

    app.__on_dispatch_handle_button(&event, &mut ctx);
    app.__on_dispatch_handle_button(&event, &mut ctx);
    app.__on_dispatch_handle_button(&event, &mut ctx);

    assert_eq!(app.button_count, 3);
}

// ---------------------------------------------------------------------------
// Tests — fine-grained widget messages dispatch via #[on(Type)]
// ---------------------------------------------------------------------------

use textual::message::{
    CollapsibleCollapsed, CollapsibleExpanded, DataTableCellSelected, DataTableRowHighlighted,
    DataTableRowLabelSelected, SelectionListHighlighted,
};

#[derive(Default)]
struct GranularApp {
    row_highlighted: Option<usize>,
    cell_selected: Option<(usize, usize)>,
    row_label_selected: Option<usize>,
    expanded_count: u32,
    collapsed_count: u32,
    selection_highlighted: Option<usize>,
}

impl GranularApp {
    #[on(DataTableRowHighlighted)]
    fn handle_row_highlighted(&mut self, event: &DataTableRowHighlighted, ctx: &mut WidgetCtx) {
        let _ = ctx;
        self.row_highlighted = Some(event.row);
    }

    #[on(DataTableCellSelected)]
    fn handle_cell_selected(&mut self, event: &DataTableCellSelected, ctx: &mut WidgetCtx) {
        let _ = ctx;
        self.cell_selected = Some((event.row, event.column));
    }

    #[on(DataTableRowLabelSelected)]
    fn handle_row_label_selected(
        &mut self,
        event: &DataTableRowLabelSelected,
        ctx: &mut WidgetCtx,
    ) {
        let _ = ctx;
        self.row_label_selected = Some(event.row);
    }

    #[on(CollapsibleExpanded)]
    fn handle_expanded(&mut self, event: &CollapsibleExpanded, ctx: &mut WidgetCtx) {
        let _ = (event, ctx);
        self.expanded_count += 1;
    }

    #[on(CollapsibleCollapsed)]
    fn handle_collapsed(&mut self, event: &CollapsibleCollapsed, ctx: &mut WidgetCtx) {
        let _ = (event, ctx);
        self.collapsed_count += 1;
    }

    #[on(SelectionListHighlighted)]
    fn handle_selection_highlighted(
        &mut self,
        event: &SelectionListHighlighted,
        ctx: &mut WidgetCtx,
    ) {
        let _ = ctx;
        self.selection_highlighted = Some(event.index);
    }
}

/// Each fine-grained message type dispatches to its own `#[on(Type)]` handler
/// and to no other.
#[test]
fn granular_widget_messages_dispatch_via_on() {
    let mut app = GranularApp::default();
    let mut ectx = test_ctx();
    let mut ctx = WidgetCtx::__from_dispatch(dummy_sender(), &mut ectx);

    let row_highlighted = MessageEvent::new(dummy_sender(), DataTableRowHighlighted { row: 3 });
    let cell_selected =
        MessageEvent::new(dummy_sender(), DataTableCellSelected { row: 1, column: 2 });
    let row_label_selected =
        MessageEvent::new(dummy_sender(), DataTableRowLabelSelected { row: 4 });
    let expanded = MessageEvent::new(dummy_sender(), CollapsibleExpanded);
    let collapsed = MessageEvent::new(dummy_sender(), CollapsibleCollapsed);
    let selection_highlighted = MessageEvent::new(
        dummy_sender(),
        SelectionListHighlighted {
            index: 5,
            option_id: None,
        },
    );

    assert!(app.__on_dispatch_handle_row_highlighted(&row_highlighted, &mut ctx));
    assert!(app.__on_dispatch_handle_cell_selected(&cell_selected, &mut ctx));
    assert!(app.__on_dispatch_handle_row_label_selected(&row_label_selected, &mut ctx));
    assert!(app.__on_dispatch_handle_expanded(&expanded, &mut ctx));
    assert!(app.__on_dispatch_handle_collapsed(&collapsed, &mut ctx));
    assert!(app.__on_dispatch_handle_selection_highlighted(&selection_highlighted, &mut ctx));

    assert_eq!(app.row_highlighted, Some(3));
    assert_eq!(app.cell_selected, Some((1, 2)));
    assert_eq!(app.row_label_selected, Some(4));
    assert_eq!(app.expanded_count, 1);
    assert_eq!(app.collapsed_count, 1);
    assert_eq!(app.selection_highlighted, Some(5));

    // Cross-type dispatch does not fire: `Expanded` is distinct from
    // `Collapsed`, and a row highlight is not a cell selection.
    assert!(!app.__on_dispatch_handle_expanded(&collapsed, &mut ctx));
    assert!(!app.__on_dispatch_handle_collapsed(&expanded, &mut ctx));
    assert!(!app.__on_dispatch_handle_cell_selected(&row_highlighted, &mut ctx));
    assert_eq!(app.expanded_count, 1);
    assert_eq!(app.collapsed_count, 1);
}
