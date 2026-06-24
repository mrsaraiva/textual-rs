//! Named theme catalog/registry — a faithful port of Python Textual's
//! `textual/theme.py` (`Theme`, `BUILTIN_THEMES`) and the token generation in
//! `textual/design.py` (`ColorSystem.generate`).
//!
//! Python's `App` keeps a registry of named [`NamedTheme`]s and resolves the
//! design tokens (`$primary`, `$text-error`, `$error-muted`, …) from whichever
//! theme is currently active. textual-rs previously hardcoded the `textual-dark`
//! token table as a global static in `style.rs`; this module adds the
//! registry + per-theme token generation so `App::set_theme_by_name` /
//! `cycle_theme` actually re-color the UI exactly like Python.
//!
//! ## Parity-safety
//!
//! The hand-tuned `textual-dark` token table in `style.rs::resolve_textual_dark_token`
//! is preserved as the default resolution path (so the styled/visual goldens that
//! were calibrated against it never regress). When a *non-default* named theme is
//! active, [`active_token`] / [`active_auto_token`] consult the generated map for
//! that theme instead.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::style::{
    Color, blend_channels_trunc, contrast_text, darken_lab, lighten_lab, parse_color_like,
};

const NUMBER_OF_SHADES: i32 = 3;
const DEFAULT_DARK_BACKGROUND: &str = "#121212";
const DEFAULT_DARK_SURFACE: &str = "#1e1e1e";
const DEFAULT_LIGHT_SURFACE: &str = "#f5f5f5";
const DEFAULT_LIGHT_BACKGROUND: &str = "#efefef";

/// A named theme — mirrors Python `textual.theme.Theme`.
#[derive(Debug, Clone)]
pub struct NamedTheme {
    pub name: String,
    pub primary: String,
    pub secondary: Option<String>,
    pub warning: Option<String>,
    pub error: Option<String>,
    pub success: Option<String>,
    pub accent: Option<String>,
    pub foreground: Option<String>,
    pub background: Option<String>,
    pub surface: Option<String>,
    pub panel: Option<String>,
    pub boost: Option<String>,
    pub dark: bool,
    pub luminosity_spread: f32,
    pub text_alpha: f32,
    pub variables: Vec<(String, String)>,
    pub ansi: bool,
}

impl NamedTheme {
    fn builder(name: &str, primary: &str) -> Self {
        NamedTheme {
            name: name.to_string(),
            primary: primary.to_string(),
            secondary: None,
            warning: None,
            error: None,
            success: None,
            accent: None,
            foreground: None,
            background: None,
            surface: None,
            panel: None,
            boost: None,
            dark: true,
            luminosity_spread: 0.15,
            text_alpha: 0.95,
            variables: Vec::new(),
            ansi: false,
        }
    }

    /// Generate the resolved design-token map for this theme.
    ///
    /// Port of `ColorSystem._generate` (truecolor) — the ANSI themes
    /// (`ansi-dark`/`ansi-light`) are intentionally not generated here; they
    /// resolve through the default path which already handles `ansi_*` names.
    pub fn generate(&self) -> HashMap<String, Color> {
        generate_tokens(self)
    }
}

// ---------------------------------------------------------------------------
// Token generation (port of ColorSystem._generate)
// ---------------------------------------------------------------------------

/// Parse a hex/CSS color string into a `Color`, panicking on the builtin
/// values (they are all valid literals).
fn parse(value: &str) -> Color {
    parse_color_like(value).unwrap_or(Color::rgb(0, 0, 0))
}

/// Python `Color.blend(dest, factor, alpha)` with an explicit alpha override.
fn blend_alpha(src: Color, dest: Color, factor: f32, alpha: f32) -> Color {
    let factor = factor.clamp(0.0, 1.0);
    Color::rgba_f(
        blend_channels_trunc(src.r, dest.r, factor),
        blend_channels_trunc(src.g, dest.g, factor),
        blend_channels_trunc(src.b, dest.b, factor),
        alpha,
    )
}

/// Python `Color.blend(dest, factor)` (alpha interpolated).
fn blend_interp(src: Color, dest: Color, factor: f32) -> Color {
    let factor = factor.clamp(0.0, 1.0);
    let alpha = src.a + (dest.a - src.a) * factor;
    Color::rgba_f(
        blend_channels_trunc(src.r, dest.r, factor),
        blend_channels_trunc(src.g, dest.g, factor),
        blend_channels_trunc(src.b, dest.b, factor),
        alpha,
    )
}

/// Python `Color.tint(color)` — combine color and alpha, keeping base alpha.
fn tint(base: Color, over: Color) -> Color {
    Color::rgba_f(
        blend_channels_trunc(base.r, over.r, over.a),
        blend_channels_trunc(base.g, over.g, over.a),
        blend_channels_trunc(base.b, over.b, over.a),
        base.a,
    )
}

/// Python `Color.__add__` (`a + b == a.blend(b, b.a, alpha=1.0)`).
fn add(base: Color, over: Color) -> Color {
    blend_alpha(base, over, over.a, 1.0)
}

fn generate_tokens(theme: &NamedTheme) -> HashMap<String, Color> {
    let mut colors: HashMap<String, Color> = HashMap::new();

    let var: HashMap<&str, &str> = theme
        .variables
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let primary = parse(&theme.primary);
    let secondary = theme.secondary.as_deref().map(parse).unwrap_or(primary);
    let warning = theme.warning.as_deref().map(parse).unwrap_or(primary);
    let error = theme.error.as_deref().map(parse).unwrap_or(secondary);
    let success = theme.success.as_deref().map(parse).unwrap_or(secondary);
    let accent = theme.accent.as_deref().map(parse).unwrap_or(primary);

    let dark = theme.dark;
    let spread = theme.luminosity_spread;

    let background = theme.background.as_deref().map(parse).unwrap_or_else(|| {
        parse(if dark {
            DEFAULT_DARK_BACKGROUND
        } else {
            DEFAULT_LIGHT_BACKGROUND
        })
    });
    let surface = theme.surface.as_deref().map(parse).unwrap_or_else(|| {
        parse(if dark {
            DEFAULT_DARK_SURFACE
        } else {
            DEFAULT_LIGHT_SURFACE
        })
    });
    let foreground = theme
        .foreground
        .as_deref()
        .map(parse)
        .unwrap_or_else(|| background.inverse());

    // Colored text + panel/boost. (`background.ansi` is always None here — ANSI
    // themes are not generated.)
    let mut boost = Color::rgba_f(0, 0, 0, 0.0); // TRANSPARENT
    let contrast_full = contrast_text(background).with_alpha(1.0);
    colors.insert(
        "text-primary".into(),
        tint(contrast_full, primary.with_alpha(0.66)),
    );
    colors.insert(
        "text-secondary".into(),
        tint(contrast_full, secondary.with_alpha(0.66)),
    );
    colors.insert(
        "text-warning".into(),
        tint(contrast_full, warning.with_alpha(0.66)),
    );
    colors.insert(
        "text-error".into(),
        tint(contrast_full, error.with_alpha(0.66)),
    );
    colors.insert(
        "text-success".into(),
        tint(contrast_full, success.with_alpha(0.66)),
    );
    colors.insert(
        "text-accent".into(),
        tint(contrast_full, accent.with_alpha(0.66)),
    );

    let panel = if let Some(p) = theme.panel.as_deref() {
        parse(p)
    } else {
        let mut panel = blend_alpha(surface, primary, 0.1, 1.0);
        if dark {
            boost = theme
                .boost
                .as_deref()
                .map(parse)
                .unwrap_or_else(|| contrast_full.with_alpha(0.04));
            panel = add(panel, boost);
        }
        panel
    };

    // Shade table for the base color families.
    let shade_colors: [(&str, Color); 13] = [
        ("primary", primary),
        ("secondary", secondary),
        ("primary-background", primary),
        ("secondary-background", secondary),
        ("background", background),
        ("foreground", foreground),
        ("panel", panel),
        ("boost", boost),
        ("surface", surface),
        ("warning", warning),
        ("error", error),
        ("success", success),
        ("accent", accent),
    ];

    let luminosity_step = spread / 2.0;
    let dark_shades = ["primary-background", "secondary-background"];

    for (name, color) in shade_colors {
        let is_dark_shade = dark && dark_shades.contains(&name);
        for n in -NUMBER_OF_SHADES..=NUMBER_OF_SHADES {
            let luminosity_delta = n as f32 * luminosity_step;
            let key = shade_key(name, n);
            if is_dark_shade {
                if let Some(v) = var.get(key.as_str()) {
                    if let Some(c) = parse_color_like(v) {
                        colors.insert(key, c);
                        continue;
                    }
                }
                let dark_background = blend_alpha(background, color, 0.15, 1.0);
                let shade_color = blend_alpha(
                    dark_background,
                    Color::rgb(255, 255, 255),
                    spread + luminosity_delta,
                    1.0,
                )
                .clamped();
                colors.insert(key, shade_color);
            } else if let Some(v) = var.get(key.as_str()) {
                colors.insert(key, parse_color_like(v).unwrap_or(color));
            } else {
                colors.insert(key, lighten_lab(color, luminosity_delta).clamped());
            }
        }
    }

    // text / text-muted / text-disabled are `auto NN%` in Python; stored as the
    // contrast-text color with the appropriate alpha (resolved like `$text`).
    let text_auto = |alpha: f32| contrast_text(background).with_alpha(alpha);
    insert_or_var(&mut colors, &var, "text", || text_auto(0.87));
    insert_or_var(&mut colors, &var, "text-muted", || text_auto(0.60));
    insert_or_var(&mut colors, &var, "text-disabled", || text_auto(0.38));

    // Muted variants of base colors (blend toward background, factor 0.7).
    insert_or_var(&mut colors, &var, "primary-muted", || {
        blend_interp(primary, background, 0.7)
    });
    insert_or_var(&mut colors, &var, "secondary-muted", || {
        blend_interp(secondary, background, 0.7)
    });
    insert_or_var(&mut colors, &var, "accent-muted", || {
        blend_interp(accent, background, 0.7)
    });
    insert_or_var(&mut colors, &var, "warning-muted", || {
        blend_interp(warning, background, 0.7)
    });
    insert_or_var(&mut colors, &var, "error-muted", || {
        blend_interp(error, background, 0.7)
    });
    insert_or_var(&mut colors, &var, "success-muted", || {
        blend_interp(success, background, 0.7)
    });

    insert_or_var(&mut colors, &var, "foreground-muted", || {
        foreground.with_alpha(0.6)
    });
    insert_or_var(&mut colors, &var, "foreground-disabled", || {
        foreground.with_alpha(0.38)
    });

    // Block cursor / hover.
    let text = *colors.get("text").unwrap();
    insert_or_var(&mut colors, &var, "block-cursor-foreground", || text);
    insert_or_var(&mut colors, &var, "block-cursor-background", || primary);
    insert_or_var(&mut colors, &var, "block-cursor-blurred-foreground", || {
        foreground
    });
    insert_or_var(&mut colors, &var, "block-cursor-blurred-background", || {
        primary.with_alpha(0.3)
    });
    insert_or_var(&mut colors, &var, "block-hover-background", || {
        boost.with_alpha(0.1)
    });

    // Borders / surface-active.
    insert_or_var(&mut colors, &var, "border", || primary);
    insert_or_var(&mut colors, &var, "border-blurred", || {
        darken_lab(surface, 0.025).clamped()
    });
    insert_or_var(&mut colors, &var, "surface-active", || {
        lighten_lab(surface, spread / 2.5).clamped()
    });

    // Scrollbars: `background-darken-1 + primary.with_alpha(0.4/0.5)`.
    let background_darken_1 = *colors.get("background-darken-1").unwrap();
    insert_or_var(&mut colors, &var, "scrollbar", || {
        add(background_darken_1, primary.with_alpha(0.4))
    });
    insert_or_var(&mut colors, &var, "scrollbar-hover", || {
        add(background_darken_1, primary.with_alpha(0.5))
    });
    insert_or_var(&mut colors, &var, "scrollbar-active", || primary);
    insert_or_var(&mut colors, &var, "scrollbar-background", || {
        background_darken_1
    });
    let scrollbar_background = *colors.get("scrollbar-background").unwrap();
    insert_or_var(&mut colors, &var, "scrollbar-corner-color", || {
        scrollbar_background
    });
    insert_or_var(&mut colors, &var, "scrollbar-background-hover", || {
        scrollbar_background
    });
    insert_or_var(&mut colors, &var, "scrollbar-background-active", || {
        scrollbar_background
    });

    // Links.
    insert_or_var(&mut colors, &var, "link-background", || {
        Color::rgba(0, 0, 0, 0)
    });
    insert_or_var(&mut colors, &var, "link-background-hover", || primary);
    insert_or_var(&mut colors, &var, "link-color", || text);
    insert_or_var(&mut colors, &var, "link-color-hover", || text);

    // Footer.
    insert_or_var(&mut colors, &var, "footer-foreground", || foreground);
    insert_or_var(&mut colors, &var, "footer-background", || panel);
    insert_or_var(&mut colors, &var, "footer-key-foreground", || accent);
    insert_or_var(&mut colors, &var, "footer-key-background", || {
        Color::rgba(0, 0, 0, 0)
    });
    insert_or_var(&mut colors, &var, "footer-description-foreground", || {
        foreground
    });
    insert_or_var(&mut colors, &var, "footer-description-background", || {
        Color::rgba(0, 0, 0, 0)
    });
    insert_or_var(&mut colors, &var, "footer-item-background", || {
        Color::rgba(0, 0, 0, 0)
    });

    // Input.
    insert_or_var(&mut colors, &var, "input-cursor-background", || foreground);
    insert_or_var(&mut colors, &var, "input-cursor-foreground", || background);
    let primary_lighten_1 = *colors.get("primary-lighten-1").unwrap();
    insert_or_var(&mut colors, &var, "input-selection-background", || {
        primary_lighten_1.with_alpha(0.4)
    });
    insert_or_var(&mut colors, &var, "input-selection-foreground", || {
        foreground
    });

    // Markdown headers.
    insert_or_var(&mut colors, &var, "markdown-h1-color", || primary);
    insert_or_var(&mut colors, &var, "markdown-h2-color", || primary);
    insert_or_var(&mut colors, &var, "markdown-h3-color", || primary);
    insert_or_var(&mut colors, &var, "markdown-h4-color", || foreground);
    insert_or_var(&mut colors, &var, "markdown-h5-color", || foreground);
    let foreground_muted = *colors.get("foreground-muted").unwrap();
    insert_or_var(&mut colors, &var, "markdown-h6-color", || foreground_muted);

    // Buttons.
    insert_or_var(&mut colors, &var, "button-foreground", || foreground);
    insert_or_var(&mut colors, &var, "button-color-foreground", || text);

    colors
}

fn shade_key(name: &str, n: i32) -> String {
    use std::cmp::Ordering;
    match n.cmp(&0) {
        Ordering::Less => format!("{name}-darken-{}", n.abs()),
        Ordering::Greater => format!("{name}-lighten-{n}"),
        Ordering::Equal => name.to_string(),
    }
}

/// Insert `name` from a theme variable override if present (parsed as a color),
/// otherwise from the computed default.
fn insert_or_var(
    colors: &mut HashMap<String, Color>,
    var: &HashMap<&str, &str>,
    name: &str,
    default: impl FnOnce() -> Color,
) {
    if let Some(v) = var.get(name) {
        if let Some(c) = parse_color_like(v) {
            colors.insert(name.to_string(), c);
            return;
        }
    }
    colors.insert(name.to_string(), default());
}

// ---------------------------------------------------------------------------
// Global registry + active theme
// ---------------------------------------------------------------------------

struct Registry {
    themes: HashMap<String, NamedTheme>,
    /// Name of the active theme, or `None` for the default (`textual-dark`)
    /// resolution path in `style.rs`.
    active: Option<String>,
    /// Generated token map for the active theme (empty when default is active).
    active_tokens: HashMap<String, Color>,
}

fn registry() -> &'static Mutex<Registry> {
    static REG: OnceLock<Mutex<Registry>> = OnceLock::new();
    REG.get_or_init(|| {
        let mut themes = HashMap::new();
        for theme in builtin_themes() {
            themes.insert(theme.name.clone(), theme);
        }
        Mutex::new(Registry {
            themes,
            active: None,
            active_tokens: HashMap::new(),
        })
    })
}

/// Register (or replace) a named theme.
pub fn register_theme(theme: NamedTheme) {
    let mut reg = registry().lock().unwrap();
    let name = theme.name.clone();
    reg.themes.insert(name.clone(), theme);
    // If the replaced theme is currently active, regenerate its tokens.
    if reg.active.as_deref() == Some(name.as_str()) {
        let regenerated = reg.themes.get(&name).unwrap().generate();
        reg.active_tokens = regenerated;
    }
}

/// Names of all registered themes, sorted.
pub fn available_theme_names() -> Vec<String> {
    let reg = registry().lock().unwrap();
    let mut names: Vec<String> = reg.themes.keys().cloned().collect();
    names.sort();
    names
}

/// Look up a registered theme by name.
pub fn get_theme(name: &str) -> Option<NamedTheme> {
    let reg = registry().lock().unwrap();
    reg.themes.get(name).cloned()
}

/// The currently active theme name (`textual-dark` if the default path is in use).
pub fn active_theme_name() -> String {
    let reg = registry().lock().unwrap();
    reg.active
        .clone()
        .unwrap_or_else(|| "textual-dark".to_string())
}

/// Activate a named theme. Returns `false` if no such theme is registered.
///
/// When the activated theme is the default `textual-dark`, the global override
/// is cleared so the hand-tuned static path in `style.rs` is used (preserving
/// the calibrated goldens).
pub fn set_active_theme(name: &str) -> bool {
    let mut reg = registry().lock().unwrap();
    let Some(theme) = reg.themes.get(name).cloned() else {
        return false;
    };
    if name == "textual-dark" {
        reg.active = None;
        reg.active_tokens = HashMap::new();
    } else {
        reg.active = Some(name.to_string());
        reg.active_tokens = theme.generate();
    }
    true
}

/// Resolve a design token (e.g. `primary`, `text-error`) against the active
/// non-default theme. Returns `None` when the default path should be used.
pub(crate) fn active_token(name: &str) -> Option<Color> {
    let reg = registry().lock().unwrap();
    if reg.active.is_none() {
        return None;
    }
    if let Some(color) = reg.active_tokens.get(name).copied() {
        return Some(color);
    }
    // Derived `-muted` for any base color not pre-generated.
    if let Some(base) = name.strip_suffix("-muted") {
        if let (Some(c), Some(bg)) = (
            reg.active_tokens.get(base).copied(),
            reg.active_tokens.get("background").copied(),
        ) {
            return Some(blend_interp(c, bg, 0.7));
        }
    }
    // Active theme present but token unknown: don't fall back to the dark
    // static table (which would mix two themes). `$text`/`$text-muted` etc.
    // resolve through the `auto NN%` path in `style.rs`, which is
    // theme-independent (alpha-only), so a miss here is expected only for the
    // handful of `auto` tokens and is handled there.
    None
}

// ---------------------------------------------------------------------------
// BUILTIN_THEMES — exact port of Python textual/theme.py
// ---------------------------------------------------------------------------

fn vars(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// The built-in named themes, ported exactly from Python `BUILTIN_THEMES`.
pub fn builtin_themes() -> Vec<NamedTheme> {
    let mut out = Vec::new();

    out.push({
        let mut t = NamedTheme::builder("textual-dark", "#0178D4");
        t.secondary = Some("#004578".into());
        t.accent = Some("#ffa62b".into());
        t.warning = Some("#ffa62b".into());
        t.error = Some("#ba3c5b".into());
        t.success = Some("#4EBF71".into());
        t.foreground = Some("#e0e0e0".into());
        t
    });

    out.push({
        let mut t = NamedTheme::builder("textual-light", "#004578");
        t.secondary = Some("#0178D4".into());
        t.accent = Some("#ffa62b".into());
        t.warning = Some("#ffa62b".into());
        t.error = Some("#ba3c5b".into());
        t.success = Some("#4EBF71".into());
        t.surface = Some("#D8D8D8".into());
        t.panel = Some("#D0D0D0".into());
        t.background = Some("#E0E0E0".into());
        t.dark = false;
        t.variables = vars(&[("footer-key-foreground", "#0178D4")]);
        t
    });

    out.push({
        let mut t = NamedTheme::builder("nord", "#88C0D0");
        t.secondary = Some("#81A1C1".into());
        t.accent = Some("#B48EAD".into());
        t.foreground = Some("#D8DEE9".into());
        t.background = Some("#2E3440".into());
        t.success = Some("#A3BE8C".into());
        t.warning = Some("#EBCB8B".into());
        t.error = Some("#BF616A".into());
        t.surface = Some("#3B4252".into());
        t.panel = Some("#434C5E".into());
        t.variables = vars(&[
            ("block-cursor-background", "#88C0D0"),
            ("block-cursor-foreground", "#2E3440"),
            ("block-cursor-text-style", "none"),
            ("footer-key-foreground", "#88C0D0"),
            ("input-selection-background", "#81a1c1 35%"),
            ("button-color-foreground", "#2E3440"),
            ("button-focus-text-style", "reverse"),
        ]);
        t
    });

    out.push({
        let mut t = NamedTheme::builder("gruvbox", "#85A598");
        t.secondary = Some("#A89A85".into());
        t.warning = Some("#fe8019".into());
        t.error = Some("#fb4934".into());
        t.success = Some("#b8bb26".into());
        t.accent = Some("#fabd2f".into());
        t.foreground = Some("#fbf1c7".into());
        t.background = Some("#282828".into());
        t.surface = Some("#3c3836".into());
        t.panel = Some("#504945".into());
        t.variables = vars(&[
            ("block-cursor-foreground", "#fbf1c7"),
            ("input-selection-background", "#689d6a40"),
            ("button-color-foreground", "#282828"),
        ]);
        t
    });

    out.push({
        let mut t = NamedTheme::builder("catppuccin-mocha", "#F5C2E7");
        t.secondary = Some("#cba6f7".into());
        t.warning = Some("#FAE3B0".into());
        t.error = Some("#F28FAD".into());
        t.success = Some("#ABE9B3".into());
        t.accent = Some("#fab387".into());
        t.foreground = Some("#cdd6f4".into());
        t.background = Some("#181825".into());
        t.surface = Some("#313244".into());
        t.panel = Some("#45475a".into());
        t.variables = vars(&[
            ("input-cursor-foreground", "#11111b"),
            ("input-cursor-background", "#f5e0dc"),
            ("input-selection-background", "#9399b2 30%"),
            ("border", "#b4befe"),
            ("border-blurred", "#585b70"),
            ("footer-background", "#45475a"),
            ("block-cursor-foreground", "#1e1e2e"),
            ("block-cursor-text-style", "none"),
            ("button-color-foreground", "#181825"),
        ]);
        t
    });

    out.push({
        let mut t = NamedTheme::builder("dracula", "#BD93F9");
        t.secondary = Some("#6272A4".into());
        t.warning = Some("#FFB86C".into());
        t.error = Some("#FF5555".into());
        t.success = Some("#50FA7B".into());
        t.accent = Some("#FF79C6".into());
        t.background = Some("#282A36".into());
        t.surface = Some("#2B2E3B".into());
        t.panel = Some("#313442".into());
        t.foreground = Some("#F8F8F2".into());
        t.variables = vars(&[("button-color-foreground", "#282A36")]);
        t
    });

    out.push({
        let mut t = NamedTheme::builder("tokyo-night", "#BB9AF7");
        t.secondary = Some("#7AA2F7".into());
        t.warning = Some("#E0AF68".into());
        t.error = Some("#F7768E".into());
        t.success = Some("#9ECE6A".into());
        t.accent = Some("#FF9E64".into());
        t.foreground = Some("#a9b1d6".into());
        t.background = Some("#1A1B26".into());
        t.surface = Some("#24283B".into());
        t.panel = Some("#414868".into());
        t.variables = vars(&[("button-color-foreground", "#24283B")]);
        t
    });

    out.push({
        let mut t = NamedTheme::builder("monokai", "#AE81FF");
        t.secondary = Some("#F92672".into());
        t.accent = Some("#66D9EF".into());
        t.warning = Some("#FD971F".into());
        t.error = Some("#F92672".into());
        t.success = Some("#A6E22E".into());
        t.foreground = Some("#d6d6d6".into());
        t.background = Some("#272822".into());
        t.surface = Some("#2e2e2e".into());
        t.panel = Some("#3E3D32".into());
        t.variables = vars(&[
            ("foreground-muted", "#797979"),
            ("input-selection-background", "#575b6190"),
            ("button-color-foreground", "#272822"),
        ]);
        t
    });

    out.push({
        let mut t = NamedTheme::builder("flexoki", "#205EA6");
        t.secondary = Some("#24837B".into());
        t.warning = Some("#AD8301".into());
        t.error = Some("#AF3029".into());
        t.success = Some("#66800B".into());
        t.accent = Some("#9B76C8".into());
        t.background = Some("#100F0F".into());
        t.surface = Some("#1C1B1A".into());
        t.panel = Some("#282726".into());
        t.foreground = Some("#FFFCF0".into());
        t.variables = vars(&[
            ("input-cursor-foreground", "#5E409D"),
            ("input-cursor-background", "#FFFCF0"),
            ("input-selection-background", "#6F6E69 35%"),
            ("button-color-foreground", "#FFFCF0"),
        ]);
        t
    });

    out.push({
        let mut t = NamedTheme::builder("catppuccin-latte", "#8839EF");
        t.secondary = Some("#DC8A78".into());
        t.warning = Some("#DF8E1D".into());
        t.error = Some("#D20F39".into());
        t.success = Some("#40A02B".into());
        t.accent = Some("#FE640B".into());
        t.foreground = Some("#4C4F69".into());
        t.background = Some("#EFF1F5".into());
        t.surface = Some("#E6E9EF".into());
        t.panel = Some("#CCD0DA".into());
        t.dark = false;
        t.variables = vars(&[("button-color-foreground", "#EFF1F5")]);
        t
    });

    out.push({
        let mut t = NamedTheme::builder("catppuccin-frappe", "#CA9EE6");
        t.secondary = Some("#EF9F76".into());
        t.warning = Some("#E5C890".into());
        t.error = Some("#E78284".into());
        t.success = Some("#A6D189".into());
        t.accent = Some("#F4B8E4".into());
        t.foreground = Some("#C6D0F5".into());
        t.background = Some("#303446".into());
        t.surface = Some("#414559".into());
        t.panel = Some("#51576D".into());
        t.dark = true;
        t.variables = vars(&[
            ("input-cursor-foreground", "#232634"),
            ("input-cursor-background", "#F2D5CF"),
            ("input-selection-background", "#949CBB 30%"),
            ("border", "#BABBF1"),
            ("border-blurred", "#838BA7"),
            ("footer-background", "#51576D"),
            ("block-cursor-foreground", "#292C3C"),
            ("block-cursor-text-style", "none"),
            ("button-color-foreground", "#303446"),
        ]);
        t
    });

    out.push({
        let mut t = NamedTheme::builder("catppuccin-macchiato", "#C6A0F6");
        t.secondary = Some("#F5A97F".into());
        t.warning = Some("#EED49F".into());
        t.error = Some("#ED8796".into());
        t.success = Some("#A6DA95".into());
        t.accent = Some("#F5BDE6".into());
        t.foreground = Some("#CAD3F5".into());
        t.background = Some("#24273A".into());
        t.surface = Some("#363A4F".into());
        t.panel = Some("#494D64".into());
        t.dark = true;
        t.variables = vars(&[
            ("input-cursor-foreground", "#181926"),
            ("input-cursor-background", "#F4DBD6"),
            ("input-selection-background", "#838BA7 30%"),
            ("border", "#B7BDF8"),
            ("border-blurred", "#737994"),
            ("footer-background", "#494D64"),
            ("block-cursor-foreground", "#1E2030"),
            ("block-cursor-text-style", "none"),
            ("button-color-foreground", "#24273A"),
        ]);
        t
    });

    out.push({
        let mut t = NamedTheme::builder("solarized-light", "#268bd2");
        t.secondary = Some("#2aa198".into());
        t.warning = Some("#cb4b16".into());
        t.error = Some("#dc322f".into());
        t.success = Some("#859900".into());
        t.accent = Some("#6c71c4".into());
        t.foreground = Some("#586e75".into());
        t.background = Some("#fdf6e3".into());
        t.surface = Some("#eee8d5".into());
        t.panel = Some("#eee8d5".into());
        t.dark = false;
        t.variables = vars(&[
            ("button-color-foreground", "#fdf6e3"),
            ("footer-background", "#268bd2"),
            ("footer-key-foreground", "#fdf6e3"),
            ("footer-description-foreground", "#fdf6e3"),
        ]);
        t
    });

    out.push({
        let mut t = NamedTheme::builder("solarized-dark", "#268bd2");
        t.secondary = Some("#2aa198".into());
        t.warning = Some("#cb4b16".into());
        t.error = Some("#dc322f".into());
        t.success = Some("#859900".into());
        t.accent = Some("#6c71c4".into());
        t.background = Some("#002b36".into());
        t.surface = Some("#073642".into());
        t.panel = Some("#073642".into());
        t.foreground = Some("#839496".into());
        t.dark = true;
        t.variables = vars(&[
            ("button-color-foreground", "#fdf6e3"),
            ("footer-background", "#268bd2"),
            ("footer-key-foreground", "#fdf6e3"),
            ("footer-description-foreground", "#fdf6e3"),
            ("input-selection-background", "#073642"),
        ]);
        t
    });

    out.push({
        let mut t = NamedTheme::builder("rose-pine", "#c4a7e7");
        t.secondary = Some("#31748f".into());
        t.warning = Some("#f6c177".into());
        t.error = Some("#eb6f92".into());
        t.success = Some("#9ccfd8".into());
        t.accent = Some("#ebbcba".into());
        t.foreground = Some("#e0def4".into());
        t.background = Some("#191724".into());
        t.surface = Some("#1f1d2e".into());
        t.panel = Some("#26233a".into());
        t.dark = true;
        t.variables = vars(&[
            ("input-cursor-background", "#f4ede8"),
            ("input-selection-background", "#403d52"),
            ("border", "#524f67"),
            ("border-blurred", "#6e6a86"),
            ("footer-background", "#26233a"),
            ("block-cursor-foreground", "#191724"),
            ("block-cursor-text-style", "none"),
            ("block-cursor-background", "#c4a7e7"),
        ]);
        t
    });

    out.push({
        let mut t = NamedTheme::builder("rose-pine-moon", "#c4a7e7");
        t.secondary = Some("#3e8fb0".into());
        t.warning = Some("#f6c177".into());
        t.error = Some("#eb6f92".into());
        t.success = Some("#9ccfd8".into());
        t.accent = Some("#ea9a97".into());
        t.foreground = Some("#e0def4".into());
        t.background = Some("#232136".into());
        t.surface = Some("#2a273f".into());
        t.panel = Some("#393552".into());
        t.dark = true;
        t.variables = vars(&[
            ("input-cursor-background", "#f4ede8"),
            ("input-selection-background", "#44415a"),
            ("border", "#56526e"),
            ("border-blurred", "#6e6a86"),
            ("footer-background", "#393552"),
            ("block-cursor-foreground", "#232136"),
            ("block-cursor-text-style", "none"),
            ("block-cursor-background", "#c4a7e7"),
        ]);
        t
    });

    out.push({
        let mut t = NamedTheme::builder("rose-pine-dawn", "#907aa9");
        t.secondary = Some("#286983".into());
        t.warning = Some("#ea9d34".into());
        t.error = Some("#b4637a".into());
        t.success = Some("#56949f".into());
        t.accent = Some("#d7827e".into());
        t.foreground = Some("#575279".into());
        t.background = Some("#faf4ed".into());
        t.surface = Some("#fffaf3".into());
        t.panel = Some("#f2e9e1".into());
        t.dark = false;
        t.variables = vars(&[
            ("input-cursor-background", "#575279"),
            ("input-selection-background", "#dfdad9"),
            ("border", "#cecacd"),
            ("border-blurred", "#9893a5"),
            ("footer-background", "#f2e9e1"),
            ("block-cursor-foreground", "#faf4ed"),
            ("block-cursor-text-style", "none"),
            ("block-cursor-background", "#575279"),
        ]);
        t
    });

    out.push({
        let mut t = NamedTheme::builder("atom-one-dark", "#61AFEF");
        t.secondary = Some("#C678DD".into());
        t.warning = Some("#DEB25B".into());
        t.error = Some("#F06262".into());
        t.success = Some("#62F062".into());
        t.accent = Some("#A378C2".into());
        t.foreground = Some("#ABB2BF".into());
        t.background = Some("#282C34".into());
        t.surface = Some("#3B414D".into());
        t.panel = Some("#4F5666".into());
        t.dark = true;
        t
    });

    out.push({
        let mut t = NamedTheme::builder("atom-one-light", "#4078F2");
        t.secondary = Some("#A626A4".into());
        t.warning = Some("#D8D938".into());
        t.error = Some("#F23F3F".into());
        t.success = Some("#6CF23F".into());
        t.accent = Some("#bf9232".into());
        t.foreground = Some("#383A42".into());
        t.background = Some("#FAFAFA".into());
        t.surface = Some("#E0E0E0".into());
        t.panel = Some("#CCCCCC".into());
        t.dark = false;
        t
    });

    out.push({
        let mut t = NamedTheme::builder("ansi-dark", "ansi_blue");
        t.ansi = true;
        t.secondary = Some("ansi_cyan".into());
        t.warning = Some("ansi_yellow".into());
        t.error = Some("ansi_red".into());
        t.success = Some("ansi_green".into());
        t.accent = Some("ansi_green".into());
        t.foreground = Some("ansi_default".into());
        t.background = Some("ansi_default".into());
        t.surface = Some("ansi_default".into());
        t.panel = Some("ansi_default".into());
        t.boost = Some("ansi_default".into());
        t.dark = true;
        t
    });

    out.push({
        let mut t = NamedTheme::builder("ansi-light", "ansi_blue");
        t.ansi = true;
        t.secondary = Some("ansi_cyan".into());
        t.warning = Some("ansi_bright_red".into());
        t.error = Some("ansi_red".into());
        t.success = Some("ansi_green".into());
        t.accent = Some("ansi_magenta".into());
        t.foreground = Some("ansi_default".into());
        t.background = Some("ansi_default".into());
        t.surface = Some("ansi_default".into());
        t.panel = Some("ansi_default".into());
        t.boost = Some("ansi_default".into());
        t.dark = false;
        t
    });

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_builtins_registered() {
        let names = available_theme_names();
        for expected in [
            "textual-dark",
            "textual-light",
            "nord",
            "gruvbox",
            "tokyo-night",
            "solarized-light",
            "dracula",
            "monokai",
        ] {
            assert!(
                names.contains(&expected.to_string()),
                "missing builtin theme {expected}"
            );
        }
    }

    fn rgb_of(t: &HashMap<String, Color>, key: &str) -> (u8, u8, u8) {
        let c = t.get(key).unwrap_or_else(|| panic!("missing token {key}"));
        (c.r, c.g, c.b)
    }

    fn hexrgb(s: &str) -> (u8, u8, u8) {
        let c = Color::parse(s).unwrap();
        (c.r, c.g, c.b)
    }

    #[test]
    fn nord_generated_tokens_match_python() {
        // Exact values from Python `ColorSystem.generate()` for the nord theme
        // (computed from textual/design.py). These exercise base copy, derived
        // muted/text/shade tokens, and theme-variable overrides.
        let tokens = get_theme("nord").unwrap().generate();
        assert_eq!(rgb_of(&tokens, "primary"), hexrgb("#88C0D0"));
        assert_eq!(rgb_of(&tokens, "background"), hexrgb("#2E3440"));
        assert_eq!(rgb_of(&tokens, "foreground"), hexrgb("#D8DEE9"));
        assert_eq!(rgb_of(&tokens, "panel"), hexrgb("#434C5E"));
        assert_eq!(rgb_of(&tokens, "primary-muted"), hexrgb("#495E6B"));
        assert_eq!(rgb_of(&tokens, "error-muted"), hexrgb("#59414C"));
        assert_eq!(rgb_of(&tokens, "text-error"), hexrgb("#D4969C"));
        assert_eq!(rgb_of(&tokens, "primary-lighten-1"), hexrgb("#9CD4E5"));
        assert_eq!(rgb_of(&tokens, "surface-active"), hexrgb("#484F60"));
        assert_eq!(rgb_of(&tokens, "scrollbar"), hexrgb("#48626F"));
        // Variable override.
        assert_eq!(rgb_of(&tokens, "footer-key-foreground"), hexrgb("#88C0D0"));
    }

    #[test]
    fn solarized_light_generated_tokens_match_python() {
        let tokens = get_theme("solarized-light").unwrap().generate();
        assert_eq!(rgb_of(&tokens, "primary"), hexrgb("#268BD2"));
        assert_eq!(rgb_of(&tokens, "background"), hexrgb("#FDF6E3"));
        assert_eq!(rgb_of(&tokens, "foreground"), hexrgb("#586E75"));
        assert_eq!(rgb_of(&tokens, "text-error"), hexrgb("#91211F"));
        assert_eq!(rgb_of(&tokens, "error-muted"), hexrgb("#F3BBAD"));
        // Variable overrides.
        assert_eq!(rgb_of(&tokens, "footer-background"), hexrgb("#268bd2"));
        assert_eq!(rgb_of(&tokens, "footer-key-foreground"), hexrgb("#fdf6e3"));
    }
}
