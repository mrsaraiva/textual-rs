//! Implementation of `#[derive(Reactive)]` proc macro.
//!
//! Generates getters, setters (with change detection), and watcher dispatch
//! for fields annotated with `#[reactive]`, `#[reactive(layout)]`,
//! `#[reactive(watch)]`, or `#[var]`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, Meta, parse2};

/// Parsed annotation on a single field.
#[derive(Debug, Clone)]
struct ReactiveField {
    /// The field identifier.
    ident: syn::Ident,
    /// The field type.
    ty: syn::Type,
    /// Whether `layout` was specified.
    layout: bool,
    /// Whether `watch` was specified (opt-in watcher dispatch).
    watch: bool,
    /// Whether this is a `#[var]` field (no repaint, no layout, no init).
    is_var: bool,
}

/// Parse reactive/var attributes from a field's attributes.
///
/// Returns `Ok(Some(...))` for annotated fields, `Ok(None)` for unannotated,
/// or `Err(...)` for malformed attributes (unknown args, parse errors).
fn parse_reactive_field(field: &syn::Field) -> Result<Option<ReactiveField>, syn::Error> {
    let ident = match field.ident.as_ref() {
        Some(id) => id.clone(),
        None => return Ok(None),
    };
    let ty = field.ty.clone();

    for attr in &field.attrs {
        // Check for #[var]
        if attr.path().is_ident("var") {
            return Ok(Some(ReactiveField {
                ident,
                ty,
                layout: false,
                watch: false,
                is_var: true,
            }));
        }

        // Check for #[reactive] or #[reactive(...)]
        if attr.path().is_ident("reactive") {
            let mut layout = false;
            let mut watch = false;

            // Parse arguments if present: #[reactive(layout, watch)]
            if let Meta::List(meta_list) = &attr.meta {
                let nested = meta_list.parse_args_with(
                    syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
                )?;

                for nested_meta in &nested {
                    if let Meta::Path(path) = nested_meta {
                        if path.is_ident("layout") {
                            layout = true;
                        } else if path.is_ident("watch") {
                            watch = true;
                        } else {
                            return Err(syn::Error::new_spanned(
                                path,
                                format!(
                                    "unknown reactive attribute `{}`; expected `layout` or `watch`",
                                    path.get_ident().map(|i| i.to_string()).unwrap_or_default()
                                ),
                            ));
                        }
                    } else {
                        return Err(syn::Error::new_spanned(
                            nested_meta,
                            "expected a simple identifier (e.g. `layout`, `watch`)",
                        ));
                    }
                }
            }

            return Ok(Some(ReactiveField {
                ident,
                ty,
                layout,
                watch,
                is_var: false,
            }));
        }
    }

    Ok(None)
}

pub fn derive_reactive_impl(input: TokenStream) -> TokenStream {
    let input: DeriveInput = match parse2(input) {
        Ok(input) => input,
        Err(err) => return err.to_compile_error(),
    };

    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return syn::Error::new_spanned(
                    name,
                    "Reactive can only be derived for structs with named fields",
                )
                .to_compile_error();
            }
        },
        _ => {
            return syn::Error::new_spanned(name, "Reactive can only be derived for structs")
                .to_compile_error();
        }
    };

    let mut reactive_fields = Vec::new();
    for field in fields {
        match parse_reactive_field(field) {
            Ok(Some(rf)) => reactive_fields.push(rf),
            Ok(None) => {}
            Err(err) => return err.to_compile_error(),
        }
    }

    if reactive_fields.is_empty() {
        // No reactive fields — still implement the trait with a no-op.
        return quote! {
            impl #impl_generics textual::reactive::ReactiveWidget for #name #ty_generics #where_clause {}
        };
    }

    // Generate getters and setters.
    let mut accessors = Vec::new();
    for field in &reactive_fields {
        let field_ident = &field.ident;
        let field_ty = &field.ty;
        let setter_name = format_ident!("set_{}", field_ident);
        let field_name_str = field_ident.to_string();

        let flags_expr = if field.is_var {
            quote! { textual::reactive::ReactiveFlags::var() }
        } else if field.layout {
            quote! { textual::reactive::ReactiveFlags::reactive_layout() }
        } else {
            quote! { textual::reactive::ReactiveFlags::reactive() }
        };

        accessors.push(quote! {
            /// Generated getter for reactive field.
            pub fn #field_ident(&self) -> &#field_ty {
                &self.#field_ident
            }

            /// Generated setter for reactive field. Records the change in
            /// the provided [`ReactiveCtx`] if the value actually changed.
            pub fn #setter_name(&mut self, value: #field_ty, ctx: &mut textual::reactive::ReactiveCtx)
            where
                #field_ty: PartialEq + Clone + Send + 'static,
            {
                if self.#field_ident != value {
                    let old = self.#field_ident.clone();
                    self.#field_ident = value;
                    let new = self.#field_ident.clone();
                    ctx.record_change(
                        #field_name_str,
                        #flags_expr,
                        Box::new(old),
                        Box::new(new),
                    );
                }
            }
        });
    }

    // Generate reactive_dispatch — only for fields with `watch` opt-in.
    let watch_fields: Vec<&ReactiveField> = reactive_fields.iter().filter(|f| f.watch).collect();

    let dispatch_body = if watch_fields.is_empty() {
        quote! {
            let _ = (changes, ctx);
        }
    } else {
        let match_arms: Vec<TokenStream> = watch_fields
            .iter()
            .map(|field| {
                let field_name_str = field.ident.to_string();
                let field_ty = &field.ty;
                let watcher_name = format_ident!("watch_{}", field.ident);

                quote! {
                    #field_name_str => {
                        if let (Some(old), Some(new)) = (
                            change.old_value.downcast_ref::<#field_ty>(),
                            change.new_value.downcast_ref::<#field_ty>(),
                        ) {
                            self.#watcher_name(old, new, ctx);
                        }
                    }
                }
            })
            .collect();

        quote! {
            for change in changes {
                match change.field_name {
                    #(#match_arms)*
                    _ => {}
                }
            }
        }
    };

    let expanded = quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            #(#accessors)*
        }

        impl #impl_generics textual::reactive::ReactiveWidget for #name #ty_generics #where_clause {
            fn reactive_dispatch(
                &mut self,
                changes: &[textual::reactive::ReactiveChange],
                ctx: &mut textual::reactive::ReactiveCtx,
            ) {
                #dispatch_body
            }
        }
    };

    expanded
}
