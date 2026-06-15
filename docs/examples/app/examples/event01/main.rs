/// Port of Python Textual `docs/examples/app/event01.py`.
///
/// Demonstrates basic event handling:
/// - On mount, the screen background is set to darkblue.
/// - Pressing digit keys 0-9 changes the screen background to the corresponding
///   color from the COLORS list.
use textual::prelude::*;

const COLORS: &[&str] = &[
    "white",
    "maroon",
    "red",
    "purple",
    "fuchsia",
    "olive",
    "yellow",
    "navy",
    "teal",
    "aqua",
];

struct EventApp;

fn color_for_name(name: &str) -> Option<Color> {
    textual::style::parse_color_like(name)
}

impl TextualApp for EventApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
    }

    fn on_mount_with_app(&mut self, app: &mut App, ctx: &mut EventCtx) {
        if let Some(color) = color_for_name("darkblue") {
            if let Ok(q) = app.query_mut("Screen") {
                q.set_styles(|styles| styles.set_bg(color));
            }
        }
        ctx.request_repaint();
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut EventCtx) {
        let name = key.name();
        if name.len() == 1 {
            let ch = name.chars().next().unwrap();
            if ch.is_ascii_digit() {
                let idx = (ch as u8 - b'0') as usize;
                if idx < COLORS.len() {
                    if let Some(color) = color_for_name(COLORS[idx]) {
                        if let Ok(q) = app.query_mut("Screen") {
                            q.set_styles(|styles| styles.set_bg(color));
                        }
                        ctx.set_handled();
                        ctx.request_repaint();
                    }
                }
            }
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(EventApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_list_has_ten_entries() {
        assert_eq!(COLORS.len(), 10);
        assert_eq!(COLORS[0], "white");
        assert_eq!(COLORS[9], "aqua");
    }

    #[test]
    fn event_app_composes_without_panic() {
        let mut app = EventApp;
        let _root = app.compose();
    }

    #[test]
    fn darkblue_parses_to_a_color() {
        assert!(color_for_name("darkblue").is_some());
    }
}
