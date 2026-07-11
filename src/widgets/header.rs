use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rich_rs::{Console, ConsoleOptions, Segment, Segments};
use textual_macros::widget;

use crate::compose::ComposeResult;
use crate::event::{BindingHint, Event};
use crate::message::*;

use super::{NodeSeed, Widget};
use crate::reactive::{ReactiveCtx, ReactiveFlags, ReactiveWidget};

const COMMAND_PALETTE_HINT_GROUP: &str = "command_palette";
const DEFAULT_COMMAND_PALETTE_KEY: &str = "ctrl+p";
const DEFAULT_COMMAND_PALETTE_TOOLTIP: &str = "Open command palette";

#[derive(Debug, Clone)]
#[widget(Focus, Interactive, HasTooltip)]
pub struct HeaderIcon {
    icon: String,
    hovered: bool,
    pressed: bool,
    command_palette_action_key: Option<String>,
    command_palette_tooltip: Option<String>,
    layout_width: u16,
    layout_height: u16,
    seed: NodeSeed,
}

impl HeaderIcon {
    crate::seed_ident_methods!();

    pub fn new(icon: impl Into<String>) -> Self {
        Self {
            icon: icon.into(),
            hovered: false,
            pressed: false,
            command_palette_action_key: Some(DEFAULT_COMMAND_PALETTE_KEY.to_string()),
            command_palette_tooltip: Some(DEFAULT_COMMAND_PALETTE_TOOLTIP.to_string()),
            layout_width: 1,
            layout_height: 1,
            seed: NodeSeed::default(),
        }
    }

    fn normalize_tooltip(text: Option<&str>) -> Option<String> {
        text.map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    }

    fn command_palette_hint(bindings: &[BindingHint]) -> Option<&BindingHint> {
        bindings
            .iter()
            .find(|hint| hint.group.as_deref() == Some(COMMAND_PALETTE_HINT_GROUP))
    }

    fn apply_bindings(&mut self, bindings: &[BindingHint]) -> bool {
        let (next_action_key, next_tooltip) =
            if let Some(hint) = Self::command_palette_hint(bindings) {
                (
                    Some(hint.key.clone()),
                    Self::normalize_tooltip(hint.tooltip.as_deref()),
                )
            } else {
                (None, None)
            };
        let changed = self.command_palette_action_key != next_action_key
            || self.command_palette_tooltip != next_tooltip;
        self.command_palette_action_key = next_action_key;
        self.command_palette_tooltip = next_tooltip;
        changed
    }
}

impl crate::widgets::Focus for HeaderIcon {
    fn mouse_interactive(&self) -> bool {
        true
    }

    fn is_active(&self) -> bool {
        self.pressed
    }
}

impl crate::widgets::Interactive for HeaderIcon {
    fn on_event(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
        match event {
            Event::BindingsChanged(bindings)
                if self.apply_bindings(bindings) => {
                    ctx.request_repaint();
                }
            Event::MouseDown(mouse) if mouse.target == self.node_id() => {
                self.pressed = true;
                ctx.set_handled();
            }
            Event::MouseUp(mouse) => {
                if !self.pressed {
                    return;
                }
                self.pressed = false;
                if mouse.target.is_some_and(|target| target == self.node_id()) {
                    ctx.post_message(HeaderIconPressed);
                    if self.command_palette_action_key.is_some() {
                        ctx.post_message(AppCommandPalette);
                    }
                    ctx.request_repaint();
                    ctx.set_handled();
                }
            }
            Event::AppFocus(false) => {
                self.hovered = false;
                self.pressed = false;
            }
            _ => {}
        }
    }

    fn on_node_state_changed(
        &mut self,
        _old: crate::widgets::NodeState,
        new: crate::widgets::NodeState,
    ) {
        self.hovered = new.hovered;
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.layout_width = width.max(1);
        self.layout_height = height.max(1);
    }
}

impl crate::widgets::HasTooltip for HeaderIcon {
    fn tooltip(&self) -> Option<String> {
        Self::normalize_tooltip(self.command_palette_tooltip.as_deref())
    }

    fn tooltip_anchor(&self) -> Option<(u16, u16)> {
        let width = self.layout_width.max(1);
        let height = self.layout_height.max(1);
        Some((width / 2, height.saturating_sub(1)))
    }
}

impl crate::widgets::Render for HeaderIcon {
    fn style_type(&self) -> &'static str {
        "HeaderIcon"
    }

    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        let mut out = Segments::new();
        out.push(Segment::new(self.icon.clone()));
        out
    }
}
#[derive(Debug, Clone)]
#[widget(Interactive)]
pub struct HeaderTitle {
    title: String,
    subtitle: Option<String>,
    default_title: String,
    default_subtitle: Option<String>,
    seed: NodeSeed,
}

impl HeaderTitle {
    crate::seed_ident_methods!();

    pub fn new(
        default_title: impl Into<String>,
        default_subtitle: Option<String>,
        title: impl Into<String>,
        subtitle: Option<String>,
    ) -> Self {
        Self {
            title: title.into(),
            subtitle,
            default_title: default_title.into(),
            default_subtitle,
            seed: NodeSeed::default(),
        }
    }

    fn active_subtitle(&self) -> Option<&str> {
        match &self.subtitle {
            Some(subtitle) if !subtitle.is_empty() => Some(subtitle.as_str()),
            _ => None,
        }
    }
}

impl crate::widgets::Interactive for HeaderTitle {
    fn on_message(&mut self, message: &crate::message::MessageEvent, ctx: &mut crate::event::WidgetCtx) {
        if let Some(m) = message.downcast_ref::<ScreenTitleChanged>() {
            self.title = m
                .title
                .as_deref()
                .map(|s| s.to_string())
                .unwrap_or_else(|| self.default_title.clone());
            self.subtitle = m
                .sub_title
                .as_deref()
                .map(|s| Some(s.to_string()))
                .unwrap_or_else(|| self.default_subtitle.clone());
            ctx.request_repaint();
        }
    }
}

impl crate::widgets::Render for HeaderTitle {
    fn style_type(&self) -> &'static str {
        "HeaderTitle"
    }

    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        // Python `App.format_title` (app.py): the ` — ` separator and the
        // subtitle are styled `dim` (`Content.assemble(title, (" — ", "dim"),
        // sub_title.stylize("dim"))`); the render pipeline's dim pre-blend then
        // fades them toward the header background exactly like Python's
        // `ANSIToTruecolor` filter.
        let mut out = Segments::new();
        match self.active_subtitle() {
            Some(subtitle) => {
                let dim = rich_rs::Style::new().with_dim(true);
                out.push(Segment::new(self.title.clone()));
                out.push(Segment::styled(" — ".to_string(), dim));
                out.push(Segment::styled(subtitle.to_string(), dim));
            }
            None => out.push(Segment::new(self.title.clone())),
        }
        out
    }
}
#[derive(Debug, Clone)]
#[widget()]
pub struct HeaderClockSpace {
    seed: NodeSeed,
}

impl HeaderClockSpace {
    crate::seed_ident_methods!();

    pub fn new() -> Self {
        Self {
            seed: NodeSeed::default(),
        }
    }
}

impl Default for HeaderClockSpace {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::widgets::Render for HeaderClockSpace {
    fn style_type(&self) -> &'static str {
        "HeaderClockSpace"
    }

    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }
}
#[derive(Debug, Clone)]
#[widget(Focus)]
pub struct HeaderClock {
    time_format: String,
    last_clock_second: Arc<AtomicU64>,
    seed: NodeSeed,
}

impl HeaderClock {
    crate::seed_ident_methods!();

    pub fn new(time_format: impl Into<String>) -> Self {
        Self {
            time_format: time_format.into(),
            last_clock_second: Arc::new(AtomicU64::new(0)),
            seed: NodeSeed::default(),
        }
    }

    fn current_clock_seconds() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    fn format_clock(&self, epoch_seconds: u64) -> String {
        let day_seconds = epoch_seconds % 86_400;
        let hours = day_seconds / 3_600;
        let minutes = (day_seconds % 3_600) / 60;
        let seconds = day_seconds % 60;

        let h = format!("{hours:02}");
        let m = format!("{minutes:02}");
        let s = format!("{seconds:02}");
        let hms = format!("{h}:{m}:{s}");

        let mut formatted = self.time_format.clone();
        formatted = formatted.replace("%X", &hms);
        formatted = formatted.replace("%T", &hms);
        formatted = formatted.replace("%H", &h);
        formatted = formatted.replace("%M", &m);
        formatted = formatted.replace("%S", &s);
        if formatted == self.time_format {
            hms
        } else {
            formatted
        }
    }
}

impl crate::widgets::Focus for HeaderClock {
    fn is_active(&self) -> bool {
        let current = Self::current_clock_seconds();
        current != self.last_clock_second.load(Ordering::Relaxed)
    }
}

impl crate::widgets::Render for HeaderClock {
    fn style_type(&self) -> &'static str {
        "HeaderClock"
    }

    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        let seconds = Self::current_clock_seconds();
        self.last_clock_second.store(seconds, Ordering::Relaxed);
        let mut out = Segments::new();
        out.push(Segment::new(self.format_clock(seconds)));
        out
    }
}
#[derive(Debug, Clone)]
#[widget(Focus, Interactive, Layout, StyleIdentity)]
pub struct Header {
    title: String,
    subtitle: Option<String>,
    /// The default (app-level) title, used as fallback when no screen title is active.
    default_title: String,
    /// The default (app-level) subtitle, used as fallback when no screen subtitle is active.
    default_subtitle: Option<String>,
    tall: bool,
    icon: String,
    pressed: bool,
    press_in_toggle_zone: bool,
    show_clock: bool,
    time_format: String,
    children_extracted: bool,
    seed: NodeSeed,
}

impl Default for Header {
    fn default() -> Self {
        Self::new()
    }
}

impl Header {
    crate::seed_ident_methods!();

    pub fn new() -> Self {
        Self {
            title: "textual-rs".to_string(),
            subtitle: None,
            default_title: "textual-rs".to_string(),
            default_subtitle: None,
            tall: false,
            icon: "⭘".to_string(),
            pressed: false,
            press_in_toggle_zone: false,
            show_clock: false,
            time_format: "%X".to_string(),
            children_extracted: false,
            seed: NodeSeed::default(),
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        let t = title.into();
        self.title = t.clone();
        self.default_title = t;
        self
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        let s = subtitle.into();
        self.subtitle = Some(s.clone());
        self.default_subtitle = Some(s);
        self
    }

    pub fn clear_subtitle(mut self) -> Self {
        self.subtitle = None;
        self.default_subtitle = None;
        self
    }

    // ── Reactive getters ─────────────────────────────────────────────────

    /// Reactive getter for `title`.
    pub fn get_title(&self) -> &str {
        &self.title
    }

    /// Reactive getter for `subtitle`.
    pub fn get_subtitle(&self) -> Option<&str> {
        self.subtitle.as_deref()
    }

    /// Reactive getter for `show_clock`.
    pub fn get_show_clock(&self) -> bool {
        self.show_clock
    }

    /// Reactive getter for `tall`.
    pub fn get_tall(&self) -> bool {
        self.tall
    }

    // ── Reactive setters ─────────────────────────────────────────────────

    /// Reactive setter for `title`. Updates the displayed title at runtime.
    ///
    /// Pass `None` to revert to the default (app-level) title.
    pub fn set_title(&mut self, title: Option<&str>, ctx: &mut ReactiveCtx) {
        let new_title = title
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.default_title.clone());
        if self.title != new_title {
            let old = self.title.clone();
            self.title = new_title;
            let new = self.title.clone();
            ctx.record_change(
                "title",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(new),
            );
        }
    }

    /// Reactive setter for `subtitle`. Updates the displayed subtitle at runtime.
    ///
    /// Pass `None` to revert to the default (app-level) subtitle.
    pub fn set_subtitle(&mut self, subtitle: Option<&str>, ctx: &mut ReactiveCtx) {
        let new_subtitle = subtitle
            .map(|s| Some(s.to_string()))
            .unwrap_or_else(|| self.default_subtitle.clone());
        if self.subtitle != new_subtitle {
            let old = self.subtitle.clone();
            self.subtitle = new_subtitle;
            let new = self.subtitle.clone();
            ctx.record_change(
                "subtitle",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(new),
            );
        }
    }

    /// Reactive setter for `show_clock`. Records the change in the provided
    /// [`ReactiveCtx`].
    pub fn set_show_clock(&mut self, value: bool, ctx: &mut ReactiveCtx) {
        if self.show_clock != value {
            let old = self.show_clock;
            self.show_clock = value;
            ctx.record_change(
                "show_clock",
                ReactiveFlags::reactive(),
                Box::new(old),
                Box::new(value),
            );
        }
    }

    /// Reactive setter for `tall`. Records the change in the provided
    /// [`ReactiveCtx`].
    pub fn set_tall(&mut self, value: bool, ctx: &mut ReactiveCtx) {
        if self.tall != value {
            let old = self.tall;
            self.tall = value;
            ctx.record_change(
                "tall",
                ReactiveFlags::reactive_layout(),
                Box::new(old),
                Box::new(value),
            );
        }
    }

    pub fn tall(mut self, tall: bool) -> Self {
        self.tall = tall;
        if tall {
            if !self.seed.classes.contains(&"-tall".to_string()) {
                self.seed.classes.push("-tall".to_string());
            }
        } else {
            self.seed.classes.retain(|c| c != "-tall");
        }
        self
    }

    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = icon.into();
        self
    }

    pub fn show_clock(mut self, show_clock: bool) -> Self {
        self.show_clock = show_clock;
        self
    }

    pub fn time_format(mut self, time_format: impl Into<String>) -> Self {
        self.time_format = time_format.into();
        self
    }
}

impl crate::widgets::Focus for Header {
    fn mouse_interactive(&self) -> bool {
        true
    }
}

impl crate::widgets::Interactive for Header {
    fn on_event(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
        match event {
            Event::MouseDown(mouse) => {
                self.pressed = true;
                // Match Python behavior: header icon lane is not a tall-toggle target.
                self.press_in_toggle_zone = mouse.x > 1;
                ctx.set_handled();
            }
            Event::MouseUp(mouse) => {
                if !self.pressed {
                    return;
                }
                self.pressed = false;
                if mouse.target.is_none() {
                    return;
                }
                let release_in_toggle_zone = mouse.x > 1;
                if self.press_in_toggle_zone && release_in_toggle_zone {
                    self.tall = !self.tall;
                    if self.tall {
                        ctx.add_class("-tall");
                    } else {
                        ctx.remove_class("-tall");
                    }
                    ctx.post_message(HeaderToggled { tall: self.tall });
                    ctx.request_layout_invalidation();
                    ctx.request_repaint();
                }
                ctx.set_handled();
            }
            Event::AppFocus(false) => {
                self.pressed = false;
                self.press_in_toggle_zone = false;
            }
            _ => {}
        }
    }

    fn on_unmount(&mut self) {
        self.pressed = false;
        self.press_in_toggle_zone = false;
    }

    fn on_message(&mut self, message: &crate::message::MessageEvent, ctx: &mut crate::event::WidgetCtx) {
        if let Some(m) = message.downcast_ref::<ScreenTitleChanged>() {
            // Direct field assignment (internal call site — not reactive setter).
            self.title = m
                .title
                .as_deref()
                .map(|s| s.to_string())
                .unwrap_or_else(|| self.default_title.clone());
            self.subtitle = m
                .sub_title
                .as_deref()
                .map(|s| Some(s.to_string()))
                .unwrap_or_else(|| self.default_subtitle.clone());
            ctx.request_repaint();
        }
    }
}

impl crate::widgets::Layout for Header {
    fn layout_height(&self) -> Option<usize> {
        Some(if self.tall { 3 } else { 1 })
    }
}

impl crate::widgets::Render for Header {
    fn compose(&mut self) -> ComposeResult {
        if self.children_extracted {
            return Vec::new();
        }
        self.children_extracted = true;
        let mut children: ComposeResult = Vec::with_capacity(3);
        children.push(crate::compose::ChildDecl::new(Box::new(HeaderIcon::new(
            self.icon.clone(),
        ))));
        children.push(crate::compose::ChildDecl::new(Box::new(HeaderTitle::new(
            self.default_title.clone(),
            self.default_subtitle.clone(),
            self.title.clone(),
            self.subtitle.clone(),
        ))));
        if self.show_clock {
            children.push(crate::compose::ChildDecl::new(Box::new(HeaderClock::new(
                self.time_format.clone(),
            ))));
        } else {
            children.push(crate::compose::ChildDecl::new(Box::new(
                HeaderClockSpace::new(),
            )));
        }
        children
    }

    fn style_type(&self) -> &'static str {
        "Header"
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        // Composition-only header surface; children render icon/title/clock.
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let mut out = Segments::new();
        for row in 0..height {
            out.push(Segment::new(" ".repeat(width)));
            if row + 1 < height {
                out.push(Segment::line());
            }
        }
        out
    }
}

impl crate::widgets::StyleIdentity for Header {
    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        let mut seed = std::mem::take(&mut self.seed);
        if self.tall && !seed.classes.contains(&"-tall".to_string()) {
            seed.classes.push("-tall".to_string());
        }
        seed
    }
}

impl ReactiveWidget for Header {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventCtx;
    use crate::event::{BindingHint, MouseDownEvent, MouseUpEvent};
    use crate::node_id::NodeId;
    use crate::reactive::ReactiveCtx;
    use slotmap::SlotMap;

    fn make_node_id() -> NodeId {
        let mut sm: SlotMap<NodeId, ()> = SlotMap::new();
        sm.insert(())
    }

    #[test]
    fn header_body_click_toggles_tall_and_emits_message() {
        let mut header = Header::new();
        let id = NodeId::default();
        let mut down_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut down_ctx);
            header.on_event(
            &Event::MouseDown(MouseDownEvent {
                x: 9,
                y: 0,
                screen_x: 9,
                screen_y: 0,
                target: id,
            }),
            &mut __w);
        }
        assert!(down_ctx.handled());

        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            header.on_event(
            &Event::MouseUp(MouseUpEvent {
                x: 9,
                y: 0,
                screen_x: 9,
                screen_y: 0,
                target: Some(id),
            }),
            &mut __w);
        }

        assert!(ctx.handled());
        assert!(ctx.repaint_requested());
        assert!(
            ctx.invalidation().layout,
            "tall toggle should request layout invalidation for immediate relayout"
        );
        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].sender, id);
        assert!(messages[0].is::<HeaderToggled>());
        assert!(messages[0].downcast_ref::<HeaderToggled>().unwrap().tall);
    }

    #[test]
    fn header_icon_click_emits_command_palette_action() {
        let mut icon = HeaderIcon::new("⭘");
        let id = NodeId::default();
        let mut down_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut down_ctx);
            icon.on_event(
            &Event::MouseDown(MouseDownEvent {
                x: 0,
                y: 0,
                screen_x: 0,
                screen_y: 0,
                target: id,
            }),
            &mut __w);
        }
        assert!(down_ctx.handled());

        let mut up_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut up_ctx);
            icon.on_event(
            &Event::MouseUp(MouseUpEvent {
                x: 0,
                y: 0,
                screen_x: 0,
                screen_y: 0,
                target: Some(id),
            }),
            &mut __w);
        }

        let messages = up_ctx.take_messages();
        assert_eq!(messages.len(), 2);
        assert!(messages[0].is::<HeaderIconPressed>());
        assert!(messages[1].is::<AppCommandPalette>());
    }

    #[test]
    fn header_compose_uses_python_widget_structure() {
        let mut header = Header::new().title("ModalApp").show_clock(true);
        let children = header.compose();
        assert_eq!(children.len(), 3);
        let types: Vec<&'static str> = children.iter().map(|child| child.widget().style_type()).collect();
        assert_eq!(types, vec!["HeaderIcon", "HeaderTitle", "HeaderClock"]);
        assert!(header.compose().is_empty());
    }

    #[test]
    fn header_compose_uses_clock_space_when_clock_disabled() {
        let mut header = Header::new().title("ModalApp").show_clock(false);
        let children = header.compose();
        let types: Vec<&'static str> = children.iter().map(|child| child.widget().style_type()).collect();
        assert_eq!(types, vec!["HeaderIcon", "HeaderTitle", "HeaderClockSpace"]);
    }

    #[test]
    fn header_tree_mode_toggles_from_child_target_click() {
        let mut header = Header::new();
        let _ = header.compose();
        let child_id = NodeId::default();

        let mut down_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut down_ctx);
            header.on_event(
            &Event::MouseDown(MouseDownEvent {
                x: 4,
                y: 0,
                screen_x: 4,
                screen_y: 0,
                target: child_id,
            }),
            &mut __w);
        }
        assert!(down_ctx.handled());

        let mut up_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut up_ctx);
            header.on_event(
            &Event::MouseUp(MouseUpEvent {
                x: 4,
                y: 0,
                screen_x: 4,
                screen_y: 0,
                target: Some(child_id),
            }),
            &mut __w);
        }
        assert!(up_ctx.handled());
        assert!(up_ctx.invalidation().layout);
        let messages = up_ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].is::<HeaderToggled>());
        assert!(messages[0].downcast_ref::<HeaderToggled>().unwrap().tall);
    }

    #[test]
    fn header_tree_mode_mouse_up_without_press_is_noop() {
        let mut header = Header::new();
        let _ = header.compose();
        let child_id = NodeId::default();
        let mut up_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut up_ctx);
            header.on_event(
            &Event::MouseUp(MouseUpEvent {
                x: 4,
                y: 0,
                screen_x: 4,
                screen_y: 0,
                target: Some(child_id),
            }),
            &mut __w);
        }
        assert!(!up_ctx.handled());
        assert!(up_ctx.take_messages().is_empty());
        assert!(!header.tall);
    }

    #[test]
    fn header_icon_bindings_update_palette_tooltip_and_action_key() {
        let mut icon = HeaderIcon::new("⭘");
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            icon.on_event(
            &Event::BindingsChanged(vec![
                BindingHint::new("f1", "Help"),
                BindingHint::new("ctrl+k", "palette")
                    .with_group("command_palette")
                    .with_tooltip("Open command palette"),
            ]),
            &mut __w);
        }
        assert!(ctx.repaint_requested());
        assert_eq!(icon.command_palette_action_key.as_deref(), Some("ctrl+k"));
        assert_eq!(
            icon.command_palette_tooltip.as_deref(),
            Some("Open command palette")
        );
    }

    #[test]
    fn header_icon_hover_state_drives_hover_selector() {
        use crate::css::{
            default_widget_stylesheet, resolve_style, selector_meta_generic, set_style_context,
        };
        use crate::node_id::node_id_from_ffi;
        use crate::runtime::dispatch_ctx::set_dispatch_recipient;
        use crate::widgets::NodeState;

        let _style_guard = set_style_context(default_widget_stylesheet());
        let icon = HeaderIcon::new("⭘");
        let dummy_id = node_id_from_ffi(1);

        let normal = resolve_style(&icon, &selector_meta_generic(&icon)).bg;
        let hovered = {
            let _hover_guard = set_dispatch_recipient(
                dummy_id,
                NodeState {
                    hovered: true,
                    ..NodeState::default()
                },
            );
            resolve_style(&icon, &selector_meta_generic(&icon)).bg
        };
        assert_ne!(hovered, normal);
        assert!(hovered.is_some());
    }

    #[test]
    fn header_icon_tooltip_anchor_tracks_layout_bottom_row() {
        let mut icon = HeaderIcon::new("⭘");
        icon.on_layout(8, 1);
        assert_eq!(icon.tooltip_anchor(), Some((4, 0)));

        icon.on_layout(8, 3);
        assert_eq!(icon.tooltip_anchor(), Some((4, 2)));
    }

    #[test]
    fn header_icon_click_without_palette_binding_emits_no_palette_action() {
        let mut icon = HeaderIcon::new("⭘");
        let mut bindings_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut bindings_ctx);
            icon.on_event(
            &Event::BindingsChanged(vec![BindingHint::new("f1", "Help")]),
            &mut __w);
        }

        let id = NodeId::default();
        let mut down_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut down_ctx);
            icon.on_event(
            &Event::MouseDown(MouseDownEvent {
                x: 0,
                y: 0,
                screen_x: 0,
                screen_y: 0,
                target: id,
            }),
            &mut __w);
        }
        assert!(down_ctx.handled());

        let mut up_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut up_ctx);
            icon.on_event(
            &Event::MouseUp(MouseUpEvent {
                x: 0,
                y: 0,
                screen_x: 0,
                screen_y: 0,
                target: Some(id),
            }),
            &mut __w);
        }
        let messages = up_ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].is::<HeaderIconPressed>());
    }

    #[test]
    fn header_tall_tree_render_moves_icon_to_middle_row() {
        use crate::runtime::{build_widget_tree_from_root, render_tree_to_frame};

        let mut header = Header::new().title("ModalApp").tall(true);
        let mut tree = build_widget_tree_from_root(&mut header).expect("tree should build");
        let console = rich_rs::Console::new();
        let frame = render_tree_to_frame(&mut tree, &mut header, &console, 40, 3);
        let lines = frame.as_plain_lines();
        assert_eq!(lines.len(), 3);
        assert!(
            !lines[0].contains("⭘"),
            "top row should keep icon lane blank in tall mode"
        );
        assert!(
            lines[1].contains("⭘"),
            "middle row should contain icon in tall mode"
        );
    }

    #[test]
    fn header_mouse_release_outside_target_is_noop() {
        let mut header = Header::new();
        let id = NodeId::default();
        let mut down_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut down_ctx);
            header.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: id,
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut __w);
        }
        assert!(down_ctx.handled());

        let mut up_ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut up_ctx);
            header.on_event(
            &Event::MouseUp(MouseUpEvent {
                x: 12,
                y: 0,
                screen_x: 12,
                screen_y: 0,
                target: None,
            }),
            &mut __w);
        }
        assert!(!up_ctx.handled());
        assert!(up_ctx.take_messages().is_empty());
    }

    #[test]
    fn modal_header_tall_has_no_right_side_ellipsis_artifacts() {
        use crate::runtime::{build_widget_tree_from_root, render_tree_to_frame};
        use crate::widgets::{AppRoot, Footer, Label};

        let mut root = AppRoot::new()
            .with_child(Header::new().title("ModalApp").tall(true))
            .with_child(Label::new("x\n".repeat(40)))
            .with_child(Footer::new());
        let mut tree = build_widget_tree_from_root(&mut root).expect("tree should build");
        let console = rich_rs::Console::new();
        let frame = render_tree_to_frame(&mut tree, &mut root, &console, 80, 12);
        let lines = frame.as_plain_lines();
        for row in 0..3 {
            assert!(
                !lines[row].contains('…'),
                "header row {row} should not show ellipsis artifact: {:?}",
                lines[row]
            );
        }
    }

    // -- P5-14: Screen title inheritance ------------------------------------

    #[test]
    fn set_title_overrides_display() {
        let mut header = Header::new().title("My App");
        let mut ctx = ReactiveCtx::new(make_node_id());
        assert_eq!(header.title, "My App");
        assert_eq!(header.default_title, "My App");

        header.set_title(Some("Settings"), &mut ctx);
        assert_eq!(header.title, "Settings");
        assert_eq!(header.default_title, "My App"); // default unchanged
    }

    #[test]
    fn set_title_none_reverts_to_default() {
        let mut header = Header::new().title("My App");
        let mut ctx = ReactiveCtx::new(make_node_id());
        header.set_title(Some("Settings"), &mut ctx);
        assert_eq!(header.title, "Settings");

        header.set_title(None, &mut ctx);
        assert_eq!(header.title, "My App");
    }

    #[test]
    fn set_subtitle_overrides_display() {
        let mut header = Header::new().subtitle("v1");
        let mut ctx = ReactiveCtx::new(make_node_id());
        assert_eq!(header.subtitle, Some("v1".to_string()));

        header.set_subtitle(Some("v2"), &mut ctx);
        assert_eq!(header.subtitle, Some("v2".to_string()));
        assert_eq!(header.default_subtitle, Some("v1".to_string()));
    }

    #[test]
    fn set_subtitle_none_reverts_to_default() {
        let mut header = Header::new().subtitle("v1");
        let mut ctx = ReactiveCtx::new(make_node_id());
        header.set_subtitle(Some("v2"), &mut ctx);
        header.set_subtitle(None, &mut ctx);
        assert_eq!(header.subtitle, Some("v1".to_string()));
    }

    #[test]
    fn on_message_screen_title_changed_updates_title() {
        use crate::message::MessageEvent;
        use crate::node_id::node_id_from_ffi;

        let mut header = Header::new().title("App").subtitle("Sub");
        let msg = MessageEvent::new(
            node_id_from_ffi(0),
            ScreenTitleChanged {
                title: Some("Screen Title".to_string()),
                sub_title: Some("Screen Sub".to_string()),
            },
        );
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            header.on_message(&msg, &mut __w);
        }

        assert_eq!(header.title, "Screen Title");
        assert_eq!(header.subtitle, Some("Screen Sub".to_string()));
        assert!(ctx.repaint_requested());
    }

    #[test]
    fn on_message_screen_title_none_reverts() {
        use crate::message::MessageEvent;
        use crate::node_id::node_id_from_ffi;

        let mut header = Header::new().title("App").subtitle("Sub");

        // First, override with screen title.
        let msg = MessageEvent::new(
            node_id_from_ffi(0),
            ScreenTitleChanged {
                title: Some("Screen".to_string()),
                sub_title: None,
            },
        );
        let mut ctx = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            header.on_message(&msg, &mut __w);
        }
        assert_eq!(header.title, "Screen");
        assert_eq!(header.subtitle, Some("Sub".to_string())); // reverted to default

        // Then, revert screen title.
        let msg2 = MessageEvent::new(
            node_id_from_ffi(0),
            ScreenTitleChanged {
                title: None,
                sub_title: None,
            },
        );
        let mut ctx2 = EventCtx::default();
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx2);
            header.on_message(&msg2, &mut __w);
        }
        assert_eq!(header.title, "App"); // back to default
    }

    #[test]
    fn header_title_blur_rule_tracks_app_active_state() {
        use crate::css::{
            default_widget_stylesheet, resolve_style, selector_meta_generic, set_app_active,
            set_style_context, with_style_stack,
        };
        use crate::widgets::AppRoot;

        let _style_guard = set_style_context(default_widget_stylesheet());
        let app_root = AppRoot::new();
        let title = HeaderTitle::new("ModalApp", None, "ModalApp", None);

        let active_opacity = {
            let _active_guard = set_app_active(true);
            let app_meta = selector_meta_generic(&app_root);
            let app_style = resolve_style(&app_root, &app_meta);
            with_style_stack(app_meta, app_style, || {
                let title_meta = selector_meta_generic(&title);
                resolve_style(&title, &title_meta).text_opacity
            })
        };

        let blur_opacity = {
            let _active_guard = set_app_active(false);
            let app_meta = selector_meta_generic(&app_root);
            let app_style = resolve_style(&app_root, &app_meta);
            with_style_stack(app_meta, app_style, || {
                let title_meta = selector_meta_generic(&title);
                resolve_style(&title, &title_meta).text_opacity
            })
        };

        assert_ne!(
            active_opacity,
            Some(50),
            "title should not be dimmed while app is focused"
        );
        assert_eq!(
            blur_opacity,
            Some(50),
            "title should dim when app is blurred"
        );
    }
}
