//! Integration tests for the reactive attribute system.

use textual::Reactive;
use textual::reactive::{
    MAX_REACTIVE_ITERATIONS, ReactiveCtx, ReactiveFieldDescriptor, ReactiveFlags,
    ReactivePhaseResult, ReactiveWidget, run_reactive_phase,
};

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

// ── P3-08: init = false ─────────────────────────────────────────────

#[derive(Reactive)]
#[allow(dead_code)]
struct InitFalseWidget {
    #[reactive(init = false)]
    value: i32,

    #[reactive(layout, init = false)]
    size: usize,

    #[reactive(watch, init = false)]
    watched: String,

    /// Normal reactive field (init = true, the default)
    #[reactive]
    normal: String,
}

impl InitFalseWidget {
    fn watch_watched(&mut self, _old: &String, new: &String, _ctx: &mut ReactiveCtx) {
        self.normal = format!("watched:{}", new);
    }
}

#[test]
fn init_false_field_has_init_false_flag() {
    let w = InitFalseWidget {
        value: 0,
        size: 0,
        watched: String::new(),
        normal: String::new(),
    };
    let descriptors = w.reactive_field_descriptors();
    // value: reactive(init = false)
    let value_desc = descriptors.iter().find(|d| d.name == "value").unwrap();
    assert!(value_desc.flags.repaint);
    assert!(!value_desc.flags.layout);
    assert!(!value_desc.flags.init);

    // size: reactive(layout, init = false)
    let size_desc = descriptors.iter().find(|d| d.name == "size").unwrap();
    assert!(size_desc.flags.repaint);
    assert!(size_desc.flags.layout);
    assert!(!size_desc.flags.init);

    // watched: reactive(watch, init = false) — watch doesn't affect flags
    let watched_desc = descriptors.iter().find(|d| d.name == "watched").unwrap();
    assert!(watched_desc.flags.repaint);
    assert!(!watched_desc.flags.init);

    // normal: reactive (init = true default)
    let normal_desc = descriptors.iter().find(|d| d.name == "normal").unwrap();
    assert!(normal_desc.flags.repaint);
    assert!(normal_desc.flags.init);
}

#[test]
fn init_false_setter_records_change_with_no_init_flag() {
    let mut w = InitFalseWidget {
        value: 0,
        size: 0,
        watched: String::new(),
        normal: String::new(),
    };
    let mut ctx = make_ctx();

    w.set_value(42, &mut ctx);
    assert_eq!(ctx.changes().len(), 1);
    assert!(ctx.changes()[0].flags.repaint);
    assert!(!ctx.changes()[0].flags.init);
}

#[test]
fn init_false_with_watch_still_calls_watcher() {
    let mut w = InitFalseWidget {
        value: 0,
        size: 0,
        watched: "old".into(),
        normal: String::new(),
    };
    let mut ctx = make_ctx();

    w.set_watched("new".to_string(), &mut ctx);
    let changes = ctx.take_changes();
    w.reactive_dispatch(&changes, &mut ctx);

    assert_eq!(w.normal, "watched:new");
}

// ── P3-08: init = true (explicit) ───────────────────────────────────

#[derive(Reactive)]
#[allow(dead_code)]
struct InitTrueWidget {
    #[reactive(init = true)]
    value: i32,
}

#[test]
fn init_true_is_default_behavior() {
    let w = InitTrueWidget { value: 0 };
    let descriptors = w.reactive_field_descriptors();
    let desc = descriptors.iter().find(|d| d.name == "value").unwrap();
    assert!(desc.flags.init);
    assert!(desc.flags.repaint);
}

// ── P3-06: computed fields ──────────────────────────────────────────

#[derive(Reactive)]
struct ComputedWidget {
    #[reactive]
    first_name: String,

    #[reactive]
    last_name: String,

    #[computed(depends_on = "first_name, last_name")]
    full_name: String,
}

impl ComputedWidget {
    fn compute_full_name(&self) -> String {
        format!("{} {}", self.first_name, self.last_name)
    }
}

#[test]
fn computed_field_getter_returns_cached_value() {
    let w = ComputedWidget {
        first_name: "Alice".into(),
        last_name: "Smith".into(),
        full_name: "Alice Smith".into(), // initial cached value
    };
    assert_eq!(w.full_name(), "Alice Smith");
}

#[test]
fn computed_field_recomputes_on_dependency_change() {
    let mut w = ComputedWidget {
        first_name: "Alice".into(),
        last_name: "Smith".into(),
        full_name: "Alice Smith".into(),
    };
    let mut ctx = make_ctx();

    // Change first_name
    w.set_first_name("Bob".to_string(), &mut ctx);
    let changes = ctx.take_changes();
    ctx.clear_flags();
    w.reactive_dispatch(&changes, &mut ctx);

    // The computed field should have been recomputed
    assert_eq!(w.full_name(), "Bob Smith");
    // And a change should have been recorded for full_name
    assert!(ctx.changes().iter().any(|c| c.field_name == "full_name"));
}

#[test]
fn computed_field_recomputes_on_other_dependency() {
    let mut w = ComputedWidget {
        first_name: "Alice".into(),
        last_name: "Smith".into(),
        full_name: "Alice Smith".into(),
    };
    let mut ctx = make_ctx();

    // Change last_name
    w.set_last_name("Jones".to_string(), &mut ctx);
    let changes = ctx.take_changes();
    ctx.clear_flags();
    w.reactive_dispatch(&changes, &mut ctx);

    assert_eq!(w.full_name(), "Alice Jones");
}

#[test]
fn computed_field_no_change_if_result_same() {
    let mut w = ComputedWidget {
        first_name: "Alice".into(),
        last_name: "Smith".into(),
        full_name: "Alice Smith".into(),
    };
    let mut ctx = make_ctx();

    // Set first_name to the same value → no change recorded
    w.set_first_name("Alice".to_string(), &mut ctx);
    assert!(ctx.changes().is_empty()); // No change at all
}

// ── P3-06: computed with single dependency ──────────────────────────

#[derive(Reactive)]
struct SingleDepComputed {
    #[reactive]
    count: i32,

    #[computed(depends_on = "count")]
    doubled: i32,
}

impl SingleDepComputed {
    fn compute_doubled(&self) -> i32 {
        self.count * 2
    }
}

#[test]
fn single_dep_computed_recomputes() {
    let mut w = SingleDepComputed {
        count: 5,
        doubled: 10,
    };
    let mut ctx = make_ctx();

    w.set_count(7, &mut ctx);
    let changes = ctx.take_changes();
    ctx.clear_flags();
    w.reactive_dispatch(&changes, &mut ctx);

    assert_eq!(*w.doubled(), 14);
}

// ── P3-09: run_reactive_phase ───────────────────────────────────────

#[derive(Reactive)]
struct PhaseWidget {
    #[reactive(watch)]
    trigger: i32,

    #[reactive]
    side_effect: String,
}

impl PhaseWidget {
    fn watch_trigger(&mut self, _old: &i32, new: &i32, _ctx: &mut ReactiveCtx) {
        self.side_effect = format!("triggered:{}", new);
    }
}

#[test]
fn run_reactive_phase_processes_changes() {
    let mut w = PhaseWidget {
        trigger: 0,
        side_effect: String::new(),
    };
    let mut ctx = make_ctx();

    w.set_trigger(42, &mut ctx);
    let result = run_reactive_phase(&mut w, &mut ctx);

    assert!(result.had_changes);
    assert!(result.needs_repaint);
    assert!(!result.needs_layout);
    assert!(!result.cycle_detected);
    assert!(result.iterations >= 1);
    assert_eq!(w.side_effect, "triggered:42");
}

#[test]
fn run_reactive_phase_no_changes_is_noop() {
    let mut w = PhaseWidget {
        trigger: 0,
        side_effect: String::new(),
    };
    let mut ctx = make_ctx();

    let result = run_reactive_phase(&mut w, &mut ctx);

    assert!(!result.had_changes);
    assert!(!result.needs_repaint);
    assert!(!result.needs_layout);
    assert!(!result.cycle_detected);
    assert_eq!(result.iterations, 0);
}

// ── P3-09: cycle detection ──────────────────────────────────────────

#[derive(Reactive)]
struct CycleWidget {
    #[reactive(watch)]
    a: i32,

    #[reactive(watch)]
    b: i32,
}

impl CycleWidget {
    fn watch_a(&mut self, _old: &i32, new: &i32, ctx: &mut ReactiveCtx) {
        // Changing b from a's watcher creates a potential cycle.
        self.set_b(*new + 1, ctx);
    }

    fn watch_b(&mut self, _old: &i32, new: &i32, ctx: &mut ReactiveCtx) {
        // Changing a from b's watcher completes the cycle.
        self.set_a(*new + 1, ctx);
    }
}

#[test]
fn cycle_detection_limits_iterations() {
    let mut w = CycleWidget { a: 0, b: 0 };
    let mut ctx = make_ctx();

    w.set_a(1, &mut ctx);
    let result = run_reactive_phase(&mut w, &mut ctx);

    assert!(result.had_changes);
    assert!(result.cycle_detected);
    assert_eq!(result.iterations, MAX_REACTIVE_ITERATIONS);
}

// ── P3-09: cascading watchers (no cycle) ────────────────────────────

#[derive(Reactive)]
struct CascadeWidget {
    #[reactive(watch)]
    input: i32,

    #[reactive(watch)]
    intermediate: i32,

    #[reactive]
    output: i32,
}

impl CascadeWidget {
    fn watch_input(&mut self, _old: &i32, new: &i32, ctx: &mut ReactiveCtx) {
        self.set_intermediate(*new * 2, ctx);
    }

    fn watch_intermediate(&mut self, _old: &i32, new: &i32, _ctx: &mut ReactiveCtx) {
        // Direct mutation (no setter) → no further reactive chain.
        self.output = *new + 10;
    }
}

#[test]
fn cascading_watchers_converge() {
    let mut w = CascadeWidget {
        input: 0,
        intermediate: 0,
        output: 0,
    };
    let mut ctx = make_ctx();

    w.set_input(5, &mut ctx);
    let result = run_reactive_phase(&mut w, &mut ctx);

    assert!(result.had_changes);
    assert!(!result.cycle_detected);
    // input=5 → watch_input sets intermediate=10 → watch_intermediate sets output=20
    assert_eq!(w.output, 20);
    assert_eq!(*w.intermediate(), 10);
}

// ── P3-07 verification: #[var] ─────────────────────────────────────

#[test]
fn var_field_descriptors_have_correct_flags() {
    let w = BasicWidget {
        label: String::new(),
        size: 0,
        counter: 0,
        _non_reactive: false,
    };
    let descriptors = w.reactive_field_descriptors();
    let counter_desc = descriptors.iter().find(|d| d.name == "counter").unwrap();
    assert!(!counter_desc.flags.repaint);
    assert!(!counter_desc.flags.layout);
    assert!(!counter_desc.flags.init);
}

// ── ReactiveFlags new constructors ──────────────────────────────────

#[test]
fn reactive_no_init_flags() {
    let flags = ReactiveFlags::reactive_no_init();
    assert!(flags.repaint);
    assert!(!flags.layout);
    assert!(!flags.init);
}

#[test]
fn reactive_layout_no_init_flags() {
    let flags = ReactiveFlags::reactive_layout_no_init();
    assert!(flags.repaint);
    assert!(flags.layout);
    assert!(!flags.init);
}

// ── ReactiveFieldDescriptor from derive ─────────────────────────────

#[test]
fn basic_widget_has_correct_descriptors() {
    let w = BasicWidget {
        label: String::new(),
        size: 0,
        counter: 0,
        _non_reactive: false,
    };
    let descriptors = w.reactive_field_descriptors();
    assert_eq!(descriptors.len(), 3); // label, size, counter

    let label_desc = descriptors.iter().find(|d| d.name == "label").unwrap();
    assert!(label_desc.flags.repaint);
    assert!(!label_desc.flags.layout);
    assert!(label_desc.flags.init);

    let size_desc = descriptors.iter().find(|d| d.name == "size").unwrap();
    assert!(size_desc.flags.repaint);
    assert!(size_desc.flags.layout);
    assert!(size_desc.flags.init);
}
