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
#[proc_macro_derive(Reactive, attributes(reactive, var))]
pub fn derive_reactive(input: TokenStream) -> TokenStream {
    reactive::derive_reactive_impl(input.into()).into()
}
