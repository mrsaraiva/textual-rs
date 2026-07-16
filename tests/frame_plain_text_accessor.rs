//! Public plain-text frame accessors: `App::frame_plain_lines` and
//! `App::frame_plain_text`. A dev-tooling harness must be able to read the
//! currently rendered frame as plain text through the public API, alongside
//! the existing accessor family (`save_frame_svg`, `frame_fingerprint`,
//! `frame_cell_bg`), and it must work in headless (`run_test`/Pilot) mode
//! where nothing is written to a real terminal.

use textual::compose;
use textual::prelude::*;

struct PlainTextApp;

impl TextualApp for PlainTextApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_compose(compose![Label::new("hello frame")])
    }
}

#[test]
fn frame_plain_text_exposes_rendered_frame() {
    textual::run_test(PlainTextApp, |pilot| {
        pilot.pause()?;

        let lines = pilot.app().frame_plain_lines();
        assert!(!lines.is_empty(), "rendered frame must have rows");
        // Every row is padded/cropped to the same frame width.
        let width = lines[0].chars().count();
        assert!(
            lines.iter().all(|l| l.chars().count() == width),
            "all plain rows must have the frame width ({width});\n{}",
            lines.join("\n")
        );
        assert!(
            lines.iter().any(|l| l.contains("hello frame")),
            "rendered frame must contain the label text; got:\n{}",
            lines.join("\n")
        );

        // The joined form is exactly the rows joined with newlines.
        assert_eq!(pilot.app().frame_plain_text(), lines.join("\n"));
        Ok(())
    })
    .unwrap();
}
