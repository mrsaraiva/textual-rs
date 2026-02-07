use std::cell::RefCell;
use std::time::Duration;

use rich_rs::{MetaValue, Segments};

use crate::debug::debug_style;
use crate::style::{
    BorderEdge, BorderType, Margin, Style, Tint, TransitionTiming, parse_color_like,
};

use crate::widgets::{Widget, WidgetId};

thread_local! {
    static STYLE_CONTEXT: RefCell<Option<StyleSheet>> = RefCell::new(None);
    static STYLE_STACK: RefCell<Vec<Style>> = RefCell::new(Vec::new());
    static SELECTOR_STACK: RefCell<Vec<SelectorMeta>> = RefCell::new(Vec::new());
    static APP_ACTIVE: RefCell<bool> = RefCell::new(true);
}

pub struct AppActiveGuard(bool);

pub fn set_app_active(active: bool) -> AppActiveGuard {
    let prev = APP_ACTIVE.with(|v| {
        let mut guard = v.borrow_mut();
        let prev = *guard;
        *guard = active;
        prev
    });
    AppActiveGuard(prev)
}

impl Drop for AppActiveGuard {
    fn drop(&mut self) {
        let prev = self.0;
        APP_ACTIVE.with(|v| {
            *v.borrow_mut() = prev;
        });
    }
}

fn app_is_active() -> bool {
    APP_ACTIVE.with(|v| *v.borrow())
}

#[derive(Debug, Clone, Default)]
pub struct StyleSelector {
    type_name: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
    pseudos: Vec<PseudoClass>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PseudoClass {
    Disabled,
    Focus,
    Hover,
    Active,
}

impl StyleSelector {
    pub fn new(type_name: impl Into<String>) -> Self {
        Self {
            type_name: Some(type_name.into()),
            id: None,
            classes: Vec::new(),
            pseudos: Vec::new(),
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

    pub fn pseudo(mut self, pseudo: PseudoClass) -> Self {
        self.pseudos.push(pseudo);
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
        if !self.pseudos.is_empty() {
            for pseudo in &self.pseudos {
                let ok = match pseudo {
                    PseudoClass::Disabled => meta.states.disabled,
                    PseudoClass::Focus => meta.states.focused,
                    PseudoClass::Hover => meta.states.hovered,
                    PseudoClass::Active => meta.states.active,
                };
                if !ok {
                    return false;
                }
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
        score = score.saturating_add(self.pseudos.len().saturating_mul(10) as u8);
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

    pub fn extend(&mut self, other: &StyleSheet) {
        self.rules.extend(other.rules.iter().cloned());
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
        self.style_for_meta(meta)
    }

    fn style_for_meta(&self, meta: &SelectorMeta) -> Style {
        let mut matches: Vec<(u8, usize, Style)> = Vec::new();
        for (idx, rule) in self.rules.iter().enumerate() {
            if let Some(score) = rule_specificity(rule, meta) {
                matches.push((score, idx, rule.style));
                if std::env::var("TEXTUAL_DEBUG_STYLE_FILE").is_ok()
                    && meta.type_name == "VerticalScroll"
                {
                    debug_style(&format!(
                        "[style] match widget={} selector=\"{}\" score={} width=({:?},{:?}) width_auto={:?}",
                        meta.type_name,
                        selector_chain_string(&rule.selector_chain),
                        score,
                        rule.style.min_width,
                        rule.style.max_width,
                        rule.style.width_auto
                    ));
                }
            }
        }
        matches.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        let mut out = Style::new();
        for (_, _, style) in matches {
            out = out.combine(&style);
        }
        if std::env::var("TEXTUAL_DEBUG_STYLE_FILE").is_ok() && meta.type_name == "VerticalScroll" {
            let stack = SELECTOR_STACK.with(|stack| {
                stack
                    .borrow()
                    .iter()
                    .map(|m| m.type_name.clone())
                    .collect::<Vec<_>>()
            });
            debug_style(&format!(
                "[style] resolved widget={} stack={:?} width=({:?},{:?}) width_auto={:?}",
                meta.type_name, stack, out.min_width, out.max_width, out.width_auto
            ));
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

pub struct StyleContextGuard(Option<StyleSheet>);

pub fn set_style_context(stylesheet: StyleSheet) -> StyleContextGuard {
    let prev = STYLE_CONTEXT.with(|ctx| ctx.borrow_mut().replace(stylesheet));
    StyleContextGuard(prev)
}

impl Drop for StyleContextGuard {
    fn drop(&mut self) {
        let prev = self.0.take();
        STYLE_CONTEXT.with(|ctx| {
            *ctx.borrow_mut() = prev;
        });
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SelectorMeta {
    type_name: String,
    id: Option<String>,
    classes: Vec<String>,
    states: SelectorStates,
}

#[derive(Debug, Clone, Copy, Default)]
struct SelectorStates {
    disabled: bool,
    focused: bool,
    hovered: bool,
    active: bool,
}

pub(crate) fn selector_meta_generic<T: Widget + ?Sized>(widget: &T) -> SelectorMeta {
    SelectorMeta {
        type_name: widget.style_type().to_string(),
        id: widget.style_id().map(|value| value.to_string()),
        classes: widget.style_classes().to_vec(),
        states: SelectorStates {
            disabled: widget.is_disabled(),
            focused: widget.has_focus() && app_is_active(),
            hovered: widget.is_hovered(),
            active: widget.is_active(),
        },
    }
}

pub(crate) fn selector_meta_component(parent_type: &str, classes: &[&str]) -> SelectorMeta {
    SelectorMeta {
        type_name: parent_type.to_string(),
        id: None,
        classes: classes.iter().map(|s| (*s).to_string()).collect(),
        states: SelectorStates::default(),
    }
}

pub(crate) fn selector_meta_component_for<T: Widget + ?Sized>(
    widget: &T,
    classes: &[&str],
) -> SelectorMeta {
    SelectorMeta {
        type_name: widget.style_type().to_string(),
        id: None,
        classes: classes.iter().map(|s| (*s).to_string()).collect(),
        states: SelectorStates {
            disabled: widget.is_disabled(),
            focused: widget.has_focus() && app_is_active(),
            hovered: widget.is_hovered(),
            active: widget.is_active(),
        },
    }
}

pub(crate) fn selector_meta_component_for_with_id<T: Widget + ?Sized>(
    widget: &T,
    id: Option<&str>,
    classes: &[&str],
) -> SelectorMeta {
    SelectorMeta {
        type_name: widget.style_type().to_string(),
        id: id.map(str::to_string),
        classes: classes.iter().map(|s| (*s).to_string()).collect(),
        states: SelectorStates {
            disabled: widget.is_disabled(),
            focused: widget.has_focus() && app_is_active(),
            hovered: widget.is_hovered(),
            active: widget.is_active(),
        },
    }
}

pub(crate) fn current_parent_style() -> Option<Style> {
    STYLE_STACK.with(|stack| stack.borrow().last().copied())
}

pub(crate) fn resolve_style<T: Widget + ?Sized>(widget: &T, meta: &SelectorMeta) -> Style {
    let sheet_style = STYLE_CONTEXT
        .with(|ctx| {
            ctx.borrow()
                .as_ref()
                .map(|sheet| sheet.style_for(widget, meta))
        })
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

pub(crate) fn resolve_style_for_meta(meta: &SelectorMeta) -> Style {
    let sheet_style = STYLE_CONTEXT
        .with(|ctx| {
            ctx.borrow()
                .as_ref()
                .map(|sheet| sheet.style_for_meta(meta))
        })
        .unwrap_or_default();
    let mut style = sheet_style;
    if let Some(parent) = STYLE_STACK.with(|stack| stack.borrow().last().copied()) {
        style = style.inherit_from(&parent);
    }
    style
}

pub(crate) fn resolve_component_style<T: Widget + ?Sized>(widget: &T, classes: &[&str]) -> Style {
    let parent_meta = selector_meta_generic(widget);
    let meta = selector_meta_component_for(widget, classes);
    SELECTOR_STACK.with(|stack| {
        stack.borrow_mut().push(parent_meta);
        let out = resolve_style_for_meta(&meta);
        stack.borrow_mut().pop();
        out
    })
}

pub(crate) fn resolve_component_style_with_id<T: Widget + ?Sized>(
    widget: &T,
    id: Option<&str>,
    classes: &[&str],
) -> Style {
    let parent_meta = selector_meta_generic(widget);
    let meta = selector_meta_component_for_with_id(widget, id, classes);
    SELECTOR_STACK.with(|stack| {
        stack.borrow_mut().push(parent_meta);
        let out = resolve_style_for_meta(&meta);
        stack.borrow_mut().pop();
        out
    })
}

pub(crate) fn with_style_stack<T>(meta: SelectorMeta, resolved: Style, f: impl FnOnce() -> T) -> T {
    STYLE_STACK.with(|style_stack| {
        SELECTOR_STACK.with(|selector_stack| {
            style_stack.borrow_mut().push(resolved);
            selector_stack.borrow_mut().push(meta);
            let out = f();
            selector_stack.borrow_mut().pop();
            style_stack.borrow_mut().pop();
            out
        })
    })
}

pub(crate) fn apply_style_to_segments(
    widget_id: WidgetId,
    segments: Segments,
    style: Style,
    parent_style: Option<Style>,
) -> Segments {
    if style.is_empty() {
        return segments;
    }
    let rich_attrs = style.to_rich_without_colors();
    let fallback_bg = crate::style::parse_color_like("$background");
    let parent_bg = parent_style.and_then(|s| s.bg).or(fallback_bg);
    segments
        .into_iter()
        .map(|mut seg| {
            if seg.control.is_some() {
                return seg;
            }

            // Only apply this widget's resolved style to segments that originate from this widget.
            // Child widgets render their own styles already (including inherited properties), and
            // parent widgets should not overwrite them during this pass.
            if let Some(meta) = seg.meta.as_ref().and_then(|meta| meta.meta.as_ref()) {
                if let Some(MetaValue::Int(value)) = meta.get("textual:widget_id") {
                    if *value != widget_id.as_u64() as i64 {
                        return seg;
                    }
                }
            }

            let rich_attrs = rich_attrs;
            if let Some(meta) = seg.meta.as_ref().and_then(|meta| meta.meta.as_ref()) {
                if let Some(MetaValue::Bool(true)) = meta.get("textual:no_style") {
                    return seg;
                }
                if let Some(MetaValue::Bool(true)) = meta.get("textual:no_bg") {
                    // We'll clear bgcolor after composing below.
                }
            }
            if let Some(rich_attrs) = rich_attrs {
                seg.style = Some(match seg.style {
                    Some(existing) => rich_attrs.combine(&existing),
                    None => rich_attrs,
                });
            }
            if let Some(mut s) = seg.style {
                let mut no_bg = false;
                if let Some(meta) = seg.meta.as_ref().and_then(|meta| meta.meta.as_ref()) {
                    if let Some(MetaValue::Bool(true)) = meta.get("textual:no_bg") {
                        no_bg = true;
                    }
                }

                let mut under_bg = s
                    .bgcolor
                    .map(crate::style::color_from_simple)
                    .or(parent_bg)
                    .unwrap_or(crate::style::Color::rgb(0, 0, 0));

                if !no_bg {
                    if let Some(bg) = style.bg {
                        // Preserve per-segment backgrounds (e.g. DataTable row/cell backgrounds,
                        // Input selection/cursor) unless the segment has no background.
                        if s.bgcolor.is_none() {
                            let flat = bg.flatten_over(under_bg);
                            under_bg = flat;
                            s.bgcolor = Some(flat.to_simple_opaque());
                        }
                    }
                } else {
                    s.bgcolor = None;
                }

                if let Some(fg) = style.fg {
                    // Preserve per-segment foregrounds unless unset.
                    if s.color.is_none() {
                        let bg_for_text = s
                            .bgcolor
                            .map(crate::style::color_from_simple)
                            .unwrap_or(under_bg);
                        let flat = fg.flatten_over(bg_for_text);
                        s.color = Some(flat.to_simple_opaque());
                    }
                }

                if let Some(tint) = style.background_tint {
                    if let Some(bg) = s.bgcolor {
                        let bg = crate::style::color_from_simple(bg);
                        let blended = crate::style::blend_colors(bg, tint.color, tint.percent);
                        let flat = blended.flatten_over(under_bg);
                        under_bg = flat;
                        s.bgcolor = Some(flat.to_simple_opaque());
                    }
                }
                if let Some(tint) = style.tint {
                    if let Some(bg) = s.bgcolor {
                        let bg = crate::style::color_from_simple(bg);
                        let blended = crate::style::blend_colors(bg, tint.color, tint.percent);
                        s.bgcolor = Some(blended.to_simple_opaque());
                    }
                }
                seg.style = Some(s);
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
    // Selector stack contains ancestors only (the current widget meta is not pushed until after
    // style resolution). For child combinators we need to start matching from the immediate parent.
    let mut idx = stack_snapshot.len() as isize - 1;
    if idx < 0 {
        return None;
    }
    let combinators = &rule.selector_chain.combinators;
    let parts = &rule.selector_chain.parts;
    for (part_index, selector) in parts[..parts.len() - 1].iter().rev().enumerate() {
        let comb = combinators[combinators.len() - 1 - part_index];
        match comb {
            Combinator::Child => {
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

fn selector_chain_string(chain: &SelectorChain) -> String {
    let mut out = String::new();
    for (idx, part) in chain.parts.iter().enumerate() {
        if idx > 0 {
            let comb = chain.combinators[idx - 1];
            match comb {
                Combinator::Child => out.push_str(" > "),
                Combinator::Descendant => out.push(' '),
            }
        }
        if let Some(name) = &part.type_name {
            out.push_str(name);
        }
        for class in &part.classes {
            out.push('.');
            out.push_str(class);
        }
        if let Some(id) = &part.id {
            out.push('#');
            out.push_str(id);
        }
        for pseudo in &part.pseudos {
            out.push(':');
            match pseudo {
                PseudoClass::Disabled => out.push_str("disabled"),
                PseudoClass::Focus => out.push_str("focus"),
                PseudoClass::Hover => out.push_str("hover"),
                PseudoClass::Active => out.push_str("active"),
            }
        }
    }
    out
}

fn parse_selector(selector: &str) -> Option<StyleSelector> {
    let selector = selector.trim();
    if selector.is_empty() {
        return None;
    }

    // Split off pseudo-classes (`Button:disabled`, `.foo:focus`, etc).
    let mut pseudo_parts = selector.split(':');
    let base_selector = pseudo_parts.next().unwrap_or("").trim();
    let pseudos: Vec<PseudoClass> = pseudo_parts
        .filter_map(|part| {
            let name = part.trim();
            if name.is_empty() {
                return None;
            }
            // Ignore any `:pseudo(...)` forms for now.
            let name = name.split('(').next().unwrap_or(name).trim().to_lowercase();
            match name.as_str() {
                "disabled" => Some(PseudoClass::Disabled),
                "focus" | "focused" => Some(PseudoClass::Focus),
                "hover" => Some(PseudoClass::Hover),
                "active" => Some(PseudoClass::Active),
                _ => None,
            }
        })
        .collect();

    let mut type_name: Option<String> = None;
    let mut id: Option<String> = None;
    let mut classes: Vec<String> = Vec::new();

    let mut chars = base_selector.chars().peekable();
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
    for pseudo in pseudos {
        selector = selector.pseudo(pseudo);
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
                if let Some(color) = parse_color_like(value) {
                    style = style.fg(color);
                }
            }
            "bg" | "background" => {
                if let Some(color) = parse_color_like(value) {
                    style = style.bg(color);
                }
            }
            "width" => {
                if value.trim().eq_ignore_ascii_case("auto") {
                    style.width_auto = Some(true);
                } else if let Ok(value) = value.parse() {
                    style = style.width(value);
                }
            }
            "height" => {
                if value.trim().eq_ignore_ascii_case("auto") {
                    style.height_auto = Some(true);
                } else if let Ok(value) = value.parse() {
                    style = style.height(value);
                }
            }
            "min-width" => {
                if let Ok(value) = value.parse() {
                    style = style.min_width(value);
                }
            }
            "max-width" => {
                if let Ok(value) = value.parse() {
                    style = style.max_width(value);
                }
            }
            "min-height" => {
                if let Ok(value) = value.parse() {
                    style = style.min_height(value);
                }
            }
            "max-height" => {
                if let Ok(value) = value.parse() {
                    style = style.max_height(value);
                }
            }
            "margin" => {
                if let Some(margin) = parse_margin(value) {
                    style = style.margin(margin);
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
            "tint" => {
                if let Some(tint) = parse_tint(value) {
                    style.tint = Some(tint);
                }
            }
            "background-tint" => {
                if let Some(tint) = parse_tint(value) {
                    style.background_tint = Some(tint);
                }
            }
            "text-style" => {
                for token in value.split(|c: char| c == ' ' || c == ',' || c == '|') {
                    let token = token.trim();
                    if token.is_empty() {
                        continue;
                    }
                    match token {
                        "bold" => style = style.bold(true),
                        "dim" => style = style.dim(true),
                        "italic" => style = style.italic(true),
                        "underline" => style = style.underline(true),
                        "reverse" => style = style.reverse(true),
                        "$button-focus-text-style" => style = style.reverse(true),
                        _ => {}
                    }
                }
            }
            "line-pad" => {
                if let Ok(value) = value.parse() {
                    style = style.line_pad(value);
                }
            }
            "transition" => {
                if let Some((duration, delay, timing)) = parse_transition_shorthand(value) {
                    if let Some(duration) = duration {
                        style = style.transition_duration(duration);
                    }
                    if let Some(delay) = delay {
                        style = style.transition_delay(delay);
                    }
                    if let Some(timing) = timing {
                        style = style.transition_timing(timing);
                    }
                }
            }
            "transition-duration" => {
                if let Some(duration) = parse_duration(value) {
                    style = style.transition_duration(duration);
                }
            }
            "transition-delay" => {
                if let Some(delay) = parse_duration(value) {
                    style = style.transition_delay(delay);
                }
            }
            "transition-timing-function" => {
                if let Some(timing) = parse_transition_timing(value) {
                    style = style.transition_timing(timing);
                }
            }
            "border-top" => {
                if let Some(edge) = parse_border_edge(value) {
                    style.border_top = edge;
                }
            }
            "border-right" => {
                if let Some(edge) = parse_border_edge(value) {
                    style.border_right = edge;
                }
            }
            "border-bottom" => {
                if let Some(edge) = parse_border_edge(value) {
                    style.border_bottom = edge;
                }
            }
            "border-left" => {
                if let Some(edge) = parse_border_edge(value) {
                    style.border_left = edge;
                }
            }
            "border" => {
                if let Some(edges) = parse_border_shorthand(value) {
                    style.border_top = edges.0;
                    style.border_right = edges.1;
                    style.border_bottom = edges.2;
                    style.border_left = edges.3;
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

fn parse_margin(value: &str) -> Option<Margin> {
    let parts: Vec<&str> = value
        .split_whitespace()
        .filter(|part| !part.is_empty())
        .collect();
    let nums: Vec<usize> = parts.iter().filter_map(|part| part.parse().ok()).collect();
    match nums.len() {
        1 => Some(Margin::all(nums[0])),
        2 => Some(Margin::vertical_horizontal(nums[0], nums[1])),
        3 => Some(Margin::new(nums[0], nums[1], nums[2], nums[1])),
        4 => Some(Margin::new(nums[0], nums[1], nums[2], nums[3])),
        _ => None,
    }
}

fn parse_border_edge(value: &str) -> Option<BorderEdge> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") {
        return Some(BorderEdge::None);
    }
    let mut tokens = value.split_whitespace().filter(|t| !t.is_empty());
    let first = tokens.next()?;
    let (border_type, rest_tokens): (BorderType, Vec<&str>) = match first.to_lowercase().as_str() {
        "tall" => (BorderType::Tall, tokens.collect()),
        "block" => (BorderType::Block, tokens.collect()),
        "solid" => (BorderType::Solid, tokens.collect()),
        // If the first token isn't a border type, treat it as a color token and default
        // to `solid`.
        _ => (
            BorderType::Solid,
            std::iter::once(first).chain(tokens).collect(),
        ),
    };
    let mut color: Option<crate::style::Color> = None;
    let mut alpha_percent: Option<u8> = None;
    for token in rest_tokens {
        if let Some(raw) = token.strip_suffix('%') {
            if let Ok(v) = raw.parse::<u8>() {
                alpha_percent = Some(v.min(100));
                continue;
            }
        }
        if let Some(c) = parse_color_like(token) {
            color = Some(c);
        }
    }
    let mut color = color?;
    if let Some(p) = alpha_percent {
        color = color.with_alpha(p as f32 / 100.0);
    }
    Some(BorderEdge::Edge { border_type, color })
}

fn parse_border_shorthand(value: &str) -> Option<(BorderEdge, BorderEdge, BorderEdge, BorderEdge)> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("none") {
        return Some((
            BorderEdge::None,
            BorderEdge::None,
            BorderEdge::None,
            BorderEdge::None,
        ));
    }
    let mut tokens = value.split_whitespace().filter(|t| !t.is_empty());
    let kind = tokens.next()?.to_lowercase();
    let border_type = match kind.as_str() {
        "block" => BorderType::Block,
        "solid" => BorderType::Solid,
        "tall" => BorderType::Tall,
        _ => return None,
    };
    let mut color: Option<crate::style::Color> = None;
    let mut alpha_percent: Option<u8> = None;
    for token in tokens {
        if let Some(raw) = token.strip_suffix('%') {
            if let Ok(v) = raw.parse::<u8>() {
                alpha_percent = Some(v.min(100));
                continue;
            }
        }
        if let Some(c) = parse_color_like(token) {
            color = Some(c);
        }
    }
    let mut color = color?;
    if let Some(p) = alpha_percent {
        color = color.with_alpha(p as f32 / 100.0);
    }
    let edge = BorderEdge::Edge { border_type, color };
    Some((edge, edge, edge, edge))
}

fn parse_tint(value: &str) -> Option<Tint> {
    // Format: "<color> <percent>%" (percent is optional, defaults to 0).
    let mut color: Option<crate::style::Color> = None;
    let mut percent: Option<u8> = None;
    for token in value.split_whitespace().filter(|t| !t.is_empty()) {
        if let Some(raw) = token.strip_suffix('%') {
            if let Ok(v) = raw.parse::<u8>() {
                percent = Some(v);
                continue;
            }
        }
        if let Some(c) = parse_color_like(token) {
            color = Some(c);
        }
    }
    Some(Tint::new(color?, percent.unwrap_or(0)))
}

fn parse_transition_shorthand(
    value: &str,
) -> Option<(Option<Duration>, Option<Duration>, Option<TransitionTiming>)> {
    // Parse only the first transition item in a comma-separated declaration.
    // Example: "offset 300ms ease-in-out 50ms".
    let first_item = value.split(',').next()?.trim();
    if first_item.is_empty() {
        return None;
    }
    let mut duration: Option<Duration> = None;
    let mut delay: Option<Duration> = None;
    let mut timing: Option<TransitionTiming> = None;

    for token in first_item.split_whitespace() {
        if duration.is_none() {
            if let Some(parsed) = parse_duration(token) {
                duration = Some(parsed);
                continue;
            }
        } else if delay.is_none() {
            if let Some(parsed) = parse_duration(token) {
                delay = Some(parsed);
                continue;
            }
        }
        if timing.is_none() {
            timing = parse_transition_timing(token);
        }
    }

    Some((duration, delay, timing))
}

fn parse_duration(value: &str) -> Option<Duration> {
    let token = value.trim().to_lowercase();
    if token.is_empty() {
        return None;
    }
    if let Some(raw) = token.strip_suffix("ms") {
        let ms: f64 = raw.trim().parse().ok()?;
        if ms.is_sign_negative() {
            return None;
        }
        return Some(Duration::from_secs_f64(ms / 1000.0));
    }
    if let Some(raw) = token.strip_suffix('s') {
        let secs: f64 = raw.trim().parse().ok()?;
        if secs.is_sign_negative() {
            return None;
        }
        return Some(Duration::from_secs_f64(secs));
    }
    None
}

fn parse_transition_timing(value: &str) -> Option<TransitionTiming> {
    match value.trim().to_lowercase().as_str() {
        "linear" => Some(TransitionTiming::Linear),
        "ease" | "ease-in-out" => Some(TransitionTiming::InOutCubic),
        "ease-out" => Some(TransitionTiming::OutCubic),
        "none" => Some(TransitionTiming::None),
        "round" | "step-end" | "steps(1,end)" => Some(TransitionTiming::Round),
        "in-out-cubic" | "in_out_cubic" => Some(TransitionTiming::InOutCubic),
        "out-cubic" | "out_cubic" => Some(TransitionTiming::OutCubic),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_duration, parse_transition_shorthand, parse_transition_timing};
    use crate::style::TransitionTiming;
    use std::time::Duration;

    #[test]
    fn parses_transition_shorthand_duration_delay_and_timing() {
        let parsed = parse_transition_shorthand("offset 300ms ease-in-out 75ms")
            .expect("transition shorthand should parse");
        assert_eq!(parsed.0, Some(Duration::from_millis(300)));
        assert_eq!(parsed.1, Some(Duration::from_millis(75)));
        assert_eq!(parsed.2, Some(TransitionTiming::InOutCubic));
    }

    #[test]
    fn parses_duration_units() {
        assert_eq!(parse_duration("250ms"), Some(Duration::from_millis(250)));
        assert_eq!(parse_duration("0.5s"), Some(Duration::from_millis(500)));
        assert_eq!(parse_duration("bogus"), None);
    }

    #[test]
    fn parses_transition_timing_aliases() {
        assert_eq!(
            parse_transition_timing("ease-out"),
            Some(TransitionTiming::OutCubic)
        );
        assert_eq!(
            parse_transition_timing("steps(1,end)"),
            Some(TransitionTiming::Round)
        );
        assert_eq!(parse_transition_timing("unknown"), None);
    }
}
