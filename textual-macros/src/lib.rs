//! Proc macros for the textual TUI framework.
//!
//! Provides `#[derive(Reactive)]` for reactive field system.

extern crate proc_macro;
use proc_macro::TokenStream;

mod reactive;

/// Derive macro for the reactive field system.
///
/// Annotate struct fields with `#[reactive]`, `#[reactive(layout)]`,
/// `#[reactive(watch)]`, or `#[var]` to generate getters, setters with
/// change detection, and watcher dispatch.
#[proc_macro_derive(Reactive, attributes(reactive, var, computed))]
pub fn derive_reactive(input: TokenStream) -> TokenStream {
    reactive::derive_reactive_impl(input.into()).into()
}

mod widget;

/// Attribute macro that generates a full `impl Widget` (and `impl Renderable`)
/// forwarding the structural / propagation method surface to a `base` field,
/// so a compound widget can "inherit" from a container without hand-forwarding
/// all ~63 delegated `Widget` methods.
///
/// This is the first-class replacement for the deprecated declarative
/// `delegate_widget_to!` / `delegate_widget_method!` macros.
///
/// # Usage
///
/// ```ignore
/// #[widget(base = VerticalGroup)]
/// #[derive(Reactive)]
/// struct StatCard {
///     base: VerticalGroup,
///     #[reactive] count: i32,
/// }
/// ```
///
/// # Options
///
/// - `base = <Type>` (required) — the container type being "inherited".
/// - `field = <ident>` — delegate to a differently-named field (default `base`).
/// - `style_type = "Name"` — emit a custom CSS type; otherwise the widget's own
///   concrete type name is used (NOT the base's).
/// - `reactive` — route `reactive_widget` to `Some(self)` (opt-in for
///   `#[derive(Reactive)]` compound widgets).
/// - `override(m1, m2, ..)` — do not forward these methods; call the user's
///   inherent method of the same name/signature instead.
#[proc_macro_attribute]
pub fn widget(attr: TokenStream, item: TokenStream) -> TokenStream {
    widget::widget_impl(attr.into(), item.into()).into()
}

mod on_handler;

/// Attribute macro for typed message handler dispatch.
///
/// Apply to a method to generate a companion dispatch method that
/// pattern-matches against the `Message` enum and calls the handler
/// with the typed event payload.
///
/// # Usage
///
/// ```ignore
/// #[on(ButtonPressed)]
/// fn handle_button(&mut self, event: &ButtonPressed, ctx: &mut EventCtx) { ... }
///
/// #[on(ButtonPressed, selector = "#save")]
/// fn handle_save(&mut self, event: &ButtonPressed, ctx: &mut EventCtx) { ... }
/// ```
#[proc_macro_attribute]
pub fn on(attr: TokenStream, item: TokenStream) -> TokenStream {
    on_handler::on_handler_impl(attr.into(), item.into()).into()
}
