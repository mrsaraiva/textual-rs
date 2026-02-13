//! Implementation of `#[derive(Reactive)]` proc macro.
//!
//! Generates getters, setters (with change detection), watcher dispatch,
//! and computed field caching for fields annotated with `#[reactive]`,
//! `#[reactive(layout)]`, `#[reactive(watch)]`, `#[reactive(init = false)]`,
//! `#[var]`, or `#[computed(depends_on = "field1, field2")]`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Expr, Fields, Lit, Meta, parse2};

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
    /// Whether `init = false` was specified (suppress watcher on mount).
    init_false: bool,
}

/// Parsed `#[computed(depends_on = "field1, field2")]` annotation.
#[derive(Debug, Clone)]
struct ComputedField {
    /// The field identifier.
    ident: syn::Ident,
    /// The field type.
    ty: syn::Type,
    /// Names of reactive fields this computed field depends on.
    depends_on: Vec<String>,
}

/// Parse reactive/var/computed attributes from a field's attributes.
///
/// Returns `Ok(Some(FieldAnnotation))` for annotated fields, `Ok(None)` for
/// unannotated, or `Err(...)` for malformed attributes.
enum FieldAnnotation {
    Reactive(ReactiveField),
    Computed(ComputedField),
}

fn parse_field_annotation(field: &syn::Field) -> Result<Option<FieldAnnotation>, syn::Error> {
    let ident = match field.ident.as_ref() {
        Some(id) => id.clone(),
        None => return Ok(None),
    };
    let ty = field.ty.clone();

    for attr in &field.attrs {
        // Check for #[var]
        if attr.path().is_ident("var") {
            return Ok(Some(FieldAnnotation::Reactive(ReactiveField {
                ident,
                ty,
                layout: false,
                watch: false,
                is_var: true,
                init_false: false,
            })));
        }

        // Check for #[computed(depends_on = "field1, field2")]
        if attr.path().is_ident("computed") {
            let mut depends_on = Vec::new();

            if let Meta::List(meta_list) = &attr.meta {
                let nested = meta_list.parse_args_with(
                    syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
                )?;

                for nested_meta in &nested {
                    if let Meta::NameValue(nv) = nested_meta {
                        if nv.path.is_ident("depends_on") {
                            if let Expr::Lit(expr_lit) = &nv.value {
                                if let Lit::Str(lit_str) = &expr_lit.lit {
                                    depends_on = lit_str
                                        .value()
                                        .split(',')
                                        .map(|s| s.trim().to_string())
                                        .filter(|s| !s.is_empty())
                                        .collect();
                                } else {
                                    return Err(syn::Error::new_spanned(
                                        &nv.value,
                                        "expected string literal for `depends_on`",
                                    ));
                                }
                            } else {
                                return Err(syn::Error::new_spanned(
                                    &nv.value,
                                    "expected string literal for `depends_on`",
                                ));
                            }
                        } else {
                            return Err(syn::Error::new_spanned(
                                &nv.path,
                                format!(
                                    "unknown computed attribute `{}`; expected `depends_on`",
                                    nv.path
                                        .get_ident()
                                        .map(|i| i.to_string())
                                        .unwrap_or_default()
                                ),
                            ));
                        }
                    } else {
                        return Err(syn::Error::new_spanned(
                            nested_meta,
                            "expected `depends_on = \"field1, field2\"`",
                        ));
                    }
                }
            } else {
                return Err(syn::Error::new_spanned(
                    attr,
                    "computed requires `depends_on` argument: #[computed(depends_on = \"field1, field2\")]",
                ));
            }

            if depends_on.is_empty() {
                return Err(syn::Error::new_spanned(
                    attr,
                    "computed requires at least one dependency in `depends_on`",
                ));
            }

            return Ok(Some(FieldAnnotation::Computed(ComputedField {
                ident,
                ty,
                depends_on,
            })));
        }

        // Check for #[reactive] or #[reactive(...)]
        if attr.path().is_ident("reactive") {
            let mut layout = false;
            let mut watch = false;
            let mut init_false = false;

            // Parse arguments if present: #[reactive(layout, watch, init = false)]
            if let Meta::List(meta_list) = &attr.meta {
                let nested = meta_list.parse_args_with(
                    syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
                )?;

                for nested_meta in &nested {
                    match nested_meta {
                        Meta::Path(path) => {
                            if path.is_ident("layout") {
                                layout = true;
                            } else if path.is_ident("watch") {
                                watch = true;
                            } else {
                                return Err(syn::Error::new_spanned(
                                    path,
                                    format!(
                                        "unknown reactive attribute `{}`; expected `layout`, `watch`, or `init = false`",
                                        path.get_ident().map(|i| i.to_string()).unwrap_or_default()
                                    ),
                                ));
                            }
                        }
                        Meta::NameValue(nv) => {
                            if nv.path.is_ident("init") {
                                // Parse `init = false`
                                if let Expr::Lit(expr_lit) = &nv.value {
                                    if let Lit::Bool(lit_bool) = &expr_lit.lit {
                                        if !lit_bool.value {
                                            init_false = true;
                                        }
                                        // init = true is the default, so just ignore it
                                    } else {
                                        return Err(syn::Error::new_spanned(
                                            &nv.value,
                                            "expected boolean literal for `init`",
                                        ));
                                    }
                                } else {
                                    return Err(syn::Error::new_spanned(
                                        &nv.value,
                                        "expected boolean literal for `init`",
                                    ));
                                }
                            } else {
                                return Err(syn::Error::new_spanned(
                                    &nv.path,
                                    format!(
                                        "unknown reactive attribute `{}`; expected `layout`, `watch`, or `init`",
                                        nv.path.get_ident().map(|i| i.to_string()).unwrap_or_default()
                                    ),
                                ));
                            }
                        }
                        _ => {
                            return Err(syn::Error::new_spanned(
                                nested_meta,
                                "expected a simple identifier (e.g. `layout`, `watch`) or `init = false`",
                            ));
                        }
                    }
                }
            }

            return Ok(Some(FieldAnnotation::Reactive(ReactiveField {
                ident,
                ty,
                layout,
                watch,
                is_var: false,
                init_false,
            })));
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
    let mut computed_fields = Vec::new();
    for field in fields {
        match parse_field_annotation(field) {
            Ok(Some(FieldAnnotation::Reactive(rf))) => reactive_fields.push(rf),
            Ok(Some(FieldAnnotation::Computed(cf))) => computed_fields.push(cf),
            Ok(None) => {}
            Err(err) => return err.to_compile_error(),
        }
    }

    if reactive_fields.is_empty() && computed_fields.is_empty() {
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
        } else if field.layout && field.init_false {
            quote! { textual::reactive::ReactiveFlags::reactive_layout_no_init() }
        } else if field.layout {
            quote! { textual::reactive::ReactiveFlags::reactive_layout() }
        } else if field.init_false {
            quote! { textual::reactive::ReactiveFlags::reactive_no_init() }
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

    // Generate computed field accessors (getter only — recomputation is in reactive_dispatch).
    for cf in &computed_fields {
        let field_ident = &cf.ident;
        let field_ty = &cf.ty;

        accessors.push(quote! {
            /// Generated getter for computed field. Returns the cached value.
            pub fn #field_ident(&self) -> &#field_ty {
                &self.#field_ident
            }
        });
    }

    // Generate reactive_dispatch — watches + computed recomputation.
    let watch_fields: Vec<&ReactiveField> = reactive_fields.iter().filter(|f| f.watch).collect();

    // Build computed recomputation arms: for each changed dependency field,
    // recompute dependent computed fields.
    let mut computed_recompute_stmts = Vec::new();
    for cf in &computed_fields {
        let field_ident = &cf.ident;
        let _field_ty = &cf.ty;
        let compute_fn = format_ident!("compute_{}", field_ident);
        let dep_strs: Vec<&str> = cf.depends_on.iter().map(|s| s.as_str()).collect();
        let field_name_str = field_ident.to_string();

        computed_recompute_stmts.push(quote! {
            // Check if any dependency of computed field changed.
            {
                let dep_names: &[&str] = &[#(#dep_strs),*];
                let dep_changed = changes.iter().any(|c| dep_names.contains(&c.field_name));
                if dep_changed {
                    let new_val = self.#compute_fn();
                    if self.#field_ident != new_val {
                        let old_val = self.#field_ident.clone();
                        self.#field_ident = new_val.clone();
                        ctx.record_change(
                            #field_name_str,
                            textual::reactive::ReactiveFlags::reactive(),
                            Box::new(old_val) as Box<dyn std::any::Any + Send>,
                            Box::new(new_val) as Box<dyn std::any::Any + Send>,
                        );
                    }
                }
            }
        });
    }

    let has_watchers = !watch_fields.is_empty();
    let has_computed = !computed_fields.is_empty();

    let dispatch_body = if !has_watchers && !has_computed {
        quote! {
            let _ = (changes, ctx);
        }
    } else {
        let watcher_block = if has_watchers {
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
        } else {
            quote! {}
        };

        let computed_block = if has_computed {
            quote! {
                #(#computed_recompute_stmts)*
            }
        } else {
            quote! {}
        };

        quote! {
            #watcher_block
            #computed_block
        }
    };

    // Generate the list of reactive field descriptors for `reactive_field_descriptors()`.
    let descriptor_entries: Vec<TokenStream> = reactive_fields
        .iter()
        .map(|field| {
            let field_name_str = field.ident.to_string();
            let flags_expr = if field.is_var {
                quote! { textual::reactive::ReactiveFlags::var() }
            } else if field.layout && field.init_false {
                quote! { textual::reactive::ReactiveFlags::reactive_layout_no_init() }
            } else if field.layout {
                quote! { textual::reactive::ReactiveFlags::reactive_layout() }
            } else if field.init_false {
                quote! { textual::reactive::ReactiveFlags::reactive_no_init() }
            } else {
                quote! { textual::reactive::ReactiveFlags::reactive() }
            };

            quote! {
                textual::reactive::ReactiveFieldDescriptor {
                    name: #field_name_str,
                    flags: #flags_expr,
                }
            }
        })
        .collect();

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

            fn reactive_field_descriptors(&self) -> &'static [textual::reactive::ReactiveFieldDescriptor] {
                static DESCRIPTORS: &[textual::reactive::ReactiveFieldDescriptor] = &[
                    #(#descriptor_entries),*
                ];
                DESCRIPTORS
            }
        }
    };

    expanded
}
