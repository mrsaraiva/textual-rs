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
