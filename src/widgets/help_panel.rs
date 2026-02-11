use rich_rs::{Console, ConsoleOptions, Renderable, Segments};

use crate::event::{BindingHint, Event, EventCtx};
use crate::message::{Message, MessageEvent};
use crate::render::FrameBuffer;

use super::{
    FooterBinding, KeyPanel, Markdown, Overlay, Widget, WidgetId, WidgetRenderable, WidgetStyles,
    helpers::{empty_classes, fixed_height_from_constraints},
};

/// Context-sensitive help panel baseline.
///
/// Composes an optional markdown help section above a `KeyPanel` and relies on the shared
/// overlay compositor for deterministic layer composition.
#[derive(Debug)]
pub struct HelpPanel {
    id: WidgetId,
    markdown: Markdown,
    key_panel: KeyPanel,
    show_help: bool,
    app_active: bool,
    help_markup: String,
    classes: Vec<String>,
    classes_with_help: Vec<String>,
    styles: WidgetStyles,
}

impl HelpPanel {
    pub fn new() -> Self {
        Self {
            id: WidgetId::new(),
            markdown: Markdown::new(""),
            key_panel: KeyPanel::new().title("Keys"),
            show_help: false,
            app_active: true,
            help_markup: String::new(),
            classes: vec!["help-panel".to_string(), "-textual-system".to_string()],
            classes_with_help: vec![
                "help-panel".to_string(),
                "-textual-system".to_string(),
                "-show-help".to_string(),
            ],
            styles: WidgetStyles::default(),
        }
    }

    pub fn with_help(mut self, markup: impl Into<String>) -> Self {
        self.set_help(markup);
        self
    }

    pub fn set_help(&mut self, markup: impl Into<String>) {
        self.help_markup = markup.into();
        self.show_help = !self.help_markup.trim().is_empty();
        self.markdown.set_markup(self.help_markup.clone());
    }

    pub fn clear_help(&mut self) {
        self.help_markup.clear();
        self.show_help = false;
        self.markdown.set_markup("");
    }

    pub fn help(&self) -> &str {
        &self.help_markup
    }

    pub fn showing_help(&self) -> bool {
        self.show_help
    }

    pub fn with_bindings(mut self, bindings: Vec<FooterBinding>) -> Self {
        self.key_panel.set_bindings(bindings);
        self
    }

    pub fn set_bindings(&mut self, bindings: Vec<FooterBinding>) {
        self.key_panel.set_bindings(bindings);
    }

    pub fn set_binding_hints(&mut self, hints: &[BindingHint]) {
        self.key_panel.set_binding_hints(hints);
    }

    fn split_heights(&self, width: usize, height: usize) -> (usize, usize) {
        let height = height.max(1);
        if !self.show_help || !self.app_active {
            return (0, height);
        }

        let markdown_intrinsic = self.markdown.layout_height().unwrap_or(1).max(1);
        let max_help = (height / 2).max(1);
        let help_height = markdown_intrinsic
            .min(max_help)
            .min(height.saturating_sub(1).max(1));
        let keys_height = height.saturating_sub(help_height).max(1);
        let _ = width;
        (help_height, keys_height)
    }
}

impl Widget for HelpPanel {
    fn id(&self) -> WidgetId {
        self.id
    }

    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);

        let mut merged = FrameBuffer::new(width, height, None);
        let (help_height, keys_height) = self.split_heights(width, height);

        if help_height > 0 {
            let mut help_options = options.clone();
            help_options.size = (width, help_height);
            help_options.max_width = width;
            help_options.max_height = help_height;
            let help_renderable = WidgetRenderable::new(&self.markdown);
            let help = FrameBuffer::from_renderable(console, &help_options, &help_renderable, None);
            Overlay::compose_overlay_at(&mut merged, &help, 0, 0);
        }

        let mut keys_options = options.clone();
        keys_options.size = (width, keys_height);
        keys_options.max_width = width;
        keys_options.max_height = keys_height;
        let keys_renderable = WidgetRenderable::new(&self.key_panel);
        let keys = FrameBuffer::from_renderable(console, &keys_options, &keys_renderable, None);
        Overlay::compose_overlay_at(&mut merged, &keys, 0, help_height);

        merged.to_segments()
    }

    fn layout_height(&self) -> Option<usize> {
        if let Some(fixed) = fixed_height_from_constraints(self.layout_constraints()) {
            return Some(fixed);
        }
        if self.show_help {
            let markdown_height = self.markdown.layout_height().unwrap_or(1).max(1);
            let key_panel_height = self.key_panel.layout_height().unwrap_or(1).max(1);
            return Some(markdown_height.saturating_add(key_panel_height));
        }
        self.key_panel.layout_height().or(Some(1))
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        let width = usize::from(width).max(1);
        let height = usize::from(height).max(1);
        let (help_height, keys_height) = self.split_heights(width, height);
        self.markdown.on_layout(width as u16, help_height as u16);
        self.key_panel.on_layout(width as u16, keys_height as u16);
    }

    fn on_resize(&mut self, width: u16, height: u16) {
        let width_usize = usize::from(width).max(1);
        let height_usize = usize::from(height).max(1);
        let (help_height, keys_height) = self.split_heights(width_usize, height_usize);
        self.markdown.on_resize(width, help_height as u16);
        self.key_panel.on_resize(width, keys_height as u16);
    }

    fn on_mount(&mut self) {
        self.markdown.on_mount();
        self.key_panel.on_mount();
    }

    fn on_unmount(&mut self) {
        self.app_active = true;
        self.markdown.on_unmount();
        self.key_panel.on_unmount();
    }

    fn on_tick(&mut self, tick: u64) {
        self.markdown.on_tick(tick);
        self.key_panel.on_tick(tick);
    }

    fn on_event_capture(&mut self, event: &Event, ctx: &mut EventCtx) {
        self.markdown.on_event_capture(event, ctx);
        if !ctx.handled() {
            self.key_panel.on_event_capture(event, ctx);
        }
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        if let Event::AppFocus(active) = event {
            if self.app_active != *active {
                self.app_active = *active;
                ctx.request_repaint();
            }
        }

        self.key_panel.on_event(event, ctx);
        if !ctx.handled() {
            self.markdown.on_event(event, ctx);
        }

        if let Event::BindingsChanged(hints) = event {
            self.key_panel.set_binding_hints(hints);
            ctx.request_repaint();
        }
    }

    fn on_message(&mut self, message: &MessageEvent, ctx: &mut EventCtx) {
        match &message.message {
            Message::HelpPanelSetHelp { panel, markup } if *panel == self.id => {
                self.set_help(markup.clone());
                ctx.request_repaint();
                ctx.set_handled();
                return;
            }
            Message::HelpPanelClearHelp { panel } if *panel == self.id => {
                self.clear_help();
                ctx.request_repaint();
                ctx.set_handled();
                return;
            }
            _ => {}
        }

        self.markdown.on_message(message, ctx);
        if !ctx.handled() {
            self.key_panel.on_message(message, ctx);
        }
    }

    fn mouse_interactive(&self) -> bool {
        true
    }

    fn style_type(&self) -> &'static str {
        "HelpPanel"
    }

    fn style_classes(&self) -> &[String] {
        if self.show_help && self.app_active {
            &self.classes_with_help
        } else if self.classes.is_empty() {
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

    fn visit_children_mut(&mut self, f: &mut dyn FnMut(&mut dyn Widget)) {
        f(&mut self.markdown);
        f(&mut self.key_panel);
    }
}

impl Renderable for HelpPanel {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_panel_toggles_show_help_class() {
        let mut panel = HelpPanel::new();
        assert!(!panel.showing_help());
        assert!(
            !panel
                .style_classes()
                .iter()
                .any(|class| class == "-show-help")
        );

        panel.set_help("# Help\nSome text");
        assert!(panel.showing_help());
        assert!(
            panel
                .style_classes()
                .iter()
                .any(|class| class == "-show-help")
        );
    }

    #[test]
    fn help_panel_clear_help_removes_show_help_class() {
        let mut panel = HelpPanel::new().with_help("## Help\nbody");
        assert!(panel.showing_help());
        assert!(
            panel
                .style_classes()
                .iter()
                .any(|class| class == "-show-help")
        );

        panel.clear_help();
        assert!(!panel.showing_help());
        assert!(
            !panel
                .style_classes()
                .iter()
                .any(|class| class == "-show-help")
        );
    }

    #[test]
    fn help_panel_split_caps_help_to_half_height() {
        let panel = HelpPanel::new().with_help("line1\nline2\nline3\nline4");
        let (help_height, keys_height) = panel.split_heights(40, 6);
        assert_eq!(help_height, 3);
        assert_eq!(keys_height, 3);
    }

    #[test]
    fn help_panel_split_keeps_key_panel_visible_in_short_heights() {
        let panel = HelpPanel::new().with_help("line1\nline2");
        let (help_height, keys_height) = panel.split_heights(40, 2);
        assert_eq!(help_height, 1);
        assert_eq!(keys_height, 1);
    }
}
