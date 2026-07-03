/// Port of Python Textual `docs/examples/guide/input/key01.py`.
///
/// Displays a RichLog that writes key event info on each key press.
use rich_rs::{Segment, Style as RichStyle};
use textual::keys::KeyEventData;
use textual::prelude::*;

#[derive(Clone, Default)]
struct InputApp;

impl TextualApp for InputApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(RichLog::new())
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut textual::event::WidgetCtx) {
        let key_name = key.name().to_string();
        let character = key.character;
        let is_printable = key.is_printable;

        let _ = app.with_query_one_mut_as::<RichLog, _>("RichLog", |log| {
            let key_style =
                RichStyle::new().with_color(Color::parse("#b73763").unwrap().to_simple_opaque());
            let field_style =
                RichStyle::new().with_color(Color::parse("#f5a623").unwrap().to_simple_opaque());
            let value_style =
                RichStyle::new().with_color(Color::parse("#98d168").unwrap().to_simple_opaque());
            let bool_style = RichStyle::new()
                .with_color(Color::parse("#b73763").unwrap().to_simple_opaque())
                .with_italic(true);

            let char_display = character
                .map(|ch| format!("'{ch}'"))
                .unwrap_or_else(|| "None".to_string());
            let printable_display = if is_printable { "True" } else { "False" };

            log.write_segments(vec![
                Segment::styled("Key".to_string(), key_style),
                Segment::new("(".to_string()),
                Segment::styled("key".to_string(), field_style),
                Segment::new("=".to_string()),
                Segment::styled(format!("'{key_name}'"), value_style),
                Segment::new(", ".to_string()),
                Segment::styled("character".to_string(), field_style),
                Segment::new("=".to_string()),
                Segment::styled(char_display, value_style),
                Segment::new(", ".to_string()),
                Segment::styled("name".to_string(), field_style),
                Segment::new("=".to_string()),
                Segment::styled(format!("'{key_name}'"), value_style),
                Segment::new(", ".to_string()),
                Segment::styled("is_printable".to_string(), field_style),
                Segment::new("=".to_string()),
                Segment::styled(printable_display.to_string(), bool_style),
                Segment::new(")".to_string()),
            ]);
        });

        ctx.request_repaint();
        ctx.set_handled();
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync(InputApp::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LIVENESS PROBE: pressing a key must write a `Key(...)` line into the
    /// `RichLog`, changing the rendered frame. Guards the on_key -> RichLog
    /// write -> repaint path.
    #[test]
    fn liveness_keypress_writes_to_richlog_and_changes_frame() {
        textual::run_test(InputApp::default(), |pilot| {
            let before = pilot.app().frame_fingerprint();
            pilot.press(&["a"])?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "pressing a key must write to the RichLog and change the frame"
            );

            // A second, different keypress writes another line — frame changes again.
            pilot.press(&["b"])?;
            let after3 = pilot.app().frame_fingerprint();
            assert_ne!(after, after3, "a second keypress must append another log line");
            Ok(())
        })
        .unwrap();
    }
}
