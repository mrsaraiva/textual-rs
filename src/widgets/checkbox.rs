use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, MetaValue, Segment, Segments};
use textual_macros::widget;

use crate::content::{Content, ContentPart};
use crate::event::{Action, Event};
use crate::message::*;
#[cfg(test)]
use crate::node_id::NodeId;
use crate::reactive::{
    ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget, RuntimeReactiveEntry,
    enqueue_runtime_reactive_entry,
};

use crate::action::ParsedAction;

use super::{BindingDecl, Focus, Interactive, Layout, NodeSeed, Render};

/// Tag a segment with `textual:no_text_style = true` so `apply_style_to_segments`
/// skips re-applying widget CSS text attributes already baked in by
/// `Content::render_strips`.
fn tag_segment_no_text_style(seg: &mut Segment) {
    let mut meta = seg.meta.take().unwrap_or_default();
    let mut map: std::collections::BTreeMap<String, MetaValue> = meta
        .meta
        .as_ref()
        .map(|m| (**m).clone())
        .unwrap_or_default();
    map.insert(
        "textual:no_text_style".to_string(),
        MetaValue::Bool(true),
    );
    meta.meta = Some(std::sync::Arc::new(map));
    seg.meta = Some(meta);
}

#[derive(Debug, Clone)]
#[widget(Focus, Interactive, Layout, reactive)]
pub struct Checkbox {
    label: String,
    checked: bool,
    pressed: bool,
    disabled: bool,
    seed: NodeSeed,
}

impl Checkbox {
    crate::seed_ident_methods!();

    pub fn new(label: impl Into<String>) -> Self {
        let mut seed = NodeSeed::default();
        seed.classes.push("checkbox".to_string());
        Self {
            label: label.into(),
            checked: false,
            pressed: false,
            disabled: false,
            seed,
        }
    }

    /// The label parsed as Textual markup, matching Python
    /// `ToggleButton._make_label` (`Content.from_markup`): tags become styled
    /// spans (`[magenta]Ginaz[/]` colourises at render), emoji shortcodes are
    /// left literal (`:sweat:`) to match Python `Content` semantics.
    fn label_content(&self) -> crate::content::Content {
        crate::content::Content::from_markup(&self.label)
    }

    // ── Reactive getters ─────────────────────────────────────────────────

    pub fn checked(&self) -> bool {
        self.checked
    }

    // ── Reactive setters ─────────────────────────────────────────────────

    /// Reactive setter for `checked`. Records the change in the provided
    /// [`ReactiveCtx`] if the value actually changed.
    pub fn set_checked(&mut self, value: bool, ctx: &mut ReactiveCtx) {
        if self.checked != value {
            let old = self.checked;
            self.checked = value;
            ctx.record_change(
                "checked",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(value),
            );
        }
    }

    // ── Watchers ─────────────────────────────────────────────────────────

    fn watch_checked(&mut self, _old: &bool, _new: &bool, ctx: &mut ReactiveCtx) {
        // Keep the detached seed classes in sync (pre-mount identity — the node
        // inherits them at mount) AND queue a class op so the arena node toggles
        // `-on` too. The node's classes drive `&.-on > .toggle--button` matching
        // and the repaint that recolors the button glyph. Mirrors Python
        // `ToggleButton.watch_value` → `self.set_class(self.value, "-on")`.
        self.rebuild_classes_in_place();
        ctx.set_class(self.checked, "-on");
    }

    // ── Builder methods ──────────────────────────────────────────────────

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self.rebuild_classes_in_place();
        self
    }

    // ── Internal helpers ─────────────────────────────────────────────────

    fn emit_changed(&self, ctx: &mut crate::event::WidgetCtx) {
        ctx.post_message(CheckboxChanged {
            checked: self.checked,
        });
    }

    fn rebuild_classes_in_place(&mut self) {
        let mut classes = vec!["checkbox".to_string()];
        if self.checked {
            classes.push("-on".to_string());
        }
        if self.disabled {
            classes.push("disabled".to_string());
        }
        self.seed.classes = classes;
    }

    fn toggle_reactive(&mut self, ctx: &mut crate::event::WidgetCtx) {
        let node_id = crate::widgets::Widget::node_id(self);
        let mut reactive = ReactiveCtx::new(node_id);
        self.set_checked(!self.checked, &mut reactive);
        if reactive.has_changes() {
            enqueue_runtime_reactive_entry(RuntimeReactiveEntry::new(node_id, reactive));
            self.emit_changed(ctx);
        }
    }
}

impl ReactiveWidget for Checkbox {
    fn reactive_dispatch(&mut self, changes: &[ReactiveChange], ctx: &mut ReactiveCtx) {
        for change in changes {
            if change.field_name == "checked" {
                if let (Some(old), Some(new)) = (
                    change.old_value.downcast_ref::<bool>(),
                    change.new_value.downcast_ref::<bool>(),
                ) {
                    self.watch_checked(old, new, ctx);
                }
            }
        }
    }
}

impl Focus for Checkbox {
    fn focusable(&self) -> bool {
        !self.disabled
    }

    fn is_active(&self) -> bool {
        self.pressed && crate::widgets::Widget::node_state(self).hovered
    }

    fn action_namespace(&self) -> &str {
        "checkbox"
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("enter,space", "toggle", "Toggle checkbox")]
    }

    fn execute_action(&mut self, action: &ParsedAction, ctx: &mut crate::event::WidgetCtx) -> bool {
        match action.name.as_str() {
            "toggle" => {
                if self.disabled {
                    return false;
                }
                self.toggle_reactive(ctx);
                ctx.set_handled();
                true
            }
            _ => false,
        }
    }
}

impl Layout for Checkbox {
    fn content_width(&self) -> Option<usize> {
        // PURE content width — the layout adds border/padding chrome (RA-2
        // contract). Python ToggleButton.get_content_width: 3 (the `▐X▌` button)
        // + 2 (the label's 1-cell left/right pad) + the label's own width. Markup
        // tags are stripped first so `[b]…[/b]` doesn't inflate the width.
        Some(self.label_content().cell_length().saturating_add(3 + 2).max(1))
    }

    fn layout_height(&self) -> Option<usize> {
        // PURE content height (1 row). The flow layout adds the CSS-resolved
        // vertical chrome (the default `border: tall` adds 2 rows) with ancestor
        // context, symmetric with the width axis.
        Some(1)
    }
}

impl Interactive for Checkbox {
    fn on_event(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
        if self.disabled {
            return;
        }
        match event {
            Event::MouseDown(mouse) if mouse.target == crate::widgets::Widget::node_id(self) => {
                self.pressed = true;
                ctx.request_repaint();
                ctx.set_handled();
            }
            Event::MouseUp(mouse)
                if self.pressed => {
                    self.pressed = false;
                    ctx.request_repaint();
                    if mouse.target.is_some_and(|t| t == crate::widgets::Widget::node_id(self)) {
                        self.toggle_reactive(ctx);
                        ctx.set_handled();
                    }
                }
            Event::AppFocus(false)
                if self.pressed => {
                    self.pressed = false;
                    ctx.request_repaint();
                }
            Event::Action(Action::Toggle) if crate::widgets::Widget::node_state(self).focused => {
                self.toggle_reactive(ctx);
                ctx.set_handled();
            }
            Event::Key(key) if crate::widgets::Widget::node_state(self).focused => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.toggle_reactive(ctx);
                    ctx.set_handled();
                }
                _ => {}
            },
            _ => {}
        }
    }
}

impl Render for Checkbox {
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);

        // Resolve the widget's visual style from the style stack so focus/hover
        // state is reflected in the background.
        let visual_style = crate::css::current_self_style().unwrap_or_default();

        // Flatten widget's own bg over the ancestor composited background so
        // transparent-bg checkboxes still get the correct surface color.
        let parent_bg =
            crate::css::current_ancestor_composited_background().unwrap_or_else(|| {
                crate::style::parse_color_like("$background")
                    .unwrap_or(crate::style::Color::rgb(0, 0, 0))
            });
        let effective_bg = visual_style
            .bg
            .map(|c| c.flatten_over(parent_bg))
            .unwrap_or(parent_bg);
        let mut render_style = visual_style.clone();
        render_style.bg = Some(effective_bg);

        // Python's `ToggleButton` renders `▐X▌` — the `X` is ALWAYS present; the
        // checked state is conveyed by the button color (`.toggle--button`, which
        // brightens via `&.-on > .toggle--button` since `self` carries `-on` when
        // checked), not by swapping the glyph.
        //
        // Resolve the component styles against the LIVE CSS context (this node's
        // meta — real classes like `-on` plus interaction states — is already the
        // top of the selector stack, pushed by `render_widget_with_meta`), the
        // same way `RadioButton` does. `resolve_component_style(self, …)` would
        // re-push a meta built from `Widget::style_classes()`, which is EMPTY for
        // `Checkbox` (no `StyleIdentity` capability), so `&.-on > .toggle--button`
        // never matched and the checked mark kept the unchecked colour.
        let button_style = crate::css::resolve_style_for_meta(
            &crate::css::selector_meta_component("", &["toggle--button"]),
        );
        let label_style = crate::css::resolve_style_for_meta(
            &crate::css::selector_meta_component("", &["toggle--label"]),
        );

        // Side half-blocks use the button background as their foreground.
        // Python: `side_style = Style(foreground=button_style.background, background=self.background_colors[1])`
        let mut side_style = crate::style::Style::new();
        side_style.fg = button_style.bg;

        // Build Content via assemble, mirroring Python `Content.assemble(button, label)`:
        //   button  = ▐ (side_style) + X (button_style) + ▌ (side_style)
        //   label   = " label " — markup-parsed label, padded 1 cell each side,
        //             with `label_style` layered UNDER the markup spans
        //             (Python: `self._label.pad(1, 1).stylize_before(label_style)`).
        let content = Content::assemble([
            ContentPart::from(("▐", side_style.clone())),
            ContentPart::from(("X", button_style)),
            ContentPart::from(("▌", side_style)),
            ContentPart::from(super::helpers::toggle_label_content(&self.label, label_style)),
        ]);

        let resolve_fn = super::helpers::markup_tag_resolve;

        // Render via Content::render_strips.
        // - width: content width as received (borders/padding excluded by caller).
        // - height=Some(1): checkbox is always single-line.
        // - no_wrap=true: single-line — never word-wrap.
        // - line_pad=0: no additional inner padding (the ` label ` pad is baked in).
        // - Left align: content already fills its natural width.
        let strips = content.render_strips(
            width,
            Some(1),
            &render_style,
            crate::style::TextAlign::Left,
            "fold",
            true,
            0,
            resolve_fn,
        );

        // Flatten strips into Segments and tag each with no_text_style so
        // apply_style_to_segments does not re-apply CSS text attrs that have
        // already been baked in by render_strips.
        let mut out = Segments::new();
        for strip in strips {
            for mut seg in strip {
                tag_segment_no_text_style(&mut seg);
                out.push(seg);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventCtx;
    use crate::keys::KeyEventData;
    use crate::runtime::dispatch_ctx::set_dispatch_recipient;
    use crate::widgets::NodeState;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use slotmap::SlotMap;

    fn make_node_id() -> NodeId {
        let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
        sm.insert(())
    }

    fn focused_state() -> NodeState {
        NodeState {
            focused: true,
            ..Default::default()
        }
    }

    #[test]
    fn checkbox_emits_message_on_toggle() {
        let mut checkbox = Checkbox::new("Remember");
        let id = make_node_id();
        let _guard = set_dispatch_recipient(id, focused_state());
        let key =
            KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            checkbox.on_event(&Event::Key(key), &mut __w);
        }
        let messages = ctx.take_messages();
        assert!(messages.iter().any(|m| {
            m.downcast_ref::<CheckboxChanged>()
                .is_some_and(|c| c.checked)
        }));
    }

    #[test]
    fn bindings_are_declared() {
        let checkbox = Checkbox::new("Test");
        let bindings = checkbox.bindings();
        assert!(!bindings.is_empty());
        assert!(bindings.iter().any(|b| b.action == "toggle"));
    }

    #[test]
    fn execute_action_handles_toggle() {
        let mut checkbox = Checkbox::new("Remember");
        let mut ctx = EventCtx::default();
        let action = ParsedAction {
            namespace: None,
            name: "toggle".to_string(),
            arguments: vec![],
        };
        assert!(!checkbox.checked());
        assert!({ let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); checkbox.execute_action(&action, &mut __w) });
        assert!(checkbox.checked());
        let messages = ctx.take_messages();
        assert!(messages.iter().any(|m| {
            m.downcast_ref::<CheckboxChanged>()
                .is_some_and(|c| c.checked)
        }));
    }

    // ── Reactive field tests ────────────────────────────────────────────

    #[test]
    fn reactive_set_checked_records_change() {
        let mut checkbox = Checkbox::new("Test");
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        checkbox.set_checked(true, &mut ctx);
        assert!(checkbox.checked());
        assert!(ctx.has_changes());
        assert!(ctx.needs_repaint());
        assert_eq!(ctx.changes()[0].field_name, "checked");
    }

    #[test]
    fn reactive_set_checked_noop_when_same() {
        let mut checkbox = Checkbox::new("Test");
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        checkbox.set_checked(false, &mut ctx);
        assert!(!ctx.has_changes());
    }

    #[test]
    fn reactive_dispatch_calls_watch_checked() {
        let mut checkbox = Checkbox::new("Test");
        let id = make_node_id();
        let mut ctx = ReactiveCtx::new(id);
        checkbox.set_checked(true, &mut ctx);
        let changes = ctx.take_changes();
        checkbox.reactive_dispatch(&changes, &mut ctx);
        // watch_checked rebuilds classes — verify -on class
        assert!(checkbox.seed.classes.contains(&"-on".to_string()));
    }

    // ── compose tests ──────────────────────────────────────────────────

    #[test]
    fn checkbox_compose_returns_empty() {
        let mut checkbox = Checkbox::new("Test");
        assert!(checkbox.compose().is_empty());
    }
}
