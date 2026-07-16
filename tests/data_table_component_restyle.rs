//! DataTable component-class restyling (component-classes Phase 2).
//!
//! DataTable's internal colours must be sourced from the `datatable--*`
//! component classes (Python parity) instead of hand-derived theme tokens, so
//! user CSS can restyle the table:
//!
//! - `DataTable > .datatable--cursor { ... }` (type-qualified)
//! - `#my-table > .datatable--cursor { ... }` (id-qualified)
//! - `DataTable.some-class > .datatable--cursor { ... }` (class-qualified)
//!
//! The golden tests pin the pre-migration hand-derived colours byte-for-byte
//! (they pass before AND after the migration), guarding the three stateful
//! regression suspects:
//!
//! 1. blurred-cursor foreground: `$block-cursor-blurred-foreground` used RAW
//!    (unflattened) — safe because `$foreground` is opaque;
//! 2. zebra even-row: `&:dark > .datatable--even-row { bg: $surface-darken-1
//!    40% }` must resolve identically to the old unconditional token math;
//! 3. header hover: the CSS `$accent 30%` must equal the old
//!    `$header-hover-background` token.

use textual::compose::ChildDecl;
use textual::css::{
    AppRuntimePseudos, default_widget_stylesheet, resolve_component_style, set_app_runtime_pseudos,
    set_style_context,
};
use textual::prelude::*;
use textual::renderables::Tint;
use textual::style::{Color, parse_color_like};

const USER_CSS_NONE: &str = "";

/// (r, g, b) triple for tolerant comparison (frame colours are opaque).
fn rgb(c: Color) -> (u8, u8, u8) {
    (c.r, c.g, c.b)
}

/// Test app: one DataTable (id `my-table`, class `some-class`) and one Input
/// (so focus can be moved OFF the table), with optional extra user CSS and
/// zebra stripes.
struct TableApp {
    user_css: &'static str,
    zebra: bool,
}

impl TableApp {
    fn new(user_css: &'static str) -> Self {
        Self {
            user_css,
            zebra: false,
        }
    }

    fn with_zebra(mut self) -> Self {
        self.zebra = true;
        self
    }
}

impl TextualApp for TableApp {
    fn compose(&mut self) -> AppRoot {
        let table = DataTable::new(
            vec!["H1".into(), "H2".into()],
            vec![
                vec!["Xx".into(), "Aa".into()],
                vec!["Yy".into(), "Bb".into()],
                vec!["Zz".into(), "Dd".into()],
            ],
        );
        AppRoot::new().with_compose(vec![
            ChildDecl::new(Box::new(table))
                .with_id("my-table")
                .with_classes(&["some-class"]),
            ChildDecl::new(Box::new(Input::new())).with_id("the-input"),
        ])
    }

    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        if !self.user_css.is_empty() {
            app.load_stylesheet(self.user_css);
        }
        Ok(())
    }

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut textual::event::WidgetCtx) {
        if self.zebra {
            if let Ok(handle) = app.query_one_typed::<DataTable>("DataTable") {
                let _ = handle.update(app, |table, rctx| {
                    table.set_zebra_stripes(true, rctx);
                });
            }
        }
    }
}

/// Frame position of the first occurrence of `glyph`.
fn find_glyph(app: &App, glyph: char) -> (usize, usize) {
    let lines = app.frame_plain_lines();
    for (y, line) in lines.iter().enumerate() {
        if let Some(x) = line.chars().position(|c| c == glyph) {
            return (x, y);
        }
    }
    panic!("glyph {glyph:?} not found in frame:\n{}", lines.join("\n"));
}

fn cell_bg(app: &App, glyph: char) -> Color {
    let (x, y) = find_glyph(app, glyph);
    app.frame_cell_bg(x, y)
        .unwrap_or_else(|| panic!("cell at {glyph:?} ({x},{y}) has no background"))
}

fn cell_fg(app: &App, glyph: char) -> Color {
    let (x, y) = find_glyph(app, glyph);
    app.frame_cell_fg(x, y)
        .unwrap_or_else(|| panic!("cell at {glyph:?} ({x},{y}) has no foreground"))
}

fn token(name: &str) -> Color {
    parse_color_like(name).unwrap_or_else(|| panic!("token {name} must resolve"))
}

// ── Golden parity: default colours byte-for-byte (pass pre- AND post-migration) ──

/// Focused table: header carries the `:focus` `background-tint: $foreground 5%`
/// fold over `$panel` (tint provenance), the cursor cell is the strong
/// `$block-cursor-background` ($primary) with NO tint re-applied on top
/// (no_style pinned), and its foreground is `$block-cursor-foreground`
/// flattened over the cursor background.
#[test]
fn focused_defaults_header_tint_and_cursor_colors() {
    textual::run_test(TableApp::new(USER_CSS_NONE), |pilot| {
        // The table is the first focusable widget: it holds focus after mount.
        pilot.pause()?;

        let panel = token("$panel");
        let foreground = token("$foreground");

        // Header: tint($panel, $foreground 5%) — provenance is the
        // `DataTable:focus > .datatable--header {{ background-tint }}` rule.
        let expected_header = Tint::<()>::blend_color_with_percent(panel, foreground, 5);
        assert_eq!(
            rgb(cell_bg(pilot.app(), 'H')),
            rgb(expected_header),
            "focused header bg must be $panel tinted by $foreground 5%"
        );

        // Focused cursor: $primary (== $block-cursor-background), untinted.
        let primary = token("$primary");
        assert_eq!(
            rgb(cell_bg(pilot.app(), 'X')),
            rgb(primary),
            "focused cursor bg must be raw $primary (no :focus background-tint on top)"
        );
        let cursor_fg = token("$block-cursor-foreground").flatten_over(primary);
        assert_eq!(
            rgb(cell_fg(pilot.app(), 'X')),
            rgb(cursor_fg),
            "focused cursor fg must be $block-cursor-foreground flattened over the cursor bg"
        );

        // Normal (non-cursor, non-zebra) row cell: $surface with the widget's
        // `:focus` background-tint applied by the widget style pass.
        let surface = token("$surface");
        let expected_row = Tint::<()>::blend_color_with_percent(surface, foreground, 5);
        assert_eq!(
            rgb(cell_bg(pilot.app(), 'Y')),
            rgb(expected_row),
            "normal row bg must be the tinted widget surface"
        );
        Ok(())
    })
    .unwrap();
}

/// Suspect 1 (blurred-cursor foreground): with focus elsewhere the cursor uses
/// the blurred tokens, and the foreground is the RAW (unflattened)
/// `$block-cursor-blurred-foreground`. The token must be opaque for that to
/// be equivalent to a flatten — pin both.
#[test]
fn blurred_cursor_keeps_raw_blurred_foreground() {
    textual::run_test(TableApp::new(USER_CSS_NONE), |pilot| {
        pilot.pause()?;
        pilot.click("#the-input")?;
        pilot.pause()?;

        let surface = token("$surface");
        let blurred_bg = token("$block-cursor-blurred-background").flatten_over(surface);
        assert_eq!(
            rgb(cell_bg(pilot.app(), 'X')),
            rgb(blurred_bg),
            "blurred cursor bg must be $block-cursor-blurred-background over $surface"
        );

        let blurred_fg = token("$block-cursor-blurred-foreground");
        assert!(
            blurred_fg.a >= 1.0,
            "premise: $block-cursor-blurred-foreground must be opaque (got a={})",
            blurred_fg.a
        );
        assert_eq!(
            rgb(cell_fg(pilot.app(), 'X')),
            rgb(blurred_fg),
            "blurred cursor fg must be the raw $block-cursor-blurred-foreground"
        );

        // Blurred header: plain $panel, no tint.
        assert_eq!(
            rgb(cell_bg(pilot.app(), 'H')),
            rgb(token("$panel")),
            "blurred header bg must be untinted $panel (tint provenance is :focus-gated)"
        );
        Ok(())
    })
    .unwrap();
}

/// Suspect 2 (zebra `:dark` gating): the even-row blend must match the old
/// unconditional `$surface-darken-1 40%` math over the composited (tinted)
/// surface — pinned against the odd row's live frame colour so the expectation
/// holds pre- and post-migration.
#[test]
fn zebra_even_row_matches_pre_migration_blend() {
    textual::run_test(TableApp::new(USER_CSS_NONE).with_zebra(), |pilot| {
        pilot.pause()?;

        // Odd row (Yy, row index 1): the plain composited surface.
        let composited = cell_bg(pilot.app(), 'Y');
        // Even row (Zz, row index 2... row 0 hosts the cursor, so use the
        // second even data row).
        let even = cell_bg(pilot.app(), 'Z');
        let expected = token("$surface-darken-1")
            .with_alpha(0.4)
            .flatten_over(composited);
        assert_eq!(
            rgb(even),
            rgb(expected),
            "zebra even row must be $surface-darken-1 40% over the composited surface"
        );
        // Zebra glyph foreground is baked to $foreground.
        assert_eq!(rgb(cell_fg(pilot.app(), 'Z')), rgb(token("$foreground")));
        Ok(())
    })
    .unwrap();
}

// ── Component-resolution parity (unit level, default stylesheet) ────────────

/// Suspect 3 (header hover token), RECONCILED: the component rule `& >
/// .datatable--header-hover { bg: $accent 30% }` resolves `$accent` to
/// Python's design token (the LAB round-trip `#FEA62B`, byte-locked by
/// `dark_design_tokens_match_python_generate`), which is what Python's own
/// `background: $accent 30%` rule paints. The legacy Rust-invented
/// `$header-hover-background` token was built from the RAW accent source
/// (`#FFA62B`) — one byte off in red from Python. The component value is the
/// Python-correct one; pin both the value and the delta so the divergence is
/// documented.
#[test]
fn header_hover_component_matches_python_accent_token() {
    let _guard = set_style_context(default_widget_stylesheet());
    let table = DataTable::empty();
    let comp = resolve_component_style(&table, &["datatable--header-hover"]);
    let accent_30 = token("$accent").with_alpha(0.3);
    assert_eq!(
        comp.bg,
        Some(accent_30),
        "component header-hover bg must be $accent 30% (Python's rule verbatim)"
    );
    let comp_bg = comp.bg.expect("header-hover bg resolves");
    assert_eq!(
        (comp_bg.r, comp_bg.g, comp_bg.b),
        (0xFE, 0xA6, 0x2B),
        "$accent must be Python's round-tripped design token"
    );
    // The legacy token keeps the raw accent base; the migration supersedes it
    // (one-byte red-channel parity fix, visible only on header hover).
    let legacy = token("$header-hover-background");
    assert_eq!((legacy.r, legacy.g, legacy.b), (0xFF, 0xA6, 0x2B));
}

/// Suspect 1 at the resolution level: the blurred (unfocused) cursor component
/// resolves to the exact legacy tokens.
#[test]
fn blurred_cursor_component_matches_legacy_tokens() {
    let _guard = set_style_context(default_widget_stylesheet());
    let table = DataTable::empty();
    let comp = resolve_component_style(&table, &["datatable--cursor"]);
    assert_eq!(comp.bg, Some(token("$block-cursor-blurred-background")));
    assert_eq!(comp.fg, Some(token("$block-cursor-blurred-foreground")));
    assert_ne!(comp.bold, Some(true), "blurred cursor must not be bold");
}

/// Suspect 2 at the resolution level: with `:dark` active on BOTH the phantom
/// and the parent, `&:dark > .datatable--even-row { bg: $surface-darken-1 40% }`
/// outranks the light-theme `& > .datatable--even-row { bg: $surface-lighten-1
/// 50% }` rule; without `:dark` the light rule wins.
#[test]
fn even_row_component_dark_gating() {
    let _guard = set_style_context(default_widget_stylesheet());
    let table = DataTable::empty();

    let _dark = set_app_runtime_pseudos(AppRuntimePseudos {
        dark: true,
        ..Default::default()
    });
    let comp = resolve_component_style(&table, &["datatable--even-row"]);
    assert_eq!(
        comp.bg,
        Some(token("$surface-darken-1").with_alpha(0.4)),
        "dark mode: even-row must resolve the :dark rule ($surface-darken-1 40%)"
    );
    drop(_dark);

    let _light = set_app_runtime_pseudos(AppRuntimePseudos {
        dark: false,
        ..Default::default()
    });
    let comp = resolve_component_style(&table, &["datatable--even-row"]);
    assert_eq!(
        comp.bg,
        Some(token("$surface-lighten-1").with_alpha(0.5)),
        "light mode: even-row must resolve $surface-lighten-1 50%"
    );
}

/// Default header component carries the `$panel` bg, `$foreground` colour and
/// bold text-style; the `:focus` `background-tint` arrives ON the component
/// (tint provenance) only when the parent is focused, which the off-tree
/// (unfocused) resolution must NOT carry.
#[test]
fn header_component_matches_legacy_tokens() {
    let _guard = set_style_context(default_widget_stylesheet());
    let table = DataTable::empty();
    let comp = resolve_component_style(&table, &["datatable--header"]);
    assert_eq!(comp.bg, Some(token("$panel")));
    assert_eq!(comp.fg, Some(token("$foreground")));
    assert_eq!(comp.bold, Some(true));
    assert_eq!(
        comp.background_tint, None,
        "unfocused: no background-tint on the header component"
    );
}

// ── Restyling through user CSS (the point of Phase 2) ───────────────────────

/// Type-qualified: `DataTable > .datatable--cursor` restyles the cursor.
#[test]
fn type_qualified_cursor_restyle() {
    textual::run_test(
        TableApp::new("DataTable > .datatable--cursor { background: #ff0000; }"),
        |pilot| {
            pilot.pause()?;
            assert_eq!(
                rgb(cell_bg(pilot.app(), 'X')),
                (0xff, 0, 0),
                "user CSS must restyle the cursor background"
            );
            Ok(())
        },
    )
    .unwrap();
}

/// Id-qualified: `#my-table > .datatable--cursor` restyles the cursor (live
/// selector stack carries the arena id).
#[test]
fn id_qualified_cursor_restyle() {
    textual::run_test(
        TableApp::new("#my-table > .datatable--cursor { background: #00ff00; }"),
        |pilot| {
            pilot.pause()?;
            assert_eq!(
                rgb(cell_bg(pilot.app(), 'X')),
                (0, 0xff, 0),
                "id-qualified user CSS must restyle the cursor background"
            );
            Ok(())
        },
    )
    .unwrap();
}

/// Class-qualified: `DataTable.some-class > .datatable--cursor` restyles the
/// cursor (live selector stack carries runtime classes).
#[test]
fn class_qualified_cursor_restyle() {
    textual::run_test(
        TableApp::new("DataTable.some-class > .datatable--cursor { background: #0000ff; }"),
        |pilot| {
            pilot.pause()?;
            assert_eq!(
                rgb(cell_bg(pilot.app(), 'X')),
                (0, 0, 0xff),
                "class-qualified user CSS must restyle the cursor background"
            );
            Ok(())
        },
    )
    .unwrap();
}

/// Header restyle: background and foreground follow user CSS. The table is
/// blurred first so the `:focus` header tint does not fold over the asserted
/// background.
#[test]
fn header_restyle_bg_and_fg() {
    textual::run_test(
        TableApp::new("DataTable > .datatable--header { background: #123456; color: #654321; }"),
        |pilot| {
            pilot.pause()?;
            pilot.click("#the-input")?;
            pilot.pause()?;
            assert_eq!(
                rgb(cell_bg(pilot.app(), 'H')),
                (0x12, 0x34, 0x56),
                "user CSS must restyle the header background"
            );
            assert_eq!(
                rgb(cell_fg(pilot.app(), 'H')),
                (0x65, 0x43, 0x21),
                "user CSS must restyle the header foreground"
            );
            Ok(())
        },
    )
    .unwrap();
}

/// Zebra restyle: even-row background follows user CSS (flattened over the
/// composited surface when semi-transparent; opaque user colours land as-is).
#[test]
fn zebra_even_row_restyle() {
    textual::run_test(
        TableApp::new("DataTable > .datatable--even-row { background: #223344; }").with_zebra(),
        |pilot| {
            pilot.pause()?;
            assert_eq!(
                rgb(cell_bg(pilot.app(), 'Z')),
                (0x22, 0x33, 0x44),
                "user CSS must restyle the zebra even-row background"
            );
            Ok(())
        },
    )
    .unwrap();
}

/// Odd-row restyle: `.datatable--odd-row` has no default rule (Python parity)
/// but must be consumable from user CSS under zebra stripes.
#[test]
fn zebra_odd_row_restyle() {
    textual::run_test(
        TableApp::new("DataTable > .datatable--odd-row { background: #443322; }").with_zebra(),
        |pilot| {
            pilot.pause()?;
            assert_eq!(
                rgb(cell_bg(pilot.app(), 'Y')),
                (0x44, 0x33, 0x22),
                "user CSS must restyle the zebra odd-row background"
            );
            Ok(())
        },
    )
    .unwrap();
}

/// The nine Python `COMPONENT_CLASSES` names are declared.
#[test]
fn datatable_declares_nine_component_classes() {
    use textual::widgets::Components;
    let table = DataTable::empty();
    let declared = Components::component_classes(&table);
    let expected = [
        "datatable--cursor",
        "datatable--hover",
        "datatable--fixed",
        "datatable--fixed-cursor",
        "datatable--header",
        "datatable--header-cursor",
        "datatable--header-hover",
        "datatable--odd-row",
        "datatable--even-row",
    ];
    assert_eq!(declared.len(), 9);
    for name in expected {
        assert!(
            declared.contains(&name),
            "{name} must be declared in COMPONENT_CLASSES"
        );
    }
}
