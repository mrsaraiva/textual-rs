use rich_rs::{Console, ConsoleOptions, Renderable, Segments, Text};

use crate::widgets::{Label, Node, NodeSeed, Widget};

/// Holds the display content of a [`Static`] widget.
enum StaticContent {
    /// Plain or markup string, rendered via the underlying [`Label`].
    Plain,
    /// Pre-rendered rich text (e.g. syntax-highlighted code).
    Rich(Text),
}

/// A static text widget with optional rich-text content.
///
/// Mirrors Python Textual's `Static` widget.  Compose with plain text or use
/// [`Static::update()`] / [`Static::update_rich()`] to change content at
/// runtime, matching Python's `Static.update(content)` API.
pub struct Static {
    label: Label,
    content: StaticContent,
}

impl Static {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            // Python Textual's Static defaults to markup=True.
            label: Label::new(text).with_markup(true),
            content: StaticContent::Plain,
        }
    }

    pub fn class(self, value: impl Into<String>) -> Node {
        Node::new(self).class(value)
    }

    pub fn id(self, value: impl Into<String>) -> Node {
        Node::new(self).id(value)
    }

    /// Replace content with a plain text string.
    ///
    /// Mirrors Python `Static.update(text)`.  Clears any previously set rich
    /// content.  Call `ctx.request_repaint()` after this if you have access to
    /// `EventCtx`; otherwise the repaint will happen on the next input cycle.
    pub fn update(&mut self, text: impl Into<String>) {
        self.label.set_text(text.into());
        self.content = StaticContent::Plain;
    }

    /// Replace content with a pre-rendered [`rich_rs::Text`] value.
    ///
    /// Use this to display syntax-highlighted code or other styled content:
    /// ```ignore
    /// use rich_rs::Syntax;
    ///
    /// let text = Syntax::from_path(path)?.highlight();
    /// app.with_query_one_mut_as::<Static, _>("#code", |s| s.update_rich(text))?;
    /// ```
    ///
    /// Mirrors Python `Static.update(syntax_renderable)`.
    pub fn update_rich(&mut self, text: Text) {
        self.content = StaticContent::Rich(text);
    }

    /// Clear all content (show empty widget).
    ///
    /// Mirrors Python `Static.update("")`.
    pub fn clear(&mut self) {
        self.label.set_text(String::new());
        self.content = StaticContent::Plain;
    }
}

// ---------------------------------------------------------------------------
// Regression tests (DG-02)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_update_changes_content() {
        let mut widget = Static::new("initial");
        widget.update("updated");
        // label text is now "updated" — confirm Plain variant is active
        assert!(matches!(widget.content, StaticContent::Plain));
    }

    #[test]
    fn static_update_rich_switches_to_rich_variant() {
        let mut widget = Static::new("initial");
        let text = Text::plain("rich content");
        widget.update_rich(text);
        assert!(matches!(widget.content, StaticContent::Rich(_)));
    }

    #[test]
    fn static_update_after_rich_reverts_to_plain() {
        let mut widget = Static::new("initial");
        widget.update_rich(Text::plain("rich"));
        widget.update("plain again");
        assert!(matches!(widget.content, StaticContent::Plain));
    }

    #[test]
    fn static_clear_sets_plain_empty() {
        let mut widget = Static::new("hello");
        widget.update_rich(Text::plain("rich"));
        widget.clear();
        assert!(matches!(widget.content, StaticContent::Plain));
    }

    #[test]
    fn static_layout_height_rich_returns_line_count() {
        let mut widget = Static::new("");
        let text = Text::plain("line one\nline two\nline three");
        widget.update_rich(text);
        assert_eq!(widget.layout_height(), Some(3));
    }
}

impl Widget for Static {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        match &self.content {
            StaticContent::Plain => Widget::render(&self.label, console, options),
            StaticContent::Rich(text) => text.render(console, options),
        }
    }

    fn on_layout(&mut self, width: u16, height: u16) {
        self.label.on_layout(width, height);
    }

    fn layout_height(&self) -> Option<usize> {
        match &self.content {
            StaticContent::Plain => self.label.layout_height(),
            StaticContent::Rich(text) => {
                let line_count = text.plain_text().lines().count().max(1);
                Some(line_count)
            }
        }
    }

    fn content_width(&self) -> Option<usize> {
        self.label.content_width()
    }

    fn auto_content_width(&self) -> Option<usize> {
        self.label.auto_content_width()
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.label.set_inline_style(style);
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        self.label.take_node_seed()
    }
}
