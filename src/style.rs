use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn with_alpha(self, alpha: f32) -> Self {
        let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
        Self { a, ..self }
    }

    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if value.is_empty() {
            return None;
        }

        // rgba(r,g,b,a)
        if let Some(args) = value
            .strip_prefix("rgba(")
            .and_then(|s| s.strip_suffix(')'))
        {
            let parts: Vec<&str> = args.split(',').map(|p| p.trim()).collect();
            if parts.len() == 4 {
                let r: u8 = parts[0].parse().ok()?;
                let g: u8 = parts[1].parse().ok()?;
                let b: u8 = parts[2].parse().ok()?;
                let a_f: f32 = parts[3].parse().ok()?;
                return Some(Color::rgba(
                    r,
                    g,
                    b,
                    (a_f.clamp(0.0, 1.0) * 255.0).round() as u8,
                ));
            }
        }

        // Try rich-rs parsing (named colors, #RRGGBB, etc.).
        if let Some(color) = rich_rs::SimpleColor::parse(value) {
            return Some(color_from_simple(color));
        }

        // Hex with alpha: #RRGGBBAA
        if let Some(hex) = value.strip_prefix('#') {
            if hex.len() == 8 {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                return Some(Self::rgba(r, g, b, a));
            }
        }

        None
    }

    pub fn to_simple_opaque(self) -> rich_rs::SimpleColor {
        rich_rs::SimpleColor::Rgb {
            r: self.r,
            g: self.g,
            b: self.b,
        }
    }

    pub fn flatten_over(self, under: Color) -> Color {
        if self.a == 255 {
            return Color::rgb(self.r, self.g, self.b);
        }
        if self.a == 0 {
            return Color::rgb(under.r, under.g, under.b);
        }
        let oa = self.a as u32;
        let inv = 255u32 - oa;
        let mix =
            |o: u8, u: u8| -> u8 { (((o as u32) * oa + (u as u32) * inv) / 255u32).min(255) as u8 };
        Color::rgb(
            mix(self.r, under.r),
            mix(self.g, under.g),
            mix(self.b, under.b),
        )
    }
}

pub(crate) fn color_from_simple(color: rich_rs::SimpleColor) -> Color {
    match color {
        rich_rs::SimpleColor::Rgb { r, g, b } => Color::rgb(r, g, b),
        other => {
            // Convert indexed colors via their palette hex.
            let hex = other.get_hex();
            if let Some(rich_rs::SimpleColor::Rgb { r, g, b }) = rich_rs::SimpleColor::parse(&hex) {
                Color::rgb(r, g, b)
            } else {
                Color::rgb(255, 255, 255)
            }
        }
    }
}

pub fn parse_color_like(value: &str) -> Option<Color> {
    // Fast path: try rich-rs simple color parsing.
    if let Some(color) = Color::parse(value.trim()) {
        return Some(color);
    }
    // Token / variable syntax: `$name` and `$name-lighten-1` / `$name-darken-2`.
    for token in value.split_whitespace() {
        if let Some(color) = resolve_color_token(token) {
            return Some(color);
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoColor {
    pub alpha_percent: u8,
}

impl AutoColor {
    pub fn new(alpha_percent: u8) -> Self {
        Self {
            alpha_percent: alpha_percent.min(100),
        }
    }

    pub fn alpha(self) -> f32 {
        self.alpha_percent as f32 / 100.0
    }
}

pub fn parse_auto_color_like(value: &str) -> Option<AutoColor> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    let tokens: Vec<&str> = value.split_whitespace().filter(|t| !t.is_empty()).collect();
    if tokens.is_empty() {
        return None;
    }

    if tokens[0].eq_ignore_ascii_case("auto") {
        let mut percent = 100;
        for token in tokens.iter().skip(1) {
            if let Some(raw) = token.strip_suffix('%') {
                if let Ok(parsed) = raw.parse::<u8>() {
                    percent = parsed.min(100);
                }
            }
        }
        return Some(AutoColor::new(percent));
    }

    for token in tokens {
        if let Some(name) = token.strip_prefix('$') {
            if let Some(auto) = resolve_textual_dark_auto_token(name) {
                return Some(auto);
            }
        }
    }
    None
}

fn resolve_textual_dark_auto_token(name: &str) -> Option<AutoColor> {
    match name {
        "text" | "button-color-foreground" => Some(AutoColor::new(87)),
        "text-muted" => Some(AutoColor::new(60)),
        "text-disabled" => Some(AutoColor::new(38)),
        _ => None,
    }
}

fn resolve_color_token(token: &str) -> Option<Color> {
    let token = token.trim();
    let name = token.strip_prefix('$')?;
    resolve_textual_dark_token(name)
}

fn resolve_textual_dark_token(name: &str) -> Option<Color> {
    // MVP: approximate Textual's default "textual-dark" theme.
    // Source of base values (Python Textual): `textual/theme.py` + `textual/design.py`.
    // We intentionally keep this simple: truecolor RGB + linear blend lighten/darken.
    use std::sync::OnceLock;

    static BASE: OnceLock<std::collections::HashMap<&'static str, Color>> = OnceLock::new();
    let base = BASE.get_or_init(|| {
        let mut m = std::collections::HashMap::new();
        // Theme "textual-dark" from Python Textual.
        m.insert("primary", Color::parse("#0178D4").unwrap());
        m.insert("secondary", Color::parse("#004578").unwrap());
        m.insert("accent", Color::parse("#FEA62B").unwrap());
        m.insert("warning", Color::parse("#FEA62B").unwrap());
        m.insert("error", Color::parse("#B93C5B").unwrap());
        m.insert("success", Color::parse("#4EBF71").unwrap());
        m.insert("foreground", Color::parse("#E0E0E0").unwrap());
        // Defaults from `textual/design.py` for dark mode.
        m.insert("background", Color::parse("#121212").unwrap());
        m.insert("surface", Color::parse("#1E1E1E").unwrap());
        // Approximated default panel for textual-dark (Textual computes panel from surface + primary,
        // then adds a subtle boost for dark themes).
        let panel = {
            let surface = m.get("surface").copied().unwrap();
            let primary = m.get("primary").copied().unwrap();
            let background = m.get("background").copied().unwrap();
            let base = blend(surface, primary, 0.10);
            let boost = contrast_text(background).with_alpha(0.04);
            boost.flatten_over(base)
        };
        m.insert("panel", panel);

        let background = m.get("background").copied().unwrap();
        let foreground = m.get("foreground").copied().unwrap();
        let surface = m.get("surface").copied().unwrap();
        let primary = m.get("primary").copied().unwrap();
        let accent = m.get("accent").copied().unwrap();
        let contrast = contrast_text(background);

        // Textual's generated semantic colors for textual-dark.
        m.insert("boost", contrast.with_alpha(0.04));
        m.insert("text", contrast.with_alpha(0.87));
        m.insert("text-muted", contrast.with_alpha(0.60));
        m.insert("text-disabled", contrast.with_alpha(0.38));
        m.insert("text-primary", Color::parse("#57A5E2").unwrap());
        m.insert("text-secondary", Color::parse("#5684A5").unwrap());
        m.insert("text-warning", Color::parse("#FFC473").unwrap());
        m.insert("text-error", Color::parse("#D17E92").unwrap());
        m.insert("text-success", Color::parse("#8AD4A1").unwrap());
        m.insert("text-accent", Color::parse("#FFC473").unwrap());
        m.insert("foreground-muted", Color::parse("#E0E0E099").unwrap());
        m.insert("foreground-disabled", Color::parse("#E0E0E060").unwrap());
        m.insert("surface-active", Color::parse("#2A2A2A").unwrap());
        m.insert("button-foreground", foreground);
        m.insert("button-color-foreground", contrast.with_alpha(0.87));

        // Exact textual-dark shades used by Button and related widgets.
        m.insert("surface-lighten-1", Color::parse("#2D2D2D").unwrap());
        m.insert("surface-darken-1", Color::parse("#0D0D0D").unwrap());
        m.insert("primary-lighten-3", Color::parse("#6DB2FF").unwrap());
        m.insert("primary-darken-3", Color::parse("#004295").unwrap());
        m.insert("primary-darken-2", Color::parse("#0053AA").unwrap());
        m.insert("primary-muted", Color::parse("#0C304C").unwrap());
        m.insert("success-lighten-2", Color::parse("#7AE998").unwrap());
        m.insert("success-darken-3", Color::parse("#008139").unwrap());
        m.insert("success-darken-2", Color::parse("#18954B").unwrap());
        m.insert("success-muted", Color::parse("#24452E").unwrap());
        m.insert("warning-lighten-2", Color::parse("#FFCF56").unwrap());
        m.insert("warning-darken-3", Color::parse("#B86B00").unwrap());
        m.insert("warning-darken-2", Color::parse("#CF7E00").unwrap());
        m.insert("warning-muted", Color::parse("#593E19").unwrap());
        m.insert("error-lighten-2", Color::parse("#E76580").unwrap());
        m.insert("error-darken-3", Color::parse("#780028").unwrap());
        m.insert("error-darken-2", Color::parse("#8D0638").unwrap());
        m.insert("error-darken-1", Color::parse("#A32549").unwrap());
        m.insert("error-muted", Color::parse("#441E27").unwrap());

        // Footer and link color tokens used by builtin styles.
        m.insert("footer-foreground", foreground);
        m.insert("footer-background", panel);
        m.insert("footer-key-foreground", accent);
        m.insert("footer-key-background", Color::rgba(0, 0, 0, 0));
        m.insert("footer-description-foreground", foreground);
        m.insert("footer-description-background", Color::rgba(0, 0, 0, 0));
        m.insert("footer-item-background", Color::rgba(0, 0, 0, 0));
        m.insert("link-background-hover", primary);
        m.insert("link-color", contrast.with_alpha(0.87));
        m.insert("link-color-hover", contrast.with_alpha(0.87));

        // Cursor/hover tokens from design defaults.
        m.insert("block-cursor-foreground", contrast.with_alpha(0.87));
        m.insert("block-cursor-background", primary);
        m.insert("block-cursor-blurred-foreground", foreground);
        m.insert("block-cursor-blurred-background", primary.with_alpha(0.30));
        // Textual's `$block-hover-background`: contrast text at 10% alpha, composed at render time.
        let background = m.get("background").copied().unwrap();
        let ct = contrast_text(background);
        m.insert("block-hover-background", ct.with_alpha(0.10));
        // Textual's datatable--header-hover: `$accent 30%` (alpha), composed at render time.
        m.insert("header-hover-background", accent.with_alpha(0.30));

        // Focused / blurred border tokens (used by many built-in widgets in Textual).
        m.insert("border", primary);
        m.insert("border-blurred", darken_lab(surface, 0.025));

        // Input tokens (Textual uses these for cursor and selection styling).
        m.insert(
            "input-cursor-background",
            m.get("foreground").copied().unwrap(),
        );
        m.insert(
            "input-cursor-foreground",
            m.get("background").copied().unwrap(),
        );
        let selection = lighten_lab(primary, 0.15 / 2.0).with_alpha(0.40);
        m.insert("input-selection-background", selection);
        m.insert("markdown-h1-color", primary);
        m.insert("markdown-h2-color", primary);
        m.insert("markdown-h3-color", primary);
        m.insert("markdown-h4-color", foreground);
        m.insert("markdown-h5-color", foreground);
        m.insert("markdown-h6-color", Color::parse("#E0E0E099").unwrap());

        // Scrollbar tokens (mirrors Textual dark design defaults closely enough for parity).
        let scrollbar_background = darken_lab(background, 0.15 / 2.0);
        let scrollbar = blend(scrollbar_background, primary, 0.40);
        let scrollbar_hover = blend(scrollbar_background, primary, 0.50);
        m.insert("scrollbar", scrollbar);
        m.insert("scrollbar-hover", scrollbar_hover);
        m.insert("scrollbar-active", primary);
        m.insert("scrollbar-background", scrollbar_background);
        m.insert("scrollbar-background-hover", scrollbar_background);
        m.insert("scrollbar-background-active", scrollbar_background);
        m.insert("scrollbar-corner-color", scrollbar_background);
        m
    });

    // Direct hit.
    if let Some(color) = base.get(name).copied() {
        return Some(color);
    }

    // Muted variants (blend towards background).
    if let Some(base_name) = name.strip_suffix("-muted") {
        let color = base.get(base_name).copied()?;
        let background = base.get("background").copied()?;
        return Some(blend(color, background, 0.70));
    }

    // Lighten / darken variants.
    // Textual uses luminosity_spread=0.15 and NUMBER_OF_SHADES=3, so step=0.075.
    if let Some((base_name, kind, n)) = parse_shade(name) {
        let color = base.get(base_name).copied()?;
        let step = 0.15 / 2.0;
        let delta = step * (n as f32);
        return Some(match kind {
            ShadeKind::Lighten => lighten_lab(color, delta),
            ShadeKind::Darken => darken_lab(color, delta),
        });
    }

    // Derived text colors (Textual uses "auto" + alpha). We approximate by blending
    // between the background and the contrast text color with alpha at render time.
    if matches!(name, "text" | "text-muted" | "text-disabled") {
        let background = base.get("background").copied()?;
        let contrast = contrast_text(background);
        let alpha = match name {
            "text" => 0.87,
            "text-muted" => 0.60,
            "text-disabled" => 0.38,
            _ => 0.87,
        };
        return Some(contrast.with_alpha(alpha));
    }

    None
}

#[derive(Debug, Clone, Copy)]
enum ShadeKind {
    Lighten,
    Darken,
}

fn parse_shade(name: &str) -> Option<(&str, ShadeKind, u8)> {
    // Accept: `<base>-lighten-<n>` or `<base>-darken-<n>`.
    let (base, suffix) = name.rsplit_once('-')?;
    let n: u8 = suffix.parse().ok()?;
    let (base, kind_suffix) = base.rsplit_once('-')?;
    let kind = match kind_suffix {
        "lighten" => ShadeKind::Lighten,
        "darken" => ShadeKind::Darken,
        _ => return None,
    };
    Some((base, kind, n))
}

fn to_rgb(color: Color) -> (u8, u8, u8) {
    (color.r, color.g, color.b)
}

fn from_rgb(r: u8, g: u8, b: u8) -> Color {
    Color::rgb(r, g, b)
}

fn blend(a: Color, b: Color, t: f32) -> Color {
    let (ar, ag, ab) = to_rgb(a);
    let (br, bg, bb) = to_rgb(b);
    let (aa, ba) = (a.a, b.a);
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| -> u8 {
        let xf = x as f32;
        let yf = y as f32;
        (xf + (yf - xf) * t).round().clamp(0.0, 255.0) as u8
    };
    Color::rgba(mix(ar, br), mix(ag, bg), mix(ab, bb), mix(aa, ba))
}

fn lighten_lab(color: Color, amount: f32) -> Color {
    darken_lab(color, -amount)
}

fn darken_lab(color: Color, amount: f32) -> Color {
    let alpha = color.a;
    let (l, a, b) = rgb_to_lab(color);
    let mut l = l - amount * 100.0;
    if l < 0.0 {
        l = 0.0;
    } else if l > 100.0 {
        l = 100.0;
    }
    let mut out = lab_to_rgb(l, a, b);
    out.a = alpha;
    out
}

pub(crate) fn contrast_text(color: Color) -> Color {
    let (r, g, b) = to_rgb(color);
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;
    let brightness = (299.0 * r + 587.0 * g + 114.0 * b) / 1000.0;
    if brightness < 0.5 {
        from_rgb(255, 255, 255)
    } else {
        from_rgb(0, 0, 0)
    }
}

fn rgb_to_lab(color: Color) -> (f32, f32, f32) {
    let (r, g, b) = to_rgb(color);
    let r = srgb_to_linear(r as f32 / 255.0);
    let g = srgb_to_linear(g as f32 / 255.0);
    let b = srgb_to_linear(b as f32 / 255.0);

    let x = r * 0.4124 + g * 0.3576 + b * 0.1805;
    let y = r * 0.2126 + g * 0.7152 + b * 0.0722;
    let z = r * 0.0193 + g * 0.1192 + b * 0.9505;

    let (xr, yr, zr) = (x / 0.95047, y / 1.0, z / 1.08883);
    let fx = lab_f(xr);
    let fy = lab_f(yr);
    let fz = lab_f(zr);

    let l = 116.0 * fy - 16.0;
    let a = 500.0 * (fx - fy);
    let b = 200.0 * (fy - fz);
    (l, a, b)
}

fn lab_to_rgb(l: f32, a: f32, b: f32) -> Color {
    let fy = (l + 16.0) / 116.0;
    let fx = fy + a / 500.0;
    let fz = fy - b / 200.0;

    let xr = lab_f_inv(fx);
    let yr = lab_f_inv(fy);
    let zr = lab_f_inv(fz);

    let x = xr * 0.95047;
    let y = yr * 1.0;
    let z = zr * 1.08883;

    let r = x * 3.2406 + y * -1.5372 + z * -0.4986;
    let g = x * -0.9689 + y * 1.8758 + z * 0.0415;
    let b = x * 0.0557 + y * -0.2040 + z * 1.0570;

    let r = linear_to_srgb(r);
    let g = linear_to_srgb(g);
    let b = linear_to_srgb(b);

    let clamp = |v: f32| -> u8 { (v * 255.0).round().clamp(0.0, 255.0) as u8 };
    from_rgb(clamp(r), clamp(g), clamp(b))
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn lab_f(t: f32) -> f32 {
    let delta: f32 = 6.0 / 29.0;
    if t > delta.powi(3) {
        t.powf(1.0 / 3.0)
    } else {
        t / (3.0 * delta.powi(2)) + 4.0 / 29.0
    }
}

fn lab_f_inv(t: f32) -> f32 {
    let delta: f32 = 6.0 / 29.0;
    if t > delta {
        t.powi(3)
    } else {
        3.0 * delta.powi(2) * (t - 4.0 / 29.0)
    }
}

pub(crate) fn blend_colors(a: Color, b: Color, percent: u8) -> Color {
    blend(a, b, (percent as f32 / 100.0).clamp(0.0, 1.0))
}

// ---------------------------------------------------------------------------
// P2 CSS types: Scalar, Spacing, layout/alignment/pointer enums
// ---------------------------------------------------------------------------

/// CSS size value with unit support.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scalar {
    /// Size determined by content.
    Auto,
    /// Fixed cell count.
    Cells(u16),
    /// Percentage of parent size.
    Percent(f32),
    /// Fractional unit (like CSS `fr`).
    Fraction(f32),
    /// Percentage of viewport width.
    ViewWidth(f32),
    /// Percentage of viewport height.
    ViewHeight(f32),
}

/// Resolve a [`Scalar`] to a concrete cell count.
pub fn resolve_scalar(
    scalar: &Scalar,
    parent_size: u16,
    viewport_size: u16,
    siblings_fr_total: f32,
    available: u16,
) -> u16 {
    match scalar {
        Scalar::Auto => 0,
        Scalar::Cells(n) => *n,
        Scalar::Percent(p) => (parent_size as f32 * p / 100.0).round() as u16,
        Scalar::Fraction(f) => {
            if siblings_fr_total > 0.0 {
                (available as f32 * f / siblings_fr_total).round() as u16
            } else {
                0
            }
        }
        Scalar::ViewWidth(p) => (viewport_size as f32 * p / 100.0).round() as u16,
        Scalar::ViewHeight(p) => (viewport_size as f32 * p / 100.0).round() as u16,
    }
}

/// 4-side spacing (used for both padding and margin).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Spacing {
    pub top: u16,
    pub right: u16,
    pub bottom: u16,
    pub left: u16,
}

impl Spacing {
    pub fn all(value: u16) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    pub fn vertical_horizontal(vertical: u16, horizontal: u16) -> Self {
        Self {
            top: vertical,
            bottom: vertical,
            left: horizontal,
            right: horizontal,
        }
    }

    pub fn new(top: u16, right: u16, bottom: u16, left: u16) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }
}

/// Backward-compatible alias — existing code that uses `Margin` keeps working.
pub type Margin = Spacing;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    Horizontal,
    Vertical,
    Grid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Display {
    Block,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Visible,
    Hidden,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overflow {
    Auto,
    Hidden,
    Scroll,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dock {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HorizontalAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerticalAlign {
    Top,
    Middle,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContentAlign {
    pub horizontal: HorizontalAlign,
    pub vertical: VerticalAlign,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Align {
    pub horizontal: HorizontalAlign,
    pub vertical: VerticalAlign,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Offset {
    pub x: i16,
    pub y: i16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pointer {
    Default,
    Pointer,
    Text,
    NotAllowed,
}

// ---------------------------------------------------------------------------
// Border types (unchanged)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderType {
    Solid,
    Block,
    Tall,
    Outer,
    HKey,
    VKey,
}

impl BorderType {
    pub fn as_edge_type(self) -> &'static str {
        match self {
            BorderType::Solid => "solid",
            BorderType::Block => "block",
            BorderType::Tall => "tall",
            BorderType::Outer => "outer",
            BorderType::HKey => "hkey",
            BorderType::VKey => "vkey",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderEdge {
    /// Not specified by any rule / inline style.
    Unset,
    /// Explicitly clear the edge.
    None,
    /// Render a 1-cell edge using a border type and a color (as foreground), like Textual.
    Edge {
        border_type: BorderType,
        color: Color,
    },
}

impl Default for BorderEdge {
    fn default() -> Self {
        BorderEdge::Unset
    }
}

impl BorderEdge {
    pub fn is_set(&self) -> bool {
        matches!(self, BorderEdge::Edge { .. })
    }

    pub fn edge_type(&self) -> &'static str {
        match self {
            BorderEdge::Edge { border_type, .. } => border_type.as_edge_type(),
            BorderEdge::None | BorderEdge::Unset => "",
        }
    }

    pub fn color(&self) -> Option<Color> {
        match self {
            BorderEdge::Edge { color, .. } => Some(*color),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Style {
    // --- Text / color properties ---
    pub fg: Option<Color>,
    pub fg_auto: Option<AutoColor>,
    pub bg: Option<Color>,
    pub text_opacity: Option<u8>,
    pub opacity: Option<u8>,
    pub bold: Option<bool>,
    pub dim: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub reverse: Option<bool>,

    // --- Border ---
    pub border: Option<bool>,
    pub border_top: BorderEdge,
    pub border_right: BorderEdge,
    pub border_bottom: BorderEdge,
    pub border_left: BorderEdge,

    // --- Tint ---
    pub tint: Option<Tint>,
    pub background_tint: Option<Tint>,

    // --- Spacing ---
    pub margin: Option<Spacing>,
    pub padding: Option<Spacing>,

    // --- Size (Scalar-based) ---
    pub width: Option<Scalar>,
    pub height: Option<Scalar>,
    pub min_width: Option<Scalar>,
    pub max_width: Option<Scalar>,
    pub min_height: Option<Scalar>,
    pub max_height: Option<Scalar>,

    // --- Layout ---
    pub layout: Option<Layout>,
    pub display: Option<Display>,
    pub visibility: Option<Visibility>,
    pub overflow: Option<Overflow>,
    pub dock: Option<Dock>,

    // --- Alignment ---
    pub text_align: Option<TextAlign>,
    pub content_align: Option<ContentAlign>,
    pub align: Option<Align>,
    pub offset: Option<Offset>,

    // --- Pointer ---
    pub pointer: Option<Pointer>,

    // --- Grid ---
    pub grid_size_columns: Option<u16>,
    pub grid_size_rows: Option<u16>,
    pub grid_columns: Option<Vec<Scalar>>,
    pub grid_rows: Option<Vec<Scalar>>,
    pub grid_gutter_horizontal: Option<u16>,
    pub grid_gutter_vertical: Option<u16>,

    // --- Layer ---
    pub layer: Option<String>,
    pub layers: Option<Vec<String>>,

    // --- Transitions ---
    pub transition_duration: Option<Duration>,
    pub transition_delay: Option<Duration>,
    pub transition_timing: Option<TransitionTiming>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tint {
    pub color: Color,
    pub percent: u8,
}

impl Tint {
    pub fn new(color: Color, percent: u8) -> Self {
        Self {
            color,
            percent: percent.min(100),
        }
    }

    pub fn amount(self) -> f32 {
        (self.percent as f32) / 100.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionTiming {
    Linear,
    InOutCubic,
    OutCubic,
    Round,
    None,
}

impl Style {
    pub fn new() -> Self {
        Self::default()
    }

    // --- Text / color builders ---

    pub fn fg(mut self, color: Color) -> Self {
        self.fg = Some(color);
        self.fg_auto = None;
        self
    }

    pub fn fg_auto(mut self, auto: AutoColor) -> Self {
        self.fg_auto = Some(auto);
        self.fg = None;
        self
    }

    pub fn bg(mut self, color: Color) -> Self {
        self.bg = Some(color);
        self
    }

    pub fn text_opacity(mut self, percent: u8) -> Self {
        self.text_opacity = Some(percent.min(100));
        self
    }

    pub fn opacity(mut self, percent: u8) -> Self {
        self.opacity = Some(percent.min(100));
        self
    }

    pub fn bold(mut self, value: bool) -> Self {
        self.bold = Some(value);
        self
    }

    pub fn dim(mut self, value: bool) -> Self {
        self.dim = Some(value);
        self
    }

    pub fn italic(mut self, value: bool) -> Self {
        self.italic = Some(value);
        self
    }

    pub fn underline(mut self, value: bool) -> Self {
        self.underline = Some(value);
        self
    }

    pub fn reverse(mut self, value: bool) -> Self {
        self.reverse = Some(value);
        self
    }

    // --- Border builders ---

    pub fn border(mut self, value: bool) -> Self {
        self.border = Some(value);
        self
    }

    pub fn border_top(mut self, color: Color) -> Self {
        self.border_top = BorderEdge::Edge {
            border_type: BorderType::Solid,
            color,
        };
        self
    }

    pub fn border_right(mut self, color: Color) -> Self {
        self.border_right = BorderEdge::Edge {
            border_type: BorderType::Solid,
            color,
        };
        self
    }

    pub fn border_bottom(mut self, color: Color) -> Self {
        self.border_bottom = BorderEdge::Edge {
            border_type: BorderType::Solid,
            color,
        };
        self
    }

    pub fn border_left(mut self, color: Color) -> Self {
        self.border_left = BorderEdge::Edge {
            border_type: BorderType::Solid,
            color,
        };
        self
    }

    // --- Spacing builders ---

    pub fn margin(mut self, margin: Spacing) -> Self {
        self.margin = Some(margin);
        self
    }

    pub fn padding(mut self, padding: Spacing) -> Self {
        self.padding = Some(padding);
        self
    }

    /// Backward-compatible builder: `line_pad` was horizontal padding applied
    /// to each content line. Maps to `padding.left` + `padding.right`.
    pub fn line_pad(mut self, value: usize) -> Self {
        let v = value as u16;
        let current = self.padding.unwrap_or_default();
        self.padding = Some(Spacing::new(current.top, v, current.bottom, v));
        self
    }

    // --- Size builders (Scalar-based) ---

    pub fn width(mut self, value: Scalar) -> Self {
        self.width = Some(value);
        self
    }

    pub fn height(mut self, value: Scalar) -> Self {
        self.height = Some(value);
        self
    }

    pub fn min_width(mut self, value: Scalar) -> Self {
        self.min_width = Some(value);
        self
    }

    pub fn max_width(mut self, value: Scalar) -> Self {
        self.max_width = Some(value);
        self
    }

    pub fn min_height(mut self, value: Scalar) -> Self {
        self.min_height = Some(value);
        self
    }

    pub fn max_height(mut self, value: Scalar) -> Self {
        self.max_height = Some(value);
        self
    }

    // --- Transition builders ---

    pub fn transition_duration(mut self, value: Duration) -> Self {
        self.transition_duration = Some(value);
        self
    }

    pub fn transition_delay(mut self, value: Duration) -> Self {
        self.transition_delay = Some(value);
        self
    }

    pub fn transition_timing(mut self, value: TransitionTiming) -> Self {
        self.transition_timing = Some(value);
        self
    }

    // --- Cascade: `other` overrides `self` for any field that is `Some` ---

    pub fn combine(&self, other: &Style) -> Style {
        let (fg, fg_auto) = if let Some(color) = other.fg {
            (Some(color), None)
        } else if let Some(auto) = other.fg_auto {
            (None, Some(auto))
        } else {
            (self.fg, self.fg_auto)
        };

        Style {
            fg,
            fg_auto,
            bg: other.bg.or(self.bg),
            text_opacity: other.text_opacity.or(self.text_opacity),
            opacity: other.opacity.or(self.opacity),
            bold: other.bold.or(self.bold),
            dim: other.dim.or(self.dim),
            italic: other.italic.or(self.italic),
            underline: other.underline.or(self.underline),
            reverse: other.reverse.or(self.reverse),
            border: other.border.or(self.border),
            border_top: if other.border_top != BorderEdge::Unset {
                other.border_top
            } else {
                self.border_top
            },
            border_right: if other.border_right != BorderEdge::Unset {
                other.border_right
            } else {
                self.border_right
            },
            border_bottom: if other.border_bottom != BorderEdge::Unset {
                other.border_bottom
            } else {
                self.border_bottom
            },
            border_left: if other.border_left != BorderEdge::Unset {
                other.border_left
            } else {
                self.border_left
            },
            tint: other.tint.or(self.tint),
            background_tint: other.background_tint.or(self.background_tint),
            margin: other.margin.or(self.margin),
            padding: other.padding.or(self.padding),
            width: other.width.or(self.width),
            height: other.height.or(self.height),
            min_width: other.min_width.or(self.min_width),
            max_width: other.max_width.or(self.max_width),
            min_height: other.min_height.or(self.min_height),
            max_height: other.max_height.or(self.max_height),
            layout: other.layout.or(self.layout),
            display: other.display.or(self.display),
            visibility: other.visibility.or(self.visibility),
            overflow: other.overflow.or(self.overflow),
            dock: other.dock.or(self.dock),
            text_align: other.text_align.or(self.text_align),
            content_align: other.content_align.or(self.content_align),
            align: other.align.or(self.align),
            offset: other.offset.or(self.offset),
            pointer: other.pointer.or(self.pointer),
            grid_size_columns: other.grid_size_columns.or(self.grid_size_columns),
            grid_size_rows: other.grid_size_rows.or(self.grid_size_rows),
            grid_columns: other.grid_columns.clone().or_else(|| self.grid_columns.clone()),
            grid_rows: other.grid_rows.clone().or_else(|| self.grid_rows.clone()),
            grid_gutter_horizontal: other.grid_gutter_horizontal.or(self.grid_gutter_horizontal),
            grid_gutter_vertical: other.grid_gutter_vertical.or(self.grid_gutter_vertical),
            layer: other.layer.clone().or_else(|| self.layer.clone()),
            layers: other.layers.clone().or_else(|| self.layers.clone()),
            transition_duration: other.transition_duration.or(self.transition_duration),
            transition_delay: other.transition_delay.or(self.transition_delay),
            transition_timing: other.transition_timing.or(self.transition_timing),
        }
    }

    // --- Inheritance: inheritable properties fall through from parent ---

    pub fn inherit_from(&self, parent: &Style) -> Style {
        let (fg, fg_auto) = if let Some(color) = self.fg {
            (Some(color), None)
        } else if let Some(auto) = self.fg_auto {
            (None, Some(auto))
        } else if let Some(color) = parent.fg {
            (Some(color), None)
        } else if let Some(auto) = parent.fg_auto {
            (None, Some(auto))
        } else {
            (None, None)
        };

        Style {
            fg,
            fg_auto,
            // bg is NOT inherited (CSS semantics).
            bg: self.bg,
            text_opacity: self.text_opacity.or(parent.text_opacity),
            opacity: self.opacity.or(parent.opacity),
            bold: self.bold.or(parent.bold),
            dim: self.dim.or(parent.dim),
            italic: self.italic.or(parent.italic),
            underline: self.underline.or(parent.underline),
            reverse: self.reverse.or(parent.reverse),
            // border edges are NOT inherited.
            border: self.border.or(parent.border),
            border_top: self.border_top,
            border_right: self.border_right,
            border_bottom: self.border_bottom,
            border_left: self.border_left,
            tint: self.tint,
            background_tint: self.background_tint,
            // margin, padding are NOT inherited.
            margin: self.margin,
            padding: self.padding,
            // size fields are NOT inherited.
            width: self.width,
            height: self.height,
            min_width: self.min_width,
            max_width: self.max_width,
            min_height: self.min_height,
            max_height: self.max_height,
            // layout/display/dock/overflow/visibility are NOT inherited.
            layout: self.layout,
            display: self.display,
            visibility: self.visibility,
            overflow: self.overflow,
            dock: self.dock,
            // text_align IS inherited (CSS semantics).
            text_align: self.text_align.or(parent.text_align),
            // content_align, align, offset are NOT inherited.
            content_align: self.content_align,
            align: self.align,
            offset: self.offset,
            pointer: self.pointer,
            // grid fields are NOT inherited (layout properties).
            grid_size_columns: self.grid_size_columns,
            grid_size_rows: self.grid_size_rows,
            grid_columns: self.grid_columns.clone(),
            grid_rows: self.grid_rows.clone(),
            grid_gutter_horizontal: self.grid_gutter_horizontal,
            grid_gutter_vertical: self.grid_gutter_vertical,
            layer: self.layer.clone(),
            // `layers` IS inherited: nested containers see ancestor layer ordering.
            layers: self.layers.clone().or_else(|| parent.layers.clone()),
            transition_duration: self.transition_duration,
            transition_delay: self.transition_delay,
            transition_timing: self.transition_timing,
        }
    }

    // --- Conversion to rich-rs rendering style ---

    /// Returns `true` if any text-rendering attribute (fg, bg, bold, etc.) is set.
    /// Used by `to_rich()` to avoid returning an empty `rich_rs::Style` when only
    /// layout/size/pointer fields are present.
    fn has_rich_text_attrs(&self) -> bool {
        self.fg.is_some()
            || self.fg_auto.is_some()
            || self.bg.is_some()
            || self.bold.is_some()
            || self.dim.is_some()
            || self.italic.is_some()
            || self.underline.is_some()
            || self.reverse.is_some()
    }

    pub fn to_rich(&self) -> Option<rich_rs::Style> {
        if !self.has_rich_text_attrs() {
            return None;
        }
        let mut style = rich_rs::Style::new();
        if let Some(fg) = self.fg {
            style = style.with_color(fg.to_simple_opaque());
        }
        if let Some(bg) = self.bg {
            style = style.with_bgcolor(bg.to_simple_opaque());
        }
        if let Some(bold) = self.bold {
            style = style.with_bold(bold);
        }
        if let Some(dim) = self.dim {
            style = style.with_dim(dim);
        }
        if let Some(italic) = self.italic {
            style = style.with_italic(italic);
        }
        if let Some(underline) = self.underline {
            style = style.with_underline(underline);
        }
        if let Some(reverse) = self.reverse {
            style.reverse = Some(reverse);
        }
        Some(style)
    }

    pub fn to_rich_without_colors(&self) -> Option<rich_rs::Style> {
        if self.bold.is_none()
            && self.dim.is_none()
            && self.italic.is_none()
            && self.underline.is_none()
            && self.reverse.is_none()
        {
            return None;
        }
        let mut style = rich_rs::Style::new();
        if let Some(bold) = self.bold {
            style = style.with_bold(bold);
        }
        if let Some(dim) = self.dim {
            style = style.with_dim(dim);
        }
        if let Some(italic) = self.italic {
            style = style.with_italic(italic);
        }
        if let Some(underline) = self.underline {
            style = style.with_underline(underline);
        }
        if let Some(reverse) = self.reverse {
            style.reverse = Some(reverse);
        }
        Some(style)
    }

    pub fn is_empty(&self) -> bool {
        self.fg.is_none()
            && self.fg_auto.is_none()
            && self.bg.is_none()
            && self.text_opacity.is_none()
            && self.opacity.is_none()
            && self.bold.is_none()
            && self.dim.is_none()
            && self.italic.is_none()
            && self.underline.is_none()
            && self.reverse.is_none()
            && self.border.is_none()
            && self.border_top == BorderEdge::Unset
            && self.border_right == BorderEdge::Unset
            && self.border_bottom == BorderEdge::Unset
            && self.border_left == BorderEdge::Unset
            && self.tint.is_none()
            && self.background_tint.is_none()
            && self.margin.is_none()
            && self.padding.is_none()
            && self.width.is_none()
            && self.height.is_none()
            && self.min_width.is_none()
            && self.max_width.is_none()
            && self.min_height.is_none()
            && self.max_height.is_none()
            && self.layout.is_none()
            && self.display.is_none()
            && self.visibility.is_none()
            && self.overflow.is_none()
            && self.dock.is_none()
            && self.text_align.is_none()
            && self.content_align.is_none()
            && self.align.is_none()
            && self.offset.is_none()
            && self.pointer.is_none()
            && self.grid_size_columns.is_none()
            && self.grid_size_rows.is_none()
            && self.grid_columns.is_none()
            && self.grid_rows.is_none()
            && self.grid_gutter_horizontal.is_none()
            && self.grid_gutter_vertical.is_none()
            && self.layer.is_none()
            && self.layers.is_none()
            && self.transition_duration.is_none()
            && self.transition_delay.is_none()
            && self.transition_timing.is_none()
    }
}


#[derive(Debug, Clone)]
pub struct Theme {
    pub base: Style,
}

impl Theme {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn base(mut self, style: Style) -> Self {
        self.base = style;
        self
    }
}

impl Default for Theme {
    fn default() -> Self {
        let mut base = Style::new();
        if let Some(bg) = parse_color_like("$background") {
            base = base.bg(bg);
        }
        if let Some(fg) = parse_color_like("$foreground") {
            base = base.fg(fg);
        }
        Self { base }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Existing foreground combine tests (kept) ----

    #[test]
    fn combine_prefers_auto_foreground_over_prior_concrete_foreground() {
        let base = Style::new().fg(Color::rgb(224, 224, 224));
        let variant = Style::new().fg_auto(AutoColor::new(87));
        let combined = base.combine(&variant);
        assert_eq!(combined.fg, None);
        assert_eq!(combined.fg_auto.map(|value| value.alpha_percent), Some(87));
    }

    #[test]
    fn combine_prefers_concrete_foreground_over_prior_auto_foreground() {
        let base = Style::new().fg_auto(AutoColor::new(87));
        let variant = Style::new().fg(Color::rgb(20, 20, 20));
        let combined = base.combine(&variant);
        assert_eq!(combined.fg, Some(Color::rgb(20, 20, 20)));
        assert_eq!(combined.fg_auto, None);
    }

    // ---- Scalar resolve_scalar tests ----

    #[test]
    fn resolve_scalar_auto_returns_zero() {
        assert_eq!(resolve_scalar(&Scalar::Auto, 100, 200, 0.0, 0), 0);
    }

    #[test]
    fn resolve_scalar_cells() {
        assert_eq!(resolve_scalar(&Scalar::Cells(42), 100, 200, 0.0, 0), 42);
    }

    #[test]
    fn resolve_scalar_percent() {
        assert_eq!(resolve_scalar(&Scalar::Percent(50.0), 80, 200, 0.0, 0), 40);
        assert_eq!(resolve_scalar(&Scalar::Percent(100.0), 80, 200, 0.0, 0), 80);
        assert_eq!(resolve_scalar(&Scalar::Percent(33.3), 100, 200, 0.0, 0), 33);
    }

    #[test]
    fn resolve_scalar_fraction() {
        // 1fr out of 3fr total, with 90 available → 30
        assert_eq!(resolve_scalar(&Scalar::Fraction(1.0), 0, 0, 3.0, 90), 30);
        // 2fr out of 3fr total, with 90 available → 60
        assert_eq!(resolve_scalar(&Scalar::Fraction(2.0), 0, 0, 3.0, 90), 60);
        // 0 total fr → 0
        assert_eq!(resolve_scalar(&Scalar::Fraction(1.0), 0, 0, 0.0, 90), 0);
    }

    #[test]
    fn resolve_scalar_view_width() {
        assert_eq!(resolve_scalar(&Scalar::ViewWidth(50.0), 0, 120, 0.0, 0), 60);
    }

    #[test]
    fn resolve_scalar_view_height() {
        assert_eq!(resolve_scalar(&Scalar::ViewHeight(25.0), 0, 200, 0.0, 0), 50);
    }

    // ---- Spacing tests ----

    #[test]
    fn spacing_default_is_zero() {
        let s = Spacing::default();
        assert_eq!(s.top, 0);
        assert_eq!(s.right, 0);
        assert_eq!(s.bottom, 0);
        assert_eq!(s.left, 0);
    }

    #[test]
    fn spacing_all() {
        let s = Spacing::all(5);
        assert_eq!((s.top, s.right, s.bottom, s.left), (5, 5, 5, 5));
    }

    #[test]
    fn spacing_vertical_horizontal() {
        let s = Spacing::vertical_horizontal(2, 4);
        assert_eq!((s.top, s.right, s.bottom, s.left), (2, 4, 2, 4));
    }

    #[test]
    fn spacing_new() {
        let s = Spacing::new(1, 2, 3, 4);
        assert_eq!((s.top, s.right, s.bottom, s.left), (1, 2, 3, 4));
    }

    #[test]
    fn margin_alias_works() {
        let m: Margin = Spacing::all(3);
        assert_eq!(m.top, 3);
    }

    // ---- Style::combine with new fields ----

    #[test]
    fn combine_new_layout_fields() {
        let base = {
            let mut s = Style::new();
            s.layout = Some(Layout::Vertical);
            s.display = Some(Display::Block);
            s.text_align = Some(TextAlign::Left);
            s
        };
        let overlay = {
            let mut s = Style::new();
            s.layout = Some(Layout::Horizontal);
            s.text_align = Some(TextAlign::Center);
            s
        };
        let combined = base.combine(&overlay);
        assert_eq!(combined.layout, Some(Layout::Horizontal));
        assert_eq!(combined.display, Some(Display::Block)); // kept from base
        assert_eq!(combined.text_align, Some(TextAlign::Center)); // overridden
    }

    #[test]
    fn combine_scalar_fields() {
        let base = Style::new().width(Scalar::Cells(40));
        let overlay = Style::new().width(Scalar::Percent(50.0));
        let combined = base.combine(&overlay);
        assert_eq!(combined.width, Some(Scalar::Percent(50.0)));
    }

    #[test]
    fn combine_layer_string() {
        let base = {
            let mut s = Style::new();
            s.layer = Some("base".to_string());
            s
        };
        let overlay = {
            let mut s = Style::new();
            s.layer = Some("overlay".to_string());
            s
        };
        let combined = base.combine(&overlay);
        assert_eq!(combined.layer.as_deref(), Some("overlay"));

        // If overlay has no layer, base is preserved.
        let empty_overlay = Style::new();
        let combined2 = base.combine(&empty_overlay);
        assert_eq!(combined2.layer.as_deref(), Some("base"));
    }

    // ---- Style::inherit_from with text_align inheritance ----

    #[test]
    fn inherit_text_align() {
        let parent = {
            let mut s = Style::new();
            s.text_align = Some(TextAlign::Right);
            s
        };
        let child = Style::new();
        let inherited = child.inherit_from(&parent);
        assert_eq!(inherited.text_align, Some(TextAlign::Right));
    }

    #[test]
    fn inherit_layout_does_not_inherit() {
        let parent = {
            let mut s = Style::new();
            s.layout = Some(Layout::Horizontal);
            s.display = Some(Display::None);
            s.visibility = Some(Visibility::Hidden);
            s.dock = Some(Dock::Top);
            s
        };
        let child = Style::new();
        let inherited = child.inherit_from(&parent);
        assert_eq!(inherited.layout, None);
        assert_eq!(inherited.display, None);
        assert_eq!(inherited.visibility, None);
        assert_eq!(inherited.dock, None);
    }

    // ---- Style::is_empty ----

    #[test]
    fn default_style_is_empty() {
        assert!(Style::new().is_empty());
    }

    #[test]
    fn style_with_new_field_is_not_empty() {
        let mut s = Style::new();
        s.layout = Some(Layout::Grid);
        assert!(!s.is_empty());
    }

    // ---- Scalar edge cases ----

    #[test]
    fn scalar_percent_zero() {
        assert_eq!(resolve_scalar(&Scalar::Percent(0.0), 100, 200, 0.0, 0), 0);
    }

    #[test]
    fn scalar_cells_zero() {
        assert_eq!(resolve_scalar(&Scalar::Cells(0), 100, 200, 0.0, 0), 0);
    }

    // ---- Grid field combine/inherit tests ----

    #[test]
    fn combine_grid_fields_override() {
        let base = {
            let mut s = Style::new();
            s.grid_size_columns = Some(3);
            s.grid_gutter_horizontal = Some(1);
            s
        };
        let overlay = {
            let mut s = Style::new();
            s.grid_size_columns = Some(5);
            s.grid_columns = Some(vec![Scalar::Fraction(1.0), Scalar::Fraction(2.0)]);
            s
        };
        let combined = base.combine(&overlay);
        assert_eq!(combined.grid_size_columns, Some(5)); // overridden
        assert_eq!(combined.grid_gutter_horizontal, Some(1)); // kept from base
        assert_eq!(combined.grid_columns.as_ref().map(|v| v.len()), Some(2)); // from overlay
    }

    #[test]
    fn inherit_grid_fields_do_not_inherit() {
        let parent = {
            let mut s = Style::new();
            s.grid_size_columns = Some(4);
            s.grid_size_rows = Some(2);
            s.grid_columns = Some(vec![Scalar::Fraction(1.0)]);
            s.grid_rows = Some(vec![Scalar::Auto]);
            s.grid_gutter_horizontal = Some(3);
            s.grid_gutter_vertical = Some(1);
            s
        };
        let child = Style::new();
        let inherited = child.inherit_from(&parent);
        assert_eq!(inherited.grid_size_columns, None);
        assert_eq!(inherited.grid_size_rows, None);
        assert_eq!(inherited.grid_columns, None);
        assert_eq!(inherited.grid_rows, None);
        assert_eq!(inherited.grid_gutter_horizontal, None);
        assert_eq!(inherited.grid_gutter_vertical, None);
    }

    #[test]
    fn grid_field_makes_style_not_empty() {
        let mut s = Style::new();
        assert!(s.is_empty());
        s.grid_size_columns = Some(2);
        assert!(!s.is_empty());
    }

    // ---- layers field tests ----

    #[test]
    fn combine_layers_override() {
        let base = {
            let mut s = Style::new();
            s.layers = Some(vec!["a".into(), "b".into()]);
            s
        };
        let overlay = {
            let mut s = Style::new();
            s.layers = Some(vec!["x".into(), "y".into(), "z".into()]);
            s
        };
        let combined = base.combine(&overlay);
        let layers = combined.layers.expect("layers should be Some");
        assert_eq!(layers, vec!["x", "y", "z"]);
    }

    #[test]
    fn combine_layers_preserves_base_when_overlay_is_none() {
        let base = {
            let mut s = Style::new();
            s.layers = Some(vec!["a".into(), "b".into()]);
            s
        };
        let overlay = Style::new();
        let combined = base.combine(&overlay);
        assert_eq!(
            combined.layers.as_ref().map(|v| v.len()),
            Some(2)
        );
    }

    #[test]
    fn inherit_layers_inherits_from_parent() {
        let parent = {
            let mut s = Style::new();
            s.layers = Some(vec!["base".into(), "overlay".into()]);
            s
        };
        let child = Style::new();
        let inherited = child.inherit_from(&parent);
        let layers = inherited.layers.expect("layers should inherit from parent");
        assert_eq!(layers, vec!["base", "overlay"]);
    }

    #[test]
    fn inherit_layers_child_overrides_parent() {
        let parent = {
            let mut s = Style::new();
            s.layers = Some(vec!["base".into(), "overlay".into()]);
            s
        };
        let child = {
            let mut s = Style::new();
            s.layers = Some(vec!["x".into()]);
            s
        };
        let inherited = child.inherit_from(&parent);
        let layers = inherited.layers.expect("child layers should override");
        assert_eq!(layers, vec!["x"]);
    }

    #[test]
    fn layers_field_makes_style_not_empty() {
        let mut s = Style::new();
        assert!(s.is_empty());
        s.layers = Some(vec!["default".into()]);
        assert!(!s.is_empty());
    }
}
