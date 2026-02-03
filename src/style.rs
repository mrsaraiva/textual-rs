pub use rich_rs::SimpleColor as Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Style {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: Option<bool>,
    pub dim: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub border: Option<bool>,
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

    pub fn combine(&self, other: &Style) -> Style {
        Style {
            fg: other.fg.or(self.fg),
            bg: other.bg.or(self.bg),
            bold: other.bold.or(self.bold),
            dim: other.dim.or(self.dim),
            italic: other.italic.or(self.italic),
            underline: other.underline.or(self.underline),
            border: other.border.or(self.border),
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
    }
}

#[derive(Debug, Clone, Copy, Default)]
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
