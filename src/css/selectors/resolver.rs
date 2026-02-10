use crate::style::Style;
use crate::widgets::Widget;

use super::ast::{SelectorMeta, SelectorStates, StyleSheet};
use super::context::{SELECTOR_STACK, STYLE_CONTEXT, STYLE_STACK, app_is_active};
use super::debug::{style_debug_matches, style_debug_meta_label, style_debug_summary};
use super::matching::rule_specificity;

impl StyleSheet {
    pub(super) fn style_for<T: Widget + ?Sized>(&self, _widget: &T, meta: &SelectorMeta) -> Style {
        self.style_for_meta(meta)
    }

    pub(super) fn style_for_meta(&self, meta: &SelectorMeta) -> Style {
        let mut matches: Vec<(u8, usize, Style)> = Vec::new();
        let debug_style_meta = style_debug_matches(meta);
        for (idx, rule) in self.rules.iter().enumerate() {
            if let Some(score) = rule_specificity(rule, meta) {
                matches.push((score, idx, rule.style));
                if debug_style_meta {
                    crate::debug::debug_style(&format!(
                        "[style] match widget={} selector=\"{}\" score={} rule={}",
                        style_debug_meta_label(meta),
                        super::debug::selector_chain_string(&rule.selector_chain),
                        score,
                        style_debug_summary(&rule.style),
                    ));
                }
            }
        }
        matches.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        let mut out = Style::new();
        for (_, _, style) in matches {
            out = out.combine(&style);
        }
        if debug_style_meta {
            let stack = SELECTOR_STACK.with(|stack| {
                stack
                    .borrow()
                    .iter()
                    .map(style_debug_meta_label)
                    .collect::<Vec<_>>()
            });
            crate::debug::debug_style(&format!(
                "[style] resolved widget={} stack={:?} style={}",
                style_debug_meta_label(meta),
                stack,
                style_debug_summary(&out),
            ));
        }
        out
    }
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
