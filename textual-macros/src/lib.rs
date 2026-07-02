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
/// Apply to a method to generate a companion `__on_dispatch_<name>` dispatch
/// method that pattern-matches the `Message` type and calls the handler with the
/// typed payload and a `&mut WidgetCtx`.
///
/// # Usage
///
/// ```ignore
/// #[on(ButtonPressed)]
/// fn handle_button(&mut self, event: &ButtonPressed, ctx: &mut WidgetCtx) { ... }
///
/// #[on(ButtonPressed, selector = "#save")]  // selector matching is deferred
/// fn handle_save(&mut self, event: &ButtonPressed, ctx: &mut WidgetCtx) { ... }
/// ```
///
/// # Wiring
///
/// The dispatcher is only *called* when you list the handler in the widget's
/// delegation derive: `#[widget(base = .., on(handle_button, ..))]`. If the
/// compiler warns that `__on_dispatch_<name>` is never used, you forgot to add
/// `<name>` to `#[widget(on(..))]` (the dispatcher deliberately carries NO
/// `#[allow(dead_code)]` so that omission is a compile-time warning).
///
/// # Semantics
///
/// `#[on]` does NOT auto-consume the message — like Python Textual, the message
/// keeps bubbling to ancestors after your handler runs. Call `ctx.set_handled()`
/// to stop propagation. Handlers see the message via routing's bubble phase, not
/// via the base-forward.
#[proc_macro_attribute]
pub fn on(attr: TokenStream, item: TokenStream) -> TokenStream {
    on_handler::on_handler_impl(attr.into(), item.into()).into()
}
