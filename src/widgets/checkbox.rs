use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, MetaValue, Renderable, Segment, Segments};

use crate::compose::ComposeResult;
use crate::content::{Content, ContentPart};
use crate::event::{Action, Event, EventCtx};
use crate::message::*;
#[cfg(test)]
use crate::node_id::NodeId;
use crate::reactive::{
    ReactiveChange, ReactiveCtx, ReactiveFlags, ReactiveWidget, RuntimeReactiveEntry,
    enqueue_runtime_reactive_entry,
};

use crate::action::ParsedAction;

use super::{BindingDecl, NodeSeed, Widget};

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

    /// The label with Rich markup applied (tags stripped to plain text), matching
    /// Python `ToggleButton._make_label` (`Content.from_markup`). Emoji shortcodes
    /// are left literal (`:sweat:`) to match Python's rendering, so `emoji=false`.
    fn label_plain(&self) -> String {
        rich_rs::Text::from_markup(&self.label, false)
            .map(|t| t.plain_text().to_string())
            .unwrap_or_else(|_| self.label.clone())
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
        // Keep the detached seed classes in sync (off-tree CSS resolution in
        // `render` reads them) AND queue a class op so the arena node toggles
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

    fn emit_changed(&self, ctx: &mut EventCtx) {
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

    fn toggle_reactive(&mut self, ctx: &mut EventCtx) {
        let node_id = self.node_id();
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

impl Widget for Checkbox {
    fn compose(&mut self) -> ComposeResult {
        // Monolithic widget: renders inline, declares no arena children.
        Vec::new()
    }

    fn focusable(&self) -> bool {
        !self.disabled
    }

    fn is_active(&self) -> bool {
        self.pressed && self.node_state().hovered
    }

    fn content_width(&self) -> Option<usize> {
        // PURE content width — the layout adds border/padding chrome (RA-2
        // contract). Python ToggleButton.get_content_width: 3 (the `▐X▌` button)
        // + 2 (the label's 1-cell left/right pad) + the label's own width. Markup
        // tags are stripped first so `[b]…[/b]` doesn't inflate the width.
        Some(rich_rs::cell_len(&self.label_plain()).saturating_add(3 + 2).max(1))
    }

    fn action_namespace(&self) -> &str {
        "checkbox"
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![BindingDecl::new("enter,space", "toggle", "Toggle checkbox")]
    }

    fn execute_action(&mut self, action: &ParsedAction, ctx: &mut EventCtx) -> bool {
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

    fn reactive_widget(&mut self) -> Option<&mut dyn ReactiveWidget> {
        Some(self)
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if self.disabled {
            return;
        }
        match event {
            Event::MouseDown(mouse) if mouse.target == self.node_id() => {
                self.pressed = true;
                ctx.request_repaint();
                ctx.set_handled();
            }
            Event::MouseUp(mouse)
                if self.pressed => {
                    self.pressed = false;
                    ctx.request_repaint();
                    if mouse.target.is_some_and(|t| t == self.node_id()) {
                        self.toggle_reactive(ctx);
                        ctx.set_handled();
                    }
                }
            Event::AppFocus(false)
                if self.pressed => {
                    self.pressed = false;
                    ctx.request_repaint();
                }
            Event::Action(Action::Toggle) if self.node_state().focused => {
                self.toggle_reactive(ctx);
                ctx.set_handled();
            }
            Event::Key(key) if self.node_state().focused => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.toggle_reactive(ctx);
                    ctx.set_handled();
                }
                _ => {}
            },
            _ => {}
        }
    }

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
        // checked), not by swapping the glyph. `self` exposes its `-on` class to
        // off-tree resolution via its seed classes.
        let button_style = crate::css::resolve_component_style(self, &["toggle--button"]);
        let label_style = crate::css::resolve_component_style(self, &["toggle--label"]);

        // Side half-blocks use the button background as their foreground.
        // Python: `side_style = Style(foreground=button_style.background, background=self.background_colors[1])`
        let mut side_style = crate::style::Style::new();
        side_style.fg = button_style.bg;

        // Build Content via assemble, mirroring Python `Content.assemble(button, label)`:
        //   button  = ▐ (side_style) + X (button_style) + ▌ (side_style)
        //   label   = " label " (label_style, padded 1 cell each side)
        let content = Content::assemble([
            ContentPart::from(("▐", side_style.clone())),
            ContentPart::from(("X", button_style)),
            ContentPart::from(("▌", side_style)),
            ContentPart::from((format!(" {} ", self.label_plain()), label_style)),
        ]);

        let resolve_fn = |raw: &str| {
            crate::content::markup::parse_tag_style(raw)
                .map(|t| t.style)
                .unwrap_or_default()
        };

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

    fn layout_height(&self) -> Option<usize> {
        // 1 content row + own border/padding chrome (the default `border: tall`
        // adds 2 rows). The layout side adds only margin (extract_child_spec).
        Some(1 + super::helpers::resolved_vertical_chrome(self))
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

impl Renderable for Checkbox {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        checkbox.on_event(&Event::Key(key), &mut ctx);
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
        assert!(checkbox.execute_action(&action, &mut ctx));
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
