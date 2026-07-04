//! `SelectCurrent` — the closed-state bar of a [`Select`](super::Select).
//!
//! Port of Python Textual's `SelectCurrent` (`textual/widgets/_select.py`): a
//! `Horizontal` that OWNS the `border: tall` chrome via its DEFAULT_CSS and
//! composes a `#label` static plus two `.arrow` statics (down / up). This Rust
//! port is a composed-children ARENA widget — its `#label` + arrow glyphs are
//! real child nodes, so the CSS cascade resolves on them directly:
//! `SelectCurrent Static#label`, `.arrow`, and the `Select.-expanded .down-arrow`
//! / `.up-arrow` display swap all match live nodes (no hand-rendered arrow, no
//! per-glyph style compensation). The framework's `render_styled` pipeline draws
//! the tall border + padding around the bar; the children composite over it.
//!
//! The `-has-value` class (which colours the label at full strength) is driven
//! onto this node by the parent [`Select`](super::Select) via
//! [`Widget::child_classes_for_tree`], and the focused border comes from the CSS
//! ancestor rule `Select:focus > SelectCurrent` — so `SelectCurrent` itself owns
//! no focus/value state.

use rich_rs::{Console, ConsoleOptions, Segments};
use textual_macros::widget;

use super::{NodeSeed, Widget};
use crate::compose::{ChildDecl, ComposeResult};
use crate::event::{Event, WidgetCtx};
use crate::message::SelectCurrentToggle;
use crate::widgets::Static;

/// The currently-selected option bar shown at the top of a
/// [`Select`](super::Select). Owns the tall border + padding chrome (via CSS
/// defaults) and composes the label + arrow glyphs as real child nodes.
#[widget(Focus, Interactive)]
pub(crate) struct SelectCurrent {
    /// Placeholder text shown when there is no current value.
    placeholder: String,
    /// The label of the current value, or `None` for the placeholder.
    label: Option<String>,
    seed: NodeSeed,
}

impl SelectCurrent {
    /// Build a `SelectCurrent` for the given placeholder + current label.
    pub(crate) fn new(placeholder: impl Into<String>, label: Option<String>) -> Self {
        Self {
            placeholder: placeholder.into(),
            label,
            seed: NodeSeed::default(),
        }
    }

    /// The text shown in the `#label` static (current label or placeholder).
    fn label_text(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.placeholder)
    }
}

impl crate::widgets::Focus for SelectCurrent {
    /// The bar is not focusable — the parent `Select` owns focus. Python
    /// `SelectCurrent.ALLOW_SELECT = False` and it has no `can_focus`.
    fn focusable(&self) -> bool {
        false
    }
}

impl crate::widgets::Interactive for SelectCurrent {
    /// Clicking the bar asks the ancestor `Select` to toggle the overlay
    /// (Python `SelectCurrent._on_click` → `post_message(self.Toggle())`).
    fn on_event(&mut self, event: &Event, ctx: &mut WidgetCtx) {
        if let Event::MouseDown(mouse) = event {
            if mouse.target == self.node_id() {
                ctx.post_message(SelectCurrentToggle);
                ctx.set_handled();
            }
        }
    }
}

impl crate::widgets::Render for SelectCurrent {
    /// Compose the label + down/up arrow glyphs as real child nodes (Python
    /// `SelectCurrent.compose`). State-pure: rebuilt identically from
    /// `placeholder`/`label` every call, so an ancestor recompose (the Select
    /// label update on selection) regenerates rather than clears.
    fn compose(&mut self) -> ComposeResult {
        vec![
            ChildDecl::new(Box::new(Static::new(self.label_text().to_string()).without_markup()))
                .with_id("label"),
            ChildDecl::new(Box::new(Static::new("▼").without_markup()))
                .with_classes(&["arrow", "down-arrow"]),
            ChildDecl::new(Box::new(Static::new("▲").without_markup()))
                .with_classes(&["arrow", "up-arrow"]),
        ]
    }

    /// Chrome-only: the framework draws the border/bg via `render_styled`; the
    /// composed children (label + arrows) render themselves and composite over
    /// this node's surface.
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn style_type(&self) -> &'static str {
        "SelectCurrent"
    }
}
