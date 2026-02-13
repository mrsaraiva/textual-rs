//! Implementation of the `#[on(MessageType)]` and `#[on(MessageType, selector = "...")]`
//! attribute macro for typed message handler dispatch.
//!
//! Transforms an annotated method into itself plus a companion dispatch method
//! that pattern-matches against the `Message` enum and calls the original handler
//! with the typed event payload.
//!
//! **Selector matching** is deferred to runtime wiring. When `selector = "..."` is
//! specified, the macro generates a companion `const __ON_SELECTOR_<NAME>: &str`
//! that the runtime can use for CSS selector matching. The generated dispatch
//! method itself only gates on message type; selector filtering happens at the
//! call site in the runtime.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Expr, ExprLit, Ident, ItemFn, Lit, Meta, Token,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

/// Parsed arguments from `#[on(MessageType)]` or `#[on(MessageType, selector = "...")]`.
#[derive(Debug)]
struct OnArgs {
    /// The message type identifier (e.g. `ButtonPressed`).
    message_type: Ident,
    /// Optional CSS selector string (e.g. `"#save"`).
    selector: Option<String>,
}

impl Parse for OnArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let message_type: Ident = input.parse()?;

        let selector = if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;

            // Parse `selector = "..."`
            let metas = Punctuated::<Meta, Token![,]>::parse_terminated(input)?;

            let mut sel: Option<String> = None;
            for meta in &metas {
                match meta {
                    Meta::NameValue(nv) if nv.path.is_ident("selector") => {
                        if sel.is_some() {
                            return Err(syn::Error::new_spanned(
                                nv,
                                "duplicate `selector` argument",
                            ));
                        }
                        if let Expr::Lit(ExprLit {
                            lit: Lit::Str(lit_str),
                            ..
                        }) = &nv.value
                        {
                            sel = Some(lit_str.value());
                        } else {
                            return Err(syn::Error::new_spanned(
                                &nv.value,
                                "expected a string literal for `selector`",
                            ));
                        }
                    }
                    other => {
                        return Err(syn::Error::new_spanned(
                            other,
                            "unknown argument; expected `selector = \"...\"`",
                        ));
                    }
                }
            }

            sel
        } else {
            None
        };

        Ok(OnArgs {
            message_type,
            selector,
        })
    }
}

/// Entry point called from the proc-macro crate's `lib.rs`.
///
/// `attr` is the token stream inside the parentheses: `ButtonPressed` or
/// `ButtonPressed, selector = "#save"`.
///
/// `item` is the annotated function/method.
pub fn on_handler_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args: OnArgs = match syn::parse2(attr) {
        Ok(a) => a,
        Err(e) => return e.to_compile_error(),
    };

    let func: ItemFn = match syn::parse2(item) {
        Ok(f) => f,
        Err(e) => return e.to_compile_error(),
    };

    let fn_name = &func.sig.ident;
    let dispatch_name = format_ident!("__on_dispatch_{}", fn_name);
    let msg_variant = &args.message_type;

    let call_expr = quote! {
        self.#fn_name(event, ctx);
    };

    // All dispatch methods share a uniform signature:
    //   fn __on_dispatch_<name>(&mut self, msg: &Message, sender: NodeId, ctx: &mut EventCtx) -> bool
    // This allows the runtime to call any generated dispatcher through a single
    // interface regardless of whether a selector was specified.
    let selector_items = if let Some(ref selector_str) = args.selector {
        let selector_const = format_ident!(
            "__ON_SELECTOR_{}",
            fn_name.to_string().to_uppercase()
        );
        quote! {
            #[doc(hidden)]
            #[allow(non_upper_case_globals)]
            const #selector_const: &str = #selector_str;
        }
    } else {
        quote! {}
    };

    let expanded = quote! {
        #func

        #selector_items

        #[doc(hidden)]
        #[allow(non_snake_case)]
        fn #dispatch_name(
            &mut self,
            msg: &textual::message::Message,
            _sender: textual::node_id::NodeId,
            ctx: &mut textual::event::EventCtx,
        ) -> bool {
            if let textual::message::Message::#msg_variant(ref event) = *msg {
                #call_expr
                return true;
            }
            false
        }
    };

    expanded
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn parse_type_only() {
        let attr: TokenStream = quote! { ButtonPressed };
        let args: OnArgs = syn::parse2(attr).unwrap();
        assert_eq!(args.message_type, "ButtonPressed");
        assert!(args.selector.is_none());
    }

    #[test]
    fn parse_type_with_selector() {
        let attr: TokenStream = quote! { ButtonPressed, selector = "#save" };
        let args: OnArgs = syn::parse2(attr).unwrap();
        assert_eq!(args.message_type, "ButtonPressed");
        assert_eq!(args.selector.as_deref(), Some("#save"));
    }

    #[test]
    fn parse_unknown_arg_errors() {
        let attr: TokenStream = quote! { ButtonPressed, unknown = "foo" };
        let result: Result<OnArgs, _> = syn::parse2(attr);
        assert!(result.is_err());
    }

    #[test]
    fn parse_duplicate_selector_errors() {
        let attr: TokenStream = quote! { ButtonPressed, selector = "#a", selector = "#b" };
        let result: Result<OnArgs, _> = syn::parse2(attr);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("duplicate"),
            "expected 'duplicate' in error: {err_msg}"
        );
    }

    #[test]
    fn generates_dispatch_no_selector() {
        let attr = quote! { ButtonPressed };
        let item = quote! {
            fn handle_button(&mut self, event: &ButtonPressed, ctx: &mut EventCtx) {
                // handler body
            }
        };
        let output = on_handler_impl(attr, item);
        let output_str = output.to_string();
        assert!(output_str.contains("__on_dispatch_handle_button"));
        assert!(output_str.contains("ButtonPressed"));
        assert!(!output_str.contains("__ON_SELECTOR"));
        // Uniform signature: sender param always present.
        assert!(output_str.contains("_sender"));
    }

    #[test]
    fn generates_dispatch_with_selector() {
        let attr = quote! { ButtonPressed, selector = "#save" };
        let item = quote! {
            fn handle_save(&mut self, event: &ButtonPressed, ctx: &mut EventCtx) {
                // handler body
            }
        };
        let output = on_handler_impl(attr, item);
        let output_str = output.to_string();
        assert!(output_str.contains("__on_dispatch_handle_save"));
        assert!(output_str.contains("__ON_SELECTOR_HANDLE_SAVE"));
        assert!(output_str.contains("\"#save\""));
        // Uniform signature: sender param always present.
        assert!(output_str.contains("_sender"));
    }
}
