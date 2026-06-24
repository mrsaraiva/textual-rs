//! Declarative message routing — the Rust analogue of Python Textual's
//! `@on(Message, selector)` decorator (`textual/_on.py`).
//!
//! Python's `@on` records `(message_type, selectors)` pairs on a handler and the
//! message pump dispatches a message to a handler when the message's concrete
//! type matches *and* every selector matches the corresponding widget attribute
//! (`control` by default). See `MessagePump._get_dispatch_methods`.
//!
//! Rust has no per-method metaclass registry, so we provide the same semantics
//! as an explicit, type-safe registry:
//!
//! ```ignore
//! let mut router: MessageRouter<MyApp> = MessageRouter::new();
//! router.on::<ButtonPressed>("#quit", |app, msg, ctx| { /* ... */ });
//! router.on_any::<InputChanged>(|app, msg, ctx| { /* ... */ });
//! // inside on_message_with_app:
//! router.dispatch(self, event, ctx);
//! ```
//!
//! A handler runs when:
//! 1. the message payload downcasts to the registered type `M`, and
//! 2. the registered [`Selector`] matches the message's [`ControlMeta`]
//!    (a selector of `None`/empty always matches, mirroring `@on(Message)`).
//!
//! The control metadata is taken from [`Message::control_meta`] (messages that
//! identify their originating widget — e.g. [`crate::message::ButtonPressed`] —
//! implement it), exactly as Python matches against `message.control`.

use std::any::{Any, TypeId};

use crate::event::EventCtx;
use crate::message::{Message, MessageEvent};

// ---------------------------------------------------------------------------
// ControlMeta — identity of the widget that produced a message ("control")
// ---------------------------------------------------------------------------

/// CSS-relevant identity of the widget a message originated from (its
/// "control", in Python terms). Used by [`Selector`] matching so `@on`-style
/// routing can filter on `#id`, `.class`, and `Type` selectors.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ControlMeta {
    /// CSS id of the control widget (without the leading `#`), if any.
    pub id: Option<String>,
    /// CSS classes of the control widget (without leading `.`).
    pub classes: Vec<String>,
    /// CSS type name of the control widget (e.g. `"Button"`), if known.
    pub type_name: Option<String>,
}

impl ControlMeta {
    /// An empty meta (no id, no classes, no type). Matches only the empty
    /// (universal) selector.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Build a meta with just an id (the common case — e.g. `Button.id`).
    pub fn with_id(id: impl Into<String>) -> Self {
        Self {
            id: Some(id.into()),
            classes: Vec::new(),
            type_name: None,
        }
    }

    /// Builder: set the type name.
    pub fn type_named(mut self, type_name: impl Into<String>) -> Self {
        self.type_name = Some(type_name.into());
        self
    }

    /// Builder: add a class.
    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.classes.push(class.into());
        self
    }

    /// Builder: add several classes.
    pub fn classes<I, S>(mut self, classes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.classes.extend(classes.into_iter().map(Into::into));
        self
    }

    fn has_class(&self, class: &str) -> bool {
        self.classes.iter().any(|c| c == class)
    }
}

// ---------------------------------------------------------------------------
// Selector — a small CSS-like selector for `@on` filtering
// ---------------------------------------------------------------------------

/// A single compound selector term: a type and/or id and/or set of classes,
/// all of which must match (logical AND), like `Button#quit.primary`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CompoundSelector {
    type_name: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
}

impl CompoundSelector {
    fn matches(&self, meta: &ControlMeta) -> bool {
        if let Some(ty) = &self.type_name {
            match &meta.type_name {
                Some(meta_ty) if meta_ty == ty => {}
                _ => return false,
            }
        }
        if let Some(id) = &self.id {
            match &meta.id {
                Some(meta_id) if meta_id == id => {}
                _ => return false,
            }
        }
        for class in &self.classes {
            if !meta.has_class(class) {
                return false;
            }
        }
        true
    }
}

/// A parsed CSS-ish selector for declarative message routing.
///
/// Supports the selector forms used by Python Textual's `@on` in the documented
/// demos: type (`Button`), id (`#quit`), class (`.toggle`), compound
/// (`Button#quit.primary`, `.toggle.dark`), and comma-separated groups
/// (`#quit, #cancel`). A group matches if *any* of its compound terms match.
///
/// The empty selector (`Selector::any()` or `""`) matches everything, mirroring
/// the selector-less `@on(Message)` form.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selector {
    /// Comma-separated alternatives. Empty => universal (matches everything).
    groups: Vec<CompoundSelector>,
}

/// Error returned when a selector string cannot be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorParseError(pub String);

impl std::fmt::Display for SelectorParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid selector: {}", self.0)
    }
}

impl std::error::Error for SelectorParseError {}

impl Selector {
    /// A universal selector that matches every control (the `@on(Message)` form).
    pub fn any() -> Self {
        Self::default()
    }

    /// Whether this is the universal (matches-everything) selector.
    pub fn is_universal(&self) -> bool {
        self.groups.is_empty()
    }

    /// Parse a selector string (`#id`, `.class`, `Type`, compound, or
    /// comma-separated groups). An empty/whitespace string yields the universal
    /// selector.
    pub fn parse(input: &str) -> Result<Self, SelectorParseError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Ok(Self::any());
        }
        let mut groups = Vec::new();
        for part in trimmed.split(',') {
            let part = part.trim();
            if part.is_empty() {
                return Err(SelectorParseError(format!(
                    "empty term in selector {input:?}"
                )));
            }
            groups.push(parse_compound(part)?);
        }
        Ok(Self { groups })
    }

    /// Whether this selector matches the given control metadata.
    ///
    /// The universal selector matches everything. Otherwise the meta must match
    /// at least one comma-separated compound term.
    pub fn matches(&self, meta: &ControlMeta) -> bool {
        if self.is_universal() {
            return true;
        }
        self.groups.iter().any(|g| g.matches(meta))
    }
}

impl std::str::FromStr for Selector {
    type Err = SelectorParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

fn parse_compound(part: &str) -> Result<CompoundSelector, SelectorParseError> {
    let mut compound = CompoundSelector::default();
    let mut chars = part.char_indices().peekable();

    // A leading bare identifier (no `#`/`.` prefix) is a type selector.
    if let Some(&(_, c)) = chars.peek() {
        if is_ident_start(c) {
            let ident = take_ident(&mut chars);
            compound.type_name = Some(ident);
        }
    }

    while let Some(&(idx, c)) = chars.peek() {
        match c {
            '#' => {
                chars.next();
                let ident = take_ident(&mut chars);
                if ident.is_empty() {
                    return Err(SelectorParseError(format!("empty id in {part:?}")));
                }
                if compound.id.is_some() {
                    return Err(SelectorParseError(format!(
                        "multiple ids in selector term {part:?}"
                    )));
                }
                compound.id = Some(ident);
            }
            '.' => {
                chars.next();
                let ident = take_ident(&mut chars);
                if ident.is_empty() {
                    return Err(SelectorParseError(format!("empty class in {part:?}")));
                }
                compound.classes.push(ident);
            }
            _ => {
                return Err(SelectorParseError(format!(
                    "unexpected character {c:?} at offset {idx} in {part:?}"
                )));
            }
        }
    }

    Ok(compound)
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '-'
}

fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

fn take_ident(chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>) -> String {
    let mut out = String::new();
    while let Some(&(_, c)) = chars.peek() {
        if is_ident_continue(c) {
            out.push(c);
            chars.next();
        } else {
            break;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// MessageRouter — the declarative `@on` registry
// ---------------------------------------------------------------------------

type RouteFn<S> = Box<dyn FnMut(&mut S, &dyn Any, &mut EventCtx) + Send + Sync>;

struct Route<S> {
    type_id: TypeId,
    selector: Selector,
    handler: RouteFn<S>,
}

/// A declarative message-routing table, the Rust analogue of accumulating
/// `@on(Message, selector)` handlers on a class.
///
/// Register handlers with [`on`][MessageRouter::on] /
/// [`on_any`][MessageRouter::on_any], then call [`dispatch`][MessageRouter::dispatch]
/// from an `on_message`/`on_message_with_app` hook. Handlers run in
/// registration order; all matching handlers run (Python runs every decorated
/// handler whose type + selectors match).
pub struct MessageRouter<S> {
    routes: Vec<Route<S>>,
}

impl<S> Default for MessageRouter<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> MessageRouter<S> {
    /// Create an empty router.
    pub fn new() -> Self {
        Self { routes: Vec::new() }
    }

    /// Register a handler for message type `M`, filtered by `selector`.
    ///
    /// Mirrors `@on(M, selector)`. The selector is matched against the message's
    /// [`Message::control_meta`]. A `""` selector matches every control (same as
    /// [`on_any`][MessageRouter::on_any]).
    ///
    /// # Panics
    /// Panics if `selector` is not a valid selector string. Use
    /// [`try_on`][MessageRouter::try_on] to handle parse errors. (Python raises
    /// `OnDecoratorError` at import time for the same condition.)
    pub fn on<M, F>(&mut self, selector: &str, handler: F) -> &mut Self
    where
        M: Message,
        F: FnMut(&mut S, &M, &mut EventCtx) + Send + Sync + 'static,
    {
        self.try_on::<M, F>(selector, handler)
            .unwrap_or_else(|e| panic!("{e}"))
    }

    /// Fallible variant of [`on`][MessageRouter::on]: returns the parse error
    /// instead of panicking on a malformed selector.
    pub fn try_on<M, F>(
        &mut self,
        selector: &str,
        mut handler: F,
    ) -> Result<&mut Self, SelectorParseError>
    where
        M: Message,
        F: FnMut(&mut S, &M, &mut EventCtx) + Send + Sync + 'static,
    {
        let selector = Selector::parse(selector)?;
        self.routes.push(Route {
            type_id: TypeId::of::<M>(),
            selector,
            handler: Box::new(move |state, any, ctx| {
                if let Some(msg) = any.downcast_ref::<M>() {
                    handler(state, msg, ctx);
                }
            }),
        });
        Ok(self)
    }

    /// Register a handler for message type `M` with no selector filtering.
    ///
    /// Mirrors the selector-less `@on(M)` form.
    pub fn on_any<M, F>(&mut self, handler: F) -> &mut Self
    where
        M: Message,
        F: FnMut(&mut S, &M, &mut EventCtx) + Send + Sync + 'static,
    {
        self.on::<M, F>("", handler)
    }

    /// Dispatch a message event through the routing table.
    ///
    /// Every registered handler whose message type matches *and* whose selector
    /// matches the message's control runs, in registration order. Returns `true`
    /// if at least one handler ran.
    pub fn dispatch(&mut self, state: &mut S, event: &MessageEvent, ctx: &mut EventCtx) -> bool {
        let type_id = event.payload_type_id();
        let meta = event.payload().control_meta();
        let mut ran = false;
        for route in &mut self.routes {
            if route.type_id != type_id {
                continue;
            }
            let matches = match &meta {
                Some(meta) => route.selector.matches(meta),
                // No control metadata => only universal selectors match,
                // mirroring Python where a missing `control` skips a selector.
                None => route.selector.is_universal(),
            };
            if matches {
                (route.handler)(state, event.payload().as_any(), ctx);
                ran = true;
            }
        }
        ran
    }

    /// Number of registered routes.
    pub fn len(&self) -> usize {
        self.routes.len()
    }

    /// Whether the router has no registered routes.
    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{ButtonPressed, CheckboxChanged};
    use crate::node_id::node_id_from_ffi;

    // ── Selector parsing ──────────────────────────────────────────────

    #[test]
    fn empty_selector_is_universal() {
        assert!(Selector::parse("").unwrap().is_universal());
        assert!(Selector::parse("   ").unwrap().is_universal());
        assert!(Selector::any().is_universal());
    }

    #[test]
    fn universal_matches_anything() {
        let sel = Selector::any();
        assert!(sel.matches(&ControlMeta::empty()));
        assert!(sel.matches(&ControlMeta::with_id("anything")));
    }

    #[test]
    fn id_selector_matches_id() {
        let sel = Selector::parse("#quit").unwrap();
        assert!(sel.matches(&ControlMeta::with_id("quit")));
        assert!(!sel.matches(&ControlMeta::with_id("bell")));
        assert!(!sel.matches(&ControlMeta::empty()));
    }

    #[test]
    fn class_selector_matches_class() {
        let sel = Selector::parse(".toggle").unwrap();
        assert!(sel.matches(&ControlMeta::empty().class("toggle")));
        assert!(!sel.matches(&ControlMeta::empty().class("other")));
    }

    #[test]
    fn compound_class_selector_requires_all_classes() {
        // Python `.toggle.dark` requires both classes.
        let sel = Selector::parse(".toggle.dark").unwrap();
        assert!(sel.matches(&ControlMeta::empty().classes(["toggle", "dark"])));
        assert!(!sel.matches(&ControlMeta::empty().class("toggle")));
        assert!(!sel.matches(&ControlMeta::empty().class("dark")));
    }

    #[test]
    fn type_selector_matches_type() {
        let sel = Selector::parse("Button").unwrap();
        assert!(sel.matches(&ControlMeta::empty().type_named("Button")));
        assert!(!sel.matches(&ControlMeta::empty().type_named("Input")));
    }

    #[test]
    fn compound_type_id_class() {
        let sel = Selector::parse("Button#save.primary").unwrap();
        let meta = ControlMeta::with_id("save")
            .type_named("Button")
            .class("primary");
        assert!(sel.matches(&meta));
        // Wrong id.
        let meta2 = ControlMeta::with_id("cancel")
            .type_named("Button")
            .class("primary");
        assert!(!sel.matches(&meta2));
    }

    #[test]
    fn comma_group_matches_any() {
        let sel = Selector::parse("#quit, #cancel").unwrap();
        assert!(sel.matches(&ControlMeta::with_id("quit")));
        assert!(sel.matches(&ControlMeta::with_id("cancel")));
        assert!(!sel.matches(&ControlMeta::with_id("ok")));
    }

    #[test]
    fn malformed_selectors_error() {
        assert!(Selector::parse("#").is_err());
        assert!(Selector::parse(".").is_err());
        assert!(Selector::parse("#a, ").is_err());
        assert!(Selector::parse("Button!").is_err());
    }

    // ── Router dispatch ───────────────────────────────────────────────

    struct State {
        bell: u32,
        toggled: u32,
        quit: u32,
        any_button: u32,
    }

    fn ev_button(id: Option<&str>) -> MessageEvent {
        MessageEvent::new(
            node_id_from_ffi(1),
            ButtonPressed {
                description: "x".into(),
                button_id: id.map(|s| s.to_string()),
            },
        )
    }

    #[test]
    fn router_routes_by_selector() {
        let mut router: MessageRouter<State> = MessageRouter::new();
        router.on::<ButtonPressed, _>("#bell", |s, _m, _c| s.bell += 1);
        router.on::<ButtonPressed, _>("#quit", |s, _m, _c| s.quit += 1);
        router.on_any::<ButtonPressed, _>(|s, _m, _c| s.any_button += 1);

        let mut state = State {
            bell: 0,
            toggled: 0,
            quit: 0,
            any_button: 0,
        };
        let mut ctx = EventCtx::default();

        router.dispatch(&mut state, &ev_button(Some("bell")), &mut ctx);
        assert_eq!(state.bell, 1);
        assert_eq!(state.quit, 0);
        // on_any matches every ButtonPressed.
        assert_eq!(state.any_button, 1);

        router.dispatch(&mut state, &ev_button(Some("quit")), &mut ctx);
        assert_eq!(state.bell, 1);
        assert_eq!(state.quit, 1);
        assert_eq!(state.any_button, 2);
    }

    #[test]
    fn router_ignores_non_matching_type() {
        let mut router: MessageRouter<State> = MessageRouter::new();
        router.on_any::<ButtonPressed, _>(|s, _m, _c| s.any_button += 1);

        let mut state = State {
            bell: 0,
            toggled: 0,
            quit: 0,
            any_button: 0,
        };
        let mut ctx = EventCtx::default();
        let ran = router.dispatch(
            &mut state,
            &MessageEvent::new(node_id_from_ffi(1), CheckboxChanged { checked: true }),
            &mut ctx,
        );
        assert!(!ran);
        assert_eq!(state.any_button, 0);
    }

    #[test]
    fn selector_route_skipped_when_no_control_meta() {
        // A message whose control_meta() is None must only fire universal routes.
        let mut router: MessageRouter<State> = MessageRouter::new();
        router.on::<CheckboxChanged, _>("#nope", |s, _m, _c| s.bell += 1);
        router.on_any::<CheckboxChanged, _>(|s, _m, _c| s.quit += 1);

        let mut state = State {
            bell: 0,
            toggled: 0,
            quit: 0,
            any_button: 0,
        };
        let mut ctx = EventCtx::default();
        router.dispatch(
            &mut state,
            &MessageEvent::new(node_id_from_ffi(1), CheckboxChanged { checked: true }),
            &mut ctx,
        );
        // CheckboxChanged has no control_meta -> selector route skipped, universal runs.
        assert_eq!(state.bell, 0);
        assert_eq!(state.quit, 1);
    }

    #[test]
    fn try_on_reports_parse_error() {
        let mut router: MessageRouter<State> = MessageRouter::new();
        let err = router
            .try_on::<ButtonPressed, _>("#", |_s, _m, _c| {})
            .map(|_| ())
            .unwrap_err();
        assert!(err.to_string().contains("invalid selector"));
    }

    #[test]
    fn button_pressed_exposes_control_meta() {
        let bp = ButtonPressed {
            description: "x".into(),
            button_id: Some("quit".into()),
        };
        let meta = bp.control_meta().expect("button exposes control meta");
        assert_eq!(meta.id.as_deref(), Some("quit"));
        assert_eq!(meta.type_name.as_deref(), Some("Button"));
    }
}
