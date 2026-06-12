use std::ops::Range;

use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments, Text};

use crate::debug::debug_message;
use crate::event::{Event, EventCtx};
use crate::message::*;
use crate::renderables::Styled;

use super::helpers::{empty_classes, fixed_height_from_constraints};
use super::{Widget, WidgetStyles};
use crate::reactive::{ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget};

fn set_class_flag(classes: &mut Vec<String>, class: &str, enabled: bool) {
    if enabled {
        if !classes.iter().any(|existing| existing == class) {
            classes.push(class.to_string());
        }
    } else {
        classes.retain(|existing| existing != class);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FooterBinding {
    pub key: String,
    pub description: String,
    pub tooltip: Option<String>,
    pub group: Option<String>,
    /// Raw key spec from the binding hint (e.g. "ctrl+p"), used for
    /// click-to-invoke dispatch. Distinct from `key` which may be a
    /// display-formatted version (e.g. "^p").
    pub action_key: Option<String>,
    /// Result of `check_action` for this binding:
    /// - `Some(true)` — enabled (rendered normally)
    /// - `Some(false)` — hidden (filtered out)
    /// - `None` — disabled but visible (rendered dimmed)
    pub enabled: Option<bool>,
}

impl FooterBinding {
    pub fn new(key: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            description: description.into(),
            tooltip: None,
            group: None,
            action_key: None,
            enabled: Some(true),
        }
    }

    pub fn with_group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }

    pub fn with_action_key(mut self, action_key: impl Into<String>) -> Self {
        self.action_key = Some(action_key.into());
        self
    }

    pub fn with_tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }
}

#[derive(Debug, Clone)]
pub struct FooterKey {
    key: String,
    description: String,
    compact: bool,
    hovered: bool,
    disabled: bool,
    parent_bg: crate::style::Color,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl FooterKey {
    pub fn new(key: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            description: description.into(),
            compact: false,
            hovered: false,
            disabled: false,
            parent_bg: crate::style::parse_color_like("$background")
                .unwrap_or(crate::style::Color::rgb(0, 0, 0)),
            classes: Vec::new(),
            styles: WidgetStyles::default(),
        }
    }

    pub fn with_compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        set_class_flag(&mut self.classes, "-compact", compact);
        self
    }

    pub fn with_grouped(mut self, grouped: bool) -> Self {
        set_class_flag(&mut self.classes, "-grouped", grouped);
        self
    }

    pub fn with_command_palette(mut self, command_palette: bool) -> Self {
        set_class_flag(&mut self.classes, "-command-palette", command_palette);
        self
    }

    pub fn with_hovered(mut self, hovered: bool) -> Self {
        self.hovered = hovered;
        self
    }

    pub fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        set_class_flag(&mut self.classes, "-disabled", disabled);
        self
    }

    pub fn with_parent_bg(mut self, parent_bg: crate::style::Color) -> Self {
        self.parent_bg = parent_bg;
        self
    }

    pub fn key_only(mut self) -> Self {
        self.description.clear();
        self
    }

    fn resolved_base_style(&self) -> crate::style::Style {
        let meta = crate::css::selector_meta_generic(self);
        crate::css::resolve_style(self, &meta)
    }

    fn component_style(&self, classes: &[&str], fallback: rich_rs::Style) -> rich_rs::Style {
        let component = crate::css::resolve_component_style(self, classes);
        if component.is_empty() {
            fallback
        } else {
            let effective_bg = component
                .bg
                .map(|bg| bg.flatten_over(self.parent_bg))
                .unwrap_or(self.parent_bg);
            component.to_rich_over(effective_bg).unwrap_or(fallback)
        }
    }

    fn render_segments(&self) -> Segments {
        let base = self.resolved_base_style();
        let base_rich = base
            .to_rich_over(self.parent_bg)
            .unwrap_or_else(rich_rs::Style::new);

        let key_component = crate::css::resolve_component_style(self, &["footer-key--key"]);
        let mut key_padding = key_component.effective_padding();
        if self.classes.iter().any(|class| class == "-command-palette") {
            // Python parity: command-palette hint border meets the key glyph without
            // an extra leading blank column.
            key_padding.left = 0;
        }
        let key_style =
            self.component_style(&["footer-key--key"], rich_rs::Style::new().with_bold(true));
        let description_component =
            crate::css::resolve_component_style(self, &["footer-key--description"]);
        let description_padding = description_component.effective_padding();
        let description_style =
            self.component_style(&["footer-key--description"], rich_rs::Style::new());

        let mut out = Segments::new();
        if self.description.is_empty() {
            out.push(Segment::styled(self.key.clone(), key_style));
        } else {
            let key_text = format!(
                "{}{}{}",
                " ".repeat(usize::from(key_padding.left)),
                self.key,
                " ".repeat(usize::from(key_padding.right))
            );
            out.push(Segment::styled(key_text, key_style));
            let description_text = format!(
                "{}{}{}",
                " ".repeat(usize::from(description_padding.left)),
                self.description,
                " ".repeat(usize::from(description_padding.right))
            );
            out.push(Segment::styled(description_text, description_style));
        }
        // Python parity: apply FooterKey rich style over the whole renderable
        // after assembling component-styled spans.
        let mut segments = Styled::<()>::process_segments(out, base_rich, rich_rs::Style::new());

        // Dim disabled bindings (check_action returned None).
        if self.disabled {
            segments = segments
                .into_iter()
                .map(|mut seg| {
                    if let Some(ref mut style) = seg.style {
                        *style = style.with_dim(true);
                    } else {
                        seg.style = Some(rich_rs::Style::new().with_dim(true));
                    }
                    seg
                })
                .collect();
        }

        segments
    }
}

impl Widget for FooterKey {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        self.render_segments()
    }

    fn style_classes(&self) -> &[String] {
        if self.classes.is_empty() {
            empty_classes()
        } else {
            &self.classes
        }
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }

    fn is_hovered(&self) -> bool {
        self.hovered
    }
}

impl Renderable for FooterKey {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug, Clone)]
pub struct FooterLabel {
    text: String,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl FooterLabel {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            classes: Vec::new(),
            styles: WidgetStyles::default(),
        }
    }

    fn render_segments(&self) -> Segments {
        let meta = crate::css::selector_meta_generic(self);
        let style = crate::css::resolve_style(self, &meta);
        let rich = style.to_rich().unwrap_or_else(rich_rs::Style::new);
        let mut out = Segments::new();
        out.push(Segment::styled(format!(" {}", self.text), rich));
        out
    }
}

impl Widget for FooterLabel {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        self.render_segments()
    }

    fn style_classes(&self) -> &[String] {
        if self.classes.is_empty() {
            empty_classes()
        } else {
            &self.classes
        }
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for FooterLabel {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug, Clone)]
pub struct Footer {
    bindings: Vec<FooterBinding>,
    compact: bool,
    hovered_item: Option<HoveredFooterItem>,
    layout_width: usize,
    app_focused: bool,
    deferred_bindings: Option<Vec<FooterBinding>>,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl Footer {
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
            compact: false,
            hovered_item: None,
            layout_width: 1,
            app_focused: true,
            deferred_bindings: None,
            classes: Vec::new(),
            styles: WidgetStyles::default(),
        }
    }

    pub fn with_binding(mut self, key: impl Into<String>, description: impl Into<String>) -> Self {
        self.bindings.push(FooterBinding::new(key, description));
        self
    }

    pub fn set_bindings(&mut self, bindings: Vec<FooterBinding>) {
        self.bindings = bindings;
    }

    pub fn clear_bindings(&mut self) {
        self.bindings.clear();
    }

    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        set_class_flag(&mut self.classes, "-compact", compact);
        self
    }

    // ── Reactive getters ─────────────────────────────────────────────────

    /// Reactive getter for `compact`.
    pub fn is_compact(&self) -> bool {
        self.compact
    }

    // ── Reactive setters ─────────────────────────────────────────────────

    /// Reactive setter for `compact`. Records the change in the provided
    /// [`ReactiveCtx`] and triggers layout invalidation.
    pub fn set_compact(&mut self, value: bool, ctx: &mut ReactiveCtx) {
        if self.compact != value {
            let old = self.compact;
            self.compact = value;
            set_class_flag(&mut self.classes, "-compact", value);
            ctx.record_change(
                "compact",
                ReactiveFlags::reactive_layout(),
                Box::new(old),
                Box::new(value),
            );
        }
    }

    // ── Watchers ─────────────────────────────────────────────────────────

    fn watch_compact(&mut self, _old: &bool, _new: &bool, _ctx: &mut ReactiveCtx) {
        // Layout invalidation is handled by ReactiveFlags::reactive_layout().
        set_class_flag(&mut self.classes, "-compact", self.compact);
    }

    fn component_style(&self, classes: &[&str], fallback: rich_rs::Style) -> rich_rs::Style {
        let style = crate::css::resolve_component_style(self, classes);
        if style.is_empty() {
            fallback
        } else {
            let fallback_bg = crate::style::parse_color_like("$background")
                .unwrap_or(crate::style::Color::rgb(0, 0, 0));
            let parent_bg = self
                .resolved_base_style()
                .bg
                .map(|bg| bg.flatten_over(fallback_bg))
                .unwrap_or(fallback_bg);
            style.to_rich_over(parent_bg).unwrap_or(fallback)
        }
    }

    fn resolved_base_style(&self) -> crate::style::Style {
        let meta = crate::css::selector_meta_generic(self);
        crate::css::resolve_style(self, &meta)
    }

    fn base_style(&self) -> rich_rs::Style {
        let fallback_bg = crate::style::parse_color_like("$background")
            .unwrap_or(crate::style::Color::rgb(0, 0, 0));
        self.resolved_base_style()
            .to_rich_over(fallback_bg)
            .unwrap_or_else(rich_rs::Style::new)
    }

    fn effective_row_bg(&self) -> crate::style::Color {
        let fallback_bg = crate::style::parse_color_like("$background")
            .unwrap_or(crate::style::Color::rgb(0, 0, 0));
        self.resolved_base_style()
            .bg
            .map(|bg| bg.flatten_over(fallback_bg))
            .unwrap_or(fallback_bg)
    }

    fn palette_separator_style(&self) -> rich_rs::Style {
        let mut style = self.component_style(&["footer-key--palette-separator"], self.base_style());
        if self.hovered_item == Some(HoveredFooterItem::CommandPalette) {
            let row_bg = self.effective_row_bg();
            let key_bg = FooterKey::new(String::new(), String::new())
                .with_command_palette(true)
                .with_parent_bg(row_bg)
                .with_hovered(true)
                .component_style(&["footer-key--key"], rich_rs::Style::new())
                .bgcolor
                .unwrap_or(row_bg.to_simple_opaque());
            style = style.with_bgcolor(key_bg);
        }
        style
    }

    fn render_binding(
        &self,
        binding: &FooterBinding,
        flat_index: Option<usize>,
        grouped: bool,
        command_palette: bool,
        key_only: bool,
    ) -> Vec<Segment> {
        let hovered = match flat_index {
            Some(idx) => self.hovered_item == Some(HoveredFooterItem::Binding(idx)),
            None => command_palette && self.hovered_item == Some(HoveredFooterItem::CommandPalette),
        };
        let row_bg = self.effective_row_bg();
        let disabled = binding.enabled.is_none();
        let mut key = FooterKey::new(binding.key.clone(), binding.description.clone())
            .with_compact(self.compact)
            .with_grouped(grouped)
            .with_command_palette(command_palette)
            .with_parent_bg(row_bg)
            .with_hovered(hovered)
            .with_disabled(disabled);
        if key_only {
            key = key.key_only();
        }
        key.render_segments().into_iter().collect()
    }

    fn render_group(
        &self,
        group_label: &str,
        group_bindings: &[FooterBinding],
        group_start_flat_index: usize,
        base_style: rich_rs::Style,
    ) -> Vec<Segment> {
        self.render_group_with_regions(
            group_label,
            group_bindings,
            group_start_flat_index,
            base_style,
        )
        .0
    }

    fn render_group_with_regions(
        &self,
        group_label: &str,
        group_bindings: &[FooterBinding],
        group_start_flat_index: usize,
        base_style: rich_rs::Style,
    ) -> (Vec<Segment>, Vec<(Range<usize>, usize)>) {
        let mut out = Vec::new();
        let mut regions = Vec::new();
        let key_separator = if self.compact { " " } else { "" };
        let mut pos = 0usize;
        for (index, binding) in group_bindings.iter().enumerate() {
            if index > 0 {
                let sep = Segment::styled(key_separator.to_string(), base_style);
                pos += Segment::cell_len(&sep);
                out.push(sep);
            }
            let binding_segments = self.render_binding(
                binding,
                Some(group_start_flat_index + index),
                true,
                false,
                true,
            );
            let width = Segment::get_line_length(&binding_segments);
            if width > 0 {
                regions.push((pos..pos + width, group_start_flat_index + index));
                pos += width;
            }
            out.extend(binding_segments);
        }
        let label = FooterLabel::new(group_label.to_string());
        let label_segments = label.render_segments();
        out.extend(label_segments);
        (out, regions)
    }

    fn split_bindings(&self) -> (Vec<LeftBindingItem>, Option<FooterBinding>) {
        let mut left_bindings = Vec::new();
        let mut palette = None::<FooterBinding>;
        for binding in &self.bindings {
            if binding.group.as_deref() == Some("command_palette") {
                palette = Some(binding.clone());
            } else {
                left_bindings.push(binding.clone());
            }
        }

        let mut left_items = Vec::new();
        let mut index = 0;
        while index < left_bindings.len() {
            let binding = &left_bindings[index];
            let Some(group_name) = binding.group.clone() else {
                left_items.push(LeftBindingItem::Single(binding.clone()));
                index += 1;
                continue;
            };

            let mut run_end = index + 1;
            while run_end < left_bindings.len()
                && left_bindings[run_end].group.as_deref() == Some(group_name.as_str())
            {
                run_end += 1;
            }
            if run_end - index > 1 {
                left_items.push(LeftBindingItem::Grouped {
                    label: group_name,
                    bindings: left_bindings[index..run_end].to_vec(),
                });
            } else {
                left_items.push(LeftBindingItem::Single(binding.clone()));
            }
            index = run_end;
        }

        (left_items, palette)
    }

    fn bindings_from_hints(hints: &[crate::event::BindingHint]) -> Vec<FooterBinding> {
        hints
            .iter()
            .filter(|hint| hint.show)
            // Filter out bindings where check_action returned Some(false) (hidden).
            .filter(|hint| hint.enabled != Some(false))
            .map(|hint| {
                let mut binding = FooterBinding::new(
                    hint.key_display.clone().unwrap_or_else(|| hint.key.clone()),
                    hint.description.clone(),
                );
                binding.tooltip = hint.tooltip.clone();
                binding.group = hint.group.clone();
                // Store the raw key spec for click-to-invoke dispatch.
                binding.action_key = Some(hint.key.clone());
                // Propagate check_action enabled state for dimming.
                binding.enabled = hint.enabled;
                binding
            })
            .collect()
    }

    fn apply_bindings(&mut self, next: Vec<FooterBinding>, ctx: &mut EventCtx) {
        if next == self.bindings {
            return;
        }
        self.bindings = next;
        if let Some(HoveredFooterItem::Binding(idx)) = self.hovered_item {
            let left_count = self
                .bindings
                .iter()
                .filter(|b| b.group.as_deref() != Some("command_palette"))
                .count();
            if idx >= left_count {
                self.hovered_item = None;
            }
        }
        ctx.post_message(FooterBindingsUpdated { count: self.bindings.len() });
        ctx.request_repaint();
    }

    fn left_binding_regions(&self) -> Vec<(Range<usize>, usize)> {
        let (left_items, _palette) = self.split_bindings();
        let separator = if self.compact { " " } else { "" };
        let separator_width = rich_rs::cell_len(separator);
        let mut pos = 0usize;
        let mut flat_index = 0usize;
        let mut regions = Vec::new();
        let base_style = self.base_style();

        for (index, item) in left_items.iter().enumerate() {
            if index > 0 {
                pos += separator_width;
            }
            match item {
                LeftBindingItem::Single(binding) => {
                    let binding_segments =
                        self.render_binding(binding, Some(flat_index), false, false, false);
                    let width = Segment::get_line_length(&binding_segments);
                    if width > 0 {
                        regions.push((pos..pos + width, flat_index));
                        pos += width;
                    }
                    flat_index += 1;
                }
                LeftBindingItem::Grouped { label, bindings } => {
                    let (grouped_segments, grouped_regions) =
                        self.render_group_with_regions(label, bindings, flat_index, base_style);
                    for (range, idx) in grouped_regions {
                        regions.push((range.start + pos..range.end + pos, idx));
                    }
                    pos += Segment::get_line_length(&grouped_segments);
                    flat_index += bindings.len();
                }
            }
        }
        regions
    }

    /// Find which binding (by flat index into `self.bindings`) is at the given
    /// content-local x coordinate. Returns `None` if no binding is at that position.
    ///
    /// This replicates the left-section layout logic from `render` to compute
    /// binding hit regions without storing mutable state.
    fn binding_index_at_x(&self, x: u16) -> Option<usize> {
        let x = x as usize;
        self.left_binding_regions()
            .into_iter()
            .find_map(|(range, flat_index)| range.contains(&x).then_some(flat_index))
    }

    fn left_segments_for_render(&self, base_style: rich_rs::Style) -> Vec<Segment> {
        let (left_bindings, _palette) = self.split_bindings();
        let separator = if self.compact { " " } else { "" };
        let mut left_segments = Vec::new();
        let mut flat_index: usize = 0;
        for (index, binding) in left_bindings.iter().enumerate() {
            if index > 0 {
                left_segments.push(Segment::styled(separator.to_string(), base_style));
            }
            match binding {
                LeftBindingItem::Single(binding) => {
                    left_segments.extend(self.render_binding(
                        binding,
                        Some(flat_index),
                        false,
                        false,
                        false,
                    ));
                    flat_index += 1;
                }
                LeftBindingItem::Grouped { label, bindings } => {
                    left_segments
                        .extend(self.render_group(label, bindings, flat_index, base_style));
                    flat_index += bindings.len();
                }
            }
        }
        left_segments
    }

    fn command_palette_binding(&self) -> Option<&FooterBinding> {
        self.bindings
            .iter()
            .find(|binding| binding.group.as_deref() == Some("command_palette"))
    }

    fn command_palette_region(&self, width: usize) -> Option<Range<usize>> {
        let palette_binding = self.command_palette_binding()?;
        let base_style = self.base_style();
        let left_segments = self.left_segments_for_render(base_style);
        let left_width = Segment::get_line_length(&left_segments);

        let palette_separator_style = self.palette_separator_style();
        let mut right_segments = self.render_binding(palette_binding, None, false, true, false);
        right_segments.insert(0, Segment::styled("▏".to_string(), palette_separator_style));
        let right_width = Segment::get_line_length(&right_segments);
        let (start, end) = if left_width + right_width < width {
            (width.saturating_sub(right_width), width)
        } else {
            (left_width, left_width + right_width)
        };
        if start >= width {
            return None;
        }
        Some(start..end.min(width))
    }

    fn hit_item_at_x(&self, x: u16) -> Option<HoveredFooterItem> {
        if let Some(flat_index) = self.binding_index_at_x(x) {
            return Some(HoveredFooterItem::Binding(flat_index));
        }
        let x = x as usize;
        if self
            .command_palette_region(self.layout_width.max(1))
            .is_some_and(|range| range.contains(&x))
        {
            return Some(HoveredFooterItem::CommandPalette);
        }
        None
    }

    /// Look up the `FooterBinding` at a flat index (skipping command_palette bindings).
    fn binding_at_flat_index(&self, flat_index: usize) -> Option<&FooterBinding> {
        self.bindings
            .iter()
            .filter(|b| b.group.as_deref() != Some("command_palette"))
            .nth(flat_index)
    }

    fn hovered_binding(&self) -> Option<&FooterBinding> {
        match self.hovered_item {
            Some(HoveredFooterItem::Binding(idx)) => self.binding_at_flat_index(idx),
            Some(HoveredFooterItem::CommandPalette) => self.command_palette_binding(),
            None => None,
        }
    }

    fn hovered_region(&self) -> Option<Range<usize>> {
        match self.hovered_item {
            Some(HoveredFooterItem::Binding(flat_index)) => self
                .left_binding_regions()
                .into_iter()
                .find_map(|(range, idx)| (idx == flat_index).then_some(range)),
            Some(HoveredFooterItem::CommandPalette) => {
                self.command_palette_region(self.layout_width.max(1))
            }
            None => None,
        }
    }

    fn hovered_anchor_local(&self) -> Option<(u16, u16)> {
        let range = self.hovered_region()?;
        if range.start >= range.end {
            return None;
        }
        let max_x = self.layout_width.max(1).saturating_sub(1);
        let start = range.start.min(max_x);
        let end = range.end.saturating_sub(1).min(max_x);
        let center = start.saturating_add(end.saturating_sub(start) / 2);
        Some((center.min(u16::MAX as usize) as u16, 0))
    }
}

#[derive(Debug, Clone)]
enum LeftBindingItem {
    Single(FooterBinding),
    Grouped {
        label: String,
        bindings: Vec<FooterBinding>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HoveredFooterItem {
    Binding(usize),
    CommandPalette,
}

impl Widget for Footer {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let base_style = self.base_style();
        let palette_separator_style = self.palette_separator_style();

        let mut line_segments = self.left_segments_for_render(base_style);
        if let Some(palette_binding) = self.command_palette_binding() {
            let mut right_segments = self.render_binding(palette_binding, None, false, true, false);
            // Keep command palette hint docked at the right with a subtle visible separator.
            right_segments.insert(0, Segment::styled("▏".to_string(), palette_separator_style));

            let left_width = Segment::get_line_length(&line_segments);
            let right_width = Segment::get_line_length(&right_segments);
            if left_width + right_width < width {
                line_segments.push(Segment::styled(
                    " ".repeat(width - left_width - right_width),
                    base_style,
                ));
                line_segments.extend(right_segments);
            } else {
                line_segments.extend(right_segments);
            }
        }

        // Ensure the footer row always carries the footer base style. This keeps
        // row background stable while preserving per-segment overrides (key colors,
        // command-palette separator, etc.).
        let line_segments: Vec<Segment> = line_segments
            .into_iter()
            .map(|mut seg| {
                if seg.control.is_none() {
                    let merged = match seg.style {
                        Some(style) => base_style.combine(&style),
                        None => base_style,
                    };
                    seg.style = Some(merged);
                }
                seg
            })
            .collect();

        let rendered = if line_segments.is_empty() {
            Text::plain(String::new()).render(console, options)
        } else {
            let mut out = Segments::new();
            out.extend(line_segments);
            out
        };
        let split = Segment::split_and_crop_lines(rendered, width, None, true, false);
        let mut out = Segments::new();
        if let Some(line) = split.first() {
            // Footer should always paint a full-width background row. Padding with
            // the footer base style avoids transient "black bar" artifacts when
            // binding sets shrink/expand between frames.
            out.extend(Segment::adjust_line_length(
                line,
                width,
                Some(base_style),
                false,
            ));
        } else {
            out.push(Segment::styled(" ".repeat(width), base_style));
        }
        out
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(Some(1))
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::AppFocus(active) => {
                self.app_focused = *active;
                if *active {
                    if let Some(next) = self.deferred_bindings.take() {
                        self.apply_bindings(next, ctx);
                    }
                }
            }
            Event::BindingsChanged(bindings) => {
                let next = Self::bindings_from_hints(bindings);
                if self.app_focused {
                    self.apply_bindings(next, ctx);
                } else {
                    self.deferred_bindings = Some(next);
                }
            }
            // Click-to-invoke: when a binding label is clicked, log the action key
            // for dispatch. Full key simulation requires runtime wiring.
            Event::MouseDown(mouse) => {
                if let Some(hit) = self.hit_item_at_x(mouse.x) {
                    let binding = match hit {
                        HoveredFooterItem::Binding(flat_index) => {
                            self.binding_at_flat_index(flat_index)
                        }
                        HoveredFooterItem::CommandPalette => self.command_palette_binding(),
                    };
                    if let Some(binding) = binding {
                        let action_key = binding.action_key.as_deref().unwrap_or(&binding.key);
                        debug_message(&format!(
                            "[footer] click binding key=\"{}\" action_key=\"{}\" desc=\"{}\"",
                            binding.key, action_key, binding.description
                        ));
                        ctx.post_message(Message::AppSimulateKey(crate::message::AppSimulateKey {
                            key: action_key.to_string(),
                        }));
                        ctx.set_handled();
                        ctx.request_repaint();
                    }
                }
            }
            Event::Leave(_) => {
                if self.hovered_item.take().is_some() {
                    ctx.request_repaint();
                }
            }
            _ => {}
        }
    }

    fn on_mouse_move(&mut self, x: u16, y: u16) -> bool {
        let next = if y == 0 { self.hit_item_at_x(x) } else { None };
        if self.hovered_item != next {
            self.hovered_item = next;
            return true;
        }
        false
    }

    fn set_hovered(&mut self, hovered: bool) {
        if !hovered {
            self.hovered_item = None;
        }
    }

    fn tooltip(&self) -> Option<String> {
        self.hovered_binding()
            .and_then(|binding| binding.tooltip.clone())
            .filter(|text| !text.trim().is_empty())
    }

    fn tooltip_anchor(&self) -> Option<(u16, u16)> {
        self.hovered_anchor_local()
    }

    fn on_layout(&mut self, width: u16, _height: u16) {
        self.layout_width = width.max(1) as usize;
    }

    fn on_unmount(&mut self) {
        self.app_focused = true;
        self.deferred_bindings = None;
        self.hovered_item = None;
    }

    fn style_classes(&self) -> &[String] {
        if self.classes.is_empty() {
            empty_classes()
        } else {
            &self.classes
        }
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for Footer {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

impl ReactiveWidget for Footer {
    fn reactive_dispatch(&mut self, changes: &[ReactiveChange], ctx: &mut ReactiveCtx) {
        for change in changes {
            match change.field_name {
                "compact" => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<bool>(),
                        change.new_value.downcast_ref::<bool>(),
                    ) {
                        self.watch_compact(old, new, ctx);
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use rich_rs::Console;

    use super::Footer;
    use crate::css::{default_widget_stylesheet, set_style_context};
    use crate::event::{BindingHint, Event, EventCtx, MouseDownEvent};
    use crate::message::*;
    use crate::node_id::NodeId;
    use crate::render::FrameBuffer;
    use crate::widgets::Widget;

    #[test]
    fn bindings_changed_posts_footer_bindings_updated_message() {
        let mut footer = Footer::new();
        let mut ctx = EventCtx::default();
        footer.on_event(
            &Event::BindingsChanged(vec![BindingHint::new("ctrl+p", "Palette")]),
            &mut ctx,
        );
        let messages = ctx.take_messages();
        assert!(messages.iter().any(|m| m
            .downcast_ref::<FooterBindingsUpdated>()
            .map_or(false, |f| f.count == 1)));
    }

    #[test]
    fn identical_bindings_changed_is_noop() {
        let mut footer = Footer::new();
        let mut first_ctx = EventCtx::default();
        let hints = vec![BindingHint::new("ctrl+p", "Palette")];
        footer.on_event(&Event::BindingsChanged(hints.clone()), &mut first_ctx);
        assert!(!first_ctx.take_messages().is_empty());

        let mut second_ctx = EventCtx::default();
        footer.on_event(&Event::BindingsChanged(hints), &mut second_ctx);
        assert!(second_ctx.take_messages().is_empty());
    }

    #[test]
    fn bindings_changed_defers_while_app_unfocused() {
        let mut footer = Footer::new();
        let mut unfocus_ctx = EventCtx::default();
        footer.on_event(&Event::AppFocus(false), &mut unfocus_ctx);
        assert!(unfocus_ctx.take_messages().is_empty());
        assert!(!unfocus_ctx.repaint_requested());

        let mut bindings_ctx = EventCtx::default();
        footer.on_event(
            &Event::BindingsChanged(vec![BindingHint::new("ctrl+p", "Palette")]),
            &mut bindings_ctx,
        );
        assert!(bindings_ctx.take_messages().is_empty());
        assert!(!bindings_ctx.repaint_requested());
    }

    #[test]
    fn focus_gain_applies_latest_deferred_bindings_once() {
        let mut footer = Footer::new();
        let mut unfocus_ctx = EventCtx::default();
        footer.on_event(&Event::AppFocus(false), &mut unfocus_ctx);

        let mut first_ctx = EventCtx::default();
        footer.on_event(
            &Event::BindingsChanged(vec![BindingHint::new("a", "alpha")]),
            &mut first_ctx,
        );
        assert!(first_ctx.take_messages().is_empty());

        let mut second_ctx = EventCtx::default();
        footer.on_event(
            &Event::BindingsChanged(vec![
                BindingHint::new("a", "alpha"),
                BindingHint::new("b", "bravo"),
            ]),
            &mut second_ctx,
        );
        assert!(second_ctx.take_messages().is_empty());

        let mut focus_ctx = EventCtx::default();
        footer.on_event(&Event::AppFocus(true), &mut focus_ctx);
        let messages = focus_ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].is::<FooterBindingsUpdated>());
        assert_eq!(messages[0].downcast_ref::<FooterBindingsUpdated>().unwrap().count, 2);
        assert!(focus_ctx.repaint_requested());
    }

    #[test]
    fn repeated_focus_loss_does_not_drop_deferred_bindings() {
        let mut footer = Footer::new();
        let mut ctx = EventCtx::default();
        footer.on_event(&Event::AppFocus(false), &mut ctx);
        footer.on_event(
            &Event::BindingsChanged(vec![BindingHint::new("ctrl+p", "Palette")]),
            &mut ctx,
        );
        footer.on_event(&Event::AppFocus(false), &mut ctx);

        let mut focus_ctx = EventCtx::default();
        footer.on_event(&Event::AppFocus(true), &mut focus_ctx);
        let messages = focus_ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].is::<FooterBindingsUpdated>());
        assert_eq!(messages[0].downcast_ref::<FooterBindingsUpdated>().unwrap().count, 1);
    }

    #[test]
    fn unmount_resets_focus_tracking_state() {
        let mut footer = Footer::new();
        let mut unfocus_ctx = EventCtx::default();
        footer.on_event(&Event::AppFocus(false), &mut unfocus_ctx);
        footer.on_unmount();

        let mut ctx = EventCtx::default();
        footer.on_event(
            &Event::BindingsChanged(vec![BindingHint::new("ctrl+p", "Palette")]),
            &mut ctx,
        );
        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].is::<FooterBindingsUpdated>());
        assert_eq!(messages[0].downcast_ref::<FooterBindingsUpdated>().unwrap().count, 1);
    }

    // ── WP-22: Footer Signal subscription + click-to-invoke ─────────────

    #[test]
    fn bindings_from_hints_stores_action_key() {
        let mut footer = Footer::new();
        let mut ctx = EventCtx::default();
        let hints = vec![BindingHint::new("ctrl+s", "Save").with_key_display("^s")];
        footer.on_event(&Event::BindingsChanged(hints), &mut ctx);

        // The displayed key should be the key_display ("^s"), not the raw key.
        assert_eq!(footer.bindings[0].key, "^s");
        // The action_key should store the raw key spec.
        assert_eq!(footer.bindings[0].action_key.as_deref(), Some("ctrl+s"));
    }

    #[test]
    fn binding_index_at_x_finds_first_binding() {
        let footer = Footer::new()
            .with_binding("^q", "Quit")
            .with_binding("^s", "Save");
        // In non-compact mode, first binding starts at x=0:
        //   " ^q Quit" = 8 chars, then "   " separator (3), then " ^s Save"
        // So clicking at x=0 should hit the first binding.
        assert_eq!(footer.binding_index_at_x(0), Some(0));
    }

    #[test]
    fn binding_index_at_x_finds_second_binding() {
        let footer = Footer::new()
            .with_binding("^q", "Quit")
            .with_binding("^s", "Save");
        // First binding: " ^q Quit" = 8 chars
        // Separator: "   " = 3 chars
        // Second binding starts at x=11
        assert_eq!(footer.binding_index_at_x(11), Some(1));
    }

    #[test]
    fn binding_index_at_x_returns_none_past_bindings() {
        let footer = Footer::new().with_binding("^q", "Quit");
        // " ^q Quit" = 8 chars, so x=8 is past the binding.
        assert_eq!(footer.binding_index_at_x(50), None);
    }

    #[test]
    fn click_on_binding_is_handled() {
        let mut footer = Footer::new()
            .with_binding("^q", "Quit")
            .with_binding("^s", "Save");
        let mut ctx = EventCtx::default();

        // Click at x=0 should hit the first binding.
        footer.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: NodeId::default(),
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut ctx,
        );

        assert!(ctx.handled());
        assert!(ctx.repaint_requested());
        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            &messages[0].message,
            Message::AppSimulateKey(crate::message::AppSimulateKey { key }) if key == "^q"
        ));
    }

    #[test]
    fn click_distinguishes_each_binding_key() {
        let mut footer = Footer::new()
            .with_binding("l", "Leto")
            .with_binding("j", "Jessica")
            .with_binding("p", "Paul");

        let regions = footer.left_binding_regions();
        let clicks = regions
            .iter()
            .take(3)
            .map(|(range, idx)| ((range.start + range.end) / 2, *idx))
            .collect::<Vec<_>>();
        for (x, idx) in clicks {
            let expected_key = match idx {
                0 => "l",
                1 => "j",
                2 => "p",
                _ => panic!("unexpected binding index {idx}"),
            };
            let mut ctx = EventCtx::default();
            footer.on_event(
                &Event::MouseDown(MouseDownEvent {
                    target: NodeId::default(),
                    screen_x: x as u16,
                    screen_y: 0,
                    x: x as u16,
                    y: 0,
                }),
                &mut ctx,
            );
            let messages = ctx.take_messages();
            assert!(
                messages.iter().any(|event| matches!(
                    &event.message,
                    Message::AppSimulateKey(crate::message::AppSimulateKey { key }) if key == expected_key
                )),
                "click at x={x} should emit {expected_key:?}"
            );
        }
    }

    #[test]
    fn click_grouped_binding_targets_specific_key() {
        let mut footer = Footer::new();
        let mut setup_ctx = EventCtx::default();
        footer.on_event(
            &Event::BindingsChanged(vec![
                BindingHint::new("a", "left").with_group("Move"),
                BindingHint::new("b", "right").with_group("Move"),
            ]),
            &mut setup_ctx,
        );

        let second_region = footer
            .left_binding_regions()
            .into_iter()
            .find(|(_, idx)| *idx == 1)
            .expect("second grouped binding region should exist");
        let x = ((second_region.0.start + second_region.0.end) / 2) as u16;

        let mut ctx = EventCtx::default();
        footer.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: NodeId::default(),
                screen_x: x,
                screen_y: 0,
                x,
                y: 0,
            }),
            &mut ctx,
        );
        let messages = ctx.take_messages();
        assert!(
            messages.iter().any(|event| matches!(
                &event.message,
                Message::AppSimulateKey(crate::message::AppSimulateKey { key }) if key == "b"
            )),
            "second grouped key click should emit key 'b'"
        );
    }

    #[test]
    fn click_command_palette_binding_is_handled() {
        let mut footer = Footer::new();
        let mut setup_ctx = EventCtx::default();
        footer.on_event(
            &Event::BindingsChanged(vec![
                BindingHint::new("j", "Jessica"),
                BindingHint::new("ctrl+p", "palette")
                    .with_key_display("^p")
                    .with_group("command_palette"),
            ]),
            &mut setup_ctx,
        );
        footer.on_layout(64, 1);
        let palette_range = footer
            .command_palette_region(64)
            .expect("palette region should exist");
        let x = ((palette_range.start + palette_range.end) / 2) as u16;

        let mut ctx = EventCtx::default();
        footer.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: NodeId::default(),
                screen_x: x,
                screen_y: 0,
                x,
                y: 0,
            }),
            &mut ctx,
        );
        let messages = ctx.take_messages();
        assert!(
            messages.iter().any(|event| matches!(
                &event.message,
                Message::AppSimulateKey(crate::message::AppSimulateKey { key }) if key == "ctrl+p"
            )),
            "command palette click should emit raw key spec ctrl+p"
        );
    }

    #[test]
    fn click_binding_uses_raw_action_key_when_display_differs() {
        let mut footer = Footer::new();
        let mut setup_ctx = EventCtx::default();
        footer.on_event(
            &Event::BindingsChanged(vec![
                BindingHint::new("ctrl+p", "Palette").with_key_display("^p"),
            ]),
            &mut setup_ctx,
        );
        let mut ctx = EventCtx::default();
        footer.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: NodeId::default(),
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut ctx,
        );
        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            &messages[0].message,
            Message::AppSimulateKey(crate::message::AppSimulateKey { key }) if key == "ctrl+p"
        ));
    }

    #[test]
    fn click_past_bindings_is_not_handled() {
        let mut footer = Footer::new().with_binding("^q", "Quit");
        let mut ctx = EventCtx::default();

        // Click way past the binding region.
        footer.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: NodeId::default(),
                screen_x: 50,
                screen_y: 0,
                x: 50,
                y: 0,
            }),
            &mut ctx,
        );

        assert!(!ctx.handled());
    }

    #[test]
    fn footer_binding_with_action_key_builder() {
        use super::FooterBinding;
        let binding = FooterBinding::new("^s", "Save").with_action_key("ctrl+s");
        assert_eq!(binding.action_key.as_deref(), Some("ctrl+s"));
    }

    #[test]
    fn binding_index_at_x_compact_mode() {
        let footer = Footer::new()
            .compact(true)
            .with_binding("^q", "Quit")
            .with_binding("^s", "Save");
        let regions = footer.left_binding_regions();
        let first = regions
            .iter()
            .find(|(_, idx)| *idx == 0)
            .expect("first binding region")
            .0
            .clone();
        let second = regions
            .iter()
            .find(|(_, idx)| *idx == 1)
            .expect("second binding region")
            .0
            .clone();
        let first_mid = ((first.start + first.end) / 2) as u16;
        let second_mid = ((second.start + second.end) / 2) as u16;
        assert_eq!(footer.binding_index_at_x(first_mid), Some(0));
        assert_eq!(footer.binding_index_at_x(second_mid), Some(1));
    }

    #[test]
    fn command_palette_separator_hugs_key_hint() {
        let _guard = set_style_context(default_widget_stylesheet());
        let mut footer = Footer::new();
        let mut setup_ctx = EventCtx::default();
        footer.on_event(
            &Event::BindingsChanged(vec![
                BindingHint::new("j", "Jessica"),
                BindingHint::new("ctrl+p", "palette")
                    .with_key_display("^p")
                    .with_group("command_palette"),
            ]),
            &mut setup_ctx,
        );

        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (64, 1);
        options.max_width = 64;
        options.max_height = 1;

        let buf = FrameBuffer::from_renderable(&console, &options, &footer, None);
        let line = &buf.as_plain_lines()[0];
        let chars: Vec<char> = line.chars().collect();
        let separator_idx = chars
            .iter()
            .position(|ch| *ch == '▏')
            .expect("expected command palette separator");
        let caret_idx = chars
            .iter()
            .position(|ch| *ch == '^')
            .expect("expected command palette key hint");
        assert_eq!(
            caret_idx,
            separator_idx + 1,
            "separator should sit immediately before the command palette key hint"
        );
    }

    #[test]
    fn non_compact_footer_binding_spacing_matches_python_pattern() {
        let _guard = set_style_context(default_widget_stylesheet());
        let mut footer = Footer::new();
        let mut setup_ctx = EventCtx::default();
        footer.on_event(
            &Event::BindingsChanged(vec![
                BindingHint::new("l", "Leto"),
                BindingHint::new("j", "Jessica"),
                BindingHint::new("p", "Paul"),
            ]),
            &mut setup_ctx,
        );
        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (64, 1);
        options.max_width = 64;
        options.max_height = 1;

        let buf = FrameBuffer::from_renderable(&console, &options, &footer, None);
        let line = buf.as_plain_lines()[0].trim_end().to_string();
        assert!(
            line.contains("l Leto  j Jessica  p Paul"),
            "non-compact footer spacing should keep bindings tight like Python; got: {line:?}"
        );
    }

    #[test]
    fn command_palette_hover_applies_to_separator_cell() {
        let _guard = set_style_context(default_widget_stylesheet());
        let mut footer = Footer::new();
        let mut setup_ctx = EventCtx::default();
        footer.on_event(
            &Event::BindingsChanged(vec![
                BindingHint::new("j", "Jessica"),
                BindingHint::new("ctrl+p", "palette")
                    .with_key_display("^p")
                    .with_group("command_palette"),
            ]),
            &mut setup_ctx,
        );
        footer.on_layout(64, 1);
        let range = footer
            .command_palette_region(64)
            .expect("command palette region should exist");
        let sep_x = range.start as u16;
        let key_x = (range.start + 1) as u16;
        assert!(footer.on_mouse_move(key_x, 0), "hover should update state");

        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (64, 1);
        options.max_width = 64;
        options.max_height = 1;
        let buf = FrameBuffer::from_renderable(&console, &options, &footer, None);

        let sep_bg = buf
            .get(sep_x as usize, 0)
            .style
            .and_then(|style| style.bgcolor);
        let key_bg = buf
            .get(key_x as usize, 0)
            .style
            .and_then(|style| style.bgcolor);
        assert_eq!(
            sep_bg, key_bg,
            "separator should share hovered command-palette background"
        );
    }

    #[test]
    fn tooltip_anchor_uses_hovered_left_binding_region_center() {
        let mut footer = Footer::new();
        let mut setup_ctx = EventCtx::default();
        footer.on_event(
            &Event::BindingsChanged(vec![
                BindingHint::new("q", "Quit").with_tooltip("Quit the app"),
                BindingHint::new("h", "Help"),
            ]),
            &mut setup_ctx,
        );
        footer.on_layout(80, 1);
        let first_region = footer
            .left_binding_regions()
            .into_iter()
            .find(|(_, idx)| *idx == 0)
            .expect("first binding region should exist")
            .0;
        let hover_x = ((first_region.start + first_region.end) / 2) as u16;
        assert!(footer.on_mouse_move(hover_x, 0));
        let anchor = footer
            .tooltip_anchor()
            .expect("hovered binding should expose tooltip anchor");
        let expected_x = (first_region.start + first_region.end.saturating_sub(1)) / 2;
        assert_eq!(anchor, (expected_x as u16, 0));
    }

    #[test]
    fn tooltip_anchor_uses_hovered_command_palette_region_center() {
        let mut footer = Footer::new();
        let mut setup_ctx = EventCtx::default();
        footer.on_event(
            &Event::BindingsChanged(vec![
                BindingHint::new("j", "Jessica"),
                BindingHint::new("ctrl+p", "palette")
                    .with_key_display("^p")
                    .with_group("command_palette")
                    .with_tooltip("Open command palette"),
            ]),
            &mut setup_ctx,
        );
        footer.on_layout(80, 1);
        let range = footer
            .command_palette_region(80)
            .expect("command palette region should exist");
        let hover_x = ((range.start + range.end) / 2) as u16;
        assert!(footer.on_mouse_move(hover_x, 0));
        let anchor = footer
            .tooltip_anchor()
            .expect("hovered command palette should expose tooltip anchor");
        let expected_x = (range.start + range.end.saturating_sub(1)) / 2;
        assert_eq!(anchor, (expected_x as u16, 0));
    }

    // ── check_action / enabled state tests ──────────────────────────────

    #[test]
    fn check_action_none_dims_binding() {
        let _guard = set_style_context(default_widget_stylesheet());
        let mut footer = Footer::new();
        let mut ctx = EventCtx::default();
        let mut hint = BindingHint::new("b", "Back");
        hint.enabled = None; // disabled but visible (dimmed)
        footer.on_event(&Event::BindingsChanged(vec![hint]), &mut ctx);

        // Binding should still be present (not filtered out).
        assert_eq!(footer.bindings.len(), 1);
        // The binding's enabled state should be None.
        assert!(footer.bindings[0].enabled.is_none());
    }

    #[test]
    fn check_action_false_hides_binding() {
        let mut footer = Footer::new();
        let mut ctx = EventCtx::default();
        let mut hint = BindingHint::new("x", "Hidden");
        hint.enabled = Some(false); // hidden entirely
        footer.on_event(&Event::BindingsChanged(vec![hint]), &mut ctx);

        // Binding should be filtered out.
        assert!(
            footer.bindings.is_empty(),
            "binding with enabled=Some(false) should be hidden"
        );
    }

    #[test]
    fn check_action_default_shows_binding_normally() {
        let mut footer = Footer::new();
        let mut ctx = EventCtx::default();
        let hint = BindingHint::new("q", "Quit"); // enabled=Some(true) by default
        footer.on_event(&Event::BindingsChanged(vec![hint]), &mut ctx);

        assert_eq!(footer.bindings.len(), 1);
        assert_eq!(footer.bindings[0].enabled, Some(true));
    }
}
