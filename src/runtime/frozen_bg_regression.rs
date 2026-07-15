//! Regression: an ancestor PSEUDO-STATE change must re-bake a transparent
//! child's glyph background (frozen-ancestor-bg re-capture).
//!
//! The frozen-ancestor-background mechanism (`FROZEN_ANCESTOR_BG` in
//! `runtime::render`) replicates Python's cached `visual_style`: a child with a
//! transparent background keeps the ancestor surface captured at its own last
//! content render when an ancestor's background changes via a direct INLINE
//! style mutation (Python inline mutations do not cascade
//! `notify_style_update`). But a CLASS or PSEUDO-CLASS change anywhere in the
//! ancestor chain routes through Python's `app.update_styles(node)`, which
//! clears every descendant's cached `visual_style` — so the child re-bakes over
//! the fresh surface.
//!
//! This is the `Select` bar bug shape: `Select:focus > SelectCurrent` applies a
//! `background-tint`; when focus moves from the Select to its overlay child,
//! the tint rule stops matching, the bar's surface repaints untinted — but the
//! transparent `#label` glyph run stayed frozen at the TINTED surface because
//! the label's own fingerprint never changed. The test is written against
//! GENERIC synthetic widgets (not `Select`) and captures the TRANSITION: render
//! focused (tinted), flip focus away, render again, assert the label glyph bg
//! re-captured to the untinted surface.

#![cfg(test)]

use rich_rs::{Console, ConsoleOptions, Renderable, Segments};

use crate::compose::{ChildDecl, ComposeResult};
use crate::widgets::{AppRoot, NodeSeed, Static, Widget};
use crate::{App, TextualApp};

const HOST_ID: &str = "tint_host";
const OTHER_ID: &str = "other_focus";
/// Distinct glyph so the test can locate the label's cells in the frame.
const LABEL_TEXT: &str = "XXXXX";

/// Focusable host whose `:focus` applies a `background-tint`; composes a
/// transparent `Static` label whose glyph cells bake the host's surface.
struct TintHost {
    seed: NodeSeed,
}

impl TintHost {
    fn new() -> Self {
        Self {
            seed: NodeSeed {
                css_id: Some(HOST_ID.to_string()),
                ..NodeSeed::default()
            },
        }
    }
}

impl Widget for TintHost {
    fn focusable(&self) -> bool {
        true
    }
    fn compose(&mut self) -> ComposeResult {
        vec![ChildDecl::new(Box::new(
            Static::new(LABEL_TEXT).without_markup(),
        ))
        .with_id("lbl")]
    }
    /// Chrome-only: the framework paints the surface from the resolved style;
    /// the composed label composites over it (the `SelectCurrent` shape).
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }
    fn style_type(&self) -> &'static str {
        "TintHost"
    }
    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }
    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
}

impl Renderable for TintHost {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

/// A second focusable widget so focus can move OFF the host.
struct OtherFocus {
    seed: NodeSeed,
}

impl OtherFocus {
    fn new() -> Self {
        Self {
            seed: NodeSeed {
                css_id: Some(OTHER_ID.to_string()),
                ..NodeSeed::default()
            },
        }
    }
}

impl Widget for OtherFocus {
    fn focusable(&self) -> bool {
        true
    }
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }
    fn style_type(&self) -> &'static str {
        "OtherFocus"
    }
    fn set_inline_style(&mut self, style: crate::style::Style) {
        self.seed.styles.style = style;
    }
    fn take_node_seed(&mut self) -> NodeSeed {
        std::mem::take(&mut self.seed)
    }
    fn layout_height(&self) -> Option<usize> {
        Some(1)
    }
}

impl Renderable for OtherFocus {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Segments {
        Widget::render(self, console, options)
    }
}

struct TintApp;

impl TextualApp for TintApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
            .with_child(TintHost::new())
            .with_child(OtherFocus::new())
    }
    fn configure(&mut self, app: &mut App) -> crate::Result<()> {
        app.load_stylesheet(
            "Screen { background: #000000; } \
             TintHost { background: #102030; height: 3; } \
             TintHost:focus { background-tint: #ffffff 50%; } \
             Static#lbl { background: transparent; height: 1; }",
        );
        Ok(())
    }
}

/// Collect the background colours of every frame cell showing the label glyph.
fn label_cell_bgs(app: &App) -> Vec<crate::style::Color> {
    let mut bgs = Vec::new();
    for y in 0..app.frame.height {
        for x in 0..app.frame.width {
            let cell = app.frame.get(x, y);
            if cell.text == "X" {
                if let Some(bg) = app.frame_cell_bg(x, y) {
                    bgs.push(bg);
                }
            }
        }
    }
    bgs
}

const UNTINTED: crate::style::Color = crate::style::Color::rgb(0x10, 0x20, 0x30);

// ---------------------------------------------------------------------------
// Own SEMI-TRANSPARENT bg over an ancestor-only INLINE bg change
// (the `events/custom01` shape: ColorButton `background: #ffffff33` while the
// Screen bg animates). Python's `visual_style` — cached on the widget's OWN
// `styles._cache_key` — keeps the whole CONTENT strip (glyphs AND the
// content-align fill) baked as `own bg over the PRE-CHANGE ancestor surface`,
// while `background_colors`-derived surfaces (CSS padding) re-render LIVE.
// ---------------------------------------------------------------------------

struct TranslucentApp;

impl TextualApp for TranslucentApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Static::new(LABEL_TEXT).without_markup().id("host"))
    }
    fn configure(&mut self, app: &mut App) -> crate::Result<()> {
        app.load_stylesheet(
            "Screen { background: #102030; } \
             Static#host { background: #ffffff33; height: 3; \
                           content-align: center middle; padding: 0 2; }",
        );
        Ok(())
    }
}

/// Frame positions of the label glyph run: ((x_first, y), bgs).
fn glyph_cells(app: &App) -> (Option<(usize, usize)>, Vec<crate::style::Color>) {
    let mut first = None;
    let mut bgs = Vec::new();
    for y in 0..app.frame.height {
        for x in 0..app.frame.width {
            if app.frame.get(x, y).text == "X" {
                if first.is_none() {
                    first = Some((x, y));
                }
                if let Some(bg) = app.frame_cell_bg(x, y) {
                    bgs.push(bg);
                }
            }
        }
    }
    (first, bgs)
}

#[test]
fn ancestor_inline_bg_change_keeps_own_translucent_content_surface_frozen() {
    crate::run_test(TranslucentApp, |pilot| {
        pilot.pause()?;
        let (first, before) = glyph_cells(pilot.app());
        let (gx, gy) = first.expect("label glyphs rendered");
        assert_eq!(before.len(), LABEL_TEXT.len(), "label cells found");
        let frozen_blend = before[0];
        assert!(
            before.iter().all(|bg| *bg == frozen_blend),
            "precondition: uniform own-bg blend over the initial screen surface"
        );
        // Content-align pad (left of the centered glyph run) and the CSS
        // padding column share the same blend before the ancestor change.
        let align_pad_before = pilot.app().frame_cell_bg(gx - 1, gy).unwrap();
        let css_pad_before = pilot.app().frame_cell_bg(0, gy).unwrap();
        assert_eq!(align_pad_before, frozen_blend);
        assert_eq!(css_pad_before, frozen_blend);

        // Ancestor-only INLINE bg mutation (the animation-frame shape): the
        // host's own fingerprint is untouched, so its frozen capture persists.
        pilot
            .app_mut()
            .query_mut("Screen")
            .expect("Screen node")
            .set_styles(|s| s.style.bg = crate::style::parse_color_like("#800000"));
        pilot.pause()?;

        let (_, after) = glyph_cells(pilot.app());
        assert_eq!(after.len(), LABEL_TEXT.len(), "label cells found");
        // Python `visual_style` parity: glyph cells AND the content-align fill
        // keep the own-translucent-bg blend over the PRE-CHANGE surface.
        assert!(
            after.iter().all(|bg| *bg == frozen_blend),
            "content glyph bg must stay frozen at the pre-change blend \
             (got {after:?}, want {frozen_blend:?})"
        );
        let align_pad_after = pilot.app().frame_cell_bg(gx - 1, gy).unwrap();
        assert_eq!(
            align_pad_after, frozen_blend,
            "content-align fill must stay frozen (Python Strip.align uses the \
             cached visual_style surface)"
        );
        // Python `background_colors` parity: the CSS padding column re-renders
        // LIVE — own translucent bg over the NEW ancestor surface.
        let css_pad_after = pilot.app().frame_cell_bg(0, gy).unwrap();
        assert_ne!(
            css_pad_after, frozen_blend,
            "CSS padding must re-render live over the new ancestor surface \
             (Python StylesCache inner style)"
        );
        Ok(())
    })
    .unwrap();
}

// ---------------------------------------------------------------------------
// Cross-app staleness: `FROZEN_ANCESTOR_BG` is thread-local and keyed by
// `NodeId`. A second app built on the same thread reissues the SAME NodeIds
// for a structurally identical tree, and a transparent child's fingerprint
// (own style + ancestor selector identity) does not see ancestor VALUE
// differences — so without clearing the cache on tree build, app B's label
// would bake app A's captured surface (the README multi-theme screenshot
// batch leaked the previous theme's surface this way).
// ---------------------------------------------------------------------------

struct ScreenLabelApp {
    screen_bg: &'static str,
}

impl TextualApp for ScreenLabelApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Static::new(LABEL_TEXT).without_markup().id("lbl"))
    }
    fn configure(&mut self, app: &mut App) -> crate::Result<()> {
        app.load_stylesheet(&format!(
            "Screen {{ background: {}; }} \
             Static#lbl {{ background: transparent; height: 1; }}",
            self.screen_bg
        ));
        Ok(())
    }
}

fn assert_label_bakes(screen_bg: &'static str, want: crate::style::Color) {
    crate::run_test(ScreenLabelApp { screen_bg }, |pilot| {
        pilot.pause()?;
        let bgs = label_cell_bgs(pilot.app());
        assert_eq!(bgs.len(), LABEL_TEXT.len(), "label cells found");
        assert!(
            bgs.iter().all(|bg| *bg == want),
            "transparent label must bake THIS app's screen surface \
             (got {bgs:?}, want {want:?})"
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn fresh_widget_tree_recaptures_frozen_ancestor_surfaces() {
    // App A bakes its transparent label over its own screen surface…
    assert_label_bakes("#102030", crate::style::Color::rgb(0x10, 0x20, 0x30));
    // …and app B (same thread, identical structure => identical NodeIds and
    // fingerprints) must bake ITS surface, not app A's captured one.
    assert_label_bakes("#803000", crate::style::Color::rgb(0x80, 0x30, 0x00));
}

#[test]
fn ancestor_focus_change_rebakes_transparent_child_glyph_bg() {
    crate::run_test(TintApp, |pilot| {
        // Frame 1: host focused -> `:focus` background-tint applies; the
        // transparent label glyph run bakes the TINTED surface.
        pilot.app_mut().action_focus(HOST_ID).unwrap();
        pilot.pause()?;
        let tinted = label_cell_bgs(pilot.app());
        assert_eq!(tinted.len(), LABEL_TEXT.len(), "label cells found");
        assert!(
            tinted.iter().all(|bg| *bg != UNTINTED),
            "precondition: focused host tints the label surface (got {tinted:?})"
        );

        // Frame 2: focus moves off the host (a real, routed Tab keypress, so
        // the runtime schedules the repaint) -> the tint rule stops matching.
        // The ancestor pseudo-state change must re-capture the frozen ancestor
        // surface, so the label glyph bg repaints UNTINTED alongside the
        // host's own surface (this is the `Select` bar staleness regression).
        pilot.press_key("tab")?;
        pilot.pause()?;
        let other = pilot.app().query_one(&format!("#{OTHER_ID}")).unwrap();
        let tree = pilot.app().active_widget_tree().unwrap();
        assert!(
            tree.node_state(other).focused,
            "precondition: tab moved focus off the host"
        );
        let fresh = label_cell_bgs(pilot.app());
        assert_eq!(fresh.len(), LABEL_TEXT.len(), "label cells found");
        assert!(
            fresh.iter().all(|bg| *bg == UNTINTED),
            "label glyph bg must re-bake to the untinted host surface after \
             the ancestor loses :focus (got {fresh:?}, want {UNTINTED:?})"
        );
        Ok(())
    })
    .unwrap();
}
