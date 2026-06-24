//! Verification for keystone "themereg": the named theme catalog/registry +
//! cycling actually re-colors the UI (the `$primary`/`$text-error`/… design
//! tokens resolve from whichever named theme is active), faithful to Python
//! Textual's `App.theme = name` / `action_cycle_theme`.
//!
//! All global-theme mutation is confined to a single test so it cannot leak
//! into other test binaries; the test restores the default `textual-dark`
//! resolution path on exit.

use textual::prelude::*;
use textual::style::parse_color_like;

fn rgb(token: &str) -> (u8, u8, u8) {
    let c = parse_color_like(token).unwrap_or_else(|| panic!("token {token} did not resolve"));
    (c.r, c.g, c.b)
}

fn hexrgb(s: &str) -> (u8, u8, u8) {
    let c = Color::parse(s).unwrap();
    (c.r, c.g, c.b)
}

#[test]
fn available_themes_includes_python_builtins() {
    let app = App::new().expect("app init");
    let names = app.available_themes();
    for expected in [
        "textual-dark",
        "textual-light",
        "nord",
        "gruvbox",
        "tokyo-night",
        "solarized-light",
        "solarized-dark",
        "dracula",
        "monokai",
        "catppuccin-mocha",
        "flexoki",
        "rose-pine",
    ] {
        assert!(
            names.contains(&expected.to_string()),
            "available_themes() missing builtin {expected}"
        );
    }
}

#[test]
fn register_custom_theme_then_query() {
    let mut app = App::new().expect("app init");
    let mut custom = textual::theme::get_theme("textual-dark").unwrap();
    custom.name = "k-custom-theme".to_string();
    custom.primary = "#123456".to_string();
    app.register_theme(custom);
    assert!(
        app.available_themes()
            .contains(&"k-custom-theme".to_string())
    );
}

#[test]
fn named_theme_switch_recolors_tokens_and_cycles() {
    // Confine all global active-theme mutation to this single test, restoring
    // the default path at the end (so concurrent default-theme assertions in
    // this binary are unaffected by interleaving — there are none that run
    // while this body holds a non-default theme because the other tests here
    // never read `$primary`).
    let mut app = App::new().expect("app init");

    // Default path: textual-dark hand-tuned static.
    assert_eq!(app.theme_name(), "textual-dark");
    assert_eq!(rgb("$primary"), hexrgb("#0178D4"));

    // Activate nord — every design token must now resolve from nord.
    assert!(app.set_theme_by_name("nord"));
    assert_eq!(app.theme_name(), "nord");
    assert_eq!(rgb("$primary"), hexrgb("#88C0D0"));
    assert_eq!(rgb("$background"), hexrgb("#2E3440"));
    assert_eq!(rgb("$text-error"), hexrgb("#D4969C"));
    assert_eq!(rgb("$error-muted"), hexrgb("#59414C"));
    assert_eq!(rgb("$primary-muted"), hexrgb("#495E6B"));

    // Activate solarized-light (a light theme) — confirm a different palette.
    assert!(app.set_theme_by_name("solarized-light"));
    assert_eq!(rgb("$primary"), hexrgb("#268BD2"));
    assert_eq!(rgb("$background"), hexrgb("#FDF6E3"));
    assert_eq!(rgb("$text-error"), hexrgb("#91211F"));

    // Unknown theme is rejected, active theme unchanged.
    assert!(!app.set_theme_by_name("does-not-exist"));
    assert_eq!(app.theme_name(), "solarized-light");

    // Cycling: exactly the Python todo_app cycle.
    app.set_theme_cycle([
        "nord",
        "gruvbox",
        "tokyo-night",
        "textual-dark",
        "solarized-light",
    ]);
    assert!(app.cycle_theme());
    assert_eq!(app.theme_name(), "nord");
    assert_eq!(rgb("$primary"), hexrgb("#88C0D0"));
    assert!(app.cycle_theme());
    assert_eq!(app.theme_name(), "gruvbox");
    assert_eq!(rgb("$primary"), hexrgb("#85A598"));
    assert!(app.cycle_theme());
    assert_eq!(app.theme_name(), "tokyo-night");
    assert_eq!(rgb("$primary"), hexrgb("#BB9AF7"));
    assert!(app.cycle_theme());
    assert_eq!(app.theme_name(), "textual-dark");
    // Back to default: hand-tuned static path again.
    assert_eq!(rgb("$primary"), hexrgb("#0178D4"));
    assert!(app.cycle_theme());
    assert_eq!(app.theme_name(), "solarized-light");
    // Wrap-around.
    assert!(app.cycle_theme());
    assert_eq!(app.theme_name(), "nord");

    // Restore default resolution before exiting so nothing leaks.
    assert!(app.set_theme_by_name("textual-dark"));
    assert_eq!(rgb("$primary"), hexrgb("#0178D4"));
}
