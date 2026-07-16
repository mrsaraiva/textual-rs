//! Implementation of `#[derive(Reactive)]` proc macro.
//!
//! Generates getters, setters (with change detection), watcher dispatch,
//! and computed field caching for fields annotated with `#[reactive]`,
//! `#[reactive(layout)]`, `#[reactive(watch)]`, `#[reactive(watch_with_app)]`,
//! `#[reactive(init = false)]`, `#[reactive(always_update)]`, `#[var]`,
//! `#[var(watch)]`, `#[var(watch_with_app)]`, `#[var(init = false)]`,
//! `#[var(always_update)]`, or `#[computed(depends_on = "field1, field2")]`.

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
    /// Whether `watch` was specified (opt-in watcher dispatch without app access).
    watch: bool,
    /// Whether `watch_with_app` was specified (watcher receives `&mut App`).
    watch_with_app: bool,
    /// Whether this is a `#[var]` field (no repaint, no layout).
    is_var: bool,
    /// Whether `init = false` was specified (suppress watcher on mount).
    init_false: bool,
    /// Whether `recompose` was specified (recompose owner subtree on change).
    recompose: bool,
    /// Whether `validate` was specified (call `validate_<field>` before store).
    validate: bool,
    /// Whether `always_update` was specified (Python `always_update=True`):
    /// the setter records the change and fires watchers even when the new
    /// value equals the old one.
    always_update: bool,
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
    /// Whether `watch` was specified — call `watch_<field>(old, new, ctx)` when
    /// the recomputed value changes (no app access).
    watch: bool,
    /// Whether `watch_with_app` was specified — call
    /// `watch_<field>(app, old, new, ctx)` when the recomputed value changes.
    watch_with_app: bool,
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
        // Check for #[var] or #[var(...)]
        if attr.path().is_ident("var") {
            let mut watch = false;
            let mut watch_with_app = false;
            let mut init_false = false;
            let mut validate = false;
            let mut always_update = false;

            // Parse optional args: watch, watch_with_app, validate, always_update, init = false
            if let Meta::List(meta_list) = &attr.meta {
                let nested = meta_list.parse_args_with(
                    syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
                )?;

                for nested_meta in &nested {
                    match nested_meta {
                        Meta::Path(path) => {
                            if path.is_ident("watch") {
                                watch = true;
                            } else if path.is_ident("watch_with_app") {
                                watch_with_app = true;
                            } else if path.is_ident("validate") {
                                validate = true;
                            } else if path.is_ident("always_update") {
                                always_update = true;
                            } else {
                                return Err(syn::Error::new_spanned(
                                    path,
                                    format!(
                                        "unknown var attribute `{}`; expected `watch`, `watch_with_app`, `validate`, `always_update`, or `init = false` (note: `recompose` is only valid on `#[reactive]`, not `#[var]`)",
                                        path.get_ident().map(|i| i.to_string()).unwrap_or_default()
                                    ),
                                ));
                            }
                        }
                        Meta::NameValue(nv) => {
                            if nv.path.is_ident("init") {
                                if let Expr::Lit(expr_lit) = &nv.value {
                                    if let Lit::Bool(lit_bool) = &expr_lit.lit {
                                        if !lit_bool.value {
                                            init_false = true;
                                        }
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
                                        "unknown var attribute `{}`; expected `watch`, `watch_with_app`, or `init`",
                                        nv.path
                                            .get_ident()
                                            .map(|i| i.to_string())
                                            .unwrap_or_default()
                                    ),
                                ));
                            }
                        }
                        _ => {
                            return Err(syn::Error::new_spanned(
                                nested_meta,
                                "expected a simple identifier (e.g. `watch`, `watch_with_app`) or `init = false`",
                            ));
                        }
                    }
                }
            }

            return Ok(Some(FieldAnnotation::Reactive(ReactiveField {
                ident,
                ty,
                layout: false,
                watch,
                watch_with_app,
                is_var: true,
                init_false,
                recompose: false,
                validate,
                always_update,
            })));
        }

        // Check for #[computed(depends_on = "field1, field2"[, watch | watch_with_app])]
        if attr.path().is_ident("computed") {
            let mut depends_on = Vec::new();
            let mut watch = false;
            let mut watch_with_app = false;

            if let Meta::List(meta_list) = &attr.meta {
                let nested = meta_list.parse_args_with(
                    syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
                )?;

                for nested_meta in &nested {
                    match nested_meta {
                        Meta::NameValue(nv) if nv.path.is_ident("depends_on") => {
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
                        }
                        Meta::Path(path) if path.is_ident("watch") => {
                            watch = true;
                        }
                        Meta::Path(path) if path.is_ident("watch_with_app") => {
                            watch_with_app = true;
                        }
                        _ => {
                            return Err(syn::Error::new_spanned(
                                nested_meta,
                                "expected `depends_on = \"field1, field2\"`, `watch`, or `watch_with_app`",
                            ));
                        }
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
                watch,
                watch_with_app,
            })));
        }

        // Check for #[reactive] or #[reactive(...)]
        if attr.path().is_ident("reactive") {
            let mut layout = false;
            let mut watch = false;
            let mut watch_with_app = false;
            let mut init_false = false;
            let mut recompose = false;
            let mut validate = false;
            let mut always_update = false;

            // Parse arguments if present:
            // #[reactive(layout, watch, watch_with_app, recompose, validate, always_update, init = false)]
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
                            } else if path.is_ident("watch_with_app") {
                                watch_with_app = true;
                            } else if path.is_ident("recompose") {
                                recompose = true;
                            } else if path.is_ident("validate") {
                                validate = true;
                            } else if path.is_ident("always_update") {
                                always_update = true;
                            } else {
                                return Err(syn::Error::new_spanned(
                                    path,
                                    format!(
                                        "unknown reactive attribute `{}`; expected `layout`, `watch`, `watch_with_app`, `recompose`, `validate`, `always_update`, or `init = false`",
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
                                        "unknown reactive attribute `{}`; expected `layout`, `watch`, `watch_with_app`, or `init`",
                                        nv.path
                                            .get_ident()
                                            .map(|i| i.to_string())
                                            .unwrap_or_default()
                                    ),
                                ));
                            }
                        }
                        _ => {
                            return Err(syn::Error::new_spanned(
                                nested_meta,
                                "expected a simple identifier (e.g. `layout`, `watch`, `watch_with_app`) or `init = false`",
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
                watch_with_app,
                is_var: false,
                init_false,
                recompose,
                validate,
                always_update,
            })));
        }
    }

    Ok(None)
}

/// Compute the `ReactiveFlags` constructor expression for a field.
///
/// `recompose` takes precedence over `layout` (recompose implies a layout +
/// repaint of the rebuilt subtree, mirroring Python's `refresh(recompose=True)`).
/// `recompose` is rejected on `#[var]` during parsing, so it only applies here
/// to `#[reactive]` fields.
fn flags_expr(field: &ReactiveField) -> TokenStream {
    let base = if field.recompose && field.init_false {
        quote! { textual::reactive::ReactiveFlags::reactive_recompose_no_init() }
    } else if field.recompose {
        quote! { textual::reactive::ReactiveFlags::reactive_recompose() }
    } else if field.is_var && field.init_false {
        quote! { textual::reactive::ReactiveFlags::var_no_init() }
    } else if field.is_var {
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
    if field.always_update {
        quote! { #base.with_always_update() }
    } else {
        base
    }
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
        let mutate_name = format_ident!("mutate_{}", field_ident);
        let field_name_str = field_ident.to_string();
        let f_flags_expr = flags_expr(field);

        // Validation hook (Python `validate_<field>`): when `validate` is set,
        // the incoming value is passed through `self.validate_<field>(value)`
        // before the equality check and store, exactly as Python's `_set` does
        // (reactive.py: public `validate_*` runs before the change is applied).
        let validate_stmt = if field.validate {
            let validate_fn = format_ident!("validate_{}", field_ident);
            quote! {
                let value = self.#validate_fn(value);
            }
        } else {
            quote! {}
        };

        // `always_update` (Python `reactive(..., always_update=True)`) bypasses
        // the equality gate: the change is recorded (and watchers fire) even
        // when the new value equals the old one.
        let set_body = if field.always_update {
            quote! {
                let old = self.#field_ident.clone();
                self.#field_ident = value;
                let new = self.#field_ident.clone();
                ctx.record_change(
                    #field_name_str,
                    #f_flags_expr,
                    Box::new(old),
                    Box::new(new),
                );
            }
        } else {
            quote! {
                if self.#field_ident != value {
                    let old = self.#field_ident.clone();
                    self.#field_ident = value;
                    let new = self.#field_ident.clone();
                    ctx.record_change(
                        #field_name_str,
                        #f_flags_expr,
                        Box::new(old),
                        Box::new(new),
                    );
                }
            }
        };

        accessors.push(quote! {
            /// Generated getter for reactive field.
            pub fn #field_ident(&self) -> &#field_ty {
                &self.#field_ident
            }

            /// Generated setter for reactive field. Records the change in
            /// the provided [`ReactiveCtx`] if the value actually changed
            /// (or unconditionally for `always_update` fields).
            pub fn #setter_name(&mut self, value: #field_ty, ctx: &mut textual::reactive::ReactiveCtx)
            where
                #field_ty: PartialEq + Clone + Send + 'static,
            {
                #validate_stmt
                #set_body
            }

            /// Generated mutation notifier for a reactive field (Python
            /// `mutate_reactive`). Call this AFTER mutating the field in place
            /// (e.g. pushing to a `Vec`), to dispatch watchers / recompose
            /// unconditionally — the value is its own old and new value.
            pub fn #mutate_name(&mut self, ctx: &mut textual::reactive::ReactiveCtx)
            where
                #field_ty: Clone + Send + 'static,
            {
                let snapshot = self.#field_ident.clone();
                let snapshot_clone = self.#field_ident.clone();
                ctx.record_mutation(
                    #field_name_str,
                    #f_flags_expr,
                    Box::new(snapshot),
                    Box::new(snapshot_clone),
                );
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

    // Plain-watch fields: watch=true, watch_with_app=false.
    // These go into reactive_dispatch (no app access).
    let plain_watch_fields: Vec<&ReactiveField> = reactive_fields
        .iter()
        .filter(|f| f.watch && !f.watch_with_app)
        .collect();

    // watch_with_app fields: watch_with_app=true (may or may not also have watch=true).
    let app_watch_fields: Vec<&ReactiveField> = reactive_fields
        .iter()
        .filter(|f| f.watch_with_app)
        .collect();

    // Computed fields whose recomputed value should fire a watcher.
    // The recompute step records a change under the computed field's name, which
    // re-iterates through dispatch; these arms invoke the matching `watch_*`.
    let computed_plain_watch: Vec<&ComputedField> = computed_fields
        .iter()
        .filter(|c| c.watch && !c.watch_with_app)
        .collect();
    let computed_app_watch: Vec<&ComputedField> = computed_fields
        .iter()
        .filter(|c| c.watch_with_app)
        .collect();

    let has_plain_watch = !plain_watch_fields.is_empty() || !computed_plain_watch.is_empty();
    let has_app_watch = !app_watch_fields.is_empty() || !computed_app_watch.is_empty();
    let has_computed = !computed_fields.is_empty();

    // ── reactive_dispatch body (plain-watch + computed; no watch_with_app) ──
    let dispatch_body = if !has_plain_watch && !has_computed {
        quote! {
            let _ = (changes, ctx);
        }
    } else {
        let watcher_block = if has_plain_watch {
            let mut match_arms: Vec<TokenStream> = plain_watch_fields
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
            // Computed-field plain watchers (fire when the recomputed value changes).
            for cf in &computed_plain_watch {
                let field_name_str = cf.ident.to_string();
                let field_ty = &cf.ty;
                let watcher_name = format_ident!("watch_{}", cf.ident);
                match_arms.push(quote! {
                    #field_name_str => {
                        if let (Some(old), Some(new)) = (
                            change.old_value.downcast_ref::<#field_ty>(),
                            change.new_value.downcast_ref::<#field_ty>(),
                        ) {
                            self.#watcher_name(old, new, ctx);
                        }
                    }
                });
            }

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

    // ── reactive_dispatch_with_app body (all watch kinds + computed) ──
    // Only generated when at least one field has watch_with_app.
    let dispatch_with_app_impl = if has_app_watch {
        // All watcher arms for the with-app override: plain-watch first, then app-watch.
        let plain_arms: Vec<TokenStream> = plain_watch_fields
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

        let mut plain_arms = plain_arms;
        // Computed-field plain watchers also fire in the with-app dispatch.
        for cf in &computed_plain_watch {
            let field_name_str = cf.ident.to_string();
            let field_ty = &cf.ty;
            let watcher_name = format_ident!("watch_{}", cf.ident);
            plain_arms.push(quote! {
                #field_name_str => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<#field_ty>(),
                        change.new_value.downcast_ref::<#field_ty>(),
                    ) {
                        self.#watcher_name(old, new, ctx);
                    }
                }
            });
        }

        let mut app_arms: Vec<TokenStream> = app_watch_fields
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
                            self.#watcher_name(app, old, new, ctx);
                        }
                    }
                }
            })
            .collect();
        // Computed-field with-app watchers.
        for cf in &computed_app_watch {
            let field_name_str = cf.ident.to_string();
            let field_ty = &cf.ty;
            let watcher_name = format_ident!("watch_{}", cf.ident);
            app_arms.push(quote! {
                #field_name_str => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<#field_ty>(),
                        change.new_value.downcast_ref::<#field_ty>(),
                    ) {
                        self.#watcher_name(app, old, new, ctx);
                    }
                }
            });
        }

        let computed_block = if has_computed {
            quote! { #(#computed_recompute_stmts)* }
        } else {
            quote! {}
        };

        // If there are no plain arms and no app arms in the match, we need a fallback.
        let match_body = quote! {
            for change in changes {
                match change.field_name {
                    #(#plain_arms)*
                    #(#app_arms)*
                    _ => {}
                }
            }
            #computed_block
        };

        quote! {
            fn reactive_dispatch_with_app(
                &mut self,
                app: &mut textual::App,
                changes: &[textual::reactive::ReactiveChange],
                ctx: &mut textual::reactive::ReactiveCtx,
            ) {
                #match_body
            }
        }
    } else {
        // No watch_with_app fields — rely on the trait default (delegates to reactive_dispatch).
        quote! {}
    };

    // ── reactive_record_init ──
    // For each non-computed reactive field whose effective flags have init=true,
    // emit a synthetic change old==new==current value.
    let init_fields: Vec<&ReactiveField> = reactive_fields
        .iter()
        .filter(|f| {
            // Determine if this field has init=true
            if f.is_var {
                !f.init_false // var() has init=true; var_no_init() has init=false
            } else {
                !f.init_false // reactive() / reactive_layout() have init=true
            }
        })
        .collect();

    let record_init_impl = if !init_fields.is_empty() {
        let record_stmts: Vec<TokenStream> = init_fields
            .iter()
            .map(|field| {
                let field_ident = &field.ident;
                let field_name_str = field_ident.to_string();
                let f_flags_expr = flags_expr(field);
                // Init-phase changes must never recompose: Python's
                // `_initialize_reactive` fires watchers via `_check_watchers`,
                // which never refreshes/recomposes (recompose only happens in
                // `Reactive._set` / `mutate_reactive`). Recomposing at mount would
                // rebuild the freshly-composed tree and discard auto-focus.
                quote! {
                    ctx.record_change(
                        #field_name_str,
                        (#f_flags_expr).without_recompose(),
                        Box::new(self.#field_ident.clone()),
                        Box::new(self.#field_ident.clone()),
                    );
                }
            })
            .collect();

        quote! {
            fn reactive_record_init(&self, ctx: &mut textual::reactive::ReactiveCtx) {
                #(#record_stmts)*
            }
        }
    } else {
        quote! {}
    };

    // Generate the list of reactive field descriptors for `reactive_field_descriptors()`.
    let descriptor_entries: Vec<TokenStream> = reactive_fields
        .iter()
        .map(|field| {
            let field_name_str = field.ident.to_string();
            let f_flags_expr = flags_expr(field);
            quote! {
                textual::reactive::ReactiveFieldDescriptor {
                    name: #field_name_str,
                    flags: #f_flags_expr,
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

            #dispatch_with_app_impl

            fn reactive_field_descriptors(&self) -> &'static [textual::reactive::ReactiveFieldDescriptor] {
                static DESCRIPTORS: &[textual::reactive::ReactiveFieldDescriptor] = &[
                    #(#descriptor_entries),*
                ];
                DESCRIPTORS
            }

            #record_init_impl
        }
    };

    expanded
}
