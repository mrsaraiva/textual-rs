use std::time::Duration;

/// An RGBA color. `r`/`g`/`b` are 8-bit channels; `a` (alpha) is a float in
/// `[0.0, 1.0]`, mirroring Python Textual's `Color` where alpha is a float.
///
/// Keeping alpha as a float (rather than a u8) is load-bearing for color parity:
/// `background: red 10%` must blend with factor exactly `0.1`, not the quantized
/// `round(0.1*255)/255 = 0.10196`, which drifts a composited channel by one.
///
/// `Eq`/`Hash` are implemented manually over the alpha bit pattern so `Color`
/// (and the structs that embed it, e.g. `Tint`) keep deriving `Eq`/`Hash`. Alpha
/// values here are always finite (clamped on construction), so bitwise hashing
/// is well-defined.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f32,
}

impl Eq for Color {}

impl std::hash::Hash for Color {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.r.hash(state);
        self.g.hash(state);
        self.b.hash(state);
        self.a.to_bits().hash(state);
    }
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Construct from 8-bit RGBA, where `a` is the legacy 0..=255 alpha. Prefer
    /// `rgba_f` / `with_alpha` for fractional alpha precision.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r,
            g,
            b,
            a: a as f32 / 255.0,
        }
    }

    /// Construct with a fractional alpha in `[0.0, 1.0]` (Python-faithful).
    pub fn rgba_f(r: u8, g: u8, b: u8, a: f32) -> Self {
        Self {
            r,
            g,
            b,
            a: a.clamp(0.0, 1.0),
        }
    }

    /// Alpha as a legacy 0..=255 byte (rounded). Use only at the ANSI/render
    /// boundary; internal blends should consume the float `a` directly.
    pub fn alpha_u8(self) -> u8 {
        (self.a.clamp(0.0, 1.0) * 255.0).round() as u8
    }

    pub fn with_alpha(self, alpha: f32) -> Self {
        Self {
            a: alpha.clamp(0.0, 1.0),
            ..self
        }
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
                return Some(Color::rgba_f(r, g, b, a_f));
            }
        }

        // hsl(h, s%, l%) / hsla(h, s%, l%, a) — Python Textual supports CSS hsl().
        for (prefix, has_alpha) in [("hsla(", true), ("hsl(", false)] {
            if let Some(args) = value.strip_prefix(prefix).and_then(|s| s.strip_suffix(')')) {
                let parts: Vec<&str> = args.split(',').map(|p| p.trim()).collect();
                if parts.len() == if has_alpha { 4 } else { 3 } {
                    let h: f32 = parts[0].parse().ok()?;
                    let s: f32 = parts[1].trim_end_matches('%').trim().parse::<f32>().ok()? / 100.0;
                    let l: f32 = parts[2].trim_end_matches('%').trim().parse::<f32>().ok()? / 100.0;
                    let a = if has_alpha {
                        parts[3].parse::<f32>().ok()?.clamp(0.0, 1.0)
                    } else {
                        1.0
                    };
                    let (r, g, b) = hsl_to_rgb(h, s, l);
                    return Some(Color::rgba_f(r, g, b, a));
                }
            }
        }

        // CSS / W3C named colors take precedence over rich-rs's ANSI-palette
        // names, so `white` = #ffffff (CSS) rather than ANSI standard white.
        // Mirrors Python Textual's COLOR_NAME_TO_RGB. (`ansi_*` names and
        // `transparent` are handled in `parse_color_like`.)
        if let Some(color) = parse_css_named_color(value) {
            return Some(color);
        }

        // Try rich-rs parsing (xterm/ANSI named colors, #RRGGBB, etc.).
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

    /// Composite an opaque `over` color onto `under` using a fractional alpha,
    /// faithful to Python `under.blend(over, factor)` == `under + over.with_alpha(factor)`:
    /// `int(u + (o - u) * factor)` per channel, computed in float and TRUNCATED.
    /// Use this instead of `with_alpha(f).flatten_over()` when the alpha is known
    /// as a float — it avoids the u8 alpha quantization that drifts the result by
    /// one (e.g. `auto`/contrast text at 87%).
    pub fn blend_over_float(self, under: Color, factor: f32) -> Color {
        let factor = factor.clamp(0.0, 1.0);
        let mix = |o: u8, u: u8| -> u8 {
            (u as f32 + (o as f32 - u as f32) * factor).clamp(0.0, 255.0) as u8
        };
        Color::rgb(mix(self.r, under.r), mix(self.g, under.g), mix(self.b, under.b))
    }

    /// Python `Color.inverse` — `Color(255 - r, 255 - g, 255 - b, a)`.
    pub fn inverse(self) -> Color {
        Color::rgba_f(255 - self.r, 255 - self.g, 255 - self.b, self.a)
    }

    /// Python `Color.clamped` — channels are already `u8` (in-range); only alpha
    /// can drift, so clamp it to `[0,1]`.
    pub fn clamped(self) -> Color {
        Color::rgba_f(self.r, self.g, self.b, self.a.clamp(0.0, 1.0))
    }

    pub fn flatten_over(self, under: Color) -> Color {
        if self.a >= 1.0 {
            return Color::rgb(self.r, self.g, self.b);
        }
        if self.a <= 0.0 {
            return Color::rgb(under.r, under.g, under.b);
        }
        // Python `under + self` == `under.blend(self, factor=self.a)`:
        // `int(u + (o - u) * a)` per channel, computed in float and TRUNCATED.
        // Using the fractional alpha directly (not a u8 round-trip) is what makes
        // `red 10%` composite to Python's exact hex.
        self.blend_over_float(under, self.a)
    }
}

/// CSS `hsl()` → RGB (Python Textual / CSS Color Module). `h` in degrees,
/// `s`/`l` in [0,1].
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let h = h.rem_euclid(360.0) / 360.0;
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    if s == 0.0 {
        let v = (l * 255.0).round() as u8;
        return (v, v, v);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let hue = |mut t: f32| -> f32 {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 1.0 / 2.0 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    (
        (hue(h + 1.0 / 3.0) * 255.0).round() as u8,
        (hue(h) * 255.0).round() as u8,
        (hue(h - 1.0 / 3.0) * 255.0).round() as u8,
    )
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
    let value = value.trim();
    if value.eq_ignore_ascii_case("transparent") {
        return Some(Color::rgba(0, 0, 0, 0));
    }
    if let Some(color) = parse_textual_ansi_color_name(value) {
        return Some(color);
    }

    // Fast path: try rich-rs simple color parsing.
    if let Some(color) = Color::parse(value) {
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

fn parse_textual_ansi_color_name(value: &str) -> Option<Color> {
    match value.to_ascii_lowercase().as_str() {
        // Textual uses ansi_default to mean terminal default; in composition terms this is transparent.
        "ansi_default" => Some(Color::rgba(0, 0, 0, 0)),
        "ansi_black" => Some(Color::rgb(0x00, 0x00, 0x00)),
        "ansi_red" => Some(Color::rgb(0x80, 0x00, 0x00)),
        "ansi_green" => Some(Color::rgb(0x00, 0x80, 0x00)),
        "ansi_yellow" => Some(Color::rgb(0x80, 0x80, 0x00)),
        "ansi_blue" => Some(Color::rgb(0x00, 0x00, 0x80)),
        "ansi_magenta" => Some(Color::rgb(0x80, 0x00, 0x80)),
        "ansi_cyan" => Some(Color::rgb(0x00, 0x80, 0x80)),
        "ansi_white" => Some(Color::rgb(0xc0, 0xc0, 0xc0)),
        "ansi_bright_black" => Some(Color::rgb(0x80, 0x80, 0x80)),
        "ansi_bright_red" => Some(Color::rgb(0xff, 0x00, 0x00)),
        "ansi_bright_green" => Some(Color::rgb(0x00, 0xff, 0x00)),
        "ansi_bright_yellow" => Some(Color::rgb(0xff, 0xff, 0x00)),
        "ansi_bright_blue" => Some(Color::rgb(0x00, 0x00, 0xff)),
        "ansi_bright_magenta" => Some(Color::rgb(0xff, 0x00, 0xff)),
        "ansi_bright_cyan" => Some(Color::rgb(0x00, 0xff, 0xff)),
        "ansi_bright_white" => Some(Color::rgb(0xff, 0xff, 0xff)),
        _ => None,
    }
}

/// CSS / W3C named colors (Textual `COLOR_NAME_TO_RGB`, web keywords).
///
/// These take precedence over rich-rs's xterm/ANSI-palette color names so CSS
/// keywords resolve to their W3C values (`white` = #ffffff, `cyan` = #00ffff),
/// matching Python Textual rather than the terminal ANSI palette. `transparent`
/// and the `ansi_*` names are handled separately by the callers.
fn parse_css_named_color(value: &str) -> Option<Color> {
    match value.to_ascii_lowercase().as_str() {
        "black" => Some(Color::rgb(0, 0, 0)),
        "silver" => Some(Color::rgb(192, 192, 192)),
        "gray" => Some(Color::rgb(128, 128, 128)),
        "white" => Some(Color::rgb(255, 255, 255)),
        "maroon" => Some(Color::rgb(128, 0, 0)),
        "red" => Some(Color::rgb(255, 0, 0)),
        "purple" => Some(Color::rgb(128, 0, 128)),
        "fuchsia" => Some(Color::rgb(255, 0, 255)),
        "green" => Some(Color::rgb(0, 128, 0)),
        "lime" => Some(Color::rgb(0, 255, 0)),
        "olive" => Some(Color::rgb(128, 128, 0)),
        "yellow" => Some(Color::rgb(255, 255, 0)),
        "navy" => Some(Color::rgb(0, 0, 128)),
        "blue" => Some(Color::rgb(0, 0, 255)),
        "teal" => Some(Color::rgb(0, 128, 128)),
        "aqua" => Some(Color::rgb(0, 255, 255)),
        "orange" => Some(Color::rgb(255, 165, 0)),
        "aliceblue" => Some(Color::rgb(240, 248, 255)),
        "antiquewhite" => Some(Color::rgb(250, 235, 215)),
        "aquamarine" => Some(Color::rgb(127, 255, 212)),
        "azure" => Some(Color::rgb(240, 255, 255)),
        "beige" => Some(Color::rgb(245, 245, 220)),
        "bisque" => Some(Color::rgb(255, 228, 196)),
        "blanchedalmond" => Some(Color::rgb(255, 235, 205)),
        "blueviolet" => Some(Color::rgb(138, 43, 226)),
        "brown" => Some(Color::rgb(165, 42, 42)),
        "burlywood" => Some(Color::rgb(222, 184, 135)),
        "cadetblue" => Some(Color::rgb(95, 158, 160)),
        "chartreuse" => Some(Color::rgb(127, 255, 0)),
        "chocolate" => Some(Color::rgb(210, 105, 30)),
        "coral" => Some(Color::rgb(255, 127, 80)),
        "cornflowerblue" => Some(Color::rgb(100, 149, 237)),
        "cornsilk" => Some(Color::rgb(255, 248, 220)),
        "crimson" => Some(Color::rgb(220, 20, 60)),
        "cyan" => Some(Color::rgb(0, 255, 255)),
        "darkblue" => Some(Color::rgb(0, 0, 139)),
        "darkcyan" => Some(Color::rgb(0, 139, 139)),
        "darkgoldenrod" => Some(Color::rgb(184, 134, 11)),
        "darkgray" => Some(Color::rgb(169, 169, 169)),
        "darkgreen" => Some(Color::rgb(0, 100, 0)),
        "darkgrey" => Some(Color::rgb(169, 169, 169)),
        "darkkhaki" => Some(Color::rgb(189, 183, 107)),
        "darkmagenta" => Some(Color::rgb(139, 0, 139)),
        "darkolivegreen" => Some(Color::rgb(85, 107, 47)),
        "darkorange" => Some(Color::rgb(255, 140, 0)),
        "darkorchid" => Some(Color::rgb(153, 50, 204)),
        "darkred" => Some(Color::rgb(139, 0, 0)),
        "darksalmon" => Some(Color::rgb(233, 150, 122)),
        "darkseagreen" => Some(Color::rgb(143, 188, 143)),
        "darkslateblue" => Some(Color::rgb(72, 61, 139)),
        "darkslategray" => Some(Color::rgb(47, 79, 79)),
        "darkslategrey" => Some(Color::rgb(47, 79, 79)),
        "darkturquoise" => Some(Color::rgb(0, 206, 209)),
        "darkviolet" => Some(Color::rgb(148, 0, 211)),
        "deeppink" => Some(Color::rgb(255, 20, 147)),
        "deepskyblue" => Some(Color::rgb(0, 191, 255)),
        "dimgray" => Some(Color::rgb(105, 105, 105)),
        "dimgrey" => Some(Color::rgb(105, 105, 105)),
        "dodgerblue" => Some(Color::rgb(30, 144, 255)),
        "firebrick" => Some(Color::rgb(178, 34, 34)),
        "floralwhite" => Some(Color::rgb(255, 250, 240)),
        "forestgreen" => Some(Color::rgb(34, 139, 34)),
        "gainsboro" => Some(Color::rgb(220, 220, 220)),
        "ghostwhite" => Some(Color::rgb(248, 248, 255)),
        "gold" => Some(Color::rgb(255, 215, 0)),
        "goldenrod" => Some(Color::rgb(218, 165, 32)),
        "greenyellow" => Some(Color::rgb(173, 255, 47)),
        "grey" => Some(Color::rgb(128, 128, 128)),
        "honeydew" => Some(Color::rgb(240, 255, 240)),
        "hotpink" => Some(Color::rgb(255, 105, 180)),
        "indianred" => Some(Color::rgb(205, 92, 92)),
        "indigo" => Some(Color::rgb(75, 0, 130)),
        "ivory" => Some(Color::rgb(255, 255, 240)),
        "khaki" => Some(Color::rgb(240, 230, 140)),
        "lavender" => Some(Color::rgb(230, 230, 250)),
        "lavenderblush" => Some(Color::rgb(255, 240, 245)),
        "lawngreen" => Some(Color::rgb(124, 252, 0)),
        "lemonchiffon" => Some(Color::rgb(255, 250, 205)),
        "lightblue" => Some(Color::rgb(173, 216, 230)),
        "lightcoral" => Some(Color::rgb(240, 128, 128)),
        "lightcyan" => Some(Color::rgb(224, 255, 255)),
        "lightgoldenrodyellow" => Some(Color::rgb(250, 250, 210)),
        "lightgray" => Some(Color::rgb(211, 211, 211)),
        "lightgreen" => Some(Color::rgb(144, 238, 144)),
        "lightgrey" => Some(Color::rgb(211, 211, 211)),
        "lightpink" => Some(Color::rgb(255, 182, 193)),
        "lightsalmon" => Some(Color::rgb(255, 160, 122)),
        "lightseagreen" => Some(Color::rgb(32, 178, 170)),
        "lightskyblue" => Some(Color::rgb(135, 206, 250)),
        "lightslategray" => Some(Color::rgb(119, 136, 153)),
        "lightslategrey" => Some(Color::rgb(119, 136, 153)),
        "lightsteelblue" => Some(Color::rgb(176, 196, 222)),
        "lightyellow" => Some(Color::rgb(255, 255, 224)),
        "limegreen" => Some(Color::rgb(50, 205, 50)),
        "linen" => Some(Color::rgb(250, 240, 230)),
        "magenta" => Some(Color::rgb(255, 0, 255)),
        "mediumaquamarine" => Some(Color::rgb(102, 205, 170)),
        "mediumblue" => Some(Color::rgb(0, 0, 205)),
        "mediumorchid" => Some(Color::rgb(186, 85, 211)),
        "mediumpurple" => Some(Color::rgb(147, 112, 219)),
        "mediumseagreen" => Some(Color::rgb(60, 179, 113)),
        "mediumslateblue" => Some(Color::rgb(123, 104, 238)),
        "mediumspringgreen" => Some(Color::rgb(0, 250, 154)),
        "mediumturquoise" => Some(Color::rgb(72, 209, 204)),
        "mediumvioletred" => Some(Color::rgb(199, 21, 133)),
        "midnightblue" => Some(Color::rgb(25, 25, 112)),
        "mintcream" => Some(Color::rgb(245, 255, 250)),
        "mistyrose" => Some(Color::rgb(255, 228, 225)),
        "moccasin" => Some(Color::rgb(255, 228, 181)),
        "navajowhite" => Some(Color::rgb(255, 222, 173)),
        "oldlace" => Some(Color::rgb(253, 245, 230)),
        "olivedrab" => Some(Color::rgb(107, 142, 35)),
        "orangered" => Some(Color::rgb(255, 69, 0)),
        "orchid" => Some(Color::rgb(218, 112, 214)),
        "palegoldenrod" => Some(Color::rgb(238, 232, 170)),
        "palegreen" => Some(Color::rgb(152, 251, 152)),
        "paleturquoise" => Some(Color::rgb(175, 238, 238)),
        "palevioletred" => Some(Color::rgb(219, 112, 147)),
        "papayawhip" => Some(Color::rgb(255, 239, 213)),
        "peachpuff" => Some(Color::rgb(255, 218, 185)),
        "peru" => Some(Color::rgb(205, 133, 63)),
        "pink" => Some(Color::rgb(255, 192, 203)),
        "plum" => Some(Color::rgb(221, 160, 221)),
        "powderblue" => Some(Color::rgb(176, 224, 230)),
        "rosybrown" => Some(Color::rgb(188, 143, 143)),
        "royalblue" => Some(Color::rgb(65, 105, 225)),
        "saddlebrown" => Some(Color::rgb(139, 69, 19)),
        "salmon" => Some(Color::rgb(250, 128, 114)),
        "sandybrown" => Some(Color::rgb(244, 164, 96)),
        "seagreen" => Some(Color::rgb(46, 139, 87)),
        "seashell" => Some(Color::rgb(255, 245, 238)),
        "sienna" => Some(Color::rgb(160, 82, 45)),
        "skyblue" => Some(Color::rgb(135, 206, 235)),
        "slateblue" => Some(Color::rgb(106, 90, 205)),
        "slategray" => Some(Color::rgb(112, 128, 144)),
        "slategrey" => Some(Color::rgb(112, 128, 144)),
        "snow" => Some(Color::rgb(255, 250, 250)),
        "springgreen" => Some(Color::rgb(0, 255, 127)),
        "steelblue" => Some(Color::rgb(70, 130, 180)),
        "tan" => Some(Color::rgb(210, 180, 140)),
        "thistle" => Some(Color::rgb(216, 191, 216)),
        "tomato" => Some(Color::rgb(255, 99, 71)),
        "turquoise" => Some(Color::rgb(64, 224, 208)),
        "violet" => Some(Color::rgb(238, 130, 238)),
        "wheat" => Some(Color::rgb(245, 222, 179)),
        "whitesmoke" => Some(Color::rgb(245, 245, 245)),
        "yellowgreen" => Some(Color::rgb(154, 205, 50)),
        "rebeccapurple" => Some(Color::rgb(102, 51, 153)),
        _ => None,
    }
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
        // `$link-color`/`$link-color-hover` default to `$text` (Python design.py:
        // `colors["link-color"] = get("link-color", colors["text"])`) = `auto 87%`.
        "text" | "button-color-foreground" | "link-color" | "link-color-hover" => {
            Some(AutoColor::new(87))
        }
        "text-muted" => Some(AutoColor::new(60)),
        "text-disabled" => Some(AutoColor::new(38)),
        _ => None,
    }
}

fn resolve_color_token(token: &str) -> Option<Color> {
    let token = token.trim();
    let name = token.strip_prefix('$')?;
    // When a non-default named theme is active, resolve from its generated token
    // map first; only fall through to the hand-tuned textual-dark static for the
    // default theme (which keeps the calibrated goldens intact).
    if let Some(color) = crate::theme::active_token(name) {
        return Some(color);
    }
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
        m.insert("accent", Color::parse("#FFA62B").unwrap());
        m.insert("warning", Color::parse("#FFA62B").unwrap());
        m.insert("error", Color::parse("#BA3C5B").unwrap());
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
        // Python's `_generate` derives tokens like `footer-key-foreground` from
        // `accent.hex` — the RAW source color (`#FFA62B`), not the round-tripped
        // `$accent` design token (`#FEA62B`). So derived tokens keep the raw base.
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

        // NOTE: no hardcoded lighten/darken/muted shades here. The generic
        // derivation below (LAB lighten/darken with step 0.075, muted =
        // blend-towards-background 0.7) is byte-exact to Python's
        // `ColorSystem.generate()`; `dark_design_tokens_match_python_generate`
        // locks the derived values against the Python reference.

        // Footer and link color tokens used by builtin styles.
        m.insert("footer-foreground", foreground);
        m.insert("footer-background", panel);
        m.insert("footer-key-foreground", accent);
        m.insert("footer-key-background", Color::rgba(0, 0, 0, 0));
        m.insert("footer-description-foreground", foreground);
        m.insert("footer-description-background", Color::rgba(0, 0, 0, 0));
        m.insert("footer-item-background", Color::rgba(0, 0, 0, 0));
        m.insert("link-background", Color::rgba(0, 0, 0, 0)); // Python: "initial" (transparent)
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
        m.insert("markdown-h1-background", Color::rgba(0, 0, 0, 0));
        m.insert("markdown-h2-background", Color::rgba(0, 0, 0, 0));
        m.insert("markdown-h3-background", Color::rgba(0, 0, 0, 0));
        m.insert("markdown-h4-background", Color::rgba(0, 0, 0, 0));
        m.insert("markdown-h5-background", Color::rgba(0, 0, 0, 0));
        m.insert("markdown-h6-background", Color::rgba(0, 0, 0, 0));

        // Scrollbar tokens (mirrors Textual dark design defaults closely enough for parity).
        let scrollbar_background = darken_lab(background, 0.15 / 2.0);
        // Python: `(background-darken-1 + primary.with_alpha(0.4))` — a blend with
        // the float factor 0.4/0.5, TRUNCATED (`int()`). Use blend_over_float so
        // the baked scrollbar token matches Python's hex exactly (round() drifts
        // the channel by one, e.g. #003055 vs #003054).
        let scrollbar = primary.blend_over_float(scrollbar_background, 0.40);
        let scrollbar_hover = primary.blend_over_float(scrollbar_background, 0.50);
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
    //
    // For the shade-bearing base colors (primary/secondary/accent/warning/error/
    // success), Python emits the BARE design token as the `n == 0` shade —
    // `color.lighten(0)` — which is the LAB round-trip of the source color, NOT
    // the source color itself. That round-trip is lossy for some hues (e.g.
    // accent/warning `#FFA62B` -> `#FEA62B`, error `#BA3C5B` -> `#B93C5B`). We
    // therefore store the ORIGINAL source colors here (so the lighten/darken
    // shades derive from the right base) and reproduce Python's bare token by
    // passing the base through `lighten_lab(_, 0.0)`.
    if let Some(color) = base.get(name).copied() {
        if matches!(
            name,
            "primary" | "secondary" | "accent" | "warning" | "error" | "success"
        ) {
            return Some(lighten_lab(color, 0.0));
        }
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
        let step = 0.15_f64 / 2.0;
        let delta = step * (n as f64);
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
        (xf + (yf - xf) * t).clamp(0.0, 255.0) as u8
    };
    let alpha = (aa + (ba - aa) * t).clamp(0.0, 1.0);
    Color::rgba_f(mix(ar, br), mix(ag, bg), mix(ab, bb), alpha)
}

pub(crate) fn lighten_lab(color: Color, amount: f64) -> Color {
    darken_lab(color, -amount)
}

pub(crate) fn darken_lab(color: Color, amount: f64) -> Color {
    // Python `Color.darken`: l -= amount*100 (no pre-conversion L clamp);
    // clamping happens only on the final RGBA via `.clamped`. `amount` is f64 so
    // the luminosity step (e.g. spread/2 = 0.075) is byte-exact with Python.
    let alpha = color.a;
    let (l, a, b) = rgb_to_lab(color);
    let l = l - amount * 100.0;
    let out = lab_to_rgb(l, a, b, alpha);
    out.clamped()
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

/// Convert an RGB color to CIE-L*a*b* via XYZ, byte-exact to Python Textual's
/// `textual.color.rgb_to_lab` (easyrgb form, f64). Cf. http://www.easyrgb.com/en/math.php.
fn rgb_to_lab(color: Color) -> (f64, f64, f64) {
    let (r8, g8, b8) = to_rgb(color);
    let mut r = r8 as f64 / 255.0;
    let mut g = g8 as f64 / 255.0;
    let mut b = b8 as f64 / 255.0;

    r = if r > 0.04045 {
        ((r + 0.055) / 1.055).powf(2.4)
    } else {
        r / 12.92
    };
    g = if g > 0.04045 {
        ((g + 0.055) / 1.055).powf(2.4)
    } else {
        g / 12.92
    };
    b = if b > 0.04045 {
        ((b + 0.055) / 1.055).powf(2.4)
    } else {
        b / 12.92
    };

    let mut x = (r * 41.24 + g * 35.76 + b * 18.05) / 95.047;
    let mut y = (r * 21.26 + g * 71.52 + b * 7.22) / 100.0;
    let mut z = (r * 1.93 + g * 11.92 + b * 95.05) / 108.883;

    let off = 16.0 / 116.0;
    x = if x > 0.008856 {
        x.powf(1.0 / 3.0)
    } else {
        7.787 * x + off
    };
    y = if y > 0.008856 {
        y.powf(1.0 / 3.0)
    } else {
        7.787 * y + off
    };
    z = if z > 0.008856 {
        z.powf(1.0 / 3.0)
    } else {
        7.787 * z + off
    };

    (116.0 * y - 16.0, 500.0 * (x - y), 200.0 * (y - z))
}

/// Convert a CIE-L*a*b* color back to RGB via XYZ, byte-exact to Python Textual's
/// `textual.color.lab_to_rgb` (easyrgb form, f64). Result is NOT clamped here —
/// callers apply `.clamped()`, matching Python's `int(c * 255)` + `.clamped`.
fn lab_to_rgb(l: f64, a: f64, b: f64, alpha: f32) -> Color {
    let mut y = (l + 16.0) / 116.0;
    let mut x = a / 500.0 + y;
    let mut z = y - b / 200.0;

    let off = 16.0 / 116.0;
    y = if y > 0.2068930344 {
        y.powi(3)
    } else {
        (y - off) / 7.787
    };
    x = if x > 0.2068930344 {
        0.95047 * x.powi(3)
    } else {
        0.122059 * (x - off)
    };
    z = if z > 0.2068930344 {
        1.08883 * z.powi(3)
    } else {
        0.139827 * (z - off)
    };

    let mut r = x * 3.2406 + y * -1.5372 + z * -0.4986;
    let mut g = x * -0.9689 + y * 1.8758 + z * 0.0415;
    let mut bb = x * 0.0557 + y * -0.2040 + z * 1.0570;

    r = if r > 0.0031308 {
        1.055 * r.powf(1.0 / 2.4) - 0.055
    } else {
        12.92 * r
    };
    g = if g > 0.0031308 {
        1.055 * g.powf(1.0 / 2.4) - 0.055
    } else {
        12.92 * g
    };
    bb = if bb > 0.0031308 {
        1.055 * bb.powf(1.0 / 2.4) - 0.055
    } else {
        12.92 * bb
    };

    // Python: Color(int(r*255), int(g*255), int(b*255), alpha). `int()` truncates
    // toward zero; Rust's saturating f64->u8 cast matches after truncation for the
    // in-gamut range, and out-of-range values are corrected by the caller's `.clamped()`.
    let to_byte = |v: f64| -> u8 { v.trunc().clamp(0.0, 255.0) as u8 };
    Color::rgba_f(to_byte(r * 255.0), to_byte(g * 255.0), to_byte(bb * 255.0), alpha)
}

pub(crate) fn blend_colors(a: Color, b: Color, percent: u8) -> Color {
    blend(a, b, (percent as f32 / 100.0).clamp(0.0, 1.0))
}

/// Interpolate a single 8-bit channel, Python-faithful (`int(a + (b-a)*t)` —
/// computed in float and TRUNCATED, matching `Color.blend`/`tint`).
pub(crate) fn blend_channels_trunc(a: u8, b: u8, t: f32) -> u8 {
    let t = t.clamp(0.0, 1.0);
    (a as f32 + (b as f32 - a as f32) * t).clamp(0.0, 255.0) as u8
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
    /// Percentage of the parent's WIDTH (CSS `w` unit), regardless of which axis
    /// this scalar is applied to (e.g. `height: 40w` = 40% of the parent width).
    Width(f32),
    /// Percentage of the parent's HEIGHT (CSS `h` unit), regardless of which axis
    /// this scalar is applied to (e.g. `width: 50h` = 50% of the parent height).
    Height(f32),
    /// Percentage of viewport width.
    ViewWidth(f32),
    /// Percentage of viewport height.
    ViewHeight(f32),
}

/// Resolve a [`Scalar`] to a concrete cell count.
///
/// `parent_size` is the parent extent on the AXIS this scalar sizes (width for a
/// `width`/`min-width`/`max-width` property, height for a height property); it is
/// what `Percent` resolves against. `parent_width`/`parent_height` are BOTH
/// parent dimensions, used by the axis-absolute `Width`/`Height` (`w`/`h`) units
/// which always resolve against a specific axis regardless of the property they
/// appear on (Python `css/scalar.py` `_resolve_width`/`_resolve_height`).
/// `viewport_width`/`viewport_height` are BOTH viewport dimensions, used the same
/// way by the axis-absolute `ViewWidth`/`ViewHeight` (`vw`/`vh`) units: `25vh`
/// is ALWAYS 25% of the viewport HEIGHT even on a `width` property, mirroring
/// `_resolve_view_width`/`_resolve_view_height` (which take `viewport.width` /
/// `viewport.height`). Passing a single axis here was the bug that made
/// `width: 25vh` resolve against the viewport WIDTH.
/// Resolve a fixed-cell scalar (`Cells`/`Percent`/`Width`/`Height`/`ViewWidth`/
/// `ViewHeight`) to its EXACT pre-floor cell count as `f64`.
///
/// Python resolves each scalar to an exact `Fraction` (`css/scalar.py`) and only
/// truncates to integer cells at PLACEMENT time, via CUMULATIVE flooring of the
/// running position (`layouts/vertical.py`/`horizontal.py`:
/// `next_y.__floor__() - y.__floor__()`). Independently flooring every child's
/// size loses the fractional remainder and under-sizes the stack (e.g. two
/// adjacent `12.5h` boxes of 3.75 each render 3+3 instead of 3+4). Returning the
/// exact `f64` here lets the flow layouts reproduce Python's fence-post rounding.
///
/// Returns `None` for `Auto`/`Fraction` (those are resolved by the 1D resolver,
/// not by a direct percentage of a known base).
#[allow(clippy::too_many_arguments)]
pub fn resolve_scalar_exact(
    scalar: &Scalar,
    parent_size: u16,
    parent_width: u16,
    parent_height: u16,
    viewport_width: u16,
    viewport_height: u16,
) -> Option<f64> {
    match scalar {
        Scalar::Auto | Scalar::Fraction(_) => None,
        Scalar::Cells(n) => Some(*n as f64),
        Scalar::Percent(p) => Some(parent_size as f64 * *p as f64 / 100.0),
        Scalar::Width(p) => Some(parent_width as f64 * *p as f64 / 100.0),
        Scalar::Height(p) => Some(parent_height as f64 * *p as f64 / 100.0),
        Scalar::ViewWidth(p) => Some(viewport_width as f64 * *p as f64 / 100.0),
        Scalar::ViewHeight(p) => Some(viewport_height as f64 * *p as f64 / 100.0),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn resolve_scalar(
    scalar: &Scalar,
    parent_size: u16,
    parent_width: u16,
    parent_height: u16,
    viewport_width: u16,
    viewport_height: u16,
    siblings_fr_total: f32,
    available: u16,
) -> u16 {
    // Python resolves each scalar to an EXACT `Fraction` (`css/scalar.py`) and
    // only converts to cells by TRUNCATION (`int()` / `Fraction.__floor__()`) at
    // the box-model / placement step (`Widget._get_box_model`,
    // `layouts/*.py`). Mirror that with `.floor()` so a non-integer percent like
    // `min-height: 75%` of 30 (= 22.5) yields 22, not a rounded 23. (`.round()`
    // only matched Python when the percentage landed on a whole number.)
    match scalar {
        Scalar::Auto => 0,
        Scalar::Cells(n) => *n,
        Scalar::Percent(p) => (parent_size as f32 * p / 100.0).floor() as u16,
        Scalar::Fraction(f) => {
            if siblings_fr_total > 0.0 {
                (available as f32 * f / siblings_fr_total).floor() as u16
            } else {
                0
            }
        }
        Scalar::Width(p) => (parent_width as f32 * p / 100.0).floor() as u16,
        Scalar::Height(p) => (parent_height as f32 * p / 100.0).floor() as u16,
        Scalar::ViewWidth(p) => (viewport_width as f32 * p / 100.0).floor() as u16,
        Scalar::ViewHeight(p) => (viewport_height as f32 * p / 100.0).floor() as u16,
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

/// A single offset axis value: either absolute cells or a percentage of the widget's own size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OffsetValue {
    Cells(i16),
    Percent(f32),
}

impl Default for OffsetValue {
    fn default() -> Self {
        OffsetValue::Cells(0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Offset {
    pub x: OffsetValue,
    pub y: OffsetValue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pointer {
    Default,
    Pointer,
    Text,
    NotAllowed,
}

/// Controls how floating/overlay elements are constrained to their container or viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Constrain {
    /// No constraint (default).
    #[default]
    None,
    /// Keep the element fully inside the viewport/container.
    Inside,
    /// Flip to the opposite side if it would overflow.
    Inflect,
}

// ---------------------------------------------------------------------------
// P2 CSS gap types: position, box-sizing, split, text-wrap, etc.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Position {
    Relative,
    Absolute,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoxSizing {
    ContentBox,
    BorderBox,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Split {
    Top,
    Right,
    Bottom,
    Left,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextWrap {
    Wrap,
    NoWrap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextOverflow {
    Clip,
    Fold,
    Ellipsis,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayMode {
    None,
    Screen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeylineType {
    None,
    Thin,
    Heavy,
    Double,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollbarGutter {
    Auto,
    Stable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollbarVisibility {
    Auto,
    Hidden,
    Visible,
}

/// Text style flags for compound text-style properties
/// (border-title-style, link-style, etc.).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextStyleFlags {
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub reverse: bool,
    pub strike: bool,
}

pub(crate) fn resolve_text_style_token_flags(token: &str) -> Option<TextStyleFlags> {
    // Text-style token defaults from Textual design/theme values.
    let mut flags = TextStyleFlags::default();
    match token {
        "bold" => flags.bold = true,
        "dim" => flags.dim = true,
        "italic" => flags.italic = true,
        "underline" => flags.underline = true,
        "reverse" => flags.reverse = true,
        "strike" | "strikethrough" => flags.strike = true,
        "$link-style" => flags.underline = true,
        "$link-style-hover" => flags.bold = true,
        "$button-focus-text-style" => {
            flags.bold = true;
            flags.reverse = true;
        }
        "$block-cursor-text-style" => flags.bold = true,
        "$block-cursor-blurred-text-style" => {}
        "$input-cursor-text-style" => {}
        "$markdown-h1-text-style" => flags.bold = true,
        "$markdown-h2-text-style" => flags.underline = true,
        "$markdown-h3-text-style" => flags.bold = true,
        "$markdown-h4-text-style" => flags.italic = true,
        "$markdown-h5-text-style" => flags.italic = true,
        "$markdown-h6-text-style" => flags.dim = true,
        _ => return None,
    }
    Some(flags)
}

/// Hatch fill pattern: a character repeated as background fill with a color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hatch {
    pub character: char,
    pub color: Color,
}

/// Keyline border drawn between child widgets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Keyline {
    pub keyline_type: KeylineType,
    pub color: Color,
}

/// Per-property transition declaration.
#[derive(Clone, Debug, PartialEq)]
pub struct PropertyTransition {
    pub property: String,
    pub duration: Duration,
    pub timing: TransitionTiming,
    pub delay: Duration,
}

// ---------------------------------------------------------------------------
// Border types (unchanged)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderType {
    // existing variants — keep order, do not reorder
    Solid,
    Heavy,
    Block,
    Tall,
    Outer,
    HKey,
    VKey,
    // new variants (Python VALID_BORDER minus none/hidden, which map to BorderEdge::None)
    Ascii,
    Blank,
    Dashed,
    Double,
    Inner,
    Panel,
    Round,
    Tab,
    Thick,
    Wide,
}

impl BorderType {
    pub fn as_edge_type(self) -> &'static str {
        match self {
            BorderType::Solid => "solid",
            BorderType::Heavy => "heavy",
            BorderType::Block => "block",
            BorderType::Tall => "tall",
            BorderType::Outer => "outer",
            BorderType::HKey => "hkey",
            BorderType::VKey => "vkey",
            BorderType::Ascii => "ascii",
            BorderType::Blank => "blank",
            BorderType::Dashed => "dashed",
            BorderType::Double => "double",
            BorderType::Inner => "inner",
            BorderType::Panel => "panel",
            BorderType::Round => "round",
            BorderType::Tab => "tab",
            BorderType::Thick => "thick",
            BorderType::Wide => "wide",
        }
    }

    /// Parse a border-type keyword (Python `VALID_BORDER`, lowercase).
    /// `none`/`hidden` are NOT border types here — they normalize to no border
    /// (`BorderEdge::None`) and are handled by the parser.
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "solid" => Self::Solid,
            "heavy" => Self::Heavy,
            "block" => Self::Block,
            "tall" => Self::Tall,
            "outer" => Self::Outer,
            "hkey" => Self::HKey,
            "vkey" => Self::VKey,
            "ascii" => Self::Ascii,
            "blank" => Self::Blank,
            "dashed" => Self::Dashed,
            "double" => Self::Double,
            "inner" => Self::Inner,
            "panel" => Self::Panel,
            "round" => Self::Round,
            "tab" => Self::Tab,
            "thick" => Self::Thick,
            "wide" => Self::Wide,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
pub enum BorderEdge {
    /// Not specified by any rule / inline style.
    #[default]
    Unset,
    /// Explicitly clear the edge.
    None,
    /// Render a 1-cell edge using a border type and a color (as foreground), like Textual.
    Edge {
        border_type: BorderType,
        color: Color,
    },
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

// ---------------------------------------------------------------------------
// Importance tracking for !important declarations
// ---------------------------------------------------------------------------

/// Identifies a CSS property on [`Style`] for importance tracking.
///
/// Each variant corresponds to a single property (or, for `Fg`, the
/// `fg`/`fg_auto` pair). The discriminant is used as a bit index in
/// [`ImportanceBitset`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum StyleProperty {
    Fg = 0,
    Bg = 1,
    TextOpacity = 2,
    Opacity = 3,
    Bold = 4,
    Dim = 5,
    Italic = 6,
    Underline = 7,
    Reverse = 8,
    Border = 9,
    BorderTop = 10,
    BorderRight = 11,
    BorderBottom = 12,
    BorderLeft = 13,
    Tint = 14,
    BackgroundTint = 15,
    Margin = 16,
    Padding = 17,
    Width = 18,
    Height = 19,
    MinWidth = 20,
    MaxWidth = 21,
    MinHeight = 22,
    MaxHeight = 23,
    Layout = 24,
    Display = 25,
    Visibility = 26,
    Overflow = 27,
    Dock = 28,
    TextAlign = 29,
    ContentAlign = 30,
    Align = 31,
    Offset = 32,
    Pointer = 33,
    GridSizeColumns = 34,
    GridSizeRows = 35,
    GridColumns = 36,
    GridRows = 37,
    GridGutterHorizontal = 38,
    GridGutterVertical = 39,
    Layer = 40,
    Layers = 41,
    TransitionDuration = 42,
    TransitionDelay = 43,
    TransitionTiming = 44,
    Constrain = 45,
    OverflowX = 46,
    OverflowY = 47,
    // --- P2 CSS gap properties (P2-24..P2-36) ---
    Position = 48,
    BoxSizing = 49,
    Split = 50,
    PaddingTop = 51,
    PaddingRight = 52,
    PaddingBottom = 53,
    PaddingLeft = 54,
    MarginTop = 55,
    MarginRight = 56,
    MarginBottom = 57,
    MarginLeft = 58,
    OutlineTop = 59,
    OutlineRight = 60,
    OutlineBottom = 61,
    OutlineLeft = 62,
    BorderTitleAlign = 63,
    BorderSubtitleAlign = 64,
    BorderTitleColor = 65,
    BorderTitleBackground = 66,
    BorderTitleStyle = 67,
    BorderSubtitleColor = 68,
    BorderSubtitleBackground = 69,
    BorderSubtitleStyle = 70,
    ScrollbarColor = 71,
    ScrollbarColorHover = 72,
    ScrollbarColorActive = 73,
    ScrollbarBackground = 74,
    ScrollbarBackgroundHover = 75,
    ScrollbarBackgroundActive = 76,
    ScrollbarCornerColor = 77,
    ScrollbarGutter = 78,
    ScrollbarSize = 79,
    ScrollbarSizeHorizontal = 80,
    ScrollbarSizeVertical = 81,
    ScrollbarVisibility = 82,
    TextWrapProp = 83,
    TextOverflowProp = 84,
    LinkColor = 85,
    LinkBackground = 86,
    LinkStyleProp = 87,
    LinkColorHover = 88,
    LinkBackgroundHover = 89,
    LinkStyleHover = 90,
    RowSpan = 91,
    ColumnSpan = 92,
    HatchProp = 93,
    OverlayProp = 94,
    KeylineProp = 95,
    ConstrainX = 96,
    ConstrainY = 97,
    ExpandProp = 98,
    TransitionsProp = 99,
    Strike = 100,
    LinePad = 101,
}

/// Bitset tracking which [`Style`] properties carry `!important`.
///
/// Each bit corresponds to a [`StyleProperty`] variant's discriminant.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportanceBitset(u128);

impl std::fmt::Debug for ImportanceBitset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0 == 0 {
            write!(f, "ImportanceBitset(0)")
        } else {
            write!(f, "ImportanceBitset({:#034x})", self.0)
        }
    }
}

impl ImportanceBitset {
    pub fn new() -> Self {
        Self(0)
    }

    pub fn set(&mut self, prop: StyleProperty) {
        self.0 |= 1u128 << (prop as u8);
    }

    pub fn get(&self, prop: StyleProperty) -> bool {
        (self.0 & (1u128 << (prop as u8))) != 0
    }

    pub fn is_empty(&self) -> bool {
        self.0 == 0
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
    pub strike: Option<bool>,

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
    /// Shorthand field: set when `overflow: <value>` is used (sets both axes).
    pub overflow: Option<Overflow>,
    pub overflow_x: Option<Overflow>,
    pub overflow_y: Option<Overflow>,
    pub dock: Option<Dock>,

    // --- Alignment ---
    pub text_align: Option<TextAlign>,
    pub content_align: Option<ContentAlign>,
    pub align: Option<Align>,
    pub offset: Option<Offset>,

    // --- Pointer ---
    pub pointer: Option<Pointer>,

    // --- Constrain ---
    pub constrain: Option<Constrain>,

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

    // --- P2 CSS gap properties (P2-24..P2-36) ---

    // P2-24: position
    pub position: Option<Position>,
    // P2-25: box-sizing
    pub box_sizing: Option<BoxSizing>,
    // P2-26: split
    pub split: Option<Split>,

    // P2-27: per-side spacing overrides (take priority over shorthand padding/margin)
    pub padding_top: Option<u16>,
    pub padding_right: Option<u16>,
    pub padding_bottom: Option<u16>,
    pub padding_left: Option<u16>,
    pub margin_top: Option<u16>,
    pub margin_right: Option<u16>,
    pub margin_bottom: Option<u16>,
    pub margin_left: Option<u16>,

    // P2-28: outline
    pub outline_top: BorderEdge,
    pub outline_right: BorderEdge,
    pub outline_bottom: BorderEdge,
    pub outline_left: BorderEdge,

    // P2-29: border title/subtitle styling
    pub border_title_align: Option<HorizontalAlign>,
    pub border_subtitle_align: Option<HorizontalAlign>,
    pub border_title_color: Option<Color>,
    pub border_title_background: Option<Color>,
    pub border_title_style: Option<TextStyleFlags>,
    pub border_subtitle_color: Option<Color>,
    pub border_subtitle_background: Option<Color>,
    pub border_subtitle_style: Option<TextStyleFlags>,

    // P2-30: scrollbar CSS
    pub scrollbar_color: Option<Color>,
    pub scrollbar_color_hover: Option<Color>,
    pub scrollbar_color_active: Option<Color>,
    pub scrollbar_background: Option<Color>,
    pub scrollbar_background_hover: Option<Color>,
    pub scrollbar_background_active: Option<Color>,
    pub scrollbar_corner_color: Option<Color>,
    pub scrollbar_gutter: Option<ScrollbarGutter>,
    pub scrollbar_size: Option<u16>,
    pub scrollbar_size_horizontal: Option<u16>,
    pub scrollbar_size_vertical: Option<u16>,
    pub scrollbar_visibility: Option<ScrollbarVisibility>,

    // P2-31: text-wrap, text-overflow
    pub text_wrap: Option<TextWrap>,
    pub text_overflow: Option<TextOverflow>,

    // P2-32: link styling
    pub link_color: Option<Color>,
    /// When `link-color` is `auto`/`$text`/`$link-color`, the foreground is a
    /// contrast color computed against the LINK background (not the screen).
    /// Mirrors Python `auto_link_color` + `link_background.get_contrast_text(a)`.
    /// Pairs with `link_color` (like `fg`/`fg_auto`): `link_color` still holds a
    /// concrete fallback contrast for non-link uses, but when this is `Some` the
    /// link renderer recomputes the contrast against the resolved link bg.
    pub link_color_auto: Option<AutoColor>,
    pub link_background: Option<Color>,
    pub link_style: Option<TextStyleFlags>,
    pub link_color_hover: Option<Color>,
    /// Auto-contrast marker for `link-color-hover` (see `link_color_auto`).
    pub link_color_hover_auto: Option<AutoColor>,
    pub link_background_hover: Option<Color>,
    pub link_style_hover: Option<TextStyleFlags>,

    // P2-33: grid child placement
    pub row_span: Option<u16>,
    pub column_span: Option<u16>,

    // P2-34: hatch, overlay, keyline
    pub hatch: Option<Hatch>,
    pub overlay: Option<OverlayMode>,
    pub keyline: Option<Keyline>,

    // P2-35: constrain-x, constrain-y, expand
    pub constrain_x: Option<Constrain>,
    pub constrain_y: Option<Constrain>,
    pub expand: Option<bool>,

    // P2-36: per-property transitions
    pub transitions: Option<Vec<PropertyTransition>>,

    // --- Render-time-only properties (not part of box model) ---
    /// Horizontal padding applied to each content line at render time.
    /// Unlike CSS `padding`, this does NOT affect the box model / layout width.
    pub line_pad: Option<u16>,

    // --- Importance tracking ---
    pub importance: ImportanceBitset,
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

// ---------------------------------------------------------------------------
// Cascade helpers for importance-aware property merging
// ---------------------------------------------------------------------------

/// Cascade two optional values respecting `!important` flags.
///
/// Returns `(resolved_value, is_important)`.
fn cascade_opt<T: Clone>(
    self_val: &Option<T>,
    other_val: &Option<T>,
    self_imp: bool,
    other_imp: bool,
) -> (Option<T>, bool) {
    match (other_val.is_some(), self_val.is_some()) {
        // Both have the value; self is important, other is not → self wins
        (true, true) if self_imp && !other_imp => (self_val.clone(), true),
        // Other has a value and self doesn't block it → other wins
        (true, _) => (other_val.clone(), other_imp),
        // Other has no value → keep self
        _ => (self_val.clone(), self_imp && self_val.is_some()),
    }
}

/// Cascade two [`BorderEdge`] values respecting `!important` flags.
fn cascade_border(
    self_val: BorderEdge,
    other_val: BorderEdge,
    self_imp: bool,
    other_imp: bool,
) -> (BorderEdge, bool) {
    let other_set = other_val != BorderEdge::Unset;
    let self_set = self_val != BorderEdge::Unset;
    if other_set && self_set && self_imp && !other_imp {
        (self_val, true)
    } else if other_set {
        (other_val, other_imp)
    } else {
        (self_val, self_imp && self_set)
    }
}

macro_rules! cascade_field {
    ($self:expr, $other:expr, $imp:ident, $field:ident, $prop:expr) => {{
        let (val, is_imp) = cascade_opt(
            &$self.$field,
            &$other.$field,
            $self.importance.get($prop),
            $other.importance.get($prop),
        );
        if is_imp {
            $imp.set($prop);
        }
        val
    }};
}

macro_rules! cascade_border_field {
    ($self:expr, $other:expr, $imp:ident, $field:ident, $prop:expr) => {{
        let (val, is_imp) = cascade_border(
            $self.$field,
            $other.$field,
            $self.importance.get($prop),
            $other.importance.get($prop),
        );
        if is_imp {
            $imp.set($prop);
        }
        val
    }};
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

    pub fn strike(mut self, value: bool) -> Self {
        self.strike = Some(value);
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

    /// Render-time horizontal padding applied to each content line.
    /// Does NOT affect the box model (not included in `effective_padding()`).
    pub fn line_pad(mut self, value: usize) -> Self {
        self.line_pad = Some(value as u16);
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

    // --- Per-side spacing synthesis (P2-27) ---

    /// Compute effective padding by merging the shorthand `padding` with per-side
    /// overrides (`padding_top`, `padding_right`, etc.). Per-side values take
    /// priority over the corresponding side of the shorthand.
    pub fn effective_padding(&self) -> Spacing {
        let base = self.padding.unwrap_or_default();
        Spacing {
            top: self.padding_top.unwrap_or(base.top),
            right: self.padding_right.unwrap_or(base.right),
            bottom: self.padding_bottom.unwrap_or(base.bottom),
            left: self.padding_left.unwrap_or(base.left),
        }
    }

    /// Compute effective margin by merging the shorthand `margin` with per-side
    /// overrides (`margin_top`, `margin_right`, etc.). Per-side values take
    /// priority over the corresponding side of the shorthand.
    pub fn effective_margin(&self) -> Spacing {
        let base = self.margin.unwrap_or_default();
        Spacing {
            top: self.margin_top.unwrap_or(base.top),
            right: self.margin_right.unwrap_or(base.right),
            bottom: self.margin_bottom.unwrap_or(base.bottom),
            left: self.margin_left.unwrap_or(base.left),
        }
    }

    // --- Cascade: `other` overrides `self` for any field that is `Some`,
    //     unless `self` has `!important` and `other` does not. ---

    pub fn combine(&self, other: &Style) -> Style {
        let mut imp = ImportanceBitset::new();

        // fg / fg_auto are a linked pair representing one "foreground color" property.
        let self_fg_imp = self.importance.get(StyleProperty::Fg);
        let other_fg_imp = other.importance.get(StyleProperty::Fg);
        let other_has_fg = other.fg.is_some() || other.fg_auto.is_some();
        let self_has_fg = self.fg.is_some() || self.fg_auto.is_some();

        let (fg, fg_auto) = if other_has_fg && self_has_fg && self_fg_imp && !other_fg_imp {
            imp.set(StyleProperty::Fg);
            (self.fg, self.fg_auto)
        } else if other_has_fg {
            if other_fg_imp {
                imp.set(StyleProperty::Fg);
            }
            if let Some(color) = other.fg {
                (Some(color), None)
            } else if let Some(auto) = other.fg_auto {
                (None, Some(auto))
            } else {
                (self.fg, self.fg_auto)
            }
        } else {
            if self_has_fg && self_fg_imp {
                imp.set(StyleProperty::Fg);
            }
            (self.fg, self.fg_auto)
        };

        Style {
            fg,
            fg_auto,
            bg: cascade_field!(self, other, imp, bg, StyleProperty::Bg),
            text_opacity: cascade_field!(
                self,
                other,
                imp,
                text_opacity,
                StyleProperty::TextOpacity
            ),
            opacity: cascade_field!(self, other, imp, opacity, StyleProperty::Opacity),
            bold: cascade_field!(self, other, imp, bold, StyleProperty::Bold),
            dim: cascade_field!(self, other, imp, dim, StyleProperty::Dim),
            italic: cascade_field!(self, other, imp, italic, StyleProperty::Italic),
            underline: cascade_field!(self, other, imp, underline, StyleProperty::Underline),
            reverse: cascade_field!(self, other, imp, reverse, StyleProperty::Reverse),
            strike: cascade_field!(self, other, imp, strike, StyleProperty::Strike),
            border: cascade_field!(self, other, imp, border, StyleProperty::Border),
            border_top: cascade_border_field!(
                self,
                other,
                imp,
                border_top,
                StyleProperty::BorderTop
            ),
            border_right: cascade_border_field!(
                self,
                other,
                imp,
                border_right,
                StyleProperty::BorderRight
            ),
            border_bottom: cascade_border_field!(
                self,
                other,
                imp,
                border_bottom,
                StyleProperty::BorderBottom
            ),
            border_left: cascade_border_field!(
                self,
                other,
                imp,
                border_left,
                StyleProperty::BorderLeft
            ),
            tint: cascade_field!(self, other, imp, tint, StyleProperty::Tint),
            background_tint: cascade_field!(
                self,
                other,
                imp,
                background_tint,
                StyleProperty::BackgroundTint
            ),
            margin: cascade_field!(self, other, imp, margin, StyleProperty::Margin),
            padding: cascade_field!(self, other, imp, padding, StyleProperty::Padding),
            width: cascade_field!(self, other, imp, width, StyleProperty::Width),
            height: cascade_field!(self, other, imp, height, StyleProperty::Height),
            min_width: cascade_field!(self, other, imp, min_width, StyleProperty::MinWidth),
            max_width: cascade_field!(self, other, imp, max_width, StyleProperty::MaxWidth),
            min_height: cascade_field!(self, other, imp, min_height, StyleProperty::MinHeight),
            max_height: cascade_field!(self, other, imp, max_height, StyleProperty::MaxHeight),
            layout: cascade_field!(self, other, imp, layout, StyleProperty::Layout),
            display: cascade_field!(self, other, imp, display, StyleProperty::Display),
            visibility: cascade_field!(self, other, imp, visibility, StyleProperty::Visibility),
            overflow: cascade_field!(self, other, imp, overflow, StyleProperty::Overflow),
            overflow_x: cascade_field!(self, other, imp, overflow_x, StyleProperty::OverflowX),
            overflow_y: cascade_field!(self, other, imp, overflow_y, StyleProperty::OverflowY),
            dock: cascade_field!(self, other, imp, dock, StyleProperty::Dock),
            text_align: cascade_field!(self, other, imp, text_align, StyleProperty::TextAlign),
            content_align: cascade_field!(
                self,
                other,
                imp,
                content_align,
                StyleProperty::ContentAlign
            ),
            align: cascade_field!(self, other, imp, align, StyleProperty::Align),
            offset: cascade_field!(self, other, imp, offset, StyleProperty::Offset),
            pointer: cascade_field!(self, other, imp, pointer, StyleProperty::Pointer),
            constrain: cascade_field!(self, other, imp, constrain, StyleProperty::Constrain),
            grid_size_columns: cascade_field!(
                self,
                other,
                imp,
                grid_size_columns,
                StyleProperty::GridSizeColumns
            ),
            grid_size_rows: cascade_field!(
                self,
                other,
                imp,
                grid_size_rows,
                StyleProperty::GridSizeRows
            ),
            grid_columns: cascade_field!(
                self,
                other,
                imp,
                grid_columns,
                StyleProperty::GridColumns
            ),
            grid_rows: cascade_field!(self, other, imp, grid_rows, StyleProperty::GridRows),
            grid_gutter_horizontal: cascade_field!(
                self,
                other,
                imp,
                grid_gutter_horizontal,
                StyleProperty::GridGutterHorizontal
            ),
            grid_gutter_vertical: cascade_field!(
                self,
                other,
                imp,
                grid_gutter_vertical,
                StyleProperty::GridGutterVertical
            ),
            layer: cascade_field!(self, other, imp, layer, StyleProperty::Layer),
            layers: cascade_field!(self, other, imp, layers, StyleProperty::Layers),
            transition_duration: cascade_field!(
                self,
                other,
                imp,
                transition_duration,
                StyleProperty::TransitionDuration
            ),
            transition_delay: cascade_field!(
                self,
                other,
                imp,
                transition_delay,
                StyleProperty::TransitionDelay
            ),
            transition_timing: cascade_field!(
                self,
                other,
                imp,
                transition_timing,
                StyleProperty::TransitionTiming
            ),
            // --- P2 CSS gap cascade ---
            position: cascade_field!(self, other, imp, position, StyleProperty::Position),
            box_sizing: cascade_field!(self, other, imp, box_sizing, StyleProperty::BoxSizing),
            split: cascade_field!(self, other, imp, split, StyleProperty::Split),
            padding_top: cascade_field!(self, other, imp, padding_top, StyleProperty::PaddingTop),
            padding_right: cascade_field!(
                self,
                other,
                imp,
                padding_right,
                StyleProperty::PaddingRight
            ),
            padding_bottom: cascade_field!(
                self,
                other,
                imp,
                padding_bottom,
                StyleProperty::PaddingBottom
            ),
            padding_left: cascade_field!(
                self,
                other,
                imp,
                padding_left,
                StyleProperty::PaddingLeft
            ),
            margin_top: cascade_field!(self, other, imp, margin_top, StyleProperty::MarginTop),
            margin_right: cascade_field!(
                self,
                other,
                imp,
                margin_right,
                StyleProperty::MarginRight
            ),
            margin_bottom: cascade_field!(
                self,
                other,
                imp,
                margin_bottom,
                StyleProperty::MarginBottom
            ),
            margin_left: cascade_field!(self, other, imp, margin_left, StyleProperty::MarginLeft),
            outline_top: cascade_border_field!(
                self,
                other,
                imp,
                outline_top,
                StyleProperty::OutlineTop
            ),
            outline_right: cascade_border_field!(
                self,
                other,
                imp,
                outline_right,
                StyleProperty::OutlineRight
            ),
            outline_bottom: cascade_border_field!(
                self,
                other,
                imp,
                outline_bottom,
                StyleProperty::OutlineBottom
            ),
            outline_left: cascade_border_field!(
                self,
                other,
                imp,
                outline_left,
                StyleProperty::OutlineLeft
            ),
            border_title_align: cascade_field!(
                self,
                other,
                imp,
                border_title_align,
                StyleProperty::BorderTitleAlign
            ),
            border_subtitle_align: cascade_field!(
                self,
                other,
                imp,
                border_subtitle_align,
                StyleProperty::BorderSubtitleAlign
            ),
            border_title_color: cascade_field!(
                self,
                other,
                imp,
                border_title_color,
                StyleProperty::BorderTitleColor
            ),
            border_title_background: cascade_field!(
                self,
                other,
                imp,
                border_title_background,
                StyleProperty::BorderTitleBackground
            ),
            border_title_style: cascade_field!(
                self,
                other,
                imp,
                border_title_style,
                StyleProperty::BorderTitleStyle
            ),
            border_subtitle_color: cascade_field!(
                self,
                other,
                imp,
                border_subtitle_color,
                StyleProperty::BorderSubtitleColor
            ),
            border_subtitle_background: cascade_field!(
                self,
                other,
                imp,
                border_subtitle_background,
                StyleProperty::BorderSubtitleBackground
            ),
            border_subtitle_style: cascade_field!(
                self,
                other,
                imp,
                border_subtitle_style,
                StyleProperty::BorderSubtitleStyle
            ),
            scrollbar_color: cascade_field!(
                self,
                other,
                imp,
                scrollbar_color,
                StyleProperty::ScrollbarColor
            ),
            scrollbar_color_hover: cascade_field!(
                self,
                other,
                imp,
                scrollbar_color_hover,
                StyleProperty::ScrollbarColorHover
            ),
            scrollbar_color_active: cascade_field!(
                self,
                other,
                imp,
                scrollbar_color_active,
                StyleProperty::ScrollbarColorActive
            ),
            scrollbar_background: cascade_field!(
                self,
                other,
                imp,
                scrollbar_background,
                StyleProperty::ScrollbarBackground
            ),
            scrollbar_background_hover: cascade_field!(
                self,
                other,
                imp,
                scrollbar_background_hover,
                StyleProperty::ScrollbarBackgroundHover
            ),
            scrollbar_background_active: cascade_field!(
                self,
                other,
                imp,
                scrollbar_background_active,
                StyleProperty::ScrollbarBackgroundActive
            ),
            scrollbar_corner_color: cascade_field!(
                self,
                other,
                imp,
                scrollbar_corner_color,
                StyleProperty::ScrollbarCornerColor
            ),
            scrollbar_gutter: cascade_field!(
                self,
                other,
                imp,
                scrollbar_gutter,
                StyleProperty::ScrollbarGutter
            ),
            scrollbar_size: cascade_field!(
                self,
                other,
                imp,
                scrollbar_size,
                StyleProperty::ScrollbarSize
            ),
            scrollbar_size_horizontal: cascade_field!(
                self,
                other,
                imp,
                scrollbar_size_horizontal,
                StyleProperty::ScrollbarSizeHorizontal
            ),
            scrollbar_size_vertical: cascade_field!(
                self,
                other,
                imp,
                scrollbar_size_vertical,
                StyleProperty::ScrollbarSizeVertical
            ),
            scrollbar_visibility: cascade_field!(
                self,
                other,
                imp,
                scrollbar_visibility,
                StyleProperty::ScrollbarVisibility
            ),
            text_wrap: cascade_field!(self, other, imp, text_wrap, StyleProperty::TextWrapProp),
            text_overflow: cascade_field!(
                self,
                other,
                imp,
                text_overflow,
                StyleProperty::TextOverflowProp
            ),
            link_color: cascade_field!(self, other, imp, link_color, StyleProperty::LinkColor),
            // `link_color_auto` pairs with `link_color`: the style that wins the
            // link-color slot also provides its auto marker.
            link_color_auto: if other.link_color.is_some() || other.link_color_auto.is_some() {
                other.link_color_auto
            } else {
                self.link_color_auto
            },
            link_background: cascade_field!(
                self,
                other,
                imp,
                link_background,
                StyleProperty::LinkBackground
            ),
            link_style: cascade_field!(self, other, imp, link_style, StyleProperty::LinkStyleProp),
            link_color_hover: cascade_field!(
                self,
                other,
                imp,
                link_color_hover,
                StyleProperty::LinkColorHover
            ),
            link_color_hover_auto: if other.link_color_hover.is_some()
                || other.link_color_hover_auto.is_some()
            {
                other.link_color_hover_auto
            } else {
                self.link_color_hover_auto
            },
            link_background_hover: cascade_field!(
                self,
                other,
                imp,
                link_background_hover,
                StyleProperty::LinkBackgroundHover
            ),
            link_style_hover: cascade_field!(
                self,
                other,
                imp,
                link_style_hover,
                StyleProperty::LinkStyleHover
            ),
            row_span: cascade_field!(self, other, imp, row_span, StyleProperty::RowSpan),
            column_span: cascade_field!(self, other, imp, column_span, StyleProperty::ColumnSpan),
            hatch: cascade_field!(self, other, imp, hatch, StyleProperty::HatchProp),
            overlay: cascade_field!(self, other, imp, overlay, StyleProperty::OverlayProp),
            keyline: cascade_field!(self, other, imp, keyline, StyleProperty::KeylineProp),
            constrain_x: cascade_field!(self, other, imp, constrain_x, StyleProperty::ConstrainX),
            constrain_y: cascade_field!(self, other, imp, constrain_y, StyleProperty::ConstrainY),
            expand: cascade_field!(self, other, imp, expand, StyleProperty::ExpandProp),
            transitions: cascade_field!(
                self,
                other,
                imp,
                transitions,
                StyleProperty::TransitionsProp
            ),
            line_pad: cascade_field!(self, other, imp, line_pad, StyleProperty::LinePad),
            importance: imp,
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
            strike: self.strike.or(parent.strike),
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
            overflow_x: self.overflow_x,
            overflow_y: self.overflow_y,
            dock: self.dock,
            // text_align IS inherited (CSS semantics).
            text_align: self.text_align.or(parent.text_align),
            // content_align, align, offset are NOT inherited.
            content_align: self.content_align,
            align: self.align,
            offset: self.offset,
            pointer: self.pointer,
            // constrain is NOT inherited (render hint for overlays).
            constrain: self.constrain,
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
            // --- P2 CSS gap fields: none are inherited (layout/render properties) ---
            position: self.position,
            box_sizing: self.box_sizing,
            split: self.split,
            padding_top: self.padding_top,
            padding_right: self.padding_right,
            padding_bottom: self.padding_bottom,
            padding_left: self.padding_left,
            margin_top: self.margin_top,
            margin_right: self.margin_right,
            margin_bottom: self.margin_bottom,
            margin_left: self.margin_left,
            outline_top: self.outline_top,
            outline_right: self.outline_right,
            outline_bottom: self.outline_bottom,
            outline_left: self.outline_left,
            border_title_align: self.border_title_align,
            border_subtitle_align: self.border_subtitle_align,
            border_title_color: self.border_title_color,
            border_title_background: self.border_title_background,
            border_title_style: self.border_title_style,
            border_subtitle_color: self.border_subtitle_color,
            border_subtitle_background: self.border_subtitle_background,
            border_subtitle_style: self.border_subtitle_style,
            scrollbar_color: self.scrollbar_color,
            scrollbar_color_hover: self.scrollbar_color_hover,
            scrollbar_color_active: self.scrollbar_color_active,
            scrollbar_background: self.scrollbar_background,
            scrollbar_background_hover: self.scrollbar_background_hover,
            scrollbar_background_active: self.scrollbar_background_active,
            scrollbar_corner_color: self.scrollbar_corner_color,
            scrollbar_gutter: self.scrollbar_gutter,
            scrollbar_size: self.scrollbar_size,
            scrollbar_size_horizontal: self.scrollbar_size_horizontal,
            scrollbar_size_vertical: self.scrollbar_size_vertical,
            scrollbar_visibility: self.scrollbar_visibility,
            // text_wrap IS inherited (like text-align in CSS).
            text_wrap: self.text_wrap.or(parent.text_wrap),
            // text_overflow IS inherited.
            text_overflow: self.text_overflow.or(parent.text_overflow),
            // link styling IS inherited (children see parent link colors).
            link_color: self.link_color.or(parent.link_color),
            link_color_auto: if self.link_color.is_some() || self.link_color_auto.is_some() {
                self.link_color_auto
            } else {
                parent.link_color_auto
            },
            link_background: self.link_background.or(parent.link_background),
            link_style: self.link_style.or(parent.link_style),
            link_color_hover: self.link_color_hover.or(parent.link_color_hover),
            link_color_hover_auto: if self.link_color_hover.is_some()
                || self.link_color_hover_auto.is_some()
            {
                self.link_color_hover_auto
            } else {
                parent.link_color_hover_auto
            },
            link_background_hover: self.link_background_hover.or(parent.link_background_hover),
            link_style_hover: self.link_style_hover.or(parent.link_style_hover),
            row_span: self.row_span,
            column_span: self.column_span,
            hatch: self.hatch,
            overlay: self.overlay,
            keyline: self.keyline,
            constrain_x: self.constrain_x,
            constrain_y: self.constrain_y,
            expand: self.expand,
            transitions: self.transitions.clone(),
            // line_pad is NOT inherited (render-time property, not part of box model).
            line_pad: self.line_pad,
            // Importance is not inherited — it only applies during cascade.
            importance: ImportanceBitset::new(),
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
            || self.strike.is_some()
    }

    pub fn to_rich(&self) -> Option<rich_rs::Style> {
        let default_bg = parse_color_like("$background").unwrap_or(Color::rgb(0, 0, 0));
        self.to_rich_over(default_bg)
    }

    pub fn to_rich_over(&self, default_bg: Color) -> Option<rich_rs::Style> {
        if !self.has_rich_text_attrs() {
            return None;
        }
        let mut style = rich_rs::Style::new();
        let mut effective_bg = default_bg;
        if let Some(bg) = self.bg {
            if bg.a >= 1.0 {
                effective_bg = bg;
                style = style.with_bgcolor(bg.to_simple_opaque());
            } else if bg.a > 0.0 {
                let flat = bg.flatten_over(default_bg);
                effective_bg = flat;
                style = style.with_bgcolor(flat.to_simple_opaque());
            }
        }
        if let Some(fg) = self.fg {
            if fg.a >= 1.0 {
                style = style.with_color(fg.to_simple_opaque());
            } else if fg.a > 0.0 {
                style = style.with_color(fg.flatten_over(effective_bg).to_simple_opaque());
            }
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
        if let Some(strike) = self.strike {
            style = style.with_strike(strike);
        }
        Some(style)
    }

    pub fn to_rich_without_colors(&self) -> Option<rich_rs::Style> {
        if self.bold.is_none()
            && self.dim.is_none()
            && self.italic.is_none()
            && self.underline.is_none()
            && self.reverse.is_none()
            && self.strike.is_none()
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
        if let Some(strike) = self.strike {
            style = style.with_strike(strike);
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
            && self.strike.is_none()
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
            && self.overflow_x.is_none()
            && self.overflow_y.is_none()
            && self.dock.is_none()
            && self.text_align.is_none()
            && self.content_align.is_none()
            && self.align.is_none()
            && self.offset.is_none()
            && self.pointer.is_none()
            && self.constrain.is_none()
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
            // --- P2 CSS gap fields ---
            && self.position.is_none()
            && self.box_sizing.is_none()
            && self.split.is_none()
            && self.padding_top.is_none()
            && self.padding_right.is_none()
            && self.padding_bottom.is_none()
            && self.padding_left.is_none()
            && self.margin_top.is_none()
            && self.margin_right.is_none()
            && self.margin_bottom.is_none()
            && self.margin_left.is_none()
            && self.outline_top == BorderEdge::Unset
            && self.outline_right == BorderEdge::Unset
            && self.outline_bottom == BorderEdge::Unset
            && self.outline_left == BorderEdge::Unset
            && self.border_title_align.is_none()
            && self.border_subtitle_align.is_none()
            && self.border_title_color.is_none()
            && self.border_title_background.is_none()
            && self.border_title_style.is_none()
            && self.border_subtitle_color.is_none()
            && self.border_subtitle_background.is_none()
            && self.border_subtitle_style.is_none()
            && self.scrollbar_color.is_none()
            && self.scrollbar_color_hover.is_none()
            && self.scrollbar_color_active.is_none()
            && self.scrollbar_background.is_none()
            && self.scrollbar_background_hover.is_none()
            && self.scrollbar_background_active.is_none()
            && self.scrollbar_corner_color.is_none()
            && self.scrollbar_gutter.is_none()
            && self.scrollbar_size.is_none()
            && self.scrollbar_size_horizontal.is_none()
            && self.scrollbar_size_vertical.is_none()
            && self.scrollbar_visibility.is_none()
            && self.text_wrap.is_none()
            && self.text_overflow.is_none()
            && self.link_color.is_none()
            && self.link_background.is_none()
            && self.link_style.is_none()
            && self.link_color_hover.is_none()
            && self.link_background_hover.is_none()
            && self.link_style_hover.is_none()
            && self.row_span.is_none()
            && self.column_span.is_none()
            && self.hatch.is_none()
            && self.overlay.is_none()
            && self.keyline.is_none()
            && self.constrain_x.is_none()
            && self.constrain_y.is_none()
            && self.expand.is_none()
            && self.transitions.is_none()
    }

    // --- Devtools introspection ---

    /// Returns `(property_name, formatted_value)` pairs for every set (non-None/non-Unset)
    /// property. Used by the devtools snapshot protocol to expose resolved CSS.
    pub fn debug_properties(&self) -> Vec<(&'static str, String)> {
        let mut out = Vec::new();

        fn fmt_color(c: &Color) -> String {
            if c.a >= 1.0 {
                format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
            } else {
                format!("rgba({},{},{},{:.2})", c.r, c.g, c.b, c.a)
            }
        }
        fn fmt_border_edge(e: &BorderEdge) -> Option<String> {
            match e {
                BorderEdge::Unset => None,
                BorderEdge::None => Some("none".to_string()),
                BorderEdge::Edge { border_type, color } => {
                    Some(format!("{:?} {}", border_type, fmt_color(color)).to_lowercase())
                }
            }
        }
        fn fmt_scalar(s: &Scalar) -> String {
            match s {
                Scalar::Auto => "auto".to_string(),
                Scalar::Cells(n) => format!("{n}"),
                Scalar::Percent(p) => format!("{p}%"),
                Scalar::Fraction(f) => format!("{f}fr"),
                Scalar::Width(p) => format!("{p}w"),
                Scalar::Height(p) => format!("{p}h"),
                Scalar::ViewWidth(p) => format!("{p}vw"),
                Scalar::ViewHeight(p) => format!("{p}vh"),
            }
        }
        fn fmt_spacing(s: &Spacing) -> String {
            if s.top == s.right && s.right == s.bottom && s.bottom == s.left {
                format!("{}", s.top)
            } else if s.top == s.bottom && s.right == s.left {
                format!("{} {}", s.top, s.right)
            } else {
                format!("{} {} {} {}", s.top, s.right, s.bottom, s.left)
            }
        }
        fn fmt_text_style_flags(f: &TextStyleFlags) -> String {
            let mut parts = Vec::new();
            if f.bold {
                parts.push("bold");
            }
            if f.dim {
                parts.push("dim");
            }
            if f.italic {
                parts.push("italic");
            }
            if f.underline {
                parts.push("underline");
            }
            if f.reverse {
                parts.push("reverse");
            }
            if f.strike {
                parts.push("strike");
            }
            if parts.is_empty() {
                "none".to_string()
            } else {
                parts.join(" ")
            }
        }

        // Text / color
        if let Some(c) = &self.fg {
            out.push(("fg", fmt_color(c)));
        }
        if let Some(a) = &self.fg_auto {
            out.push(("fg-auto", format!("{}%", a.alpha_percent)));
        }
        if let Some(c) = &self.bg {
            out.push(("bg", fmt_color(c)));
        }
        if let Some(v) = self.text_opacity {
            out.push(("text-opacity", format!("{v}%")));
        }
        if let Some(v) = self.opacity {
            out.push(("opacity", format!("{v}%")));
        }
        if let Some(v) = self.bold {
            out.push(("bold", v.to_string()));
        }
        if let Some(v) = self.dim {
            out.push(("dim", v.to_string()));
        }
        if let Some(v) = self.italic {
            out.push(("italic", v.to_string()));
        }
        if let Some(v) = self.underline {
            out.push(("underline", v.to_string()));
        }
        if let Some(v) = self.reverse {
            out.push(("reverse", v.to_string()));
        }
        if let Some(v) = self.strike {
            out.push(("strike", v.to_string()));
        }

        // Border
        if let Some(v) = self.border {
            out.push(("border", v.to_string()));
        }
        if let Some(s) = fmt_border_edge(&self.border_top) {
            out.push(("border-top", s));
        }
        if let Some(s) = fmt_border_edge(&self.border_right) {
            out.push(("border-right", s));
        }
        if let Some(s) = fmt_border_edge(&self.border_bottom) {
            out.push(("border-bottom", s));
        }
        if let Some(s) = fmt_border_edge(&self.border_left) {
            out.push(("border-left", s));
        }

        // Tint
        if let Some(t) = &self.tint {
            out.push(("tint", format!("{} {}%", fmt_color(&t.color), t.percent)));
        }
        if let Some(t) = &self.background_tint {
            out.push((
                "background-tint",
                format!("{} {}%", fmt_color(&t.color), t.percent),
            ));
        }

        // Spacing
        if let Some(s) = &self.margin {
            out.push(("margin", fmt_spacing(s)));
        }
        if let Some(s) = &self.padding {
            out.push(("padding", fmt_spacing(s)));
        }

        // Size
        if let Some(v) = &self.width {
            out.push(("width", fmt_scalar(v)));
        }
        if let Some(v) = &self.height {
            out.push(("height", fmt_scalar(v)));
        }
        if let Some(v) = &self.min_width {
            out.push(("min-width", fmt_scalar(v)));
        }
        if let Some(v) = &self.max_width {
            out.push(("max-width", fmt_scalar(v)));
        }
        if let Some(v) = &self.min_height {
            out.push(("min-height", fmt_scalar(v)));
        }
        if let Some(v) = &self.max_height {
            out.push(("max-height", fmt_scalar(v)));
        }

        // Layout
        if let Some(v) = &self.layout {
            out.push(("layout", format!("{v:?}").to_lowercase()));
        }
        if let Some(v) = &self.display {
            out.push(("display", format!("{v:?}").to_lowercase()));
        }
        if let Some(v) = &self.visibility {
            out.push(("visibility", format!("{v:?}").to_lowercase()));
        }
        if let Some(v) = &self.overflow {
            out.push(("overflow", format!("{v:?}").to_lowercase()));
        }
        if let Some(v) = &self.overflow_x {
            out.push(("overflow-x", format!("{v:?}").to_lowercase()));
        }
        if let Some(v) = &self.overflow_y {
            out.push(("overflow-y", format!("{v:?}").to_lowercase()));
        }
        if let Some(v) = &self.dock {
            out.push(("dock", format!("{v:?}").to_lowercase()));
        }

        // Alignment
        if let Some(v) = &self.text_align {
            out.push(("text-align", format!("{v:?}").to_lowercase()));
        }
        if let Some(v) = &self.content_align {
            out.push((
                "content-align",
                format!("{:?} {:?}", v.horizontal, v.vertical).to_lowercase(),
            ));
        }
        if let Some(v) = &self.align {
            out.push((
                "align",
                format!("{:?} {:?}", v.horizontal, v.vertical).to_lowercase(),
            ));
        }
        if let Some(v) = &self.offset {
            out.push(("offset", format!("{:?} {:?}", v.x, v.y)));
        }

        // Pointer
        if let Some(v) = &self.pointer {
            out.push(("pointer", format!("{v:?}").to_lowercase()));
        }

        // Constrain
        if let Some(v) = &self.constrain {
            out.push(("constrain", format!("{v:?}").to_lowercase()));
        }

        // Grid
        if let Some(v) = self.grid_size_columns {
            out.push(("grid-size-columns", v.to_string()));
        }
        if let Some(v) = self.grid_size_rows {
            out.push(("grid-size-rows", v.to_string()));
        }
        if let Some(v) = &self.grid_columns {
            out.push((
                "grid-columns",
                v.iter().map(fmt_scalar).collect::<Vec<_>>().join(" "),
            ));
        }
        if let Some(v) = &self.grid_rows {
            out.push((
                "grid-rows",
                v.iter().map(fmt_scalar).collect::<Vec<_>>().join(" "),
            ));
        }
        if let Some(v) = self.grid_gutter_horizontal {
            out.push(("grid-gutter-horizontal", v.to_string()));
        }
        if let Some(v) = self.grid_gutter_vertical {
            out.push(("grid-gutter-vertical", v.to_string()));
        }

        // Layer
        if let Some(v) = &self.layer {
            out.push(("layer", v.clone()));
        }
        if let Some(v) = &self.layers {
            out.push(("layers", v.join(" ")));
        }

        // Transitions
        if let Some(v) = self.transition_duration {
            out.push(("transition-duration", format!("{}ms", v.as_millis())));
        }
        if let Some(v) = self.transition_delay {
            out.push(("transition-delay", format!("{}ms", v.as_millis())));
        }
        if let Some(v) = &self.transition_timing {
            out.push(("transition-timing", format!("{v:?}").to_lowercase()));
        }

        // P2 properties
        if let Some(v) = &self.position {
            out.push(("position", format!("{v:?}").to_lowercase()));
        }
        if let Some(v) = &self.box_sizing {
            out.push(("box-sizing", format!("{v:?}").to_lowercase()));
        }
        if let Some(v) = &self.split {
            out.push(("split", format!("{v:?}").to_lowercase()));
        }

        // Per-side spacing overrides
        if let Some(v) = self.padding_top {
            out.push(("padding-top", v.to_string()));
        }
        if let Some(v) = self.padding_right {
            out.push(("padding-right", v.to_string()));
        }
        if let Some(v) = self.padding_bottom {
            out.push(("padding-bottom", v.to_string()));
        }
        if let Some(v) = self.padding_left {
            out.push(("padding-left", v.to_string()));
        }
        if let Some(v) = self.margin_top {
            out.push(("margin-top", v.to_string()));
        }
        if let Some(v) = self.margin_right {
            out.push(("margin-right", v.to_string()));
        }
        if let Some(v) = self.margin_bottom {
            out.push(("margin-bottom", v.to_string()));
        }
        if let Some(v) = self.margin_left {
            out.push(("margin-left", v.to_string()));
        }

        // Outline
        if let Some(s) = fmt_border_edge(&self.outline_top) {
            out.push(("outline-top", s));
        }
        if let Some(s) = fmt_border_edge(&self.outline_right) {
            out.push(("outline-right", s));
        }
        if let Some(s) = fmt_border_edge(&self.outline_bottom) {
            out.push(("outline-bottom", s));
        }
        if let Some(s) = fmt_border_edge(&self.outline_left) {
            out.push(("outline-left", s));
        }

        // Border title/subtitle
        if let Some(v) = &self.border_title_align {
            out.push(("border-title-align", format!("{v:?}").to_lowercase()));
        }
        if let Some(v) = &self.border_subtitle_align {
            out.push(("border-subtitle-align", format!("{v:?}").to_lowercase()));
        }
        if let Some(c) = &self.border_title_color {
            out.push(("border-title-color", fmt_color(c)));
        }
        if let Some(c) = &self.border_title_background {
            out.push(("border-title-background", fmt_color(c)));
        }
        if let Some(f) = &self.border_title_style {
            out.push(("border-title-style", fmt_text_style_flags(f)));
        }
        if let Some(c) = &self.border_subtitle_color {
            out.push(("border-subtitle-color", fmt_color(c)));
        }
        if let Some(c) = &self.border_subtitle_background {
            out.push(("border-subtitle-background", fmt_color(c)));
        }
        if let Some(f) = &self.border_subtitle_style {
            out.push(("border-subtitle-style", fmt_text_style_flags(f)));
        }

        // Scrollbar
        if let Some(c) = &self.scrollbar_color {
            out.push(("scrollbar-color", fmt_color(c)));
        }
        if let Some(c) = &self.scrollbar_color_hover {
            out.push(("scrollbar-color-hover", fmt_color(c)));
        }
        if let Some(c) = &self.scrollbar_color_active {
            out.push(("scrollbar-color-active", fmt_color(c)));
        }
        if let Some(c) = &self.scrollbar_background {
            out.push(("scrollbar-background", fmt_color(c)));
        }
        if let Some(c) = &self.scrollbar_background_hover {
            out.push(("scrollbar-background-hover", fmt_color(c)));
        }
        if let Some(c) = &self.scrollbar_background_active {
            out.push(("scrollbar-background-active", fmt_color(c)));
        }
        if let Some(c) = &self.scrollbar_corner_color {
            out.push(("scrollbar-corner-color", fmt_color(c)));
        }
        if let Some(v) = &self.scrollbar_gutter {
            out.push(("scrollbar-gutter", format!("{v:?}").to_lowercase()));
        }
        if let Some(v) = self.scrollbar_size {
            out.push(("scrollbar-size", v.to_string()));
        }
        if let Some(v) = self.scrollbar_size_horizontal {
            out.push(("scrollbar-size-horizontal", v.to_string()));
        }
        if let Some(v) = self.scrollbar_size_vertical {
            out.push(("scrollbar-size-vertical", v.to_string()));
        }
        if let Some(v) = &self.scrollbar_visibility {
            out.push(("scrollbar-visibility", format!("{v:?}").to_lowercase()));
        }

        // Text wrap/overflow
        if let Some(v) = &self.text_wrap {
            out.push(("text-wrap", format!("{v:?}").to_lowercase()));
        }
        if let Some(v) = &self.text_overflow {
            out.push(("text-overflow", format!("{v:?}").to_lowercase()));
        }

        // Link styling
        if let Some(c) = &self.link_color {
            out.push(("link-color", fmt_color(c)));
        }
        if let Some(c) = &self.link_background {
            out.push(("link-background", fmt_color(c)));
        }
        if let Some(f) = &self.link_style {
            out.push(("link-style", fmt_text_style_flags(f)));
        }
        if let Some(c) = &self.link_color_hover {
            out.push(("link-color-hover", fmt_color(c)));
        }
        if let Some(c) = &self.link_background_hover {
            out.push(("link-background-hover", fmt_color(c)));
        }
        if let Some(f) = &self.link_style_hover {
            out.push(("link-style-hover", fmt_text_style_flags(f)));
        }

        // Grid child
        if let Some(v) = self.row_span {
            out.push(("row-span", v.to_string()));
        }
        if let Some(v) = self.column_span {
            out.push(("column-span", v.to_string()));
        }

        // Hatch, overlay, keyline
        if let Some(h) = &self.hatch {
            out.push((
                "hatch",
                format!("'{}' {}", h.character, fmt_color(&h.color)),
            ));
        }
        if let Some(v) = &self.overlay {
            out.push(("overlay", format!("{v:?}").to_lowercase()));
        }
        if let Some(k) = &self.keyline {
            out.push((
                "keyline",
                format!("{:?} {}", k.keyline_type, fmt_color(&k.color)).to_lowercase(),
            ));
        }

        // Constrain-x/y, expand
        if let Some(v) = &self.constrain_x {
            out.push(("constrain-x", format!("{v:?}").to_lowercase()));
        }
        if let Some(v) = &self.constrain_y {
            out.push(("constrain-y", format!("{v:?}").to_lowercase()));
        }
        if let Some(v) = self.expand {
            out.push(("expand", v.to_string()));
        }

        // Per-property transitions
        if let Some(ts) = &self.transitions {
            for t in ts {
                out.push((
                    "transition",
                    format!(
                        "{} {}ms {:?} {}ms",
                        t.property,
                        t.duration.as_millis(),
                        t.timing,
                        t.delay.as_millis()
                    )
                    .to_lowercase(),
                ));
            }
        }

        // Line-pad
        if let Some(v) = self.line_pad {
            out.push(("line-pad", v.to_string()));
        }

        out
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

    /// Byte-exact LAB lighten/darken parity with Python Textual
    /// (`textual.color.Color.lighten/.darken`, easyrgb f64 form).
    /// Expected hexes generated from the Python reference; any divergence here
    /// means the LAB conversion drifted from Python and shade tokens will mismatch.
    #[test]
    fn lab_shade_parity_with_python() {
        fn hex(c: Color) -> String {
            format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b)
        }
        // (input_hex, amount, expected_hex). amount >= 0 => darken(amount);
        // amount < 0 => lighten(-amount). Mirrors Color.darken/.lighten.
        let cases: &[(&str, f64, &str)] = &[
            ("#004578", 0.10, "#002F5E"),
            ("#004578", 0.20, "#001A46"),
            ("#004578", 0.30, "#000030"),
            ("#004578", -0.10, "#2C5C91"),
            ("#004578", -0.20, "#4974AC"),
            ("#0178D4", 0.10, "#005FB7"),
            ("#0178D4", 0.20, "#00489C"),
            ("#0178D4", 0.30, "#003282"),
            ("#0178D4", -0.10, "#4091F0"),
            ("#0178D4", -0.20, "#64ABFF"),
            ("#FFA62B", 0.10, "#DF8B00"),
            ("#FFA62B", 0.20, "#C07100"),
            ("#FFA62B", 0.30, "#A15900"),
            ("#FFA62B", -0.10, "#FFC148"),
            ("#FFA62B", -0.20, "#FFDD64"),
            ("#BA3C5B", 0.10, "#9C1D43"),
            ("#BA3C5B", 0.20, "#7F002D"),
            ("#BA3C5B", 0.30, "#620019"),
            ("#BA3C5B", -0.10, "#D85773"),
            ("#BA3C5B", -0.20, "#F6728C"),
            ("#1E1E1E", 0.10, "#040404"),
            ("#1E1E1E", 0.20, "#000000"),
            ("#1E1E1E", -0.10, "#333333"),
            ("#1E1E1E", -0.20, "#494949"),
            ("#121212", 0.10, "#000000"),
            ("#121212", -0.10, "#262626"),
            ("#121212", -0.20, "#3C3C3C"),
            ("#E0E0E0", 0.10, "#C4C4C4"),
            ("#E0E0E0", 0.20, "#A8A8A8"),
            ("#E0E0E0", 0.30, "#8E8E8E"),
            ("#E0E0E0", -0.10, "#FCFCFC"),
            ("#E0E0E0", -0.20, "#FFFFFF"),
            ("#3465A4", 0.10, "#094D89"),
            ("#3465A4", 0.20, "#00366F"),
            ("#3465A4", 0.30, "#002256"),
            ("#3465A4", -0.10, "#527DBF"),
            ("#3465A4", -0.20, "#6E97DB"),
            ("#264F78", 0.10, "#00385F"),
            ("#264F78", 0.20, "#002347"),
            ("#264F78", 0.30, "#000C30"),
            ("#264F78", -0.10, "#416691"),
            ("#264F78", -0.20, "#5C7FAC"),
        ];
        for (input, amount, expected) in cases {
            let c = parse_color_like(input).expect("parse input hex");
            let out = if *amount >= 0.0 {
                darken_lab(c, *amount)
            } else {
                lighten_lab(c, -*amount)
            };
            assert_eq!(
                hex(out),
                *expected,
                "lab shade {input} {amount:+.2} -> got {} expected {expected}",
                hex(out)
            );
        }
    }

    /// The resolved `$token` design tokens (bare + lighten/darken shades) for the
    /// default `textual-dark` theme must be byte-exact to Python Textual's
    /// `ColorSystem.generate()`. The bare base-color tokens are Python's `n == 0`
    /// shade (LAB round-trip), so accent/warning resolve to `#FEA62B` and error to
    /// `#B93C5B` even though the SHADES derive from the un-round-tripped base.
    #[test]
    fn dark_design_tokens_match_python_generate() {
        fn tok(name: &str) -> String {
            let c = parse_color_like(&format!("${name}"))
                .unwrap_or_else(|| panic!("token ${name} did not resolve"));
            format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b)
        }
        // (token, expected hex from Python `cs.generate()`).
        let cases: &[(&str, &str)] = &[
            // Bare base colors = Python's n==0 shade (LAB round-trip).
            ("primary", "#0178D4"),
            ("secondary", "#004578"),
            ("accent", "#FEA62B"),
            ("warning", "#FEA62B"),
            ("error", "#B93C5B"),
            ("success", "#4EBF71"),
            // Shades derive from the ORIGINAL (un-round-tripped) base color.
            ("accent-darken-1", "#E7920D"),
            ("accent-darken-2", "#CF7E00"),
            ("accent-darken-3", "#B86B00"),
            ("accent-lighten-1", "#FFBA41"),
            ("accent-lighten-2", "#FFCF56"),
            ("accent-lighten-3", "#FFE46B"),
            ("warning-darken-1", "#E7920D"),
            ("error-darken-1", "#A32549"),
            ("error-darken-2", "#8D0638"),
            ("error-darken-3", "#780028"),
            ("error-lighten-1", "#D0506D"),
            ("error-lighten-2", "#E76580"),
            ("error-lighten-3", "#FE7993"),
            ("primary-darken-1", "#0065BE"),
            ("primary-darken-2", "#0053AA"),
            ("primary-darken-3", "#004295"),
            ("primary-lighten-1", "#368AE9"),
            ("primary-lighten-2", "#539EFF"),
            ("primary-lighten-3", "#6DB2FF"),
            ("secondary-darken-1", "#003465"),
            ("secondary-lighten-1", "#23568B"),
            ("success-darken-1", "#36AA5E"),
            ("success-lighten-1", "#64D484"),
            ("surface-lighten-1", "#2D2D2D"),
            ("surface-darken-1", "#0D0D0D"),
            // Previously hardcoded in the token map; now derived by the generic
            // LAB shade path (proven bit-exact to Python's darken/lighten).
            ("success-lighten-2", "#7AE998"),
            ("success-lighten-3", "#8FFFAC"),
            ("success-darken-2", "#18954B"),
            ("success-darken-3", "#008139"),
            ("warning-lighten-1", "#FFBA41"),
            ("warning-lighten-2", "#FFCF56"),
            ("warning-lighten-3", "#FFE46B"),
            ("warning-darken-2", "#CF7E00"),
            ("warning-darken-3", "#B86B00"),
            ("surface-lighten-2", "#3E3E3E"),
            ("surface-lighten-3", "#4F4F4F"),
            ("surface-darken-2", "#000000"),
            ("surface-darken-3", "#000000"),
            ("secondary-darken-2", "#002452"),
            ("secondary-darken-3", "#001541"),
            ("secondary-lighten-2", "#3B689F"),
            ("secondary-lighten-3", "#507BB3"),
            // Muted family: `base.blend(background, 0.7)` (design.py), derived
            // by the generic `-muted` path.
            ("primary-muted", "#0C304C"),
            ("secondary-muted", "#0C2130"),
            ("accent-muted", "#593E19"),
            ("warning-muted", "#593E19"),
            ("error-muted", "#441E27"),
            ("success-muted", "#24452E"),
            // Computed bases (panel = surface+primary 10% with dark boost) and
            // panel shades derived from that computed base.
            ("panel", "#242F38"),
            ("panel-lighten-1", "#343F49"),
            ("panel-darken-1", "#141F27"),
        ];
        for (name, expected) in cases {
            assert_eq!(tok(name), *expected, "design token ${name}");
        }
    }

    #[test]
    fn resolve_scalar_exact_keeps_fraction() {
        // `12.5h` of height 30 = 3.75 — the exact pre-floor value used for
        // Python-style CUMULATIVE flooring (two adjacent 3.75 boxes render 3+4,
        // not the 3+3 that independent truncation would produce).
        let h = Scalar::Height(12.5);
        assert_eq!(resolve_scalar_exact(&h, 30, 120, 30, 120, 30), Some(3.75));
        // Integer-valued scalar (`5w` of width 120 = 6.0) stays exact and is a
        // no-op under cumulative flooring.
        let w = Scalar::Width(5.0);
        assert_eq!(resolve_scalar_exact(&w, 30, 120, 30, 120, 30), Some(6.0));
        // `25vh` always resolves against the VIEWPORT height (30), regardless of
        // the property axis: 25% of 30 = 7.5.
        let vh = Scalar::ViewHeight(25.0);
        assert_eq!(resolve_scalar_exact(&vh, 120, 120, 30, 120, 30), Some(7.5));
        // `auto`/`fr` have no direct percentage base → resolved by the 1D solver.
        assert_eq!(resolve_scalar_exact(&Scalar::Auto, 30, 30, 30, 30, 30), None);
        assert_eq!(
            resolve_scalar_exact(&Scalar::Fraction(2.0), 30, 30, 30, 30, 30),
            None
        );
    }

    #[test]
    fn cumulative_floor_recovers_fractional_carry() {
        // The fence-post identity the flow layouts rely on: a stack of equal
        // 3.75-cell boxes renders so the cumulative position is floored at each
        // step (3,4,4,4 here), summing to floor(N*3.75)=15, instead of every box
        // independently flooring to 3 (sum 12 — losing 25% of the total). This is
        // Python `layouts/vertical.py` (`next_y.__floor__() - y.__floor__()`).
        let exact = [3.75_f64; 4];
        let mut cum = 0.0_f64;
        let mut sizes = Vec::new();
        for e in exact {
            let disp = ((cum + e).floor() - cum.floor()) as u16;
            sizes.push(disp);
            cum += e;
        }
        assert_eq!(sizes, vec![3, 4, 4, 4]);
        assert_eq!(sizes.iter().sum::<u16>(), (4.0 * 3.75_f64).floor() as u16);
        // Independent truncation would have summed to only 4*3 = 12.
        assert!(sizes.iter().sum::<u16>() > 4 * 3);
    }

    #[test]
    fn css_named_colors_use_w3c_values_not_ansi_palette() {
        // CSS keywords resolve to their W3C values (Python COLOR_NAME_TO_RGB),
        // taking precedence over rich-rs's ANSI-palette names where the names
        // collide (e.g. `white` is #ffffff, not the dim ANSI standard white).
        assert_eq!(parse_color_like("white"), Some(Color::rgb(255, 255, 255)));
        assert_eq!(parse_color_like("cyan"), Some(Color::rgb(0, 255, 255)));
        assert_eq!(parse_color_like("yellow"), Some(Color::rgb(255, 255, 0)));
        assert_eq!(parse_color_like("magenta"), Some(Color::rgb(255, 0, 255)));
        // CSS `green` is #008000 (not lime #00ff00) — a classic CSS/ANSI gotcha.
        assert_eq!(parse_color_like("green"), Some(Color::rgb(0, 128, 0)));
        assert_eq!(parse_color_like("lime"), Some(Color::rgb(0, 255, 0)));
        // Case-insensitive; extended keywords resolve too.
        assert_eq!(parse_color_like("White"), Some(Color::rgb(255, 255, 255)));
        assert_eq!(parse_color_like("rebeccapurple"), Some(Color::rgb(102, 51, 153)));
        // `ansi_*` names keep the terminal-palette values (handled separately).
        assert_eq!(parse_color_like("ansi_white"), Some(Color::rgb(192, 192, 192)));
    }

    #[test]
    fn hsl_and_hsla_parse_to_rgb() {
        // hsl(240,100%,50%) = pure blue (Python Textual / CSS).
        assert_eq!(parse_color_like("hsl(240, 100%, 50%)"), Some(Color::rgb(0, 0, 255)));
        assert_eq!(parse_color_like("hsl(0, 100%, 50%)"), Some(Color::rgb(255, 0, 0)));
        assert_eq!(parse_color_like("hsl(120, 100%, 50%)"), Some(Color::rgb(0, 255, 0)));
        // s=0 => grey at lightness.
        assert_eq!(parse_color_like("hsl(0, 0%, 50%)"), Some(Color::rgb(128, 128, 128)));
        // hsla carries alpha (kept as a float, not u8-quantized).
        assert_eq!(
            parse_color_like("hsla(240, 100%, 50%, 0.5)"),
            Some(Color::rgba_f(0, 0, 255, 0.5))
        );
    }

    #[test]
    fn float_alpha_composites_like_python() {
        // `red 10%` over the dark background must use the exact float factor 0.1,
        // not the u8-quantized round(0.1*255)/255 = 0.10196. Python:
        // Color(18,18,18) + Color(255,0,0,a=0.1) == Color(41,16,16).
        let base = Color::rgb(18, 18, 18);
        let red10 = Color::rgb(255, 0, 0).with_alpha(0.1);
        assert!((red10.a - 0.1).abs() < 1e-6, "alpha stays the exact float 0.1");
        let composited = red10.flatten_over(base);
        assert_eq!(composited, Color::rgb(41, 16, 16));
        // The old u8 path would have produced 42 in the red channel.
        assert_ne!(composited.r, 42);
    }

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

    #[test]
    fn to_rich_ignores_fully_transparent_fg_and_bg() {
        let style = Style::new()
            .fg(Color::rgba(10, 20, 30, 0))
            .bg(Color::rgba(40, 50, 60, 0))
            .underline(true);

        let rich = style.to_rich().expect("style should map to rich style");
        assert_eq!(rich.color, None);
        assert_eq!(rich.bgcolor, None);
        assert_eq!(rich.underline, Some(true));
    }

    #[test]
    fn to_rich_flattens_semi_transparent_colors_against_default_background() {
        let style = Style::new()
            .bg(Color::rgba(255, 255, 255, 51)) // 20%
            .fg(Color::rgba(255, 255, 255, 128)); // 50%
        let rich = style.to_rich().expect("style should map to rich style");
        assert!(rich.bgcolor.is_some());
        assert!(rich.color.is_some());
        assert_ne!(
            rich.bgcolor,
            Some(Color::rgb(255, 255, 255).to_simple_opaque())
        );
        assert_ne!(
            rich.color,
            Some(Color::rgb(255, 255, 255).to_simple_opaque())
        );
    }

    // ---- Scalar resolve_scalar tests ----

    // Signature: resolve_scalar(scalar, parent_size, parent_width, parent_height,
    //                           viewport_width, viewport_height, fr_total, available)
    #[test]
    fn resolve_scalar_auto_returns_zero() {
        assert_eq!(resolve_scalar(&Scalar::Auto, 100, 100, 50, 200, 60, 0.0, 0), 0);
    }

    #[test]
    fn resolve_scalar_cells() {
        assert_eq!(
            resolve_scalar(&Scalar::Cells(42), 100, 100, 50, 200, 60, 0.0, 0),
            42
        );
    }

    #[test]
    fn resolve_scalar_percent() {
        assert_eq!(
            resolve_scalar(&Scalar::Percent(50.0), 80, 80, 40, 200, 60, 0.0, 0),
            40
        );
        assert_eq!(
            resolve_scalar(&Scalar::Percent(100.0), 80, 80, 40, 200, 60, 0.0, 0),
            80
        );
        assert_eq!(
            resolve_scalar(&Scalar::Percent(33.3), 100, 100, 50, 200, 60, 0.0, 0),
            33
        );
    }

    #[test]
    fn resolve_scalar_fraction() {
        // 1fr out of 3fr total, with 90 available → 30
        assert_eq!(resolve_scalar(&Scalar::Fraction(1.0), 0, 0, 0, 0, 0, 3.0, 90), 30);
        // 2fr out of 3fr total, with 90 available → 60
        assert_eq!(resolve_scalar(&Scalar::Fraction(2.0), 0, 0, 0, 0, 0, 3.0, 90), 60);
        // 0 total fr → 0
        assert_eq!(resolve_scalar(&Scalar::Fraction(1.0), 0, 0, 0, 0, 0, 0.0, 90), 0);
    }

    #[test]
    fn resolve_scalar_width_unit() {
        // `40w` = 40% of the parent WIDTH, regardless of the axis. Here the axis
        // dim (`parent_size`) is the parent HEIGHT (50), but `Width` must use the
        // parent width (100): 40% of 100 = 40.
        assert_eq!(
            resolve_scalar(&Scalar::Width(40.0), 50, 100, 50, 200, 60, 0.0, 0),
            40
        );
    }

    #[test]
    fn resolve_scalar_height_unit() {
        // `50h` = 50% of the parent HEIGHT, regardless of the axis. Here the axis
        // dim is the parent WIDTH (100), but `Height` must use the height (50):
        // 50% of 50 = 25.
        assert_eq!(
            resolve_scalar(&Scalar::Height(50.0), 100, 100, 50, 200, 60, 0.0, 0),
            25
        );
    }

    #[test]
    fn resolve_scalar_view_width() {
        // `50vw` = 50% of viewport WIDTH (120), not the height. 50% of 120 = 60.
        assert_eq!(
            resolve_scalar(&Scalar::ViewWidth(50.0), 0, 0, 0, 120, 30, 0.0, 0),
            60
        );
    }

    #[test]
    fn resolve_scalar_view_height() {
        // `25vh` = 25% of viewport HEIGHT (200), not the width. 25% of 200 = 50.
        assert_eq!(
            resolve_scalar(&Scalar::ViewHeight(25.0), 0, 0, 0, 120, 200, 0.0, 0),
            50
        );
    }

    // Regression (docs/examples/styles/width_comparison): a `vh` unit on a WIDTH
    // property must resolve against viewport HEIGHT, not viewport width. Before
    // the fix, `width: 25vh` in a 120x30 terminal resolved to 25% of 120 = 30
    // (using the width-axis viewport extent) instead of 25% of 30 (= 7.5 → 7,
    // Python truncates). Symmetrically, `vw` on a height property uses viewport
    // width.
    #[test]
    fn resolve_scalar_view_units_are_axis_absolute() {
        // viewport = 120 wide x 30 tall.
        // `25vh` resolves against HEIGHT (30): 25% of 30 = 7.5 → 7 (floor).
        assert_eq!(
            resolve_scalar(&Scalar::ViewHeight(25.0), 120, 120, 30, 120, 30, 0.0, 0),
            7
        );
        // `15vw` resolves against WIDTH (120): 15% of 120 = 18.
        assert_eq!(
            resolve_scalar(&Scalar::ViewWidth(15.0), 30, 120, 30, 120, 30, 0.0, 0),
            18
        );
    }

    // Python truncates (`int()`/`Fraction.__floor__()`) rather than rounds: a
    // non-integer percent floors down. Guards the `.round()` → `.floor()` change.
    #[test]
    fn resolve_scalar_percent_truncates() {
        // 75% of 30 = 22.5 → 22 (Python `min-height: 75%` cross-axis height).
        assert_eq!(
            resolve_scalar(&Scalar::Percent(75.0), 30, 30, 30, 0, 0, 0.0, 0),
            22
        );
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
        assert_eq!(
            resolve_scalar(&Scalar::Percent(0.0), 100, 100, 50, 200, 60, 0.0, 0),
            0
        );
    }

    #[test]
    fn scalar_cells_zero() {
        assert_eq!(
            resolve_scalar(&Scalar::Cells(0), 100, 100, 50, 200, 60, 0.0, 0),
            0
        );
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
        assert_eq!(combined.layers.as_ref().map(|v| v.len()), Some(2));
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

    // ---- ImportanceBitset tests ----

    #[test]
    fn bitset_default_is_empty() {
        let b = ImportanceBitset::new();
        assert!(b.is_empty());
        assert!(!b.get(StyleProperty::Fg));
        assert!(!b.get(StyleProperty::Bg));
    }

    #[test]
    fn bitset_set_and_get() {
        let mut b = ImportanceBitset::new();
        b.set(StyleProperty::Bg);
        assert!(b.get(StyleProperty::Bg));
        assert!(!b.get(StyleProperty::Fg));
        assert!(!b.is_empty());
    }

    #[test]
    fn bitset_multiple_properties() {
        let mut b = ImportanceBitset::new();
        b.set(StyleProperty::Fg);
        b.set(StyleProperty::Width);
        b.set(StyleProperty::TransitionTiming);
        assert!(b.get(StyleProperty::Fg));
        assert!(b.get(StyleProperty::Width));
        assert!(b.get(StyleProperty::TransitionTiming));
        assert!(!b.get(StyleProperty::Bg));
        assert!(!b.get(StyleProperty::Height));
    }

    #[test]
    fn bitset_correct_property_indices() {
        // Verify that each property maps to a distinct bit.
        let props = [
            StyleProperty::Fg,
            StyleProperty::Bg,
            StyleProperty::TextOpacity,
            StyleProperty::Bold,
            StyleProperty::BorderTop,
            StyleProperty::Margin,
            StyleProperty::Width,
            StyleProperty::Layout,
            StyleProperty::TextAlign,
            StyleProperty::GridSizeColumns,
            StyleProperty::Layer,
            StyleProperty::TransitionTiming,
        ];
        for (i, &prop) in props.iter().enumerate() {
            let mut b = ImportanceBitset::new();
            b.set(prop);
            // Only this property should be set.
            for (j, &other) in props.iter().enumerate() {
                if i == j {
                    assert!(b.get(other), "{:?} should be set", other);
                } else {
                    assert!(
                        !b.get(other),
                        "{:?} should NOT be set when {:?} is",
                        other,
                        prop
                    );
                }
            }
        }
    }

    // ---- Importance-aware combine tests ----

    #[test]
    fn combine_important_wins_over_normal_override() {
        // Self has bg with !important; other has bg without !important.
        // Self should win because !important overrides normal cascade.
        let mut base = Style::new().bg(Color::rgb(255, 0, 0));
        base.importance.set(StyleProperty::Bg);
        let overlay = Style::new().bg(Color::rgb(0, 255, 0));
        let combined = base.combine(&overlay);
        assert_eq!(combined.bg, Some(Color::rgb(255, 0, 0)));
        assert!(combined.importance.get(StyleProperty::Bg));
    }

    #[test]
    fn combine_both_important_other_wins() {
        // Both have bg with !important; other (higher specificity in cascade) wins.
        let mut base = Style::new().bg(Color::rgb(255, 0, 0));
        base.importance.set(StyleProperty::Bg);
        let mut overlay = Style::new().bg(Color::rgb(0, 255, 0));
        overlay.importance.set(StyleProperty::Bg);
        let combined = base.combine(&overlay);
        assert_eq!(combined.bg, Some(Color::rgb(0, 255, 0)));
        assert!(combined.importance.get(StyleProperty::Bg));
    }

    #[test]
    fn combine_normal_cascade_unchanged() {
        // Neither has !important; other wins as before.
        let base = Style::new().bg(Color::rgb(255, 0, 0));
        let overlay = Style::new().bg(Color::rgb(0, 255, 0));
        let combined = base.combine(&overlay);
        assert_eq!(combined.bg, Some(Color::rgb(0, 255, 0)));
        assert!(!combined.importance.get(StyleProperty::Bg));
    }

    #[test]
    fn combine_important_fg_auto_wins_over_normal_fg() {
        // Self has fg_auto with !important; other has concrete fg without !important.
        let mut base = Style::new().fg_auto(AutoColor::new(87));
        base.importance.set(StyleProperty::Fg);
        let overlay = Style::new().fg(Color::rgb(20, 20, 20));
        let combined = base.combine(&overlay);
        assert_eq!(combined.fg, None);
        assert_eq!(combined.fg_auto.map(|a| a.alpha_percent), Some(87));
        assert!(combined.importance.get(StyleProperty::Fg));
    }

    #[test]
    fn combine_important_not_overridden_by_normal_in_later_rule() {
        // Simulates cascade: rule A (low specificity, !important) → rule B (high specificity, normal).
        // Fold: start with A, then combine B.
        let mut rule_a = Style::new().fg(Color::rgb(255, 0, 0));
        rule_a.importance.set(StyleProperty::Fg);
        let rule_b = Style::new().fg(Color::rgb(0, 0, 255));
        // A is "self" (accumulated), B is "other" (higher specificity).
        let result = rule_a.combine(&rule_b);
        assert_eq!(
            result.fg,
            Some(Color::rgb(255, 0, 0)),
            "important fg should survive"
        );
    }

    #[test]
    fn combine_source_order_breaks_ties_at_same_importance() {
        // Both normal, applied in source order: later rule wins.
        let s1 = Style::new().bg(Color::rgb(255, 0, 0));
        let s2 = Style::new().bg(Color::rgb(0, 255, 0));
        let result = s1.combine(&s2);
        assert_eq!(result.bg, Some(Color::rgb(0, 255, 0)));
    }

    #[test]
    fn combine_importance_per_property_independent() {
        // Self has bg important, fg normal. Other has bg normal, fg important.
        // Result: bg from self (important), fg from other (important).
        let mut base = Style::new()
            .bg(Color::rgb(255, 0, 0))
            .fg(Color::rgb(100, 100, 100));
        base.importance.set(StyleProperty::Bg);
        let mut overlay = Style::new()
            .bg(Color::rgb(0, 255, 0))
            .fg(Color::rgb(200, 200, 200));
        overlay.importance.set(StyleProperty::Fg);
        let combined = base.combine(&overlay);
        assert_eq!(
            combined.bg,
            Some(Color::rgb(255, 0, 0)),
            "bg: self is important"
        );
        assert_eq!(
            combined.fg,
            Some(Color::rgb(200, 200, 200)),
            "fg: other is important"
        );
        assert!(combined.importance.get(StyleProperty::Bg));
        assert!(combined.importance.get(StyleProperty::Fg));
    }

    #[test]
    fn combine_important_border_edge_wins() {
        let mut base = Style::new();
        base.border_top = BorderEdge::Edge {
            border_type: BorderType::Solid,
            color: Color::rgb(255, 0, 0),
        };
        base.importance.set(StyleProperty::BorderTop);
        let mut overlay = Style::new();
        overlay.border_top = BorderEdge::Edge {
            border_type: BorderType::Block,
            color: Color::rgb(0, 255, 0),
        };
        let combined = base.combine(&overlay);
        assert_eq!(
            combined.border_top,
            BorderEdge::Edge {
                border_type: BorderType::Solid,
                color: Color::rgb(255, 0, 0),
            }
        );
    }

    #[test]
    fn inherit_from_clears_importance() {
        let mut child = Style::new().bg(Color::rgb(255, 0, 0));
        child.importance.set(StyleProperty::Bg);
        let parent = Style::new().fg(Color::rgb(0, 255, 0));
        let inherited = child.inherit_from(&parent);
        assert!(
            inherited.importance.is_empty(),
            "importance should be cleared after inheritance"
        );
    }

    // ---- Constrain enum tests ----

    #[test]
    fn constrain_default_is_none() {
        assert_eq!(Constrain::default(), Constrain::None);
    }

    #[test]
    fn constrain_field_in_combine_cascade() {
        let base = {
            let mut s = Style::new();
            s.constrain = Some(Constrain::Inside);
            s
        };
        let overlay = {
            let mut s = Style::new();
            s.constrain = Some(Constrain::Inflect);
            s
        };
        let combined = base.combine(&overlay);
        assert_eq!(combined.constrain, Some(Constrain::Inflect));
    }

    #[test]
    fn constrain_combine_preserves_base_when_overlay_is_none() {
        let base = {
            let mut s = Style::new();
            s.constrain = Some(Constrain::Inside);
            s
        };
        let overlay = Style::new();
        let combined = base.combine(&overlay);
        assert_eq!(combined.constrain, Some(Constrain::Inside));
    }

    #[test]
    fn constrain_not_inherited() {
        let parent = {
            let mut s = Style::new();
            s.constrain = Some(Constrain::Inside);
            s
        };
        let child = Style::new();
        let inherited = child.inherit_from(&parent);
        assert_eq!(inherited.constrain, None);
    }

    #[test]
    fn constrain_field_makes_style_not_empty() {
        let mut s = Style::new();
        assert!(s.is_empty());
        s.constrain = Some(Constrain::Inside);
        assert!(!s.is_empty());
    }

    #[test]
    fn combine_important_constrain_wins() {
        let mut base = Style::new();
        base.constrain = Some(Constrain::Inside);
        base.importance.set(StyleProperty::Constrain);
        let mut overlay = Style::new();
        overlay.constrain = Some(Constrain::Inflect);
        let combined = base.combine(&overlay);
        assert_eq!(combined.constrain, Some(Constrain::Inside));
        assert!(combined.importance.get(StyleProperty::Constrain));
    }

    #[test]
    fn border_type_name_round_trip() {
        use BorderType::*;
        let all = [
            Solid, Heavy, Block, Tall, Outer, HKey, VKey, Ascii, Blank, Dashed, Double, Inner,
            Panel, Round, Tab, Thick, Wide,
        ];
        for t in all {
            assert_eq!(
                BorderType::from_name(t.as_edge_type()),
                Some(t),
                "round-trip failed for {:?}",
                t
            );
        }
        // none/hidden/bogus must not resolve to a BorderType
        assert_eq!(BorderType::from_name("none"), None);
        assert_eq!(BorderType::from_name("hidden"), None);
        assert_eq!(BorderType::from_name("bogus"), None);
    }
}
