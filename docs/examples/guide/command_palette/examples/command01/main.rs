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
