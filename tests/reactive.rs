//! Integration tests for the reactive attribute system.

use textual::reactive::{ReactiveCtx, ReactiveFlags, ReactiveWidget};
use textual::Reactive;

// ── Basic derive + getters/setters ──────────────────────────────────

#[derive(Reactive)]
struct BasicWidget {
    #[reactive]
    label: String,

    #[reactive(layout)]
    size: usize,

    #[var]
    counter: u32,

    /// A field without any reactive annotation — should be ignored.
    _non_reactive: bool,
}

fn make_ctx() -> ReactiveCtx {
    use slotmap::SlotMap;
    let mut sm: SlotMap<textual::NodeId, ()> = SlotMap::new();
    let id = sm.insert(());
    ReactiveCtx::new(id)
}

#[test]
fn getters_return_field_references() {
    let w = BasicWidget {
        label: "hello".into(),
        size: 42,
        counter: 7,
        _non_reactive: false,
    };
    assert_eq!(w.label(), "hello");
    assert_eq!(*w.size(), 42);
    assert_eq!(*w.counter(), 7);
}

#[test]
fn setter_records_change_when_value_differs() {
    let mut w = BasicWidget {
        label: "old".into(),
        size: 10,
        counter: 0,
        _non_reactive: false,
    };
    let mut ctx = make_ctx();

    w.set_label("new".to_string(), &mut ctx);
    assert_eq!(w.label(), "new");
    assert_eq!(ctx.changes().len(), 1);
    assert_eq!(ctx.changes()[0].field_name, "label");
    assert!(ctx.needs_repaint());
}

#[test]
fn setter_does_not_record_change_when_value_unchanged() {
    let mut w = BasicWidget {
        label: "same".into(),
        size: 10,
        counter: 0,
        _non_reactive: false,
    };
    let mut ctx = make_ctx();

    w.set_label("same".to_string(), &mut ctx);
    assert!(ctx.changes().is_empty());
    assert!(!ctx.needs_repaint());
}

#[test]
fn reactive_layout_sets_layout_flag() {
    let mut w = BasicWidget {
        label: String::new(),
        size: 10,
        counter: 0,
        _non_reactive: false,
    };
    let mut ctx = make_ctx();

    w.set_size(20, &mut ctx);
    assert!(ctx.needs_repaint());
    assert!(ctx.needs_layout());
    assert_eq!(ctx.changes()[0].field_name, "size");
    assert!(ctx.changes()[0].flags.layout);
}

#[test]
fn var_does_not_set_repaint_flag() {
    let mut w = BasicWidget {
        label: String::new(),
        size: 0,
        counter: 0,
        _non_reactive: false,
    };
    let mut ctx = make_ctx();

    w.set_counter(1, &mut ctx);
    assert!(!ctx.needs_repaint());
    assert!(!ctx.needs_layout());
    assert_eq!(ctx.changes().len(), 1);
    assert_eq!(ctx.changes()[0].field_name, "counter");
    assert!(!ctx.changes()[0].flags.repaint);
}

// ── Watcher dispatch ────────────────────────────────────────────────

#[derive(Reactive)]
struct WatcherWidget {
    #[reactive(watch)]
    label: String,

    #[reactive(layout, watch)]
    width: usize,

    /// No watch — should not generate watcher call.
    #[reactive]
    color: String,
}

impl WatcherWidget {
    fn watch_label(&mut self, _old: &String, new: &String, _ctx: &mut ReactiveCtx) {
        // Store a side-effect we can check in tests.
        self.color = format!("watched:{}", new);
    }

    fn watch_width(&mut self, old: &usize, new: &usize, _ctx: &mut ReactiveCtx) {
        self.color = format!("width:{}→{}", old, new);
    }
}

#[test]
fn watcher_called_for_watch_field() {
    let mut w = WatcherWidget {
        label: "old".into(),
        width: 10,
        color: String::new(),
    };
    let mut ctx = make_ctx();

    w.set_label("new".to_string(), &mut ctx);
    let changes = ctx.take_changes();
    w.reactive_dispatch(&changes, &mut ctx);

    assert_eq!(w.color, "watched:new");
}

#[test]
fn watcher_called_for_layout_watch_field() {
    let mut w = WatcherWidget {
        label: String::new(),
        width: 10,
        color: String::new(),
    };
    let mut ctx = make_ctx();

    w.set_width(20, &mut ctx);
    let changes = ctx.take_changes();
    w.reactive_dispatch(&changes, &mut ctx);

    assert_eq!(w.color, "width:10→20");
}

#[test]
fn no_watcher_called_for_non_watch_field() {
    let mut w = WatcherWidget {
        label: String::new(),
        width: 10,
        color: "untouched".into(),
    };
    let mut ctx = make_ctx();

    w.set_color("red".to_string(), &mut ctx);
    let changes = ctx.take_changes();
    w.reactive_dispatch(&changes, &mut ctx);

    // color was set to "red" by the setter, but no watcher was called
    // so it should remain "red" (not "watched:..." or "width:...")
    assert_eq!(w.color, "red");
}

// ── Multiple changes accumulate ─────────────────────────────────────

#[test]
fn multiple_changes_accumulate_in_ctx() {
    let mut w = BasicWidget {
        label: "a".into(),
        size: 1,
        counter: 0,
        _non_reactive: false,
    };
    let mut ctx = make_ctx();

    w.set_label("b".to_string(), &mut ctx);
    w.set_size(2, &mut ctx);
    w.set_counter(1, &mut ctx);

    assert_eq!(ctx.changes().len(), 3);
    assert!(ctx.needs_repaint()); // from label and size
    assert!(ctx.needs_layout()); // from size
}

// ── Old/new value downcasting ───────────────────────────────────────

#[test]
fn change_values_can_be_downcast() {
    let mut w = BasicWidget {
        label: "old".into(),
        size: 0,
        counter: 0,
        _non_reactive: false,
    };
    let mut ctx = make_ctx();

    w.set_label("new".to_string(), &mut ctx);

    let change = &ctx.changes()[0];
    let old = change.old_value.downcast_ref::<String>().unwrap();
    let new = change.new_value.downcast_ref::<String>().unwrap();
    assert_eq!(old, "old");
    assert_eq!(new, "new");
}

// ── No reactive fields → no-op trait impl ───────────────────────────

#[derive(Reactive)]
#[allow(dead_code)]
struct EmptyWidget {
    plain_field: i32,
}

#[test]
fn empty_widget_reactive_dispatch_is_noop() {
    let mut w = EmptyWidget { plain_field: 42 };
    let mut ctx = make_ctx();
    // Should compile and not panic.
    w.reactive_dispatch(&[], &mut ctx);
}

// ── ReactiveCtx unit behavior ───────────────────────────────────────

#[test]
fn ctx_take_changes_leaves_empty() {
    let mut ctx = make_ctx();
    ctx.record_change(
        "x",
        ReactiveFlags::reactive(),
        Box::new(0_i32),
        Box::new(1_i32),
    );
    let taken = ctx.take_changes();
    assert_eq!(taken.len(), 1);
    assert!(ctx.changes().is_empty());
    // Flags remain set even after take.
    assert!(ctx.needs_repaint());
}

#[test]
fn ctx_clear_flags_resets_state() {
    let mut ctx = make_ctx();
    ctx.record_change(
        "x",
        ReactiveFlags::reactive_layout(),
        Box::new(0_i32),
        Box::new(1_i32),
    );
    ctx.clear_flags();
    assert!(!ctx.needs_repaint());
    assert!(!ctx.needs_layout());
}
