//! Command-palette reusable primitives.
//!
//! The real, composed command palette is a `SystemModalScreen`
//! (`CommandPaletteScreen`) whose body is real arena children; it lives in
//! `command_palette_screen.rs` (Wave 1). This file keeps only the reusable
//! primitives that the screen (and provider integrations) build on:
//! `FuzzyMatcher`, the `Provider`/`ProviderResult` model, `SystemCommandsProvider`,
//! `PaletteCommand`, and the `SearchIcon` / `CommandInput` widgets — plus the
//! `SystemModalScreen` marker trait. The legacy always-mounted hand-drawn
//! `CommandPalette` host widget and its routing/host bypass were removed in Wave 2.

use rich_rs::{Console, ConsoleOptions, Segment, Segments};
use textual_macros::widget;

use crate::event::Event;
use crate::message::MessageEvent;
use crate::node_id::NodeId;
use crate::runtime::dispatch_ctx::set_dispatch_recipient;

use super::{Input, NodeSeed, NodeState, Widget, helpers::adjust_line_length_no_bg};

// ---------------------------------------------------------------------------
// Provider trait & ProviderResult
// ---------------------------------------------------------------------------

/// A single result from a [`Provider`] search.
#[derive(Debug, Clone)]
pub struct ProviderResult {
    /// Unique identifier for this result (used in selection messages).
    pub id: String,
    /// Display text shown in the result list.
    pub title: String,
    /// Optional help text shown below the title.
    pub help: String,
    /// Match score — higher values sort first.
    pub score: f64,
}

/// A source of commands for the command palette.
///
/// Implement this trait to feed commands into the palette from any source.
///
/// # Lifecycle
///
/// * [`startup()`](Provider::startup) — called once when the palette opens.
/// * [`search()`](Provider::search) — called on every keystroke.
/// * [`shutdown()`](Provider::shutdown) — called when the palette closes.
pub trait Provider: Send + Sync + 'static {
    /// Human-readable name for this provider.
    fn name(&self) -> &str;

    /// Called once when the command palette opens.
    fn startup(&mut self) {}

    /// Return commands matching `query`. Empty `query` = discovery mode.
    fn search(&self, query: &str) -> Vec<ProviderResult>;

    /// Called when the command palette closes.
    fn shutdown(&mut self) {}
}

// ---------------------------------------------------------------------------
// SystemCommandsProvider
// ---------------------------------------------------------------------------

/// Built-in provider that serves the static list of [`PaletteCommand`]s.
pub struct SystemCommandsProvider {
    commands: Vec<PaletteCommand>,
    indexed: Vec<SystemCommandEntry>,
}

#[derive(Debug, Clone)]
struct SystemCommandEntry {
    command: PaletteCommand,
    title_lower: String,
}

impl SystemCommandsProvider {
    pub fn new(commands: Vec<PaletteCommand>) -> Self {
        let indexed = commands
            .iter()
            .cloned()
            .map(|command| SystemCommandEntry {
                title_lower: command.title.to_lowercase(),
                command,
            })
            .collect();
        Self { commands, indexed }
    }

    /// Borrow the current command list.
    /// Public API accessor for command palette providers.
    #[allow(dead_code)]
    pub fn commands(&self) -> &[PaletteCommand] {
        &self.commands
    }
}

impl Provider for SystemCommandsProvider {
    fn name(&self) -> &str {
        "system"
    }

    fn search(&self, query: &str) -> Vec<ProviderResult> {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            let mut sorted = self.indexed.iter().collect::<Vec<_>>();
            sorted.sort_by(|a, b| a.title_lower.cmp(&b.title_lower));
            return sorted
                .into_iter()
                .map(|entry| ProviderResult {
                    id: entry.command.id.clone(),
                    title: entry.command.title.clone(),
                    help: entry.command.help.clone(),
                    score: 0.0,
                })
                .collect();
        }
        self.indexed
            .iter()
            .filter_map(|entry| {
                // Python parity: system command search score is based on title only.
                FuzzyMatcher::score(&needle, &entry.title_lower).map(|score| ProviderResult {
                    id: entry.command.id.clone(),
                    title: entry.command.title.clone(),
                    help: entry.command.help.clone(),
                    score,
                })
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// FuzzyMatcher
// ---------------------------------------------------------------------------

/// Python Textual-aligned fuzzy matcher.
///
/// Ranking semantics intentionally mirror `textual.fuzzy.FuzzySearch`:
/// - case-insensitive subsequence matching,
/// - score boost for word-start hits,
/// - score boost for contiguous groups,
/// - substring fast-path with multiplier.
pub struct FuzzyMatcher;

impl FuzzyMatcher {
    fn first_letter_positions(candidate: &[char]) -> std::collections::HashSet<usize> {
        let mut starts = std::collections::HashSet::new();
        let mut in_word = false;
        for (idx, ch) in candidate.iter().enumerate() {
            let is_word = ch.is_alphanumeric() || *ch == '_';
            if is_word && !in_word {
                starts.insert(idx);
            }
            in_word = is_word;
        }
        starts
    }

    fn score_positions(candidate: &[char], positions: &[usize]) -> f64 {
        if positions.is_empty() {
            return 0.0;
        }
        let first_letters = Self::first_letter_positions(candidate);
        let offset_count = positions.len() as f64;
        let first_letter_hits = positions
            .iter()
            .filter(|offset| first_letters.contains(offset))
            .count() as f64;

        let mut groups = 1usize;
        let mut last = positions[0];
        for &offset in positions.iter().skip(1) {
            if offset != last + 1 {
                groups += 1;
            }
            last = offset;
        }
        let normalized_groups = (offset_count - (groups.saturating_sub(1) as f64)) / offset_count;
        (offset_count + first_letter_hits) * (1.0 + normalized_groups * normalized_groups)
    }

    fn find_subslice(haystack: &[char], needle: &[char]) -> Option<usize> {
        if needle.is_empty() {
            return Some(0);
        }
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    }

    fn best_match_indices(query: &str, text: &str) -> Option<(f64, Vec<usize>)> {
        if query.is_empty() {
            return Some((0.0, Vec::new()));
        }
        let query_chars: Vec<char> = query.chars().map(|ch| ch.to_ascii_lowercase()).collect();
        let text_chars: Vec<char> = text.chars().map(|ch| ch.to_ascii_lowercase()).collect();
        if query_chars.len() > text_chars.len() {
            return None;
        }

        // Python parity: quick substring fast-path with multiplier.
        if let Some(start) = Self::find_subslice(&text_chars, &query_chars) {
            let offsets = (start..start + query_chars.len()).collect::<Vec<_>>();
            let base = Self::score_positions(&text_chars, &offsets);
            let exact = text_chars == query_chars;
            let boosted = base * if exact { 2.0 } else { 1.5 };
            return Some((boosted, offsets));
        }

        let mut letter_positions = Vec::with_capacity(query_chars.len());
        let mut position = 0usize;

        for (offset, &needle) in query_chars.iter().enumerate() {
            let last_index = text_chars.len().saturating_sub(offset);
            let mut positions = Vec::new();
            let mut index = position;
            while index < text_chars.len() {
                if let Some(found_rel) = text_chars[index..].iter().position(|&ch| ch == needle) {
                    let location = index + found_rel;
                    positions.push(location);
                    index = location + 1;
                    if index >= last_index {
                        break;
                    }
                } else {
                    break;
                }
            }
            if positions.is_empty() {
                return None;
            }
            position = positions[0] + 1;
            letter_positions.push(positions);
        }

        let mut best: Option<(f64, Vec<usize>)> = None;
        let query_len = query_chars.len();
        let mut stack: Vec<usize> = Vec::with_capacity(query_len);

        fn recurse(
            letter_positions: &[Vec<usize>],
            positions_index: usize,
            stack: &mut Vec<usize>,
            candidate_chars: &[char],
            best: &mut Option<(f64, Vec<usize>)>,
        ) {
            for &offset in &letter_positions[positions_index] {
                if stack.last().is_some_and(|last| offset <= *last) {
                    continue;
                }
                stack.push(offset);
                if positions_index + 1 == letter_positions.len() {
                    let score = FuzzyMatcher::score_positions(candidate_chars, stack);
                    match best {
                        Some((best_score, _)) if *best_score >= score => {}
                        _ => *best = Some((score, stack.clone())),
                    }
                } else {
                    recurse(
                        letter_positions,
                        positions_index + 1,
                        stack,
                        candidate_chars,
                        best,
                    );
                }
                let _ = stack.pop();
            }
        }

        recurse(&letter_positions, 0, &mut stack, &text_chars, &mut best);
        best
    }

    /// Convert matched indices into contiguous `[start, end)` character ranges.
    pub fn highlight_ranges(query: &str, text: &str) -> Vec<(usize, usize)> {
        let Some((score, indices)) = Self::best_match_indices(query, text) else {
            return Vec::new();
        };
        if score <= 0.0 || indices.is_empty() {
            return Vec::new();
        }

        let mut ranges = Vec::new();
        let mut start = indices[0];
        let mut prev = indices[0];
        for &idx in indices.iter().skip(1) {
            if idx == prev + 1 {
                prev = idx;
                continue;
            }
            ranges.push((start, prev + 1));
            start = idx;
            prev = idx;
        }
        ranges.push((start, prev + 1));
        ranges
    }

    /// Returns a score if all characters in `query` appear (in order) in `text`.
    /// Higher score = better match. Returns `None` if no match.
    pub fn score(query: &str, text: &str) -> Option<f64> {
        Self::best_match_indices(query, text).map(|(score, _)| score)
    }
}

// ---------------------------------------------------------------------------
// PaletteCommand
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PaletteCommand {
    pub id: String,
    pub title: String,
    pub help: String,
}

impl PaletteCommand {
    pub fn new(id: impl Into<String>, title: impl Into<String>, help: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            help: help.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// SearchIcon
// ---------------------------------------------------------------------------

/// Widget for displaying a search icon before the command input.
#[derive(Debug, Clone)]
#[widget()]
pub struct SearchIcon {
    icon: String,
    seed: NodeSeed,
}

impl SearchIcon {
    crate::seed_ident_methods!();

    pub fn new() -> Self {
        Self {
            icon: "🔎".to_string(),
            seed: NodeSeed::default(),
        }
    }

    pub fn set_icon(&mut self, icon: impl Into<String>) {
        self.icon = icon.into();
    }
}

impl Default for SearchIcon {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::widgets::Render for SearchIcon {
    fn style_type(&self) -> &'static str {
        "SearchIcon"
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let line = adjust_line_length_no_bg(
            &[Segment::new(self.icon.clone())],
            width.max(rich_rs::cell_len(&self.icon)),
        );
        line.into_iter().collect()
    }
}
// ---------------------------------------------------------------------------
// CommandInput
// ---------------------------------------------------------------------------

/// Command palette input control (`CommandInput` in Python Textual).
#[widget(Focus, Interactive, Scrollable)]
pub struct CommandInput {
    input: Input,
    seed: NodeSeed,
    focused: bool,
}

impl CommandInput {
    crate::seed_ident_methods!();

    pub fn new(placeholder: impl Into<String>) -> Self {
        Self {
            input: Input::new()
                .with_style_type("CommandInput", ["Input"])
                .class("command-palette--input")
                .with_placeholder(placeholder),
            seed: NodeSeed::default(),
            focused: false,
        }
    }

    pub fn text(&self) -> &str {
        self.input.text()
    }

    pub fn set_text(&mut self, value: impl Into<String>) {
        self.input.set_text(value);
    }

    pub fn input_node_id(&self) -> NodeId {
        self.input.node_id()
    }
}

impl crate::widgets::Focus for CommandInput {
    fn focusable(&self) -> bool {
        Widget::focusable(&self.input)
    }
}

impl crate::widgets::Interactive for CommandInput {
    fn on_mount(&mut self, ctx: &mut crate::event::WidgetCtx) {
        Widget::on_mount(&mut self.input, ctx);
    }

    fn on_unmount(&mut self) {
        Widget::on_unmount(&mut self.input);
    }

    fn on_tick(&mut self, tick: u64) {
        Widget::on_tick(&mut self.input, tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        Widget::on_resize(&mut self.input, width, height);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        Widget::on_layout(&mut self.input, width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
        let state = NodeState {
            focused: self.focused,
            ..Default::default()
        };
        let _guard = set_dispatch_recipient(self.input.node_id(), state);
        Widget::on_event_capture(&mut self.input, event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
        let state = NodeState {
            focused: self.focused,
            ..Default::default()
        };
        let _guard = set_dispatch_recipient(self.input.node_id(), state);
        Widget::on_event(&mut self.input, event, ctx);
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut crate::event::WidgetCtx) {
        Widget::on_message(&mut self.input, message, ctx);
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        Widget::on_mouse_move(&mut self.input, x, y)
    }

    fn on_node_state_changed(
        &mut self,
        old: crate::widgets::NodeState,
        new: crate::widgets::NodeState,
    ) {
        self.focused = new.focused;
        Widget::on_node_state_changed(&mut self.input, old, new);
    }
}

impl crate::widgets::Scrollable for CommandInput {
    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut crate::event::WidgetCtx) {
        self.input.on_mouse_scroll(delta_x, delta_y, ctx);
    }
}

impl crate::widgets::Render for CommandInput {
    fn style_type(&self) -> &'static str {
        "CommandInput"
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(&self.input, console, options)
    }
}
// ---------------------------------------------------------------------------
// SystemModalScreen — marker trait for system-level modal screens
// ---------------------------------------------------------------------------

/// A variant of `Screen` for system-level modal overlays.
///
/// System modal screens (such as the command palette) are isolated from the
/// main application CSS and are used for internal/system UI. They always
/// render as modal (blocking interaction with screens below) and do not
/// inherit the app's stylesheet by default.
///
/// This follows the Python Textual `SystemModalScreen` pattern.
pub trait SystemModalScreen: crate::screen::Screen {
    /// Whether this screen inherits CSS from the application.
    ///
    /// Default: `false` — system screens are style-isolated.
    fn inherit_css(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_commands_provider_returns_all_on_empty_query() {
        let commands = vec![
            PaletteCommand::new("a", "Alpha", "First"),
            PaletteCommand::new("b", "Beta", "Second"),
        ];
        let provider = SystemCommandsProvider::new(commands);
        let results = provider.search("");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "a");
        assert_eq!(results[1].id, "b");
    }

    #[test]
    fn system_commands_provider_filters_on_query() {
        let commands = vec![
            PaletteCommand::new("a", "Alpha", "First"),
            PaletteCommand::new("b", "Beta", "Second"),
        ];
        let provider = SystemCommandsProvider::new(commands);
        let results = provider.search("beta");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "b");
    }

    #[test]
    fn system_commands_provider_search_scores_title_only() {
        let commands = vec![PaletteCommand::new(
            "keys",
            "Keys",
            "Show help for the focused widget and a summary of available keys",
        )];
        let provider = SystemCommandsProvider::new(commands);
        let results = provider.search("focused");
        assert!(
            results.is_empty(),
            "title-only scoring should not match query text present only in help"
        );
    }

    #[test]
    fn system_commands_provider_discovery_is_alpha_but_search_keeps_source_order_for_ties() {
        let commands = vec![
            PaletteCommand::new("theme", "Theme", "Change theme"),
            PaletteCommand::new("quit", "Quit", "Quit app"),
            PaletteCommand::new("keys", "Keys", "Show keys"),
            PaletteCommand::new("screenshot", "Screenshot", "Take screenshot"),
        ];
        let provider = SystemCommandsProvider::new(commands);

        let discover_titles = provider
            .search("")
            .into_iter()
            .map(|result| result.title)
            .collect::<Vec<_>>();
        assert_eq!(discover_titles, vec!["Keys", "Quit", "Screenshot", "Theme"]);

        let searched = provider.search("e");
        assert!(
            !searched.is_empty(),
            "query should produce matches for tie-order check"
        );
        assert_eq!(
            searched[0].id, "theme",
            "search-mode tie ordering should preserve provider source order"
        );
    }

    #[test]
    fn fuzzy_matcher_highlight_ranges_follow_matched_subsequence() {
        assert_eq!(FuzzyMatcher::highlight_ranges("ey", "Keys"), vec![(1, 3)]);
        assert_eq!(
            FuzzyMatcher::highlight_ranges("ss", "Screenshot"),
            vec![(0, 1), (6, 7)]
        );
        assert!(FuzzyMatcher::highlight_ranges("zzz", "Keys").is_empty());
    }
}
