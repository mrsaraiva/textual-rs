/// Port of Python Textual's `docs/examples/guide/input/key02.py`.
///
/// Displays key events in a RichLog. Each key press writes the event details
/// (key, character, name, is_printable) to the log. The space key also rings
/// the terminal bell.
use textual::keys::KeyEventData;
use textual::prelude::*;

#[derive(Clone, Default)]
struct InputApp;

fn write_key_event(log: &mut RichLog, key: &KeyEventData) {
    let key_str = key.name().to_string();
    // Python's Key.name is _key_to_identifier(self.key).lower() — the identifier form.
    let name_str = key.identifier();
    let character = key
        .character
        .map(|ch| format!("'{ch}'"))
        .unwrap_or_else(|| "None".to_string());
    let printable = if key.is_printable { "True" } else { "False" };

    // Python's Key.__rich_repr__ yields: key, character, name, is_printable, aliases.
    // The `aliases` field has default=[self.key], so Rich only shows it when
    // aliases differ from [key] (i.e. the key has extra aliases like enter/return).
    let aliases = key.aliases();
    let show_aliases = aliases != vec![key.name()];

    let mut repr = format!(
        "Key(key='{key_str}', character={character}, name='{name_str}', is_printable={printable}"
    );
    if show_aliases {
        let aliases_str = format!(
            "[{}]",
            aliases
                .iter()
                .map(|a| format!("'{a}'"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        repr.push_str(&format!(", aliases={aliases_str}"));
    }
    repr.push(')');
    // Python: `RichLog.write(event)` wraps the Key event in `Pretty`, coloured
    // by rich's `ReprHighlighter` (ANSI-standard colours mapped to the terminal
    // theme at paint time). Mirror that path — no hardcoded colours.
    log.write_pretty(repr);
}

impl TextualApp for InputApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(RichLog::new())
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut textual::event::WidgetCtx) {
        // Space key rings the bell (Python: key_space -> self.bell())
        if key.name() == "space" {
            app.action_bell();
        }

        // Write key event details to the RichLog for every key press
        let _ = app.with_query_one_mut_as::<RichLog, _>("RichLog", |log| {
            write_key_event(log, key);
        });

        ctx.request_repaint();
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
    /// `RichLog` and change the rendered frame. Guards on_key -> RichLog write.
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

            // The space key additionally rings the bell — and still logs a line.
            pilot.press(&["space"])?;
            let after_space = pilot.app().frame_fingerprint();
            assert_ne!(
                after, after_space,
                "pressing space must also append a log line (and ring the bell)"
            );
            Ok(())
        })
        .unwrap();
    }
}
