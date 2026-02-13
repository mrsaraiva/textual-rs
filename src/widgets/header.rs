use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rich_rs::{Console, ConsoleOptions, Renderable, Segment, Segments};

use crate::event::{Event, EventCtx};
use crate::message::*;

use crate::node_id::NodeId;

use super::helpers::{adjust_line_length_no_bg, empty_classes, fixed_height_from_constraints};
use super::{Widget, WidgetStyles};

#[derive(Debug, Clone)]
pub struct Header {
    title: String,
    subtitle: Option<String>,
    /// The default (app-level) title, used as fallback when no screen title is active.
    default_title: String,
    /// The default (app-level) subtitle, used as fallback when no screen subtitle is active.
    default_subtitle: Option<String>,
    tall: bool,
    hovered: bool,
    icon: String,
    icon_hover: bool,
    pressed_icon: Option<bool>,
    icon_width: usize,
    clock_width: usize,
    show_clock: bool,
    time_format: String,
    last_clock_second: Arc<AtomicU64>,
    classes: Vec<String>,
    styles: WidgetStyles,
}

impl Header {
    pub fn new() -> Self {
        Self {
            title: "textual-rs".to_string(),
            subtitle: None,
            default_title: "textual-rs".to_string(),
            default_subtitle: None,
            tall: false,
            hovered: false,
            icon: "⭘".to_string(),
            icon_hover: false,
            pressed_icon: None,
            icon_width: 8,
            clock_width: 10,
            show_clock: false,
            time_format: "%X".to_string(),
            last_clock_second: Arc::new(AtomicU64::new(0)),
            classes: Vec::new(),
            styles: WidgetStyles::default(),
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

    /// Update the displayed title at runtime (e.g. from a screen title).
    ///
    /// Pass `None` to revert to the default (app-level) title.
    pub fn set_title(&mut self, title: Option<&str>) {
        self.title = title
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.default_title.clone());
    }

    /// Update the displayed subtitle at runtime (e.g. from a screen sub-title).
    ///
    /// Pass `None` to revert to the default (app-level) subtitle.
    pub fn set_subtitle(&mut self, subtitle: Option<&str>) {
        self.subtitle = subtitle
            .map(|s| Some(s.to_string()))
            .unwrap_or_else(|| self.default_subtitle.clone());
    }

    pub fn tall(mut self, tall: bool) -> Self {
        self.tall = tall;
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

    fn line_text(&self) -> String {
        match &self.subtitle {
            Some(subtitle) if !subtitle.is_empty() => format!(" {} — {}", self.title, subtitle),
            _ => format!(" {}", self.title),
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

    fn component_style(&self, classes: &[&str]) -> rich_rs::Style {
        let style = crate::css::resolve_component_style(self, classes);
        if style.is_empty() {
            rich_rs::Style::new()
        } else {
            style.to_rich().unwrap_or_else(rich_rs::Style::new)
        }
    }
}

impl Widget for Header {
    fn style_type(&self) -> &'static str {
        "Header"
    }

    fn mouse_interactive(&self) -> bool {
        true
    }

    fn on_mouse_move(&mut self, x: u16, _y: u16) -> bool {
        if !self.hovered {
            return false;
        }
        let new_hover = (x as usize) < self.icon_width;
        if new_hover != self.icon_hover {
            self.icon_hover = new_hover;
            return true;
        }
        false
    }

    fn on_event(&mut self, event: &Event, ctx: &mut EventCtx) {
        match event {
            Event::MouseDown(mouse) => {
                // TODO(P1-14 integration): wire tree-based NodeId comparison
                if mouse.target != NodeId::default() {
                    return;
                }
                self.pressed_icon = Some((mouse.x as usize) < self.icon_width);
                ctx.set_handled();
            }
            Event::MouseUp(mouse) => {
                // TODO(P1-14 integration): wire tree-based NodeId comparison
                if mouse.target != Some(NodeId::default()) {
                    self.pressed_icon = None;
                    return;
                }
                let released_on_icon = (mouse.x as usize) < self.icon_width;
                let Some(pressed_icon) = self.pressed_icon.take() else {
                    return;
                };
                if pressed_icon != released_on_icon {
                    ctx.set_handled();
                    return;
                }
                if released_on_icon {
                    // Parity with Python Header: icon click is handled separately and
                    // shouldn't toggle header height.
                    ctx.post_message(Message::HeaderIconPressed(HeaderIconPressed));
                    ctx.set_handled();
                    return;
                }
                self.tall = !self.tall;
                ctx.post_message(Message::HeaderToggled(HeaderToggled { tall: self.tall }));
                ctx.request_repaint();
                ctx.set_handled();
            }
            Event::AppFocus(false) => {
                self.pressed_icon = None;
                if self.hovered || self.icon_hover {
                    self.hovered = false;
                    self.icon_hover = false;
                    ctx.request_repaint();
                }
            }
            _ => {}
        }
    }

    fn on_unmount(&mut self) {
        self.hovered = false;
        self.icon_hover = false;
        self.pressed_icon = None;
    }

    fn on_message(&mut self, message: &crate::message::MessageEvent, ctx: &mut EventCtx) {
        if let Message::ScreenTitleChanged(ScreenTitleChanged {
            ref title,
            ref sub_title,
        }) = message.message
        {
            self.set_title(title.as_deref());
            self.set_subtitle(sub_title.as_deref());
            ctx.request_repaint();
        }
    }

    fn render(&self, _console: &Console, options: &ConsoleOptions) -> Segments {
        let width = options.size.0.max(1);
        let line_text = self.line_text();
        let icon_text = format!(" {} ", self.icon);
        let icon_core_width = rich_rs::cell_len(&icon_text);
        let icon_text = if icon_core_width >= self.icon_width {
            rich_rs::set_cell_size(&icon_text, self.icon_width)
        } else {
            format!(
                "{}{}",
                icon_text,
                " ".repeat(self.icon_width.saturating_sub(icon_core_width))
            )
        };
        let clock_seconds = Self::current_clock_seconds();
        self.last_clock_second
            .store(clock_seconds, Ordering::Relaxed);
        let right_label = if self.show_clock {
            self.format_clock(clock_seconds)
        } else {
            String::new()
        };
        let right_text = {
            let right_width = self.clock_width.min(width.saturating_sub(1));
            let clipped = rich_rs::set_cell_size(&right_label, right_width);
            if rich_rs::cell_len(&clipped) >= right_width {
                clipped
            } else {
                format!(
                    "{}{}",
                    clipped,
                    " ".repeat(right_width - rich_rs::cell_len(&clipped))
                )
            }
        };
        let center_width = width
            .saturating_sub(self.icon_width.min(width))
            .saturating_sub(rich_rs::cell_len(&right_text))
            .max(1);
        let title_width = rich_rs::cell_len(&line_text).min(center_width);
        let left_pad = center_width.saturating_sub(title_width) / 2;
        let right_pad = center_width.saturating_sub(title_width + left_pad);
        let center_text = format!(
            "{}{}{}",
            " ".repeat(left_pad),
            rich_rs::set_cell_size(&line_text, title_width),
            " ".repeat(right_pad)
        );

        let icon_style = if self.icon_hover {
            self.component_style(&["header--icon", "-hover"])
        } else {
            self.component_style(&["header--icon"])
        };
        let title_style = self.component_style(&["header--title"]);
        let clock_style = self.component_style(&["header--clock"]);

        let mut first_line = Vec::new();
        first_line.push(Segment::styled(icon_text, icon_style));
        first_line.push(Segment::styled(center_text, title_style));
        first_line.push(Segment::styled(right_text, clock_style));
        let first_line = adjust_line_length_no_bg(&first_line, width);

        let mut out = Segments::new();
        out.extend(first_line);
        if self.tall {
            for _ in 0..2 {
                out.push(Segment::line());
                out.push(Segment::new(" ".repeat(width)));
            }
        }
        out
    }

    fn layout_height(&self) -> Option<usize> {
        fixed_height_from_constraints(self.layout_constraints()).or(Some(if self.tall {
            3
        } else {
            1
        }))
    }

    fn style_classes(&self) -> &[String] {
        if self.tall {
            static TALL: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
            TALL.get_or_init(|| vec!["-tall".to_string()])
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

    fn is_hovered(&self) -> bool {
        self.hovered
    }

    fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
        if !hovered {
            self.icon_hover = false;
        }
    }

    fn is_active(&self) -> bool {
        if !self.show_clock {
            return false;
        }
        let current = Self::current_clock_seconds();
        current != self.last_clock_second.load(Ordering::Relaxed)
    }
}

impl Renderable for Header {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{MouseDownEvent, MouseUpEvent};
    use crate::node_id::NodeId;

    #[test]
    fn header_body_click_toggles_tall_and_emits_message() {
        let mut header = Header::new();
        let id = NodeId::default(); // TODO(P1-14 integration): use WidgetTree-assigned NodeId
        let mut down_ctx = EventCtx::default();
        header.on_event(
            &Event::MouseDown(MouseDownEvent {
                x: 9,
                y: 0,
                screen_x: 9,
                screen_y: 0,
                target: id,
            }),
            &mut down_ctx,
        );
        assert!(down_ctx.handled());

        let mut ctx = EventCtx::default();
        header.on_event(
            &Event::MouseUp(MouseUpEvent {
                x: 9,
                y: 0,
                screen_x: 9,
                screen_y: 0,
                target: Some(id),
            }),
            &mut ctx,
        );

        assert!(ctx.handled());
        assert!(ctx.repaint_requested());
        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].sender, id);
        assert!(matches!(
            messages[0].message,
            Message::HeaderToggled(HeaderToggled { tall: true })
        ));
    }

    #[test]
    fn header_icon_click_does_not_emit_toggle_message() {
        let mut header = Header::new();
        let id = NodeId::default(); // TODO(P1-14 integration): use WidgetTree-assigned NodeId
        let mut down_ctx = EventCtx::default();
        header.on_event(
            &Event::MouseDown(MouseDownEvent {
                x: 0,
                y: 0,
                screen_x: 0,
                screen_y: 0,
                target: id,
            }),
            &mut down_ctx,
        );
        assert!(down_ctx.handled());

        let mut ctx = EventCtx::default();
        header.on_event(
            &Event::MouseUp(MouseUpEvent {
                x: 0,
                y: 0,
                screen_x: 0,
                screen_y: 0,
                target: Some(id),
            }),
            &mut ctx,
        );

        assert!(ctx.handled());
        let messages = ctx.take_messages();
        assert_eq!(messages.len(), 1);
        assert!(matches!(messages[0].message, Message::HeaderIconPressed(_)));
    }

    #[test]
    fn header_hover_leave_clears_icon_hover_state() {
        let mut header = Header::new();
        header.set_hovered(true);
        assert!(header.on_mouse_move(0, 0));
        assert!(header.icon_hover);

        header.set_hovered(false);
        assert!(!header.is_hovered());
        assert!(!header.icon_hover);
    }

    #[test]
    fn header_unmount_clears_hover_state() {
        let mut header = Header::new();
        header.set_hovered(true);
        header.on_mouse_move(0, 0);
        assert!(header.hovered);
        assert!(header.icon_hover);

        header.on_unmount();
        assert!(!header.hovered);
        assert!(!header.icon_hover);
    }

    #[test]
    fn header_cross_region_press_release_is_noop() {
        let mut header = Header::new();
        let id = NodeId::default(); // TODO(P1-14 integration): use WidgetTree-assigned NodeId
        let mut down_ctx = EventCtx::default();
        header.on_event(
            &Event::MouseDown(MouseDownEvent {
                target: id,
                screen_x: 0,
                screen_y: 0,
                x: 0,
                y: 0,
            }),
            &mut down_ctx,
        );
        assert!(down_ctx.handled());

        let mut up_ctx = EventCtx::default();
        header.on_event(
            &Event::MouseUp(MouseUpEvent {
                x: 12,
                y: 0,
                screen_x: 12,
                screen_y: 0,
                target: Some(id),
            }),
            &mut up_ctx,
        );
        assert!(up_ctx.handled());
        assert!(up_ctx.take_messages().is_empty());
    }

    // -- P5-14: Screen title inheritance ------------------------------------

    #[test]
    fn set_title_overrides_display() {
        let mut header = Header::new().title("My App");
        assert_eq!(header.title, "My App");
        assert_eq!(header.default_title, "My App");

        header.set_title(Some("Settings"));
        assert_eq!(header.title, "Settings");
        assert_eq!(header.default_title, "My App"); // default unchanged
    }

    #[test]
    fn set_title_none_reverts_to_default() {
        let mut header = Header::new().title("My App");
        header.set_title(Some("Settings"));
        assert_eq!(header.title, "Settings");

        header.set_title(None);
        assert_eq!(header.title, "My App");
    }

    #[test]
    fn set_subtitle_overrides_display() {
        let mut header = Header::new().subtitle("v1");
        assert_eq!(header.subtitle, Some("v1".to_string()));

        header.set_subtitle(Some("v2"));
        assert_eq!(header.subtitle, Some("v2".to_string()));
        assert_eq!(header.default_subtitle, Some("v1".to_string()));
    }

    #[test]
    fn set_subtitle_none_reverts_to_default() {
        let mut header = Header::new().subtitle("v1");
        header.set_subtitle(Some("v2"));
        header.set_subtitle(None);
        assert_eq!(header.subtitle, Some("v1".to_string()));
    }

    #[test]
    fn on_message_screen_title_changed_updates_title() {
        use crate::message::MessageEvent;
        use crate::node_id::node_id_from_ffi;

        let mut header = Header::new().title("App").subtitle("Sub");
        let msg = MessageEvent {
            sender: node_id_from_ffi(0),
            message: Message::ScreenTitleChanged(ScreenTitleChanged {
                title: Some("Screen Title".to_string()),
                sub_title: Some("Screen Sub".to_string()),
            }),
        };
        let mut ctx = EventCtx::default();
        header.on_message(&msg, &mut ctx);

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
        let msg = MessageEvent {
            sender: node_id_from_ffi(0),
            message: Message::ScreenTitleChanged(ScreenTitleChanged {
                title: Some("Screen".to_string()),
                sub_title: None,
            }),
        };
        let mut ctx = EventCtx::default();
        header.on_message(&msg, &mut ctx);
        assert_eq!(header.title, "Screen");
        assert_eq!(header.subtitle, Some("Sub".to_string())); // reverted to default

        // Then, revert screen title.
        let msg2 = MessageEvent {
            sender: node_id_from_ffi(0),
            message: Message::ScreenTitleChanged(ScreenTitleChanged {
                title: None,
                sub_title: None,
            }),
        };
        let mut ctx2 = EventCtx::default();
        header.on_message(&msg2, &mut ctx2);
        assert_eq!(header.title, "App"); // back to default
    }
}
