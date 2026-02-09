use std::cell::RefCell;

use crate::style::Style;

use super::ast::{SelectorMeta, StyleSheet};

thread_local! {
    pub(super) static STYLE_CONTEXT: RefCell<Option<StyleSheet>> = RefCell::new(None);
    pub(super) static STYLE_STACK: RefCell<Vec<Style>> = RefCell::new(Vec::new());
    pub(super) static SELECTOR_STACK: RefCell<Vec<SelectorMeta>> = RefCell::new(Vec::new());
    pub(super) static APP_ACTIVE: RefCell<bool> = RefCell::new(true);
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

pub(super) fn app_is_active() -> bool {
    APP_ACTIVE.with(|v| *v.borrow())
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
