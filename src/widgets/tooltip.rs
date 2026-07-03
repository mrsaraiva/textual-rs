use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::node_id::NodeId;
use crate::style::parse_color_like;

use super::{NodeSeed, Widget};

pub const SYSTEM_TOOLTIP_STYLE_ID: &str = "textual-tooltip";

/// The system tooltip bubble.
///
/// A port of Python `textual.widgets._tooltip.Tooltip` (a `Static` subclass).
/// It is a real node on the shared widget path: a `position: absolute;
/// overlay: screen` bubble that the screen mounts once and the runtime shows,
/// positions (via the node's `absolute_offset` anchor + CSS `offset-x: -50%`)
/// and constrains (via the `overlay: screen` deferred-paint escape, which
/// honors `constrain: inside inflect`). It has no focus, no dismiss result and
/// no children — a transient hover overlay, not a screen.
///
/// The widget owns only its text + visibility + the owner it is currently
/// describing. All geometry (size, centering, viewport constraint) is done by
/// layout + the overlay:screen paint pass, not by the widget.
pub struct Tooltip {
    text: String,
    visible: bool,
    /// The node whose tooltip this bubble is currently showing (used by the
    /// hover path to keep the anchor stable while the pointer stays over the
    /// same owner, and to re-anchor when it changes).
    system_owner: Option<NodeId>,
    /// Last width the bubble was laid out at, used to count wrapped lines for
    /// `height: auto` (mirrors the `Static`/`Label` `layout_width` pattern).
    layout_width: usize,
    seed: NodeSeed,
}

impl Tooltip {
    crate::seed_ident_methods!();

    /// Create a tooltip bubble holding `text`.
    pub fn new(text: impl Into<String>) -> Self {
        let mut seed = NodeSeed::default();
        seed.classes.push("tooltip".to_string());
        seed.classes.push("-textual-system".to_string());
        Self {
            text: text.into(),
            visible: false,
            system_owner: None,
            // Seed with the default content max-width (max-width 40 minus the
            // `padding: 1 2` horizontal chrome) so the first-frame `height: auto`
            // measurement wraps close to the eventual laid-out width; `on_layout`
            // then refines it.
            layout_width: 36,
            seed,
        }
    }

    /// The single system tooltip mounted by the runtime on every screen.
    pub fn system() -> Self {
        Self::new("")
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// The node this bubble is currently describing (runtime hover path).
    pub(crate) fn system_owner(&self) -> Option<NodeId> {
        self.system_owner
    }

    /// Update the bubble to describe `owner` with `text` and mark it visible.
    /// Returns `true` when anything changed. Positioning (the mouse anchor) is
    /// applied separately by the runtime via the node's `absolute_offset`.
    pub(crate) fn apply_system_state(&mut self, owner: NodeId, text: String) -> bool {
        let mut changed = false;
        if self.system_owner != Some(owner) {
            self.system_owner = Some(owner);
            changed = true;
        }
        if self.text != text {
            self.text = text;
            changed = true;
        }
        if !self.visible {
            self.visible = true;
            changed = true;
        }
        changed
    }

    /// Hide the bubble and forget its owner. Returns `true` when anything
    /// changed.
    pub(crate) fn hide_system(&mut self) -> bool {
        let mut changed = false;
        if self.visible {
            self.visible = false;
            changed = true;
        }
        if self.system_owner.take().is_some() {
            changed = true;
        }
        changed
    }

    /// Resolve the text style (`Tooltip > .tooltip--text` component, falling back
    /// to `$foreground`). Padding + background come from the node render path
    /// (resolved `padding`/`bg`), so this is fg-only.
    fn text_style(&self) -> rich_rs::Style {
        crate::css::resolve_component_style(self, &["tooltip--text"])
            .to_rich()
            .unwrap_or_else(|| {
                if let Some(fg) = parse_color_like("$foreground") {
                    rich_rs::Style::new().with_color(fg.to_simple_opaque())
                } else {
                    rich_rs::Style::new()
                }
            })
    }

    /// Natural (unwrapped) content width: the widest hard line. CSS `max-width`
    /// caps the outer box in layout; `height: auto` then wraps at the resulting
    /// content width.
    fn intrinsic_content_width(&self) -> usize {
        self.text
            .lines()
            .map(rich_rs::cell_len)
            .max()
            .unwrap_or(0)
            .max(1)
    }

    fn wrap_text(text: &str, width: usize) -> Vec<String> {
        let width = width.max(1);
        let mut out = Vec::new();

        for source_line in text.lines() {
            let mut current = String::new();
            for word in source_line.split_whitespace() {
                let word_width = rich_rs::cell_len(word);
                if current.is_empty() {
                    if word_width <= width {
                        current.push_str(word);
                    } else {
                        let mut chunk = String::new();
                        for ch in word.chars() {
                            chunk.push(ch);
                            if rich_rs::cell_len(&chunk) >= width {
                                out.push(chunk.clone());
                                chunk.clear();
                            }
                        }
                        if !chunk.is_empty() {
                            current.push_str(&chunk);
                        }
                    }
                    continue;
                }

                let with_space = format!("{current} {word}");
                if rich_rs::cell_len(&with_space) <= width {
                    current = with_space;
                } else {
                    out.push(current);
                    current = String::new();
                    if word_width <= width {
                        current.push_str(word);
                    } else {
                        let mut chunk = String::new();
                        for ch in word.chars() {
                            chunk.push(ch);
                            if rich_rs::cell_len(&chunk) >= width {
                                out.push(chunk.clone());
                                chunk.clear();
                            }
                        }
                        if !chunk.is_empty() {
                            current.push_str(&chunk);
                        }
                    }
                }
            }

            if current.is_empty() {
                out.push(String::new());
            } else {
                out.push(current);
            }
        }

        if out.is_empty() {
            out.push(String::new());
        }

        out
    }
}

impl Widget for Tooltip {
    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        if !self.visible {
            return Segments::new();
        }
        let text = self.text.trim();
        if text.is_empty() {
            return Segments::new();
        }
        let width = options.size.0.max(1);
        let height = options.size.1.max(1);
        let text_style = self.text_style();

        let mut lines = Self::wrap_text(text, width);
        if lines.len() > height {
            lines.truncate(height);
        }
        let line_count = lines.len();
        let mut out = Segments::new();
        for (idx, line) in lines.into_iter().enumerate() {
            out.push(Segment::styled(
                rich_rs::set_cell_size(&line, width),
                text_style,
            ));
            if idx + 1 < line_count {
                out.push(Segment::line());
            }
        }
        out
    }

    fn on_layout(&mut self, width: u16, _height: u16) {
        // Hidden/disconnected nodes can transiently receive width=0/1 during
        // display toggles; keep the last stable width so wrapped-height stays
        // stable (mirrors `Static`).
        if width > 1 {
            self.layout_width = usize::from(width);
        }
    }

    fn auto_content_width(&self) -> Option<usize> {
        Some(self.intrinsic_content_width())
    }

    fn layout_height(&self) -> Option<usize> {
        // Outer box height = wrapped line count (at the laid-out content width)
        // + resolved vertical chrome (padding/border). Matches the `Static`
        // convention `measure_intrinsic_content_height` relies on for `height:
        // auto` (chrome is NOT re-added by the caller).
        let lines =
            super::text::intrinsic_wrapped_height(&self.text, self.layout_width.max(1), true);
        let chrome = super::helpers::resolved_vertical_chrome(self);
        Some(lines.saturating_add(chrome))
    }

    fn on_unmount(&mut self) {
        self.visible = false;
        self.system_owner = None;
    }

    fn focusable(&self) -> bool {
        false
    }

    fn mouse_interactive(&self) -> bool {
        false
    }

    fn style_type(&self) -> &'static str {
        "Tooltip"
    }

    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }

    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

impl Renderable for Tooltip {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rich_rs::ConsoleOptions;

    #[test]
    fn hidden_tooltip_renders_nothing() {
        let tooltip = Tooltip::new("tip");
        let console = Console::new();
        let out = Widget::render(&tooltip, &console, console.options());
        assert!(out.is_empty());
    }

    #[test]
    fn visible_tooltip_renders_wrapped_text() {
        let mut tooltip = Tooltip::new("");
        let changed = tooltip.apply_system_state(NodeId::default(), "hello world".to_string());
        assert!(changed);
        assert!(tooltip.is_visible());

        let console = Console::new();
        let mut options: ConsoleOptions = console.options().clone();
        options.size = (11, 1);
        let out = Widget::render(&tooltip, &console, &options);
        let text: String = out.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("hello world"));
    }

    #[test]
    fn apply_and_hide_toggle_visibility_and_owner() {
        let mut tooltip = Tooltip::system();
        let owner = NodeId::default();
        assert!(tooltip.apply_system_state(owner, "tip".to_string()));
        assert!(tooltip.is_visible());
        assert_eq!(tooltip.system_owner(), Some(owner));

        // Re-applying the same owner/text/visibility is a no-op.
        assert!(!tooltip.apply_system_state(owner, "tip".to_string()));

        assert!(tooltip.hide_system());
        assert!(!tooltip.is_visible());
        assert_eq!(tooltip.system_owner(), None);
    }

    #[test]
    fn auto_width_tracks_widest_line() {
        let tooltip = Tooltip::new("short\nmuch longer line");
        assert_eq!(
            tooltip.auto_content_width(),
            Some("much longer line".len())
        );
    }
}
