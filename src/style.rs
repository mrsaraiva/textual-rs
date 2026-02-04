pub use rich_rs::SimpleColor as Color;

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
        m.insert("accent", Color::parse("#ffa62b").unwrap());
        m.insert("warning", Color::parse("#ffa62b").unwrap());
        m.insert("error", Color::parse("#ba3c5b").unwrap());
        m.insert("success", Color::parse("#4EBF71").unwrap());
        m.insert("foreground", Color::parse("#e0e0e0").unwrap());
        // Defaults from `textual/design.py` for dark mode.
        m.insert("background", Color::parse("#121212").unwrap());
        m.insert("surface", Color::parse("#1e1e1e").unwrap());
        // Minimal convenience aliases.
        m.insert("text", Color::parse("#e0e0e0").unwrap());
        m.insert("button-foreground", Color::parse("#e0e0e0").unwrap());
        m.insert("button-color-foreground", Color::parse("#e0e0e0").unwrap());
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
            ShadeKind::Lighten => lighten(color, delta),
            ShadeKind::Darken => darken(color, delta),
        });
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
    match color {
        Color::Rgb { r, g, b } => (r, g, b),
        other => {
            // Convert indexed colors via their palette hex.
            let hex = other.get_hex();
            match Color::parse(&hex) {
                Some(Color::Rgb { r, g, b }) => (r, g, b),
                _ => (255, 255, 255),
            }
        }
    }
}

fn from_rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb { r, g, b }
}

fn blend(a: Color, b: Color, t: f32) -> Color {
    let (ar, ag, ab) = to_rgb(a);
    let (br, bg, bb) = to_rgb(b);
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| -> u8 {
        let xf = x as f32;
        let yf = y as f32;
        (xf + (yf - xf) * t).round().clamp(0.0, 255.0) as u8
    };
    from_rgb(mix(ar, br), mix(ag, bg), mix(ab, bb))
}

fn lighten(color: Color, amount: f32) -> Color {
    blend(color, from_rgb(255, 255, 255), amount)
}

fn darken(color: Color, amount: f32) -> Color {
    blend(color, from_rgb(0, 0, 0), amount)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderType {
    Solid,
    Block,
    Tall,
}

impl BorderType {
    pub fn as_edge_type(self) -> &'static str {
        match self {
            BorderType::Solid => "solid",
            BorderType::Block => "block",
            BorderType::Tall => "tall",
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Style {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: Option<bool>,
    pub dim: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub border: Option<bool>,
    pub margin: Option<Margin>,
    pub line_pad: Option<usize>,
    pub border_top: BorderEdge,
    pub border_right: BorderEdge,
    pub border_bottom: BorderEdge,
    pub border_left: BorderEdge,
    pub width_auto: Option<bool>,
    pub height_auto: Option<bool>,
    pub min_width: Option<usize>,
    pub max_width: Option<usize>,
    pub min_height: Option<usize>,
    pub max_height: Option<usize>,
}

impl Style {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fg(mut self, color: Color) -> Self {
        self.fg = Some(color);
        self
    }

    pub fn bg(mut self, color: Color) -> Self {
        self.bg = Some(color);
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

    pub fn border(mut self, value: bool) -> Self {
        self.border = Some(value);
        self
    }

    pub fn margin(mut self, margin: Margin) -> Self {
        self.margin = Some(margin);
        self
    }

    pub fn line_pad(mut self, value: usize) -> Self {
        self.line_pad = Some(value);
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

    pub fn width(mut self, value: usize) -> Self {
        let value = value.max(1);
        self.width_auto = Some(false);
        self.min_width = Some(value);
        self.max_width = Some(value);
        self
    }

    pub fn height(mut self, value: usize) -> Self {
        let value = value.max(1);
        self.height_auto = Some(false);
        self.min_height = Some(value);
        self.max_height = Some(value);
        self
    }

    pub fn min_width(mut self, value: usize) -> Self {
        self.min_width = Some(value.max(1));
        self
    }

    pub fn max_width(mut self, value: usize) -> Self {
        self.max_width = Some(value.max(1));
        self
    }

    pub fn min_height(mut self, value: usize) -> Self {
        self.min_height = Some(value.max(1));
        self
    }

    pub fn max_height(mut self, value: usize) -> Self {
        self.max_height = Some(value.max(1));
        self
    }

    pub fn combine(&self, other: &Style) -> Style {
        Style {
            fg: other.fg.or(self.fg),
            bg: other.bg.or(self.bg),
            bold: other.bold.or(self.bold),
            dim: other.dim.or(self.dim),
            italic: other.italic.or(self.italic),
            underline: other.underline.or(self.underline),
            border: other.border.or(self.border),
            margin: other.margin.or(self.margin),
            line_pad: other.line_pad.or(self.line_pad),
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
            width_auto: other.width_auto.or(self.width_auto),
            height_auto: other.height_auto.or(self.height_auto),
            min_width: other.min_width.or(self.min_width),
            max_width: other.max_width.or(self.max_width),
            min_height: other.min_height.or(self.min_height),
            max_height: other.max_height.or(self.max_height),
        }
    }

    pub fn inherit_from(&self, parent: &Style) -> Style {
        Style {
            fg: self.fg.or(parent.fg),
            bg: self.bg.or(parent.bg),
            bold: self.bold.or(parent.bold),
            dim: self.dim.or(parent.dim),
            italic: self.italic.or(parent.italic),
            underline: self.underline.or(parent.underline),
            border: self.border.or(parent.border),
            margin: self.margin,
            line_pad: self.line_pad,
            border_top: self.border_top,
            border_right: self.border_right,
            border_bottom: self.border_bottom,
            border_left: self.border_left,
            width_auto: self.width_auto,
            height_auto: self.height_auto,
            min_width: self.min_width,
            max_width: self.max_width,
            min_height: self.min_height,
            max_height: self.max_height,
        }
    }

    pub fn to_rich(&self) -> Option<rich_rs::Style> {
        if self.is_empty() {
            return None;
        }
        let mut style = rich_rs::Style::new();
        if let Some(fg) = self.fg {
            style = style.with_color(fg);
        }
        if let Some(bg) = self.bg {
            style = style.with_bgcolor(bg);
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
        Some(style)
    }

    pub fn is_empty(&self) -> bool {
        self.fg.is_none()
            && self.bg.is_none()
            && self.bold.is_none()
            && self.dim.is_none()
            && self.italic.is_none()
            && self.underline.is_none()
            && self.border.is_none()
            && self.margin.is_none()
            && self.line_pad.is_none()
            && self.border_top == BorderEdge::Unset
            && self.border_right == BorderEdge::Unset
            && self.border_bottom == BorderEdge::Unset
            && self.border_left == BorderEdge::Unset
            && self.width_auto.is_none()
            && self.height_auto.is_none()
            && self.min_width.is_none()
            && self.max_width.is_none()
            && self.min_height.is_none()
            && self.max_height.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Margin {
    pub top: usize,
    pub right: usize,
    pub bottom: usize,
    pub left: usize,
}

impl Margin {
    pub fn all(value: usize) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    pub fn vertical_horizontal(vertical: usize, horizontal: usize) -> Self {
        Self {
            top: vertical,
            bottom: vertical,
            left: horizontal,
            right: horizontal,
        }
    }

    pub fn new(top: usize, right: usize, bottom: usize, left: usize) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }
}

#[derive(Debug, Clone, Copy)]
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
