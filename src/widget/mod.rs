use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments, Text};
use rich_rs::markdown::Markdown as RichMarkdown;

use crate::debug::DebugLayout;
use crate::event::{Action, Event, EventCtx};
use crate::style::Style;
use crossterm::event::KeyCode;

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
    fn focusable(&self) -> bool {
        false
    }
    fn set_focus(&mut self, _focused: bool) {}
    fn layout_height(&self) -> Option<usize> {
        None
    }
    fn layout_constraints(&self) -> LayoutConstraints {
        LayoutConstraints::default()
    }
    fn style(&self) -> Option<Style> {
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

pub struct WidgetRenderable<'a> {
    widget: &'a dyn Widget,
}

#[derive(Default)]
pub struct Container {
    id: WidgetId,
    children: Vec<Box<dyn Widget>>,
}

impl Container {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            children: Vec::new(),
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.children.push(Box::new(child));
    }
}

impl Widget for Container {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height_limit = options.size.1.max(1);
        let bounds = rich_rs::Region::from_size(width as u32, height_limit as u32);

        let mut lines: Vec<Vec<Segment>> = Vec::new();
        let mut cursor_y: i32 = 0;

        for child in &self.children {
            let constraints = child.layout_constraints();
            let render_width =
                clamp_with_constraints(width, constraints.min_width, constraints.max_width, width);
            let render_height = clamp_with_constraints(
                height_limit,
                constraints.min_height,
                constraints.max_height,
                height_limit,
            );
            let mut child_options = options.clone();
            child_options.size = (render_width, render_height);
            child_options.max_width = render_width;
            child_options.max_height = render_height;

            let segments = child.render_styled(console, &child_options);
            let mut child_lines =
                Segment::split_and_crop_lines(segments, render_width, None, true, false);
            let mut target_height = child.layout_height().unwrap_or(child_lines.len().max(1));
            target_height = clamp_with_constraints(
                target_height,
                constraints.min_height,
                constraints.max_height,
                target_height,
            );
            child_lines =
                Segment::set_shape(&child_lines, render_width, Some(target_height), None, false);
            child_lines = pad_lines_to_width(child_lines, width);
            let child_height = child_lines.len();
            let child_region =
                rich_rs::Region::new(0, cursor_y, width as u32, child_height as u32);
            if let Some(visible) = child_region.intersection(&bounds) {
                let start = (visible.y - child_region.y).max(0) as usize;
                let end = (start + visible.height as usize).min(child_lines.len());
                for line in child_lines.into_iter().skip(start).take(end - start) {
                    if lines.len() >= height_limit {
                        break;
                    }
                    lines.push(line);
                }
            }
            cursor_y += child_height as i32;
            if cursor_y as usize >= height_limit {
                break;
            }
        }

        let line_count = lines.len();
        let mut out = Segments::new();
        for (idx, line) in lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        let width = options.size.0.max(1);
        let height_limit = options.size.1.max(1);
        let bounds = rich_rs::Region::from_size(width as u32, height_limit as u32);

        let mut lines: Vec<Vec<Segment>> = Vec::new();
        let mut cursor_y: i32 = 0;

        for (idx, child) in self.children.iter().enumerate() {
            let constraints = child.layout_constraints();
            let render_width =
                clamp_with_constraints(width, constraints.min_width, constraints.max_width, width);
            let render_height = clamp_with_constraints(
                height_limit,
                constraints.min_height,
                constraints.max_height,
                height_limit,
            );
            let mut child_options = options.clone();
            child_options.size = (render_width, render_height);
            child_options.max_width = render_width;
            child_options.max_height = render_height;

            let segments = child.render_styled(console, &child_options);
            let mut child_lines =
                Segment::split_and_crop_lines(segments, render_width, None, true, false);
            let mut target_height = child.layout_height().unwrap_or(child_lines.len().max(1));
            target_height = clamp_with_constraints(
                target_height,
                constraints.min_height,
                constraints.max_height,
                target_height,
            );
            child_lines =
                Segment::set_shape(&child_lines, render_width, Some(target_height), None, false);
            child_lines = pad_lines_to_width(child_lines, width);
            let child_height = child_lines.len().max(1);
            let debug_height = (child_height + 2).max(3);
            let child_region =
                rich_rs::Region::new(0, cursor_y, width as u32, debug_height as u32);
            if let Some(visible) = child_region.intersection(&bounds) {
                let start = (visible.y - child_region.y).max(0) as usize;
                let end = (start + visible.height as usize).min(debug_height);
                let label = if debug.show_sizes {
                    Some(format!("{width}x{debug_height}"))
                } else {
                    None
                };
                let wrapped = apply_debug_box(
                    child_lines,
                    width,
                    debug_height,
                    label.as_deref(),
                    debug.style_for(idx),
                );
                for line in wrapped.into_iter().skip(start).take(end - start) {
                    if lines.len() >= height_limit {
                        break;
                    }
                    lines.push(line);
                }
            }
            cursor_y += debug_height as i32;
            if cursor_y as usize >= height_limit {
                break;
            }
        }

        let line_count = lines.len();
        let mut out = Segments::new();
        for (idx, line) in lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn on_mount(&mut self) {
        for child in &mut self.children {
            child.on_mount();
        }
    }

    fn on_unmount(&mut self) {
        for child in &mut self.children {
            child.on_unmount();
        }
    }

    fn on_tick(&mut self, tick: u64) {
        for child in &mut self.children {
            child.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        for child in &mut self.children {
            child.on_resize(width, height);
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        for child in &mut self.children {
            child.on_event_capture(event, ctx);
            if ctx.handled() {
                break;
            }
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        for child in &mut self.children {
            child.on_event(event, ctx);
            if ctx.handled() {
                break;
            }
        }
    }
}

impl Renderable for Container {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

impl<'a> WidgetRenderable<'a> {
    pub fn new(widget: &'a dyn Widget) -> Self {
        Self { widget }
    }
}

impl Renderable for WidgetRenderable<'_> {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.widget.render_styled(console, options)
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

pub struct Constrained {
    id: WidgetId,
    child: Box<dyn Widget>,
    constraints: LayoutConstraints,
}

impl Constrained {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            id: WidgetId::new(),
            child: Box::new(child),
            constraints: LayoutConstraints::default(),
        }
    }

    pub fn min_width(mut self, value: usize) -> Self {
        self.constraints = self.constraints.min_width(value);
        self
    }

    pub fn max_width(mut self, value: usize) -> Self {
        self.constraints = self.constraints.max_width(value);
        self
    }

    pub fn min_height(mut self, value: usize) -> Self {
        self.constraints = self.constraints.min_height(value);
        self
    }

    pub fn max_height(mut self, value: usize) -> Self {
        self.constraints = self.constraints.max_height(value);
        self
    }
}

impl Widget for Constrained {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.child.render_styled(console, options)
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        self.child.render_styled_with_debug(console, options, debug)
    }

    fn on_mount(&mut self) {
        self.child.on_mount();
    }

    fn on_unmount(&mut self) {
        self.child.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.child.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.child.on_resize(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event(event, ctx);
    }

    fn focusable(&self) -> bool {
        self.child.focusable()
    }

    fn set_focus(&mut self, focused: bool) {
        self.child.set_focus(focused);
    }

    fn layout_height(&self) -> Option<usize> {
        if let (Some(min), Some(max)) = (self.constraints.min_height, self.constraints.max_height) {
            if min == max {
                return Some(min);
            }
        }
        self.child.layout_height()
    }

    fn layout_constraints(&self) -> LayoutConstraints {
        self.constraints
    }
}

impl Renderable for Constrained {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

pub struct Styled {
    id: WidgetId,
    child: Box<dyn Widget>,
    style: Style,
}

impl Styled {
    pub fn new(child: impl Widget + 'static, style: Style) -> Self {
        Self {
            id: WidgetId::new(),
            child: Box::new(child),
            style,
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }
}

impl Widget for Styled {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.child.render_styled(console, options)
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        self.child.render_styled_with_debug(console, options, debug)
    }

    fn on_mount(&mut self) {
        self.child.on_mount();
    }

    fn on_unmount(&mut self) {
        self.child.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.child.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.child.on_resize(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event(event, ctx);
    }

    fn focusable(&self) -> bool {
        self.child.focusable()
    }

    fn set_focus(&mut self, focused: bool) {
        self.child.set_focus(focused);
    }

    fn layout_height(&self) -> Option<usize> {
        self.child.layout_height()
    }

    fn layout_constraints(&self) -> LayoutConstraints {
        self.child.layout_constraints()
    }

    fn style(&self) -> Option<Style> {
        Some(self.style)
    }

    fn style_type(&self) -> &'static str {
        self.child.style_type()
    }
}

impl Renderable for Styled {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

pub struct Node {
    id: WidgetId,
    child: Box<dyn Widget>,
    style_id: Option<String>,
    classes: Vec<String>,
}

impl Node {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            id: WidgetId::new(),
            child: Box::new(child),
            style_id: None,
            classes: Vec::new(),
        }
    }

    pub fn id(mut self, value: impl Into<String>) -> Self {
        self.style_id = Some(value.into());
        self
    }

    pub fn class(mut self, value: impl Into<String>) -> Self {
        self.classes.push(value.into());
        self
    }

    pub fn classes(mut self, values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        for value in values {
            self.classes.push(value.into());
        }
        self
    }
}

impl Widget for Node {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        self.child.render_styled(console, options)
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        self.child.render_styled_with_debug(console, options, debug)
    }

    fn on_mount(&mut self) {
        self.child.on_mount();
    }

    fn on_unmount(&mut self) {
        self.child.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.child.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.child.on_resize(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event(event, ctx);
    }

    fn focusable(&self) -> bool {
        self.child.focusable()
    }

    fn set_focus(&mut self, focused: bool) {
        self.child.set_focus(focused);
    }

    fn layout_height(&self) -> Option<usize> {
        self.child.layout_height()
    }

    fn layout_constraints(&self) -> LayoutConstraints {
        self.child.layout_constraints()
    }

    fn style(&self) -> Option<Style> {
        self.child.style()
    }

    fn style_type(&self) -> &'static str {
        self.child.style_type()
    }

    fn style_id(&self) -> Option<&str> {
        self.style_id.as_deref()
    }

    fn style_classes(&self) -> &[String] {
        &self.classes
    }
}

impl Renderable for Node {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug, Clone)]
pub struct Label {
    id: WidgetId,
    text: String,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            text: text.into(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }
}

impl Widget for Label {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let text = Text::plain(&self.text);
        text.render(console, options)
    }

    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }
}

impl Renderable for Label {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

pub struct Row {
    id: WidgetId,
    children: Vec<Box<dyn Widget>>,
}

impl Row {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            children: Vec::new(),
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.children.push(Box::new(child));
    }
}

impl Widget for Row {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height_limit = options.size.1.max(1);

        let count = self.children.len().max(1);
        let base = width / count;
        let remainder = width % count;

        let widths: Vec<usize> = (0..count)
            .map(|idx| base + if idx < remainder { 1 } else { 0 })
            .collect();

        let mut child_lines: Vec<Vec<Vec<Segment>>> = Vec::new();

        for (idx, child) in self.children.iter().enumerate() {
            let child_width = widths[idx].max(1);
            let constraints = child.layout_constraints();
            let render_width = clamp_with_constraints(
                child_width,
                constraints.min_width,
                constraints.max_width,
                child_width,
            );
            let render_height = clamp_with_constraints(
                height_limit,
                constraints.min_height,
                constraints.max_height,
                height_limit,
            );
            let mut child_options = options.clone();
            child_options.size = (render_width, render_height);
            child_options.max_width = render_width;
            child_options.max_height = render_height;

            let segments = child.render_styled(console, &child_options);
            let mut lines =
                Segment::split_and_crop_lines(segments, render_width, None, true, false);
            let mut target_height = child.layout_height().unwrap_or(lines.len().max(1));
            target_height = clamp_with_constraints(
                target_height,
                constraints.min_height,
                constraints.max_height,
                height_limit,
            );
            lines = Segment::set_shape(&lines, render_width, Some(target_height), None, false);
            lines = pad_lines_to_width(lines, child_width);
            child_lines.push(lines);
        }

        let max_child_height = child_lines
            .iter()
            .map(|lines| lines.len())
            .max()
            .unwrap_or(1)
            .max(1)
            .min(height_limit);

        let mut out_lines: Vec<Vec<Segment>> = Vec::new();
        for row in 0..max_child_height {
            let mut line: Vec<Segment> = Vec::new();
            for (idx, lines) in child_lines.iter().enumerate() {
                let child_width = widths.get(idx).copied().unwrap_or(1).max(1);
                let child_line = lines.get(row).cloned().unwrap_or_else(|| {
                    vec![Segment::new(" ".repeat(child_width))]
                });
                let adjusted = Segment::adjust_line_length(&child_line, child_width, None, true);
                line.extend(adjusted);
            }
            out_lines.push(line);
        }

        let line_count = out_lines.len();
        let mut out = Segments::new();
        for (idx, line) in out_lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        let width = options.size.0.max(1);
        let height_limit = options.size.1.max(1);

        let count = self.children.len().max(1);
        let base = width / count;
        let remainder = width % count;

        let widths: Vec<usize> = (0..count)
            .map(|idx| base + if idx < remainder { 1 } else { 0 })
            .collect();

        let mut child_lines: Vec<Vec<Vec<Segment>>> = Vec::new();

        for (idx, child) in self.children.iter().enumerate() {
            let child_width = widths[idx].max(1);
            let constraints = child.layout_constraints();
            let render_width = clamp_with_constraints(
                child_width,
                constraints.min_width,
                constraints.max_width,
                child_width,
            );
            let render_height = clamp_with_constraints(
                height_limit,
                constraints.min_height,
                constraints.max_height,
                height_limit,
            );
            let mut child_options = options.clone();
            child_options.size = (render_width, render_height);
            child_options.max_width = render_width;
            child_options.max_height = render_height;

            let segments = child.render_styled(console, &child_options);
            let mut lines =
                Segment::split_and_crop_lines(segments, render_width, None, true, false);
            let mut target_height = child.layout_height().unwrap_or(lines.len().max(1));
            target_height = clamp_with_constraints(
                target_height,
                constraints.min_height,
                constraints.max_height,
                height_limit,
            );
            lines = Segment::set_shape(&lines, render_width, Some(target_height), None, false);
            lines = pad_lines_to_width(lines, child_width);
            let child_height = lines.len().max(1);
            let debug_height = (child_height + 2).max(3);
            let label = if debug.show_sizes {
                Some(format!("{child_width}x{debug_height}"))
            } else {
                None
            };
            let wrapped = apply_debug_box(
                lines,
                child_width,
                debug_height,
                label.as_deref(),
                debug.style_for(idx),
            );
            child_lines.push(wrapped);
        }

        let max_child_height = child_lines
            .iter()
            .map(|lines| lines.len())
            .max()
            .unwrap_or(1)
            .max(1)
            .min(height_limit);

        let mut out_lines: Vec<Vec<Segment>> = Vec::new();
        for row in 0..max_child_height {
            let mut line: Vec<Segment> = Vec::new();
            for (idx, lines) in child_lines.iter().enumerate() {
                let child_width = widths.get(idx).copied().unwrap_or(1).max(1);
                let child_line = lines.get(row).cloned().unwrap_or_else(|| {
                    vec![Segment::new(" ".repeat(child_width))]
                });
                let adjusted = Segment::adjust_line_length(&child_line, child_width, None, true);
                line.extend(adjusted);
            }
            out_lines.push(line);
        }

        let line_count = out_lines.len();
        let mut out = Segments::new();
        for (idx, line) in out_lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn on_mount(&mut self) {
        for child in &mut self.children {
            child.on_mount();
        }
    }

    fn on_unmount(&mut self) {
        for child in &mut self.children {
            child.on_unmount();
        }
    }

    fn on_tick(&mut self, tick: u64) {
        for child in &mut self.children {
            child.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        for child in &mut self.children {
            child.on_resize(width, height);
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        for child in &mut self.children {
            child.on_event_capture(event, ctx);
            if ctx.handled() {
                break;
            }
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        for child in &mut self.children {
            child.on_event(event, ctx);
            if ctx.handled() {
                break;
            }
        }
    }
}

impl Renderable for Row {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockKind {
    Top,
    Bottom,
    Left,
    Right,
    Fill,
}

pub struct DockItem {
    kind: DockKind,
    size: Option<usize>,
    child: Box<dyn Widget>,
}

pub struct Dock {
    id: WidgetId,
    items: Vec<DockItem>,
    fixed_height: Option<usize>,
}

impl Dock {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            items: Vec::new(),
            fixed_height: None,
        }
    }

    pub fn height(mut self, height: usize) -> Self {
        self.fixed_height = Some(height.max(1));
        self
    }

    pub fn push_top(mut self, height: Option<usize>, child: impl Widget + 'static) -> Self {
        self.items.push(DockItem {
            kind: DockKind::Top,
            size: height,
            child: Box::new(child),
        });
        self
    }

    pub fn push_bottom(mut self, height: Option<usize>, child: impl Widget + 'static) -> Self {
        self.items.push(DockItem {
            kind: DockKind::Bottom,
            size: height,
            child: Box::new(child),
        });
        self
    }

    pub fn push_left(mut self, width: usize, child: impl Widget + 'static) -> Self {
        self.items.push(DockItem {
            kind: DockKind::Left,
            size: Some(width),
            child: Box::new(child),
        });
        self
    }

    pub fn push_right(mut self, width: usize, child: impl Widget + 'static) -> Self {
        self.items.push(DockItem {
            kind: DockKind::Right,
            size: Some(width),
            child: Box::new(child),
        });
        self
    }

    pub fn push_fill(mut self, child: impl Widget + 'static) -> Self {
        self.items.push(DockItem {
            kind: DockKind::Fill,
            size: None,
            child: Box::new(child),
        });
        self
    }
}

impl Widget for Dock {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let mut remaining_width = options.size.0.max(1);
        let mut remaining_height = self
            .fixed_height
            .unwrap_or_else(|| options.size.1.max(1));

        let mut top_lines: Vec<Vec<Segment>> = Vec::new();
        let mut bottom_lines: Vec<Vec<Segment>> = Vec::new();

        let mut left_columns: Vec<(usize, Vec<Vec<Segment>>)> = Vec::new();
        let mut right_columns: Vec<(usize, Vec<Vec<Segment>>)> = Vec::new();
        let mut fill_lines: Option<Vec<Vec<Segment>>> = None;

        for item in &self.items {
            match item.kind {
                DockKind::Top => {
                    let height = item
                        .size
                        .or_else(|| item.child.layout_height())
                        .unwrap_or(1)
                        .min(remaining_height);
                    let constraints = item.child.layout_constraints();
                    let render_height = clamp_with_constraints(
                        height,
                        constraints.min_height,
                        constraints.max_height,
                        height,
                    );
                    let render_width = clamp_with_constraints(
                        remaining_width,
                        constraints.min_width,
                        constraints.max_width,
                        remaining_width,
                    );
                    let mut child_options = options.clone();
                    child_options.size = (render_width, render_height);
                    child_options.max_width = render_width;
                    child_options.max_height = render_height;
                    let segments = item.child.render_styled(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, render_width, None, true, false);
                    lines = Segment::set_shape(&lines, render_width, Some(render_height), None, false);
                    lines = pad_lines_to_width(lines, remaining_width);
                    top_lines.extend(lines);
                    remaining_height = remaining_height.saturating_sub(height);
                }
                DockKind::Bottom => {
                    let height = item
                        .size
                        .or_else(|| item.child.layout_height())
                        .unwrap_or(1)
                        .min(remaining_height);
                    let constraints = item.child.layout_constraints();
                    let render_height = clamp_with_constraints(
                        height,
                        constraints.min_height,
                        constraints.max_height,
                        height,
                    );
                    let render_width = clamp_with_constraints(
                        remaining_width,
                        constraints.min_width,
                        constraints.max_width,
                        remaining_width,
                    );
                    let mut child_options = options.clone();
                    child_options.size = (render_width, render_height);
                    child_options.max_width = render_width;
                    child_options.max_height = render_height;
                    let segments = item.child.render_styled(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, render_width, None, true, false);
                    lines = Segment::set_shape(&lines, render_width, Some(render_height), None, false);
                    lines = pad_lines_to_width(lines, remaining_width);
                    bottom_lines.extend(lines);
                    remaining_height = remaining_height.saturating_sub(height);
                }
                DockKind::Left => {
                    let width = item.size.unwrap_or(1).min(remaining_width);
                    let constraints = item.child.layout_constraints();
                    let render_width = clamp_with_constraints(
                        width,
                        constraints.min_width,
                        constraints.max_width,
                        width,
                    );
                    let render_height = clamp_with_constraints(
                        remaining_height,
                        constraints.min_height,
                        constraints.max_height,
                        remaining_height,
                    );
                    let mut child_options = options.clone();
                    child_options.size = (render_width, render_height);
                    child_options.max_width = render_width;
                    child_options.max_height = render_height;
                    let segments = item.child.render_styled(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, render_width, None, true, false);
                    lines =
                        Segment::set_shape(&lines, render_width, Some(render_height), None, false);
                    lines = pad_lines_to_width(lines, width);
                    left_columns.push((width, lines));
                    remaining_width = remaining_width.saturating_sub(width);
                }
                DockKind::Right => {
                    let width = item.size.unwrap_or(1).min(remaining_width);
                    let constraints = item.child.layout_constraints();
                    let render_width = clamp_with_constraints(
                        width,
                        constraints.min_width,
                        constraints.max_width,
                        width,
                    );
                    let render_height = clamp_with_constraints(
                        remaining_height,
                        constraints.min_height,
                        constraints.max_height,
                        remaining_height,
                    );
                    let mut child_options = options.clone();
                    child_options.size = (render_width, render_height);
                    child_options.max_width = render_width;
                    child_options.max_height = render_height;
                    let segments = item.child.render_styled(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, render_width, None, true, false);
                    lines =
                        Segment::set_shape(&lines, render_width, Some(render_height), None, false);
                    lines = pad_lines_to_width(lines, width);
                    right_columns.push((width, lines));
                    remaining_width = remaining_width.saturating_sub(width);
                }
                DockKind::Fill => {
                    let constraints = item.child.layout_constraints();
                    let render_width = clamp_with_constraints(
                        remaining_width,
                        constraints.min_width,
                        constraints.max_width,
                        remaining_width,
                    );
                    let render_height = clamp_with_constraints(
                        remaining_height,
                        constraints.min_height,
                        constraints.max_height,
                        remaining_height,
                    );
                    let mut child_options = options.clone();
                    child_options.size = (render_width, render_height);
                    child_options.max_width = render_width;
                    child_options.max_height = render_height;
                    let segments = item.child.render_styled(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, render_width, None, true, false);
                    lines =
                        Segment::set_shape(&lines, render_width, Some(render_height), None, false);
                    lines = pad_lines_to_width(lines, remaining_width);
                    fill_lines = Some(lines);
                }
            }
        }

        let mut middle_lines: Vec<Vec<Segment>> = Vec::new();
        for row in 0..remaining_height {
            let mut line: Vec<Segment> = Vec::new();

            for (col_width, column) in &left_columns {
                let col_line = column.get(row).cloned().unwrap_or_else(|| {
                    vec![Segment::new(" ".repeat(*col_width))]
                });
                let adjusted = Segment::adjust_line_length(&col_line, *col_width, None, true);
                line.extend(adjusted);
            }

            let remaining_mid_width = remaining_width;
            if let Some(lines) = &fill_lines {
                let fill_line = lines.get(row).cloned().unwrap_or_else(|| {
                    vec![Segment::new(" ".repeat(remaining_mid_width))]
                });
                let adjusted =
                    Segment::adjust_line_length(&fill_line, remaining_mid_width, None, true);
                line.extend(adjusted);
            } else {
                line.extend(vec![Segment::new(" ".repeat(remaining_mid_width))]);
            }

            for (col_width, column) in &right_columns {
                let col_line = column.get(row).cloned().unwrap_or_else(|| {
                    vec![Segment::new(" ".repeat(*col_width))]
                });
                let adjusted = Segment::adjust_line_length(&col_line, *col_width, None, true);
                line.extend(adjusted);
            }

            middle_lines.push(line);
        }

        let mut out_lines: Vec<Vec<Segment>> = Vec::new();
        out_lines.extend(top_lines);
        out_lines.extend(middle_lines);
        out_lines.extend(bottom_lines);

        let line_count = out_lines.len();
        let mut out = Segments::new();
        for (idx, line) in out_lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        let mut remaining_width = options.size.0.max(1);
        let mut remaining_height = self
            .fixed_height
            .unwrap_or_else(|| options.size.1.max(1));

        let mut top_lines: Vec<Vec<Segment>> = Vec::new();
        let mut bottom_lines: Vec<Vec<Segment>> = Vec::new();

        let mut left_columns: Vec<(usize, Vec<Vec<Segment>>)> = Vec::new();
        let mut right_columns: Vec<(usize, Vec<Vec<Segment>>)> = Vec::new();
        let mut fill_lines: Option<Vec<Vec<Segment>>> = None;

        for (idx, item) in self.items.iter().enumerate() {
            match item.kind {
                DockKind::Top => {
                    let height = item
                        .size
                        .or_else(|| item.child.layout_height())
                        .unwrap_or(1)
                        .min(remaining_height);
                    let constraints = item.child.layout_constraints();
                    let render_height = clamp_with_constraints(
                        height,
                        constraints.min_height,
                        constraints.max_height,
                        height,
                    );
                    let render_width = clamp_with_constraints(
                        remaining_width,
                        constraints.min_width,
                        constraints.max_width,
                        remaining_width,
                    );
                    let mut child_options = options.clone();
                    child_options.size = (render_width, render_height);
                    child_options.max_width = render_width;
                    child_options.max_height = render_height;
                    let segments = item.child.render_styled(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, render_width, None, true, false);
                    lines = Segment::set_shape(&lines, render_width, Some(render_height), None, false);
                    lines = pad_lines_to_width(lines, remaining_width);
                    let debug_height = (height + 2).max(3);
                    let label = if debug.show_sizes {
                        Some(format!("{remaining_width}x{debug_height}"))
                    } else {
                        None
                    };
                    let wrapped = apply_debug_box(
                        lines,
                        remaining_width,
                        debug_height,
                        label.as_deref(),
                        debug.style_for(idx),
                    );
                    top_lines.extend(wrapped);
                    remaining_height = remaining_height.saturating_sub(height);
                }
                DockKind::Bottom => {
                    let height = item
                        .size
                        .or_else(|| item.child.layout_height())
                        .unwrap_or(1)
                        .min(remaining_height);
                    let constraints = item.child.layout_constraints();
                    let render_height = clamp_with_constraints(
                        height,
                        constraints.min_height,
                        constraints.max_height,
                        height,
                    );
                    let render_width = clamp_with_constraints(
                        remaining_width,
                        constraints.min_width,
                        constraints.max_width,
                        remaining_width,
                    );
                    let mut child_options = options.clone();
                    child_options.size = (render_width, render_height);
                    child_options.max_width = render_width;
                    child_options.max_height = render_height;
                    let segments = item.child.render_styled(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, render_width, None, true, false);
                    lines = Segment::set_shape(&lines, render_width, Some(render_height), None, false);
                    lines = pad_lines_to_width(lines, remaining_width);
                    let debug_height = (height + 2).max(3);
                    let label = if debug.show_sizes {
                        Some(format!("{remaining_width}x{debug_height}"))
                    } else {
                        None
                    };
                    let wrapped = apply_debug_box(
                        lines,
                        remaining_width,
                        debug_height,
                        label.as_deref(),
                        debug.style_for(idx),
                    );
                    bottom_lines.extend(wrapped);
                    remaining_height = remaining_height.saturating_sub(height);
                }
                DockKind::Left => {
                    let width = item.size.unwrap_or(1).min(remaining_width);
                    let constraints = item.child.layout_constraints();
                    let render_width = clamp_with_constraints(
                        width,
                        constraints.min_width,
                        constraints.max_width,
                        width,
                    );
                    let render_height = clamp_with_constraints(
                        remaining_height,
                        constraints.min_height,
                        constraints.max_height,
                        remaining_height,
                    );
                    let mut child_options = options.clone();
                    child_options.size = (render_width, render_height);
                    child_options.max_width = render_width;
                    child_options.max_height = render_height;
                    let segments = item.child.render_styled(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, render_width, None, true, false);
                    lines =
                        Segment::set_shape(&lines, render_width, Some(render_height), None, false);
                    lines = pad_lines_to_width(lines, width);
                    let debug_height = (remaining_height + 2).max(3);
                    let label = if debug.show_sizes {
                        Some(format!("{width}x{debug_height}"))
                    } else {
                        None
                    };
                    let wrapped = apply_debug_box(
                        lines,
                        width,
                        debug_height,
                        label.as_deref(),
                        debug.style_for(idx),
                    );
                    left_columns.push((width, wrapped));
                    remaining_width = remaining_width.saturating_sub(width);
                }
                DockKind::Right => {
                    let width = item.size.unwrap_or(1).min(remaining_width);
                    let constraints = item.child.layout_constraints();
                    let render_width = clamp_with_constraints(
                        width,
                        constraints.min_width,
                        constraints.max_width,
                        width,
                    );
                    let render_height = clamp_with_constraints(
                        remaining_height,
                        constraints.min_height,
                        constraints.max_height,
                        remaining_height,
                    );
                    let mut child_options = options.clone();
                    child_options.size = (render_width, render_height);
                    child_options.max_width = render_width;
                    child_options.max_height = render_height;
                    let segments = item.child.render_styled(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, render_width, None, true, false);
                    lines =
                        Segment::set_shape(&lines, render_width, Some(render_height), None, false);
                    lines = pad_lines_to_width(lines, width);
                    let debug_height = (remaining_height + 2).max(3);
                    let label = if debug.show_sizes {
                        Some(format!("{width}x{debug_height}"))
                    } else {
                        None
                    };
                    let wrapped = apply_debug_box(
                        lines,
                        width,
                        debug_height,
                        label.as_deref(),
                        debug.style_for(idx),
                    );
                    right_columns.push((width, wrapped));
                    remaining_width = remaining_width.saturating_sub(width);
                }
                DockKind::Fill => {
                    let constraints = item.child.layout_constraints();
                    let render_width = clamp_with_constraints(
                        remaining_width,
                        constraints.min_width,
                        constraints.max_width,
                        remaining_width,
                    );
                    let render_height = clamp_with_constraints(
                        remaining_height,
                        constraints.min_height,
                        constraints.max_height,
                        remaining_height,
                    );
                    let mut child_options = options.clone();
                    child_options.size = (render_width, render_height);
                    child_options.max_width = render_width;
                    child_options.max_height = render_height;
                    let segments = item.child.render_styled(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, render_width, None, true, false);
                    lines =
                        Segment::set_shape(&lines, render_width, Some(render_height), None, false);
                    lines = pad_lines_to_width(lines, remaining_width);
                    let debug_height = (remaining_height + 2).max(3);
                    let label = if debug.show_sizes {
                        Some(format!("{remaining_width}x{debug_height}"))
                    } else {
                        None
                    };
                    let wrapped = apply_debug_box(
                        lines,
                        remaining_width,
                        debug_height,
                        label.as_deref(),
                        debug.style_for(idx),
                    );
                    fill_lines = Some(wrapped);
                }
            }
        }

        let mut middle_lines: Vec<Vec<Segment>> = Vec::new();
        for row in 0..remaining_height {
            let mut line: Vec<Segment> = Vec::new();

            for (col_width, column) in &left_columns {
                let col_line = column.get(row).cloned().unwrap_or_else(|| {
                    vec![Segment::new(" ".repeat(*col_width))]
                });
                let adjusted = Segment::adjust_line_length(&col_line, *col_width, None, true);
                line.extend(adjusted);
            }

            let remaining_mid_width = remaining_width;
            if let Some(lines) = &fill_lines {
                let fill_line = lines.get(row).cloned().unwrap_or_else(|| {
                    vec![Segment::new(" ".repeat(remaining_mid_width))]
                });
                let adjusted =
                    Segment::adjust_line_length(&fill_line, remaining_mid_width, None, true);
                line.extend(adjusted);
            } else {
                line.extend(vec![Segment::new(" ".repeat(remaining_mid_width))]);
            }

            for (col_width, column) in &right_columns {
                let col_line = column.get(row).cloned().unwrap_or_else(|| {
                    vec![Segment::new(" ".repeat(*col_width))]
                });
                let adjusted = Segment::adjust_line_length(&col_line, *col_width, None, true);
                line.extend(adjusted);
            }

            middle_lines.push(line);
        }

        let mut out_lines: Vec<Vec<Segment>> = Vec::new();
        out_lines.extend(top_lines);
        out_lines.extend(middle_lines);
        out_lines.extend(bottom_lines);

        let line_count = out_lines.len();
        let mut out = Segments::new();
        for (idx, line) in out_lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn on_mount(&mut self) {
        for item in &mut self.items {
            item.child.on_mount();
        }
    }

    fn on_unmount(&mut self) {
        for item in &mut self.items {
            item.child.on_unmount();
        }
    }

    fn on_tick(&mut self, tick: u64) {
        for item in &mut self.items {
            item.child.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        for item in &mut self.items {
            item.child.on_resize(width, height);
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        for item in &mut self.items {
            item.child.on_event_capture(event, ctx);
            if ctx.handled() {
                break;
            }
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        for item in &mut self.items {
            item.child.on_event(event, ctx);
            if ctx.handled() {
                break;
            }
        }
    }

    fn layout_height(&self) -> Option<usize> {
        self.fixed_height
    }
}

impl Renderable for Dock {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug, Clone)]
pub struct Button {
    id: WidgetId,
    label: String,
    focused: bool,
    pressed: bool,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            label: label.into(),
            focused: false,
            pressed: false,
        }
    }

    pub fn pressed(&self) -> bool {
        self.pressed
    }
}

impl Widget for Button {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.focused {
            return;
        }
        if let Event::Action(Action::Toggle) = event {
            self.pressed = !self.pressed;
            ctx.set_handled();
            return;
        }
        if let Event::Key(key) = event {
            match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.pressed = !self.pressed;
                    ctx.set_handled();
                }
                _ => {}
            }
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let marker = if self.focused { "> " } else { "  " };
        let state = if self.pressed { "[x]" } else { "[ ]" };
        let text = Text::plain(format!("{marker}{state} {}", self.label));
        text.render(console, options)
    }

    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }
}

impl Renderable for Button {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug, Clone)]
pub struct ListView {
    id: WidgetId,
    items: Vec<String>,
    selected: usize,
    offset: usize,
    focused: bool,
}

impl ListView {
    pub fn new(items: Vec<String>) -> Self {
        Self {
            id: WidgetId::new(),
            items,
            selected: 0,
            offset: 0,
            focused: false,
        }
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn set_selected(&mut self, index: usize) {
        if self.items.is_empty() {
            self.selected = 0;
            self.offset = 0;
            return;
        }
        self.selected = index.min(self.items.len() - 1);
    }

    fn ensure_visible(&mut self, height: usize) {
        if self.items.is_empty() {
            self.offset = 0;
            return;
        }
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + height {
            self.offset = self.selected + 1 - height;
        }
    }
}

impl Widget for ListView {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.focused {
            return;
        }
        let mut handled = false;
        match event {
            Event::Action(Action::ScrollUp) => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                handled = true;
            }
            Event::Action(Action::ScrollDown) => {
                if self.selected + 1 < self.items.len() {
                    self.selected += 1;
                }
                handled = true;
            }
            Event::Action(Action::ScrollPageUp) => {
                if self.selected > 0 {
                    let step = 5.min(self.selected);
                    self.selected -= step;
                }
                handled = true;
            }
            Event::Action(Action::ScrollPageDown) => {
                if self.selected + 1 < self.items.len() {
                    let step = 5.min(self.items.len().saturating_sub(1) - self.selected);
                    self.selected += step;
                }
                handled = true;
            }
            Event::Key(key) => match key.code {
                KeyCode::Up => {
                    if self.selected > 0 {
                        self.selected -= 1;
                    }
                    handled = true;
                }
                KeyCode::Down => {
                    if self.selected + 1 < self.items.len() {
                        self.selected += 1;
                    }
                    handled = true;
                }
                KeyCode::PageUp => {
                    if self.selected > 0 {
                        let step = 5.min(self.selected);
                        self.selected -= step;
                    }
                    handled = true;
                }
                KeyCode::PageDown => {
                    if self.selected + 1 < self.items.len() {
                        let step = 5.min(self.items.len().saturating_sub(1) - self.selected);
                        self.selected += step;
                    }
                    handled = true;
                }
                _ => {}
            },
            _ => {}
        }
        if handled {
            ctx.set_handled();
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let height = options.size.1.max(1);
        let mut view = self.clone();
        view.ensure_visible(height);

        let mut lines: Vec<String> = Vec::new();
        for (idx, item) in view.items.iter().enumerate() {
            if idx < view.offset {
                continue;
            }
            if lines.len() >= height {
                break;
            }
            let marker = if self.focused && idx == view.selected {
                "> "
            } else if idx == view.selected {
                "* "
            } else {
                "  "
            };
            lines.push(format!("{marker}{item}"));
        }
        if lines.is_empty() {
            lines.push(String::new());
        }
        let text = Text::plain(lines.join("\n"));
        text.render(console, options)
    }
}

impl Renderable for ListView {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug, Clone)]
pub struct DataTable {
    id: WidgetId,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    selected: usize,
    offset: usize,
    focused: bool,
}

impl DataTable {
    pub fn new(headers: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        Self {
            id: WidgetId::new(),
            headers,
            rows,
            selected: 0,
            offset: 0,
            focused: false,
        }
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn set_selected(&mut self, index: usize) {
        if self.rows.is_empty() {
            self.selected = 0;
            self.offset = 0;
            return;
        }
        self.selected = index.min(self.rows.len() - 1);
    }

    fn ensure_visible(&mut self, height: usize) {
        if self.rows.is_empty() || height == 0 {
            self.offset = 0;
            return;
        }
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + height {
            self.offset = self.selected + 1 - height;
        }
    }
}

impl Widget for DataTable {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.focused {
            return;
        }
        let mut handled = false;
        match event {
            Event::Action(Action::ScrollUp) => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                handled = true;
            }
            Event::Action(Action::ScrollDown) => {
                if self.selected + 1 < self.rows.len() {
                    self.selected += 1;
                }
                handled = true;
            }
            Event::Action(Action::ScrollPageUp) => {
                if self.selected > 0 {
                    let step = 5.min(self.selected);
                    self.selected -= step;
                }
                handled = true;
            }
            Event::Action(Action::ScrollPageDown) => {
                if self.selected + 1 < self.rows.len() {
                    let step = 5.min(self.rows.len().saturating_sub(1) - self.selected);
                    self.selected += step;
                }
                handled = true;
            }
            Event::Key(key) => match key.code {
                KeyCode::Up => {
                    if self.selected > 0 {
                        self.selected -= 1;
                    }
                    handled = true;
                }
                KeyCode::Down => {
                    if self.selected + 1 < self.rows.len() {
                        self.selected += 1;
                    }
                    handled = true;
                }
                KeyCode::PageUp => {
                    if self.selected > 0 {
                        let step = 5.min(self.selected);
                        self.selected -= step;
                    }
                    handled = true;
                }
                KeyCode::PageDown => {
                    if self.selected + 1 < self.rows.len() {
                        let step = 5.min(self.rows.len().saturating_sub(1) - self.selected);
                        self.selected += step;
                    }
                    handled = true;
                }
                _ => {}
            },
            _ => {}
        }
        if handled {
            ctx.set_handled();
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let cols = self.headers.len().max(1);

        let sep_width = 3usize; // " | "
        let total_sep = sep_width.saturating_mul(cols.saturating_sub(1));
        let inner_width = width.saturating_sub(total_sep).max(1);
        let base = inner_width / cols;
        let rem = inner_width % cols;
        let col_widths: Vec<usize> = (0..cols)
            .map(|idx| base + if idx < rem { 1 } else { 0 })
            .collect();

        let mut lines: Vec<String> = Vec::new();
        let mut header_cells: Vec<String> = Vec::new();
        for (idx, header) in self.headers.iter().enumerate() {
            let w = col_widths.get(idx).copied().unwrap_or(1).max(1);
            header_cells.push(rich_rs::set_cell_size(header, w));
        }
        if !header_cells.is_empty() {
            lines.push(header_cells.join(" | "));
            let sep = header_cells
                .iter()
                .map(|h| "-".repeat(rich_rs::cell_len(h)))
                .collect::<Vec<_>>()
                .join("-+-");
            lines.push(sep);
        }

        let mut view = self.clone();
        let rows_available = height.saturating_sub(lines.len()).max(0);
        view.ensure_visible(rows_available);

        for (row_idx, row) in view.rows.iter().enumerate() {
            if row_idx < view.offset {
                continue;
            }
            if lines.len() >= height {
                break;
            }
            let mut cells: Vec<String> = Vec::new();
            for (idx, w) in col_widths.iter().enumerate() {
                let mut cell = row.get(idx).cloned().unwrap_or_default();
                if idx == 0 && row_idx == view.selected {
                    if *w >= 2 {
                        let marker = if self.focused { "> " } else { "* " };
                        cell = format!("{marker}{cell}");
                    }
                }
                cells.push(rich_rs::set_cell_size(&cell, *w));
            }
            lines.push(cells.join(" | "));
        }

        if lines.is_empty() {
            lines.push(String::new());
        }
        let text = Text::plain(lines.join("\n"));
        text.render(console, options)
    }
}

impl Renderable for DataTable {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug, Clone)]
pub struct TreeNode {
    label: String,
    children: Vec<TreeNode>,
    expanded: bool,
}

impl TreeNode {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            children: Vec::new(),
            expanded: true,
        }
    }

    pub fn with_child(mut self, child: TreeNode) -> Self {
        self.children.push(child);
        self
    }

    pub fn push(&mut self, child: TreeNode) {
        self.children.push(child);
    }

    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }
}

#[derive(Debug, Clone)]
pub struct Tree {
    id: WidgetId,
    roots: Vec<TreeNode>,
    selected: usize,
    offset: usize,
    focused: bool,
}

impl Tree {
    pub fn new(roots: Vec<TreeNode>) -> Self {
        Self {
            id: WidgetId::new(),
            roots,
            selected: 0,
            offset: 0,
            focused: false,
        }
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn set_selected(&mut self, index: usize) {
        let total = self.visible_count();
        if total == 0 {
            self.selected = 0;
            self.offset = 0;
            return;
        }
        self.selected = index.min(total - 1);
    }

    fn visible_count(&self) -> usize {
        fn walk(nodes: &[TreeNode], count: &mut usize) {
            for node in nodes {
                *count += 1;
                if node.expanded {
                    walk(&node.children, count);
                }
            }
        }
        let mut count = 0;
        walk(&self.roots, &mut count);
        count
    }

    fn ensure_visible(&mut self, height: usize) {
        if height == 0 {
            self.offset = 0;
            return;
        }
        let total = self.visible_count();
        if total == 0 {
            self.offset = 0;
            return;
        }
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + height {
            self.offset = self.selected + 1 - height;
        }
    }

    fn toggle_selected(&mut self) {
        let mut index = 0usize;
        if let Some(node) = node_mut_by_visible_index(&mut self.roots, self.selected, &mut index) {
            if !node.children.is_empty() {
                node.expanded = !node.expanded;
            }
        }
    }
}

impl Widget for Tree {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.focused {
            return;
        }
        let mut handled = false;
        match event {
            Event::Action(Action::ScrollUp) => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                handled = true;
            }
            Event::Action(Action::ScrollDown) => {
                let total = self.visible_count();
                if self.selected + 1 < total {
                    self.selected += 1;
                }
                handled = true;
            }
            Event::Action(Action::ScrollPageUp) => {
                if self.selected > 0 {
                    let step = 5.min(self.selected);
                    self.selected -= step;
                }
                handled = true;
            }
            Event::Action(Action::ScrollPageDown) => {
                let total = self.visible_count();
                if self.selected + 1 < total {
                    let step = 5.min(total.saturating_sub(1) - self.selected);
                    self.selected += step;
                }
                handled = true;
            }
            Event::Action(Action::Toggle) => {
                self.toggle_selected();
                handled = true;
            }
            Event::Key(key) => match key.code {
                KeyCode::Up => {
                    if self.selected > 0 {
                        self.selected -= 1;
                    }
                    handled = true;
                }
                KeyCode::Down => {
                    let total = self.visible_count();
                    if self.selected + 1 < total {
                        self.selected += 1;
                    }
                    handled = true;
                }
                KeyCode::PageUp => {
                    if self.selected > 0 {
                        let step = 5.min(self.selected);
                        self.selected -= step;
                    }
                    handled = true;
                }
                KeyCode::PageDown => {
                    let total = self.visible_count();
                    if self.selected + 1 < total {
                        let step = 5.min(total.saturating_sub(1) - self.selected);
                        self.selected += step;
                    }
                    handled = true;
                }
                KeyCode::Left => {
                    let mut index = 0usize;
                    if let Some(node) =
                        node_mut_by_visible_index(&mut self.roots, self.selected, &mut index)
                    {
                        if node.expanded {
                            node.expanded = false;
                        }
                    }
                    handled = true;
                }
                KeyCode::Right => {
                    let mut index = 0usize;
                    if let Some(node) =
                        node_mut_by_visible_index(&mut self.roots, self.selected, &mut index)
                    {
                        if !node.children.is_empty() {
                            node.expanded = true;
                        }
                    }
                    handled = true;
                }
                _ => {}
            },
            _ => {}
        }
        if handled {
            ctx.set_handled();
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let height = options.size.1.max(1);
        let mut view = self.clone();
        view.ensure_visible(height);

        let mut lines: Vec<String> = Vec::new();
        let mut index = 0usize;
        render_tree_lines(
            &view.roots,
            0,
            &mut index,
            view.selected,
            view.offset,
            height,
            view.focused,
            &mut lines,
        );

        if lines.is_empty() {
            lines.push(String::new());
        }
        let text = Text::plain(lines.join("\n"));
        text.render(console, options)
    }
}

impl Renderable for Tree {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

fn node_mut_by_visible_index<'a>(
    nodes: &'a mut [TreeNode],
    target: usize,
    index: &mut usize,
) -> Option<&'a mut TreeNode> {
    for node in nodes {
        if *index == target {
            return Some(node);
        }
        *index += 1;
        if node.expanded {
            if let Some(found) = node_mut_by_visible_index(&mut node.children, target, index) {
                return Some(found);
            }
        }
    }
    None
}

fn render_tree_lines(
    nodes: &[TreeNode],
    depth: usize,
    index: &mut usize,
    selected: usize,
    offset: usize,
    height: usize,
    focused: bool,
    lines: &mut Vec<String>,
) {
    for node in nodes {
        if lines.len() >= height {
            return;
        }
        if *index >= offset && lines.len() < height {
            let marker = if *index == selected {
                if focused { "> " } else { "* " }
            } else {
                "  "
            };
            let twist = if node.children.is_empty() {
                " "
            } else if node.expanded {
                "v"
            } else {
                ">"
            };
            let indent = "  ".repeat(depth);
            lines.push(format!("{marker}{indent}{twist} {}", node.label));
        }
        *index += 1;
        if node.expanded {
            render_tree_lines(
                &node.children,
                depth + 1,
                index,
                selected,
                offset,
                height,
                focused,
                lines,
            );
        }
    }
}

pub struct Tabs {
    id: WidgetId,
    tabs: Vec<Tab>,
    active: usize,
    focused: bool,
}

pub struct Tab {
    title: String,
    child: Box<dyn Widget>,
}

impl Tabs {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            tabs: Vec::new(),
            active: 0,
            focused: false,
        }
    }

    pub fn with_tab(mut self, title: impl Into<String>, child: impl Widget + 'static) -> Self {
        self.tabs.push(Tab {
            title: title.into(),
            child: Box::new(child),
        });
        self
    }

    pub fn push(&mut self, title: impl Into<String>, child: impl Widget + 'static) {
        self.tabs.push(Tab {
            title: title.into(),
            child: Box::new(child),
        });
    }

    pub fn active(&self) -> usize {
        self.active
    }

    pub fn set_active(&mut self, index: usize) {
        if self.tabs.is_empty() {
            self.active = 0;
            return;
        }
        let next = index.min(self.tabs.len() - 1);
        if self.focused {
            if let Some(tab) = self.tabs.get_mut(self.active) {
                tab.child.set_focus(false);
            }
        }
        self.active = next;
        if self.focused {
            if let Some(tab) = self.tabs.get_mut(self.active) {
                tab.child.set_focus(true);
            }
        }
    }

    fn activate_prev(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        let next = if self.active == 0 {
            self.tabs.len() - 1
        } else {
            self.active - 1
        };
        self.set_active(next);
    }

    fn activate_next(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        let next = (self.active + 1) % self.tabs.len();
        self.set_active(next);
    }
}

impl Widget for Tabs {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.child.set_focus(focused);
        }
    }

    fn on_mount(&mut self) {
        for tab in &mut self.tabs {
            tab.child.on_mount();
        }
    }

    fn on_unmount(&mut self) {
        for tab in &mut self.tabs {
            tab.child.on_unmount();
        }
    }

    fn on_tick(&mut self, tick: u64) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.child.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.child.on_resize(width, height);
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.child.on_event_capture(event, ctx);
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if self.focused {
            if let Event::Key(key) = event {
                match key.code {
                    KeyCode::Left => {
                        self.activate_prev();
                        ctx.set_handled();
                        return;
                    }
                    KeyCode::Right => {
                        self.activate_next();
                        ctx.set_handled();
                        return;
                    }
                    KeyCode::Char('h') => {
                        self.activate_prev();
                        ctx.set_handled();
                        return;
                    }
                    KeyCode::Char('l') => {
                        self.activate_next();
                        ctx.set_handled();
                        return;
                    }
                    _ => {}
                }
            }
        }
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.child.on_event(event, ctx);
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);

        let header = if self.tabs.is_empty() {
            "no tabs".to_string()
        } else {
            let mut parts = Vec::new();
            for (idx, tab) in self.tabs.iter().enumerate() {
                if idx == self.active {
                    parts.push(format!("[{}]", tab.title));
                } else {
                    parts.push(format!(" {} ", tab.title));
                }
            }
            parts.join(" ")
        };
        let header_line = rich_rs::set_cell_size(&header, width);
        let header_segments = Text::plain(header_line).render(console, options);
        let mut lines = Segment::split_and_crop_lines(header_segments, width, None, true, false);
        lines = Segment::set_shape(&lines, width, Some(1), None, false);

        if height > 1 {
            if let Some(tab) = self.tabs.get(self.active) {
                let mut child_options = options.clone();
                child_options.size = (width, height - 1);
                child_options.max_width = width;
                child_options.max_height = height - 1;
                let child_segments = tab.child.render_styled(console, &child_options);
                let mut child_lines =
                    Segment::split_and_crop_lines(child_segments, width, None, true, false);
                child_lines =
                    Segment::set_shape(&child_lines, width, Some(height - 1), None, false);
                lines.extend(child_lines);
            }
        }

        let line_count = lines.len();
        let mut out = Segments::new();
        for (idx, line) in lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn layout_height(&self) -> Option<usize> {
        let child_height = self
            .tabs
            .get(self.active)
            .and_then(|tab| tab.child.layout_height());
        child_height.map(|height| height + 1)
    }
}

impl Renderable for Tabs {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug, Clone)]
pub struct Markdown {
    id: WidgetId,
    markup: String,
}

impl Markdown {
    pub fn new(markup: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            markup: markup.into(),
        }
    }

    pub fn set_markup(&mut self, markup: impl Into<String>) {
        self.markup = markup.into();
    }
}

impl Widget for Markdown {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        RichMarkdown::new(self.markup.clone()).render(console, options)
    }
}

impl Renderable for Markdown {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug, Clone)]
pub struct Checkbox {
    id: WidgetId,
    label: String,
    checked: bool,
    focused: bool,
}

impl Checkbox {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            id: WidgetId::new(),
            label: label.into(),
            checked: false,
            focused: false,
        }
    }

    pub fn checked(&self) -> bool {
        self.checked
    }

    pub fn set_checked(&mut self, checked: bool) {
        self.checked = checked;
    }
}

impl Widget for Checkbox {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.focused {
            return;
        }
        if let Event::Action(Action::Toggle) = event {
            self.checked = !self.checked;
            ctx.set_handled();
            return;
        }
        if let Event::Key(key) = event {
            match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.checked = !self.checked;
                    ctx.set_handled();
                }
                _ => {}
            }
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let marker = if self.focused { "> " } else { "  " };
        let state = if self.checked { "[x]" } else { "[ ]" };
        let text = Text::plain(format!("{marker}{state} {}", self.label));
        text.render(console, options)
    }

    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }
}

impl Renderable for Checkbox {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[derive(Debug, Clone)]
pub struct Input {
    id: WidgetId,
    text: String,
    cursor: usize,
    focused: bool,
    placeholder: Option<String>,
}

impl Input {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            text: String::new(),
            cursor: 0,
            focused: false,
            placeholder: None,
        }
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn value(&self) -> &str {
        &self.text
    }

    pub fn set_value(&mut self, value: impl Into<String>) {
        self.text = value.into();
        self.cursor = self.text.len();
    }
}

impl Widget for Input {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if !self.focused {
            return;
        }
        if let Event::Key(key) = event {
            match key.code {
                KeyCode::Char(ch) => {
                    self.text.insert(self.cursor, ch);
                    self.cursor += 1;
                    ctx.set_handled();
                }
                KeyCode::Backspace => {
                    if self.cursor > 0 {
                        self.cursor -= 1;
                        self.text.remove(self.cursor);
                        ctx.set_handled();
                    }
                }
                KeyCode::Delete => {
                    if self.cursor < self.text.len() {
                        self.text.remove(self.cursor);
                        ctx.set_handled();
                    }
                }
                KeyCode::Left => {
                    if self.cursor > 0 {
                        self.cursor -= 1;
                        ctx.set_handled();
                    }
                }
                KeyCode::Right => {
                    if self.cursor < self.text.len() {
                        self.cursor += 1;
                        ctx.set_handled();
                    }
                }
                KeyCode::Home => {
                    self.cursor = 0;
                    ctx.set_handled();
                }
                KeyCode::End => {
                    self.cursor = self.text.len();
                    ctx.set_handled();
                }
                _ => {}
            }
        }
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let marker = if self.focused { "> " } else { "  " };
        let content = if self.text.is_empty() {
            self.placeholder.clone().unwrap_or_default()
        } else {
            self.text.clone()
        };
        let text = Text::plain(format!("{marker}{content}"));
        text.render(console, options)
    }

    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }
}

impl Renderable for Input {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

pub struct AppRoot {
    id: WidgetId,
    children: Vec<Box<dyn Widget>>,
    focused: Option<usize>,
}

impl AppRoot {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            children: Vec::new(),
            focused: None,
        }
    }

    pub fn with_child(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    pub fn push(&mut self, child: impl Widget + 'static) {
        self.children.push(Box::new(child));
    }

    pub fn focus_first(&mut self) {
        self.focused = None;
        for (idx, child) in self.children.iter_mut().enumerate() {
            if child.focusable() {
                child.set_focus(true);
                self.focused = Some(idx);
                break;
            }
        }
    }

    pub fn focus_next(&mut self) {
        if self.children.is_empty() {
            return;
        }
        let start = self.focused.unwrap_or(usize::MAX);
        if let Some(idx) = self.focused {
            self.children[idx].set_focus(false);
        }
        let mut i = if start == usize::MAX { 0 } else { (start + 1) % self.children.len() };
        let mut visited = 0;
        while visited < self.children.len() {
            if self.children[i].focusable() {
                self.children[i].set_focus(true);
                self.focused = Some(i);
                return;
            }
            i = (i + 1) % self.children.len();
            visited += 1;
        }
        self.focused = None;
    }

    pub fn focus_prev(&mut self) {
        if self.children.is_empty() {
            return;
        }
        let start = self.focused.unwrap_or(0);
        if let Some(idx) = self.focused {
            self.children[idx].set_focus(false);
        }
        let mut i = if start == 0 { self.children.len() - 1 } else { start - 1 };
        let mut visited = 0;
        while visited < self.children.len() {
            if self.children[i].focusable() {
                self.children[i].set_focus(true);
                self.focused = Some(i);
                return;
            }
            if i == 0 {
                i = self.children.len() - 1;
            } else {
                i -= 1;
            }
            visited += 1;
        }
        self.focused = None;
    }

    pub fn focus(&mut self, id: WidgetId) -> bool {
        let target = self
            .children
            .iter()
            .enumerate()
            .find(|(_, child)| child.id() == id && child.focusable())
            .map(|(idx, _)| idx);

        if let Some(idx) = target {
            if let Some(cur) = self.focused {
                self.children[cur].set_focus(false);
            }
            self.children[idx].set_focus(true);
            self.focused = Some(idx);
            return true;
        }

        false
    }
}

impl Default for AppRoot {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for AppRoot {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height_limit = options.size.1.max(1);
        let bounds = rich_rs::Region::from_size(width as u32, height_limit as u32);

        let mut lines: Vec<Vec<Segment>> = Vec::new();
        let mut cursor_y: i32 = 0;

        for child in &self.children {
            let constraints = child.layout_constraints();
            let render_width =
                clamp_with_constraints(width, constraints.min_width, constraints.max_width, width);
            let render_height = clamp_with_constraints(
                height_limit,
                constraints.min_height,
                constraints.max_height,
                height_limit,
            );
            let mut child_options = options.clone();
            child_options.size = (render_width, render_height);
            child_options.max_width = render_width;
            child_options.max_height = render_height;

            let segments = child.render_styled(console, &child_options);
            let mut child_lines =
                Segment::split_and_crop_lines(segments, render_width, None, true, false);
            let mut target_height = child.layout_height().unwrap_or(child_lines.len().max(1));
            target_height = clamp_with_constraints(
                target_height,
                constraints.min_height,
                constraints.max_height,
                target_height,
            );
            child_lines =
                Segment::set_shape(&child_lines, render_width, Some(target_height), None, false);
            child_lines = pad_lines_to_width(child_lines, width);
            let child_height = child_lines.len();
            let child_region =
                rich_rs::Region::new(0, cursor_y, width as u32, child_height as u32);
            if let Some(visible) = child_region.intersection(&bounds) {
                let start = (visible.y - child_region.y).max(0) as usize;
                let end = (start + visible.height as usize).min(child_lines.len());
                for line in child_lines.into_iter().skip(start).take(end - start) {
                    if lines.len() >= height_limit {
                        break;
                    }
                    lines.push(line);
                }
            }
            cursor_y += child_height as i32;
            if cursor_y as usize >= height_limit {
                break;
            }
        }

        let line_count = lines.len();
        let mut out = Segments::new();
        for (idx, line) in lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        let width = options.size.0.max(1);
        let height_limit = options.size.1.max(1);
        let bounds = rich_rs::Region::from_size(width as u32, height_limit as u32);

        let mut lines: Vec<Vec<Segment>> = Vec::new();
        let mut cursor_y: i32 = 0;

        for (idx, child) in self.children.iter().enumerate() {
            let constraints = child.layout_constraints();
            let render_width =
                clamp_with_constraints(width, constraints.min_width, constraints.max_width, width);
            let render_height = clamp_with_constraints(
                height_limit,
                constraints.min_height,
                constraints.max_height,
                height_limit,
            );
            let mut child_options = options.clone();
            child_options.size = (render_width, render_height);
            child_options.max_width = render_width;
            child_options.max_height = render_height;

            let segments = child.render_styled(console, &child_options);
            let mut child_lines =
                Segment::split_and_crop_lines(segments, render_width, None, true, false);
            let mut target_height = child.layout_height().unwrap_or(child_lines.len().max(1));
            target_height = clamp_with_constraints(
                target_height,
                constraints.min_height,
                constraints.max_height,
                target_height,
            );
            child_lines =
                Segment::set_shape(&child_lines, render_width, Some(target_height), None, false);
            child_lines = pad_lines_to_width(child_lines, width);
            let child_height = child_lines.len().max(1);
            let debug_height = (child_height + 2).max(3);
            let child_region =
                rich_rs::Region::new(0, cursor_y, width as u32, debug_height as u32);
            if let Some(visible) = child_region.intersection(&bounds) {
                let start = (visible.y - child_region.y).max(0) as usize;
                let end = (start + visible.height as usize).min(debug_height);
                let label = if debug.show_sizes {
                    Some(format!("{width}x{debug_height}"))
                } else {
                    None
                };
                let wrapped = apply_debug_box(
                    child_lines,
                    width,
                    debug_height,
                    label.as_deref(),
                    debug.style_for(idx),
                );
                for line in wrapped.into_iter().skip(start).take(end - start) {
                    if lines.len() >= height_limit {
                        break;
                    }
                    lines.push(line);
                }
            }
            cursor_y += debug_height as i32;
            if cursor_y as usize >= height_limit {
                break;
            }
        }

        let line_count = lines.len();
        let mut out = Segments::new();
        for (idx, line) in lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn on_mount(&mut self) {
        for child in &mut self.children {
            child.on_mount();
        }
        self.focus_first();
    }

    fn on_unmount(&mut self) {
        for child in &mut self.children {
            child.on_unmount();
        }
    }

    fn on_tick(&mut self, tick: u64) {
        for child in &mut self.children {
            child.on_tick(tick);
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        for child in &mut self.children {
            child.on_resize(width, height);
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        for child in &mut self.children {
            child.on_event_capture(event, ctx);
            if ctx.handled() {
                break;
            }
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Event::Action(action) = event {
            match action {
                Action::FocusNext => {
                    self.focus_next();
                    ctx.set_handled();
                    return;
                }
                Action::FocusPrev => {
                    self.focus_prev();
                    ctx.set_handled();
                    return;
                }
                _ => {}
            }
        }
        if let Event::Key(key) = event {
            if key.code == KeyCode::Tab {
                self.focus_next();
                ctx.set_handled();
                return;
            }
        }

        if let Some(idx) = self.focused {
            self.children[idx].on_event(event, ctx);
            if ctx.handled() {
                return;
            }
        }

        for child in &mut self.children {
            child.on_event(event, ctx);
            if ctx.handled() {
                break;
            }
        }
    }
}

impl Renderable for AppRoot {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

pub struct Frame {
    id: WidgetId,
    child: Box<dyn Widget>,
    padding: usize,
    border: bool,
}

impl Frame {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            id: WidgetId::new(),
            child: Box::new(child),
            padding: 1,
            border: true,
        }
    }

    pub fn padding(mut self, padding: usize) -> Self {
        self.padding = padding;
        self
    }

    pub fn border(mut self, border: bool) -> Self {
        self.border = border;
        self
    }
}

impl Widget for Frame {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let border_width = if self.border { 1 } else { 0 };
        let total_padding = self.padding * 2;

        let width = options.size.0.max(1);
        let height = options.size.1.max(1);

        let inner_width = width.saturating_sub(border_width * 2 + total_padding).max(1);
        let mut inner_height = height.saturating_sub(border_width * 2 + total_padding).max(1);
        if let Some(child_height) = self.child.layout_height() {
            inner_height = inner_height.min(child_height.max(1));
        }

        let mut child_options = options.clone();
        child_options.size = (inner_width, inner_height);
        child_options.max_width = inner_width;
        child_options.max_height = inner_height;

        let child_segments = self.child.render_styled(console, &child_options);
        let mut child_lines =
            Segment::split_and_crop_lines(child_segments, inner_width, None, true, false);
        if let Some(height) = self.child.layout_height() {
            let capped = height.min(inner_height);
            child_lines = Segment::set_shape(&child_lines, inner_width, Some(capped), None, false);
        }

        let padding_line = vec![Segment::new(" ".repeat(inner_width))];
        let mut content_lines: Vec<Vec<Segment>> = Vec::new();
        for _ in 0..self.padding {
            content_lines.push(padding_line.clone());
        }
        content_lines.extend(child_lines.into_iter());
        for _ in 0..self.padding {
            content_lines.push(padding_line.clone());
        }
        content_lines =
            Segment::set_shape(&content_lines, inner_width, Some(inner_height + total_padding), None, false);

        let inner_total = inner_width + total_padding;
        let mut out = Segments::new();
        let line_count = content_lines.len();

        if self.border {
            let b = rich_rs::r#box::SQUARE;
            let top = format!(
                "{}{}{}",
                b.top_left,
                std::iter::repeat(b.top).take(inner_total).collect::<String>(),
                b.top_right
            );
            out.push(Segment::new(top));
            out.push(Segment::line());

            for (idx, line) in content_lines.into_iter().enumerate() {
                out.push(Segment::new(b.mid_left.to_string()));
                if self.padding > 0 {
                    out.push(Segment::new(" ".repeat(self.padding)));
                }
                let adjusted = Segment::adjust_line_length(&line, inner_width, None, true);
                out.extend(adjusted);
                if self.padding > 0 {
                    out.push(Segment::new(" ".repeat(self.padding)));
                }
                out.push(Segment::new(b.mid_right.to_string()));
                if idx + 1 < line_count {
                    out.push(Segment::line());
                }
            }

            let bottom = format!(
                "{}{}{}",
                b.bottom_left,
                std::iter::repeat(b.bottom).take(inner_total).collect::<String>(),
                b.bottom_right
            );
            out.push(Segment::line());
            out.push(Segment::new(bottom));
        } else {
            for (idx, line) in content_lines.into_iter().enumerate() {
                let adjusted = Segment::adjust_line_length(&line, inner_total, None, true);
                out.extend(adjusted);
                if idx + 1 < line_count {
                    out.push(Segment::line());
                }
            }
        }

        out
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let segments = Widget::render(self, console, options);
        let mut lines = Segment::split_and_crop_lines(segments, width, None, true, false);
        let label = if debug.show_sizes {
            Some(format!("{width}x{height}"))
        } else {
            None
        };
        lines = apply_debug_box(lines, width, height, label.as_deref(), debug.style_for(0));
        let line_count = lines.len();
        let mut out = Segments::new();
        for (idx, line) in lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn on_mount(&mut self) {
        self.child.on_mount();
    }

    fn on_unmount(&mut self) {
        self.child.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.child.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.child.on_resize(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event(event, ctx);
    }

    fn layout_height(&self) -> Option<usize> {
        self.child
            .layout_height()
            .map(|h| h + self.padding * 2 + if self.border { 2 } else { 0 })
    }

    fn focusable(&self) -> bool {
        self.child.focusable()
    }

    fn set_focus(&mut self, focused: bool) {
        self.child.set_focus(focused);
    }
}

impl Renderable for Frame {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

pub struct ScrollView {
    id: WidgetId,
    child: Box<dyn Widget>,
    height: Option<usize>,
    offset_y: usize,
    scroll_step: usize,
    content_height: AtomicUsize,
    viewport_height: AtomicUsize,
    offset_x: usize,
    scroll_step_x: usize,
    content_width: AtomicUsize,
    viewport_width: AtomicUsize,
}

impl ScrollView {
    pub fn new(child: impl Widget + 'static) -> Self {
        Self {
            id: WidgetId::new(),
            child: Box::new(child),
            height: None,
            offset_y: 0,
            scroll_step: 1,
            content_height: AtomicUsize::new(0),
            viewport_height: AtomicUsize::new(0),
            offset_x: 0,
            scroll_step_x: 2,
            content_width: AtomicUsize::new(0),
            viewport_width: AtomicUsize::new(0),
        }
    }

    pub fn height(mut self, height: usize) -> Self {
        self.height = Some(height.max(1));
        self
    }

    pub fn scroll_to(&mut self, offset_y: usize) {
        self.offset_y = offset_y;
        self.clamp_offset();
    }

    pub fn scroll_to_x(&mut self, offset_x: usize) {
        self.offset_x = offset_x;
        self.clamp_offset();
    }

    pub fn scroll_by(&mut self, delta: i32) {
        if delta.is_negative() {
            self.offset_y = self.offset_y.saturating_sub(delta.unsigned_abs() as usize);
        } else {
            self.offset_y = self.offset_y.saturating_add(delta as usize);
        }
        self.clamp_offset();
    }

    pub fn scroll_by_x(&mut self, delta: i32) {
        if delta.is_negative() {
            self.offset_x = self.offset_x.saturating_sub(delta.unsigned_abs() as usize);
        } else {
            self.offset_x = self.offset_x.saturating_add(delta as usize);
        }
        self.clamp_offset();
    }

    pub fn scroll_step(mut self, step: usize) -> Self {
        self.scroll_step = step.max(1);
        self
    }

    pub fn scroll_step_x(mut self, step: usize) -> Self {
        self.scroll_step_x = step.max(1);
        self
    }

    pub fn offset_y(&self) -> usize {
        self.offset_y
    }

    pub fn offset_x(&self) -> usize {
        self.offset_x
    }

    fn max_offset(&self) -> usize {
        let content = self.content_height.load(Ordering::Relaxed);
        let viewport = self.viewport_height.load(Ordering::Relaxed).max(1);
        content.saturating_sub(viewport)
    }

    fn max_offset_x(&self) -> usize {
        let content = self.content_width.load(Ordering::Relaxed);
        let viewport = self.viewport_width.load(Ordering::Relaxed).max(1);
        content.saturating_sub(viewport)
    }

    fn clamp_offset(&mut self) {
        let max_y = self.max_offset();
        if self.offset_y > max_y {
            self.offset_y = max_y;
        }
        let max_x = self.max_offset_x();
        if self.offset_x > max_x {
            self.offset_x = max_x;
        }
    }
}

impl Widget for ScrollView {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let viewport_height = self.height.unwrap_or_else(|| options.size.1.max(1));
        self.viewport_height
            .store(viewport_height, Ordering::Relaxed);
        self.viewport_width
            .store(width, Ordering::Relaxed);

        let constraints = self.child.layout_constraints();
        let target_height = self
            .child
            .layout_height()
            .unwrap_or_else(|| viewport_height.saturating_add(self.offset_y).max(1));
        let render_width =
            clamp_with_constraints(width, constraints.min_width, constraints.max_width, width);
        let render_height = clamp_with_constraints(
            target_height,
            constraints.min_height,
            constraints.max_height,
            target_height,
        );
        let mut child_options = options.clone();
        child_options.size = (render_width, render_height);
        child_options.max_width = render_width;
        child_options.max_height = render_height;

        let segments = self.child.render_styled(console, &child_options);
        let mut lines = Segment::split_and_crop_lines(segments, render_width, None, true, false);
        if let Some(height) = self.child.layout_height() {
            lines = Segment::set_shape(&lines, render_width, Some(height.max(1)), None, false);
        }
        lines = pad_lines_to_width(lines, width);

        let content_height = lines.len().max(viewport_height);
        self.content_height
            .store(content_height, Ordering::Relaxed);
        let content_width = lines
            .iter()
            .map(|line| Segment::get_line_length(line))
            .max()
            .unwrap_or(width)
            .max(width);
        self.content_width
            .store(content_width, Ordering::Relaxed);

        let max_offset = content_height.saturating_sub(viewport_height);
        let offset = self.offset_y.min(max_offset);
        let max_offset_x = content_width.saturating_sub(width);
        let offset_x = self.offset_x.min(max_offset_x);
        let start = offset.min(lines.len());
        let end = (start + viewport_height).min(lines.len());
        let slice = lines[start..end]
            .to_vec()
            .into_iter()
            .map(|line| {
                let cropped = crop_line_horizontal(&line, offset_x, width);
                Segment::adjust_line_length(&cropped, width, None, true)
            })
            .collect::<Vec<_>>();
        let slice = Segment::set_shape(&slice, width, Some(viewport_height), None, false);

        let line_count = slice.len();
        let mut out = Segments::new();
        for (idx, line) in slice.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        let width = options.size.0.max(1);
        let height = self.height.unwrap_or_else(|| options.size.1.max(1));
        let segments = Widget::render(self, console, options);
        let mut lines = Segment::split_and_crop_lines(segments, width, None, true, false);
        let label = if debug.show_sizes {
            Some(format!("{width}x{height}"))
        } else {
            None
        };
        lines = apply_debug_box(lines, width, height.max(3), label.as_deref(), debug.style_for(0));
        let line_count = lines.len();
        let mut out = Segments::new();
        for (idx, line) in lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn on_mount(&mut self) {
        self.child.on_mount();
    }

    fn on_unmount(&mut self) {
        self.child.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.child.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.child.on_resize(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.child.on_event_capture(event, ctx);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        let mut child_ctx = EventCtx::default();
        self.child.on_event(event, &mut child_ctx);
        if child_ctx.handled() {
            ctx.set_handled();
            return;
        }
        if let Event::Action(action) = event {
            match action {
                Action::ScrollUp => {
                    self.scroll_by(-(self.scroll_step as i32));
                    ctx.set_handled();
                    return;
                }
                Action::ScrollDown => {
                    self.scroll_by(self.scroll_step as i32);
                    ctx.set_handled();
                    return;
                }
                Action::ScrollPageUp => {
                    let page = self.height.unwrap_or(1).max(1);
                    self.scroll_by(-(page as i32));
                    ctx.set_handled();
                    return;
                }
                Action::ScrollPageDown => {
                    let page = self.height.unwrap_or(1).max(1);
                    self.scroll_by(page as i32);
                    ctx.set_handled();
                    return;
                }
                Action::ScrollLeft => {
                    self.scroll_by_x(-(self.scroll_step_x as i32));
                    ctx.set_handled();
                    return;
                }
                Action::ScrollRight => {
                    self.scroll_by_x(self.scroll_step_x as i32);
                    ctx.set_handled();
                    return;
                }
                Action::ScrollPageLeft => {
                    let page = self.viewport_width.load(Ordering::Relaxed).max(1);
                    self.scroll_by_x(-(page as i32));
                    ctx.set_handled();
                    return;
                }
                Action::ScrollPageRight => {
                    let page = self.viewport_width.load(Ordering::Relaxed).max(1);
                    self.scroll_by_x(page as i32);
                    ctx.set_handled();
                    return;
                }
                _ => {}
            }
        }
        self.child.on_event(event, ctx);
    }

    fn layout_height(&self) -> Option<usize> {
        self.height
    }
}

impl Renderable for ScrollView {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

pub struct Grid {
    id: WidgetId,
    rows: usize,
    cols: usize,
    cells: Vec<Option<Box<dyn Widget>>>,
    row_gaps: usize,
    col_gaps: usize,
    row_sizes: Option<Vec<usize>>,
    col_sizes: Option<Vec<usize>>,
}

impl Grid {
    pub fn new(rows: usize, cols: usize) -> Self {
        let rows = rows.max(1);
        let cols = cols.max(1);
        Self {
            id: WidgetId::new(),
            rows,
            cols,
            cells: (0..rows * cols).map(|_| None).collect(),
            row_gaps: 0,
            col_gaps: 0,
            row_sizes: None,
            col_sizes: None,
        }
    }

    pub fn set(&mut self, row: usize, col: usize, child: impl Widget + 'static) {
        if row >= self.rows || col >= self.cols {
            return;
        }
        let idx = row * self.cols + col;
        self.cells[idx] = Some(Box::new(child));
    }

    pub fn with_cell(mut self, row: usize, col: usize, child: impl Widget + 'static) -> Self {
        self.set(row, col, child);
        self
    }

    pub fn row_gap(mut self, gap: usize) -> Self {
        self.row_gaps = gap;
        self
    }

    pub fn col_gap(mut self, gap: usize) -> Self {
        self.col_gaps = gap;
        self
    }

    pub fn row_sizes(mut self, sizes: Vec<usize>) -> Self {
        if sizes.len() == self.rows {
            self.row_sizes = Some(sizes);
        }
        self
    }

    pub fn col_sizes(mut self, sizes: Vec<usize>) -> Self {
        if sizes.len() == self.cols {
            self.col_sizes = Some(sizes);
        }
        self
    }
}

impl Widget for Grid {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);

        let total_col_gaps = self.col_gaps.saturating_mul(self.cols.saturating_sub(1));
        let total_row_gaps = self.row_gaps.saturating_mul(self.rows.saturating_sub(1));
        let inner_width = width.saturating_sub(total_col_gaps).max(1);
        let inner_height = height.saturating_sub(total_row_gaps).max(1);

        let col_widths: Vec<usize> = if let Some(sizes) = &self.col_sizes {
            sizes.clone()
        } else {
            let base_w = inner_width / self.cols;
            let rem_w = inner_width % self.cols;
            (0..self.cols)
                .map(|c| base_w + if c < rem_w { 1 } else { 0 })
                .collect()
        };

        let row_heights: Vec<usize> = if let Some(sizes) = &self.row_sizes {
            sizes.clone()
        } else {
            let base_h = inner_height / self.rows;
            let rem_h = inner_height % self.rows;
            (0..self.rows)
                .map(|r| base_h + if r < rem_h { 1 } else { 0 })
                .collect()
        };

        let mut cell_lines: Vec<Vec<Vec<Vec<Segment>>>> = Vec::new();
        for r in 0..self.rows {
            let mut row_cells = Vec::new();
            for c in 0..self.cols {
                let idx = r * self.cols + c;
                let cell_width = col_widths[c].max(1);
                let cell_height = row_heights[r].max(1);
                let constraints = self.cells[idx]
                    .as_ref()
                    .map(|child| child.layout_constraints())
                    .unwrap_or_default();
                let render_width = clamp_with_constraints(
                    cell_width,
                    constraints.min_width,
                    constraints.max_width,
                    cell_width,
                );
                let render_height = clamp_with_constraints(
                    cell_height,
                    constraints.min_height,
                    constraints.max_height,
                    cell_height,
                );
                let mut child_options = options.clone();
                child_options.size = (render_width, render_height);
                child_options.max_width = render_width;
                child_options.max_height = render_height;
                let lines = if let Some(child) = &self.cells[idx] {
                    let segments = child.render_styled(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, render_width, None, true, false);
                    lines = Segment::set_shape(&lines, render_width, Some(render_height), None, false);
                    lines = pad_lines_to_width(lines, cell_width);
                    lines
                } else {
                    Segment::set_shape(&[], cell_width, Some(cell_height), None, false)
                };
                row_cells.push(lines);
            }
            cell_lines.push(row_cells);
        }

        let mut out_lines: Vec<Vec<Segment>> = Vec::new();
        for r in 0..self.rows {
            let cell_height = row_heights[r].max(1);
            for row in 0..cell_height {
                let mut line: Vec<Segment> = Vec::new();
                for c in 0..self.cols {
                    let cell_width = col_widths[c].max(1);
                    let lines = &cell_lines[r][c];
                    let cell_line = lines.get(row).cloned().unwrap_or_else(|| {
                        vec![Segment::new(" ".repeat(cell_width))]
                    });
                    let adjusted = Segment::adjust_line_length(&cell_line, cell_width, None, true);
                    line.extend(adjusted);
                    if c + 1 < self.cols && self.col_gaps > 0 {
                        line.push(Segment::new(" ".repeat(self.col_gaps)));
                    }
                }
                out_lines.push(line);
            }
            if r + 1 < self.rows && self.row_gaps > 0 {
                let gap_line = vec![Segment::new(" ".repeat(width))];
                for _ in 0..self.row_gaps {
                    out_lines.push(gap_line.clone());
                }
            }
        }

        let line_count = out_lines.len();
        let mut out = Segments::new();
        for (idx, line) in out_lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn render_with_debug(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        debug: &DebugLayout,
    ) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);

        let total_col_gaps = self.col_gaps.saturating_mul(self.cols.saturating_sub(1));
        let total_row_gaps = self.row_gaps.saturating_mul(self.rows.saturating_sub(1));
        let inner_width = width.saturating_sub(total_col_gaps).max(1);
        let inner_height = height.saturating_sub(total_row_gaps).max(1);

        let col_widths: Vec<usize> = if let Some(sizes) = &self.col_sizes {
            sizes.clone()
        } else {
            let base_w = inner_width / self.cols;
            let rem_w = inner_width % self.cols;
            (0..self.cols)
                .map(|c| base_w + if c < rem_w { 1 } else { 0 })
                .collect()
        };

        let row_heights: Vec<usize> = if let Some(sizes) = &self.row_sizes {
            sizes.clone()
        } else {
            let base_h = inner_height / self.rows;
            let rem_h = inner_height % self.rows;
            (0..self.rows)
                .map(|r| base_h + if r < rem_h { 1 } else { 0 })
                .collect()
        };

        let mut cell_lines: Vec<Vec<Vec<Vec<Segment>>>> = Vec::new();
        let mut cell_index = 0;
        for r in 0..self.rows {
            let mut row_cells = Vec::new();
            for c in 0..self.cols {
                let idx = r * self.cols + c;
                let cell_width = col_widths[c].max(1);
                let cell_height = row_heights[r].max(1);
                let constraints = self.cells[idx]
                    .as_ref()
                    .map(|child| child.layout_constraints())
                    .unwrap_or_default();
                let render_width = clamp_with_constraints(
                    cell_width,
                    constraints.min_width,
                    constraints.max_width,
                    cell_width,
                );
                let render_height = clamp_with_constraints(
                    cell_height,
                    constraints.min_height,
                    constraints.max_height,
                    cell_height,
                );
                let mut child_options = options.clone();
                child_options.size = (render_width, render_height);
                child_options.max_width = render_width;
                child_options.max_height = render_height;
                let lines = if let Some(child) = &self.cells[idx] {
                    let segments = child.render_styled(console, &child_options);
                    let mut lines =
                        Segment::split_and_crop_lines(segments, render_width, None, true, false);
                    lines = Segment::set_shape(&lines, render_width, Some(render_height), None, false);
                    lines = pad_lines_to_width(lines, cell_width);
                    let label = if debug.show_sizes {
                        Some(format!("{cell_width}x{cell_height}"))
                    } else {
                        None
                    };
                    apply_debug_box(
                        lines,
                        cell_width,
                        (cell_height + 2).max(3),
                        label.as_deref(),
                        debug.style_for(cell_index),
                    )
                } else {
                    Segment::set_shape(&[], cell_width, Some(cell_height), None, false)
                };
                row_cells.push(lines);
                cell_index += 1;
            }
            cell_lines.push(row_cells);
        }

        let mut out_lines: Vec<Vec<Segment>> = Vec::new();
        for r in 0..self.rows {
            let cell_height = row_heights[r].max(1);
            for row in 0..cell_height {
                let mut line: Vec<Segment> = Vec::new();
                for c in 0..self.cols {
                    let cell_width = col_widths[c].max(1);
                    let lines = &cell_lines[r][c];
                    let cell_line = lines.get(row).cloned().unwrap_or_else(|| {
                        vec![Segment::new(" ".repeat(cell_width))]
                    });
                    let adjusted = Segment::adjust_line_length(&cell_line, cell_width, None, true);
                    line.extend(adjusted);
                    if c + 1 < self.cols && self.col_gaps > 0 {
                        line.push(Segment::new(" ".repeat(self.col_gaps)));
                    }
                }
                out_lines.push(line);
            }
            if r + 1 < self.rows && self.row_gaps > 0 {
                let gap_line = vec![Segment::new(" ".repeat(width))];
                for _ in 0..self.row_gaps {
                    out_lines.push(gap_line.clone());
                }
            }
        }

        let line_count = out_lines.len();
        let mut out = Segments::new();
        for (idx, line) in out_lines.into_iter().enumerate() {
            out.extend(line);
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn on_mount(&mut self) {
        for cell in &mut self.cells {
            if let Some(child) = cell {
                child.on_mount();
            }
        }
    }

    fn on_unmount(&mut self) {
        for cell in &mut self.cells {
            if let Some(child) = cell {
                child.on_unmount();
            }
        }
    }

    fn on_tick(&mut self, tick: u64) {
        for cell in &mut self.cells {
            if let Some(child) = cell {
                child.on_tick(tick);
            }
        }
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        for cell in &mut self.cells {
            if let Some(child) = cell {
                child.on_resize(width, height);
            }
        }
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        for cell in &mut self.cells {
            if let Some(child) = cell {
                child.on_event_capture(event, ctx);
                if ctx.handled() {
                    break;
                }
            }
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        for cell in &mut self.cells {
            if let Some(child) = cell {
                child.on_event(event, ctx);
                if ctx.handled() {
                    break;
                }
            }
        }
    }
}

impl Renderable for Grid {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

pub struct Overlay {
    id: WidgetId,
    base: Box<dyn Widget>,
    modal: Box<dyn Widget>,
    visible: bool,
}

impl Overlay {
    pub fn new(base: impl Widget + 'static, modal: impl Widget + 'static) -> Self {
        Self {
            id: WidgetId::new(),
            base: Box::new(base),
            modal: Box::new(modal),
            visible: true,
        }
    }

    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }
}

impl Widget for Overlay {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        if !self.visible {
            return self.base.render_styled(console, options);
        }
        let base_renderable = WidgetRenderable::new(self.base.as_ref());
        let modal_renderable = WidgetRenderable::new(self.modal.as_ref());
        let base = crate::render::FrameBuffer::from_renderable(
            console,
            options,
            &base_renderable,
            None,
        );
        let top = crate::render::FrameBuffer::from_renderable(
            console,
            options,
            &modal_renderable,
            None,
        );
        let mut merged = base.clone();
        for y in 0..base.height {
            for x in 0..base.width {
                let cell = top.get(x, y);
                if !cell.continuation && !cell.text.is_empty() && cell.text != " " {
                    let out = merged.get_mut(x, y);
                    *out = cell.clone();
                }
            }
        }
        let lines = merged.as_plain_lines().join("\n");
        Text::plain(lines).render(console, options)
    }

    fn on_mount(&mut self) {
        self.base.on_mount();
        self.modal.on_mount();
    }

    fn on_unmount(&mut self) {
        self.base.on_unmount();
        self.modal.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.base.on_tick(tick);
        self.modal.on_tick(tick);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        self.base.on_resize(width, height);
        self.modal.on_resize(width, height);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.modal.on_event_capture(event, ctx);
        if !ctx.handled() {
            self.base.on_event_capture(event, ctx);
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.modal.on_event(event, ctx);
        if !ctx.handled() {
            self.base.on_event(event, ctx);
        }
    }
}

impl Renderable for Overlay {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
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
