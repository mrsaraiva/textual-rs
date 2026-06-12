/// Port of Python Textual `examples/five_by_five.py`.
///
/// A 5×5 toggle-puzzle game demonstrating:
/// - Proper widget composition: `GameCell` widgets in a CSS grid,
///   `GameHeader` for stats, `WinnerMessage` for victory display.
/// - Screen stack (`push_screen`/`pop_screen`) for a help overlay.
/// - Key bindings for grid navigation (arrows, WASD, hjkl) and game actions.
/// - Footer with binding hints.
/// - Win detection with a move-count message.
///
/// Python: `Game(Screen)` composes `GameHeader`, `GameGrid` (5×5 grid of
/// `GameCell(Button)`), `Footer`, and `WinnerMessage(Label)` overlay.
/// Reactive properties track moves/filled counts.
/// Rust: `FiveByFiveApp` composes equivalent widgets with state managed
/// by `GameState`. Widgets are queried and mutated via `with_query_one_mut_as`.
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
// GameState — pure game logic, no widget concerns.
// ---------------------------------------------------------------------------

pub struct GameState {
    pub cells: [[bool; SIZE]; SIZE],
    pub cursor: (usize, usize),
    pub moves: usize,
    pub won: bool,
}

impl GameState {
    pub fn new() -> Self {
        let mut state = Self {
            cells: [[false; SIZE]; SIZE],
            cursor: (SIZE / 2, SIZE / 2),
            moves: 0,
            won: false,
        };
        state.toggle_cross(SIZE / 2, SIZE / 2);
        state
    }

    fn toggle_at(&mut self, row: usize, col: usize) {
        self.cells[row][col] = !self.cells[row][col];
    }

    fn toggle_cross(&mut self, row: usize, col: usize) {
        self.toggle_at(row, col);
        if row > 0 {
            self.toggle_at(row - 1, col);
        }
        if row + 1 < SIZE {
            self.toggle_at(row + 1, col);
        }
        if col > 0 {
            self.toggle_at(row, col - 1);
        }
        if col + 1 < SIZE {
            self.toggle_at(row, col + 1);
        }
    }

    pub fn filled_count(&self) -> usize {
        self.cells.iter().flatten().filter(|&&c| c).count()
    }

    pub fn all_filled(&self) -> bool {
        self.filled_count() == SIZE * SIZE
    }

    /// Make a move at the cursor. Returns the list of toggled cell coordinates.
    pub fn make_move(&mut self) -> Vec<(usize, usize)> {
        if self.won {
            return vec![];
        }
        let (r, c) = self.cursor;
        let mut affected = vec![(r, c)];
        self.toggle_at(r, c);
        if r > 0 {
            self.toggle_at(r - 1, c);
            affected.push((r - 1, c));
        }
        if r + 1 < SIZE {
            self.toggle_at(r + 1, c);
            affected.push((r + 1, c));
        }
        if c > 0 {
            self.toggle_at(r, c - 1);
            affected.push((r, c - 1));
        }
        if c + 1 < SIZE {
            self.toggle_at(r, c + 1);
            affected.push((r, c + 1));
        }
        self.moves += 1;
        if self.all_filled() {
            self.won = true;
        }
        affected
    }

    /// Navigate cursor by (dr, dc) with wrapping. Returns old cursor position.
    pub fn navigate(&mut self, dr: i32, dc: i32) -> (usize, usize) {
        let old = self.cursor;
        let (r, c) = self.cursor;
        let nr = ((r as i32 + dr).rem_euclid(SIZE as i32)) as usize;
        let nc = ((c as i32 + dc).rem_euclid(SIZE as i32)) as usize;
        self.cursor = (nr, nc);
        old
    }

    /// Reset and start a new game (middle cell cross-toggled as initial state).
    pub fn new_game(&mut self) {
        self.cells = [[false; SIZE]; SIZE];
        self.moves = 0;
        self.won = false;
        self.cursor = (SIZE / 2, SIZE / 2);
        self.toggle_cross(SIZE / 2, SIZE / 2);
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

// ---------------------------------------------------------------------------
// GameCell — individual playable cell (matches Python's GameCell(Button))
// ---------------------------------------------------------------------------

pub struct GameCell {
    #[allow(dead_code)]
    row: usize,
    #[allow(dead_code)]
    col: usize,
    filled: bool,
    is_cursor: bool,
    id: String,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl GameCell {
    pub fn new(row: usize, col: usize, filled: bool, is_cursor: bool) -> Self {
        let mut cell = Self {
            row,
            col,
            filled,
            is_cursor,
            id: Self::id_for(row, col),
            classes: Vec::new(),
            styles: WidgetStyles::default(),
        };
        cell.rebuild_classes();
        cell
    }

    pub fn id_for(row: usize, col: usize) -> String {
        format!("cell-{}-{}", row, col)
    }

    pub fn set_filled(&mut self, filled: bool) {
        self.filled = filled;
        self.rebuild_classes();
    }

    pub fn set_cursor(&mut self, is_cursor: bool) {
        self.is_cursor = is_cursor;
        self.rebuild_classes();
    }

    fn rebuild_classes(&mut self) {
        self.classes.clear();
        if self.filled {
            self.classes.push("filled".to_string());
        }
        if self.is_cursor {
            self.classes.push("cursor".to_string());
        }
    }
}

impl Widget for GameCell {
    fn style_type(&self) -> &'static str {
        "GameCell"
    }

    fn style_id(&self) -> Option<&str> {
        Some(&self.id)
    }

    fn style_classes(&self) -> &[String] {
        &self.classes
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        // Visual appearance comes from CSS background and border.
        Widget::render(&Label::new(" "), console, options)
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }

    fn on_event_capture(&mut self, _event: &Event, _ctx: &mut EventCtx) {}
    fn on_event(&mut self, _event: &Event, _ctx: &mut EventCtx) {}
    fn on_message(&mut self, _msg: &MessageEvent, _ctx: &mut EventCtx) {}
}

impl Renderable for GameCell {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

// ---------------------------------------------------------------------------
// GameHeader — title + moves + filled counts
// (Python: reactive labels in Horizontal, five_by_five.py:84-93)
// ---------------------------------------------------------------------------

pub struct GameHeader {
    moves: usize,
    filled: usize,
    children_extracted: bool,
    styles: WidgetStyles,
}

impl GameHeader {
    pub fn new(moves: usize, filled: usize) -> Self {
        Self {
            moves,
            filled,
            children_extracted: false,
            styles: WidgetStyles::default(),
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

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        if self.children_extracted {
            return Vec::new();
        }
        self.children_extracted = true;
        vec![Box::new(
            Horizontal::new()
                .with_child(Label::new(APP_TITLE).with_id("app-title"))
                .with_child(Label::new(moves_text(self.moves)).with_id("moves"))
                .with_child(Label::new(progress_text(self.filled)).with_id("progress")),
        )]
    }

    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
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
    styles: WidgetStyles,
}

impl WinnerMessage {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            visible: false,
            styles: WidgetStyles::default(),
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

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
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
// Main app
// ---------------------------------------------------------------------------

struct FiveByFiveApp {
    state: GameState,
}

impl FiveByFiveApp {
    fn new() -> Self {
        Self {
            state: GameState::new(),
        }
    }

    /// Sync all cells + header + winner after a new game.
    fn sync_all(&self, app: &mut App) {
        for row in 0..SIZE {
            for col in 0..SIZE {
                let id = format!("#cell-{}-{}", row, col);
                let filled = self.state.cells[row][col];
                let is_cursor = self.state.cursor == (row, col);
                let _ = app.with_query_one_mut_as::<GameCell, _>(&id, |cell| {
                    cell.set_filled(filled);
                    cell.set_cursor(is_cursor);
                });
            }
        }
        let moves = self.state.moves;
        let filled = self.state.filled_count();
        let _ = app.with_query_one_mut_as::<Label, _>("#moves", |l| l.set_text(moves_text(moves)));
        let _ = app.with_query_one_mut_as::<Label, _>("#progress", |l| l.set_text(progress_text(filled)));
        let _ = app.with_query_one_mut_as::<WinnerMessage, _>("WinnerMessage", |w| w.hide());
    }

    /// Update specific cells after a move.
    fn sync_cells(&self, app: &mut App, cells: &[(usize, usize)]) {
        for &(row, col) in cells {
            let id = format!("#cell-{}-{}", row, col);
            let filled = self.state.cells[row][col];
            let _ = app.with_query_one_mut_as::<GameCell, _>(&id, |cell| {
                cell.set_filled(filled);
            });
        }
        let moves = self.state.moves;
        let filled = self.state.filled_count();
        let _ = app.with_query_one_mut_as::<Label, _>("#moves", |l| l.set_text(moves_text(moves)));
        let _ = app.with_query_one_mut_as::<Label, _>("#progress", |l| l.set_text(progress_text(filled)));
    }

    /// Update cursor display (clear old, set new).
    fn sync_cursor(&self, app: &mut App, old: (usize, usize), new: (usize, usize)) {
        if old != new {
            let old_id = format!("#cell-{}-{}", old.0, old.1);
            let _ = app.with_query_one_mut_as::<GameCell, _>(&old_id, |cell| {
                cell.set_cursor(false);
            });
            let new_id = format!("#cell-{}-{}", new.0, new.1);
            let _ = app.with_query_one_mut_as::<GameCell, _>(&new_id, |cell| {
                cell.set_cursor(true);
            });
        }
    }
}

impl TextualApp for FiveByFiveApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        app.add_mode("help", || Box::new(HelpScreen));
        Ok(())
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("n", "new_game", "New Game"),
            BindingDecl::new("?", "app.push_screen('help')", "Help"),
            BindingDecl::new("q", "app.quit", "Quit"),
            BindingDecl::new("ctrl+d", "app.toggle_dark", "Toggle Dark Mode"),
        ]
    }

    fn compose(&mut self) -> AppRoot {
        let header = GameHeader::new(self.state.moves, self.state.filled_count());

        let mut grid = Grid::new(SIZE, SIZE).id("game-grid");
        for row in 0..SIZE {
            for col in 0..SIZE {
                let filled = self.state.cells[row][col];
                let is_cursor = self.state.cursor == (row, col);
                grid = grid.with_child(GameCell::new(row, col, filled, is_cursor));
            }
        }

        AppRoot::new()
            .with_child(header)
            .with_child(grid)
            .with_child(WinnerMessage::new())
            .with_child(Footer::new())
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut EventCtx) {
        let handled = match key.name() {
            // Navigation — arrow keys, WASD, hjkl
            "up" | "w" | "k" => {
                let old = self.state.navigate(-1, 0);
                self.sync_cursor(app, old, self.state.cursor);
                true
            }
            "down" | "s" | "j" => {
                let old = self.state.navigate(1, 0);
                self.sync_cursor(app, old, self.state.cursor);
                true
            }
            "left" | "a" | "h" => {
                let old = self.state.navigate(0, -1);
                self.sync_cursor(app, old, self.state.cursor);
                true
            }
            "right" | "d" | "l" => {
                let old = self.state.navigate(0, 1);
                self.sync_cursor(app, old, self.state.cursor);
                true
            }
            // Make a move at the current cursor position
            " " => {
                let affected = self.state.make_move();
                if !affected.is_empty() {
                    self.sync_cells(app, &affected);
                    if self.state.won {
                        let moves = self.state.moves;
                        let _ = app
                            .with_query_one_mut_as::<WinnerMessage, _>("WinnerMessage", |w| {
                                w.show(moves)
                            });
                    }
                }
                true
            }
            // New game
            "n" => {
                self.state.new_game();
                self.sync_all(app);
                true
            }
            _ => false,
        };
        if handled {
            ctx.set_handled();
            ctx.request_repaint();
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(FiveByFiveApp::new())
}

// ---------------------------------------------------------------------------
// Regression tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_by_five_app_composes_without_panic() {
        let mut app = FiveByFiveApp::new();
        let _root = app.compose();
    }

    #[test]
    fn game_state_starts_with_cross_filled() {
        let state = GameState::new();
        // Middle cross: (2,2), (1,2), (3,2), (2,1), (2,3) — 5 cells.
        assert_eq!(state.filled_count(), 5);
        assert!(!state.won);
    }

    #[test]
    fn game_state_toggle_cross_fills_adjacent() {
        let mut state = GameState::new();
        state.new_game();
        let before = state.filled_count();
        state.cursor = (0, 0);
        let affected = state.make_move();
        let after = state.filled_count();
        assert_ne!(before, after, "toggle cross should change cell count");
        assert_eq!(state.moves, 1);
        // Corner move affects 3 cells: (0,0), (1,0), (0,1)
        assert_eq!(affected.len(), 3);
    }

    #[test]
    fn game_state_navigate_wraps_at_edges() {
        let mut state = GameState::new();
        state.cursor = (0, 0);
        let old = state.navigate(-1, 0); // wrap top → bottom
        assert_eq!(old, (0, 0));
        assert_eq!(state.cursor.0, SIZE - 1, "should wrap to last row");
        let old = state.navigate(0, -1); // wrap left → right
        assert_eq!(old, (SIZE - 1, 0));
        assert_eq!(state.cursor.1, SIZE - 1, "should wrap to last col");
    }

    #[test]
    fn game_state_new_game_resets() {
        let mut state = GameState::new();
        state.cursor = (0, 0);
        state.make_move();
        assert!(state.moves > 0);

        state.new_game();
        assert_eq!(state.moves, 0);
        assert!(!state.won);
        assert_eq!(state.cursor, (SIZE / 2, SIZE / 2));
        assert_eq!(state.filled_count(), 5);
    }

    #[test]
    fn game_cell_classes_reflect_state() {
        let cell = GameCell::new(0, 0, false, false);
        assert!(cell.style_classes().is_empty());

        let cell = GameCell::new(1, 1, true, false);
        assert_eq!(cell.style_classes(), &["filled".to_string()]);

        let cell = GameCell::new(2, 2, false, true);
        assert_eq!(cell.style_classes(), &["cursor".to_string()]);

        let cell = GameCell::new(3, 3, true, true);
        assert_eq!(
            cell.style_classes(),
            &["filled".to_string(), "cursor".to_string()]
        );
    }

    #[test]
    fn game_cell_id_format() {
        assert_eq!(GameCell::id_for(2, 3), "cell-2-3");
        let cell = GameCell::new(2, 3, false, false);
        assert_eq!(cell.style_id(), Some("cell-2-3"));
    }

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

    #[test]
    fn game_header_label_texts() {
        assert_eq!(APP_TITLE, "5x5 -- A little annoying puzzle");
        assert_eq!(moves_text(0), "Moves: 0");
        assert_eq!(progress_text(5), "Filled: 5");
    }
}
