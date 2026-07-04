//! Kanban task board — a COLD-USER app built to pressure-test the textual-rs 1.0
//! public API. Imports are ONLY `std` + `textual::prelude::*` (plus `rich-rs`,
//! which the `Widget::render` signature forces on any custom widget). No runtime,
//! reactive, or css internals are reached into.
//!
//! Exercises: `#[widget(base = ...)]` delegation compounds (TaskCard, Column,
//! AutoSaveIndicator, ControlBar), `#[derive(Reactive)]` recompose + watch, a
//! widget-owned timer via `WidgetCtx::set_interval`, widget-scoped self-mutation
//! via `WidgetCtx::query_one_id` + `update_via` + `add_class`, a custom message
//! bubbling from a card to the board (`CardClicked`), a modal add-task screen that
//! dismisses with a typed result, toast notifications, and a `Link` tooltip. It
//! exercises the rebuilt overlay widgets (modal screen, ToastRack, Tooltip).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use textual::prelude::*;

// ===========================================================================
// Model
// ===========================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Priority {
    Low,
    Medium,
    High,
}

impl Priority {
    fn tag(self) -> &'static str {
        match self {
            Priority::Low => "low",
            Priority::Medium => "med",
            Priority::High => "HIGH",
        }
    }
    fn css_class(self) -> &'static str {
        match self {
            Priority::Low => "prio-low",
            Priority::Medium => "prio-med",
            Priority::High => "prio-high",
        }
    }
    fn cycle(self) -> Self {
        match self {
            Priority::Low => Priority::Medium,
            Priority::Medium => Priority::High,
            Priority::High => Priority::Low,
        }
    }
}

#[derive(Debug, Clone)]
struct CardData {
    id: u64,
    title: String,
    priority: Priority,
}

#[derive(Debug, Clone)]
struct ColumnModel {
    title: String,
    cards: Vec<CardData>,
}

impl ColumnModel {
    fn new(title: &str) -> Self {
        Self { title: title.to_string(), cards: Vec::new() }
    }
}

const DONE_COL: usize = 2;

// ===========================================================================
// Messages
// ===========================================================================

/// Bubbles from a `TaskCard` up to the `Board` when a card is clicked, so the
/// board can make it the active card. Mirrors Python `self.post_message(...)`.
#[derive(Debug, Clone)]
struct CardClicked {
    id: u64,
}
textual::impl_message!(CardClicked);

/// Bubbles from the `ControlBar`'s "Add task" button up to the board.
#[derive(Debug, Clone)]
struct AddTaskRequested;
textual::impl_message!(AddTaskRequested);

/// Typed result returned by the modal add-task screen (`dismiss(NewTask)`).
#[derive(Debug, Clone)]
struct NewTask {
    title: String,
    priority: Priority,
}

// ===========================================================================
// TaskCard — a compound widget via the delegation derive; posts CardClicked up
// ===========================================================================

#[textual::widget(base = VerticalGroup, style_type = "TaskCard", override(on_event))]
struct TaskCard {
    base: VerticalGroup,
    id: u64,
}

impl TaskCard {
    fn new(data: &CardData) -> Self {
        let base = VerticalGroup::new().with_compose(vec![
            ChildDecl::from(Label::new(data.title.clone())),
            ChildDecl::new(Box::new(Label::new(data.priority.tag()))).with_classes(&["prio-tag"]),
        ]);
        Self { base, id: data.id }
    }

    // MUST match `Widget::on_event` exactly (override footgun).
    fn on_event(&mut self, event: &Event, ctx: &mut WidgetCtx) {
        if matches!(event, Event::Click(_)) {
            ctx.post_message(CardClicked { id: self.id });
        }
        // Forward to the base so inner widgets still get the event.
        self.base.on_event(event, ctx);
    }
}

// ===========================================================================
// Column — a compound widget holding a dynamic list of TaskCards
// ===========================================================================

#[textual::widget(base = VerticalScroll, style_type = "Column")]
struct Column {
    base: VerticalScroll,
}

impl Column {
    fn new(model: &ColumnModel, active: bool, active_card: usize) -> Self {
        let header = format!("{}  ({})", model.title, model.cards.len());
        let mut children: Vec<ChildDecl> = vec![
            ChildDecl::new(Box::new(Label::new(header))).with_classes(&["col-header"]),
        ];
        for (i, card) in model.cards.iter().enumerate() {
            let mut classes: Vec<&str> = vec![card.priority.css_class()];
            if active && i == active_card {
                classes.push("selected");
            }
            children.push(ChildDecl::new(Box::new(TaskCard::new(card))).with_classes(&classes));
        }
        Self { base: VerticalScroll::new().with_compose(children) }
    }
}

// ===========================================================================
// AutoSaveIndicator — widget-owned timer + widget-level reactive watch
// ===========================================================================

#[textual::widget(base = Static, reactive, override(on_mount), style_type = "AutoSave")]
#[derive(textual::Reactive)]
struct AutoSaveIndicator {
    base: Static,
    #[reactive(watch, init = false)]
    ticks: u64,
    timer: Option<TimerHandle>,
}

impl AutoSaveIndicator {
    fn new() -> Self {
        Self { base: Static::new("auto-save: idle"), ticks: 0, timer: None }
    }

    fn on_mount(&mut self, ctx: &mut WidgetCtx) {
        // Own a 2s interval; each fire bumps the reactive `ticks`.
        self.timer =
            Some(ctx.set_interval(Duration::from_secs(2), false, |w: &mut Self, c, _tick| {
                w.tick(c);
            }));
    }

    fn tick(&mut self, ctx: &mut WidgetCtx) {
        let next = self.ticks + 1;
        self.set_ticks(next, ctx);
    }

    // Widget-level reactive watch: self-mutate the owned base widget.
    fn watch_ticks(&mut self, _old: &u64, new: &u64, _ctx: &mut ReactiveCtx) {
        self.base.update(format!("auto-save: saved #{new}"));
    }
}

// ===========================================================================
// ControlBar — a Button + status Label; the button self-mutates via WidgetCtx
// query_one_id + update_via + add_class, then bubbles AddTaskRequested up
// ===========================================================================

#[textual::widget(base = HorizontalGroup, on(on_add), style_type = "ControlBar")]
struct ControlBar {
    base: HorizontalGroup,
}

impl ControlBar {
    fn new() -> Self {
        let base = HorizontalGroup::new().with_compose(vec![
            ChildDecl::from(Button::primary("Add task").id("add-btn")),
            ChildDecl::from(Label::new("ready").with_id("ctrl-hint")),
        ]);
        Self { base }
    }

    #[textual::on(ButtonPressed)]
    fn on_add(&mut self, event: &ButtonPressed, ctx: &mut WidgetCtx) {
        if event.button_id.as_deref() == Some("add-btn") {
            // Widget-scoped self-mutation from a handler (the "Pomodoro #1" gap):
            // query a sibling child by id and update it, and add a class to self.
            let hint = ctx.query_one_id::<Label>("#ctrl-hint");
            hint.update_via(ctx, |l, _| l.set_text("opening dialog…"));
            ctx.add_class("busy");
            // Bubble the intent up to the board (which owns screen pushing).
            ctx.post_message(AddTaskRequested);
        }
    }
}

// ===========================================================================
// AddTaskScreen — a modal screen that dismisses with a typed NewTask result
// ===========================================================================

struct AddTaskRoot;

impl Widget for AddTaskRoot {
    fn style_type(&self) -> &'static str {
        "AddTaskScreen"
    }

    fn compose(&mut self) -> ComposeResult {
        // NOTE (friction): child ids only resolve when the id-bearing widget is a
        // child of a *container* (harvested at mount), not a direct child of the
        // screen root. So the fields live inside a `VerticalGroup` wrapper.
        vec![ChildDecl::from(
            VerticalGroup::new()
                .with_child(Label::new("Add a new task").with_id("add-title"))
                .with_child(Label::new("Title:"))
                .with_child(Input::new().with_placeholder("Describe the task").id("task-title"))
                .with_child(Label::new("Priority:"))
                .with_child(Button::new("Low").id("p-low"))
                .with_child(Button::new("Med").id("p-med"))
                .with_child(Button::error("High").id("p-high"))
                .with_child(Label::new("Enter = add (medium) · Esc = cancel")),
        )]
    }

    fn render(&self, _console: &rich_rs::Console, _options: &rich_rs::ConsoleOptions) -> rich_rs::Segments {
        rich_rs::Segments::new()
    }
}

struct AddTaskScreen {
    // Latest title, tracked from InputChanged (the modal cannot query its own
    // Input via ScreenMessageCtx — see friction report).
    title: String,
}

impl AddTaskScreen {
    fn new() -> Self {
        Self { title: String::new() }
    }
}

impl Screen for AddTaskScreen {
    fn name(&self) -> &str {
        "AddTaskScreen"
    }

    fn compose(&self) -> Box<dyn Widget> {
        Box::new(AddTaskRoot)
    }

    fn css(&self) -> Option<&str> {
        Some(concat!(env!("CARGO_MANIFEST_DIR"), "/src/add_task.tcss"))
    }

    fn auto_focus(&self) -> Option<&str> {
        Some("#task-title")
    }

    fn on_event(&mut self, event: &Event, ctx: &mut ScreenMessageCtx) {
        if let Event::Key(key) = event
            && key.name() == "escape"
        {
            ctx.dismiss_none();
        }
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut ScreenMessageCtx) {
        if let Some(changed) = message.downcast_ref::<InputChanged>() {
            self.title = changed.value.clone();
        } else if let Some(submitted) = message.downcast_ref::<InputSubmitted>() {
            let title = if submitted.value.is_empty() {
                self.title.clone()
            } else {
                submitted.value.clone()
            };
            if !title.is_empty() {
                ctx.dismiss(NewTask { title, priority: Priority::Medium });
            }
        }
    }

    fn on_button_pressed(
        &mut self,
        pressed: &ButtonPressed,
        _control: NodeId,
        ctx: &mut ScreenMessageCtx,
    ) {
        let priority = match pressed.button_id.as_deref() {
            Some("p-low") => Priority::Low,
            Some("p-high") => Priority::High,
            _ => Priority::Medium,
        };
        if !self.title.is_empty() {
            ctx.dismiss(NewTask { title: self.title.clone(), priority });
        } else {
            ctx.dismiss_none();
        }
    }
}

// ===========================================================================
// Board — the App: owns the model, keyboard, reactive recompose + watch
// ===========================================================================

#[derive(textual::Reactive)]
struct Board {
    columns: Vec<ColumnModel>,
    active_col: usize,
    active_card: usize,
    next_id: u64,
    pending: Arc<Mutex<Vec<NewTask>>>,
    /// Bumped to force a full app recompose (rebuild columns from the model).
    #[reactive(recompose)]
    rev: u64,
    /// Completed-task counter; the watcher updates the top-bar "Done" label.
    #[reactive(watch_with_app, init = false)]
    completed: u32,
}

impl Board {
    fn new() -> Self {
        let mut todo = ColumnModel::new("To-Do");
        todo.cards = vec![
            CardData { id: 1, title: "Write friction report".into(), priority: Priority::High },
            CardData { id: 2, title: "Buy oat milk".into(), priority: Priority::Low },
        ];
        let mut doing = ColumnModel::new("In-Progress");
        doing.cards =
            vec![CardData { id: 3, title: "Port Kanban board".into(), priority: Priority::Medium }];
        let done = ColumnModel::new("Done");
        Self {
            columns: vec![todo, doing, done],
            active_col: 0,
            active_card: 0,
            next_id: 4,
            pending: Arc::new(Mutex::new(Vec::new())),
            rev: 0,
            completed: 0,
        }
    }

    fn clamp_cursor(&mut self) {
        if self.active_col >= self.columns.len() {
            self.active_col = self.columns.len().saturating_sub(1);
        }
        let n = self.columns[self.active_col].cards.len();
        if self.active_card >= n {
            self.active_card = n.saturating_sub(1);
        }
    }

    fn touch(&mut self, app: &mut App) {
        self.clamp_cursor();
        let next = self.rev.wrapping_add(1);
        self.set_rev(next, app.reactive_ctx());
    }

    fn move_active_card(&mut self, app: &mut App, dir: i32) {
        let from = self.active_col;
        if self.columns[from].cards.is_empty() {
            return;
        }
        let to = (from as i32 + dir).clamp(0, self.columns.len() as i32 - 1) as usize;
        if to == from {
            return;
        }
        let card = self.columns[from].cards.remove(self.active_card);
        self.columns[to].cards.push(card);
        self.active_col = to;
        self.active_card = self.columns[to].cards.len() - 1;
        self.touch(app);
    }

    fn complete_active_card(&mut self, app: &mut App) {
        let from = self.active_col;
        if from == DONE_COL || self.columns[from].cards.is_empty() {
            return;
        }
        let card = self.columns[from].cards.remove(self.active_card);
        let title = card.title.clone();
        self.columns[DONE_COL].cards.push(card);
        let next = self.completed + 1;
        self.set_completed(next, app.reactive_ctx());
        self.touch(app);
        app.notify(format!("Completed: {title}"), "Nice", ToastSeverity::Information, None);
    }

    fn delete_active_card(&mut self, app: &mut App) {
        let col = self.active_col;
        if self.columns[col].cards.is_empty() {
            return;
        }
        self.columns[col].cards.remove(self.active_card);
        self.touch(app);
    }

    fn cycle_active_priority(&mut self, app: &mut App) {
        let col = self.active_col;
        let idx = self.active_card;
        if let Some(card) = self.columns[col].cards.get_mut(idx) {
            card.priority = card.priority.cycle();
            self.touch(app);
        }
    }

    fn select_card_by_id(&mut self, app: &mut App, id: u64) {
        for (ci, col) in self.columns.iter().enumerate() {
            if let Some(idx) = col.cards.iter().position(|c| c.id == id) {
                self.active_col = ci;
                self.active_card = idx;
                self.touch(app);
                return;
            }
        }
    }

    fn drain_pending(&mut self, app: &mut App) {
        let drained: Vec<NewTask> = {
            let mut guard = self.pending.lock().unwrap();
            std::mem::take(&mut *guard)
        };
        if drained.is_empty() {
            return;
        }
        for task in drained {
            let id = self.next_id;
            self.next_id += 1;
            self.columns[0]
                .cards
                .push(CardData { id, title: task.title.clone(), priority: task.priority });
            app.notify(format!("Added: {}", task.title), "Task", ToastSeverity::Information, None);
        }
        self.touch(app);
    }

    /// Reactive watch (`#[reactive(watch_with_app)] completed`): repaint the
    /// top-bar "Done" label whenever the completed counter changes.
    fn watch_completed(&mut self, app: &mut App, _old: &u32, new: &u32, _ctx: &mut ReactiveCtx) {
        let text = format!("Done: {new}");
        let _ = app.with_query_one_mut_as::<Label, _>("#done-label", |l| l.set_text(text));
    }

    fn open_add_dialog(&mut self, app: &mut App) {
        let sink = self.pending.clone();
        app.push_screen_with_callback(
            Box::new(AddTaskScreen::new()),
            Box::new(move |result| {
                if let ScreenResult::Value(value) = result
                    && let Ok(task) = value.downcast::<NewTask>()
                {
                    sink.lock().unwrap().push(*task);
                }
            }),
        );
    }
}

const CSS: &str = r#"
Screen { layout: vertical; background: $surface; }

#topbar {
    height: 1;
    background: $primary;
    color: $text;
    layout: horizontal;
}
#topbar Label { width: auto; margin: 0 2 0 0; }
#board-title { text-style: bold; }
#done-label { width: 1fr; text-align: right; }

#board { layout: horizontal; height: 1fr; }

Column {
    width: 1fr;
    height: 100%;
    border: round $panel;
    margin: 0 1;
    padding: 0 1;
}
Column.active { border: round $accent; }
.col-header { text-style: bold; color: $accent; width: 100%; text-align: center; }

TaskCard {
    height: auto;
    width: 100%;
    border: round $panel-darken-1;
    background: $boost;
    margin: 0 0 1 0;
    padding: 0 1;
}
TaskCard.selected { border: round $success; background: $success-muted; }
.prio-tag { text-align: right; color: $text-muted; }
TaskCard.prio-high .prio-tag { color: $error; text-style: bold; }
TaskCard.prio-med .prio-tag { color: $warning; }
TaskCard.prio-low .prio-tag { color: $success; }

ControlBar { height: 1; layout: horizontal; }
ControlBar.busy #ctrl-hint { color: $warning; }
#ctrl-hint { margin: 0 0 0 2; width: auto; }

AutoSave { width: auto; color: $text-muted; }
"#;

impl TextualApp for Board {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("a", "add", "Add task"),
            BindingDecl::new("d", "delete", "Delete"),
            BindingDecl::new("c", "complete", "Complete"),
            BindingDecl::new("p", "priority", "Cycle priority"),
        ]
    }

    fn reactive_widget_mut(&mut self) -> Option<&mut dyn ReactiveWidget> {
        Some(self)
    }

    fn compose(&mut self) -> AppRoot {
        let done = self.completed;
        let topbar = HorizontalGroup::new().with_compose(vec![
            ChildDecl::new(Box::new(Label::new("KANBAN"))).with_id("board-title"),
            ChildDecl::from(
                Link::new("[?]").with_tooltip(
                    "h/l select column · j/k select card · Shift+H/L move card · \
                     a add · d delete · c complete · p priority",
                ),
            ),
            ChildDecl::from(AutoSaveIndicator::new()),
            ChildDecl::new(Box::new(Label::new(format!("Done: {done}")))).with_id("done-label"),
        ]);

        let mut board_children: Vec<ChildDecl> = Vec::new();
        for (ci, model) in self.columns.iter().enumerate() {
            let active = ci == self.active_col;
            let col = Column::new(model, active, self.active_card);
            let mut classes: Vec<&str> = Vec::new();
            if active {
                classes.push("active");
            }
            board_children.push(ChildDecl::new(Box::new(col)).with_classes(&classes));
        }
        let board = HorizontalGroup::new().with_compose(board_children);

        AppRoot::new().with_compose(vec![
            ChildDecl::new(Box::new(topbar)).with_id("topbar"),
            ChildDecl::new(Box::new(board)).with_id("board"),
            ChildDecl::from(ControlBar::new()),
            ChildDecl::from(Footer::new()),
        ])
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut WidgetCtx) {
        // Drain any task queued by the modal callback first.
        self.drain_pending(app);
        let handled = match key.name() {
            "left" | "h" => {
                self.active_col = self.active_col.saturating_sub(1);
                self.touch(app);
                true
            }
            "right" | "l" => {
                self.active_col = (self.active_col + 1).min(self.columns.len() - 1);
                self.touch(app);
                true
            }
            "up" | "k" => {
                self.active_card = self.active_card.saturating_sub(1);
                self.touch(app);
                true
            }
            "down" | "j" => {
                self.active_card += 1;
                self.touch(app);
                true
            }
            "H" => {
                self.move_active_card(app, -1);
                true
            }
            "L" => {
                self.move_active_card(app, 1);
                true
            }
            _ => false,
        };
        if handled {
            ctx.set_handled();
        }
    }

    fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut WidgetCtx) {
        self.drain_pending(app);
        match action {
            "add" => self.open_add_dialog(app),
            "delete" => self.delete_active_card(app),
            "complete" => self.complete_active_card(app),
            "priority" => self.cycle_active_priority(app),
            _ => return,
        }
        ctx.set_handled();
    }

    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut WidgetCtx) {
        if let Some(clicked) = message.downcast_ref::<CardClicked>() {
            self.select_card_by_id(app, clicked.id);
            ctx.set_handled();
            return;
        }
        if message.downcast_ref::<AddTaskRequested>().is_some() {
            self.open_add_dialog(app);
            ctx.set_handled();
        }
    }

    fn on_tick_with_app(&mut self, app: &mut App, _tick: u64, _ctx: &mut WidgetCtx) {
        self.drain_pending(app);
    }
}

fn main() -> textual::Result<()> {
    run_sync(Board::new())
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn card_titles(pilot: &mut Pilot, col: usize) -> Vec<String> {
        pilot
            .app_mut()
            .with_app_struct::<Board, _>(
                |b, _app, _ctx| b.columns[col].cards.iter().map(|c| c.title.clone()).collect(),
                &mut textual::event::EventCtx::default(),
            )
            .unwrap_or_default()
    }

    fn cursor(pilot: &mut Pilot) -> (usize, usize) {
        pilot
            .app_mut()
            .with_app_struct::<Board, _>(
                |b, _app, _ctx| (b.active_col, b.active_card),
                &mut textual::event::EventCtx::default(),
            )
            .unwrap_or((0, 0))
    }

    fn completed(pilot: &mut Pilot) -> u32 {
        pilot
            .app_mut()
            .with_app_struct::<Board, _>(|b, _app, _ctx| b.completed, &mut textual::event::EventCtx::default())
            .unwrap_or(0)
    }

    #[test]
    fn board_composes_without_panic() {
        let mut app = Board::new();
        let _root = app.compose();
    }

    #[test]
    fn priority_cycles() {
        assert_eq!(Priority::Low.cycle(), Priority::Medium);
        assert_eq!(Priority::Medium.cycle(), Priority::High);
        assert_eq!(Priority::High.cycle(), Priority::Low);
    }

    /// Cursor navigation moves between columns and cards.
    #[test]
    fn cursor_navigation_moves_selection() {
        run_test(Board::new(), |pilot| {
            assert_eq!(cursor(pilot), (0, 0));
            pilot.press(&["right"])?;
            assert_eq!(cursor(pilot).0, 1, "right must move to the next column");
            pilot.press(&["left"])?;
            assert_eq!(cursor(pilot).0, 0, "left must move back");
            pilot.press(&["down"])?;
            assert_eq!(cursor(pilot).1, 1, "down must move to the next card");
            Ok(())
        })
        .unwrap();
    }

    /// Moving a card re-parents it: it leaves To-Do and lands in In-Progress
    /// (dynamic add/remove → recompose).
    #[test]
    fn moving_a_card_reparents_it() {
        run_test(Board::new(), |pilot| {
            let todo_before = card_titles(pilot, 0).len();
            let doing_before = card_titles(pilot, 1).len();
            // Active card is To-Do[0]; move it right into In-Progress (Shift+L).
            pilot.press(&["L"])?;
            let todo_after = card_titles(pilot, 0).len();
            let doing_after = card_titles(pilot, 1).len();
            assert_eq!(todo_after, todo_before - 1, "card must leave To-Do");
            assert_eq!(doing_after, doing_before + 1, "card must arrive in In-Progress");
            Ok(())
        })
        .unwrap();
    }

    /// Completing a card moves it to Done and bumps the completed counter (the
    /// reactive watch that repaints the top-bar label).
    #[test]
    fn completing_a_card_bumps_the_counter() {
        run_test(Board::new(), |pilot| {
            assert_eq!(completed(pilot), 0);
            let done_before = card_titles(pilot, DONE_COL).len();
            pilot.press(&["c"])?; // action: complete
            assert_eq!(completed(pilot), 1, "completing must bump the counter");
            let done_after = card_titles(pilot, DONE_COL).len();
            assert_eq!(done_after, done_before + 1, "completed card must land in Done");
            Ok(())
        })
        .unwrap();
    }

    /// Deleting removes the active card from its column.
    #[test]
    fn deleting_removes_the_active_card() {
        run_test(Board::new(), |pilot| {
            let before = card_titles(pilot, 0).len();
            pilot.press(&["d"])?;
            let after = card_titles(pilot, 0).len();
            assert_eq!(after, before - 1, "delete must remove one card from To-Do");
            Ok(())
        })
        .unwrap();
    }

    /// The add-task screen owns its dismiss decision: a submitted title yields a
    /// typed `NewTask`, and a priority button yields that priority. Unit-tested
    /// against `ScreenMessageCtx::for_test` (mirrors the docs modal examples).
    #[test]
    fn add_task_screen_dismisses_with_a_typed_result() {
        use std::sync::Mutex;
        // InputSubmitted → NewTask(medium).
        let mut screen = AddTaskScreen::new();
        let slot: Mutex<Option<ScreenResult>> = Mutex::new(None);
        let mut ectx = textual::event::EventCtx::default();
        let mut sctx = ScreenMessageCtx::for_test(&mut ectx, &slot);
        screen.on_message(
            &MessageEvent::new(NodeId::default(), InputSubmitted { value: "Buy milk".into() }),
            &mut sctx,
        );
        match slot.lock().unwrap().take() {
            Some(ScreenResult::Value(v)) => {
                let task = v.downcast::<NewTask>().expect("dismiss value must be a NewTask");
                assert_eq!(task.title, "Buy milk");
                assert_eq!(task.priority, Priority::Medium);
            }
            other => panic!("expected NewTask value, got {:?}", other.is_none()),
        }

        // A priority button dismisses with the picked priority.
        let mut screen = AddTaskScreen::new();
        screen.title = "Ship it".into();
        let slot: Mutex<Option<ScreenResult>> = Mutex::new(None);
        let mut ectx = textual::event::EventCtx::default();
        let mut sctx = ScreenMessageCtx::for_test(&mut ectx, &slot);
        screen.on_button_pressed(
            &ButtonPressed { description: "High".into(), button_id: Some("p-high".into()) },
            NodeId::default(),
            &mut sctx,
        );
        match slot.lock().unwrap().take() {
            Some(ScreenResult::Value(v)) => {
                let task = v.downcast::<NewTask>().unwrap();
                assert_eq!(task.priority, Priority::High);
            }
            _ => panic!("expected a High NewTask"),
        }
    }

    /// End-to-end through the real runtime: pressing `a` pushes the modal;
    /// dismissing it with a `NewTask` result fires the board's push callback,
    /// which stashes the task; the next key drains it and appends the card.
    #[test]
    fn modal_result_appends_a_card() {
        run_test(Board::new(), |pilot| {
            let before = card_titles(pilot, 0).len();
            pilot.press(&["a"])?;
            assert_eq!(pilot.app().screen_count(), 1, "`a` must push the add-task modal");
            // Return a typed result (the screen's own dismiss path is unit-tested
            // above; modal internals are not query/click-reachable headlessly).
            pilot.app_mut().dismiss_screen(ScreenResult::Value(Box::new(NewTask {
                title: "Widget".into(),
                priority: Priority::High,
            })));
            assert_eq!(pilot.app().screen_count(), 0, "dismissing must pop the modal");
            // A key press drains the pending queue into the model.
            pilot.press(&["k"])?;
            let after = card_titles(pilot, 0);
            assert_eq!(after.len(), before + 1, "the new task must be appended to To-Do");
            assert!(
                after.iter().any(|t| t == "Widget"),
                "the appended card must carry the result title, got {after:?}"
            );
            Ok(())
        })
        .unwrap();
    }

    /// Clicking the ControlBar's "Add task" button runs its `#[on(ButtonPressed)]`
    /// handler, which self-mutates (`query_one_id` + `update_via` + `add_class`)
    /// and bubbles `AddTaskRequested` up to the board — which opens the modal.
    #[test]
    fn controlbar_button_opens_the_modal() {
        run_test(Board::new(), |pilot| {
            assert_eq!(pilot.app().screen_count(), 0);
            pilot.click("#add-btn")?;
            assert_eq!(
                pilot.app().screen_count(),
                1,
                "clicking Add task must bubble AddTaskRequested and open the modal"
            );
            Ok(())
        })
        .unwrap();
    }

    /// The AutoSaveIndicator owns a 2s interval; advancing the clock fires it,
    /// bumping its reactive `ticks`, whose watcher rewrites the indicator text —
    /// proving the widget-owned timer + widget-level reactive watch are live.
    #[test]
    fn autosave_timer_updates_the_indicator() {
        run_test(Board::new(), |pilot| {
            assert!(pilot.clock_is_manual());
            let before = pilot.app().frame_fingerprint();
            pilot.advance_clock(Duration::from_secs(2))?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(before, after, "the auto-save interval tick must repaint the indicator");
            Ok(())
        })
        .unwrap();
    }

    /// Clicking a card posts `CardClicked`, which bubbles to the board and makes
    /// that card active (post_up round-trip).
    #[test]
    fn clicking_a_card_selects_it() {
        run_test(Board::new(), |pilot| {
            // Move the cursor off To-Do so a click on the first card is observable.
            pilot.press(&["right"])?; // active col 1, card 0
            let start = cursor(pilot);
            assert_eq!(start, (1, 0));
            // `click("TaskCard")` hits the first (topmost) TaskCard = To-Do[0].
            // Its click posts `CardClicked`, which bubbles to the board and makes
            // that card active — moving the cursor back to column 0.
            pilot.click("TaskCard")?;
            let after = cursor(pilot);
            assert_eq!(after.0, 0, "clicking To-Do's card must select column 0 (got {after:?})");
            Ok(())
        })
        .unwrap();
    }
}
