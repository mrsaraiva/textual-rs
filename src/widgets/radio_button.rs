use rich_rs::{Console, ConsoleOptions, Segment, Segments};
use textual_macros::widget;

use crate::event::Event;
use crate::message::*;

use super::{NodeSeed, Widget, option_list::toggle_option::BinaryToggleState};

/// A radio button widget that represents a boolean on/off value.
///
/// RadioButton is very similar to Checkbox but uses radio semantics:
/// - Circle glyph (`●` / `○`) instead of checkbox marks
/// - Typically used inside a `RadioSet` for mutual exclusion
///
/// On its own a RadioButton can be toggled freely. When placed inside a
/// `RadioSet`, the set enforces that only one button is selected at a time.
#[derive(Debug, Clone)]
#[widget(Focus, Interactive, Layout, Components)]
pub struct RadioButton {
    label: String,
    state: BinaryToggleState,
    /// Ordinal within the owning `RadioSet` (set at compose time). Reported in
    /// the `RadioButtonChanged` message so the set can route the change without
    /// owning this child's `NodeId`.
    ordinal: usize,
    seed: NodeSeed,
}

impl RadioButton {
    crate::seed_ident_methods!();

    pub fn new(label: impl Into<String>) -> Self {
        let label = label.into();
        let seed = NodeSeed {
            classes: vec!["radio-button".to_string(), "-off".to_string()],
            ..NodeSeed::default()
        };
        Self {
            label,
            state: BinaryToggleState::new(false),
            ordinal: 0,
            seed,
        }
    }

    /// Set the ordinal within the owning `RadioSet`. Called by `RadioSet` at
    /// compose time (mirrors `ListItem::set_ordinal`).
    pub(crate) fn set_ordinal(&mut self, ordinal: usize) {
        self.ordinal = ordinal;
    }

    /// Create a radio button with an initial value.
    pub fn with_value(mut self, value: bool) -> Self {
        self.state.set_value(value);
        self.rebuild_classes();
        self
    }

    /// Builder method to set the disabled state.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.state.set_disabled(disabled);
        self
    }

    /// Returns the current value (`true` = selected).
    pub fn value(&self) -> bool {
        self.state.value()
    }

    /// Returns the label text.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns `true` if this button is disabled.
    pub fn is_disabled(&self) -> bool {
        self.state.disabled()
    }

    /// Set the value without emitting a message.
    ///
    /// This is used by `RadioSet` to programmatically deselect buttons
    /// without triggering further change events.
    pub fn set_value_silent(&mut self, value: bool) {
        self.state.set_value(value);
        self.rebuild_classes();
    }

    /// Toggle the value and emit a `RadioButtonChanged` message.
    pub fn toggle(&mut self, ctx: &mut crate::event::WidgetCtx) {
        if self.state.disabled() {
            return;
        }
        self.state.toggle();
        self.on_toggled();
        self.emit_changed(ctx);
        ctx.request_repaint();
        ctx.set_handled();
    }

    fn emit_changed(&self, ctx: &mut crate::event::WidgetCtx) {
        ctx.post_message(RadioButtonChanged {
            value: self.state.value(),
            ordinal: self.ordinal,
        });
    }

    fn on_toggled(&mut self) {
        self.rebuild_classes();
    }

    fn rebuild_classes(&mut self) {
        let on_off = if self.state.value() { "-on" } else { "-off" };
        self.seed.classes = vec!["radio-button".to_string(), on_off.to_string()];
    }
}

impl crate::widgets::Focus for RadioButton {
    fn focusable(&self) -> bool {
        self.state.focusable()
    }

    fn is_active(&self) -> bool {
        self.state.is_active()
    }
}

impl crate::widgets::Interactive for RadioButton {
    fn on_node_state_changed(
        &mut self,
        _old: crate::widgets::NodeState,
        new: crate::widgets::NodeState,
    ) {
        self.state.set_focused(new.focused);
        self.state.set_hovered(new.hovered);
    }

    fn on_event(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
        let outcome = self.state.handle_event(event, self.node_id());
        if outcome.toggled {
            self.on_toggled();
            self.emit_changed(ctx);
        }
        if outcome.repaint {
            ctx.request_repaint();
        }
        if outcome.handled {
            ctx.set_handled();
        }
    }
}

impl crate::widgets::Layout for RadioButton {
    fn content_width(&self) -> Option<usize> {
        let meta = crate::css::selector_meta_generic(self);
        let resolved = crate::css::resolve_style(self, &meta);
        let padding = resolved.effective_padding();
        let (_, _, border_left, border_right) =
            super::helpers::border_spacing_from_style(&resolved);
        let chrome_lr =
            usize::from(padding.left.saturating_add(padding.right)) + border_left + border_right;
        // Rendered content is "▐●▌ " + label. Markup tags are stripped first so
        // `[bold italic red]…[/]` doesn't inflate the width (Python measures the
        // parsed `Content`, not the raw markup).
        let label_width = crate::content::Content::from_markup(&self.label).cell_length();
        let content = label_width.saturating_add(4);
        Some(content.saturating_add(chrome_lr).max(1))
    }

    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }
}

impl crate::widgets::Render for RadioButton {
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);

        // Python's `ToggleButton` always renders the inner glyph (`RadioButton`
        // overrides `BUTTON_INNER = "●"`); the on/off state is conveyed by the
        // glyph *colour* (`.-on > .toggle--button`), not by swapping to `○`.
        let glyph = "●";

        // Resolve the `toggle--button` / `toggle--label` component styles
        // through the canonical API. As a real arena node, this node's own live
        // meta — including the `-on` / `-selected` classes the owning `RadioSet`
        // drove on via `child_classes_for_tree`, plus any `RadioSet:focus` /
        // `:blur` ancestor — is ALREADY the top of the selector stack, and
        // `resolve_component_style` resolves the typeless phantom directly
        // against that live context, so
        // `RadioSet:focus > RadioButton.-selected > .toggle--label` matches.
        let button_style = crate::css::resolve_component_style(self, &["toggle--button"]);
        let button_rich = button_style.to_rich().unwrap_or_else(rich_rs::Style::new);
        let label_style = crate::css::resolve_component_style(self, &["toggle--label"]);

        // Flatten a (possibly semi-transparent) selected-label background over
        // the composited ancestor surface — the same `background_colors`
        // compositing Python performs. `current_composited_background()` here is
        // the `RadioSet` surface (including its `:focus` `background-tint`).
        let surface_bg = crate::css::current_composited_background();
        let mut label_effective = label_style.clone();
        if let (Some(bg), Some(surf)) = (label_style.bg, surface_bg) {
            label_effective.bg = Some(bg.flatten_over(surf));
        } else if let Some(bg) = label_style.bg {
            // No composited surface known: flatten over the theme background,
            // matching the previous `Style::to_rich()` conversion.
            if bg.a > 0.0 && bg.a < 1.0 {
                let under = crate::style::parse_color_like("$background")
                    .unwrap_or(crate::style::Color::rgb(0, 0, 0));
                label_effective.bg = Some(bg.flatten_over(under));
            } else if bg.a <= 0.0 {
                label_effective.bg = None;
            }
        }

        // The side half-blocks take the button's *background* as their
        // foreground (Python `side_style.foreground = button_style.background`)
        // and leave their own background transparent so the surface composites
        // through, matching Python's `background_colors[1]`.
        let panel_bg = crate::style::parse_color_like("$panel")
            .unwrap_or(crate::style::Color::rgb(0, 0, 0))
            .to_simple_opaque();
        let side_fg = button_rich.bgcolor.unwrap_or(panel_bg);
        let side_style = rich_rs::Style::new().with_color(side_fg);

        // Label: markup-parsed, padded (1, 1), with the `toggle--label` style
        // layered UNDER the markup spans — Python's
        // `self._label.pad(1, 1).stylize_before(label_style)` — so
        // `[bold italic red]…[/]` colourises like any `Static` label.
        let label_content =
            super::helpers::toggle_label_content(&self.label, label_effective);
        let mut segments = vec![
            Segment::styled("▐".to_string(), side_style),
            Segment::styled(glyph.to_string(), button_rich),
            Segment::styled("▌".to_string(), side_style),
        ];
        segments.extend(label_content.render_label_segments(
            &crate::style::Style::new(),
            super::helpers::markup_tag_resolve,
        ));

        // Pad/crop to width.
        let line = super::helpers::adjust_line_length_no_bg(&segments, width);
        let mut out = Segments::new();
        out.extend(line);
        out
    }

    fn style_type(&self) -> &'static str {
        "RadioButton"
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventCtx;
    use crate::keys::KeyEventData;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn radio_button_toggle_emits_message() {
        let mut button = RadioButton::new("A");
        // BinaryToggleState uses its own focused field for keyboard routing.
        button.on_node_state_changed(
            crate::widgets::NodeState::default(),
            crate::widgets::NodeState {
                focused: true,
                ..Default::default()
            },
        );
        let mut ctx = EventCtx::default();
        let key =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            button.on_event(&Event::Key(key), &mut __w);
        }
        assert!(button.value());
        let messages = ctx.take_messages();
        assert!(messages.iter().any(|m| {
            m.downcast_ref::<RadioButtonChanged>()
                .is_some_and(|r| r.value)
        }));
    }

    #[test]
    fn radio_button_disabled_is_not_focusable() {
        let button = RadioButton::new("A").disabled(true);
        assert!(!button.focusable());
    }
}

impl crate::widgets::Components for RadioButton {
    fn component_classes(&self) -> &[&'static str] {
        &[
            "toggle--button",
            "toggle--label",
        ]
    }
}
