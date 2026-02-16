use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};
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
    WidgetStyles, helpers::adjust_line_length_no_bg,
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
}

impl SystemCommandsProvider {
    pub fn new(commands: Vec<PaletteCommand>) -> Self {
        Self { commands }
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
            return self
                .commands
                .iter()
                .map(|cmd| ProviderResult {
                    id: cmd.id.clone(),
                    title: cmd.title.clone(),
                    help: cmd.help.clone(),
                    score: 0.0,
                })
                .collect();
        }
        self.commands
            .iter()
            .filter_map(|cmd| {
                let best = [&cmd.id, &cmd.title, &cmd.help]
                    .iter()
                    .filter_map(|text| FuzzyMatcher::score(&needle, &text.to_lowercase()))
                    .max();
                best.map(|score| ProviderResult {
                    id: cmd.id.clone(),
                    title: cmd.title.clone(),
                    help: cmd.help.clone(),
                    score: score as f64,
                })
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// FuzzyMatcher
// ---------------------------------------------------------------------------

/// Simple fuzzy matcher: scores a query against text based on character positions,
/// consecutive-match bonuses, and start-of-word bonuses.
pub struct FuzzyMatcher;

impl FuzzyMatcher {
    /// Returns a score if all characters in `query` appear (in order) in `text`.
    /// Higher score = better match. Returns `None` if no match.
    pub fn score(query: &str, text: &str) -> Option<u32> {
        if query.is_empty() {
            return Some(0);
        }

        let query_chars: Vec<char> = query.chars().collect();
        let text_chars: Vec<char> = text.chars().collect();

        if query_chars.len() > text_chars.len() {
            return None;
        }

        let mut qi = 0;
        let mut score: u32 = 0;
        let mut prev_match_pos: Option<usize> = None;

        for (ti, &tc) in text_chars.iter().enumerate() {
            if qi < query_chars.len() && tc == query_chars[qi] {
                // Base score per character matched
                score += 10;

                // Consecutive match bonus
                if let Some(prev) = prev_match_pos {
                    if ti == prev + 1 {
                        score += 5;
                    }
                }

                // Start-of-word bonus (first char or preceded by separator)
                if ti == 0
                    || text_chars
                        .get(ti.wrapping_sub(1))
                        .map_or(false, |&c| c == ' ' || c == '_' || c == '-')
                {
                    score += 8;
                }

                // Early position bonus (penalize late matches less)
                score += (text_chars.len().saturating_sub(ti) as u32).min(5);

                prev_match_pos = Some(ti);
                qi += 1;
            }
        }

        if qi == query_chars.len() {
            Some(score)
        } else {
            None
        }
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
            input: Input::new().with_placeholder(placeholder),
            styles: WidgetStyles::default(),
        }
    }

    pub fn text(&self) -> &str {
        self.input.text()
    }

    pub fn set_text(&mut self, value: impl Into<String>) {
        self.input.set_text(value);
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
#[derive(Debug, Clone)]
pub struct CommandList {
    list: ListView,
    entries: Vec<CommandListEntry>,
    visible: bool,
    populating: bool,
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
}

impl Default for CommandList {
    fn default() -> Self {
        Self::new()
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
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let mut out = Segments::new();

        let base_title_style = crate::css::resolve_component_style(self, &["option-list--option"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);
        let selected_style =
            crate::css::resolve_component_style(self, &["option-list--option-highlighted"])
                .to_rich()
                .unwrap_or(base_title_style);
        let help_style = crate::css::resolve_component_style(self, &["command-palette--help-text"])
            .to_rich()
            .unwrap_or(base_title_style);

        let visible_items = (height / 2).max(1);
        let start = self
            .offset()
            .min(self.entries.len().saturating_sub(visible_items));
        let selected = self.selected().min(self.entries.len().saturating_sub(1));

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
                let title_style = if active {
                    selected_style
                } else {
                    base_title_style
                };
                let help_line_style = if active {
                    selected_style.combine(&help_style)
                } else {
                    help_style
                };
                let text = if is_help { &entry.help } else { &entry.title };
                let style = if is_help {
                    help_line_style
                } else {
                    title_style
                };
                let mut rich_text = console.render_str(text, Some(true), None, None, None);
                rich_text.stylize_before(style, 0, None);
                let rendered = rich_text.render(console, options);
                let lines = rich_rs::Segment::split_lines(rendered);
                let first_line = lines.into_iter().next().unwrap_or_default();
                adjust_line_length_no_bg(&first_line, width)
            };
            out.extend(line);
            if visual_row + 1 < height {
                out.push(Segment::line());
            }
        }
        out
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
        self.list.on_event(event, ctx);
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
        self.list.focusable()
    }

    fn set_focus(&mut self, focused: bool) {
        self.list.set_focus(focused);
    }

    fn has_focus(&self) -> bool {
        self.list.has_focus()
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
    open: bool,
    show_key_panel: bool,
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
    styles: WidgetStyles,
    /// External providers registered via [`add_provider`](Self::add_provider).
    providers: Vec<Box<dyn Provider>>,
    /// Merged results from all providers, sorted by descending score.
    provider_results: Vec<ProviderResult>,
}

impl CommandPalette {
    const KEY_PANEL_WIDTH_ATTR: &'static str = "command_palette.key_panel_width";
    const PANEL_Y_ATTR: &'static str = "command_palette.panel_y";
    const CLOSED_PANEL_Y: f32 = 0.0;

    pub fn new(child: impl Widget + 'static) -> Self {
        let commands = vec![
            PaletteCommand::new(
                "keys",
                "Keys",
                "Show help for the focused widget and a summary of available keys",
            ),
            PaletteCommand::new("maximize", "Maximize", "Maximize the focused widget"),
            PaletteCommand::new("quit", "Quit", "Quit the application as soon as possible"),
            PaletteCommand::new(
                "screenshot",
                "Screenshot",
                "Save an SVG screenshot of the current screen",
            ),
            PaletteCommand::new("theme", "Theme", "Change the current theme"),
        ];
        let mut out = Self {
            child: Box::new(child),
            child_extracted: false,
            open: false,
            show_key_panel: false,
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
            styles: WidgetStyles::default(),
            providers: Vec::new(),
            provider_results: Vec::new(),
        };
        out.rebuild_results();
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
        self.rebuild_results();
        self
    }

    pub fn set_commands(&mut self, commands: Vec<PaletteCommand>) {
        self.system_provider = SystemCommandsProvider::new(commands);
        self.rebuild_results();
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn is_visible_in_tree(&self) -> bool {
        self.open || self.panel_visible
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

    fn palette_geometry(&self, width: usize, height: usize) -> (usize, usize, usize, usize) {
        let panel_x = 0usize;
        let panel_y = 2usize.min(height.saturating_sub(1));
        let panel_width = width.max(1);
        let max_panel_height = height.saturating_sub(panel_y).max(1);
        let panel_height = max_panel_height.min(14).max(1);
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
        let content_x = panel_x.saturating_add(1);
        let content_y = panel_y.saturating_add(2);
        let content_width = Self::palette_content_width(panel_width);
        let content_height = panel_height.saturating_sub(3).max(1);
        (content_x, content_y, content_width, content_height)
    }

    fn rebuild_results(&mut self) {
        let query = self.query.text().trim().to_string();

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

        let entries = self
            .provider_results
            .iter()
            .map(|r| CommandListEntry {
                title: r.title.clone(),
                help: r.help.clone(),
            })
            .collect::<Vec<_>>();
        self.list.set_entries(entries);
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

    fn set_open(&mut self, open: bool, ctx: &mut EventCtx) {
        if self.open == open
            && ((self.open && self.panel_visible) || (!self.open && !self.panel_visible))
        {
            return;
        }
        let was_open = self.open;
        let was_visible = self.panel_visible;
        self.open = open;
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
            self.list.set_focus(true);
            self.list.set_visible(true);
            self.list.set_populating(false);
            self.rebuild_results();
            let target_y = self.panel_target_y();
            let start_y = if was_visible {
                self.panel_render_y
            } else {
                Self::CLOSED_PANEL_Y
            };
            self.animate_panel_y(start_y, target_y, ctx);
            ctx.post_message(Message::CommandPaletteOpened(CommandPaletteOpened));
        } else {
            for provider in &mut self.providers {
                provider.shutdown();
            }
            self.query.set_focus(false);
            self.list.set_focus(false);
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
                ctx.post_message(Message::CommandPaletteClosed(CommandPaletteClosed));
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
        ctx.post_message(Message::CommandPaletteCommandSelected(
            CommandPaletteCommandSelected {
                id: result.id.clone(),
                title: result.title.clone(),
            },
        ));
        match result.id.as_str() {
            "quit" => ctx.request_stop(),
            "keys" => {
                let before = self
                    .key_panel_render_width
                    .round()
                    .clamp(0.0, self.layout_width as f32) as usize;
                self.show_key_panel = !self.show_key_panel;
                let target = if self.show_key_panel {
                    self.key_panel_width(self.layout_width)
                } else {
                    0
                };
                self.animate_key_panel_width(before, target, ctx);
                ctx.request_repaint();
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
        // Keep the wrapped child subtree rendered in tree mode so the command
        // palette behaves as a true overlay/modal over the existing UI.
        Some(true)
    }

    fn preserve_underlay(&self) -> bool {
        // Tree-mode command palette behaves like an overlay/modal and should
        // not trigger full-rect background fill from the generic styled path.
        true
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let (width, height) = options.size;
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
        let (panel_x, target_panel_y, panel_width, panel_height) =
            self.palette_geometry(width, height);
        let panel_y = self
            .panel_render_y
            .round()
            .clamp(0.0, target_panel_y as f32) as usize;
        let panel_style = crate::css::resolve_component_style(self, &["command-palette--panel"])
            .to_rich()
            .unwrap_or_else(rich_rs::Style::new);
        let apply_panel_surface = |mut cell: Cell| -> Cell {
            cell.style = Some(match cell.style {
                Some(style) => panel_style.combine(&style),
                None => panel_style,
            });
            cell
        };

        for y in panel_y..panel_y.saturating_add(panel_height).min(height) {
            for x in panel_x..panel_x.saturating_add(panel_width).min(width) {
                *overlay.get_mut(x, y) = Cell::blank(Some(panel_style));
            }
        }

        let search_width = Self::palette_content_width(panel_width)
            .saturating_sub(2)
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

        let search_y = panel_y;
        let search_icon_x = panel_x.saturating_add(1);
        if search_y < height {
            for sx in 0..icon_buffer.width.min(2) {
                let tx = search_icon_x.saturating_add(sx);
                if tx >= width {
                    break;
                }
                *overlay.get_mut(tx, search_y) =
                    apply_panel_surface(icon_buffer.get(sx, 0).clone());
            }
        }
        if search_y < height {
            for sx in 0..search_buffer.width.min(search_width) {
                let tx = panel_x.saturating_add(4).saturating_add(sx);
                if tx >= width {
                    break;
                }
                *overlay.get_mut(tx, search_y) =
                    apply_panel_surface(search_buffer.get(sx, 0).clone());
            }
        }

        let (results_x, results_y, results_w, results_h) =
            self.palette_results_geometry(panel_x, panel_y, panel_width, panel_height);
        let mut results_options = options.clone();
        results_options.size = (results_w.max(1), results_h.max(1));
        results_options.max_width = results_w.max(1);
        results_options.max_height = results_h.max(1);
        let results_buffer = FrameBuffer::from_renderable(
            console,
            &results_options,
            &WidgetRenderable::new(&self.list),
            None,
        );
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
                *overlay.get_mut(tx, ty) = apply_panel_surface(results_buffer.get(x, y).clone());
            }
        }
        for ty in results_y..results_y.saturating_add(results_h).min(height) {
            for tx in results_x..results_x.saturating_add(results_w).min(width) {
                let cell = overlay.get(tx, ty);
                if (cell.text.is_empty() || cell.text == " ")
                    && cell.style.is_none()
                    && cell.meta.is_none()
                {
                    *overlay.get_mut(tx, ty) = Cell::blank(Some(panel_style));
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
                let tx = panel_x.saturating_add(4).saturating_add(sx);
                if tx >= width {
                    break;
                }
                let cell = search_buffer.get(sx, sy).clone();
                if cell.meta.is_some() {
                    *overlay.get_mut(tx, ty) = apply_panel_surface(cell);
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
                    seg.meta = cell.meta.clone();
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
        let panel_target_y = self.panel_target_y();
        if self.open {
            self.panel_visible = true;
            self.panel_render_y = self
                .panel_render_y
                .clamp(Self::CLOSED_PANEL_Y, panel_target_y);
            if self.panel_render_y <= Self::CLOSED_PANEL_Y && panel_target_y > Self::CLOSED_PANEL_Y
            {
                self.panel_render_y = panel_target_y;
            }
        } else if !self.panel_visible {
            self.panel_render_y = Self::CLOSED_PANEL_Y;
        } else {
            self.panel_render_y = self
                .panel_render_y
                .clamp(Self::CLOSED_PANEL_Y, panel_target_y);
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
            .saturating_sub(2)
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
        let panel_target_y = self.panel_target_y();
        if self.open {
            self.panel_visible = true;
            self.panel_render_y = self
                .panel_render_y
                .clamp(Self::CLOSED_PANEL_Y, panel_target_y);
            if self.panel_render_y <= Self::CLOSED_PANEL_Y && panel_target_y > Self::CLOSED_PANEL_Y
            {
                self.panel_render_y = panel_target_y;
            }
        } else if !self.panel_visible {
            self.panel_render_y = Self::CLOSED_PANEL_Y;
        } else {
            self.panel_render_y = self
                .panel_render_y
                .clamp(Self::CLOSED_PANEL_Y, panel_target_y);
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
            .saturating_sub(2)
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

            if mouse.target == self.node_id() {
                // Let Input handle cursor placement/focus details.
            } else {
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
                }
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
            && matches!(
                message.message,
                Message::OverlaySetVisible(..)
                    | Message::OverlayToggle(..)
                    | Message::OverlayDismissRequested(..)
                    | Message::OverlayVisibilityChanged(..)
            )
        {
            self.set_open(false, ctx);
        }
        self.query.on_message(message, ctx);
        self.list.on_message(message, ctx);
        self.key_panel.on_message(message, ctx);
        if let Message::CommandPaletteSetCommands(CommandPaletteSetCommands { commands }) =
            &message.message
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
        if self.open
            && message.sender == self.query.node_id()
            && matches!(message.message, Message::InputChanged(..))
        {
            self.rebuild_results();
            ctx.request_repaint();
            ctx.set_handled();
            return;
        }
        if !self.is_tree_mode() {
            self.child.on_message(message, ctx);
        }
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
    use crate::widgets::Label;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
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
        assert!(
            messages
                .iter()
                .any(|event| matches!(event.message, Message::CommandPaletteCommandSelected(..)))
        );
        assert!(!palette.is_open());
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
        assert!(messages.iter().any(|event| {
            matches!(
                event.message,
                Message::CommandPaletteCommandSelected(CommandPaletteCommandSelected { ref id, .. }) if id == "keys"
            )
        }));
        assert!(
            messages
                .iter()
                .any(|event| matches!(event.message, Message::CommandPaletteClosed(_)))
        );
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
            matches!(
                event.message,
                Message::CommandPaletteCommandSelected(CommandPaletteCommandSelected { ref id, .. }) if id == "quit"
            )
        });
        let close_idx = messages
            .iter()
            .position(|event| matches!(event.message, Message::CommandPaletteClosed(_)));
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
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::CommandPaletteSetCommands(CommandPaletteSetCommands {
                    commands: vec![CommandPaletteCommand {
                        id: "deploy".to_string(),
                        title: "Deploy".to_string(),
                        help: "Ship current build".to_string(),
                    }],
                }),
                control: None,
            },
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
            &MessageEvent {
                sender: NodeId::default(),
                message: Message::OverlayVisibilityChanged(OverlayVisibilityChanged {
                    overlay: NodeId::default(),
                    visible: true,
                }),
                control: None,
            },
            &mut transition_ctx,
        );
        assert!(!palette.is_open());
        let messages = transition_ctx.take_messages();
        assert!(
            messages
                .iter()
                .any(|event| matches!(event.message, Message::CommandPaletteClosed(_)))
        );
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
        assert!(
            messages
                .iter()
                .any(|event| matches!(event.message, Message::CommandPaletteClosed(_)))
        );
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
            .position(|event| matches!(event.message, Message::CommandPaletteCommandSelected(..)));
        let close_idx = messages
            .iter()
            .position(|event| matches!(event.message, Message::CommandPaletteClosed(_)));

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
                        score: s as f64,
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
        palette.rebuild_results();

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

        // 5 built-in + 1 provider = 6 results on empty query.
        assert_eq!(palette.provider_results.len(), 6);
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
}
