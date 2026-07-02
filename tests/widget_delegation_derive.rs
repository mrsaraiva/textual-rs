//! Acceptance tests for the first-class `#[widget(base = ..)]` delegation derive.
//!
//! The derive is the "inheritance substitute": a compound widget declares a
//! `base` field of a container type and gets the whole structural / propagation
//! `Widget` surface forwarded for free — no hand-written 63-method `impl Widget`.
//!
//! These tests prove the four things the derive must do:
//!  1. forward `take_composed_children` (children reach the arena),
//!  2. give the compound its OWN CSS `style_type` (not the base's),
//!  3. compose with `#[derive(Reactive)]` (`reactive` opt-in exposes `self`),
//!  4. let a user override one forwarded method via an inherent method
//!     (`override(..)`), while everything else still forwards,
//!  5. propagate a real click to a composed child end-to-end (via `Pilot`).

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use textual::prelude::*;
use textual::reactive::ReactiveCtx;
use textual::runtime::Pilot;
use textual::widgets::Widget;

// ─────────────────────────────────────────────────────────────────────────
// StatCard — the flagship: base = VerticalGroup, custom style_type, reactive.
// The ENTIRE hand-written boilerplate this replaces is a ~40-method `impl
// Widget` + `impl Renderable` (see tests/containment_pattern.rs `OuterWidget`).
// ─────────────────────────────────────────────────────────────────────────

#[textual::widget(base = VerticalGroup, style_type = "StatCard", reactive)]
#[derive(textual::Reactive)]
struct StatCard {
    base: VerticalGroup,
    #[reactive]
    count: i32,
}

impl StatCard {
    fn new() -> Self {
        Self {
            base: VerticalGroup::new()
                .with_child(Static::new("Total").id("label"))
                .with_child(Button::new("Go").id("go")),
            count: 0,
        }
    }
}

// A plain compound with no `style_type`/`reactive` options: proves the default
// `style_type` is the compound's OWN concrete type name, and that `focusable`
// forwards to the base (VerticalGroup -> false).
#[textual::widget(base = VerticalGroup)]
struct PlainCard {
    base: VerticalGroup,
}

impl PlainCard {
    fn new() -> Self {
        Self {
            base: VerticalGroup::new()
                .with_child(Static::new("a").id("a"))
                .with_child(Static::new("b").id("b")),
        }
    }
}

// Overrides ONE forwarded method (`focusable`) via an inherent method, while
// every other method still forwards to the base. Proves the override mechanism
// resolves to the inherent method (inherent wins over the trait method — no
// recursion).
#[textual::widget(base = VerticalGroup, override(focusable))]
struct FocusableCard {
    base: VerticalGroup,
}

impl FocusableCard {
    fn new() -> Self {
        Self {
            base: VerticalGroup::new().with_child(Static::new("x").id("x")),
        }
    }

    // MUST match `Widget::focusable`'s signature exactly.
    fn focusable(&self) -> bool {
        true
    }
}

fn make_ctx() -> ReactiveCtx {
    use slotmap::SlotMap;
    let mut sm: SlotMap<textual::NodeId, ()> = SlotMap::new();
    let id = sm.insert(());
    ReactiveCtx::new(id)
}

// ── (2) own CSS identity ────────────────────────────────────────────────

#[test]
fn derive_sets_custom_style_type() {
    let card = StatCard::new();
    assert_eq!(
        card.style_type(),
        "StatCard",
        "style_type = \"StatCard\" attr must set the CSS type"
    );
}

#[test]
fn derive_default_style_type_is_own_type_name() {
    let card = PlainCard::new();
    assert_eq!(
        card.style_type(),
        "PlainCard",
        "without a style_type attr the compound keeps its OWN type name, not the base's (Vertical)"
    );
}

// ── (1) take_composed_children forwards ─────────────────────────────────

#[test]
fn derive_forwards_take_composed_children() {
    let mut card = StatCard::new();
    let children = card.take_composed_children();
    assert_eq!(
        children.len(),
        2,
        "the two children built into `base` must surface through the forwarded take_composed_children"
    );
    // Idempotent: base drained, second call is empty.
    assert_eq!(card.take_composed_children().len(), 0);
}

// ── (4) override vs forward ─────────────────────────────────────────────

#[test]
fn derive_forwards_focusable_to_base_by_default() {
    // VerticalGroup (a non-scrolling container) is not itself focusable.
    assert!(
        !PlainCard::new().focusable(),
        "focusable must forward to the base (VerticalGroup -> false)"
    );
}

#[test]
fn derive_override_calls_inherent_method() {
    // override(focusable) routes the generated trait method to the inherent
    // `FocusableCard::focusable`, which returns true — proving no recursion and
    // that the override wins over the forwarded default.
    assert!(
        FocusableCard::new().focusable(),
        "override(focusable) must call the user's inherent method (true)"
    );
    // A non-overridden method on the same widget still forwards.
    assert_eq!(FocusableCard::new().take_composed_children().len(), 1);
}

// ── (3) composability with #[derive(Reactive)] ──────────────────────────

#[test]
fn derive_composes_with_reactive() {
    let mut card = StatCard::new();
    let mut ctx = make_ctx();

    // Generated reactive getter/setter work on the same struct.
    assert_eq!(*card.count(), 0);
    card.set_count(5, &mut ctx);
    assert_eq!(*card.count(), 5);
    assert_eq!(ctx.changes().len(), 1, "reactive setter must record a change");

    // The `reactive` opt-in routes reactive_widget to SELF (so the compound's
    // own reactive fields are reachable), not to the base.
    assert!(
        card.reactive_widget().is_some(),
        "reactive flag must expose the compound as its own reactive surface"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// (5) End-to-end: a real click on a child Button composed INSIDE the derived
// compound must reach it and bubble a ButtonPressed. This exercises the
// forwarded render (child gets layout/area to hit-test) + take_composed_children
// (child reaches the arena) through the derive.
// ─────────────────────────────────────────────────────────────────────────

struct CardApp {
    presses: Arc<AtomicU32>,
}

const CARD_CSS: &str = r#"
Screen { align: center middle; }
StatCard { width: auto; height: auto; }
"#;

impl TextualApp for CardApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CARD_CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(StatCard::new())
    }

    fn on_message_with_app(&mut self, _app: &mut App, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Some(bp) = message.downcast_ref::<ButtonPressed>() {
            if bp.button_id.as_deref() == Some("go") {
                self.presses.fetch_add(1, Ordering::SeqCst);
                ctx.set_handled();
            }
        }
    }
}

#[test]
fn derive_propagates_click_to_composed_child() {
    let presses = Arc::new(AtomicU32::new(0));
    let app = CardApp {
        presses: Arc::clone(&presses),
    };

    textual::run_test(app, |pilot: &mut Pilot| {
        pilot.click("#go")?;
        pilot.click("#go")?;
        Ok(())
    })
    .expect("headless run_test must succeed");

    assert_eq!(
        presses.load(Ordering::SeqCst),
        2,
        "clicking the Button composed inside the #[widget(base=..)] compound must \
         reach it and bubble ButtonPressed (proves render + take_composed_children forwarding)"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// (6) #[on(..)] runtime wiring: a composed widget handles its OWN child's
// ButtonPressed via #[on(ButtonPressed)] with NO hand-written on_message.
// `#[widget(.., on(on_button))]` generates an on_message that materializes a
// WidgetCtx over the real bubble EventCtx, calls `__on_dispatch_on_button`, and
// forwards to the base. Proves the sub-step-3 macro glue end-to-end via Pilot.
// ─────────────────────────────────────────────────────────────────────────

#[textual::widget(base = VerticalGroup, on(on_button))]
struct ClickCard {
    base: VerticalGroup,
    presses: Arc<AtomicU32>,
}

impl ClickCard {
    fn new(presses: Arc<AtomicU32>) -> Self {
        Self {
            base: VerticalGroup::new().with_child(Button::new("Go").id("go")),
            presses,
        }
    }

    #[textual::on(ButtonPressed)]
    fn on_button(&mut self, event: &ButtonPressed, ctx: &mut WidgetCtx) {
        if event.button_id.as_deref() == Some("go") {
            self.presses.fetch_add(1, Ordering::SeqCst);
            ctx.set_handled();
        }
    }
}

const CLICK_CARD_CSS: &str = r#"
Screen { align: center middle; }
ClickCard { width: auto; height: auto; }
"#;

struct ClickCardApp {
    presses: Arc<AtomicU32>,
}

impl TextualApp for ClickCardApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CLICK_CARD_CSS);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(ClickCard::new(Arc::clone(&self.presses)))
    }
    // NB: NO on_message / on_message_with_app — ClickCard handles ButtonPressed
    // itself via #[on(ButtonPressed)] + the generated on_message glue.
}

#[test]
fn on_handler_widget_receives_child_button_pressed_without_hand_written_on_message() {
    let presses = Arc::new(AtomicU32::new(0));
    let app = ClickCardApp {
        presses: Arc::clone(&presses),
    };

    textual::run_test(app, |pilot: &mut Pilot| {
        pilot.click("#go")?;
        pilot.click("#go")?;
        Ok(())
    })
    .expect("headless run_test must succeed");

    assert_eq!(
        presses.load(Ordering::SeqCst),
        2,
        "#[on(ButtonPressed)] on the compound (wired via #[widget(.., on(on_button))]) must \
         receive the child Button's ButtonPressed with NO hand-written on_message"
    );
}
