//! `SelectCurrent` — the closed-state bar of a [`Select`](super::Select).
//!
//! Port of Python Textual's `SelectCurrent` (`textual/widgets/_select.py`).
//!
//! In Python, `SelectCurrent` is a `Horizontal` that OWNS the `border: tall`
//! chrome via its `DEFAULT_CSS`, composing a `#label` static and `.arrow` static.
//! This Rust port mirrors that ownership: `SelectCurrent` is a first-class widget
//! whose `style_type()` is `"SelectCurrent"`, so the shared
//! `SelectCurrent { border: tall ...; padding: 0 2; ... }` defaults (see
//! `css/defaults/select.rs`) apply automatically. The framework's `render_styled`
//! pipeline draws the tall border + padding around the bar content — no border is
//! hand-drawn here.
//!
//! [`Select`](super::Select) builds and renders this widget for its closed state
//! and delegates `layout_height` / `content_width` to it, so a closed `Select` is
//! a proper 3-row tall-bordered box (border top/bottom + 1 content row). It is
//! rendered internally (not mounted as an arena child), so it carries no
//! `NodeSeed`; its CSS is resolved off-tree via `style_type()` + `style_classes()`.

use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use super::Widget;
use super::helpers::adjust_line_length_no_bg;

/// The currently-selected option bar shown when a [`Select`](super::Select) is
/// closed. Owns the tall border + padding chrome (via CSS defaults).
pub(crate) struct SelectCurrent {
    /// Placeholder text shown when there is no current value.
    placeholder: String,
    /// The label of the current value, or `None` for the placeholder.
    label: Option<String>,
    /// Whether the parent `Select` currently has a value selected.
    has_value: bool,
    /// Whether the parent `Select` is focused (drives the focused border).
    focused: bool,
    /// Whether the dropdown is expanded (swaps the down/up arrow glyph).
    expanded: bool,
    /// CSS classes computed for off-tree style resolution (`style_classes`).
    classes: Vec<String>,
}

impl SelectCurrent {
    pub(crate) fn new(placeholder: impl Into<String>) -> Self {
        let mut this = Self {
            placeholder: placeholder.into(),
            label: None,
            has_value: false,
            focused: false,
            expanded: false,
            classes: Vec::new(),
        };
        this.recompute_classes();
        this
    }

    /// Update the displayed label. `None` shows the placeholder.
    pub(crate) fn with_label(mut self, label: Option<String>) -> Self {
        self.has_value = label.is_some();
        self.label = label;
        self.recompute_classes();
        self
    }

    pub(crate) fn with_focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self.recompute_classes();
        self
    }

    pub(crate) fn with_expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    /// Mirror Python's `-has-value`/focus class toggling for off-tree style
    /// resolution (`style_classes` is consulted by `selector_meta_generic`).
    fn recompute_classes(&mut self) {
        self.classes.clear();
        if self.has_value {
            self.classes.push("-has-value".to_string());
        }
        if self.focused {
            // `Select:focus > SelectCurrent` raises the border in Python; since
            // this widget is rendered internally (not a DOM child of Select),
            // we mirror that with an explicit `-focus` class + matching rule.
            self.classes.push("-focus".to_string());
        }
    }

    /// The text shown in the bar (current label or placeholder).
    fn bar_text(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.placeholder)
    }
}

impl Widget for SelectCurrent {
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        // `options.size` here is the CONTENT box (border + padding already
        // subtracted by `render_widget_with_meta`). Compose: label (1fr, left) +
        // arrow (right), matching Python's `#label` + `.arrow`.
        let width = options.size.0.max(1);

        // Resolve component styles so colors track the `.label` / `.arrow` CSS
        // rules. `-has-value` lives on the SelectCurrent element itself (in
        // `style_classes`), so `SelectCurrent.-has-value .label` resolves via meta.
        let label_style = crate::css::resolve_component_style(self, &["label"])
            .to_rich()
            .unwrap_or_default();
        let arrow_style = crate::css::resolve_component_style(self, &["arrow"])
            .to_rich()
            .unwrap_or(label_style);

        let arrow = if self.expanded { "▲" } else { "▼" };
        // Arrow occupies 1 glyph + 1 cell of left padding (Python `.arrow`
        // `padding: 0 0 0 1`). Reserve 2 cells for it.
        let arrow_width = 2usize;
        let label_width = width.saturating_sub(arrow_width).max(1);

        let label_seg = Segment::styled(
            rich_rs::set_cell_size(self.bar_text(), label_width),
            label_style,
        );
        let arrow_seg = Segment::styled(format!(" {arrow}"), arrow_style);

        let line = adjust_line_length_no_bg(&[label_seg, arrow_seg], width);
        let mut out = Segments::new();
        out.extend(line);
        out
    }

    fn style_type(&self) -> &'static str {
        "SelectCurrent"
    }

    fn style_classes(&self) -> &[String] {
        &self.classes
    }

    fn layout_height(&self) -> Option<usize> {
        // OUTER auto height = 1 content row + own border/padding vertical chrome
        // (the layout side adds only margin on top — see `extract_child_spec`).
        let meta = crate::css::selector_meta_generic(self);
        let resolved = crate::css::resolve_style(self, &meta);
        let padding = resolved.effective_padding();
        let (border_top, border_bottom, _, _) =
            super::helpers::border_spacing_from_style(&resolved);
        let chrome_v =
            usize::from(padding.top.saturating_add(padding.bottom)) + border_top + border_bottom;
        Some(1usize.saturating_add(chrome_v))
    }

    fn content_width(&self) -> Option<usize> {
        let text_width = rich_rs::cell_len(self.bar_text());
        let meta = crate::css::selector_meta_generic(self);
        let resolved = crate::css::resolve_style(self, &meta);
        let padding = resolved.effective_padding();
        let (_, _, border_left, border_right) =
            super::helpers::border_spacing_from_style(&resolved);
        let chrome_lr =
            usize::from(padding.left.saturating_add(padding.right)) + border_left + border_right;
        // text + 2 cells for the arrow (space + glyph) + own chrome.
        Some(
            text_width
                .saturating_add(2)
                .saturating_add(chrome_lr)
                .max(1),
        )
    }
}

impl Renderable for SelectCurrent {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}
