//! Implementation of the `#[widget(base = <Type>)]` delegation attribute macro.
//!
//! Generates a complete `impl Widget` (and `impl Renderable`) block that
//! forwards the framework's structural / propagation method surface to a
//! `base` field, so a compound widget can "inherit" from a container without
//! hand-forwarding all ~63 delegated `Widget` methods.
//!
//! This is the first-class replacement for the deprecated declarative
//! `delegate_widget_to!` / `delegate_widget_method!` macros: the derive is the
//! single place that knows the delegated surface, so a future trait-shape
//! change (RA-2) becomes a macro-internal edit instead of user-visible churn.
//!
//! ## What is forwarded (the STRUCTURAL / PROPAGATION surface)
//!
//! render / measure / layout, `compose` (child declaration),
//! `on_event` / `on_event_capture` / `on_message` (PROPAGATION to the
//! base's arena children), `on_tick` / lifecycle, scroll, bindings/actions,
//! selection, tooltip/help, styles, etc. — exactly the 63-method list the
//! deprecated `delegate_widget_to!` forwarded.
//!
//! ## What is NOT forwarded (BEHAVIOR — supplied orthogonally by the user)
//!
//! - `style_type` / `style_type_aliases` keep the trait default, which returns
//!   the compound widget's OWN concrete type name — so the widget gets its own
//!   CSS identity, not the base's. Set a custom name with
//!   `#[widget(base = X, style_type = "Name")]`.
//! - Typed message handling is provided by `#[on(..)]` inherent methods, which
//!   dispatch through the runtime's separate handler surface — NOT through the
//!   forwarded `Widget::on_message` (that stays wired to the base for child
//!   propagation).
//! - Reactive state is provided by `#[derive(Reactive)]`. Because a reactive
//!   compound widget must expose ITSELF (not the base) as the reactive surface,
//!   `reactive_widget` is only routed to `Some(self)` when you opt in with
//!   `#[widget(base = X, reactive)]`; otherwise it forwards to the base.
//!
//! ## Options
//!
//! ```ignore
//! #[widget(base = VerticalGroup)]                       // field defaults to `base`
//! #[widget(base = Vertical, field = inner)]             // custom field name
//! #[widget(base = VerticalGroup, style_type = "Card")]  // custom CSS type
//! #[widget(base = VerticalGroup, reactive)]             // reactive_widget -> Some(self)
//! #[widget(base = VerticalGroup, override(render, on_message))] // user overrides
//! ```
//!
//! ## Overriding a forwarded method (no second `impl Widget` block)
//!
//! List the method in `override(..)` and write it as an *inherent* method with
//! the exact `Widget` signature. The generated trait method body then calls
//! `self.method(..)`, which resolves to your inherent method (inherent methods
//! win over trait methods in method-call syntax), so there is no recursion:
//!
//! ```ignore
//! #[widget(base = VerticalGroup, override(render))]
//! struct Card { base: VerticalGroup }
//!
//! impl Card {
//!     // MUST match the Widget::render signature exactly.
//!     fn render(&self, console: &rich_rs::Console, options: &rich_rs::ConsoleOptions)
//!         -> rich_rs::Segments { /* custom chrome */ }
//! }
//! ```
//!
//! Footgun: if the inherent method's signature does not match, method
//! resolution falls back to the trait method and you get infinite recursion.
//! For a method outside the delegated surface, hand-write the full `impl
//! Widget` instead (escape hatch).

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse::{Parse, ParseStream},
    parse2,
    punctuated::Punctuated,
    Ident, ItemStruct, LitStr, Path, Token, Type,
};

/// Parsed `#[widget(..)]` arguments.
struct WidgetArgs {
    /// The `base = <Type>` type path (documentation + readability; forwarding is
    /// by field name, not type).
    _base: Path,
    /// Field name to forward to (default `base`).
    field: Ident,
    /// Optional custom `style_type` string.
    style_type: Option<String>,
    /// Whether `reactive_widget` should return `Some(self)` (opt-in for
    /// `#[derive(Reactive)]` compound widgets).
    reactive: bool,
    /// Methods the user overrides via an inherent method.
    overrides: Vec<Ident>,
    /// `#[on(..)]` handler method names to wire into the generated `on_message`
    /// (e.g. `on(on_button, on_checkbox)`). The generated `on_message` calls each
    /// `__on_dispatch_<name>` with a materialized `WidgetCtx` before forwarding to
    /// the base. Empty = keep the plain forward-to-base `on_message`.
    on_handlers: Vec<Ident>,
}

impl Parse for WidgetArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut base: Option<Path> = None;
        let mut field: Option<Ident> = None;
        let mut style_type: Option<String> = None;
        let mut reactive = false;
        let mut overrides: Vec<Ident> = Vec::new();
        let mut on_handlers: Vec<Ident> = Vec::new();

        while !input.is_empty() {
            // `override` is a reserved keyword, so it does not parse as an
            // `Ident`; handle it explicitly.
            if input.peek(Token![override]) {
                input.parse::<Token![override]>()?;
                let content;
                syn::parenthesized!(content in input);
                let names = Punctuated::<Ident, Token![,]>::parse_terminated(&content)?;
                overrides.extend(names);
                if input.peek(Token![,]) {
                    input.parse::<Token![,]>()?;
                }
                continue;
            }

            let key: Ident = input.parse()?;
            // `on(handler1, handler2)` — list of `#[on(..)]` handler methods to
            // wire into the generated `on_message`.
            if key == "on" {
                let content;
                syn::parenthesized!(content in input);
                let names = Punctuated::<Ident, Token![,]>::parse_terminated(&content)?;
                on_handlers.extend(names);
                if input.peek(Token![,]) {
                    input.parse::<Token![,]>()?;
                }
                continue;
            }
            match key.to_string().as_str() {
                "base" => {
                    input.parse::<Token![=]>()?;
                    let ty: Type = input.parse()?;
                    // Accept any type; keep the path form when possible for docs.
                    let path = match ty {
                        Type::Path(tp) => tp.path,
                        other => {
                            return Err(syn::Error::new_spanned(
                                other,
                                "`base` must be a type path, e.g. `base = VerticalGroup`",
                            ))
                        }
                    };
                    base = Some(path);
                }
                "field" => {
                    input.parse::<Token![=]>()?;
                    field = Some(input.parse()?);
                }
                "style_type" => {
                    input.parse::<Token![=]>()?;
                    let lit: LitStr = input.parse()?;
                    style_type = Some(lit.value());
                }
                "reactive" => {
                    reactive = true;
                }
                other => {
                    return Err(syn::Error::new_spanned(
                        &key,
                        format!(
                            "unknown `#[widget]` argument `{other}`; expected one of: \
                             base, field, style_type, reactive, override(..), on(..)"
                        ),
                    ));
                }
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        let base = base.ok_or_else(|| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                "`#[widget(..)]` requires `base = <Type>`",
            )
        })?;

        Ok(WidgetArgs {
            _base: base,
            field: field.unwrap_or_else(|| format_ident!("base")),
            style_type,
            reactive,
            overrides,
            on_handlers,
        })
    }
}

/// One delegated `Widget` method: its name, full signature (no body), and the
/// `name(args)` call expression used to build the forwarding / override body.
struct MethodSpec {
    name: &'static str,
    sig: TokenStream,
    call: TokenStream,
}

fn method_table() -> Vec<MethodSpec> {
    macro_rules! m {
        ($name:literal, $sig:expr, $call:expr) => {
            MethodSpec {
                name: $name,
                sig: $sig,
                call: $call,
            }
        };
    }

    vec![
        // ── Rendering ──────────────────────────────────────────────────
        m!(
            "render",
            quote! { fn render(&self, console: &rich_rs::Console, options: &rich_rs::ConsoleOptions) -> rich_rs::Segments },
            quote! { render(console, options) }
        ),
        m!(
            "render_with_debug",
            quote! { fn render_with_debug(&self, console: &rich_rs::Console, options: &rich_rs::ConsoleOptions, debug: &textual::debug::DebugLayout) -> rich_rs::Segments },
            quote! { render_with_debug(console, options, debug) }
        ),
        m!(
            "render_line",
            quote! { fn render_line(&self, y: usize, console: &rich_rs::Console, options: &rich_rs::ConsoleOptions) -> rich_rs::Segments },
            quote! { render_line(y, console, options) }
        ),
        m!(
            "render_lines",
            quote! { fn render_lines(&self, start_y: usize, line_count: usize, console: &rich_rs::Console, options: &rich_rs::ConsoleOptions) -> Vec<rich_rs::Segments> },
            quote! { render_lines(start_y, line_count, console, options) }
        ),
        // ── Composition ────────────────────────────────────────────────
        m!(
            "compose",
            quote! { fn compose(&mut self) -> textual::compose::ComposeResult },
            quote! { compose() }
        ),
        m!(
            "take_pending_mount_messages",
            quote! { fn take_pending_mount_messages(&mut self) -> Vec<Box<dyn textual::message::Message>> },
            quote! { take_pending_mount_messages() }
        ),
        // ── Focus / node state ─────────────────────────────────────────
        m!("focusable", quote! { fn focusable(&self) -> bool }, quote! { focusable() }),
        m!("can_focus", quote! { fn can_focus(&self) -> bool }, quote! { can_focus() }),
        m!(
            "can_focus_children",
            quote! { fn can_focus_children(&self) -> bool },
            quote! { can_focus_children() }
        ),
        m!(
            "on_node_state_changed",
            quote! { fn on_node_state_changed(&mut self, old: textual::widgets::NodeState, new: textual::widgets::NodeState) },
            quote! { on_node_state_changed(old, new) }
        ),
        // ── Lifecycle ──────────────────────────────────────────────────
        m!(
            "on_mount",
            quote! { fn on_mount(&mut self, ctx: &mut textual::event::WidgetCtx) },
            quote! { on_mount(ctx) }
        ),
        m!("on_unmount", quote! { fn on_unmount(&mut self) }, quote! { on_unmount() }),
        m!("on_tick", quote! { fn on_tick(&mut self, tick: u64) }, quote! { on_tick(tick) }),
        m!(
            "on_resize",
            quote! { fn on_resize(&mut self, width: u16, height: u16) },
            quote! { on_resize(width, height) }
        ),
        m!(
            "on_layout",
            quote! { fn on_layout(&mut self, width: u16, height: u16) },
            quote! { on_layout(width, height) }
        ),
        m!(
            "set_virtual_content_size",
            quote! { fn set_virtual_content_size(&mut self, width: usize, height: usize) },
            quote! { set_virtual_content_size(width, height) }
        ),
        // ── Events ─────────────────────────────────────────────────────
        m!(
            "on_event_capture",
            quote! { fn on_event_capture(&mut self, event: &textual::event::Event, ctx: &mut textual::event::WidgetCtx) },
            quote! { on_event_capture(event, ctx) }
        ),
        m!(
            "on_event",
            quote! { fn on_event(&mut self, event: &textual::event::Event, ctx: &mut textual::event::WidgetCtx) },
            quote! { on_event(event, ctx) }
        ),
        m!(
            "on_message",
            quote! { fn on_message(&mut self, message: &textual::message::MessageEvent, ctx: &mut textual::event::WidgetCtx) },
            quote! { on_message(message, ctx) }
        ),
        m!(
            "on_mouse_scroll",
            quote! { fn on_mouse_scroll(&mut self, delta_x: i32, delta_y: i32, ctx: &mut textual::event::WidgetCtx) },
            quote! { on_mouse_scroll(delta_x, delta_y, ctx) }
        ),
        m!(
            "on_mouse_move",
            quote! { fn on_mouse_move(&mut self, x: u16, y: u16) -> bool },
            quote! { on_mouse_move(x, y) }
        ),
        // ── App-level hooks ────────────────────────────────────────────
        m!(
            "on_app_key",
            quote! { fn on_app_key(&mut self, app: &mut textual::App, key: &textual::keys::KeyEventData, ctx: &mut textual::event::WidgetCtx) },
            quote! { on_app_key(app, key, ctx) }
        ),
        m!(
            "on_app_action",
            quote! { fn on_app_action(&mut self, app: &mut textual::App, action: textual::event::Action, ctx: &mut textual::event::WidgetCtx) },
            quote! { on_app_action(app, action, ctx) }
        ),
        m!(
            "on_app_message",
            quote! { fn on_app_message(&mut self, app: &mut textual::App, message: &textual::message::MessageEvent, ctx: &mut textual::event::WidgetCtx) },
            quote! { on_app_message(app, message, ctx) }
        ),
        m!(
            "on_app_tick",
            quote! { fn on_app_tick(&mut self, app: &mut textual::App, tick: u64, ctx: &mut textual::event::WidgetCtx) },
            quote! { on_app_tick(app, tick, ctx) }
        ),
        m!(
            "on_app_mount",
            quote! { fn on_app_mount(&mut self, app: &mut textual::App, ctx: &mut textual::event::WidgetCtx) },
            quote! { on_app_mount(app, ctx) }
        ),
        // ── Scroll ─────────────────────────────────────────────────────
        m!(
            "scroll_offset",
            quote! { fn scroll_offset(&self) -> (usize, usize) },
            quote! { scroll_offset() }
        ),
        m!(
            "scroll_offset_f32",
            quote! { fn scroll_offset_f32(&self) -> (f32, f32) },
            quote! { scroll_offset_f32() }
        ),
        m!(
            "scroll_viewport_size",
            quote! { fn scroll_viewport_size(&self) -> Option<(usize, usize)> },
            quote! { scroll_viewport_size() }
        ),
        m!(
            "scroll_virtual_content_size",
            quote! { fn scroll_virtual_content_size(&self) -> Option<(usize, usize)> },
            quote! { scroll_virtual_content_size() }
        ),
        m!(
            "clips_descendants_to_content",
            quote! { fn clips_descendants_to_content(&self) -> bool },
            quote! { clips_descendants_to_content() }
        ),
        // ── Tree / layout ──────────────────────────────────────────────
        m!(
            "child_display_for_tree",
            quote! { fn child_display_for_tree(&self, child_index: usize) -> Option<bool> },
            quote! { child_display_for_tree(child_index) }
        ),
        m!(
            "tree_child_content_inset",
            quote! { fn tree_child_content_inset(&self) -> (u16, u16, u16, u16) },
            quote! { tree_child_content_inset() }
        ),
        m!(
            "layout_height",
            quote! { fn layout_height(&self) -> Option<usize> },
            quote! { layout_height() }
        ),
        m!(
            "content_width",
            quote! { fn content_width(&self) -> Option<usize> },
            quote! { content_width() }
        ),
        m!(
            "preserve_underlay",
            quote! { fn preserve_underlay(&self) -> bool },
            quote! { preserve_underlay() }
        ),
        // ── Actions / bindings ─────────────────────────────────────────
        m!(
            "bindings",
            quote! { fn bindings(&self) -> Vec<textual::widgets::BindingDecl> },
            quote! { bindings() }
        ),
        m!(
            "binding_hints",
            quote! { fn binding_hints(&self) -> Vec<textual::event::BindingHint> },
            quote! { binding_hints() }
        ),
        m!(
            "execute_action",
            quote! { fn execute_action(&mut self, action: &textual::action::ParsedAction, ctx: &mut textual::event::WidgetCtx) -> bool },
            quote! { execute_action(action, ctx) }
        ),
        m!(
            "action_namespace",
            quote! { fn action_namespace(&self) -> &str },
            quote! { action_namespace() }
        ),
        m!(
            "action_registry",
            quote! { fn action_registry(&self) -> &[textual::action::ActionDecl] },
            quote! { action_registry() }
        ),
        // ── Styles / seed ──────────────────────────────────────────────
        m!(
            "style",
            quote! { fn style(&self) -> Option<textual::style::Style> },
            quote! { style() }
        ),
        m!(
            "set_inline_style",
            quote! { fn set_inline_style(&mut self, style: textual::style::Style) },
            quote! { set_inline_style(style) }
        ),
        m!(
            "take_node_seed",
            quote! { fn take_node_seed(&mut self) -> textual::widgets::NodeSeed },
            quote! { take_node_seed() }
        ),
        m!(
            "border_title",
            quote! { fn border_title(&self) -> Option<&str> },
            quote! { border_title() }
        ),
        m!(
            "border_subtitle",
            quote! { fn border_subtitle(&self) -> Option<&str> },
            quote! { border_subtitle() }
        ),
        // ── State ──────────────────────────────────────────────────────
        m!("is_active", quote! { fn is_active(&self) -> bool }, quote! { is_active() }),
        m!(
            "mouse_interactive",
            quote! { fn mouse_interactive(&self) -> bool },
            quote! { mouse_interactive() }
        ),
        // ── Tooltip / help ─────────────────────────────────────────────
        m!("tooltip", quote! { fn tooltip(&self) -> Option<String> }, quote! { tooltip() }),
        m!(
            "tooltip_anchor",
            quote! { fn tooltip_anchor(&self) -> Option<(u16, u16)> },
            quote! { tooltip_anchor() }
        ),
        m!(
            "help_markup",
            quote! { fn help_markup(&self) -> Option<&str> },
            quote! { help_markup() }
        ),
        // ── Selection ──────────────────────────────────────────────────
        m!("allow_select", quote! { fn allow_select(&self) -> bool }, quote! { allow_select() }),
        m!(
            "selection_at",
            quote! { fn selection_at(&self, x: u16, y: u16) -> Option<textual::widgets::WidgetSelectionAnchor> },
            quote! { selection_at(x, y) }
        ),
        m!(
            "selection_word_range_at",
            quote! { fn selection_word_range_at(&self, x: u16, y: u16) -> Option<(textual::widgets::WidgetSelectionAnchor, textual::widgets::WidgetSelectionAnchor)> },
            quote! { selection_word_range_at(x, y) }
        ),
        m!(
            "selection_all_range",
            quote! { fn selection_all_range(&self) -> Option<(textual::widgets::WidgetSelectionAnchor, textual::widgets::WidgetSelectionAnchor)> },
            quote! { selection_all_range() }
        ),
        m!(
            "update_selection",
            quote! { fn update_selection(&mut self, from: textual::widgets::WidgetSelectionAnchor, to: textual::widgets::WidgetSelectionAnchor) -> bool },
            quote! { update_selection(from, to) }
        ),
        m!(
            "clear_selection",
            quote! { fn clear_selection(&mut self) -> bool },
            quote! { clear_selection() }
        ),
        m!(
            "get_selection",
            quote! { fn get_selection(&self) -> Option<String> },
            quote! { get_selection() }
        ),
        m!(
            "selection_updated",
            quote! { fn selection_updated(&mut self, ctx: &mut textual::event::WidgetCtx) },
            quote! { selection_updated(ctx) }
        ),
        // ── Reactive ───────────────────────────────────────────────────
        m!(
            "reactive_widget",
            quote! { fn reactive_widget(&mut self) -> Option<&mut dyn textual::reactive::ReactiveWidget> },
            quote! { reactive_widget() }
        ),
        // ── Style type (NOT forwarded by default; overridable / attr) ───
        m!(
            "style_type",
            quote! { fn style_type(&self) -> &'static str },
            quote! { style_type() }
        ),
        m!(
            "style_type_aliases",
            quote! { fn style_type_aliases(&self) -> &[&'static str] },
            quote! { style_type_aliases() }
        ),
    ]
}

/// Names that are forwarded to the base by DEFAULT (the vetted 63-method
/// structural surface — everything in the table EXCEPT `style_type` /
/// `style_type_aliases`, which keep the trait default so the compound widget
/// gets its own CSS identity).
fn is_default_forwarded(name: &str) -> bool {
    !matches!(name, "style_type" | "style_type_aliases")
}

pub fn widget_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args: WidgetArgs = match parse2(attr) {
        Ok(a) => a,
        Err(e) => return e.to_compile_error(),
    };
    let item_struct: ItemStruct = match parse2(item.clone()) {
        Ok(s) => s,
        Err(e) => return e.to_compile_error(),
    };

    let name = &item_struct.ident;
    let (impl_generics, ty_generics, where_clause) = item_struct.generics.split_for_impl();
    let field = &args.field;

    let table = method_table();

    // Validate `override(..)` names against the known surface.
    let known: std::collections::HashSet<&str> = table.iter().map(|m| m.name).collect();
    for ov in &args.overrides {
        let ov_s = ov.to_string();
        if !known.contains(ov_s.as_str()) {
            return syn::Error::new_spanned(
                ov,
                format!(
                    "`override({ov_s})` is not a delegated `Widget` method; \
                     override an unknown method by hand-writing the full `impl Widget` instead"
                ),
            )
            .to_compile_error();
        }
    }
    let overrides: std::collections::HashSet<String> =
        args.overrides.iter().map(|i| i.to_string()).collect();

    // `on(..)` wires the generated `on_message`; `override(on_message)` replaces
    // it. Both at once is contradictory.
    if !args.on_handlers.is_empty() && overrides.contains("on_message") {
        return syn::Error::new_spanned(
            &args.on_handlers[0],
            "`on(..)` cannot be combined with `override(on_message)`; the override \
             replaces the generated `on_message` that `on(..)` would wire",
        )
        .to_compile_error();
    }

    // Validate the target field exists on the struct.
    let field_exists = item_struct.fields.iter().any(|f| {
        f.ident
            .as_ref()
            .map(|id| id == field)
            .unwrap_or(false)
    });
    if !field_exists {
        return syn::Error::new_spanned(
            field,
            format!(
                "`#[widget]` expects a field named `{field}` to delegate to \
                 (use `field = <name>` to point at a differently-named field)"
            ),
        )
        .to_compile_error();
    }

    let mut methods: Vec<TokenStream> = Vec::new();
    for spec in &table {
        let overridden = overrides.contains(spec.name);
        let sig = &spec.sig;
        let call = &spec.call;

        // `reactive_widget` with the `reactive` opt-in exposes SELF, not base.
        if spec.name == "reactive_widget" && args.reactive && !overridden {
            methods.push(quote! {
                fn reactive_widget(&mut self) -> Option<&mut dyn textual::reactive::ReactiveWidget> {
                    Some(self)
                }
            });
            continue;
        }

        // `on_message` with an `on(..)` handler list: run the widget's own
        // `#[on(..)]` handlers with the REAL dispatch `WidgetCtx` (so
        // handler-posted messages / reactive changes flow through the ctx the
        // routing dispatch site enqueues after `on_message` returns), THEN
        // forward to the base's own `on_message`.
        //
        // Since RA2.2 the trait `on_message` already receives a `&mut WidgetCtx`
        // (the routing site materializes it via `__from_dispatch` and enqueues
        // the recorded reactive changes on return), so the glue no longer
        // materializes its own ctx or enqueues — it hands the incoming ctx to
        // each handler and to the base.
        //
        // NOTE: the base forward invokes the BASE widget's own on_message
        // behavior — it does NOT re-dispatch to children (children receive the
        // message through routing's bubble phase, node by node). To replace this
        // glue entirely, `override(on_message)` and call each `__on_dispatch_*`
        // yourself (`on(..)` + `override(on_message)` is a compile error).
        if spec.name == "on_message" && !args.on_handlers.is_empty() && !overridden {
            let dispatch_calls: Vec<TokenStream> = args
                .on_handlers
                .iter()
                .map(|h| {
                    let dispatch = format_ident!("__on_dispatch_{}", h);
                    quote! { let _ = self.#dispatch(message, ctx); }
                })
                .collect();
            methods.push(quote! {
                fn on_message(
                    &mut self,
                    message: &textual::message::MessageEvent,
                    ctx: &mut textual::event::WidgetCtx,
                ) {
                    #(#dispatch_calls)*
                    // Forward to the base widget's own on_message (its behavior),
                    // exactly as the plain forwarding row would — this is NOT a
                    // re-dispatch to children.
                    self.#field.on_message(message, ctx);
                }
            });
            continue;
        }

        // `style_type` with the `style_type = "..."` attr emits a literal.
        if spec.name == "style_type" && args.style_type.is_some() && !overridden {
            let lit = args.style_type.as_ref().unwrap();
            methods.push(quote! {
                fn style_type(&self) -> &'static str { #lit }
            });
            continue;
        }

        if overridden {
            // Call the user's inherent method (inherent wins over the trait
            // method in method-call syntax, so this is not recursive).
            methods.push(quote! { #sig { self.#call } });
        } else if is_default_forwarded(spec.name) {
            methods.push(quote! { #sig { self.#field.#call } });
        }
        // else: style_type / style_type_aliases with no attr and no override
        // -> keep the trait default (own concrete type name).
    }

    quote! {
        #item_struct

        impl #impl_generics textual::widgets::Widget for #name #ty_generics #where_clause {
            #(#methods)*
        }

        impl #impl_generics rich_rs::Renderable for #name #ty_generics #where_clause {
            fn render(&self, console: &rich_rs::Console, options: &rich_rs::ConsoleOptions) -> rich_rs::Segments {
                textual::widgets::Widget::render(self, console, options)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_defaults_field_to_base() {
        let args: WidgetArgs = parse2(quote! { base = VerticalGroup }).unwrap();
        assert_eq!(args.field.to_string(), "base");
        assert!(args.style_type.is_none());
        assert!(!args.reactive);
        assert!(args.overrides.is_empty());
    }

    #[test]
    fn parse_all_options() {
        let args: WidgetArgs = parse2(
            quote! { base = Vertical, field = inner, style_type = "Card", reactive, override(render, on_message) },
        )
        .unwrap();
        assert_eq!(args.field.to_string(), "inner");
        assert_eq!(args.style_type.as_deref(), Some("Card"));
        assert!(args.reactive);
        let ov: Vec<String> = args.overrides.iter().map(|i| i.to_string()).collect();
        assert_eq!(ov, vec!["render".to_string(), "on_message".to_string()]);
    }

    #[test]
    fn parse_on_handler_list() {
        let args: WidgetArgs =
            parse2(quote! { base = Vertical, on(on_button, on_checkbox) }).unwrap();
        let on: Vec<String> = args.on_handlers.iter().map(|i| i.to_string()).collect();
        assert_eq!(on, vec!["on_button".to_string(), "on_checkbox".to_string()]);
    }

    #[test]
    fn on_message_glue_emitted_for_on_list() {
        let out = widget_impl(
            quote! { base = Vertical, on(on_button) },
            quote! { struct W { base: Vertical } },
        )
        .to_string();
        // Generated on_message calls the dispatch method with the incoming ctx
        // (RA2.2: the routing site owns __from_dispatch + reactive enqueue).
        assert!(out.contains("__on_dispatch_on_button"));
    }

    #[test]
    fn on_list_with_override_on_message_is_an_error() {
        let out = widget_impl(
            quote! { base = Vertical, on(on_button), override(on_message) },
            quote! { struct W { base: Vertical } },
        )
        .to_string();
        assert!(out.contains("cannot be combined"));
    }

    #[test]
    fn missing_base_is_an_error() {
        assert!(parse2::<WidgetArgs>(quote! { field = inner }).is_err());
    }

    #[test]
    fn unknown_argument_is_an_error() {
        assert!(parse2::<WidgetArgs>(quote! { base = X, bogus = 1 }).is_err());
    }

    #[test]
    fn method_table_has_no_duplicate_names() {
        let table = method_table();
        let mut names: Vec<&str> = table.iter().map(|m| m.name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "duplicate method name in the delegated surface table");
    }

    #[test]
    fn default_forwarded_surface_matches_delegate_widget_to() {
        // The default-forwarded set is the 63-method surface the deprecated
        // `delegate_widget_to!` forwarded (table minus the two non-forwarded
        // style_type methods, which keep the trait default). RA2.2 merged the
        // former `on_mount` + `on_mount_ctx` into a single `on_mount(ctx)` row,
        // so the count is back to 63.
        let forwarded = method_table()
            .iter()
            .filter(|m| is_default_forwarded(m.name))
            .count();
        assert_eq!(
            forwarded, 63,
            "default-forwarded surface = delegate_widget_to! (63)"
        );
    }
}
