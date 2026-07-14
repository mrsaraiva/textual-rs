//! `SelectOverlay` — the pop-up option list of a [`Select`](super::Select).
//!
//! Port of Python Textual's `SelectOverlay(OptionList)`
//! (`textual/widgets/_select.py`). It is a composed child of `Select` that
//! resolves `overlay: screen; display: block` (when the Select is `-expanded`),
//! so it floats UNCLIPPED at the top z below its sibling `SelectCurrent` bar via
//! the Mechanism-A deferred-overlay paint (RA2.4). The whole structural /
//! rendering / navigation `Widget` surface is DELEGATED to the inner
//! [`OptionList`] via `#[widget(base = OptionList)]`; this file supplies only the
//! Select-specific behaviour:
//!
//! - `style_type = "SelectOverlay"` (its own CSS identity) with an `OptionList`
//!   style alias so the base `OptionList` rules (`:focus` border, highlighted-row
//!   colours) still resolve on this node.
//! - `on_event`: Escape → dismiss (keep Select focused); a lost-focus `Blur` →
//!   dismiss (click outside); printable keys → type-to-search; everything else
//!   forwards to the inner `OptionList` (mouse). Navigation / Enter ride the
//!   inner `OptionList`'s declarative BINDINGS (delegated `bindings()` /
//!   `execute_action()`), exactly like Python's `SelectOverlay(OptionList)`
//!   inheriting `OptionList.BINDINGS`.

use rich_rs::Text;

use super::option_list::OptionItem;
use super::option_list::OptionList;
use super::Widget;
use crate::event::{Event, WidgetCtx};
use crate::message::SelectOverlayDismiss;
use crossterm::event::KeyCode;

/// The pop-up option list for a [`Select`](super::Select). Wraps an
/// [`OptionList`]; the `Select` builds it (blank row + option rows) and assigns
/// its stable CSS id at compose time.
#[textual::widget(base = OptionList, field = inner, style_type = "SelectOverlay",
    override(on_event, style_type_aliases, layout_height))]
pub(crate) struct SelectOverlay {
    inner: OptionList,
    /// Accumulated type-to-search query (reset by the parent on open).
    search: String,
}

impl SelectOverlay {
    /// Build the overlay from pre-constructed option rows (built by `Select`,
    /// including the leading dim blank/prompt row when `allow_blank`), with the
    /// current-value row highlighted.
    pub(crate) fn new(items: Vec<OptionItem>, highlighted: Option<usize>) -> Self {
        let mut inner = OptionList::with_items(items);
        // Python `Select > SelectOverlay > .option-list--option { padding: 0 1 }`:
        // one cell of per-option inset (inside the option background), consistently
        // applied to both the render indent and the wrap-width measurement.
        inner.set_option_pad_left(1);
        match highlighted {
            Some(index) => inner.set_highlighted(index),
            None => inner.clear_highlighted(),
        }
        Self {
            inner,
            search: String::new(),
        }
    }

    /// Also match `OptionList` in CSS so the base option-list rules (focused
    /// border + highlighted-row colours) resolve on this node, alongside the
    /// `SelectOverlay` / `Select > SelectOverlay` rules.
    fn style_type_aliases(&self) -> &[&'static str] {
        &["OptionList"]
    }

    /// PURE content height (the inner [`OptionList`] reports content lines only).
    /// The `border: tall` chrome (2 rows) is added by the flow layout's height
    /// arm (`full_v_chrome`, the height-chrome keystone) with full ancestor CSS
    /// context — NOT manually here. A manual `+ 2` was needed BEFORE the keystone
    /// (when the auto-HEIGHT edge added no chrome); post-keystone it double-counts
    /// and pushes the overlay 2 rows too tall (regressed `select_open_overlay` /
    /// `select_from_values_open` until removed).
    fn layout_height(&self) -> Option<usize> {
        self.inner.layout_height()
    }

    /// A simple case-insensitive substring search that favours options whose
    /// prompt contains the query earliest (Python `_find_search_match`).
    fn find_search_match(&self, query: &str) -> Option<usize> {
        let query = query.to_lowercase();
        let mut best: Option<usize> = None;
        let mut best_pos: Option<usize> = None;
        for index in 0..self.inner.option_count() {
            let Some(item) = self.inner.get_option(index) else {
                continue;
            };
            let Some(prompt) = item.prompt() else {
                continue;
            };
            if let Some(pos) = prompt.to_lowercase().find(&query) {
                if best_pos.is_none_or(|b| pos < b) {
                    best = Some(index);
                    best_pos = Some(pos);
                }
            }
        }
        best
    }

    fn on_event(&mut self, event: &Event, ctx: &mut WidgetCtx) {
        match event {
            Event::Key(key) => match key.code {
                KeyCode::Esc => {
                    // Dismiss but keep the Select focused (Python action_dismiss).
                    ctx.post_message(SelectOverlayDismiss { lost_focus: false });
                    ctx.set_handled();
                    return;
                }
                KeyCode::Char(ch) if !ch.is_control() => {
                    // Type-to-search: jump to the first matching option
                    // (Python SelectOverlay._on_key).
                    self.search.push(ch);
                    let query = std::mem::take(&mut self.search);
                    if let Some(index) = self.find_search_match(&query) {
                        self.inner.set_highlighted(index);
                    }
                    self.search = query;
                    ctx.request_repaint();
                    ctx.set_handled();
                    return;
                }
                _ => {}
            },
            Event::Blur(_) => {
                // Lost focus (e.g. a click outside) — dismiss without re-focusing
                // the Select (Python SelectOverlay._on_blur).
                ctx.post_message(SelectOverlayDismiss { lost_focus: true });
                // Fall through so the inner list also observes the blur.
            }
            _ => {}
        }
        self.inner.on_event(event, ctx);
    }
}

impl SelectOverlay {
    /// Build a dim blank/prompt option row (Python `Option(Text(prompt, "dim"))`).
    pub(crate) fn blank_option(prompt: &str) -> OptionItem {
        OptionItem::rich(
            prompt.to_string(),
            Text::styled(prompt, rich_rs::Style::new().with_dim(true)),
        )
    }
}
