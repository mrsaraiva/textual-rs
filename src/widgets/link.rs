use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments, StyleMeta};

use crate::event::{Action, Event, EventCtx};
use crate::message::*;

use crate::node_id::NodeId;

use super::{
    Widget, WidgetStyles,
    helpers::{empty_classes, fixed_height_from_constraints},
};

/// A simple clickable text widget that posts a message with a URL when activated.
///
/// Renders as a single line of text; CSS provides underline/color styling.
/// Activated via click or Enter key when focused.
#[derive(Debug, Clone)]
pub struct Link {
    text: String,
    url: String,
    /// Optional tooltip text (Python Textual parity).
    ///
    /// Note: tooltip rendering infrastructure is not yet implemented in the framework;
    /// this field stores the value for API parity and future use.
    tooltip: Option<String>,
    focused: bool,
    hovered: bool,
    pressed: bool,
    classes: Vec<String>,
    focused_classes: Vec<String>,
    styles: WidgetStyles,
}

impl Link {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let url_str = text.clone();
        Self {
            text,
            url: url_str,
            tooltip: None,
            focused: false,
            hovered: false,
            pressed: false,
            classes: vec!["link".to_string()],
            focused_classes: vec!["link".to_string(), "focused".to_string()],
            styles: WidgetStyles::default(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// Get the tooltip text, if set.
    pub fn tooltip(&self) -> Option<&str> {
        self.tooltip.as_deref()
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }

    pub fn set_url(&mut self, url: impl Into<String>) {
        self.url = url.into();
    }

    /// Set the tooltip text. Pass `None` to clear.
    pub fn set_tooltip(&mut self, tooltip: Option<impl Into<String>>) {
        self.tooltip = tooltip.map(|t| t.into());
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = url.into();
        self
    }

    /// Builder-style tooltip setter.
    pub fn with_tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    fn activate(&mut self, ctx: &mut EventCtx) {
        if !self.url.is_empty() {
            // Attempt to open the URL in the default browser/handler.
            if let Err(err) = open::that(&self.url) {
                eprintln!("Link: failed to open URL {:?}: {}", self.url, err);
            }
            ctx.post_message(Message::LinkClicked(LinkClicked {
                url: self.url.clone(),
            }));
        }
        ctx.request_repaint();
        ctx.set_handled();
    }
}

impl Widget for Link {
    fn focusable(&self) -> bool {
        true
    }

    fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    fn has_focus(&self) -> bool {
        self.focused
    }

    fn is_hovered(&self) -> bool {
        self.hovered
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    fn is_active(&self) -> bool {
        self.pressed && self.hovered
    }

    fn content_width(&self) -> Option<usize> {
        Some(rich_rs::cell_len(&self.text).max(1))
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            // TODO(P1-14 integration): wire tree-based NodeId comparison
            Event::MouseDown(mouse) if mouse.target == NodeId::default() => {
                self.pressed = true;
                ctx.request_repaint();
                ctx.set_handled();
            }
            Event::MouseUp(mouse) => {
                if self.pressed {
                    self.pressed = false;
                    ctx.request_repaint();
                    // TODO(P1-14 integration): wire tree-based NodeId comparison
                    if mouse.target == Some(NodeId::default()) {
                        self.activate(ctx);
                        return;
                    }
                }
            }
            Event::AppFocus(false) => {
                if self.pressed {
                    self.pressed = false;
                    ctx.request_repaint();
                }
            }
            Event::Action(Action::Toggle) if self.focused => {
                self.activate(ctx);
                return;
            }
            Event::Key(key) if self.focused => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.activate(ctx);
                    return;
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let line = rich_rs::set_cell_size(&self.text, width);
        let mut out = Segments::new();
        let mut segment = Segment::styled(
            line,
            crate::css::resolve_component_style(self, &["link"])
                .to_rich()
                .unwrap_or_else(rich_rs::Style::new),
        );
        if !self.url.is_empty() {
            // Hyperlink policy: set URL only and rely on rich-rs per-Console link-id registry.
            segment.meta = Some(StyleMeta::with_link(self.url.clone()));
        }
        out.push(segment);
        out
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(Some(1))
    }

    fn style_classes(&self) -> &[String] {
        if self.focused {
            &self.focused_classes
        } else if self.classes.is_empty() {
            empty_classes()
        } else {
            &self.classes
        }
    }

    fn style_type(&self) -> &'static str {
        "Link"
    }

    fn styles(&self) -> Option<&WidgetStyles> {
        Some(&self.styles)
    }

    fn styles_mut(&mut self) -> Option<&mut WidgetStyles> {
        Some(&mut self.styles)
    }
}

impl Renderable for Link {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::KeyEventData;

    #[test]
    fn default_url_equals_text() {
        let link = Link::new("https://example.com");
        assert_eq!(link.text(), "https://example.com");
        assert_eq!(link.url(), "https://example.com");
    }

    #[test]
    fn with_url_overrides() {
        let link = Link::new("Click here").with_url("https://example.com");
        assert_eq!(link.text(), "Click here");
        assert_eq!(link.url(), "https://example.com");
    }

    #[test]
    fn set_text_and_url() {
        let mut link = Link::new("initial");
        link.set_text("new text");
        link.set_url("https://new.url");
        assert_eq!(link.text(), "new text");
        assert_eq!(link.url(), "https://new.url");
    }

    #[test]
    fn tooltip_default_is_none() {
        let link = Link::new("text");
        assert_eq!(link.tooltip(), None);
    }

    #[test]
    fn with_tooltip_builder() {
        let link = Link::new("text").with_tooltip("hover me");
        assert_eq!(link.tooltip(), Some("hover me"));
    }

    #[test]
    fn set_tooltip() {
        let mut link = Link::new("text");
        link.set_tooltip(Some("tip"));
        assert_eq!(link.tooltip(), Some("tip"));
        link.set_tooltip(Option::<&str>::None);
        assert_eq!(link.tooltip(), None);
    }

    #[test]
    fn focusable() {
        let link = Link::new("text");
        assert!(link.focusable());
    }

    #[test]
    fn focus_state() {
        let mut link = Link::new("text");
        assert!(!link.has_focus());
        link.set_focus(true);
        assert!(link.has_focus());
        link.set_focus(false);
        assert!(!link.has_focus());
    }

    #[test]
    fn hover_state() {
        let mut link = Link::new("text");
        assert!(!link.is_hovered());
        link.set_hovered(true);
        assert!(link.is_hovered());
    }

    #[test]
    fn content_width_matches_text() {
        let link = Link::new("hello");
        assert_eq!(link.content_width(), Some(5));
    }

    #[test]
    fn content_width_min_1() {
        let link = Link::new("");
        assert_eq!(link.content_width(), Some(1));
    }

    #[test]
    fn layout_height_is_1() {
        let link = Link::new("text");
        assert_eq!(link.layout_height(), Some(1));
    }

    #[test]
    fn style_type_is_link() {
        let link = Link::new("text");
        assert_eq!(link.style_type(), "Link");
    }

    #[test]
    fn render_sets_hyperlink_meta_without_explicit_link_id() {
        let link = Link::new("Click here").with_url("https://example.com");
        let console = Console::new();
        let mut options = console.options().clone();
        options.size = (20, 1);
        options.max_width = 20;
        options.max_height = 1;

        let rendered = Widget::render(&link, &console, &options);
        let first = rendered
            .iter()
            .find(|segment| segment.control.is_none())
            .expect("expected text segment");
        let meta = first.meta.as_ref().expect("expected hyperlink metadata");
        assert_eq!(meta.link.as_deref(), Some("https://example.com"));
        assert!(meta.link_id.is_none());
    }

    #[test]
    fn focused_classes_include_focused() {
        let mut link = Link::new("text");
        link.set_focus(true);
        assert!(link.style_classes().iter().any(|c| c == "focused"));
    }

    #[test]
    fn unfocused_classes_exclude_focused() {
        let link = Link::new("text");
        assert!(!link.style_classes().iter().any(|c| c == "focused"));
    }

    #[test]
    fn activate_posts_link_clicked() {
        let mut link = Link::new("text").with_url("https://example.com");
        let mut ctx = EventCtx::default();
        link.activate(&mut ctx);
        assert!(ctx.handled());
        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);
        match &messages[0].message {
            Message::LinkClicked(LinkClicked { url }) => assert_eq!(url, "https://example.com"),
            other => panic!("expected LinkClicked, got {:?}", other),
        }
    }

    #[test]
    fn activate_empty_url_no_message() {
        let mut link = Link::new("text");
        link.set_url("");
        let mut ctx = EventCtx::default();
        link.activate(&mut ctx);
        let messages = ctx.take_messages();
        assert!(messages.is_empty());
    }

    fn make_key_event(code: KeyCode) -> Event {
        Event::Key(KeyEventData::from_crossterm(
            crossterm::event::KeyEvent::new(code, crossterm::event::KeyModifiers::NONE),
        ))
    }

    #[test]
    fn key_enter_activates_when_focused() {
        let mut link = Link::new("text").with_url("https://example.com");
        link.set_focus(true);
        let mut ctx = EventCtx::default();
        let event = make_key_event(KeyCode::Enter);
        link.on_event(&event, &mut ctx);
        assert!(ctx.handled());
        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);
        match &messages[0].message {
            Message::LinkClicked(LinkClicked { url }) => assert_eq!(url, "https://example.com"),
            other => panic!("expected LinkClicked, got {:?}", other),
        }
    }

    #[test]
    fn key_space_activates_when_focused() {
        let mut link = Link::new("text").with_url("https://example.com");
        link.set_focus(true);
        let mut ctx = EventCtx::default();
        let event = make_key_event(KeyCode::Char(' '));
        link.on_event(&event, &mut ctx);
        assert!(ctx.handled());
    }

    #[test]
    fn key_enter_ignored_when_unfocused() {
        let mut link = Link::new("text").with_url("https://example.com");
        let mut ctx = EventCtx::default();
        let event = make_key_event(KeyCode::Enter);
        link.on_event(&event, &mut ctx);
        assert!(!ctx.handled());
    }
}
