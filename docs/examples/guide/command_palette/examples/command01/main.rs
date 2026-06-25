/// Port of Python Textual `docs/examples/guide/command_palette/command01.py`.
///
/// Demonstrates extending the command palette with a custom "Bell" command.
/// Opens the command palette with Ctrl+P; selecting "Bell" rings the terminal bell.
use textual::message::CommandPaletteCommand;
use textual::prelude::*;
use textual::textual_app::CommandPaletteProvider;

/// Provider that contributes a single "Bell" command to the command palette.
struct BellProvider;

impl CommandPaletteProvider for BellProvider {
    fn commands(&mut self) -> Vec<CommandPaletteCommand> {
        vec![CommandPaletteCommand {
            id: "bell".to_string(),
            title: "Bell".to_string(),
            help: "Ring the bell".to_string(),
        }]
    }

    fn on_command_selected(&mut self, command_id: &str, ctx: &mut EventCtx) {
        if command_id == "bell" {
            ctx.post_message(textual::message::AppBell);
        }
    }
}

struct BellCommandApp;

impl TextualApp for BellCommandApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
    }

    fn command_palette_providers(&mut self) -> Vec<Box<dyn CommandPaletteProvider>> {
        vec![Box::new(BellProvider)]
    }
}

fn main() -> Result<()> {
    run_sync(BellCommandApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LIVENESS PROBE: pressing Ctrl+P must open the command palette and change
    /// the rendered frame (the palette overlay + search box appear over the
    /// app). The custom `BellProvider` contributes a "Bell" command; pressing
    /// Enter selects it (posting `AppBell`). Guards the ctrl+p -> palette open
    /// -> provider startup/commands path. (The `CommandPalette` node is
    /// pre-mounted in the TextualApp runtime root and toggled open by Ctrl+P, so
    /// the liveness signal is the visible frame change on open, not a mount.)
    #[test]
    fn liveness_ctrl_p_opens_palette_with_bell_command() {
        textual::run_test_sized(BellCommandApp, 60, 20, |pilot| {
            let before = pilot.app().frame_fingerprint();

            pilot.press(&["ctrl+p"])?;

            assert_ne!(
                before,
                pilot.app().frame_fingerprint(),
                "opening the command palette (Ctrl+P) must change the rendered frame"
            );

            // The provider's "Bell" command is selectable; Enter selects it and
            // posts AppBell. This must not panic (and typically closes the
            // palette, changing the frame again).
            let opened = pilot.app().frame_fingerprint();
            pilot.press(&["enter"])?;
            assert_ne!(
                opened,
                pilot.app().frame_fingerprint(),
                "selecting a command (Enter) must change the rendered frame (palette closes)"
            );
            Ok(())
        })
        .unwrap();
    }
}
