//! Component-classes Phase 3 (D8): ProgressBar sub-widget split.
//!
//! ProgressBar composes real arena children mirroring Python
//! `_progress_bar.py`: `Bar` (id `bar`), `PercentageStatus` (id
//! `percentage`), `ETAStatus` (id `eta`) — conditionally on
//! `show_bar` / `show_percentage` / `show_eta`. These tests gate:
//!
//! - addressability: `#bar` / `#percentage` / `#eta` are queryable;
//!   `show_bar = false` composes no `Bar` (spec acceptance);
//! - live layout: the scoped defaults (`ProgressBar Bar { width: 32 }`,
//!   percentage width 5, eta width 9) drive the auto width (46), replacing
//!   the retired hardcoded `content_width()`;
//! - live component CSS: Python-shaped user rules
//!   (`Bar > .bar--indeterminate { color: ... }`) restyle the bar glyphs;
//! - value propagation: a reactive progress update recomposes the children
//!   (the Rust analogue of Python's `data_bind`).

use std::time::Duration;

use textual::compose::ChildDecl;
use textual::layout::{Region, inspect_node_rects, resolve_layout};
use textual::prelude::*;
use textual::reactive::ReactiveCtx;
use textual::runtime::build_widget_tree_from_root;
use textual::widget_tree::WidgetTree;

fn throwaway_ctx() -> ReactiveCtx {
    ReactiveCtx::new(textual::node_id::NodeId::default())
}

/// Build a tree with a single ProgressBar under an AppRoot and return it.
fn tree_with_progress_bar(bar: ProgressBar) -> WidgetTree {
    struct Host {
        root: AppRoot,
    }
    impl Widget for Host {
        fn compose(&mut self) -> textual::compose::ComposeResult {
            self.root.compose()
        }
        fn render(
            &self,
            _console: &rich_rs::Console,
            _options: &rich_rs::ConsoleOptions,
        ) -> rich_rs::Segments {
            rich_rs::Segments::new()
        }
    }
    let mut host = Host {
        root: AppRoot::new().with_compose(vec![ChildDecl::from(bar).with_id("pb")]),
    };
    build_widget_tree_from_root(&mut host).expect("tree should build with children")
}

#[test]
fn sub_widgets_are_addressable_by_id() {
    let tree = tree_with_progress_bar(ProgressBar::new(Some(100.0)));
    for selector in ["#bar", "#percentage", "#eta"] {
        let hits = tree.query(selector).expect("query should parse");
        assert_eq!(hits.len(), 1, "{selector} must resolve to exactly one node");
    }
    // The Bar child is a real arena descendant of the ProgressBar.
    let hits = tree.query("ProgressBar Bar").expect("query should parse");
    assert_eq!(
        hits.len(),
        1,
        "ProgressBar Bar must match the composed child"
    );
}

#[test]
fn show_flags_gate_composed_children() {
    let mut bar = ProgressBar::new(Some(100.0));
    let mut ctx = throwaway_ctx();
    bar.set_show_bar(false, &mut ctx);
    let tree = tree_with_progress_bar(bar);
    assert_eq!(
        tree.query("#bar").expect("query should parse").len(),
        0,
        "show_bar = false must compose no Bar"
    );
    assert_eq!(tree.query("#percentage").expect("parse").len(), 1);
    assert_eq!(tree.query("#eta").expect("parse").len(), 1);

    let mut bar = ProgressBar::new(Some(100.0));
    let mut ctx = throwaway_ctx();
    bar.set_show_percentage(false, &mut ctx);
    bar.set_show_eta(false, &mut ctx);
    let tree = tree_with_progress_bar(bar);
    assert_eq!(tree.query("#bar").expect("parse").len(), 1);
    assert_eq!(tree.query("#percentage").expect("parse").len(), 0);
    assert_eq!(tree.query("#eta").expect("parse").len(), 0);
}

/// The scoped defaults (`ProgressBar Bar { width: 32 }` + percentage 5 +
/// eta 9) are live layout inputs: a `width: auto` ProgressBar arranges to
/// 46 cells from its children, not from a hardcoded intrinsic hint.
#[test]
fn auto_width_derives_from_child_layout() {
    // The scoped widths live in the framework default sheet; install it as
    // the live style context for this headless layout pass (the runtime does
    // the same around its layout/render passes).
    let _guard = textual::css::set_style_context(textual::css::default_widget_stylesheet());
    let mut tree = tree_with_progress_bar(ProgressBar::new(Some(100.0)));
    let root = tree.root().expect("root");
    resolve_layout(&mut tree, root, Region::new(0, 0, 120, 24), (120, 24));

    let pb = tree.query("#pb").expect("parse")[0];
    let ((x0, _, x1, _), _) = inspect_node_rects(&tree, pb).expect("pb rect");
    assert_eq!(
        x1 - x0,
        46,
        "ProgressBar auto width = Bar(32) + PercentageStatus(5) + ETAStatus(9)"
    );

    let bar = tree.query("#bar").expect("parse")[0];
    let ((bx0, _, bx1, _), _) = inspect_node_rects(&tree, bar).expect("bar rect");
    assert_eq!(bx1 - bx0, 32, "Bar takes its default CSS width: 32");

    let pct = tree.query("#percentage").expect("parse")[0];
    let ((px0, _, px1, _), _) = inspect_node_rects(&tree, pct).expect("pct rect");
    assert_eq!(px1 - px0, 5, "PercentageStatus takes its default width: 5");

    let eta = tree.query("#eta").expect("parse")[0];
    let ((ex0, _, ex1, _), _) = inspect_node_rects(&tree, eta).expect("eta rect");
    assert_eq!(ex1 - ex0, 9, "ETAStatus takes its default width: 9");
}

// ── Pilot-level tests ───────────────────────────────────────────────

struct BarCssApp {
    css: &'static str,
    bar: Option<ProgressBar>,
}

impl TextualApp for BarCssApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(self.css);
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        let bar = self.bar.take().expect("compose called once");
        AppRoot::new().with_compose(vec![ChildDecl::from(bar).with_id("pb")])
    }
}

/// Find the first bar glyph in the rendered frame and return its (x, y).
fn find_bar_glyph(pilot: &textual::runtime::Pilot) -> (usize, usize) {
    for (y, line) in pilot.app().frame_plain_lines().iter().enumerate() {
        for (x, ch) in line.chars().enumerate() {
            if ch == '━' || ch == '╺' || ch == '╸' {
                return (x, y);
            }
        }
    }
    panic!("no bar glyph rendered");
}

/// Python-shaped app CSS `Bar > .bar--indeterminate { color: ... }` goes
/// live on the composed Bar child: the rendered glyphs pick up the color.
#[test]
fn bar_indeterminate_app_css_takes_effect() {
    let mut bar = ProgressBar::new(None);
    // Static full-width indeterminate bar: every glyph carries the
    // highlight (component fg) color, deterministically.
    bar.set_animation_level(textual::event::AnimationLevel::None);
    BarCssApp {
        css: "Bar > .bar--indeterminate { color: #ff0000; }",
        bar: Some(bar),
    }
    .run_test(|pilot| {
        let (x, y) = find_bar_glyph(pilot);
        let fg = pilot
            .app()
            .frame_cell_fg(x, y)
            .expect("bar glyph has a fg color");
        assert_eq!(
            (fg.r, fg.g, fg.b),
            (0xff, 0, 0),
            "Bar > .bar--indeterminate color must reach the bar glyphs"
        );
        Ok(())
    })
    .expect("run_test");
}

/// Same for the determinate state: `Bar > .bar--bar { color: ... }`.
#[test]
fn bar_determinate_app_css_takes_effect() {
    let bar = ProgressBar::new(Some(100.0)).with_progress(50.0);
    BarCssApp {
        css: "Bar > .bar--bar { color: #00ff00; }",
        bar: Some(bar),
    }
    .run_test(|pilot| {
        let (x, y) = find_bar_glyph(pilot);
        let fg = pilot
            .app()
            .frame_cell_fg(x, y)
            .expect("bar glyph has a fg color");
        assert_eq!(
            (fg.r, fg.g, fg.b),
            (0, 0xff, 0),
            "Bar > .bar--bar color must reach the filled glyphs"
        );
        Ok(())
    })
    .expect("run_test");
}

/// Value propagation: a reactive progress update (Handle::update path)
/// recomposes the children — the PercentageStatus shows the new value.
#[test]
fn progress_update_recomposes_children() {
    BarCssApp {
        css: "",
        bar: Some(ProgressBar::new(Some(100.0))),
    }
    .run_test(|pilot| {
        let text = pilot.app().frame_plain_text();
        assert!(
            text.contains("0%") && !text.contains("50%"),
            "initial frame shows 0%: {text:?}"
        );

        let app = pilot.app_mut();
        let handle = app
            .query_one_typed::<ProgressBar>("#pb")
            .expect("progress bar handle");
        handle
            .update(app, |bar, rctx| bar.advance(50.0, rctx))
            .expect("update");
        pilot.wait_for_idle()?;

        let text = pilot.app().frame_plain_text();
        assert!(
            text.contains("50%"),
            "after advance(50), the recomposed PercentageStatus shows 50%: {text:?}"
        );
        Ok(())
    })
    .expect("run_test");
}

/// The rendered strip keeps the pre-split shape: bar glyphs, right-aligned
/// percentage, right-aligned ETA placeholder on one row.
#[test]
fn rendered_row_structure_matches_monolithic_layout() {
    BarCssApp {
        css: "",
        bar: Some(ProgressBar::new(Some(100.0))),
    }
    .run_test(|pilot| {
        let lines = pilot.app().frame_plain_lines();
        let row = lines
            .iter()
            .find(|l| l.contains('━') || l.contains('╺'))
            .expect("bar row rendered");
        assert!(
            row.contains("0%"),
            "percentage rendered on the bar row: {row:?}"
        );
        assert!(
            row.contains("--:--:--"),
            "ETA placeholder rendered on the bar row: {row:?}"
        );
        // Column layout: Bar 0..32, percentage right-aligned in 32..37,
        // ETA right-aligned in 37..46.
        let chars: Vec<char> = row.chars().collect();
        let pct_field: String = chars[32..37].iter().collect();
        assert_eq!(pct_field, "   0%", "percentage right-aligned in width 5");
        let eta_field: String = chars[37..46].iter().collect();
        assert_eq!(eta_field, " --:--:--", "ETA right-aligned in width 9");
        Ok(())
    })
    .expect("run_test");
}

/// Post-mount `set_show_eta(false)` (the docs_progress_bar mount sequence:
/// Python `ProgressBar(total=100, show_eta=False)`) recomposes without the
/// ETAStatus child.
#[test]
fn post_mount_show_eta_false_removes_eta_child() {
    BarCssApp {
        css: "",
        bar: Some(ProgressBar::new(Some(100.0))),
    }
    .run_test(|pilot| {
        assert!(pilot.app().frame_plain_text().contains("--:--:--"));

        let app = pilot.app_mut();
        let handle = app
            .query_one_typed::<ProgressBar>("#pb")
            .expect("progress bar handle");
        handle
            .update(app, |bar, rctx| bar.set_show_eta(false, rctx))
            .expect("update");
        pilot.wait_for_idle()?;

        assert!(
            !pilot.app().frame_plain_text().contains("--:--:--"),
            "the ETAStatus child must be gone after set_show_eta(false)"
        );
        assert_eq!(
            pilot.app().query_one("#eta").ok(),
            None,
            "#eta must no longer resolve"
        );
        Ok(())
    })
    .expect("run_test");
}

/// The 1-second ETA refresh interval must not recompose when the displayed
/// ETA is unchanged (no per-frame/idle churn), and the indeterminate Bar
/// self-animates without parent recomposes (it reports active; the runtime
/// repaints it per tick).
#[test]
fn eta_interval_is_quiet_when_display_unchanged() {
    BarCssApp {
        css: "",
        bar: Some(ProgressBar::new(Some(100.0))),
    }
    .run_test(|pilot| {
        let before = pilot.app().frame_fingerprint();
        pilot.advance_clock(Duration::from_secs(3))?;
        let after = pilot.app().frame_fingerprint();
        assert_eq!(
            before, after,
            "with no progress and no ETA change, the 1s interval must not alter the frame"
        );
        Ok(())
    })
    .expect("run_test");
}
