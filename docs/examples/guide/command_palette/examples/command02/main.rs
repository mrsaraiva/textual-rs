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
///
/// Mirrors Python's `PythonFileCommands.read_files()` (which globs `*.py` in the
/// cwd). The file discovery is factored into [`read_files`](Self::read_files) so
/// it can be seeded deterministically: when constructed with an explicit path
/// list (see [`with_paths`](Self::with_paths)), that fixed list is used instead
/// of scanning the filesystem. Production (`main`) uses the scanning default;
/// the headless test seeds a fixed list so results are stable and reproducible.
struct PythonFileCommandsProvider {
    python_paths: Vec<String>,
    /// When `Some`, `startup` uses this fixed list instead of scanning the cwd.
    /// Used to make the provider deterministic under headless tests.
    seeded_paths: Option<Vec<String>>,
}

impl PythonFileCommandsProvider {
    /// Production constructor: discovers `*.py` files by scanning the cwd on
    /// startup (Python parity).
    fn new() -> Self {
        Self {
            python_paths: Vec::new(),
            seeded_paths: None,
        }
    }

    /// Test constructor: serve a fixed, deterministic list of file paths instead
    /// of scanning the filesystem. Mirrors overriding Python's `read_files()`.
    fn with_paths(paths: Vec<String>) -> Self {
        Self {
            python_paths: Vec::new(),
            seeded_paths: Some(paths),
        }
    }

    /// Discover the candidate file paths. Mirrors Python's `read_files()`:
    /// returns the seeded list when one was provided, else globs `*.py` in cwd.
    fn read_files(&self) -> Vec<String> {
        if let Some(seeded) = &self.seeded_paths {
            return seeded.clone();
        }
        let mut paths: Vec<String> = std::fs::read_dir(".")
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
        paths.sort();
        paths
    }
}

impl CommandPaletteProvider for PythonFileCommandsProvider {
    fn startup(&mut self, _ctx: &mut textual::event::WidgetCtx) {
        // Mirror Python: discover the candidate paths once when the palette opens.
        self.python_paths = self.read_files();
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

    fn on_command_selected(&mut self, command_id: &str, ctx: &mut textual::event::WidgetCtx) {
        // command_id is the file path; post a message so the app can update the UI.
        ctx.post_message(OpenFile {
            path: command_id.to_string(),
        });
    }
}

#[derive(Default)]
struct ViewerApp {
    /// When set, the command provider serves this fixed path list instead of
    /// scanning the cwd. Used by the headless test for deterministic results.
    seeded_paths: Option<Vec<String>>,
}

impl TextualApp for ViewerApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            VerticalScroll::new().with_child(
                Static::new("").with_expand(true).id("code"),
            ),
        )
    }

    fn command_palette_providers(&mut self) -> Vec<Box<dyn CommandPaletteProvider>> {
        let provider = match &self.seeded_paths {
            Some(paths) => PythonFileCommandsProvider::with_paths(paths.clone()),
            None => PythonFileCommandsProvider::new(),
        };
        vec![Box::new(provider)]
    }

    fn on_message_with_app(&mut self, app: &mut App, message: &MessageEvent, ctx: &mut textual::event::WidgetCtx) {
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
    run_sync(ViewerApp::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LIVENESS PROBE: drives the full command-palette flow headlessly and
    /// asserts the file-open effect end-to-end.
    ///
    /// The `PythonFileCommandsProvider` is seeded with a deterministic path
    /// (a real `.py` fixture written to a unique temp dir) so results are stable
    /// and reproducible regardless of the test cwd. The probe then:
    ///   1. opens the palette (Ctrl+P) — provider `startup` discovers the seeded
    ///      path and contributes an "open <path>" command,
    ///   2. types a query that matches the seeded file,
    ///   3. presses Enter to select it.
    ///
    /// Selecting the command posts `OpenFile`, which `on_message_with_app` turns
    /// into a syntax-highlighted `update_rich` of the `#code` Static. The
    /// assertions confirm the palette opened (frame changed + `CommandPalette`
    /// present), then that selecting the command closed the palette and updated
    /// the viewer (frame changed again, proving the `#code` update rendered).
    #[test]
    fn liveness_open_file_updates_code_viewer() {
        // Seed a real, readable `.py` fixture in a unique temp dir so
        // `Syntax::from_path` succeeds deterministically.
        let dir = std::env::temp_dir().join(format!(
            "command02_fixture_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("create fixture dir");
        let fixture = dir.join("example.py");
        std::fs::write(&fixture, "def greet(name):\n    return f\"Hello, {name}!\"\n")
            .expect("write fixture");
        let fixture_path = fixture.to_str().expect("utf-8 path").to_string();

        let app = ViewerApp {
            seeded_paths: Some(vec![fixture_path.clone()]),
        };

        let result = textual::run_test_sized(app, 80, 24, |pilot| {
            // The #code viewer target must exist.
            assert!(
                pilot.app().query_one("#code").is_ok(),
                "#code Static must be present"
            );

            // 1) Open the palette.
            let before_open = pilot.app().frame_fingerprint();
            pilot.press(&["ctrl+p"])?;
            let palette_count = pilot
                .app()
                .query("CommandPalette")
                .map(|q| q.into_ids().len())
                .unwrap_or(0);
            assert_eq!(
                palette_count, 1,
                "Ctrl+P must open (mount) the CommandPalette"
            );
            assert_ne!(
                before_open,
                pilot.app().frame_fingerprint(),
                "opening the command palette must change the rendered frame"
            );

            // 2) Type a query that matches the seeded "open <path>" command.
            let opened = pilot.app().frame_fingerprint();
            pilot.press(&["e", "x", "a", "m", "p", "l", "e"])?;

            // 3) Select the (top) matching command.
            pilot.press(&["enter"])?;

            // Selecting a command closes the palette: the overlay disappears, so
            // the frame must differ from the open-palette frame.
            assert_ne!(
                opened,
                pilot.app().frame_fingerprint(),
                "selecting a command (Enter) must change the rendered frame \
                 (palette closes + #code viewer updates)"
            );

            // The selection must also have updated the viewer relative to the
            // pre-open state (the `#code` Static now shows highlighted source),
            // proving the OpenFile -> on_message_with_app -> #code path fired.
            assert_ne!(
                before_open,
                pilot.app().frame_fingerprint(),
                "selecting a file must update the #code viewer (rendered frame changed)"
            );

            Ok(())
        });

        // Clean up the fixture regardless of test outcome.
        let _ = std::fs::remove_dir_all(&dir);
        result.unwrap();
    }
}
