use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments, Text};

use crate::debug::debug_message;
use crate::event::{Event, EventCtx};
use crate::message::*;

use super::helpers::{adjust_line_length_no_bg, empty_classes, fixed_height_from_constraints};
use super::{Widget, WidgetStyles};
use crate::reactive::{ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FooterBinding {
    pub key: String,
    pub description: String,
    pub group: Option<String>,
    /// Raw key spec from the binding hint (e.g. "ctrl+p"), used for
    /// click-to-invoke dispatch. Distinct from `key` which may be a
    /// display-formatted version (e.g. "^p").
    pub action_key: Option<String>,
}

impl FooterBinding {
    pub fn new(key: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            description: description.into(),
            group: None,
            action_key: None,
        }
    }

    pub fn with_group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }

    pub fn with_action_key(mut self, action_key: impl Into<String>) -> Self {
        self.action_key = Some(action_key.into());
        self
    }
}

#[derive(Debug, Clone)]
pub struct Footer {
    bindings: Vec<FooterBinding>,
    compact: bool,
    app_focused: bool,
    deferred_bindings: Option<Vec<FooterBinding>>,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl Footer {
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
            compact: false,
            app_focused: true,
            deferred_bindings: None,
            classes: Vec::new(),
            styles: WidgetStyles::default(),
        }
    }

    pub fn with_binding(mut self, key: impl Into<String>, description: impl Into<String>) -> Self {
        self.bindings.push(FooterBinding::new(key, description));
        self
    }

    pub fn set_bindings(&mut self, bindings: Vec<FooterBinding>) {
        self.bindings = bindings;
    }

    pub fn clear_bindings(&mut self) {
        self.bindings.clear();
    }

    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    // ── Reactive getters ─────────────────────────────────────────────────

    /// Reactive getter for `compact`.
    pub fn is_compact(&self) -> bool {
        self.compact
    }

    // ── Reactive setters ─────────────────────────────────────────────────

    /// Reactive setter for `compact`. Records the change in the provided
    /// [`ReactiveCtx`] and triggers layout invalidation.
    pub fn set_compact(&mut self, value: bool, ctx: &mut ReactiveCtx) {
        if self.compact != value {
            let old = self.compact;
            self.compact = value;
            ctx.record_change(
                "compact",
                ReactiveFlags::reactive_layout(),
                Box::new(old),
                Box::new(value),
            );
        }
    }

    // ── Watchers ─────────────────────────────────────────────────────────

    fn watch_compact(&mut self, _old: &bool, _new: &bool, _ctx: &mut ReactiveCtx) {
        // Layout invalidation is handled by ReactiveFlags::reactive_layout().
    }

    fn component_style(&self, classes: &[&str], fallback: rich_rs::Style) -> rich_rs::Style {
        let style = crate::css::resolve_component_style(self, classes);
        if style.is_empty() {
            fallback
        } else {
            style.to_rich().unwrap_or(fallback)
        }
    }

    fn base_style(&self) -> rich_rs::Style {
        self.component_style(&["footer"], rich_rs::Style::new())
    }

    fn key_style(&self) -> rich_rs::Style {
        self.component_style(&["footer-key--key"], self.base_style().with_bold(true))
    }

    fn description_style(&self) -> rich_rs::Style {
        self.component_style(&["footer-key--description"], self.base_style())
    }

    fn command_palette_style(&self) -> rich_rs::Style {
        self.component_style(&["footer-key--command-palette"], self.description_style())
    }

    fn palette_separator_style(&self) -> rich_rs::Style {
        self.component_style(&["footer-key--palette-separator"], self.base_style())
    }

    fn render_binding(
        &self,
        binding: &FooterBinding,
        key_style: rich_rs::Style,
        description_style: rich_rs::Style,
    ) -> Vec<Segment> {
        let mut out = Vec::new();
        let key_text = if self.compact {
            binding.key.clone()
        } else {
            format!(" {}", binding.key)
        };
        out.push(Segment::styled(key_text, key_style));
        if binding.description.is_empty() {
            if !self.compact {
                out.push(Segment::styled(" ".to_string(), description_style));
            }
        } else {
            out.push(Segment::styled(
                format!(" {}", binding.description),
                description_style,
            ));
        }
        out
    }

    fn render_group(
        &self,
        group_label: &str,
        group_bindings: &[FooterBinding],
        key_style: rich_rs::Style,
        description_style: rich_rs::Style,
        base_style: rich_rs::Style,
    ) -> Vec<Segment> {
        let mut out = Vec::new();
        let key_separator = if self.compact { " " } else { "  " };
        for (index, binding) in group_bindings.iter().enumerate() {
            if index > 0 {
                out.push(Segment::styled(key_separator.to_string(), base_style));
            }
            let mut key_only = binding.clone();
            key_only.description.clear();
            out.extend(self.render_binding(&key_only, key_style, description_style));
        }
        out.push(Segment::styled(
            format!(" {}", group_label),
            description_style,
        ));
        out
    }

    fn split_bindings(&self) -> (Vec<LeftBindingItem>, Option<FooterBinding>) {
        let mut left_bindings = Vec::new();
        let mut palette = None::<FooterBinding>;
        for binding in &self.bindings {
            if binding.group.as_deref() == Some("command_palette") {
                palette = Some(binding.clone());
            } else {
                left_bindings.push(binding.clone());
            }
        }

        let mut left_items = Vec::new();
        let mut index = 0;
        while index < left_bindings.len() {
            let binding = &left_bindings[index];
            let Some(group_name) = binding.group.clone() else {
                left_items.push(LeftBindingItem::Single(binding.clone()));
                index += 1;
                continue;
            };

            let mut run_end = index + 1;
            while run_end < left_bindings.len()
                && left_bindings[run_end].group.as_deref() == Some(group_name.as_str())
            {
                run_end += 1;
            }
            if run_end - index > 1 {
                left_items.push(LeftBindingItem::Grouped {
                    label: group_name,
                    bindings: left_bindings[index..run_end].to_vec(),
                });
            } else {
                left_items.push(LeftBindingItem::Single(binding.clone()));
            }
            index = run_end;
        }

        (left_items, palette)
    }

    fn bindings_from_hints(hints: &[crate::event::BindingHint]) -> Vec<FooterBinding> {
        hints
            .iter()
            .filter(|hint| hint.show)
            .map(|hint| {
                let mut binding = FooterBinding::new(
                    hint.key_display.clone().unwrap_or_else(|| hint.key.clone()),
                    hint.description.clone(),
                );
                binding.group = hint.group.clone();
                // Store the raw key spec for click-to-invoke dispatch.
                binding.action_key = Some(hint.key.clone());
                binding
            })
            .collect()
    }

    fn apply_bindings(&mut self, next: Vec<FooterBinding>, ctx: &mut EventCtx) {
        if next == self.bindings {
            return;
        }
        self.bindings = next;
        ctx.post_message(Message::FooterBindingsUpdated(FooterBindingsUpdated {
            count: self.bindings.len(),
        }));
        ctx.request_repaint();
    }

    /// Compute the width (in cells) of a single binding's rendered segments.
    fn binding_width(binding: &FooterBinding, compact: bool) -> usize {
        let key_width = if compact {
            rich_rs::cell_len(&binding.key)
        } else {
            rich_rs::cell_len(&binding.key) + 1 // " " prefix
        };
        let desc_width = if binding.description.is_empty() {
            if compact { 0 } else { 1 } // trailing space
        } else {
            rich_rs::cell_len(&binding.description) + 1 // " " prefix
        };
        key_width + desc_width
    }

    /// Find which binding (by flat index into `self.bindings`) is at the given
    /// content-local x coordinate. Returns `None` if no binding is at that position.
    ///
    /// This replicates the left-section layout logic from `render` to compute
    /// binding hit regions without storing mutable state.
    fn binding_index_at_x(&self, x: u16) -> Option<usize> {
        let (left_items, _palette) = self.split_bindings();
        let separator_width: usize = if self.compact { 1 } else { 3 };
        let mut pos: usize = 0;
        let mut flat_index: usize = 0;

        for (i, item) in left_items.iter().enumerate() {
            if i > 0 {
                pos += separator_width;
            }
            match item {
                LeftBindingItem::Single(binding) => {
                    let w = Self::binding_width(binding, self.compact);
                    if (x as usize) >= pos && (x as usize) < pos + w {
                        return Some(flat_index);
                    }
                    pos += w;
                    flat_index += 1;
                }
                LeftBindingItem::Grouped { label, bindings } => {
                    let group_start = pos;
                    let key_sep_width: usize = if self.compact { 1 } else { 2 };
                    for (j, binding) in bindings.iter().enumerate() {
                        if j > 0 {
                            pos += key_sep_width;
                        }
                        // In groups, only the key part is rendered (description cleared).
                        let key_width = if self.compact {
                            rich_rs::cell_len(&binding.key)
                        } else {
                            rich_rs::cell_len(&binding.key) + 1
                        };
                        // Trailing space when description is empty & not compact.
                        let trail = if self.compact { 0 } else { 1 };
                        pos += key_width + trail;
                    }
                    pos += rich_rs::cell_len(label) + 1; // " label"
                    if (x as usize) >= group_start && (x as usize) < pos {
                        // Map click within the group to the first binding.
                        return Some(flat_index);
                    }
                    flat_index += bindings.len();
                }
            }
        }

        None
    }

    /// Look up the `FooterBinding` at a flat index (skipping command_palette bindings).
    fn binding_at_flat_index(&self, flat_index: usize) -> Option<&FooterBinding> {
        self.bindings
            .iter()
            .filter(|b| b.group.as_deref() != Some("command_palette"))
            .nth(flat_index)
    }
}

#[derive(Debug, Clone)]
enum LeftBindingItem {
    Single(FooterBinding),
    Grouped {
        label: String,
        bindings: Vec<FooterBinding>,
    },
}

impl Widget for Footer {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let base_style = self.base_style();
        let key_style = self.key_style();
        let description_style = self.description_style();
        let command_palette_style = self.command_palette_style();
        let palette_separator_style = self.palette_separator_style();

        let (left_bindings, palette) = self.split_bindings();

        let separator = if self.compact { " " } else { "   " };
        let mut left_segments = Vec::new();
        for (index, binding) in left_bindings.iter().enumerate() {
            if index > 0 {
                left_segments.push(Segment::styled(separator.to_string(), base_style));
            }
            match binding {
                LeftBindingItem::Single(binding) => {
                    left_segments.extend(self.render_binding(
                        binding,
                        key_style,
                        description_style,
                    ));
                }
                LeftBindingItem::Grouped { label, bindings } => {
                    left_segments.extend(self.render_group(
                        label,
                        bindings,
                        key_style,
                        description_style,
                        base_style,
                    ));
                }
            }
        }

        let mut line_segments = left_segments;
        if let Some(palette_binding) = palette {
            let mut right_segments =
                self.render_binding(&palette_binding, key_style, command_palette_style);
            // Keep command palette hint docked at the right with a subtle visible separator.
            if self.compact {
                right_segments.insert(0, Segment::styled("│".to_string(), palette_separator_style));
            } else {
                right_segments.insert(
                    0,
                    Segment::styled(" │ ".to_string(), palette_separator_style),
                );
            }

            let left_width = Segment::get_line_length(&line_segments);
            let right_width = Segment::get_line_length(&right_segments);
            if left_width + right_width < width {
                line_segments.push(Segment::styled(
                    " ".repeat(width - left_width - right_width),
                    base_style,
                ));
                line_segments.extend(right_segments);
            } else {
                line_segments.extend(right_segments);
            }
        }

        let rendered = if line_segments.is_empty() {
            Text::plain(String::new()).render(console, options)
        } else {
            let mut out = Segments::new();
            out.extend(line_segments);
            out
        };
        let split = Segment::split_and_crop_lines(rendered, width, None, true, false);
        let mut out = Segments::new();
        if let Some(line) = split.first() {
            out.extend(adjust_line_length_no_bg(line, width));
        } else {
            out.push(Segment::styled(" ".repeat(width), base_style));
        }
        out
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(Some(1))
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::AppFocus(active) => {
                self.app_focused = *active;
                if *active {
                    if let Some(next) = self.deferred_bindings.take() {
                        self.apply_bindings(next, ctx);
                    }
                }
            }
            Event::BindingsChanged(bindings) => {
                let next = Self::bindings_from_hints(bindings);
                if self.app_focused {
                    self.apply_bindings(next, ctx);
                } else {
                    self.deferred_bindings = Some(next);
                }
            }
            // Click-to-invoke: when a binding label is clicked, log the action key
            // for dispatch. Full key simulation requires runtime wiring.
            Event::MouseDown(mouse) => {
                if let Some(flat_index) = self.binding_index_at_x(mouse.x) {
                    if let Some(binding) = self.binding_at_flat_index(flat_index) {
                        let action_key = binding.action_key.as_deref().unwrap_or(&binding.key);
                        debug_message(&format!(
                            "[footer] click binding key=\"{}\" action_key=\"{}\" desc=\"{}\"",
                            binding.key, action_key, binding.description
                        ));
                        ctx.set_handled();
                        ctx.request_repaint();
                    }
                }
            }
            _ => {}
        }
    }

    fn on_unmount(&mut self) {
        self.app_focused = true;
        self.deferred_bindings = None;
    }

    fn style_classes(&self) -> &[String] {
        if self.classes.is_empty() {
            empty_classes()
        } else {
            &self.classes
        }
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for Footer {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

impl ReactiveWidget for Footer {
    fn reactive_dispatch(&mut self, changes: &[ReactiveChange], ctx: &mut ReactiveCtx) {
        for change in changes {
            match change.field_name {
                "compact" => {
                    if let (Some(old), Some(new)) = (
                        change.old_value.downcast_ref::<bool>(),
                        change.new_value.downcast_ref::<bool>(),
                    ) {
                        self.watch_compact(old, new, ctx);
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Footer;
    use crate::event::{BindingHint, Event, EventCtx, MouseDownEvent};
    use crate::message::*;
    use crate::node_id::NodeId;
    use crate::widgets::Widget;

    #[test]
    fn bindings_changed_posts_footer_bindings_updated_message() {
        let mut footer = Footer::new();
        let mut ctx = EventCtx::default();
        footer.on_event(
            &Event::BindingsChanged(vec![BindingHint::new("ctrl+p", "Palette")]),
            &mut ctx,
        );
        let messages = ctx.take_messages();
        assert!(messages.iter().any(|m| matches!(
            m.message,
            Message::FooterBindingsUpdated(FooterBindingsUpdated { count: 1 })
        )));
    }

    #[test]
    fn identical_bindings_changed_is_noop() {
        let mut footer = Footer::new();
        let mut first_ctx = EventCtx::default();
        let hints = vec![BindingHint::new("ctrl+p", "Palette")];
        footer.on_event(&Event::BindingsChanged(hints.clone()), &mut first_ctx);
        assert!(!first_ctx.take_messages().is_empty());

        let mut second_ctx = EventCtx::default();
        footer.on_event(&Event::BindingsChanged(hints), &mut second_ctx);
        assert!(second_ctx.take_messages().is_empty());
    }

    #[test]
    fn bindings_changed_defers_while_app_unfocused() {
        let mut footer = Footer::new();
        let mut unfocus_ctx = EventCtx::default();
        footer.on_event(&Event::AppFocus(false), &mut unfocus_ctx);
        assert!(unfocus_ctx.take_messages().is_empty());
        assert!(!unfocus_ctx.repaint_requested());

        let mut bindings_ctx = EventCtx::default();
        footer.on_event(
            &Event::BindingsChanged(vec![BindingHint::new("ctrl+p", "Palette")]),
            &mut bindings_ctx,
        );
        assert!(bindings_ctx.take_messages().is_empty());
        assert!(!bindings_ctx.repaint_requested());
    }

    #[test]
    fn focus_gain_applies_latest_deferred_bindings_once() {
        let mut footer = Footer::new();
        let mut unfocus_ctx = EventCtx::default();
        footer.on_event(&Event::AppFocus(false), &mut unfocus_ctx);

        let mut first_ctx = EventCtx::default();
        footer.on_event(
            &Event::BindingsChanged(vec![BindingHint::new("a", "alpha")]),
            &mut first_ctx,
        );
        assert!(first_ctx.take_messages().is_empty());

        let mut second_ctx = EventCtx::default();
        footer.on_event(
            &Event::BindingsChanged(vec![
                BindingHint::new("a", "alpha"),
                BindingHint::new("b", "bravo"),
            ]),
            &mut second_ctx,
        );
        assert!(second_ctx.take_messages().is_empty());

        let mut focus_ctx = EventCtx::default();
        footer.on_event(&Event::AppFocus(true), &mut focus_ctx);
        let messages = focus_ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            messages[0].message,
            Message::FooterBindingsUpdated(FooterBindingsUpdated { count: 2 })
        ));
        assert!(focus_ctx.repaint_requested());
    }

    #[test]
    fn repeated_focus_loss_does_not_drop_deferred_bindings() {
        let mut footer = Footer::new();
        let mut ctx = EventCtx::default();
        footer.on_event(&Event::AppFocus(false), &mut ctx);
        footer.on_event(
            &Event::BindingsChanged(vec![BindingHint::new("ctrl+p", "Palette")]),
            &mut ctx,
        );
        footer.on_event(&Event::AppFocus(false), &mut ctx);

        let mut focus_ctx = EventCtx::default();
        footer.on_event(&Event::AppFocus(true), &mut focus_ctx);
        let messages = focus_ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            messages[0].message,
            Message::FooterBindingsUpdated(FooterBindingsUpdated { count: 1 })
        ));
    }

    #[test]
    fn unmount_resets_focus_tracking_state() {
        let mut footer = Footer::new();
        let mut unfocus_ctx = EventCtx::default();
        footer.on_event(&Event::AppFocus(false), &mut unfocus_ctx);
        footer.on_unmount();

        let mut ctx = EventCtx::default();
        footer.on_event(
            &Event::BindingsChanged(vec![BindingHint::new("ctrl+p", "Palette")]),
            &mut ctx,
        );
        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert!(matches!(
            messages[0].message,
            Message::FooterBindingsUpdated(FooterBindingsUpdated { count: 1 })
        ));
    }

    // ── WP-22: Footer Signal subscription + click-to-invoke ─────────────

    #[test]
    fn bindings_from_hints_stores_action_key() {
        let mut footer = Footer::new();
        let mut ctx = EventCtx::default();
        let hints = vec![BindingHint::new("ctrl+s", "Save").with_key_display("^s")];
        footer.on_event(&Event::BindingsChanged(hints), &mut ctx);

        // The displayed key should be the key_display ("^s"), not the raw key.
        assert_eq!(footer.bindings[0].key, "^s");
        // The action_key should store the raw key spec.
        assert_eq!(footer.bindings[0].action_key.as_deref(), Some("ctrl+s"));
    }

    #[test]
    fn binding_index_at_x_finds_first_binding() {
        let footer = Footer::new()
            .with_binding("^q", "Quit")
            .with_binding("^s", "Save");
        // In non-compact mode, first binding starts at x=0:
        //   " ^q Quit" = 8 chars, then "   " separator (3), then " ^s Save"
        // So clicking at x=0 should hit the first binding.
        assert_eq!(footer.binding_index_at_x(0), Some(0));
    }

    #[test]
    fn binding_index_at_x_finds_second_binding() {
        let footer = Footer::new()
            .with_binding("^q", "Quit")
            .with_binding("^s", "Save");
        // First binding: " ^q Quit" = 8 chars
        // Separator: "   " = 3 chars
        // Second binding starts at x=11
        assert_eq!(footer.binding_index_at_x(11), Some(1));
    }

    #[test]
    fn binding_index_at_x_returns_none_past_bindings() {
        let footer = Footer::new().with_binding("^q", "Quit");
        // " ^q Quit" = 8 chars, so x=8 is past the binding.
        assert_eq!(footer.binding_index_at_x(50), None);
    }

    #[test]
    fn click_on_binding_is_handled() {
        let mut footer = Footer::new()
            .with_binding("^q", "Quit")
            .with_binding("^s", "Save");
        let mut ctx = EventCtx::default();

        // Click at x=0 should hit the first binding.
        footer.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: NodeId::default(),
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut ctx,
        );

        assert!(ctx.handled());
        assert!(ctx.repaint_requested());
    }

    #[test]
    fn click_past_bindings_is_not_handled() {
        let mut footer = Footer::new().with_binding("^q", "Quit");
        let mut ctx = EventCtx::default();

        // Click way past the binding region.
        footer.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: NodeId::default(),
                screen_x: 50,
                screen_y: 0,
                x: 50,
                y: 0,
            }),
            &mut ctx,
        );

        assert!(!ctx.handled());
    }

    #[test]
    fn footer_binding_with_action_key_builder() {
        use super::FooterBinding;
        let binding = FooterBinding::new("^s", "Save").with_action_key("ctrl+s");
        assert_eq!(binding.action_key.as_deref(), Some("ctrl+s"));
    }

    #[test]
    fn binding_index_at_x_compact_mode() {
        let footer = Footer::new()
            .compact(true)
            .with_binding("^q", "Quit")
            .with_binding("^s", "Save");
        // Compact mode: "^q Quit" (7 chars), " " separator (1), "^s Save" (7 chars)
        // First binding: 0..7
        // Separator: 7..8
        // Second binding: 8..15
        assert_eq!(footer.binding_index_at_x(0), Some(0));
        assert_eq!(footer.binding_index_at_x(6), Some(0));
        assert_eq!(footer.binding_index_at_x(8), Some(1));
    }
}
