//! Render-time style queries for custom widgets (public seam).
//!
//! A custom widget's [`Widget::render`](crate::widgets::Widget::render) runs
//! while the framework holds the widget's resolved style and its ancestor
//! surface on an internal render stack. This module exposes that state as a
//! small, documented read-only API so a `render()` body can ask:
//!
//! - [`resolved_style`]: "what is my own resolved [`Style`] right now?"
//!   (stylesheet + inline styles + inheritance, exactly what the framework
//!   paints with),
//! - [`composited_background`]: "what background surface am I actually
//!   composited over?" (the nearest painted ancestor surface),
//! - [`theme_color`]: "what does a theme token like `$accent` resolve to
//!   against the active theme?".
//!
//! # CSS semantics vs render-time composition
//!
//! CSS `background` is **not** an inherited property: a widget with no
//! explicit `background` has `resolved_style().bg == None`. Visually,
//! however, such a transparent widget is composited over its ancestors'
//! painted surfaces at render time. [`composited_background`] returns that
//! effective surface color, flattening every ancestor `background` (including
//! `background-tint`) top-down, so custom render code can blend against the
//! same surface the framework uses.
//!
//! # Scope
//!
//! These functions read render-scoped state: they return `Some` only while
//! the framework is rendering a widget (inside `render()` / `render_line()` /
//! component renderables invoked from them). Called anywhere else (event
//! handlers, `on_mount`, timers, tests without a render pass) they return
//! `None`. [`theme_color`] is the exception: it resolves against the active
//! theme and works in any context.
//!
//! # Example
//!
//! ```ignore
//! use textual::render_context;
//!
//! fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
//!     let style = render_context::resolved_style().unwrap_or_default();
//!     let surface = render_context::composited_background();
//!     let accent = render_context::theme_color("$accent");
//!     // ... paint using the same colors the framework resolved ...
//! }
//! ```
//!
//! This is the supported public surface for render-time style access; the
//! component-classes work builds on it. Do not reach for framework internals.

use crate::style::{Color, Style};

/// The resolved [`Style`] of the widget currently being rendered.
///
/// This is the widget's own computed style: stylesheet rules matched against
/// the widget's type/id/classes/pseudo-classes, combined with inline styles,
/// with inherited properties (like `color`) already flowed in from ancestors.
/// It is exactly the style the framework paints this widget with.
///
/// Note that CSS `background` is not inherited: `resolved_style().bg` is
/// `None` unless this widget (or a rule matching it) sets a background. Use
/// [`composited_background`] for the effective surface underneath.
///
/// Returns `None` outside of a render call.
pub fn resolved_style() -> Option<Style> {
    crate::css::current_self_style()
}

/// The effective composited background surface of the widget currently being
/// rendered.
///
/// Flattens every painted ancestor `background` (top-down, honoring alpha and
/// `background-tint`), including this widget's own background if it has one.
/// This is the surface color a transparent widget visually sits on, and the
/// color the framework blends `color: auto`, opacity, and transparent
/// children against.
///
/// Returns `None` outside of a render call, or when no ancestor in the
/// current render stack paints any background (terminal-default surface).
pub fn composited_background() -> Option<Color> {
    crate::css::current_composited_background()
}

/// Resolve a theme color token (e.g. `$accent`, `$primary`, `$surface`)
/// against the active theme.
///
/// Accepts the token with or without the leading `$` (`"$accent"` and
/// `"accent"` are equivalent) and supports the shade-variant syntax
/// (`$primary-darken-1`, `$accent-lighten-2`). Resolution order matches CSS
/// parsing: the active named theme's token map first, then the built-in
/// textual-dark defaults.
///
/// Unlike [`resolved_style`] / [`composited_background`], this works in any
/// context (not just during render).
///
/// Returns `None` for an unknown token.
pub fn theme_color(token: &str) -> Option<Color> {
    let token = token.trim();
    let name = token.strip_prefix('$').unwrap_or(token);
    if name.is_empty() {
        return None;
    }
    crate::style::parse_color_like(&format!("${name}"))
}
