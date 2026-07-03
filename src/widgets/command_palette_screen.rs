//! Wave-1 CommandPalette as a real composed modal screen.
//!
//! This is the target architecture from `docs/devel/CMD_PALETTE_INVESTIGATION.md`
//! §3: a `SystemModalScreen` (Python `CommandPalette(SystemModalScreen[None])`,
//! `command.py:532`) whose body is composed of **real arena children** through
//! the RA2.1 `compose()`/`ChildDecl` path — no hand-drawn `FrameBuffer`, no
//! routing bypass:
//!
//! ```text
//! CommandPaletteScreen (SystemModalScreen; name "CommandPaletteScreen";
//!                       auto_focus "CommandInput"; ModalScreen dim = free)
//!   CommandPaletteBody (style_type "CommandPalette"; chrome-only render)
//!     Vertical #--container
//!       Horizontal #--input   ├ SearchIcon   └ CommandInput (wraps real Input)
//!       Vertical  #--results  { overlay: screen; height: auto }   ← Mechanism A
//!          ├ PaletteCommandList (OptionList subclass)   └ LoadingIndicator
//! ```
//!
//! Search: on `InputChanged` the body fuzzy-matches the provider snapshot and
//! pushes the new rows into the `PaletteCommandList` via a cross-node deferred
//! command (`WidgetCtx::query_one::<PaletteCommandList>().update_via`), so the
//! `CommandInput`'s text/cursor/focus is never rebuilt (the Select↔SelectOverlay
//! discipline).
//!
//! Selection → dismiss-with-result: a selected command bubbles a
//! [`CommandPaletteExecute`] to the screen root, which
//! `ctx.dismiss(SelectedCommandId { .. })`. The push site (the adapter, on the
//! app-root tree) registered a callback that defers a `CommandPaletteCommandSelected`
//! app message via the runtime `WidgetCommand::PostMessage` FIFO — so the chosen
//! command runs in the app context after the screen pops.

use rich_rs::{Console, ConsoleOptions, Segments, Style as RichStyle, Text};

use super::command_palette::{CommandInput, FuzzyMatcher, SearchIcon};
use super::option_list::{OptionItem, OptionList};
use super::{LoadingIndicator, Vertical, Widget};
use crate::compose::{ChildDecl, ComposeResult};
use crate::event::{Event, WidgetCtx};
use crate::message::{CommandPaletteCommand, InputChanged, InputSubmitted};
use crate::message::{MessageEvent, OptionHighlighted, OptionSelected};
use crate::screen::{Screen, ScreenMessageCtx};
use crate::widgets::{BindingDecl, Horizontal, NodeSeed};

// ---------------------------------------------------------------------------
// Cross-tree contract types
// ---------------------------------------------------------------------------

/// Dismiss-result value carried from the palette screen to the push callback.
///
/// The screen `ctx.dismiss(SelectedCommandId { .. })` on a selection; the
/// adapter's push callback downcasts it and defers a `CommandPaletteCommandSelected`
/// app message (see the module docs). Not a `Message` — it is a screen dismiss
/// value (`ScreenResult::Value`).
#[derive(Debug, Clone)]
pub(crate) struct SelectedCommandId {
    pub id: String,
    pub title: String,
}

/// Internal message bubbled from [`CommandPaletteBody`] to the screen root when a
/// command is selected, so the screen (which owns the screen-scoped dismiss
/// capability) can `ctx.dismiss(SelectedCommandId { .. })`.
#[derive(Debug, Clone)]
pub(crate) struct CommandPaletteExecute {
    pub id: String,
    pub title: String,
}
crate::impl_message!(CommandPaletteExecute);

// ---------------------------------------------------------------------------
// CommandRow — one search hit
// ---------------------------------------------------------------------------

/// A single command as displayed in the palette list: the command's id/title/help
/// plus the highlight ranges (char offsets into `title`) for the current query.
#[derive(Debug, Clone)]
struct CommandRow {
    id: String,
    title: String,
    help: String,
    ranges: Vec<(usize, usize)>,
}

/// Fuzzy-match `commands` against `query` (empty = discovery, sorted by title).
fn search_commands(commands: &[CommandPaletteCommand], query: &str) -> Vec<CommandRow> {
    let query = query.trim();
    if query.is_empty() {
        let mut rows: Vec<CommandRow> = commands
            .iter()
            .map(|c| CommandRow {
                id: c.id.clone(),
                title: c.title.clone(),
                help: c.help.clone(),
                ranges: Vec::new(),
            })
            .collect();
        rows.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        return rows;
    }
    let mut scored: Vec<(f64, CommandRow)> = commands
        .iter()
        .filter_map(|c| {
            FuzzyMatcher::score(query, &c.title).map(|score| {
                (
                    score,
                    CommandRow {
                        id: c.id.clone(),
                        title: c.title.clone(),
                        help: c.help.clone(),
                        ranges: FuzzyMatcher::highlight_ranges(query, &c.title),
                    },
                )
            })
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().map(|(_, row)| row).collect()
}

// ---------------------------------------------------------------------------
// PaletteCommandList — OptionList subclass
// ---------------------------------------------------------------------------

/// The palette's dropdown, delegating all structure/navigation/rendering to an
/// inner [`OptionList`] (`#[widget(base = OptionList)]`, mirroring
/// `SelectOverlay`). Supplies only the command-row rendering (title line with
/// fuzzy highlights + a dim help line, Python `command.py` `Command` option) and
/// its own CSS identity (`CommandList`).
#[textual::widget(base = OptionList, field = inner, style_type = "CommandList",
    override(layout_height))]
pub(crate) struct PaletteCommandList {
    inner: OptionList,
}

impl PaletteCommandList {
    fn from_rows(rows: &[CommandRow]) -> Self {
        // The base `CommandList { visibility: hidden }` default (Python parity,
        // shown via `.--visible`) hides the list until it has content; add
        // `--visible` so the populated dropdown is shown. Kept simple for Wave 1
        // (always shown when composed; the `.--populating` busy dance is 1.x).
        Self {
            inner: OptionList::with_items(Self::items(rows)).class("--visible"),
        }
    }

    /// Replace the displayed options in place (cross-node deferred update path;
    /// does NOT rebuild the sibling `CommandInput`).
    fn set_rows(&mut self, rows: &[CommandRow]) {
        self.inner.set_items(Self::items(rows));
    }

    /// Build one rich `OptionItem` per command row: a title line (with bold +
    /// underline over the fuzzy-matched ranges) then a dim help line.
    fn items(rows: &[CommandRow]) -> Vec<OptionItem> {
        rows.iter()
            .map(|row| {
                let mut text = Text::plain(row.title.clone());
                let hl = RichStyle::new().with_bold(true).with_underline(true);
                for &(start, end) in &row.ranges {
                    text.stylize(start, end, hl);
                }
                if !row.help.is_empty() {
                    text.append("\n", None);
                    text.append(row.help.clone(), Some(RichStyle::new().with_dim(true)));
                }
                OptionItem::rich_with_id(row.title.clone(), text, row.id.clone())
            })
            .collect()
    }

    /// Report the content auto-height plus the 2 rows of horizontal border chrome
    /// (`CommandList { border-top: blank; border-bottom: hkey black }`, Python
    /// `command.py:453-456`). Two reasons this override is required:
    /// 1. The `#[widget(base)]` derive does NOT delegate `layout_height` (a
    ///    `None`-defaulting Widget hook), so without it a `height: auto`
    ///    `CommandList` reports no intrinsic height and collapses to zero rows.
    /// 2. The layout engine's auto-HEIGHT edge does not add CSS chrome (unlike the
    ///    width edge), so the border would otherwise clip the option rows to zero
    ///    (exactly the `SelectOverlay::layout_height` +2 case).
    fn layout_height(&self) -> Option<usize> {
        self.inner.layout_height().map(|h| h + 2)
    }
}

// ---------------------------------------------------------------------------
// CommandPaletteBody — composed arena body
// ---------------------------------------------------------------------------

/// The composed body of the palette (chrome-only render, like `Select`). Owns
/// the provider-command snapshot and the current search results; drives the
/// `PaletteCommandList` options via a cross-node deferred command on each
/// keystroke. `style_type = "CommandPalette"` so the screen CSS (`CommandPalette
/// #--input`, `CommandPalette #--results`, …) resolves against its descendants.
pub(crate) struct CommandPaletteBody {
    commands: Vec<CommandPaletteCommand>,
    placeholder: String,
    /// Current filtered results (index-aligned with the `PaletteCommandList`).
    results: Vec<CommandRow>,
    /// Highlighted row (tracked from `OptionHighlighted`; used when Enter selects
    /// from the focused input rather than clicking a row).
    highlighted: usize,
    seed: NodeSeed,
}

impl CommandPaletteBody {
    pub(crate) fn new(commands: Vec<CommandPaletteCommand>, placeholder: impl Into<String>) -> Self {
        let placeholder = placeholder.into();
        let results = search_commands(&commands, "");
        Self {
            commands,
            placeholder,
            results,
            highlighted: 0,
            seed: NodeSeed::default(),
        }
    }

    fn execute(&self, index: usize, ctx: &mut WidgetCtx) {
        if let Some(row) = self.results.get(index) {
            ctx.post_message(CommandPaletteExecute {
                id: row.id.clone(),
                title: row.title.clone(),
            });
            ctx.set_handled();
        }
    }
}

impl Widget for CommandPaletteBody {
    fn style_type(&self) -> &'static str {
        "CommandPalette"
    }

    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn compose(&mut self) -> ComposeResult {
        // State-pure: rebuilt identically from `commands` / current results, so a
        // recompose regenerates rather than clears (Select discipline). Keystroke
        // search does NOT recompose this subtree — it updates the list options via
        // a cross-node deferred command, leaving the CommandInput untouched.
        let input_row = Horizontal::new()
            .id("--input")
            .with_child(SearchIcon::new())
            .with_child(CommandInput::new(self.placeholder.clone()));
        let results = Vertical::new()
            .id("--results")
            .with_child(PaletteCommandList::from_rows(&self.results))
            .with_child(LoadingIndicator::new());
        let container = Vertical::new()
            .id("--container")
            .with_child(input_row)
            .with_child(results);
        vec![ChildDecl::new(Box::new(container))]
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut WidgetCtx) {
        if let Some(changed) = message.downcast_ref::<InputChanged>() {
            // Re-run the fuzzy search over the provider snapshot and push the new
            // rows into the list via a cross-node deferred command (the list is a
            // descendant of this body). The CommandInput is a sibling subtree — it
            // is never rebuilt, so text/cursor/focus survive the keystroke.
            self.results = search_commands(&self.commands, &changed.value);
            self.highlighted = 0;
            let items = self.results.clone();
            ctx.query_one::<PaletteCommandList>()
                .update_via(ctx, move |list, cctx| {
                    list.set_rows(&items);
                    cctx.request_repaint();
                    cctx.request_layout();
                });
            ctx.request_repaint();
            ctx.set_handled();
            return;
        }
        if let Some(highlighted) = message.downcast_ref::<OptionHighlighted>() {
            self.highlighted = highlighted.index;
            return;
        }
        if let Some(selected) = message.downcast_ref::<OptionSelected>() {
            self.execute(selected.index, ctx);
            return;
        }
        if message.is::<InputSubmitted>() {
            // Enter in the focused input selects the highlighted command.
            let index = self.highlighted.min(self.results.len().saturating_sub(1));
            self.execute(index, ctx);
        }
    }
}

// ---------------------------------------------------------------------------
// CommandPaletteScreen — the pushed modal screen
// ---------------------------------------------------------------------------

/// The command palette as a pushed `SystemModalScreen` (Python
/// `CommandPalette(SystemModalScreen[None])`). Thin: it owns the command
/// snapshot + the screen-scoped surface (bindings, auto-focus, dismiss); the
/// [`CommandPaletteBody`] does the composing/searching.
pub struct CommandPaletteScreen {
    commands: Vec<CommandPaletteCommand>,
    placeholder: String,
}

impl CommandPaletteScreen {
    /// Create the palette screen from a synchronous provider-command snapshot
    /// (`TextualAppAdapter::gather_command_palette_commands`).
    pub fn new(commands: Vec<CommandPaletteCommand>) -> Self {
        Self {
            commands,
            placeholder: "Search for commands\u{2026}".to_string(),
        }
    }
}

/// The palette is a style-isolated system modal (Python
/// `CommandPalette(SystemModalScreen[None])`, `command.py:532`): `inherit_css =
/// false`, always modal.
impl super::command_palette::SystemModalScreen for CommandPaletteScreen {}

impl Screen for CommandPaletteScreen {
    fn name(&self) -> &str {
        "CommandPaletteScreen"
    }

    fn is_modal(&self) -> bool {
        true
    }

    /// Python `Screen.AUTO_FOCUS = "CommandInput"` (`command.py:535`).
    fn auto_focus(&self) -> Option<&str> {
        Some("CommandInput")
    }

    fn compose(&self) -> Box<dyn Widget> {
        Box::new(CommandPaletteBody::new(
            self.commands.clone(),
            self.placeholder.clone(),
        ))
    }

    fn css(&self) -> Option<&str> {
        // Layout only — the modal dim (`bg: $background 60%`) comes free from the
        // shared `ModalScreen` default CSS. Mirrors the load-bearing parts of
        // Python `command.py:548-630`: the results list escapes layout via
        // `overlay: screen` (Mechanism A) and the input row sizes to content.
        Some(
            "\
CommandPalette { color: $foreground; align-horizontal: center; }
CommandPalette > Vertical { margin-top: 3; height: auto; background: $surface; }
CommandPalette #--input { height: auto; }
CommandPalette #--results { overlay: screen; height: auto; }
CommandPalette CommandList { width: 1fr; height: auto; max-height: 12; }
CommandPalette LoadingIndicator { height: auto; display: none; }
",
        )
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("escape", "dismiss", "Close")]
    }

    fn on_event(&mut self, event: &Event, ctx: &mut ScreenMessageCtx) {
        // Escape (or a click that reached the screen root, i.e. on the dimmed
        // backdrop rather than a palette child) dismisses without a result.
        match event {
            Event::Key(key) if key.aliases().iter().any(|a| *a == "escape") => {
                ctx.dismiss_none();
            }
            Event::MouseDown(_) => {
                ctx.dismiss_none();
            }
            _ => {}
        }
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut ScreenMessageCtx) {
        if let Some(execute) = message.downcast_ref::<CommandPaletteExecute>() {
            ctx.dismiss(SelectedCommandId {
                id: execute.id.clone(),
                title: execute.title.clone(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventCtx;
    use crate::node_id::NodeId;
    use crate::screen::ScreenResult;
    use std::sync::Mutex;

    fn cmd(id: &str, title: &str, help: &str) -> CommandPaletteCommand {
        CommandPaletteCommand {
            id: id.to_string(),
            title: title.to_string(),
            help: help.to_string(),
        }
    }

    fn sample() -> Vec<CommandPaletteCommand> {
        vec![
            cmd("quit", "Quit", "Quit the app"),
            cmd("bell", "Bell", "Ring the bell"),
            cmd("theme", "Theme", "Change theme"),
        ]
    }

    #[test]
    fn empty_query_returns_all_sorted_by_title() {
        let rows = search_commands(&sample(), "");
        let titles: Vec<&str> = rows.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(titles, ["Bell", "Quit", "Theme"]);
        assert!(rows.iter().all(|r| r.ranges.is_empty()));
    }

    #[test]
    fn query_filters_and_highlights() {
        let rows = search_commands(&sample(), "bell");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "bell");
        assert!(!rows[0].ranges.is_empty(), "matched title should carry highlight ranges");
    }

    #[test]
    fn screen_accessors_match_python() {
        let screen = CommandPaletteScreen::new(sample());
        assert_eq!(screen.name(), "CommandPaletteScreen");
        assert_eq!(screen.auto_focus(), Some("CommandInput"));
        assert!(screen.is_modal());
        assert!(screen.css().is_some());
    }

    #[test]
    fn body_option_selected_posts_execute_for_that_row() {
        // Discovery order is Bell, Quit, Theme; selecting index 1 => Quit.
        let mut body = CommandPaletteBody::new(sample(), "search");
        let mut ectx = EventCtx::default();
        let mut ctx = crate::event::WidgetCtx::__from_dispatch(NodeId::default(), &mut ectx);
        body.on_message(
            &MessageEvent::new(NodeId::default(), OptionSelected { index: 1 }),
            &mut ctx,
        );
        let msgs = ectx.take_messages();
        let execute = msgs
            .iter()
            .find_map(|m| m.downcast_ref::<CommandPaletteExecute>())
            .expect("selection should bubble a CommandPaletteExecute");
        assert_eq!(execute.id, "quit");
        assert_eq!(execute.title, "Quit");
    }

    #[test]
    fn screen_execute_message_dismisses_with_selected_command_id() {
        let mut screen = CommandPaletteScreen::new(sample());
        let slot: Mutex<Option<ScreenResult>> = Mutex::new(None);
        let mut ectx = EventCtx::default();
        let mut sctx = ScreenMessageCtx::for_test(&mut ectx, &slot);
        screen.on_message(
            &MessageEvent::new(
                NodeId::default(),
                CommandPaletteExecute {
                    id: "bell".to_string(),
                    title: "Bell".to_string(),
                },
            ),
            &mut sctx,
        );
        let staged = slot.lock().unwrap().take().expect("execute should stage a dismissal");
        match staged {
            ScreenResult::Value(v) => {
                let sel = v.downcast_ref::<SelectedCommandId>().expect("SelectedCommandId");
                assert_eq!(sel.id, "bell");
                assert_eq!(sel.title, "Bell");
            }
            ScreenResult::Dismissed => panic!("expected a value dismissal"),
        }
    }
}
