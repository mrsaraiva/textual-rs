/// Port of Python Textual `examples/five_by_five.py`.
///
/// A 5×5 toggle-puzzle game demonstrating:
/// - Signals-first state: `FiveByFiveApp` derives `Reactive`; all DOM mutation
///   happens inside `watch_*` watchers, not inline in key handlers.
/// - Screen stack (`push_screen`/`pop_screen`) for a help overlay.
/// - Key bindings for grid navigation (arrows, WASD, hjkl) and game actions.
/// - Footer with binding hints.
/// - Win detection with a move-count message.
///
/// Python: `Game(Screen)` composes `GameHeader`, `GameGrid` (5×5 grid of
/// `GameCell(Button)`), `Footer`, and `WinnerMessage(Label)` overlay.
/// Reactive properties: `GameHeader.moves`, `GameHeader.filled`
/// (five_by_five.py:78-81); cell fill state lives in DOM classes toggled
/// via queries (five_by_five.py:218-242).
use rich_rs::{Console, ConsoleOptions, Renderable, Segments};
use textual::prelude::*;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const SIZE: usize = 5;
const MIN_MOVES: usize = 14;
const APP_TITLE: &str = "5x5 -- A little annoying puzzle"; // Python TITLE, five_by_five.py:314
fn moves_text(moves: usize) -> String {
    format!("Moves: {moves}") // Python watch_moves, five_by_five.py:101
}
fn progress_text(filled: usize) -> String {
    format!("Filled: {filled}") // Python watch_filled, five_by_five.py:109
}

const HELP_TEXT: &str = r#"# 5x5

## Introduction

An annoying puzzle for the terminal, built with textual-rs.

## Objective

The object of the game is to fill all of the squares. When you press space
on a square, it, and the squares above, below and to the sides will be toggled.

It is possible to solve the puzzle in as few as 14 moves.

## Controls

- **Arrow keys / WASD / hjkl** — navigate the grid
- **Space** — toggle the current cell (makes a move)
- **n** — start a new game
- **?** — show/hide this help screen
- **q** — quit

Good luck!
"#;

const CSS: &str = r#"
GameHeader {
    background: $primary-background;
    color: $text;
    height: 1;
    dock: top;
    width: 100%;
}

GameHeader #app-title {
    width: 60%;
}

GameHeader #moves {
    width: 20%;
}

GameHeader #progress {
    width: 20%;
}

#game-grid {
    grid-size: 5 5;
}

GameCell {
    width: 100%;
    height: 100%;
    background: $surface;
    border: round $surface-darken-1;
    content-align: center middle;
}

GameCell:hover {
    background: $panel-lighten-1;
    border: round $panel;
}

GameCell.filled {
    background: $secondary;
    border: round $secondary-darken-1;
}

GameCell.filled:hover {
    background: $secondary-lighten-1;
    border: round $secondary;
}

GameCell.cursor {
    border: round $primary;
}

WinnerMessage {
    width: 100%;
    height: auto;
    content-align: center middle;
    background: $success;
    color: $text;
    padding: 1;
    text-align: center;
    border: round;
}

HelpRoot {
    border: round $primary-lighten-3;
}
"#;

// ---------------------------------------------------------------------------
// Pure game-logic helpers (replace GameState methods — unit-testable).
// ---------------------------------------------------------------------------

type Cells = [[bool; SIZE]; SIZE];

/// Toggle the cross pattern (cell + 4 neighbors) centered at (row, col).
/// Mirrors Python `Game._toggle_cell` / `action_new_game` toggling logic.
fn toggle_cross(cells: &mut Cells, row: usize, col: usize) {
    cells[row][col] = !cells[row][col];
    if row > 0 {
        cells[row - 1][col] = !cells[row - 1][col];
    }
    if row + 1 < SIZE {
        cells[row + 1][col] = !cells[row + 1][col];
    }
    if col > 0 {
        cells[row][col - 1] = !cells[row][col - 1];
    }
    if col + 1 < SIZE {
        cells[row][col + 1] = !cells[row][col + 1];
    }
}

/// Count filled cells.
fn filled_count(cells: &Cells) -> usize {
    cells.iter().flatten().filter(|&&c| c).count()
}

/// Navigate cursor by (dr, dc) with wrapping.
fn wrap_navigate(cur: (usize, usize), dr: i32, dc: i32) -> (usize, usize) {
    let nr = ((cur.0 as i32 + dr).rem_euclid(SIZE as i32)) as usize;
    let nc = ((cur.1 as i32 + dc).rem_euclid(SIZE as i32)) as usize;
    (nr, nc)
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

// ---------------------------------------------------------------------------
// GameCell — individual playable cell (containment pattern, SPEC-RA5).
//
// Python: `GameCell(Button)` — subclasses Button, inheriting focus, press
// behavior, and Button CSS rules. Rust analog: GameCell CONTAINS a Button
// child, uses `style_type_aliases = &["Button"]` so that Button CSS rules
// match, and intercepts ButtonPressed via on_message.
//
// Cell fill/cursor state lives on the arena node (set by watchers via
// app.query_mut); no fill/cursor fields on the struct.
// ---------------------------------------------------------------------------

pub struct GameCell {
    #[allow(dead_code)]
    row: usize,
    #[allow(dead_code)]
    col: usize,
    /// Inner Button child: provides focus + press behavior.
    /// No CSS id is set on it — GameCell is the CSS-identity node.
    inner: Button,
    /// Guard so take_composed_children is idempotent.
    child_extracted: bool,
    seed: NodeSeed,
}

impl GameCell {
    pub fn new(row: usize, col: usize) -> Self {
        let id = Self::id_for(row, col);
        let mut seed = NodeSeed::default();
        seed.css_id = Some(id);
        Self {
            row,
            col,
            // compact(true) suppresses the tall-border chrome (▔/▁) from Button's
            // default CSS so the Button child is visually transparent inside the
            // GameCell frame. No CSS id — GameCell is the CSS-identity node.
            inner: Button::new("").compact(true),
            child_extracted: false,
            seed,
        }
    }

    pub fn id_for(row: usize, col: usize) -> String {
        format!("cell-{}-{}", row, col)
    }
}

impl Widget for GameCell {
    fn style_type(&self) -> &'static str {
        "GameCell"
    }

    // GameCell CSS rules apply (via style_type = "GameCell") AND
    // Button CSS rules apply (via alias "Button"), mirroring Python's MRO
    // where GameCell(Button) matches both `GameCell { }` and `Button { }`.
    fn style_type_aliases(&self) -> &[&'static str] {
        &["Button"]
    }

    // Outer wrapper has no content to render; the Button child renders via arena.
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    // Consume the NodeSeed at mount (RA-2 pattern).
    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }

    // Move the Button child into the arena tree.
    fn compose(&mut self) -> textual::compose::ComposeResult {
        if self.child_extracted {
            return vec![];
        }
        self.child_extracted = true;
        // Replace inner with a compact sentinel so the field stays valid.
        vec![textual::compose::ChildDecl::new(Box::new(std::mem::replace(
            &mut self.inner,
            Button::new("").compact(true),
        )))]
    }

    // Outer wrapper is not itself focusable.
    fn focusable(&self) -> bool {
        false
    }

    // The Button child is a behavior-only child; it must not appear in the focus
    // chain. Focus traversal is suppressed so the child's bindings (enter/space
    // "Press button") do not bleed into the footer — all keyboard logic is handled
    // at the app level via on_key_with_app. Mouse-click events still reach the
    // Button via arena hit-testing independently of focus.
    fn can_focus_children(&self) -> bool {
        false
    }

    // Intercept ButtonPressed from the Button child and stop bubbling.
    // The FiveByFiveApp handles game logic via on_key_with_app (space key calls
    // toggle_cross directly). Mouse-click presses are absorbed here; a future
    // RA-1 demo enhancement could re-emit a custom GameCellPressed { row, col }
    // message instead (Alternative G in SPEC-RA5).
    fn on_message(&mut self, msg: &MessageEvent, ctx: &mut EventCtx) {
        if msg.downcast_ref::<ButtonPressed>().is_some() {
            ctx.set_handled();
        }
    }

    // Forward hover/active state from inner Button for off-tree CSS resolution.
    // GameCell is the CSS-identity node, so :hover and :active rules are
    // resolved against its SelectorMeta; the Button child tracks the actual
    // hover/active state. After RA-2 (node.state owns this), forwarding can go.
    fn is_hovered(&self) -> bool {
        self.inner.is_hovered()
    }

    fn is_active(&self) -> bool {
        self.inner.is_active()
    }

    fn mouse_interactive(&self) -> bool {
        true
    }

    fn on_event_capture(&mut self, _event: &Event, _ctx: &mut EventCtx) {}
    fn on_event(&mut self, _event: &Event, _ctx: &mut EventCtx) {}
}

impl Renderable for GameCell {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

// ---------------------------------------------------------------------------
// GameHeader — title + moves + filled counts
// (Python: reactive labels in Horizontal, five_by_five.py:84-93)
// After mount, moves/filled are updated by the watchers via direct
// label queries (#moves, #progress) since composed children are in the arena.
// ---------------------------------------------------------------------------

pub struct GameHeader {
    children_extracted: bool,
    seed: NodeSeed,
}

impl GameHeader {
    pub fn new() -> Self {
        Self {
            children_extracted: false,
            seed: NodeSeed::default(),
        }
    }
}

impl Widget for GameHeader {
    fn style_type(&self) -> &'static str {
        "GameHeader"
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        // Chrome/background only; labels are composed children.
        Widget::render(&Label::new(""), console, options)
    }

    fn compose(&mut self) -> textual::compose::ComposeResult {
        if self.children_extracted {
            return Vec::new();
        }
        self.children_extracted = true;
        vec![textual::compose::ChildDecl::new(Box::new(
            Horizontal::new()
                .with_child(Label::new(APP_TITLE).with_id("app-title"))
                .with_child(Label::new(moves_text(0)).with_id("moves"))
                .with_child(Label::new(progress_text(0)).with_id("progress")),
        ))]
    }

    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }

    fn on_event_capture(&mut self, _event: &Event, _ctx: &mut EventCtx) {}
    fn on_event(&mut self, _event: &Event, _ctx: &mut EventCtx) {}
    fn on_message(&mut self, _msg: &MessageEvent, _ctx: &mut EventCtx) {}
}

impl Renderable for GameHeader {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

// ---------------------------------------------------------------------------
// WinnerMessage — displayed when the game is won
// (Python: Label on a separate layer with visibility toggling)
// ---------------------------------------------------------------------------

pub struct WinnerMessage {
    text: String,
    visible: bool,
    seed: NodeSeed,
}

impl WinnerMessage {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            visible: false,
            seed: NodeSeed::default(),
        }
    }

    pub fn show(&mut self, moves: usize) {
        let over_msg = if moves > MIN_MOVES {
            format!(
                " It is possible to solve the puzzle in {}, you were {} move{} over.",
                MIN_MOVES,
                moves - MIN_MOVES,
                plural(moves - MIN_MOVES),
            )
        } else {
            " Well done! That's the minimum number of moves to solve the puzzle!".to_string()
        };
        self.text = format!(
            "W I N N E R !\n\nYou solved the puzzle in {} move{}.{}",
            moves,
            plural(moves),
            over_msg,
        );
        self.visible = true;
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }
}

impl Widget for WinnerMessage {
    fn style_type(&self) -> &'static str {
        "WinnerMessage"
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        if self.visible {
            Widget::render(&Label::new(&self.text), console, options)
        } else {
            Widget::render(&Label::new(""), console, options)
        }
    }

    fn layout_height(&self) -> Option<usize> {
        if self.visible {
            None // auto-size to content
        } else {
            Some(0) // collapse when hidden
        }
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }

    fn on_event_capture(&mut self, _event: &Event, _ctx: &mut EventCtx) {}
    fn on_event(&mut self, _event: &Event, _ctx: &mut EventCtx) {}
    fn on_message(&mut self, _msg: &MessageEvent, _ctx: &mut EventCtx) {}
}

impl Renderable for WinnerMessage {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

// ---------------------------------------------------------------------------
// HelpRoot widget — root widget for the Help screen.
// ---------------------------------------------------------------------------

struct HelpRoot;

impl Widget for HelpRoot {
    fn style_type(&self) -> &'static str {
        "HelpRoot"
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("escape", "app.pop_screen", "Close"),
            BindingDecl::new("space", "app.pop_screen", "Close"),
            BindingDecl::new("q", "app.pop_screen", "Close"),
        ]
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(&ScrollView::new(Markdown::new(HELP_TEXT)), console, options)
    }

    fn on_event_capture(&mut self, _event: &Event, _ctx: &mut EventCtx) {}
    fn on_event(&mut self, _event: &Event, _ctx: &mut EventCtx) {}
    fn on_message(&mut self, _msg: &MessageEvent, _ctx: &mut EventCtx) {}
}

impl Renderable for HelpRoot {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

// ---------------------------------------------------------------------------
// HelpScreen — the help overlay.
// ---------------------------------------------------------------------------

struct HelpScreen;

impl Screen for HelpScreen {
    fn name(&self) -> &str {
        "Help"
    }

    fn compose(&self) -> Box<dyn Widget> {
        Box::new(HelpRoot)
    }

    fn is_modal(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// Main app — signals-first state model
// ---------------------------------------------------------------------------

/// Cell fill state is the canonical source of truth; watchers mirror it into
/// arena node classes (CSS matching unions node classes via
/// `node_selector_meta_from_node`, src/css/selectors/resolver.rs:172-220).
#[derive(Reactive)]
struct FiveByFiveApp {
    /// Cell fill array. Python: fill state lives in DOM classes (five_by_five.py:218-242).
    #[reactive(watch_with_app, init = false)]
    cells: Cells,
    /// Cursor position. init=true → watch_cursor fires at mount to set initial class.
    #[reactive(watch_with_app)]
    cursor: (usize, usize),
    /// Move counter. init=true → watch_moves fires at mount to initialize header.
    #[reactive(watch_with_app)]
    moves: usize,
    /// Some(moves) once won; None while playing (Python: WinnerMessage show/hide).
    #[reactive(watch_with_app, init = false)]
    won_at: Option<usize>,
    /// Handle slot for the WinnerMessage overlay (direct AppRoot child).
    winner: HandleSlot<WinnerMessage>,
}

impl FiveByFiveApp {
    fn new() -> Self {
        Self {
            cells: [[false; SIZE]; SIZE],
            cursor: (SIZE / 2, SIZE / 2),
            moves: 0,
            won_at: None,
            winner: HandleSlot::new(),
        }
    }

    /// Mirrors Python `Game.on_mount` → `action_new_game` (five_by_five.py:267-299).
    /// Sets all reactive fields; watchers apply DOM mutations before first render.
    fn new_game(&mut self, app: &mut App) {
        let mut cells: Cells = [[false; SIZE]; SIZE];
        toggle_cross(&mut cells, SIZE / 2, SIZE / 2);
        self.set_moves(0, app.reactive_ctx());
        self.set_won_at(None, app.reactive_ctx());
        self.set_cursor((SIZE / 2, SIZE / 2), app.reactive_ctx());
        self.set_cells(cells, app.reactive_ctx());
    }

    /// Diff old/new cell arrays and update arena node classes for changed cells.
    fn watch_cells(
        &mut self,
        app: &mut App,
        old: &Cells,
        new: &Cells,
        ctx: &mut ReactiveCtx,
    ) {
        for row in 0..SIZE {
            for col in 0..SIZE {
                if old[row][col] != new[row][col] {
                    let _ = app
                        .query_mut(&format!("#cell-{row}-{col}"))
                        .map(|q| q.set_class(new[row][col], &["filled"]));
                }
            }
        }
        let filled = filled_count(new);
        let _ = app.with_query_one_mut_as::<Label, _>("#progress", |l| {
            l.set_text(progress_text(filled));
        });
        ctx.request_styles();
        ctx.request_repaint();
    }

    /// Move cursor class from old node to new node. init fires with old == new →
    /// adds initial cursor class on the starting cell.
    fn watch_cursor(
        &mut self,
        app: &mut App,
        old: &(usize, usize),
        new: &(usize, usize),
        ctx: &mut ReactiveCtx,
    ) {
        if old != new {
            let _ = app
                .query_mut(&format!("#cell-{}-{}", old.0, old.1))
                .map(|q| q.remove_class("cursor"));
        }
        let _ = app
            .query_mut(&format!("#cell-{}-{}", new.0, new.1))
            .map(|q| q.add_class("cursor"));
        ctx.request_styles();
        ctx.request_repaint();
    }

    /// Update the moves label. init fires at mount → initializes header to 0.
    fn watch_moves(
        &mut self,
        app: &mut App,
        _old: &usize,
        new: &usize,
        ctx: &mut ReactiveCtx,
    ) {
        let moves = *new;
        let _ = app.with_query_one_mut_as::<Label, _>("#moves", |l| {
            l.set_text(moves_text(moves));
        });
        ctx.request_repaint();
    }

    /// Show/hide the winner overlay. init = false → does not fire at mount.
    fn watch_won_at(
        &mut self,
        app: &mut App,
        _old: &Option<usize>,
        new: &Option<usize>,
        ctx: &mut ReactiveCtx,
    ) {
        let new = *new;
        if let Ok(h) = self.winner.handle() {
            let _ = h.update(app, |w, _ctx| match new {
                Some(moves) => w.show(moves),
                None => w.hide(),
            });
        }
        ctx.request_layout();
        ctx.request_repaint();
    }
}

impl TextualApp for FiveByFiveApp {
    fn reactive_widget_mut(&mut self) -> Option<&mut dyn ReactiveWidget> {
        Some(self)
    }

    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        app.add_mode("help", || Box::new(HelpScreen));
        Ok(())
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("n", "new_game", "New Game"),
            BindingDecl::new("question_mark", "app.push_screen('help')", "Help"),
            BindingDecl::new("q", "app.quit", "Quit"),
            BindingDecl::new("ctrl+d", "app.toggle_dark", "Toggle Dark Mode"),
        ]
    }

    fn compose(&mut self) -> AppRoot {
        let mut grid = Grid::new(SIZE, SIZE).id("game-grid");
        for row in 0..SIZE {
            for col in 0..SIZE {
                grid = grid.with_child(GameCell::new(row, col));
            }
        }

        AppRoot::new()
            .with_child(GameHeader::new())
            .with_child(grid)
            .with_child_handle(WinnerMessage::new(), &self.winner)
            .with_child(Footer::new())
    }

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut EventCtx) {
        self.new_game(app);
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut EventCtx) {
        let handled = match key.name() {
            // Navigation — arrow keys, WASD, hjkl
            "up" | "w" | "k" => {
                self.set_cursor(wrap_navigate(self.cursor, -1, 0), app.reactive_ctx());
                true
            }
            "down" | "s" | "j" => {
                self.set_cursor(wrap_navigate(self.cursor, 1, 0), app.reactive_ctx());
                true
            }
            "left" | "a" | "h" => {
                self.set_cursor(wrap_navigate(self.cursor, 0, -1), app.reactive_ctx());
                true
            }
            "right" | "d" | "l" => {
                self.set_cursor(wrap_navigate(self.cursor, 0, 1), app.reactive_ctx());
                true
            }
            // Make a move at the current cursor position.
            // Key identifier is "space", not a literal " " (see keys::key_to_identifier).
            "space" => {
                if self.won_at.is_none() {
                    let mut cells = self.cells;
                    toggle_cross(&mut cells, self.cursor.0, self.cursor.1);
                    let moves = self.moves + 1;
                    self.set_cells(cells, app.reactive_ctx());
                    self.set_moves(moves, app.reactive_ctx());
                    if filled_count(&self.cells) == SIZE * SIZE {
                        self.set_won_at(Some(moves), app.reactive_ctx());
                    }
                }
                true
            }
            // New game
            "n" => {
                self.new_game(app);
                true
            }
            _ => false,
        };
        if handled {
            ctx.set_handled();
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(FiveByFiveApp::new())
}

// ---------------------------------------------------------------------------
// Regression tests — rewritten against pure helpers (GameState dissolved).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_by_five_app_composes_without_panic() {
        let mut app = FiveByFiveApp::new();
        let _root = app.compose();
    }

    // --- pure-helper tests ---

    #[test]
    fn toggle_cross_fills_center() {
        let mut cells: Cells = [[false; SIZE]; SIZE];
        toggle_cross(&mut cells, SIZE / 2, SIZE / 2);
        // Middle cross: (2,2), (1,2), (3,2), (2,1), (2,3) — 5 cells.
        assert_eq!(filled_count(&cells), 5);
        assert!(cells[2][2]);
        assert!(cells[1][2]);
        assert!(cells[3][2]);
        assert!(cells[2][1]);
        assert!(cells[2][3]);
    }

    #[test]
    fn toggle_cross_fills_adjacent_at_corner() {
        let mut cells: Cells = [[false; SIZE]; SIZE];
        toggle_cross(&mut cells, 0, 0);
        // Corner: (0,0), (1,0), (0,1) — 3 cells.
        assert_eq!(filled_count(&cells), 3);
        assert!(cells[0][0]);
        assert!(cells[1][0]);
        assert!(cells[0][1]);
    }

    #[test]
    fn toggle_cross_double_toggle_is_identity() {
        let mut cells: Cells = [[false; SIZE]; SIZE];
        toggle_cross(&mut cells, 2, 2);
        toggle_cross(&mut cells, 2, 2);
        assert_eq!(filled_count(&cells), 0);
    }

    #[test]
    fn initial_cells_are_all_false() {
        let cells: Cells = [[false; SIZE]; SIZE];
        assert_eq!(filled_count(&cells), 0);
    }

    #[test]
    fn new_game_cells_have_cross_filled() {
        let mut cells: Cells = [[false; SIZE]; SIZE];
        toggle_cross(&mut cells, SIZE / 2, SIZE / 2);
        assert_eq!(filled_count(&cells), 5);
    }

    #[test]
    fn wrap_navigate_normal_move() {
        assert_eq!(wrap_navigate((2, 2), -1, 0), (1, 2));
        assert_eq!(wrap_navigate((2, 2), 1, 0), (3, 2));
        assert_eq!(wrap_navigate((2, 2), 0, -1), (2, 1));
        assert_eq!(wrap_navigate((2, 2), 0, 1), (2, 3));
    }

    #[test]
    fn wrap_navigate_wraps_at_top() {
        let result = wrap_navigate((0, 0), -1, 0);
        assert_eq!(result.0, SIZE - 1, "should wrap to last row");
        assert_eq!(result.1, 0);
    }

    #[test]
    fn wrap_navigate_wraps_at_left() {
        let result = wrap_navigate((0, 0), 0, -1);
        assert_eq!(result.0, 0);
        assert_eq!(result.1, SIZE - 1, "should wrap to last col");
    }

    #[test]
    fn wrap_navigate_wraps_at_bottom() {
        let result = wrap_navigate((SIZE - 1, 0), 1, 0);
        assert_eq!(result.0, 0, "should wrap to first row");
    }

    #[test]
    fn wrap_navigate_wraps_at_right() {
        let result = wrap_navigate((0, SIZE - 1), 0, 1);
        assert_eq!(result.1, 0, "should wrap to first col");
    }

    // --- GameCell ---

    #[test]
    fn game_cell_id_format() {
        assert_eq!(GameCell::id_for(2, 3), "cell-2-3");
        let cell = GameCell::new(2, 3);
        let id = cell.seed.css_id.as_deref();
        assert_eq!(id, Some("cell-2-3"));
    }

    #[test]
    fn game_cell_has_button_child() {
        // take_composed_children returns one child on first call, empty on second.
        let mut cell = GameCell::new(1, 2);
        let first = cell.compose();
        assert_eq!(first.len(), 1, "first call must return the Button child");
        let second = cell.compose();
        assert_eq!(second.len(), 0, "second call must be empty (idempotent)");
    }

    #[test]
    fn game_cell_style_aliases() {
        let cell = GameCell::new(0, 0);
        let aliases = cell.style_type_aliases();
        assert_eq!(aliases, &["Button"], "GameCell must alias Button for CSS type-selector matching");
    }

    #[test]
    fn game_cell_not_focusable() {
        let cell = GameCell::new(0, 0);
        assert!(!cell.focusable(), "outer GameCell wrapper must not be focusable itself");
        // Focus traversal is suppressed so the Button child's bindings do not bleed
        // into the footer. All keyboard logic is at the app level (on_key_with_app).
        assert!(!cell.can_focus_children(), "GameCell must suppress focus into Button child to avoid footer binding bleed");
    }

    // --- WinnerMessage ---

    #[test]
    fn winner_message_starts_hidden() {
        let msg = WinnerMessage::new();
        assert!(!msg.visible);
        assert_eq!(msg.layout_height(), Some(0));
    }

    #[test]
    fn winner_message_show_and_hide() {
        let mut msg = WinnerMessage::new();
        msg.show(14);
        assert!(msg.visible);
        assert!(msg.text.contains("W I N N E R"));
        assert!(msg.text.contains("minimum"));
        assert!(msg.layout_height().is_none());

        msg.show(20);
        assert!(msg.text.contains("6 moves over"));

        msg.hide();
        assert!(!msg.visible);
        assert_eq!(msg.layout_height(), Some(0));
    }

    // --- GameHeader ---

    #[test]
    fn game_header_label_texts() {
        assert_eq!(APP_TITLE, "5x5 -- A little annoying puzzle");
        assert_eq!(moves_text(0), "Moves: 0");
        assert_eq!(progress_text(5), "Filled: 5");
        // GameHeader::new() takes no args; initial label text uses 0/0.
        let _header = GameHeader::new();
    }
}
