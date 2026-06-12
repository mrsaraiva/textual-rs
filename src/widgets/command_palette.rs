use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::event::{
    Action, AnimationEase, AnimationLevel, AnimationRequest, AnimationValueEvent, Event, EventCtx,
};
use crate::message::*;
use crate::render::{Cell, FrameBuffer};
use crate::style::TransitionTiming;

use crate::node_id::NodeId;

use crate::action::ParsedAction;

use super::{
    BindingDecl, Input, KeyPanel, ListView, Overlay, Spacer, Widget, WidgetRenderable,
    WidgetStyles, helpers,
    helpers::adjust_line_length_no_bg,
};

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

/// A source of commands for the [`CommandPalette`].
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
            .into_iter()
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

#[derive(Debug, Clone)]
struct CommandListEntry {
    title: String,
    help: String,
    title_highlight_ranges: Vec<(usize, usize)>,
}

/// Widget for displaying a search icon before the command input.
#[derive(Debug, Clone)]
pub struct SearchIcon {
    icon: String,
    styles: WidgetStyles,
}

impl SearchIcon {
    pub fn new() -> Self {
        Self {
            icon: "🔎".to_string(),
            styles: WidgetStyles::default(),
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

impl Widget for SearchIcon {
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

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for SearchIcon {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

/// Command palette input control (`CommandInput` in Python Textual).
pub struct CommandInput {
    input: Input,
    styles: WidgetStyles,
}

impl CommandInput {
    pub fn new(placeholder: impl Into<String>) -> Self {
        Self {
            input: Input::new()
                .with_style_type("CommandInput", ["Input"])
                .class("command-palette--input")
                .with_placeholder(placeholder),
            styles: WidgetStyles::default(),
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

impl Widget for CommandInput {
    fn style_type(&self) -> &'static str {
        "CommandInput"
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(&self.input, console, options)
    }

    fn on_mount(&mut self) {
        self.input.on_mount();
    }

    fn on_unmount(&mut self) {
        self.input.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.input.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.input.on_resize(width, height);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.input.on_layout(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.input.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.input.on_event(event, ctx);
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        self.input.on_message(message, ctx);
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        self.input.on_mouse_move(x, y)
    }

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        self.input.on_mouse_scroll(delta_x, delta_y, ctx);
    }

    fn focusable(&self) -> bool {
        self.input.focusable()
    }

    fn set_focus(&mut self, focused: bool) {
        self.input.set_focus(focused);
    }

    fn has_focus(&self) -> bool {
        self.input.has_focus()
    }

    fn style_classes(&self) -> &[String] {
        self.input.style_classes()
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for CommandInput {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

/// Command result list widget mirroring Python's `CommandList`.
#[derive(Debug)]
pub struct CommandList {
    list: ListView,
    entries: Vec<CommandListEntry>,
    visible: bool,
    populating: bool,
    surface_bg: Option<crate::style::Color>,
    help_style_override: Mutex<Option<rich_rs::Style>>,
    highlight_style_override: Mutex<Option<rich_rs::Style>>,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl CommandList {
    pub fn new() -> Self {
        Self {
            list: ListView::new(Vec::new()).scroll_step(2),
            entries: Vec::new(),
            visible: false,
            populating: false,
            surface_bg: None,
            help_style_override: Mutex::new(None),
            highlight_style_override: Mutex::new(None),
            classes: vec!["command-list".to_string()],
            styles: WidgetStyles::default(),
        }
    }

    pub fn selected(&self) -> usize {
        self.list.selected()
    }

    pub fn offset(&self) -> usize {
        self.list.offset()
    }

    pub fn hovered_index(&self) -> Option<usize> {
        self.list.hovered_index()
    }

    pub fn set_selected(&mut self, index: usize) {
        self.list.set_selected(index);
    }

    fn set_entries(&mut self, entries: Vec<CommandListEntry>) {
        let labels = entries.iter().map(|entry| entry.title.clone()).collect();
        self.entries = entries;
        self.list.set_items(labels);
        self.list.set_selected(0);
        self.visible = !self.entries.is_empty();
        self.rebuild_classes();
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
        self.rebuild_classes();
    }

    fn set_populating(&mut self, populating: bool) {
        self.populating = populating;
        self.rebuild_classes();
    }

    fn set_surface_bg(&mut self, surface_bg: Option<crate::style::Color>) {
        self.surface_bg = surface_bg;
    }

    fn rebuild_classes(&mut self) {
        self.classes.clear();
        self.classes.push("command-list".to_string());
        if self.visible {
            self.classes.push("--visible".to_string());
        }
        if self.populating {
            self.classes.push("--populating".to_string());
        }
    }

    fn set_help_style_override(&self, style: Option<rich_rs::Style>) {
        if let Ok(mut guard) = self.help_style_override.lock() {
            *guard = style;
        }
    }

    fn set_highlight_style_override(&self, style: Option<rich_rs::Style>) {
        if let Ok(mut guard) = self.highlight_style_override.lock() {
            *guard = style;
        }
    }
}

impl Default for CommandList {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandList {
    fn render_with_help_style(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        help_style_override: Option<rich_rs::Style>,
    ) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let mut out = Segments::new();

        let default_bg = self
            .surface_bg
            .or_else(|| crate::style::parse_color_like("$background"))
            .unwrap_or(crate::style::Color::rgb(0, 0, 0));

        let option_style = crate::css::resolve_component_style(self, &["option-list--option"]);
        let option_padding = option_style.effective_padding();
        let left_pad = option_padding.left as usize;
        let right_pad = option_padding.right as usize;
        let base_title_style = option_style
            .to_rich_over(default_bg)
            .unwrap_or_else(rich_rs::Style::new);
        let hover_style = crate::css::resolve_component_style(self, &["option-list--option-hover"])
            .to_rich_over(default_bg)
            .unwrap_or(base_title_style);
        let selected_style =
            crate::css::resolve_component_style(self, &["option-list--option-highlighted"])
                .to_rich_over(default_bg)
                .unwrap_or(base_title_style);
        let mut help_style = help_style_override.unwrap_or_else(|| {
            crate::css::resolve_component_style(self, &["command-palette--help-text"])
                .to_rich_over(default_bg)
                .unwrap_or(base_title_style)
        });
        help_style.dim = Some(true);
        help_style.bold = Some(false);
        help_style.bgcolor = None;
        let highlight_style = self
            .highlight_style_override
            .lock()
            .ok()
            .and_then(|guard| *guard)
            .or_else(|| {
                crate::css::resolve_component_style(self, &["command-palette--highlight"])
                    .to_rich_without_colors()
            });

        let visible_items = (height / 2).max(1);
        let start = self
            .offset()
            .min(self.entries.len().saturating_sub(visible_items));
        let selected = self.selected().min(self.entries.len().saturating_sub(1));
        let hovered = self.hovered_index();

        let pad_row = |line: &[Segment], pad_style: rich_rs::Style| -> Vec<Segment> {
            let mut adjusted =
                rich_rs::Segment::adjust_line_length(line, width, Some(pad_style), false);
            let len = rich_rs::Segment::get_line_length(&adjusted);
            if len < width {
                adjusted.push(rich_rs::Segment::styled(" ".repeat(width - len), pad_style));
            }
            adjusted
        };

        for visual_row in 0..height {
            let entry_row = visual_row / 2;
            let is_help = visual_row % 2 == 1;
            let index = if visual_row >= visible_items.saturating_mul(2) {
                self.entries.len()
            } else {
                start.saturating_add(entry_row)
            };

            let line = if index >= self.entries.len() {
                adjust_line_length_no_bg(&[], width)
            } else {
                let entry = &self.entries[index];
                let active = index == selected;
                let row_base_style = if active {
                    selected_style
                } else if hovered == Some(index) {
                    hover_style
                } else {
                    base_title_style
                };
                let text = if is_help { &entry.help } else { &entry.title };
                let style = if is_help {
                    row_base_style.combine(&help_style)
                } else {
                    row_base_style
                };
                let available_width = width.saturating_sub(left_pad + right_pad).max(1);
                let mut padded_text = String::new();
                if left_pad > 0 {
                    padded_text.push_str(&" ".repeat(left_pad));
                }
                padded_text.push_str(text);
                let mut rich_text = console.render_str(&padded_text, Some(true), None, None, None);
                rich_text.stylize_before(style, 0, None);
                if !is_help && let Some(highlight_style) = highlight_style {
                    for &(start, end) in &entry.title_highlight_ranges {
                        rich_text.stylize(start + left_pad, end + left_pad, highlight_style);
                    }
                }
                let rendered = rich_text.render(console, options);
                let lines = rich_rs::Segment::split_and_crop_lines(
                    rendered,
                    available_width + left_pad,
                    None,
                    false,
                    false,
                );
                let first_line = lines.into_iter().next().unwrap_or_default();
                pad_row(&first_line, style)
            };
            out.extend(line);
            if visual_row + 1 < height {
                out.push(Segment::line());
            }
        }
        out
    }
}

impl Widget for CommandList {
    fn style_type(&self) -> &'static str {
        "CommandList"
    }

    fn style_classes(&self) -> &[String] {
        &self.classes
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let help_style_override = self
            .help_style_override
            .lock()
            .ok()
            .and_then(|guard| guard.clone());
        self.render_with_help_style(console, options, help_style_override)
    }

    fn on_mount(&mut self) {
        self.list.on_mount();
    }

    fn on_unmount(&mut self) {
        self.list.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.list.on_tick(tick);
    }

    fn on_resize(&mut self, _width: u16, height: u16) {
        self.list.on_resize(1, (height / 2).max(1));
    }

    fn on_layout(&mut self, _width: u16, height: u16) {
        self.list.on_layout(1, (height / 2).max(1));
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.list.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::MouseDown(mouse) => {
                // CommandList renders each entry as two visual rows
                // (title + help). Map pointer Y back to OptionList row space.
                let mapped = Event::MouseDown(crate::event::MouseDownEvent {
                    target: mouse.target,
                    screen_x: mouse.screen_x,
                    screen_y: mouse.screen_y,
                    x: mouse.x,
                    y: mouse.y / 2,
                });
                self.list.on_event(&mapped, ctx);
            }
            _ => self.list.on_event(event, ctx),
        }
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        self.list.on_message(message, ctx);
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        self.list.on_mouse_move(x, y / 2)
    }

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        self.list
            .on_mouse_scroll(delta_x, delta_y.saturating_mul(2), ctx);
    }

    fn focusable(&self) -> bool {
        false
    }

    fn set_focus(&mut self, _focused: bool) {}

    fn has_focus(&self) -> bool {
        false
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for CommandList {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

pub struct CommandPalette {
    child: Box<dyn Widget>,
    child_extracted: bool,
    tree_show_wrapped_child: bool,
    open: bool,
    show_key_panel: bool,
    help_panel_visible: bool,
    search_icon: SearchIcon,
    query: CommandInput,
    list: CommandList,
    key_panel: KeyPanel,
    /// Built-in system commands served via the Provider pattern.
    system_provider: SystemCommandsProvider,
    key_panel_render_width: f32,
    panel_visible: bool,
    panel_render_y: f32,
    previously_focused_child: Option<NodeId>,
    layout_width: usize,
    layout_height: usize,
    last_render_width: AtomicUsize,
    last_render_height: AtomicUsize,
    styles: WidgetStyles,
    /// External providers registered via [`add_provider`](Self::add_provider).
    providers: Vec<Box<dyn Provider>>,
    /// Merged results from all providers, sorted by descending score.
    provider_results: Vec<ProviderResult>,
    /// Last query used to build `provider_results`.
    last_built_query: String,
}

impl CommandPalette {
    const KEY_PANEL_WIDTH_ATTR: &'static str = "command_palette.key_panel_width";
    const PANEL_Y_ATTR: &'static str = "command_palette.panel_y";
    const CLOSED_PANEL_Y: f32 = 0.0;
    const SEARCH_ROW_OFFSET: usize = 2;
    const RESULTS_ROW_OFFSET: usize = 5;
    const HEADER_ROWS: usize = Self::RESULTS_ROW_OFFSET;
    const SEARCH_ICON_X_OFFSET: usize = 2;
    const SEARCH_TEXT_X_OFFSET: usize = 5;

    pub fn new(child: impl Widget + 'static) -> Self {
        let commands = vec![
            PaletteCommand::new(
                "keys",
                "Keys",
                "Show help for the focused widget and a summary of available keys",
            ),
            PaletteCommand::new("quit", "Quit", "Quit the application as soon as possible"),
            PaletteCommand::new(
                "screenshot",
                "Screenshot",
                "Save an SVG 'screenshot' of the current screen",
            ),
            PaletteCommand::new("theme", "Theme", "Change the current theme"),
        ];
        let mut out = Self {
            child: Box::new(child),
            child_extracted: false,
            tree_show_wrapped_child: true,
            open: false,
            show_key_panel: false,
            help_panel_visible: false,
            search_icon: SearchIcon::new(),
            query: CommandInput::new("Search for commands…"),
            list: CommandList::new(),
            key_panel: KeyPanel::new(),
            system_provider: SystemCommandsProvider::new(commands),
            key_panel_render_width: 0.0,
            panel_visible: false,
            panel_render_y: 0.0,
            previously_focused_child: None,
            layout_width: 1,
            layout_height: 1,
            last_render_width: AtomicUsize::new(1),
            last_render_height: AtomicUsize::new(1),
            styles: WidgetStyles::default(),
            providers: Vec::new(),
            provider_results: Vec::new(),
            last_built_query: String::new(),
        };
        out.rebuild_results(true);
        out
    }

    /// Register an external command provider.
    pub fn add_provider(&mut self, provider: impl Provider) {
        self.providers.push(Box::new(provider));
    }

    /// Builder variant of [`add_provider`](Self::add_provider).
    pub fn with_provider(mut self, provider: impl Provider) -> Self {
        self.add_provider(provider);
        self
    }

    pub fn with_commands(mut self, commands: Vec<PaletteCommand>) -> Self {
        self.system_provider = SystemCommandsProvider::new(commands);
        self.rebuild_results(true);
        self
    }

    pub fn set_commands(&mut self, commands: Vec<PaletteCommand>) {
        self.system_provider = SystemCommandsProvider::new(commands);
        self.rebuild_results(true);
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn is_visible_in_tree(&self) -> bool {
        self.open || self.panel_visible
    }

    /// Configure whether the extracted wrapped child subtree is rendered in tree mode.
    ///
    /// Runtime command-palette hosts should disable this because the app body is
    /// rendered as a sibling subtree and should not be re-rendered inside the palette node.
    pub fn with_tree_wrapped_child_visible(mut self, visible: bool) -> Self {
        self.tree_show_wrapped_child = visible;
        self
    }

    fn key_panel_width(&self, width: usize) -> usize {
        if !self.show_key_panel || width < 56 {
            return 0;
        }
        let preferred = ((width as f32) * 0.30).round() as usize;
        preferred.clamp(28, 40).min(width.saturating_sub(20))
    }

    fn visible_key_panel_width(&self, width: usize) -> usize {
        if self.open {
            return 0;
        }
        let max_width = self.key_panel_width(width);
        if !self.show_key_panel && self.key_panel_render_width <= 0.5 {
            return 0;
        }
        self.key_panel_render_width
            .round()
            .clamp(0.0, max_width as f32) as usize
    }

    fn key_panel_animation_params(&self) -> Option<(Duration, Duration, AnimationEase)> {
        let style = crate::css::resolve_component_style(self, &["command-palette--key-panel"]);
        let duration = style.transition_duration?;
        if duration.is_zero() {
            return None;
        }
        let delay = style.transition_delay.unwrap_or(Duration::ZERO);
        let ease = style
            .transition_timing
            .map(Self::transition_timing_to_animation_ease)
            .unwrap_or(AnimationEase::OutCubic);
        Some((duration, delay, ease))
    }

    fn panel_animation_params(&self) -> Option<(Duration, Duration, AnimationEase)> {
        let style = crate::css::resolve_component_style(self, &["command-palette--panel"]);
        let duration = style.transition_duration?;
        if duration.is_zero() {
            return None;
        }
        let delay = style.transition_delay.unwrap_or(Duration::ZERO);
        let ease = style
            .transition_timing
            .map(Self::transition_timing_to_animation_ease)
            .unwrap_or(AnimationEase::OutCubic);
        Some((duration, delay, ease))
    }

    fn transition_timing_to_animation_ease(timing: TransitionTiming) -> AnimationEase {
        match timing {
            TransitionTiming::Linear => AnimationEase::Linear,
            TransitionTiming::InOutCubic => AnimationEase::InOutCubic,
            TransitionTiming::OutCubic => AnimationEase::OutCubic,
            TransitionTiming::Round => AnimationEase::Round,
            TransitionTiming::None => AnimationEase::None,
        }
    }

    fn update_surface_styles(&mut self) {
        let panel_bg = crate::css::resolve_component_style(self, &["command-palette--panel"]).bg;
        self.list.set_surface_bg(panel_bg);
    }

    fn animate_key_panel_width(&mut self, from: usize, to: usize, ctx: &mut EventCtx) {
        if from == to {
            self.key_panel_render_width = to as f32;
            return;
        }
        if let Some((duration, delay, ease)) = self.key_panel_animation_params() {
            self.key_panel_render_width = from as f32;
            ctx.request_animation(
                AnimationRequest::new(
                    self.node_id(),
                    Self::KEY_PANEL_WIDTH_ATTR,
                    from as f32,
                    to as f32,
                    duration,
                )
                .with_delay(delay)
                .with_ease(ease)
                .with_level(AnimationLevel::Basic),
            );
        } else {
            self.key_panel_render_width = to as f32;
        }
    }

    fn animate_panel_y(&mut self, from: f32, to: f32, ctx: &mut EventCtx) {
        if (from - to).abs() < f32::EPSILON {
            self.panel_render_y = to;
            return;
        }
        if let Some((duration, delay, ease)) = self.panel_animation_params() {
            self.panel_render_y = from;
            ctx.request_animation(
                AnimationRequest::new(self.node_id(), Self::PANEL_Y_ATTR, from, to, duration)
                    .with_delay(delay)
                    .with_ease(ease)
                    .with_level(AnimationLevel::Basic),
            );
        } else {
            self.panel_render_y = to;
            if !self.open && self.panel_render_y <= Self::CLOSED_PANEL_Y {
                self.panel_visible = false;
            }
        }
    }

    fn panel_target_y(&self) -> f32 {
        let (_, panel_y, _, _) = self.palette_geometry(self.layout_width, self.layout_height);
        panel_y as f32
    }

    fn rendered_panel_geometry(
        &self,
        width: usize,
        height: usize,
    ) -> Option<(usize, usize, usize, usize)> {
        if !self.open && !self.panel_visible {
            return None;
        }
        let (panel_x, target_panel_y, panel_width, panel_height) =
            self.palette_geometry(width, height);
        let panel_y =
            if self.open && self.panel_render_y <= Self::CLOSED_PANEL_Y && target_panel_y > 0 {
                target_panel_y
            } else {
                self.panel_render_y
                    .round()
                    .clamp(0.0, target_panel_y as f32) as usize
            };
        Some((panel_x, panel_y, panel_width, panel_height))
    }

    fn palette_geometry(&self, width: usize, height: usize) -> (usize, usize, usize, usize) {
        let panel_style = crate::css::resolve_component_style(self, &["command-palette--panel"]);
        let panel_x = 0usize;
        let panel_y = panel_style
            .margin_top
            .map(|value| value as usize)
            .unwrap_or(1)
            .min(height.saturating_sub(1));
        let panel_width = width.max(1);
        let max_panel_height = height.saturating_sub(panel_y).max(1);
        // Account for the CommandList border overhead (blank top + hkey bottom) so the
        // desired height correctly accommodates all entries.
        let list_style = crate::css::resolve_component_style(&self.list, &[]);
        let list_border_overhead = helpers::border_vertical_padding(&list_style);
        let desired_results_height = self
            .provider_results
            .len()
            .saturating_mul(2)
            .saturating_add(1 + list_border_overhead)
            .max(1);
        let results_height =
            desired_results_height.min(max_panel_height.saturating_sub(Self::HEADER_ROWS).max(1));
        let panel_height = Self::HEADER_ROWS
            .saturating_add(results_height)
            .min(max_panel_height)
            .max(1);
        (panel_x, panel_y, panel_width, panel_height)
    }

    fn palette_content_width(panel_width: usize) -> usize {
        panel_width.saturating_sub(2).max(1)
    }

    fn palette_results_geometry(
        &self,
        panel_x: usize,
        panel_y: usize,
        panel_width: usize,
        panel_height: usize,
    ) -> (usize, usize, usize, usize) {
        let content_x = panel_x;
        let content_y = panel_y.saturating_add(Self::RESULTS_ROW_OFFSET);
        let content_width = panel_width.max(1);
        let content_height = panel_height.saturating_sub(Self::RESULTS_ROW_OFFSET).max(1);
        (content_x, content_y, content_width, content_height)
    }

    fn rebuild_results(&mut self, force: bool) -> bool {
        let query = self.query.text().trim().to_string();
        if !force && query == self.last_built_query {
            return false;
        }

        // System provider always contributes (it holds the built-in commands).
        let mut all: Vec<ProviderResult> = self.system_provider.search(&query);

        // External providers only contribute when the palette is open
        // (respects the startup-before-search lifecycle contract).
        if self.open {
            for provider in &self.providers {
                all.extend(provider.search(&query));
            }
        }

        all.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        self.provider_results = all;
        self.last_built_query = query.clone();

        let entries = self
            .provider_results
            .iter()
            .map(|r| CommandListEntry {
                title: r.title.clone(),
                help: r.help.clone(),
                title_highlight_ranges: FuzzyMatcher::highlight_ranges(&query, &r.title),
            })
            .collect::<Vec<_>>();
        self.list.set_entries(entries);
        true
    }

    fn focused_widget_id(widget: &dyn Widget) -> Option<NodeId> {
        if widget.has_focus() {
            return Some(widget.node_id());
        }
        None
    }

    fn restore_child_focus(&mut self) {
        if self.previously_focused_child.take().is_some() {
            self.child.set_focus(true);
        }
    }

    fn set_key_panel_visible(&mut self, visible: bool, ctx: &mut EventCtx) {
        if self.show_key_panel == visible {
            return;
        }
        let before = self
            .key_panel_render_width
            .round()
            .clamp(0.0, self.layout_width as f32) as usize;
        self.show_key_panel = visible;
        let target = if self.show_key_panel {
            self.key_panel_width(self.layout_width)
        } else {
            0
        };
        self.animate_key_panel_width(before, target, ctx);
        ctx.request_repaint();
    }

    fn set_open(&mut self, open: bool, ctx: &mut EventCtx) {
        if self.open == open
            && ((self.open && self.panel_visible) || (!self.open && !self.panel_visible))
        {
            return;
        }
        let was_open = self.open;
        let was_visible = self.panel_visible;
        self.open = open;
        self.update_surface_styles();
        if self.open {
            self.panel_visible = true;
            self.previously_focused_child = Self::focused_widget_id(self.child.as_ref());
            if self.previously_focused_child.is_some() {
                self.child.set_focus(false);
            }
            for provider in &mut self.providers {
                provider.startup();
            }
            self.query.set_text("");
            self.query.set_focus(true);
            self.list.set_visible(true);
            self.list.set_populating(false);
            self.rebuild_results(true);
            let target_y = self.panel_target_y();
            let start_y = if was_visible {
                self.panel_render_y
            } else {
                Self::CLOSED_PANEL_Y
            };
            self.animate_panel_y(start_y, target_y, ctx);
            ctx.post_message(CommandPaletteOpened);
        } else {
            for provider in &mut self.providers {
                provider.shutdown();
            }
            self.query.set_focus(false);
            self.list.set_visible(false);
            self.list.set_populating(false);
            self.restore_child_focus();
            let start_y = self.panel_render_y;
            if was_visible {
                self.animate_panel_y(start_y, Self::CLOSED_PANEL_Y, ctx);
                if !self.panel_visible && self.panel_render_y <= Self::CLOSED_PANEL_Y {
                    self.panel_visible = false;
                }
            }
            if was_open {
                ctx.post_message(CommandPaletteClosed);
            }
        }
        ctx.request_repaint();
    }

    fn execute_selected(&mut self, ctx: &mut EventCtx) {
        if self.provider_results.is_empty() {
            self.set_open(false, ctx);
            return;
        }
        let selected = self.list.selected().min(self.provider_results.len() - 1);
        let result = &self.provider_results[selected];
        ctx.post_message(CommandPaletteCommandSelected {
            id: result.id.clone(),
            title: result.title.clone(),
        });
        match result.id.as_str() {
            "quit" => ctx.request_stop(),
            "keys" => {
                let hide = self.help_panel_visible || self.show_key_panel;
                if hide {
                    ctx.post_message(Message::AppHideHelpPanel(crate::message::AppHideHelpPanel));
                    self.help_panel_visible = false;
                    self.set_key_panel_visible(false, ctx);
                } else {
                    ctx.post_message(Message::AppShowHelpPanel(crate::message::AppShowHelpPanel));
                    self.help_panel_visible = true;
                    self.set_key_panel_visible(true, ctx);
                }
            }
            "theme" => {
                ctx.post_message(Message::AppChangeTheme(crate::message::AppChangeTheme));
            }
            "screenshot" => {
                ctx.post_message(Message::AppScreenshot(crate::message::AppScreenshot {
                    filename: None,
                    path: None,
                }));
            }
            _ => {}
        }
        self.set_open(false, ctx);
    }

    fn is_tree_mode(&self) -> bool {
        self.child_extracted
    }
}

impl Widget for CommandPalette {
    fn style_type(&self) -> &'static str {
        "CommandPalette"
    }

    fn take_composed_children(&mut self) -> Vec<Box<dyn Widget>> {
        if self.child_extracted {
            return Vec::new();
        }
        self.child_extracted = true;
        let child = std::mem::replace(&mut self.child, Box::new(Spacer::new(1)));
        vec![child]
    }

    fn child_display_for_tree(&self, child_index: usize) -> Option<bool> {
        if !self.is_tree_mode() || child_index != 0 {
            return None;
        }
        Some(self.tree_show_wrapped_child)
    }

    fn preserve_underlay(&self) -> bool {
        // Tree-mode command palette behaves like an overlay/modal and should
        // not trigger full-rect background fill from the generic styled path.
        true
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let (width, height) = options.size;
        self.last_render_width
            .store(width.max(1), Ordering::Relaxed);
        self.last_render_height
            .store(height.max(1), Ordering::Relaxed);
        let tree_mode = self.is_tree_mode();

        if tree_mode && !self.open && !self.panel_visible {
            return Segments::new();
        }

        if !tree_mode && !self.open && !self.panel_visible {
            let key_width = self.visible_key_panel_width(width);
            if key_width == 0 {
                return self.child.render_styled(console, options);
            }

            let body_width = width.saturating_sub(key_width).max(1);
            let mut body_options = options.clone();
            body_options.size = (body_width, height);
            body_options.max_width = body_width;
            body_options.max_height = height;

            let mut panel_options = options.clone();
            panel_options.size = (key_width, height);
            panel_options.max_width = key_width;
            panel_options.max_height = height;

            let body_buffer = FrameBuffer::from_renderable(
                console,
                &body_options,
                &WidgetRenderable::new(self.child.as_ref()),
                None,
            );
            let panel_buffer = FrameBuffer::from_renderable(
                console,
                &panel_options,
                &WidgetRenderable::new(&self.key_panel),
                None,
            );

            let mut merged = FrameBuffer::new(width, height, None);
            Overlay::compose_overlay_at(&mut merged, &body_buffer, 0, 0);
            Overlay::compose_overlay_at(&mut merged, &panel_buffer, body_width, 0);
            return merged.to_segments();
        }

        let base = (!tree_mode).then(|| {
            FrameBuffer::from_renderable(
                console,
                options,
                &WidgetRenderable::new(self.child.as_ref()),
                None,
            )
        });
        let mut overlay = FrameBuffer::new(width, height, None);
        let (panel_x, panel_y, panel_width, panel_height) = self
            .rendered_panel_geometry(width, height)
            .unwrap_or_else(|| self.palette_geometry(width, height));
        let panel_style = crate::css::resolve_component_style(self, &["command-palette--panel"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);
        let background_bg = crate::style::parse_color_like("$background")
            .unwrap_or_else(|| crate::style::Color::rgb(0, 0, 0))
            .to_simple_opaque();
        let surface_bg = crate::style::parse_color_like("$surface")
            .unwrap_or_else(|| crate::style::Color::rgb(0, 0, 0))
            .to_simple_opaque();
        let panel_bg = panel_style.bgcolor;
        let apply_panel_surface = |mut cell: Cell| -> Cell {
            let mut composed = match cell.style {
                Some(style) => panel_style.combine(&style),
                None => panel_style,
            };
            let keep_panel_bg = composed.bgcolor.is_none()
                || matches!(composed.bgcolor, Some(rich_rs::SimpleColor::Default))
                || composed.bgcolor == Some(background_bg)
                || composed.bgcolor == Some(surface_bg);
            if keep_panel_bg {
                composed.bgcolor = panel_bg;
            }
            cell.style = Some(composed);
            cell
        };

        for y in panel_y..panel_y.saturating_add(panel_height).min(height) {
            for x in panel_x..panel_x.saturating_add(panel_width).min(width) {
                overlay.set_cell(x, y, Cell::blank(Some(panel_style)));
            }
        }

        let search_width = Self::palette_content_width(panel_width)
            .saturating_sub(3)
            .max(1);
        let mut search_options = options.clone();
        search_options.size = (search_width, 1);
        search_options.max_width = search_width;
        search_options.max_height = 1;
        let search_buffer = FrameBuffer::from_renderable(
            console,
            &search_options,
            &WidgetRenderable::new(&self.query),
            None,
        );
        let mut icon_options = options.clone();
        icon_options.size = (2, 1);
        icon_options.max_width = 2;
        icon_options.max_height = 1;
        let icon_buffer = FrameBuffer::from_renderable(
            console,
            &icon_options,
            &WidgetRenderable::new(&self.search_icon),
            None,
        );

        let search_y = panel_y.saturating_add(Self::SEARCH_ROW_OFFSET);
        let search_icon_x = panel_x.saturating_add(Self::SEARCH_ICON_X_OFFSET);
        if search_y < height {
            for sx in 0..icon_buffer.width.min(2) {
                let tx = search_icon_x.saturating_add(sx);
                if tx >= width {
                    break;
                }
                overlay.set_cell(
                    tx,
                    search_y,
                    apply_panel_surface(icon_buffer.get(sx, 0).clone()),
                );
            }
        }
        if search_y < height {
            for sx in 0..search_buffer.width.min(search_width) {
                let tx = panel_x
                    .saturating_add(Self::SEARCH_TEXT_X_OFFSET)
                    .saturating_add(sx);
                if tx >= width {
                    break;
                }
                overlay.set_cell(
                    tx,
                    search_y,
                    apply_panel_surface(search_buffer.get(sx, 0).clone()),
                );
            }
        }

        let (results_x, results_y, results_w, results_h) =
            self.palette_results_geometry(panel_x, panel_y, panel_width, panel_height);
        let mut results_options = options.clone();
        results_options.size = (results_w.max(1), results_h.max(1));
        results_options.max_width = results_w.max(1);
        results_options.max_height = results_h.max(1);
        let help_style = crate::css::resolve_component_style(self, &["command-palette--help-text"])
            .to_rich_over(
                crate::css::resolve_component_style(self, &["command-palette--panel"])
                    .bg
                    .unwrap_or_else(|| crate::style::Color::rgb(0, 0, 0)),
            );
        let highlight_style =
            crate::css::resolve_component_style(self, &["command-palette--highlight"])
                .to_rich_without_colors();
        self.list.set_help_style_override(help_style);
        self.list.set_highlight_style_override(highlight_style);
        let results_buffer = FrameBuffer::from_renderable(
            console,
            &results_options,
            &WidgetRenderable::new(&self.list),
            None,
        );
        self.list.set_help_style_override(None);
        self.list.set_highlight_style_override(None);
        for y in 0..results_buffer.height.min(results_h) {
            let ty = results_y.saturating_add(y);
            if ty >= height {
                break;
            }
            for x in 0..results_buffer.width.min(results_w) {
                let tx = results_x.saturating_add(x);
                if tx >= width {
                    break;
                }
                overlay.set_cell(
                    tx,
                    ty,
                    apply_panel_surface(results_buffer.get(x, y).clone()),
                );
            }
        }
        for ty in results_y..results_y.saturating_add(results_h).min(height) {
            for tx in results_x..results_x.saturating_add(results_w).min(width) {
                let cell = overlay.get(tx, ty);
                if (cell.text.is_empty() || cell.text == " ")
                    && cell.style.is_none()
                    && cell.meta.is_none()
                {
                    overlay.set_cell(tx, ty, Cell::blank(Some(panel_style)));
                }
            }
        }

        // Ensure search input metadata remains addressable (for click/focus/cursor behavior).
        for sy in 0..search_buffer.height.min(1) {
            let ty = search_y.saturating_add(sy);
            if ty >= height {
                break;
            }
            for sx in 0..search_buffer.width.min(search_width) {
                let tx = panel_x
                    .saturating_add(Self::SEARCH_TEXT_X_OFFSET)
                    .saturating_add(sx);
                if tx >= width {
                    break;
                }
                let cell = search_buffer.get(sx, sy).clone();
                if cell.meta.is_some() {
                    overlay.set_cell(tx, ty, apply_panel_surface(cell));
                }
            }
        }

        if let Some(base) = base {
            Overlay::compose_overlay(&base, &overlay).to_segments()
        } else {
            // Tree-mode overlay path: emit sparse lines only up to the palette
            // region. This preserves the underlay for untouched rows.
            let row_end = panel_y.saturating_add(panel_height).min(height);
            let mut lines: Vec<Vec<rich_rs::Segment>> = Vec::with_capacity(row_end.max(1));
            for y in 0..row_end {
                if y < panel_y {
                    lines.push(Vec::new());
                    continue;
                }
                let mut line: Vec<rich_rs::Segment> = Vec::new();
                for x in 0..width {
                    let cell = overlay.get(x, y);
                    if cell.continuation {
                        continue;
                    }
                    let paints = (!cell.text.is_empty() && cell.text != " ")
                        || cell.style.is_some()
                        || cell.meta.is_some();
                    if !paints {
                        continue;
                    }
                    let mut seg = rich_rs::Segment::new(cell.text.clone());
                    seg.style = cell.style;
                    // Tree mode renders nested internal widgets via `WidgetRenderable`,
                    // which can carry `NodeId::default()` metadata. Keep tree hit-testing
                    // on the CommandPalette node itself so mouse routing consistently
                    // runs through CommandPalette geometry mapping.
                    seg.meta = None;
                    line.push(seg);
                }
                lines.push(line);
            }

            let mut out = Segments::new();
            for (idx, line) in lines.into_iter().enumerate() {
                out.extend(line);
                if idx + 1 < row_end {
                    out.push(rich_rs::Segment::line());
                }
            }
            out
        }
    }

    fn on_mount(&mut self) {
        self.update_surface_styles();
        if !self.is_tree_mode() {
            self.child.on_mount();
        }
        self.query.on_mount();
        self.list.on_mount();
        self.key_panel.on_mount();
    }

    fn on_unmount(&mut self) {
        if !self.is_tree_mode() {
            self.child.on_unmount();
        }
        self.query.on_unmount();
        self.list.on_unmount();
        self.key_panel.on_unmount();
        if self.open {
            for provider in &mut self.providers {
                provider.shutdown();
            }
        }
        self.open = false;
        self.panel_visible = false;
        self.panel_render_y = Self::CLOSED_PANEL_Y;
        self.query.set_focus(false);
        self.list.set_focus(false);
        self.previously_focused_child = None;
    }

    fn on_tick(&mut self, tick: u64) {
        if !self.is_tree_mode() {
            self.child.on_tick(tick);
        }
        self.key_panel.on_tick(tick);
        if self.open {
            self.query.on_tick(tick);
            self.list.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        let total_width = usize::from(width);
        let total_height = usize::from(height);
        self.layout_width = total_width.max(1);
        self.layout_height = total_height.max(1);
        self.update_surface_styles();
        let panel_target_y = self.panel_target_y();
        if self.open {
            self.panel_visible = true;
            if self.panel_render_y <= Self::CLOSED_PANEL_Y && panel_target_y > Self::CLOSED_PANEL_Y
            {
                self.panel_render_y = panel_target_y;
            }
        } else if !self.panel_visible {
            self.panel_render_y = Self::CLOSED_PANEL_Y;
        }
        let key_width = self.visible_key_panel_width(total_width);
        let child_width = total_width.saturating_sub(key_width).max(1) as u16;
        if !self.is_tree_mode() {
            self.child.on_resize(child_width, height);
        }
        if key_width > 0 {
            self.key_panel.on_resize(key_width as u16, height);
        }

        let (_x, _y, panel_w, panel_h) = self.palette_geometry(total_width, total_height);
        let query_width = Self::palette_content_width(panel_w)
            .saturating_sub(3)
            .max(1);
        self.query.on_resize(query_width as u16, 1);
        let (_, _, results_w, results_h) = self.palette_results_geometry(0, 0, panel_w, panel_h);
        self.list
            .on_resize(results_w.max(1) as u16, results_h.max(1) as u16);
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        let total_width = usize::from(width);
        let total_height = usize::from(height);
        self.layout_width = total_width.max(1);
        self.layout_height = total_height.max(1);
        self.update_surface_styles();
        let panel_target_y = self.panel_target_y();
        if self.open {
            self.panel_visible = true;
            if self.panel_render_y <= Self::CLOSED_PANEL_Y && panel_target_y > Self::CLOSED_PANEL_Y
            {
                self.panel_render_y = panel_target_y;
            }
        } else if !self.panel_visible {
            self.panel_render_y = Self::CLOSED_PANEL_Y;
        }
        let key_width = self.visible_key_panel_width(total_width);
        let child_width = total_width.saturating_sub(key_width).max(1) as u16;
        if !self.is_tree_mode() {
            self.child.on_layout(child_width, height);
        }
        if key_width > 0 {
            self.key_panel.on_layout(key_width as u16, height);
        }

        let (_x, _y, panel_w, panel_h) = self.palette_geometry(total_width, total_height);
        let query_width = Self::palette_content_width(panel_w)
            .saturating_sub(3)
            .max(1);
        self.query.on_layout(query_width as u16, 1);
        let (_, _, results_w, results_h) = self.palette_results_geometry(0, 0, panel_w, panel_h);
        self.list
            .on_layout(results_w.max(1) as u16, results_h.max(1) as u16);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        if matches!(event, Event::AppFocus(..)) {
            if !self.is_tree_mode() {
                self.child.on_event_capture(event, ctx);
            }
            self.query.on_event_capture(event, ctx);
            self.list.on_event_capture(event, ctx);
            return;
        }
        if self.open {
            self.query.on_event_capture(event, ctx);
            if !ctx.handled() {
                self.list.on_event_capture(event, ctx);
            }
        } else if !self.is_tree_mode() {
            self.child.on_event_capture(event, ctx);
        }
    }

    fn action_namespace(&self) -> &str {
        "command-palette"
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        if !self.open {
            return Vec::new();
        }
        vec![
            BindingDecl::new("escape", "dismiss", "Dismiss command palette"),
            BindingDecl::new(
                "enter",
                "command_list.select_cursor",
                "Execute selected command",
            ),
        ]
    }

    fn execute_action(&mut self, action: &ParsedAction, ctx: &mut EventCtx) -> bool {
        match action.name.as_str() {
            "dismiss" => {
                self.set_open(false, ctx);
                ctx.set_handled();
                true
            }
            _ => false,
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Event::AnimationValue(AnimationValueEvent {
            target,
            attribute,
            value,
            done,
        }) = event
        {
            if *target == self.node_id() {
                if attribute == Self::KEY_PANEL_WIDTH_ATTR {
                    self.key_panel_render_width = (*value).max(0.0);
                    if *done && !self.show_key_panel {
                        self.key_panel_render_width = 0.0;
                    }
                    ctx.request_repaint();
                    ctx.set_handled();
                    return;
                }
                if attribute == Self::PANEL_Y_ATTR {
                    self.panel_render_y = (*value).max(Self::CLOSED_PANEL_Y);
                    if *done && !self.open && self.panel_render_y <= Self::CLOSED_PANEL_Y {
                        self.panel_visible = false;
                    } else if self.open {
                        self.panel_visible = true;
                    }
                    ctx.request_repaint();
                    ctx.set_handled();
                    return;
                }
            }
        }
        if matches!(event, Event::BindingsChanged(_)) {
            self.key_panel.on_event(event, ctx);
        }
        if let Event::AppFocus(active) = event {
            self.query.on_event(event, ctx);
            self.list.on_event(event, ctx);
            if !self.is_tree_mode() {
                self.child.on_event(event, ctx);
            }
            if self.open && !*active {
                self.set_open(false, ctx);
            }
            return;
        }

        if let Event::Action(Action::CommandPalette) = event {
            self.set_open(!self.open, ctx);
            ctx.set_handled();
            return;
        }

        if !self.open && self.panel_visible {
            if matches!(
                event,
                Event::MouseDown(_)
                    | Event::MouseUp(_)
                    | Event::MouseScroll(_)
                    | Event::Key(_)
                    | Event::Action(_)
            ) {
                ctx.set_handled();
                return;
            }
        }

        if !self.open {
            if self.show_key_panel {
                match event {
                    Event::MouseDown(mouse) if mouse.target == self.node_id() => {
                        self.key_panel.on_event(event, ctx);
                        if ctx.handled() {
                            return;
                        }
                    }
                    Event::MouseUp(mouse) if mouse.target.is_some_and(|t| t == self.node_id()) => {
                        self.key_panel.on_event(event, ctx);
                        if ctx.handled() {
                            return;
                        }
                    }
                    Event::MouseScroll(mouse)
                        if mouse.target.is_some_and(|t| t == self.node_id()) =>
                    {
                        self.key_panel.on_event(event, ctx);
                        if ctx.handled() {
                            return;
                        }
                    }
                    _ => {}
                }
            }
            if !self.is_tree_mode() {
                self.child.on_event(event, ctx);
            }
            return;
        }

        if let Event::Key(key) = event {
            if key.code == crossterm::event::KeyCode::Esc {
                self.set_open(false, ctx);
                ctx.set_handled();
                return;
            }
            match key.code {
                crossterm::event::KeyCode::Up => {
                    self.list
                        .set_selected(self.list.selected().saturating_sub(1));
                    ctx.request_repaint();
                    ctx.set_handled();
                    return;
                }
                crossterm::event::KeyCode::Down => {
                    if !self.provider_results.is_empty() {
                        let next = self
                            .list
                            .selected()
                            .saturating_add(1)
                            .min(self.provider_results.len().saturating_sub(1));
                        self.list.set_selected(next);
                        ctx.request_repaint();
                    }
                    ctx.set_handled();
                    return;
                }
                crossterm::event::KeyCode::Home => {
                    self.list.set_selected(0);
                    ctx.request_repaint();
                    ctx.set_handled();
                    return;
                }
                crossterm::event::KeyCode::End => {
                    if !self.provider_results.is_empty() {
                        self.list
                            .set_selected(self.provider_results.len().saturating_sub(1));
                        ctx.request_repaint();
                    }
                    ctx.set_handled();
                    return;
                }
                crossterm::event::KeyCode::PageUp => {
                    let next = self.list.selected().saturating_sub(5);
                    self.list.set_selected(next);
                    ctx.request_repaint();
                    ctx.set_handled();
                    return;
                }
                crossterm::event::KeyCode::PageDown => {
                    if !self.provider_results.is_empty() {
                        let next = self
                            .list
                            .selected()
                            .saturating_add(5)
                            .min(self.provider_results.len().saturating_sub(1));
                        self.list.set_selected(next);
                        ctx.request_repaint();
                    }
                    ctx.set_handled();
                    return;
                }
                _ => {}
            }
            if key.code == crossterm::event::KeyCode::Enter {
                self.execute_selected(ctx);
                ctx.set_handled();
                return;
            }
        }

        if let Event::MouseDown(mouse) = event {
            let (panel_x, target_panel_y, panel_w, panel_h) =
                self.palette_geometry(self.layout_width, self.layout_height);
            let panel_y =
                self.panel_render_y
                    .round()
                    .clamp(Self::CLOSED_PANEL_Y, target_panel_y as f32) as usize;
            // MouseDown coordinates are relative to the event target widget.
            // Use screen coordinates so panel hit-testing remains correct when
            // bubbling from children (e.g. search input) and during panel animation.
            let (x, y) = if mouse.target == self.node_id() {
                (mouse.x as usize, mouse.y as usize)
            } else {
                (mouse.screen_x as usize, mouse.screen_y as usize)
            };
            let inside_panel = x >= panel_x
                && x < panel_x.saturating_add(panel_w)
                && y >= panel_y
                && y < panel_y.saturating_add(panel_h);

            if mouse.target != self.node_id() && !inside_panel {
                self.set_open(false, ctx);
                ctx.set_handled();
                return;
            }

            let (results_x, results_y, results_w, results_h) =
                self.palette_results_geometry(panel_x, panel_y, panel_w, panel_h);
            if x >= results_x
                && x < results_x.saturating_add(results_w)
                && y >= results_y
                && y < results_y.saturating_add(results_h)
            {
                let row = y.saturating_sub(results_y) / 2;
                let visible_items = (results_h / 2).max(1);
                let start = self
                    .list
                    .offset()
                    .min(self.provider_results.len().saturating_sub(visible_items));
                let index = start.saturating_add(row);
                if index < self.provider_results.len() {
                    self.list.set_selected(index);
                    self.execute_selected(ctx);
                    ctx.set_handled();
                    return;
                }
                ctx.set_handled();
                return;
            }

            if mouse.target != self.node_id() {
                ctx.set_handled();
                return;
            }
        }

        self.query.on_event(event, ctx);
        if !ctx.handled() {
            self.list.on_event(event, ctx);
        }
        if !ctx.handled() {
            ctx.set_handled();
        }
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        if self.open
            && (message.is::<OverlaySetVisible>()
                || message.is::<OverlayToggle>()
                || message.is::<OverlayDismissRequested>()
                || message.is::<OverlayVisibilityChanged>())
        {
            self.set_open(false, ctx);
        }
        self.query.on_message(message, ctx);
        self.list.on_message(message, ctx);
        self.key_panel.on_message(message, ctx);
        if matches!(message.message, Message::AppShowHelpPanel(_)) {
            self.help_panel_visible = true;
            self.set_key_panel_visible(true, ctx);
        } else if matches!(message.message, Message::AppHideHelpPanel(_)) {
            self.help_panel_visible = false;
            self.set_key_panel_visible(false, ctx);
        }
        if let Some(CommandPaletteSetCommands { commands }) =
            message.downcast_ref::<CommandPaletteSetCommands>()
        {
            let next = commands
                .iter()
                .map(|command| PaletteCommand {
                    id: command.id.clone(),
                    title: command.title.clone(),
                    help: command.help.clone(),
                })
                .collect::<Vec<_>>();
            self.set_commands(next);
            ctx.request_repaint();
            ctx.set_handled();
            return;
        }
        if self.open && message.is::<InputChanged>() {
            // In tree mode, InputChanged sender may be the focused branch node
            // rather than the inline query widget node. The open palette owns
            // query updates, so rebuild on any InputChanged while open.
            if self.rebuild_results(false) {
                ctx.request_repaint();
            }
            ctx.set_handled();
            return;
        }
        if !self.is_tree_mode() {
            self.child.on_message(message, ctx);
        }
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        if !self.open {
            return false;
        }

        let px = x as usize;
        let py = y as usize;
        let render_w = self
            .last_render_width
            .load(Ordering::Relaxed)
            .max(self.layout_width)
            .max(1);
        let render_h = self
            .last_render_height
            .load(Ordering::Relaxed)
            .max(self.layout_height)
            .max(1);
        let (_, target_panel_y, panel_w, panel_h) = self.palette_geometry(render_w, render_h);
        let panel_y =
            if self.open && self.panel_render_y <= Self::CLOSED_PANEL_Y && target_panel_y > 0 {
                target_panel_y
            } else {
                self.panel_render_y
                    .round()
                    .clamp(Self::CLOSED_PANEL_Y, target_panel_y as f32) as usize
            };
        let (results_x, results_y, results_w, results_h) =
            self.palette_results_geometry(0, panel_y, panel_w, panel_h);

        let inside_results = px >= results_x
            && px < results_x.saturating_add(results_w)
            && py >= results_y
            && py < results_y.saturating_add(results_h);

        if inside_results {
            let local_x = px.saturating_sub(results_x) as u16;
            let local_y = py.saturating_sub(results_y) as u16;
            let changed = self.list.on_mouse_move(local_x, local_y);
            return changed;
        }

        // Clear stale row hover when leaving the results area.
        let changed = self.list.on_mouse_move(0, u16::MAX);
        changed
    }

    fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut EventCtx) {
        if self.open {
            self.list.on_mouse_scroll(delta_x, delta_y, ctx);
            if !ctx.handled() {
                ctx.set_handled();
            }
        } else {
            if self.show_key_panel {
                self.key_panel.on_mouse_scroll(delta_x, delta_y, ctx);
                if ctx.handled() {
                    return;
                }
            }
            if !self.is_tree_mode() {
                self.child.on_mouse_scroll(delta_x, delta_y, ctx);
            }
        }
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for CommandPalette {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
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

// ---------------------------------------------------------------------------
// CommandPaletteScreen — Screen wrapper for CommandPalette
// ---------------------------------------------------------------------------

/// A screen that displays the command palette as a full-screen modal overlay.
///
/// This wraps `CommandPalette` as a `Screen` so it can be pushed onto the
/// screen stack via `App::push_screen()`. The palette is automatically opened
/// when the screen is mounted.
///
/// Implements both `Screen` and `SystemModalScreen` (style-isolated modal).
pub struct CommandPaletteScreen {
    commands: Vec<PaletteCommand>,
}

impl CommandPaletteScreen {
    /// Create a new command palette screen with default commands.
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    /// Create a command palette screen with the given commands.
    pub fn with_commands(commands: Vec<PaletteCommand>) -> Self {
        Self { commands }
    }
}

impl Default for CommandPaletteScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::screen::Screen for CommandPaletteScreen {
    fn name(&self) -> &str {
        "CommandPaletteScreen"
    }

    fn compose(&self) -> Box<dyn Widget> {
        // Use a blank Label as the child — when the palette opens as a screen
        // the underlying content is the screen below in the stack.
        let mut palette = CommandPalette::new(super::Label::new(""));
        if !self.commands.is_empty() {
            palette.set_commands(self.commands.clone());
        }
        // Auto-open: the palette opens immediately when composed as a screen.
        let mut ctx = crate::event::EventCtx::default();
        palette.set_open(true, &mut ctx);
        Box::new(palette)
    }

    fn is_modal(&self) -> bool {
        true
    }
}

impl SystemModalScreen for CommandPaletteScreen {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css::{StyleSheet, set_style_context};
    use crate::event::{Action, Event, EventCtx};
    use crate::message::{CommandPaletteCommand, Message};
    use crate::node_id::NodeId;
    use crate::render::FrameBuffer;
    use crate::widgets::Label;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use rich_rs::Console;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    struct FocusProbe {
        focused: Arc<AtomicBool>,
    }

    impl FocusProbe {
        fn new(focused: Arc<AtomicBool>) -> Self {
            Self { focused }
        }
    }

    impl Widget for FocusProbe {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn focusable(&self) -> bool {
            true
        }

        fn set_focus(&mut self, focused: bool) {
            self.focused.store(focused, Ordering::Relaxed);
        }

        fn has_focus(&self) -> bool {
            self.focused.load(Ordering::Relaxed)
        }
    }

    struct EventProbe {
        mouse_downs: Arc<AtomicUsize>,
    }

    impl EventProbe {
        fn new(mouse_downs: Arc<AtomicUsize>) -> Self {
            Self { mouse_downs }
        }
    }

    impl Widget for EventProbe {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn on_event(&mut self, event: &Event, _ctx: &mut EventCtx) {
            if matches!(event, Event::MouseDown(_)) {
                self.mouse_downs.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    #[test]
    fn command_palette_toggles_from_action() {
        let mut palette = CommandPalette::new(Label::new("body"));
        let mut ctx = EventCtx::default();

        palette.on_event(&Event::Action(Action::CommandPalette), &mut ctx);
        assert!(palette.is_open());
        assert!(ctx.handled());
    }

    #[test]
    fn command_palette_emits_selection_message_on_enter() {
        let mut palette = CommandPalette::new(Label::new("body"));
        let mut ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut ctx);

        let down = crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
            KeyCode::Down,
            KeyModifiers::NONE,
        ));
        let mut nav_ctx = EventCtx::default();
        palette.on_event(&Event::Key(down), &mut nav_ctx);

        let key = crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ));
        let mut execute_ctx = EventCtx::default();
        palette.on_event(&Event::Key(key), &mut execute_ctx);

        let messages = execute_ctx.take_messages();
        assert!(messages.iter().any(|event| event.is::<CommandPaletteCommandSelected>()));
        assert!(!palette.is_open());
    }

    #[test]
    fn command_palette_down_key_moves_selection_while_input_has_focus() {
        let mut palette = CommandPalette::new(Label::new("body"));
        let mut ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut ctx);
        assert!(palette.is_open());
        assert_eq!(palette.list.selected(), 0);

        let down = crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
            KeyCode::Down,
            KeyModifiers::NONE,
        ));
        let mut nav_ctx = EventCtx::default();
        palette.on_event(&Event::Key(down), &mut nav_ctx);
        assert!(nav_ctx.handled());
        assert_eq!(palette.list.selected(), 1);
    }

    #[test]
    fn command_palette_input_changed_rebuilds_results_even_with_non_query_sender() {
        let mut palette = CommandPalette::new(Label::new("body"));
        let mut open_ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut open_ctx);
        assert!(palette.is_open());
        assert!(!palette.provider_results.is_empty());

        palette.query.set_text("zzzzzzzz");
        let mut msg_ctx = EventCtx::default();
        palette.on_message(
            &MessageEvent::new(
                crate::node_id::node_id_from_ffi(77),
                InputChanged {
                    value: "zzzzzzzz".to_string(),
                    validation: crate::validation::ValidationResult::success(),
                },
            ),
            &mut msg_ctx,
        );
        assert!(msg_ctx.handled());
        assert!(palette.provider_results.is_empty());
    }

    #[test]
    fn command_palette_emits_selection_message_for_keys_builtin() {
        let mut palette = CommandPalette::new(Label::new("body"));
        let mut ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut ctx);

        let enter = crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ));
        let mut execute_ctx = EventCtx::default();
        palette.on_event(&Event::Key(enter), &mut execute_ctx);

        let messages = execute_ctx.take_messages();
        assert!(
            messages
                .iter()
                .any(|event| matches!(event.message, Message::AppShowHelpPanel(_)))
        );
        assert!(messages.iter().any(|event| {
            event.downcast_ref::<CommandPaletteCommandSelected>().is_some_and(|m| m.id == "keys")
        }));
        assert!(messages.iter().any(|event| event.is::<CommandPaletteClosed>()));
        assert!(!palette.is_open());
    }

    #[test]
    fn command_palette_quit_builtin_emits_selection_and_requests_stop() {
        let mut palette = CommandPalette::new(Label::new("body"));
        palette.set_commands(vec![PaletteCommand::new("quit", "Quit", "Quit app")]);

        let mut open_ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut open_ctx);
        assert!(palette.is_open());

        let enter = crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ));
        let mut execute_ctx = EventCtx::default();
        palette.on_event(&Event::Key(enter), &mut execute_ctx);

        assert!(execute_ctx.stop_requested());
        let messages = execute_ctx.take_messages();
        let selected_idx = messages.iter().position(|event| {
            event.downcast_ref::<CommandPaletteCommandSelected>().is_some_and(|m| m.id == "quit")
        });
        let close_idx = messages
            .iter()
            .position(|event| event.is::<CommandPaletteClosed>());
        assert!(selected_idx.is_some());
        assert!(close_idx.is_some());
        assert!(selected_idx < close_idx);
        assert!(!palette.is_open());
    }

    #[test]
    fn command_palette_keys_command_toggles_key_panel() {
        let mut palette = CommandPalette::new(Label::new("body"));
        let mut ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut ctx);
        assert!(palette.is_open());

        let enter = crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ));
        let mut execute_ctx = EventCtx::default();
        palette.on_event(&Event::Key(enter), &mut execute_ctx);

        assert!(!palette.is_open());
        assert!(palette.show_key_panel);
    }

    #[test]
    fn command_palette_set_commands_message_replaces_command_list() {
        let mut palette = CommandPalette::new(Label::new("body"));
        let mut ctx = EventCtx::default();
        palette.on_message(
            &MessageEvent::new(
                NodeId::default(),
                CommandPaletteSetCommands {
                    commands: vec![CommandPaletteCommand {
                        id: "deploy".to_string(),
                        title: "Deploy".to_string(),
                        help: "Ship current build".to_string(),
                    }],
                },
            ),
            &mut ctx,
        );
        assert!(ctx.handled());
        assert!(ctx.repaint_requested());
        assert_eq!(palette.system_provider.commands().len(), 1);
        assert_eq!(palette.system_provider.commands()[0].id, "deploy");
    }

    #[test]
    fn command_palette_keys_command_emits_key_panel_animation_request() {
        let _guard = set_style_context(StyleSheet::parse(
            "CommandPalette > .command-palette--key-panel { transition: command-palette.key-panel 220ms ease-out; }",
        ));
        let mut palette = CommandPalette::new(Label::new("body"));
        palette.on_layout(80, 20);
        let mut ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut ctx);
        assert!(palette.is_open());

        let enter = crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ));
        let mut execute_ctx = EventCtx::default();
        palette.on_event(&Event::Key(enter), &mut execute_ctx);

        let requests = execute_ctx.take_animation_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].attribute, CommandPalette::KEY_PANEL_WIDTH_ATTR);
        assert!(requests[0].end > requests[0].start);
    }

    #[test]
    fn command_palette_keys_command_hides_key_panel_on_second_toggle() {
        let _guard = set_style_context(StyleSheet::parse(
            "CommandPalette > .command-palette--key-panel { transition: command-palette.key-panel 220ms ease-out; }",
        ));
        let mut palette = CommandPalette::new(Label::new("body"));
        palette.on_layout(80, 20);
        let mut ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut ctx);
        assert!(palette.is_open());

        let enter = crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ));

        let mut first_ctx = EventCtx::default();
        palette.on_event(&Event::Key(enter.clone()), &mut first_ctx);
        assert!(!palette.is_open());
        assert!(palette.show_key_panel);
        let first = first_ctx.take_animation_requests();
        assert_eq!(first.len(), 1);
        assert!(first[0].end > 0.0);
        let mut settle_ctx = EventCtx::default();
        palette.on_event(
            &Event::AnimationValue(AnimationValueEvent {
                target: NodeId::default(),
                attribute: CommandPalette::KEY_PANEL_WIDTH_ATTR.to_string(),
                value: first[0].end,
                done: true,
            }),
            &mut settle_ctx,
        );

        let mut reopen_ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut reopen_ctx);
        assert!(palette.is_open());

        let mut second_ctx = EventCtx::default();
        palette.on_event(&Event::Key(enter), &mut second_ctx);
        assert!(!palette.is_open());
        assert!(!palette.show_key_panel);
        let second_messages = second_ctx.take_messages();
        assert!(
            second_messages
                .iter()
                .any(|event| matches!(event.message, Message::AppHideHelpPanel(_))),
            "second keys invocation should emit hide-help message"
        );
        let second = second_ctx.take_animation_requests();
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].attribute, CommandPalette::KEY_PANEL_WIDTH_ATTR);
        assert_eq!(second[0].end, 0.0);
    }

    #[test]
    fn command_palette_open_close_emits_panel_animation_requests() {
        let _guard = set_style_context(StyleSheet::parse(
            "CommandPalette > .command-palette--panel { transition: command-palette.panel-y 180ms ease-out; }",
        ));
        let mut palette = CommandPalette::new(Label::new("body"));
        palette.on_layout(80, 20);

        let mut open_ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut open_ctx);
        assert!(palette.is_open());
        let open_requests = open_ctx.take_animation_requests();
        assert_eq!(open_requests.len(), 1);
        assert_eq!(open_requests[0].attribute, CommandPalette::PANEL_Y_ATTR);
        assert!(open_requests[0].end > open_requests[0].start);
        let mut settle_ctx = EventCtx::default();
        palette.on_event(
            &Event::AnimationValue(AnimationValueEvent {
                target: NodeId::default(),
                attribute: CommandPalette::PANEL_Y_ATTR.to_string(),
                value: open_requests[0].end,
                done: true,
            }),
            &mut settle_ctx,
        );

        let mut close_ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut close_ctx);
        assert!(!palette.is_open());
        let close_requests = close_ctx.take_animation_requests();
        assert_eq!(close_requests.len(), 1);
        assert_eq!(close_requests[0].attribute, CommandPalette::PANEL_Y_ATTR);
        assert!(close_requests[0].end <= close_requests[0].start);
    }

    #[test]
    fn command_palette_layout_passes_do_not_force_panel_y_downward() {
        let _guard = set_style_context(crate::css::default_widget_stylesheet());
        let mut palette = CommandPalette::new(Label::new("body"));
        palette.on_layout(80, 20);

        let mut open_ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut open_ctx);
        assert!(palette.is_open());
        assert_eq!(palette.panel_render_y, 3.0);

        // Simulate a transient constrained layout pass (smaller height) after open.
        // Runtime render still uses viewport-space geometry, so this pass must not
        // overwrite panel animation state and pin the panel too high.
        palette.on_layout(80, 2);
        assert_eq!(palette.panel_render_y, 3.0);
    }

    #[test]
    fn command_palette_restores_child_focus_on_close() {
        // This test verifies palette open/close lifecycle and focus-tracking
        // state. Full focus delegation tests require tree-based focus management.
        let child_focus = Arc::new(AtomicBool::new(true));
        let child = FocusProbe::new(child_focus.clone());
        let mut palette = CommandPalette::new(child);

        let mut open_ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut open_ctx);
        assert!(palette.is_open());
        // focused_widget_id detects the focused child and records it.
        assert_eq!(palette.previously_focused_child, Some(NodeId::default()));

        let mut close_ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut close_ctx);
        assert!(!palette.is_open());
        // previously_focused_child is consumed by restore_child_focus.
        assert!(palette.previously_focused_child.is_none());
    }

    #[test]
    fn command_palette_closes_on_overlay_visibility_change_message() {
        let mut palette = CommandPalette::new(Label::new("body"));
        let mut open_ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut open_ctx);
        assert!(palette.is_open());

        let mut transition_ctx = EventCtx::default();
        palette.on_message(
            &MessageEvent::new(
                NodeId::default(),
                OverlayVisibilityChanged {
                    overlay: NodeId::default(),
                    visible: true,
                },
            ),
            &mut transition_ctx,
        );
        assert!(!palette.is_open());
        let messages = transition_ctx.take_messages();
        assert!(messages.iter().any(|event| event.is::<CommandPaletteClosed>()));
    }

    #[test]
    fn command_palette_closes_on_app_focus_loss() {
        let mut palette = CommandPalette::new(Label::new("body"));
        let mut open_ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut open_ctx);
        assert!(palette.is_open());

        let mut focus_ctx = EventCtx::default();
        palette.on_event(&Event::AppFocus(false), &mut focus_ctx);
        assert!(!palette.is_open());
        let messages = focus_ctx.take_messages();
        assert!(messages.iter().any(|event| event.is::<CommandPaletteClosed>()));
    }

    #[test]
    fn command_palette_selection_message_emits_before_close_message() {
        let mut palette = CommandPalette::new(Label::new("body"));
        let mut open_ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut open_ctx);
        assert!(palette.is_open());

        let down = crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
            KeyCode::Down,
            KeyModifiers::NONE,
        ));
        let mut nav_ctx = EventCtx::default();
        palette.on_event(&Event::Key(down), &mut nav_ctx);

        let enter = crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        ));
        let mut execute_ctx = EventCtx::default();
        palette.on_event(&Event::Key(enter), &mut execute_ctx);
        let messages = execute_ctx.take_messages();
        let selected_idx = messages
            .iter()
            .position(|event| event.is::<CommandPaletteCommandSelected>());
        let close_idx = messages
            .iter()
            .position(|event| event.is::<CommandPaletteClosed>());

        assert!(selected_idx.is_some());
        assert!(close_idx.is_some());
        assert!(selected_idx < close_idx);
    }

    #[test]
    fn command_palette_keeps_open_when_search_input_receives_click() {
        let mut palette = CommandPalette::new(Label::new("body"));
        palette.on_layout(80, 20);

        let mut open_ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut open_ctx);
        assert!(palette.is_open());

        let mut click_ctx = EventCtx::default();
        palette.on_event(
            &Event::MouseDown(crate::event::MouseDownEvent {
                target: NodeId::default(), // matches self.node_id() in tests (no dispatch context)
                screen_x: 5,
                screen_y: 2,
                x: 0,
                y: 0,
            }),
            &mut click_ctx,
        );

        assert!(palette.is_open());
        assert!(click_ctx.handled());
    }

    #[test]
    fn command_palette_query_click_with_local_coordinates_keeps_palette_open() {
        let mut palette = CommandPalette::new(Label::new("body"));
        palette.on_layout(80, 20);

        let mut open_ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut open_ctx);
        assert!(palette.is_open());

        let mut click_ctx = EventCtx::default();
        palette.on_event(
            &Event::MouseDown(crate::event::MouseDownEvent {
                target: NodeId::default(), // matches self.node_id() in tests (no dispatch context)
                screen_x: 0,
                screen_y: 0,
                x: 2,
                y: 0,
            }),
            &mut click_ctx,
        );

        assert!(palette.is_open());
        assert!(click_ctx.handled());
    }

    #[test]
    fn command_palette_row_click_selects_when_target_is_self() {
        let mut palette = CommandPalette::new(Label::new("body"));
        palette.on_layout(80, 20);
        let mut open_ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut open_ctx);
        assert!(palette.is_open());

        // row 0 in results block with default panel geometry:
        // panel_y=3, results_y=6, each entry consumes two rows.
        let mut click_ctx = EventCtx::default();
        palette.on_event(
            &Event::MouseDown(crate::event::MouseDownEvent {
                target: NodeId::default(),
                screen_x: 4,
                screen_y: 6,
                x: 4,
                y: 6,
            }),
            &mut click_ctx,
        );
        assert!(click_ctx.handled());
        assert!(!palette.is_open());
        let messages = click_ctx.take_messages();
        assert!(messages.iter().any(|event| event.is::<CommandPaletteCommandSelected>()));
    }

    #[test]
    fn command_list_mouse_down_on_help_row_selects_same_command() {
        let mut list = CommandList::new();
        list.set_entries(vec![
            CommandListEntry {
                title: "One".to_string(),
                help: "First".to_string(),
                title_highlight_ranges: Vec::new(),
            },
            CommandListEntry {
                title: "Two".to_string(),
                help: "Second".to_string(),
                title_highlight_ranges: Vec::new(),
            },
        ]);
        list.on_layout(40, 8);
        list.set_selected(0);

        let mut ctx = EventCtx::default();
        list.on_event(
            &Event::MouseDown(crate::event::MouseDownEvent {
                target: list.node_id(),
                screen_x: 2,
                screen_y: 3,
                x: 2,
                // Help row for the first command in two-row rendering.
                y: 1,
            }),
            &mut ctx,
        );

        assert!(ctx.handled());
        assert_eq!(list.selected(), 0);
    }

    #[test]
    fn command_list_mouse_move_keeps_keyboard_selection() {
        let mut list = CommandList::new();
        list.set_entries(vec![
            CommandListEntry {
                title: "One".to_string(),
                help: "First".to_string(),
                title_highlight_ranges: Vec::new(),
            },
            CommandListEntry {
                title: "Two".to_string(),
                help: "Second".to_string(),
                title_highlight_ranges: Vec::new(),
            },
            CommandListEntry {
                title: "Three".to_string(),
                help: "Third".to_string(),
                title_highlight_ranges: Vec::new(),
            },
        ]);
        list.on_layout(40, 8);
        list.set_selected(0);

        // y=4 corresponds to row 2 (third command title) in two-row layout.
        assert!(list.on_mouse_move(0, 4));
        assert_eq!(
            list.selected(),
            0,
            "mouse hover should not overwrite keyboard selection"
        );
    }

    #[test]
    fn command_list_selected_row_styles_title_and_help_rows() {
        let _guard = set_style_context(crate::css::default_widget_stylesheet());
        let mut list = CommandList::new();
        list.set_entries(vec![CommandListEntry {
            title: "Keys".to_string(),
            help: "Show help text".to_string(),
            title_highlight_ranges: Vec::new(),
        }]);
        list.on_layout(60, 4);
        list.set_selected(0);

        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (60, 4);
        options.max_width = 60;
        options.max_height = 4;
        let buf =
            FrameBuffer::from_renderable(&console, &options, &WidgetRenderable::new(&list), None);

        // Row 0 is the blank top border (border-top: blank from default CommandList CSS).
        // Entry 0 starts at row 1 (title) and row 2 (help).
        let title_bg = buf.get(0, 1).style.as_ref().and_then(|style| style.bgcolor);
        let help_bg = buf.get(0, 2).style.as_ref().and_then(|style| style.bgcolor);
        assert_eq!(title_bg, help_bg);
        assert!(
            title_bg.is_some(),
            "selected rows should paint a full-width background"
        );
    }

    #[test]
    fn command_list_hover_row_styles_title_and_help_rows() {
        let _guard = set_style_context(crate::css::default_widget_stylesheet());
        let mut list = CommandList::new();
        list.set_entries(vec![
            CommandListEntry {
                title: "Keys".to_string(),
                help: "Show help text".to_string(),
                title_highlight_ranges: Vec::new(),
            },
            CommandListEntry {
                title: "Maximize".to_string(),
                help: "Maximize focused widget".to_string(),
                title_highlight_ranges: Vec::new(),
            },
        ]);
        list.on_layout(60, 8);
        list.set_selected(0);
        assert!(list.on_mouse_move(0, 2));

        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (60, 8);
        options.max_width = 60;
        options.max_height = 8;
        let buf =
            FrameBuffer::from_renderable(&console, &options, &WidgetRenderable::new(&list), None);

        // Row 0 is the blank top border (border-top: blank from default CommandList CSS).
        // Entry 0 (selected) starts at row 1, entry 1 (hovered) starts at row 3.
        let selected_bg = buf.get(0, 1).style.as_ref().and_then(|style| style.bgcolor);
        let hover_title_bg = buf.get(0, 3).style.as_ref().and_then(|style| style.bgcolor);
        let hover_help_bg = buf.get(0, 4).style.as_ref().and_then(|style| style.bgcolor);
        assert_eq!(hover_title_bg, hover_help_bg);
        assert!(
            hover_title_bg.is_some(),
            "hovered rows should paint a full-width background"
        );
        assert_ne!(
            hover_title_bg, selected_bg,
            "hover and selected backgrounds should remain visually distinct"
        );
    }

    #[test]
    fn command_palette_mouse_move_updates_and_clears_list_hover() {
        let mut palette = CommandPalette::new(Label::new("body"));
        palette.on_layout(80, 20);
        let mut open_ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut open_ctx);
        assert!(palette.is_open());

        let (_, panel_y, panel_w, panel_h) =
            palette.palette_geometry(palette.layout_width, palette.layout_height);
        let (_, results_y, _, _) = palette.palette_results_geometry(0, panel_y, panel_w, panel_h);

        // First result row (title of first command).
        assert!(palette.on_mouse_move(3, results_y as u16));
        assert_eq!(palette.list.hovered_index(), Some(0));

        // Third visual row in results (title of second command).
        assert!(palette.on_mouse_move(3, results_y.saturating_add(2) as u16));
        assert_eq!(palette.list.hovered_index(), Some(1));

        // Move outside results area; hover should clear.
        assert!(palette.on_mouse_move(3, results_y.saturating_sub(1) as u16));
        assert_eq!(palette.list.hovered_index(), None);
    }

    #[test]
    fn command_palette_mouse_move_can_hover_last_command_rows() {
        let mut palette = CommandPalette::new(Label::new("body"));
        palette.on_layout(80, 20);
        let mut open_ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut open_ctx);
        assert!(palette.is_open());

        let (_, panel_y, panel_w, panel_h) =
            palette.palette_geometry(palette.layout_width, palette.layout_height);
        let (_, results_y, _, _) = palette.palette_results_geometry(0, panel_y, panel_w, panel_h);

        let last = palette.provider_results.len().saturating_sub(1);
        let last_title_row = last.saturating_mul(2);
        let last_help_row = last_title_row.saturating_add(1);
        assert!(palette.on_mouse_move(3, results_y.saturating_add(last_title_row) as u16));
        assert_eq!(palette.list.hovered_index(), Some(last));
        let _ = palette.on_mouse_move(3, results_y.saturating_add(last_help_row) as u16);
        assert_eq!(palette.list.hovered_index(), Some(last));

        // Entering directly on the help row from outside results should also set hover.
        assert!(palette.on_mouse_move(3, results_y.saturating_sub(1) as u16));
        assert_eq!(palette.list.hovered_index(), None);
        assert!(palette.on_mouse_move(3, results_y.saturating_add(last_help_row) as u16));
        assert_eq!(palette.list.hovered_index(), Some(last));
    }

    #[test]
    fn command_palette_hover_uses_render_geometry_when_layout_is_shorter() {
        let mut palette = CommandPalette::new(Label::new("body"));
        palette.on_layout(95, 16);
        let mut open_ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut open_ctx);
        assert!(palette.is_open());

        // Simulate tree-mode render where CommandPalette receives full viewport
        // options while on_layout was called with a shorter height.
        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (95, 35);
        options.max_width = 95;
        options.max_height = 35;
        let _ = Widget::render(&palette, &console, &options);

        let (_, panel_y, panel_w, panel_h) = palette.palette_geometry(95, 35);
        let (_, results_y, _, _) = palette.palette_results_geometry(0, panel_y, panel_w, panel_h);

        // Last command title row in 2-row command-list layout.
        let last = palette.provider_results.len().saturating_sub(1);
        let last_title_row = last.saturating_mul(2);
        assert!(palette.on_mouse_move(3, results_y.saturating_add(last_title_row) as u16));
        assert_eq!(palette.list.hovered_index(), Some(last));
    }

    #[test]
    fn command_palette_search_row_keeps_panel_bg_when_app_blurs() {
        let _guard = set_style_context(crate::css::default_widget_stylesheet());
        let mut palette = CommandPalette::new(Label::new("body"));
        palette.on_layout(95, 35);

        let mut open_ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut open_ctx);
        assert!(palette.is_open());

        let mut blur_ctx = EventCtx::default();
        palette
            .query
            .on_event(&Event::AppFocus(false), &mut blur_ctx);

        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (95, 35);
        options.max_width = 95;
        options.max_height = 35;
        let buf = FrameBuffer::from_renderable(
            &console,
            &options,
            &WidgetRenderable::new(&palette),
            None,
        );

        let (panel_x, panel_y, panel_w, _) = palette.palette_geometry(95, 35);
        let search_y = panel_y.saturating_add(CommandPalette::SEARCH_ROW_OFFSET);
        let search_icon_x = panel_x.saturating_add(CommandPalette::SEARCH_ICON_X_OFFSET);
        let panel_bg = crate::css::resolve_component_style(&palette, &["command-palette--panel"])
            .to_rich()
            .and_then(|style| style.bgcolor)
            .expect("panel bg must resolve");

        for tx in search_icon_x..panel_x.saturating_add(panel_w).min(95) {
            let bg = buf
                .get(tx, search_y)
                .style
                .as_ref()
                .and_then(|style| style.bgcolor);
            assert_eq!(
                bg,
                Some(panel_bg),
                "search row x={} should keep panel bg when app blurs",
                tx
            );
        }
    }

    #[test]
    fn command_palette_blocks_child_clicks_while_close_animation_visible() {
        let _guard = set_style_context(StyleSheet::parse(
            "CommandPalette > .command-palette--panel { transition: command-palette.panel-y 180ms ease-out; }",
        ));
        let mouse_downs = Arc::new(AtomicUsize::new(0));
        let child = EventProbe::new(mouse_downs.clone());
        let mut palette = CommandPalette::new(child);
        palette.on_layout(80, 20);

        let mut open_ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut open_ctx);
        assert!(palette.is_open());

        let mut close_ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut close_ctx);
        assert!(!palette.is_open());
        assert!(palette.panel_visible);

        let mut click_ctx = EventCtx::default();
        palette.on_event(
            &Event::MouseDown(crate::event::MouseDownEvent {
                target: NodeId::default(),
                screen_x: 1,
                screen_y: 1,
                x: 1,
                y: 1,
            }),
            &mut click_ctx,
        );
        assert!(click_ctx.handled());
        assert_eq!(mouse_downs.load(Ordering::Relaxed), 0);

        let mut settle_ctx = EventCtx::default();
        palette.on_event(
            &Event::AnimationValue(AnimationValueEvent {
                target: NodeId::default(),
                attribute: CommandPalette::PANEL_Y_ATTR.to_string(),
                value: CommandPalette::CLOSED_PANEL_Y,
                done: true,
            }),
            &mut settle_ctx,
        );
        assert!(!palette.panel_visible);

        let mut click_ctx = EventCtx::default();
        palette.on_event(
            &Event::MouseDown(crate::event::MouseDownEvent {
                target: NodeId::default(),
                screen_x: 1,
                screen_y: 1,
                x: 1,
                y: 1,
            }),
            &mut click_ctx,
        );
        assert_eq!(mouse_downs.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn command_palette_unmount_resets_open_and_panel_visibility_state() {
        let mut palette = CommandPalette::new(Label::new("body"));
        palette.on_layout(80, 20);
        let mut open_ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut open_ctx);
        assert!(palette.is_open());
        assert!(palette.panel_visible);

        palette.on_unmount();

        assert!(!palette.is_open());
        assert!(!palette.panel_visible);
        assert_eq!(palette.panel_render_y, CommandPalette::CLOSED_PANEL_Y);
    }

    #[test]
    fn command_palette_tree_mode_shows_wrapped_child_by_default() {
        let mut palette = CommandPalette::new(Label::new("body"));
        let children = palette.take_composed_children();
        assert_eq!(children.len(), 1);
        assert_eq!(palette.child_display_for_tree(0), Some(true));
    }

    #[test]
    fn command_palette_tree_mode_can_hide_wrapped_child_for_runtime_host() {
        let mut palette =
            CommandPalette::new(Label::new("body")).with_tree_wrapped_child_visible(false);
        let children = palette.take_composed_children();
        assert_eq!(children.len(), 1);
        assert_eq!(palette.child_display_for_tree(0), Some(false));
    }

    #[test]
    fn command_palette_panel_component_style_resolves_non_empty_background() {
        let _guard = set_style_context(crate::css::default_widget_stylesheet());
        let palette = CommandPalette::new(Label::new("body"));
        let style = crate::css::resolve_component_style(&palette, &["command-palette--panel"]);
        assert!(
            style.bg.is_some(),
            "CommandPalette panel component must resolve a background color"
        );
    }

    #[test]
    fn bindings_hidden_while_closed_and_exposed_when_open() {
        let mut palette = CommandPalette::new(Label::new("body"));
        assert!(palette.bindings().is_empty());

        palette.set_open(true, &mut EventCtx::default());
        let bindings = palette.bindings();
        assert!(!bindings.is_empty());
        assert!(bindings.iter().any(|b| b.action == "dismiss"));
    }

    #[test]
    fn execute_action_handles_dismiss() {
        use crate::action::ParsedAction;
        let mut palette = CommandPalette::new(Label::new("body"));
        palette.set_open(true, &mut EventCtx::default());
        assert!(palette.is_open());
        let mut ctx = EventCtx::default();
        let action = ParsedAction {
            namespace: None,
            name: "dismiss".to_string(),
            arguments: vec![],
        };
        assert!(palette.execute_action(&action, &mut ctx));
        assert!(!palette.is_open());
    }

    // -----------------------------------------------------------------------
    // Provider pattern tests
    // -----------------------------------------------------------------------

    struct TestProvider {
        name: &'static str,
        commands: Vec<(&'static str, &'static str, &'static str)>,
        startup_count: Arc<AtomicUsize>,
        shutdown_count: Arc<AtomicUsize>,
    }

    impl TestProvider {
        fn new(
            name: &'static str,
            commands: Vec<(&'static str, &'static str, &'static str)>,
            startup_count: Arc<AtomicUsize>,
            shutdown_count: Arc<AtomicUsize>,
        ) -> Self {
            Self {
                name,
                commands,
                startup_count,
                shutdown_count,
            }
        }
    }

    impl Provider for TestProvider {
        fn name(&self) -> &str {
            self.name
        }

        fn startup(&mut self) {
            self.startup_count.fetch_add(1, Ordering::Relaxed);
        }

        fn search(&self, query: &str) -> Vec<ProviderResult> {
            let needle = query.trim().to_lowercase();
            self.commands
                .iter()
                .filter_map(|(id, title, help)| {
                    if needle.is_empty() {
                        return Some(ProviderResult {
                            id: id.to_string(),
                            title: title.to_string(),
                            help: help.to_string(),
                            score: 0.0,
                        });
                    }
                    FuzzyMatcher::score(&needle, &title.to_lowercase()).map(|s| ProviderResult {
                        id: id.to_string(),
                        title: title.to_string(),
                        help: help.to_string(),
                        score: s,
                    })
                })
                .collect()
        }

        fn shutdown(&mut self) {
            self.shutdown_count.fetch_add(1, Ordering::Relaxed);
        }
    }

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
    fn provider_results_sorted_by_score_across_providers() {
        let s1 = Arc::new(AtomicUsize::new(0));
        let d1 = Arc::new(AtomicUsize::new(0));
        let s2 = Arc::new(AtomicUsize::new(0));
        let d2 = Arc::new(AtomicUsize::new(0));

        let mut palette = CommandPalette::new(Label::new("body"))
            .with_commands(Vec::new())
            .with_provider(TestProvider::new(
                "p1",
                vec![("zz", "Zzz sleep", "low score")],
                s1.clone(),
                d1.clone(),
            ))
            .with_provider(TestProvider::new(
                "p2",
                vec![("aa", "Alpha action", "high score")],
                s2.clone(),
                d2.clone(),
            ));

        let mut ctx = EventCtx::default();
        palette.set_open(true, &mut ctx);

        palette.query.set_text("a");
        palette.rebuild_results(true);

        assert_eq!(s1.load(Ordering::Relaxed), 1);
        assert_eq!(s2.load(Ordering::Relaxed), 1);
        assert!(!palette.provider_results.is_empty());
        assert_eq!(palette.provider_results[0].id, "aa");

        palette.set_open(false, &mut ctx);
        assert_eq!(d1.load(Ordering::Relaxed), 1);
        assert_eq!(d2.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn provider_startup_shutdown_called_on_open_close() {
        let startup = Arc::new(AtomicUsize::new(0));
        let shutdown = Arc::new(AtomicUsize::new(0));

        let mut palette = CommandPalette::new(Label::new("body")).with_provider(TestProvider::new(
            "tracker",
            vec![("cmd", "Command", "Help")],
            startup.clone(),
            shutdown.clone(),
        ));

        let mut ctx = EventCtx::default();
        palette.set_open(true, &mut ctx);
        assert_eq!(startup.load(Ordering::Relaxed), 1);
        assert_eq!(shutdown.load(Ordering::Relaxed), 0);

        palette.set_open(false, &mut ctx);
        assert_eq!(startup.load(Ordering::Relaxed), 1);
        assert_eq!(shutdown.load(Ordering::Relaxed), 1);

        palette.set_open(true, &mut ctx);
        assert_eq!(startup.load(Ordering::Relaxed), 2);
        palette.set_open(false, &mut ctx);
        assert_eq!(shutdown.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn provider_results_merged_with_builtin_commands() {
        let startup = Arc::new(AtomicUsize::new(0));
        let shutdown = Arc::new(AtomicUsize::new(0));

        let mut palette = CommandPalette::new(Label::new("body")).with_provider(TestProvider::new(
            "custom",
            vec![("deploy", "Deploy", "Ship it")],
            startup.clone(),
            shutdown.clone(),
        ));

        let mut ctx = EventCtx::default();
        palette.set_open(true, &mut ctx);

        // 4 built-in + 1 provider = 5 results on empty query.
        assert_eq!(palette.provider_results.len(), 5);
        let ids: Vec<&str> = palette
            .provider_results
            .iter()
            .map(|r| r.id.as_str())
            .collect();
        assert!(ids.contains(&"deploy"));
        assert!(ids.contains(&"quit"));
    }

    #[test]
    fn provider_shutdown_called_on_unmount_while_open() {
        let startup = Arc::new(AtomicUsize::new(0));
        let shutdown = Arc::new(AtomicUsize::new(0));

        let mut palette = CommandPalette::new(Label::new("body")).with_provider(TestProvider::new(
            "tracker",
            vec![("cmd", "Command", "Help")],
            startup.clone(),
            shutdown.clone(),
        ));

        let mut ctx = EventCtx::default();
        palette.set_open(true, &mut ctx);
        assert_eq!(startup.load(Ordering::Relaxed), 1);

        palette.on_unmount();
        assert_eq!(shutdown.load(Ordering::Relaxed), 1);
        assert!(!palette.is_open());
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

    #[test]
    fn command_palette_match_highlight_underlines_matched_chars() {
        let _guard = set_style_context(crate::css::default_widget_stylesheet());
        let mut palette = CommandPalette::new(Label::new("body"));
        let mut open_ctx = EventCtx::default();
        palette.on_event(&Event::Action(Action::CommandPalette), &mut open_ctx);
        assert!(open_ctx.handled());

        for ch in ['e', 'y'] {
            let key = crate::keys::KeyEventData::from_crossterm(KeyEvent::new(
                KeyCode::Char(ch),
                KeyModifiers::NONE,
            ));
            let mut key_ctx = EventCtx::default();
            palette.on_event(&Event::Key(key), &mut key_ctx);
            for message in key_ctx.take_messages() {
                let mut msg_ctx = EventCtx::default();
                palette.on_message(&message, &mut msg_ctx);
            }
        }

        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (72, 14);
        options.max_width = 72;
        options.max_height = 14;
        let buf = FrameBuffer::from_renderable(
            &console,
            &options,
            &WidgetRenderable::new(&palette),
            None,
        );
        let lines = buf.as_plain_lines();
        let (keys_y, keys_x) = lines
            .iter()
            .enumerate()
            .find_map(|(y, line)| line.find("Keys").map(|x| (y, x)))
            .expect("Keys row should be present");

        let e_style = buf
            .get(keys_x + 1, keys_y)
            .style
            .as_ref()
            .expect("matched 'e' should have style");
        let y_style = buf
            .get(keys_x + 2, keys_y)
            .style
            .as_ref()
            .expect("matched 'y' should have style");
        let k_style = buf
            .get(keys_x, keys_y)
            .style
            .as_ref()
            .expect("base title character should have style");

        assert_eq!(e_style.underline, Some(true));
        assert_eq!(y_style.underline, Some(true));
        assert_eq!(e_style.bgcolor, k_style.bgcolor);
        assert_eq!(y_style.bgcolor, k_style.bgcolor);
    }
}
