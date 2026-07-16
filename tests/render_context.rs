//! Public render-time style seam (`textual::render_context`).
//!
//! A custom widget's `render()` reads its resolved fg/bg, the composited
//! ancestor surface, and theme tokens through the documented public API
//! (`render_context::resolved_style` / `composited_background` /
//! `theme_color`), with no access to framework internals. This is the seam
//! the 1.1 component-classes work consumes.

use std::sync::{Arc, Mutex};

use rich_rs::{Console, ConsoleOptions, Segment, Segments};
use textual::prelude::*;
use textual::widgets::Widget;

/// Everything a probe widget observes through the public seam during render.
#[derive(Clone, Debug, Default)]
struct Captured {
    fg: Option<Color>,
    bg: Option<Color>,
    composited_bg: Option<Color>,
    accent: Option<Color>,
    accent_no_dollar: Option<Color>,
}

/// A custom widget whose `render()` reads the public render-time seam and
/// records what it saw.
struct Probe {
    captured: Arc<Mutex<Option<Captured>>>,
}

impl Widget for Probe {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        let style = render_context::resolved_style();
        let snapshot = Captured {
            fg: style.as_ref().and_then(|s| s.fg),
            bg: style.as_ref().and_then(|s| s.bg),
            composited_bg: render_context::composited_background(),
            accent: render_context::theme_color("$accent"),
            accent_no_dollar: render_context::theme_color("accent"),
        };
        *self.captured.lock().unwrap() = Some(snapshot);
        vec![Segment::new("probe")].into()
    }

    fn style_type(&self) -> &'static str {
        "Probe"
    }
}

const CSS: &str = r##"
Probe {
    width: 10;
    height: 1;
}

#styled {
    color: #ff0000;
    background: #0000ff;
}

#wrap {
    background: #00ff00;
    width: 20;
    height: 3;
}
"##;

struct SeamApp {
    styled: Arc<Mutex<Option<Captured>>>,
    bare: Arc<Mutex<Option<Captured>>>,
}

impl TextualApp for SeamApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_compose(vec![
            ChildDecl::from(Probe {
                captured: Arc::clone(&self.styled),
            })
            .with_id("styled"),
            ChildDecl::new(Box::new(Container::new().with_compose(vec![
                ChildDecl::from(Probe {
                    captured: Arc::clone(&self.bare),
                })
                .with_id("bare"),
            ])))
            .with_id("wrap"),
        ])
    }

    fn configure(&mut self, app: &mut App) -> Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }
}

#[test]
fn render_can_read_resolved_style_composited_bg_and_theme_token() {
    let styled = Arc::new(Mutex::new(None));
    let bare = Arc::new(Mutex::new(None));
    let app = SeamApp {
        styled: Arc::clone(&styled),
        bare: Arc::clone(&bare),
    };

    textual::run_test(app, |pilot| {
        pilot.pause()?;

        // Widget with explicit color/background: resolved_style() carries both,
        // and the composited surface is its own opaque background.
        let s = styled
            .lock()
            .unwrap()
            .clone()
            .expect("styled probe rendered");
        assert_eq!(
            s.fg,
            Some(Color::rgb(0xff, 0x00, 0x00)),
            "resolved fg comes from the stylesheet"
        );
        assert_eq!(
            s.bg,
            Some(Color::rgb(0x00, 0x00, 0xff)),
            "resolved bg comes from the stylesheet"
        );
        assert_eq!(
            s.composited_bg,
            Some(Color::rgb(0x00, 0x00, 0xff)),
            "an opaque own bg IS the composited surface"
        );

        // Theme token resolution matches the CSS token path, with or without
        // the leading `$`.
        let expected_accent =
            textual::style::parse_color_like("$accent").expect("$accent resolves");
        assert_eq!(s.accent, Some(expected_accent), "$accent resolves in render");
        assert_eq!(
            s.accent_no_dollar,
            Some(expected_accent),
            "token accepted without the $ prefix"
        );

        // Transparent widget inside a painted container: CSS bg is NOT
        // inherited (resolved bg stays None), but the render-time composited
        // surface is the ancestor's painted background.
        let b = bare.lock().unwrap().clone().expect("bare probe rendered");
        assert_eq!(
            b.bg, None,
            "bg is not an inherited property; a bare widget resolves none"
        );
        assert_eq!(
            b.composited_bg,
            Some(Color::rgb(0x00, 0xff, 0x00)),
            "composited background is the painted ancestor surface (#wrap)"
        );

        // Outside a render call the render-scoped queries return None, while
        // the theme resolver still works.
        assert_eq!(
            render_context::resolved_style(),
            None,
            "resolved_style is render-scoped"
        );
        assert_eq!(
            render_context::composited_background(),
            None,
            "composited_background is render-scoped"
        );
        assert_eq!(render_context::theme_color("$accent"), Some(expected_accent));
        assert_eq!(
            render_context::theme_color("$no-such-token"),
            None,
            "unknown tokens resolve to None"
        );

        Ok(())
    })
    .expect("headless run_test must succeed");
}
