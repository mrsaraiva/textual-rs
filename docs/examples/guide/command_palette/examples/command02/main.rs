/// Port of Python Textual `docs/examples/guide/command_palette/command02.py`.
///
/// Demonstrates a custom command provider that discovers Python files in the
/// current working directory and offers them as "open <path>" commands.
/// Selecting a command opens the file with syntax highlighting in a viewer.
use rich_rs::Syntax;
use textual::message::CommandPaletteCommand;
use textual::prelude::*;
use textual::textual_app::CommandPaletteProvider;

/// Custom message sent when the user selects a file to open.
#[derive(Debug, Clone)]
struct OpenFile {
    pub path: String,
}
textual::impl_message!(OpenFile);

/// Command provider that discovers Python files in the current working directory.
struct PythonFileCommandsProvider {
    python_paths: Vec<String>,
}

impl PythonFileCommandsProvider {
    fn new() -> Self {
        Self {
            python_paths: Vec::new(),
        }
    }
}

impl CommandPaletteProvider for PythonFileCommandsProvider {
    fn startup(&mut self, _ctx: &mut EventCtx) {
        // Mirror Python: scan cwd for *.py files.
        self.python_paths = std::fs::read_dir(".")
            .into_iter()
            .flat_map(|entries| entries.flatten())
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("py") {
                    path.to_str().map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect();
        self.python_paths.sort();
    }

    fn commands(&mut self) -> Vec<CommandPaletteCommand> {
        self.python_paths
            .iter()
            .map(|path| {
                // Mirror Python: command title is "open <path>".
                let title = format!("open {path}");
                CommandPaletteCommand {
                    id: path.clone(),
                    title,
                    help: "Open this file in the viewer".to_string(),
                }
            })
            .collect()
    }

    fn on_command_selected(&mut self, command_id: &str, ctx: &mut EventCtx) {
        // command_id is the file path; post a message so the app can update the UI.
        ctx.post_message(OpenFile {
            path: command_id.to_string(),
        });
    }
}

struct ViewerApp;

impl TextualApp for ViewerApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            VerticalScroll::new().with_child(
                Static::new("").with_expand(true).id("code"),
            ),
        )
    }

    fn command_palette_providers(&mut self) -> Vec<Box<dyn CommandPaletteProvider>> {
        vec![Box::new(PythonFileCommandsProvider::new())]
    }

    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut EventCtx) {
        if let Some(OpenFile { path }) = message.downcast_ref::<OpenFile>() {
            let path = path.clone();
            // Load and syntax-highlight the file, then update the #code Static widget.
            match Syntax::from_path(&path) {
                Ok(syntax) => {
                    let syntax = syntax
                        .with_line_numbers(true)
                        .with_word_wrap(false)
                        .with_indent_guides(true)
                        .with_theme("github-dark");
                    let text = syntax.highlight();
                    let _ = app.with_query_one_mut_as::<Static, _>("#code", |s| {
                        s.update_rich(text);
                    });
                }
                Err(_) => {
                    let _ = app.with_query_one_mut_as::<Static, _>("#code", |s| {
                        s.update(format!("Could not read file: {path}"));
                    });
                }
            }
            ctx.request_repaint();
            ctx.set_handled();
        }
    }
}

fn main() -> Result<()> {
    run_sync(ViewerApp)
}
