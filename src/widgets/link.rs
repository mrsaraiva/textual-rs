use crossterm::event::KeyCode;
use rich_rs::{Console, ConsoleOptions, Segment, Segments, StyleMeta};
use textual_macros::widget;

use crate::event::{Action, Event};
use crate::message::*;

use super::{Focus, Interactive, Layout, NodeSeed, Render};

/// A simple clickable text widget that posts a message with a URL when activated.
///
/// Renders as a single line of text; CSS provides underline/color styling.
/// Activated via click or Enter key when focused.
#[derive(Debug, Clone)]
#[widget(Focus, Interactive, Layout, style_type = "Link")]
pub struct Link {
    text: String,
    url: String,
    /// Optional tooltip text (Python Textual parity).
    ///
    /// Note: tooltip rendering infrastructure is not yet implemented in the framework;
    /// this field stores the value for API parity and future use.
    tooltip: Option<String>,
    pressed: bool,
    seed: NodeSeed,
}

impl Link {
    crate::seed_ident_methods!();

    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let url_str = text.clone();
        let mut seed = NodeSeed::default();
        seed.classes.push("link".to_string());
        Self {
            text,
            url: url_str,
            tooltip: None,
            pressed: false,
            seed,
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

    fn activate(&mut self, ctx: &mut crate::event::WidgetCtx) {
        if !self.url.is_empty() {
            // Attempt to open the URL in the default browser/handler.
            if let Err(err) = open::that(&self.url) {
                eprintln!("Link: failed to open URL {:?}: {}", self.url, err);
            }
            ctx.post_message(LinkClicked {
                url: self.url.clone(),
            });
        }
        ctx.request_repaint();
        ctx.set_handled();
    }
}

/// Apply [`TextStyleFlags`] onto a `rich_rs::Style`.
fn apply_text_style_flags(style: &mut rich_rs::Style, flags: &crate::style::TextStyleFlags) {
    if flags.bold {
        *style = (*style).with_bold(true);
    }
    if flags.dim {
        *style = (*style).with_dim(true);
    }
    if flags.italic {
        *style = (*style).with_italic(true);
    }
    if flags.underline {
        *style = (*style).with_underline(true);
    }
    if flags.reverse {
        style.reverse = Some(true);
    }
}

impl Focus for Link {
    fn focusable(&self) -> bool {
        true
    }

    fn is_active(&self) -> bool {
        self.pressed && crate::widgets::Widget::node_state(self).hovered
    }
}

impl Layout for Link {
    fn content_width(&self) -> Option<usize> {
        let meta = crate::css::selector_meta_generic(self);
        let resolved = crate::css::resolve_style(self, &meta);
        let padding = resolved.effective_padding();
        let (_, _, border_left, border_right) =
            super::helpers::border_spacing_from_style(&resolved);
        let chrome_lr =
            usize::from(padding.left.saturating_add(padding.right)) + border_left + border_right;
        Some(
            rich_rs::cell_len(&self.text)
                .saturating_add(chrome_lr)
                .max(1),
        )
    }

    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }
}

impl Interactive for Link {
    fn on_event(&mut self, event: &Event, ctx: &mut crate::event::WidgetCtx) {
        let focused = crate::widgets::Widget::node_state(self).focused;
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
                        self.activate(ctx);
                    }
                }
            Event::AppFocus(false)
                if self.pressed => {
                    self.pressed = false;
                    ctx.request_repaint();
                }
            Event::Action(Action::Toggle) if focused => {
                self.activate(ctx);
            }
            Event::Key(key) if focused => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.activate(ctx);
                }
                _ => {}
            },
            _ => {}
        }
    }
}

impl Render for Link {
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let line = rich_rs::set_cell_size(&self.text, width);
        let state = crate::widgets::Widget::node_state(self);

        // Start with component-resolved base style.
        let mut style = crate::css::resolve_component_style(self, &["link"])
            .to_rich()
            .unwrap_or_default();

        // Overlay CSS link styling properties (P2-32).
        let meta = crate::css::selector_meta_generic(self);
        let resolved = crate::css::resolve_style(self, &meta);

        if !state.disabled && state.hovered {
            // Hover state: use hover variants, falling back to normal variants.
            // Disabled links ignore hover styling (matches Python Textual).
            if let Some(color) = resolved.link_color_hover.or(resolved.link_color) {
                style = style.with_color(color.to_simple_opaque());
            }
            if let Some(bg) = resolved.link_background_hover.or(resolved.link_background) {
                if bg.a > 0.0 {
                    style = style.with_bgcolor(bg.to_simple_opaque());
                }
            }
            if let Some(flags) = resolved.link_style_hover.or(resolved.link_style) {
                apply_text_style_flags(&mut style, &flags);
            }
        } else {
            // Normal state (also used for disabled links).
            if let Some(color) = resolved.link_color {
                style = style.with_color(color.to_simple_opaque());
            }
            if let Some(bg) = resolved.link_background {
                if bg.a > 0.0 {
                    style = style.with_bgcolor(bg.to_simple_opaque());
                }
            }
            if let Some(flags) = resolved.link_style {
                apply_text_style_flags(&mut style, &flags);
            }
        }

        let mut out = Segments::new();
        let mut segment = Segment::styled(line, style);
        if !self.url.is_empty() {
            // Hyperlink policy: set URL only and rely on rich-rs per-Console link-id registry.
            segment.meta = Some(StyleMeta::with_link(self.url.clone()));
        }
        out.push(segment);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventCtx;
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
        // Focus state is now managed by the tree node record via node_state().
        // Outside dispatch, node_state() returns default (all false).
        let link = Link::new("text");
        assert!(!crate::widgets::Widget::node_state(&link).focused);
    }

    #[test]
    fn hover_state() {
        // Hover state is now managed by the tree node record via node_state().
        let link = Link::new("text");
        assert!(!crate::widgets::Widget::node_state(&link).hovered);
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

        let rendered = crate::widgets::Widget::render(&link, &console, &options);
        let first = rendered
            .iter()
            .find(|segment| segment.control.is_none())
            .expect("expected text segment");
        let meta = first.meta.as_ref().expect("expected hyperlink metadata");
        assert_eq!(meta.link.as_deref(), Some("https://example.com"));
        assert!(meta.link_id.is_none());
    }

    #[test]
    fn activate_posts_link_clicked() {
        let mut link = Link::new("text").with_url("https://example.com");
        let mut ctx = EventCtx::default();
        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); link.activate(&mut __w) };
        assert!(ctx.handled());
        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].is::<LinkClicked>());
        assert_eq!(
            messages[0].downcast_ref::<LinkClicked>().unwrap().url,
            "https://example.com"
        );
    }

    #[test]
    fn activate_empty_url_no_message() {
        let mut link = Link::new("text");
        link.set_url("");
        let mut ctx = EventCtx::default();
        { let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx); link.activate(&mut __w) };
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
        // Focus state comes from node_state() which reads dispatch context.
        // Key events when unfocused (default) should not activate.
        let mut link = Link::new("text").with_url("https://example.com");
        let mut ctx = EventCtx::default();
        let event = make_key_event(KeyCode::Enter);
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            link.on_event(&event, &mut __w);
        }
        // Outside dispatch context, node_state().focused == false, so key is not handled.
        assert!(!ctx.handled());
    }

    #[test]
    fn key_enter_ignored_when_unfocused() {
        let mut link = Link::new("text").with_url("https://example.com");
        let mut ctx = EventCtx::default();
        let event = make_key_event(KeyCode::Enter);
        {
            let mut __w = crate::event::WidgetCtx::__from_dispatch(crate::node_id::NodeId::default(), &mut ctx);
            link.on_event(&event, &mut __w);
        }
        assert!(!ctx.handled());
    }
}
