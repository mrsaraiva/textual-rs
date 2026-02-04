use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};

use rich_rs::{Console, ConsoleOptions, Segment, Segments};

use crate::debug::DebugLayout;
use crate::event::{Event, EventCtx};
use crate::style::{Color, Style};

mod controls;
mod containers;
mod layout;
mod text;

pub use controls::{
    Button, Checkbox, DataTable, Input, ListView, Spacer, Tab, Tabs, Tree, TreeNode,
};
pub use containers::{
    AppRoot, Constrained, Container, Frame, Node, Overlay, Panel, ScrollView, Styled,
};
pub use layout::{Dock, DockItem, DockKind, Grid, Row, RowAlign};
pub use text::{Label, Markdown};

thread_local! {
    static STYLE_CONTEXT: RefCell<Option<StyleSheet>> = RefCell::new(None);
    static STYLE_STACK: RefCell<Vec<Style>> = RefCell::new(Vec::new());
    static SELECTOR_STACK: RefCell<Vec<SelectorMeta>> = RefCell::new(Vec::new());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct WidgetId(u64);

impl WidgetId {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

pub trait Widget: Send + Sync {
    fn id(&self) -> WidgetId;
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments;
    fn render_styled_dyn_obj(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: Option<&DebugLayout>,
    ) -> Segments {
        let meta = selector_meta_generic(self);
        let resolved = resolve_style(self, &meta);
        let segments = STYLE_STACK.with(|style_stack| {
            SELECTOR_STACK.with(|selector_stack| {
                style_stack.borrow_mut().push(resolved);
                selector_stack.borrow_mut().push(meta);
                let rendered = match debug {
                    Some(debug) => self.render_with_debug(console, options, debug),
                    None => self.render(console, options),
                };
                selector_stack.borrow_mut().pop();
                style_stack.borrow_mut().pop();
                rendered
            })
        });
        apply_style_to_segments(segments, resolved)
    }
    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        _debug: &DebugLayout,
    ) -> Segments {
        self.render(console, options)
    }
    fn on_mount(&mut self) {}
    fn on_unmount(&mut self) {}
    fn on_tick(&mut self, _tick: u64) {}
    fn on_resize(&mut self, _width: u16, _height: u16) {}
    fn on_event_capture(&mut self, _event: &Event, _ctx: &mut EventCtx) {}
    fn on_event(&mut self, _event: &Event, _ctx: &mut EventCtx) {}
    fn visit_children_mut(&mut self, _f: &mut dyn FnMut(&mut dyn Widget)) {}
    fn focusable(&self) -> bool {
        false
    }
    fn set_focus(&mut self, _focused: bool) {}
    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints())
    }
    fn layout_constraints(&self) -> LayoutConstraints {
        self.styles()
            .map(|styles| styles.layout)
            .unwrap_or_default()
    }
    fn style(&self) -> Option<Style> {
        self.styles().map(|styles| styles.style)
    }
    fn styles(&self) -> Option<&WidgetStyles> {
        None
    }
    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        None
    }
    fn style_type(&self) -> &'static str {
        std::any::type_name::<Self>()
            .rsplit("::")
            .next()
            .unwrap_or("Widget")
    }
    fn style_id(&self) -> Option<&str> {
        None
    }
    fn style_classes(&self) -> &[String] {
        empty_classes()
    }
    fn render_styled(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.render_styled_dyn_obj(console, options, None)
    }
    fn render_styled_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        self.render_styled_dyn_obj(console, options, Some(debug))
    }
    fn set_width(&mut self, value: usize) {
        if let Some(styles) = self.styles_mut() {
            *styles = styles.width(value);
        }
    }

    fn set_height(&mut self, value: usize) {
        if let Some(styles) = self.styles_mut() {
            *styles = styles.height(value);
        }
    }

    fn set_min_width(&mut self, value: usize) {
        if let Some(styles) = self.styles_mut() {
            *styles = styles.min_width(value);
        }
    }

    fn set_max_width(&mut self, value: usize) {
        if let Some(styles) = self.styles_mut() {
            *styles = styles.max_width(value);
        }
    }

    fn set_min_height(&mut self, value: usize) {
        if let Some(styles) = self.styles_mut() {
            *styles = styles.min_height(value);
        }
    }

    fn set_max_height(&mut self, value: usize) {
        if let Some(styles) = self.styles_mut() {
            *styles = styles.max_height(value);
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LayoutConstraints {
    pub min_width: Option<usize>,
    pub max_width: Option<usize>,
    pub min_height: Option<usize>,
    pub max_height: Option<usize>,
}

impl LayoutConstraints {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn min_width(mut self, value: usize) -> Self {
        self.min_width = Some(value.max(1));
        self
    }

    pub fn max_width(mut self, value: usize) -> Self {
        self.max_width = Some(value.max(1));
        self
    }

    pub fn min_height(mut self, value: usize) -> Self {
        self.min_height = Some(value.max(1));
        self
    }

    pub fn max_height(mut self, value: usize) -> Self {
        self.max_height = Some(value.max(1));
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct StyleSelector {
    type_name: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
}

impl StyleSelector {
    pub fn new(type_name: impl Into<String>) -> Self {
        Self {
            type_name: Some(type_name.into()),
            id: None,
            classes: Vec::new(),
        }
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.classes.push(class.into());
        self
    }

    fn matches(&self, meta: &SelectorMeta) -> bool {
        if let Some(type_name) = &self.type_name {
            if meta.type_name != *type_name {
                return false;
            }
        }
        if let Some(id) = &self.id {
            if meta.id.as_deref() != Some(id.as_str()) {
                return false;
            }
        }
        if !self.classes.is_empty() {
            if !self
                .classes
                .iter()
                .all(|class| meta.classes.iter().any(|value| value == class))
            {
                return false;
            }
        }
        true
    }

    fn specificity(&self) -> u8 {
        let mut score = 0u8;
        if self.type_name.is_some() {
            score += 1;
        }
        score = score.saturating_add(self.classes.len().saturating_mul(10) as u8);
        if self.id.is_some() {
            score = score.saturating_add(100);
        }
        score
    }
}

#[derive(Debug, Clone, Copy)]
enum Combinator {
    Descendant,
    Child,
}

#[derive(Debug, Clone)]
struct SelectorChain {
    parts: Vec<StyleSelector>,
    combinators: Vec<Combinator>,
}

#[derive(Debug, Clone)]
pub struct StyleRule {
    selector_chain: SelectorChain,
    style: Style,
}

#[derive(Debug, Clone, Default)]
pub struct StyleSheet {
    rules: Vec<StyleRule>,
}

impl StyleSheet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_rule(&mut self, selector: StyleSelector, style: Style) {
        self.rules.push(StyleRule {
            selector_chain: SelectorChain {
                parts: vec![selector],
                combinators: Vec::new(),
            },
            style,
        });
    }

    pub fn add_type(&mut self, name: impl Into<String>, style: Style) {
        self.add_rule(StyleSelector::new(name), style);
    }

    pub fn add_id(&mut self, id: impl Into<String>, style: Style) {
        self.add_rule(StyleSelector::default().id(id), style);
    }

    pub fn add_class(&mut self, class: impl Into<String>, style: Style) {
        self.add_rule(StyleSelector::default().class(class), style);
    }

    fn style_for<T: Widget + ?Sized>(&self, _widget: &T, meta: &SelectorMeta) -> Style {
        let mut matches: Vec<(u8, usize, Style)> = Vec::new();
        for (idx, rule) in self.rules.iter().enumerate() {
            if let Some(score) = rule_specificity(rule, meta) {
                matches.push((score, idx, rule.style));
            }
        }
        matches.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        let mut out = Style::new();
        for (_, _, style) in matches {
            out = out.combine(&style);
        }
        out
    }

    pub fn parse(input: &str) -> Self {
        let mut sheet = StyleSheet::new();
        let mut rest = input;
        while let Some(start) = rest.find('{') {
            let selector = rest[..start].trim();
            let after = &rest[start + 1..];
            let end = match after.find('}') {
                Some(pos) => pos,
                None => break,
            };
            let body = &after[..end];
            let style = parse_style_body(body);
            if !style.is_empty() {
                for selector_chain in parse_selector_list(selector) {
                    sheet.rules.push(StyleRule {
                        selector_chain,
                        style,
                    });
                }
            }
            rest = &after[end + 1..];
        }
        sheet
    }
}

pub struct StyleContextGuard;

pub fn set_style_context(stylesheet: StyleSheet) -> StyleContextGuard {
    STYLE_CONTEXT.with(|ctx| {
        *ctx.borrow_mut() = Some(stylesheet);
    });
    StyleContextGuard
}

impl Drop for StyleContextGuard {
    fn drop(&mut self) {
        STYLE_CONTEXT.with(|ctx| {
            *ctx.borrow_mut() = None;
        });
    }
}



#[derive(Debug, Clone, Copy, Default)]
pub struct WidgetStyles {
    pub style: Style,
    pub layout: LayoutConstraints,
}

impl WidgetStyles {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fg(mut self, color: Color) -> Self {
        self.style = self.style.fg(color);
        self
    }

    pub fn bg(mut self, color: Color) -> Self {
        self.style = self.style.bg(color);
        self
    }

    pub fn bold(mut self, value: bool) -> Self {
        self.style = self.style.bold(value);
        self
    }

    pub fn dim(mut self, value: bool) -> Self {
        self.style = self.style.dim(value);
        self
    }

    pub fn italic(mut self, value: bool) -> Self {
        self.style = self.style.italic(value);
        self
    }

    pub fn underline(mut self, value: bool) -> Self {
        self.style = self.style.underline(value);
        self
    }

    pub fn border(mut self, value: bool) -> Self {
        self.style = self.style.border(value);
        self
    }

    pub fn set_fg(&mut self, color: Color) {
        self.style = self.style.fg(color);
    }

    pub fn set_bg(&mut self, color: Color) {
        self.style = self.style.bg(color);
    }

    pub fn set_bold(&mut self, value: bool) {
        self.style = self.style.bold(value);
    }

    pub fn set_dim(&mut self, value: bool) {
        self.style = self.style.dim(value);
    }

    pub fn set_italic(&mut self, value: bool) {
        self.style = self.style.italic(value);
    }

    pub fn set_underline(&mut self, value: bool) {
        self.style = self.style.underline(value);
    }

    pub fn set_border(&mut self, value: bool) {
        self.style = self.style.border(value);
    }

    pub fn width(mut self, value: usize) -> Self {
        let value = value.max(1);
        self.layout.min_width = Some(value);
        self.layout.max_width = Some(value);
        self
    }

    pub fn height(mut self, value: usize) -> Self {
        let value = value.max(1);
        self.layout.min_height = Some(value);
        self.layout.max_height = Some(value);
        self
    }

    pub fn min_width(mut self, value: usize) -> Self {
        self.layout.min_width = Some(value.max(1));
        self
    }

    pub fn max_width(mut self, value: usize) -> Self {
        self.layout.max_width = Some(value.max(1));
        self
    }

    pub fn min_height(mut self, value: usize) -> Self {
        self.layout.min_height = Some(value.max(1));
        self
    }

    pub fn max_height(mut self, value: usize) -> Self {
        self.layout.max_height = Some(value.max(1));
        self
    }

    pub fn set_width(&mut self, value: usize) {
        let value = value.max(1);
        self.layout.min_width = Some(value);
        self.layout.max_width = Some(value);
    }

    pub fn set_height(&mut self, value: usize) {
        let value = value.max(1);
        self.layout.min_height = Some(value);
        self.layout.max_height = Some(value);
    }

    pub fn set_min_width(&mut self, value: usize) {
        self.layout.min_width = Some(value.max(1));
    }

    pub fn set_max_width(&mut self, value: usize) {
        self.layout.max_width = Some(value.max(1));
    }

    pub fn set_min_height(&mut self, value: usize) {
        self.layout.min_height = Some(value.max(1));
    }

    pub fn set_max_height(&mut self, value: usize) {
        self.layout.max_height = Some(value.max(1));
    }
}

fn merge_constraints(primary: LayoutConstraints, fallback: LayoutConstraints) -> LayoutConstraints {
    LayoutConstraints {
        min_width: primary.min_width.or(fallback.min_width),
        max_width: primary.max_width.or(fallback.max_width),
        min_height: primary.min_height.or(fallback.min_height),
        max_height: primary.max_height.or(fallback.max_height),
    }
}

fn fixed_height_from_constraints(constraints: LayoutConstraints) -> Option<usize> {
    match (constraints.min_height, constraints.max_height) {
        (Some(min), Some(max)) if min == max => Some(min),
        _ => None,
    }
}

fn clamp_with_constraints(
    value: usize,
    min: Option<usize>,
    max: Option<usize>,
    limit: usize,
) -> usize {
    let mut out = value.max(1);
    if let Some(min) = min {
        out = out.max(min);
    }
    if let Some(max) = max {
        out = out.min(max);
    }
    out.min(limit.max(1))
}

fn pad_lines_to_width(lines: Vec<Vec<Segment>>, width: usize) -> Vec<Vec<Segment>> {
    lines
        .into_iter()
        .map(|line| Segment::adjust_line_length(&line, width, None, true))
        .collect()
}

fn empty_classes() -> &'static [String] {
    use std::sync::OnceLock;
    static EMPTY: OnceLock<Vec<String>> = OnceLock::new();
    EMPTY.get_or_init(Vec::new)
}

fn focused_classes() -> &'static [String] {
    use std::sync::OnceLock;
    static FOCUSED: OnceLock<Vec<String>> = OnceLock::new();
    FOCUSED.get_or_init(|| vec!["focused".to_string()])
}

#[derive(Debug, Clone)]
struct SelectorMeta {
    type_name: String,
    id: Option<String>,
    classes: Vec<String>,
}

fn selector_meta_generic<T: Widget + ?Sized>(widget: &T) -> SelectorMeta {
    SelectorMeta {
        type_name: widget.style_type().to_string(),
        id: widget.style_id().map(|value| value.to_string()),
        classes: widget.style_classes().to_vec(),
    }
}

fn resolve_style<T: Widget + ?Sized>(widget: &T, meta: &SelectorMeta) -> Style {
    let sheet_style = STYLE_CONTEXT
        .with(|ctx| ctx.borrow().as_ref().map(|sheet| sheet.style_for(widget, meta)))
        .unwrap_or_default();
    let mut style = sheet_style;
    if let Some(inline) = widget.style() {
        style = style.combine(&inline);
    }
    if let Some(parent) = STYLE_STACK.with(|stack| stack.borrow().last().copied()) {
        style = style.inherit_from(&parent);
    }
    style
}

fn apply_style_to_segments(segments: Segments, style: Style) -> Segments {
    if style.is_empty() {
        return segments;
    }
    let rich_style = style.to_rich();
    segments
        .into_iter()
        .map(|mut seg| {
            if seg.control.is_some() {
                return seg;
            }
            if let Some(rich_style) = rich_style {
                seg.style = Some(match seg.style {
                    Some(existing) => existing.combine(&rich_style),
                    None => rich_style,
                });
            }
            seg
        })
        .collect()
}

fn rule_specificity(rule: &StyleRule, meta: &SelectorMeta) -> Option<u8> {
    if rule.selector_chain.parts.is_empty() {
        return None;
    }
    let last = rule.selector_chain.parts.last().unwrap();
    if !last.matches(meta) {
        return None;
    }
    if rule.selector_chain.parts.len() == 1 {
        return Some(last.specificity());
    }

    let stack_snapshot = SELECTOR_STACK.with(|stack| stack.borrow().clone());
    let mut idx = stack_snapshot.len() as isize - 2;
    let combinators = &rule.selector_chain.combinators;
    let parts = &rule.selector_chain.parts;
    for (part_index, selector) in parts[..parts.len() - 1].iter().rev().enumerate() {
        let comb = combinators[combinators.len() - 1 - part_index];
        match comb {
            Combinator::Child => {
                if idx < 0 {
                    return None;
                }
                let meta = &stack_snapshot[idx as usize];
                if !selector.matches(meta) {
                    return None;
                }
                idx -= 1;
            }
            Combinator::Descendant => {
                let mut found = false;
                let mut current = idx;
                while current >= 0 {
                    let meta = &stack_snapshot[current as usize];
                    if selector.matches(meta) {
                        found = true;
                        idx = current - 1;
                        break;
                    }
                    current -= 1;
                }
                if !found {
                    return None;
                }
            }
        }
    }

    let score = parts.iter().map(|part| part.specificity()).sum();
    Some(score)
}

fn parse_selector_list(selector: &str) -> Vec<SelectorChain> {
    let mut groups = Vec::new();
    for group in selector.split(',') {
        let group = group.trim();
        if group.is_empty() {
            continue;
        }
        if let Some(chain) = parse_selector_chain(group) {
            groups.push(chain);
        }
    }
    groups
}

fn parse_selector(selector: &str) -> Option<StyleSelector> {
    let selector = selector.trim();
    if selector.is_empty() {
        return None;
    }

    let mut type_name: Option<String> = None;
    let mut id: Option<String> = None;
    let mut classes: Vec<String> = Vec::new();

    let mut chars = selector.chars().peekable();
    let mut current = String::new();
    let mut mode: Option<char> = None;

    while let Some(ch) = chars.next() {
        match ch {
            '#' | '.' => {
                if mode.is_none() && !current.is_empty() {
                    type_name = Some(current.clone());
                } else if let Some(mode) = mode {
                    match mode {
                        '#' => id = Some(current.clone()),
                        '.' => classes.push(current.clone()),
                        _ => {}
                    }
                }
                current.clear();
                mode = Some(ch);
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        match mode {
            None => type_name = Some(current),
            Some('#') => id = Some(current),
            Some('.') => classes.push(current),
            _ => {}
        }
    }

    let mut selector = StyleSelector::default();
    if let Some(type_name) = type_name {
        selector = StyleSelector::new(type_name);
    }
    if let Some(id) = id {
        selector = selector.id(id);
    }
    for class in classes {
        selector = selector.class(class);
    }
    Some(selector)
}

fn parse_selector_chain(selector: &str) -> Option<SelectorChain> {
    let mut tokens: Vec<String> = Vec::new();
    let mut buf = String::new();
    for ch in selector.chars() {
        match ch {
            '>' => {
                if !buf.trim().is_empty() {
                    tokens.push(buf.trim().to_string());
                }
                tokens.push(">".to_string());
                buf.clear();
            }
            c if c.is_whitespace() => {
                if !buf.trim().is_empty() {
                    tokens.push(buf.trim().to_string());
                    buf.clear();
                }
            }
            _ => buf.push(ch),
        }
    }
    if !buf.trim().is_empty() {
        tokens.push(buf.trim().to_string());
    }

    let mut parts = Vec::new();
    let mut combinators = Vec::new();
    let mut pending: Option<Combinator> = None;
    for token in tokens {
        if token == ">" {
            pending = Some(Combinator::Child);
            continue;
        }
        if let Some(selector) = parse_selector(&token) {
            if !parts.is_empty() {
                combinators.push(pending.unwrap_or(Combinator::Descendant));
            }
            parts.push(selector);
            pending = None;
        }
    }

    if parts.is_empty() {
        return None;
    }

    Some(SelectorChain { parts, combinators })
}

fn parse_style_body(body: &str) -> Style {
    let mut style = Style::new();
    for decl in body.split(';') {
        let decl = decl.trim();
        if decl.is_empty() {
            continue;
        }
        let mut parts = decl.splitn(2, ':');
        let key = parts.next().unwrap_or("").trim().to_lowercase();
        let value = parts.next().unwrap_or("").trim();
        match key.as_str() {
            "fg" | "color" => {
                if let Some(color) = crate::style::Color::parse(value) {
                    style = style.fg(color);
                }
            }
            "bg" | "background" => {
                if let Some(color) = crate::style::Color::parse(value) {
                    style = style.bg(color);
                }
            }
            "bold" => {
                if let Some(val) = parse_bool(value) {
                    style = style.bold(val);
                }
            }
            "dim" => {
                if let Some(val) = parse_bool(value) {
                    style = style.dim(val);
                }
            }
            "italic" => {
                if let Some(val) = parse_bool(value) {
                    style = style.italic(val);
                }
            }
            "underline" => {
                if let Some(val) = parse_bool(value) {
                    style = style.underline(val);
                }
            }
            "border" => {
                if let Some(val) = parse_bool(value) {
                    style = style.border(val);
                }
            }
            _ => {}
        }
    }
    style
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}


fn crop_line_horizontal(line: &[Segment], start: usize, width: usize) -> Vec<Segment> {
    if width == 0 {
        return Vec::new();
    }
    if start == 0 {
        return Segment::adjust_line_length(line, width, None, true);
    }

    let mut out: Vec<Segment> = Vec::new();
    let mut skipped = 0usize;
    let mut remaining = width;

    for segment in line {
        if segment.control.is_some() {
            out.push(segment.clone());
            continue;
        }

        let seg_len = segment.cell_len();
        if skipped + seg_len <= start {
            skipped += seg_len;
            continue;
        }

        let offset_in_seg = start.saturating_sub(skipped);
        let visible_len = seg_len.saturating_sub(offset_in_seg);
        if visible_len == 0 {
            skipped += seg_len;
            continue;
        }

        let slice_len = visible_len.min(remaining);
        let mut text = segment.text.to_string();
        if offset_in_seg > 0 {
            text = rich_rs::set_cell_size(&text, seg_len - offset_in_seg);
            text = text.chars().skip(offset_in_seg).collect();
        }
        let cropped_text = rich_rs::set_cell_size(&text, slice_len);
        let mut out_segment = segment.clone();
        out_segment.text = cropped_text.into();
        out_segment.control = None;
        out.push(out_segment);
        remaining = remaining.saturating_sub(slice_len);
        skipped += seg_len;
        if remaining == 0 {
            break;
        }
    }

    if remaining > 0 {
        let padding = " ".repeat(remaining);
        out.push(Segment::new(padding));
    }

    out
}

fn collect_focus_ids(widget: &mut dyn Widget, out: &mut Vec<WidgetId>) {
    if widget.focusable() {
        out.push(widget.id());
    }
    widget.visit_children_mut(&mut |child| collect_focus_ids(child, out));
}

fn set_focus_by_id(widget: &mut dyn Widget, target: Option<WidgetId>) {
    if widget.focusable() {
        widget.set_focus(target == Some(widget.id()));
    }
    widget.visit_children_mut(&mut |child| set_focus_by_id(child, target));
}

fn dispatch_event_to_focus(
    widget: &mut dyn Widget,
    target: WidgetId,
    event: &Event,
    ctx: &mut EventCtx,
) {
    if widget.id() == target {
        widget.on_event(event, ctx);
        return;
    }
    widget.visit_children_mut(&mut |child| {
        if !ctx.handled() {
            dispatch_event_to_focus(child, target, event, ctx);
        }
    });
}

pub struct WidgetRenderable<'a> {
    widget: &'a dyn Widget,
}

fn apply_debug_box(
    lines: Vec<Vec<Segment>>,
    width: usize,
    height: usize,
    label: Option<&str>,
    style: rich_rs::Style,
) -> Vec<Vec<Segment>> {
    if width < 3 || height < 3 {
        return lines;
    }

    let b = rich_rs::r#box::SQUARE;
    let mut out: Vec<Vec<Segment>> = Vec::new();

    let mut top = String::new();
    top.push(b.top_left);
    let mut label_text = String::new();
    if let Some(text) = label {
        for ch in text.chars() {
            label_text.push(ch);
            if rich_rs::cell_len(&label_text) > width - 2 {
                label_text.pop();
                break;
            }
        }
    }
    let label_width = rich_rs::cell_len(&label_text);
    let fill_width = (width - 2).saturating_sub(label_width);
    top.push_str(&label_text);
    top.push_str(&std::iter::repeat(b.top).take(fill_width).collect::<String>());
    top.push(b.top_right);
    out.push(vec![Segment::styled(top, style)]);

    let mut content = lines;
    content = Segment::set_shape(&content, width - 2, Some(height - 2), None, false);

    for line in content.into_iter().take(height - 2) {
        let mut row: Vec<Segment> = Vec::new();
        row.push(Segment::styled(b.mid_left.to_string(), style));
        let inner = Segment::adjust_line_length(&line, width - 2, None, true);
        row.extend(inner);
        row.push(Segment::styled(b.mid_right.to_string(), style));
        out.push(row);
    }

    let mut bottom = String::new();
    bottom.push(b.bottom_left);
    bottom.push_str(&std::iter::repeat(b.bottom).take(width - 2).collect::<String>());
    bottom.push(b.bottom_right);
    out.push(vec![Segment::styled(bottom, style)]);

    out
}
