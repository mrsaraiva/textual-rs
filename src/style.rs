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
    pub margin: Option<Margin>,
    pub line_pad: Option<usize>,
    pub border_top: Option<Color>,
    pub border_bottom: Option<Color>,
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
        self.border_top = Some(color);
        self
    }

    pub fn border_bottom(mut self, color: Color) -> Self {
        self.border_bottom = Some(color);
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
            border_top: other.border_top.or(self.border_top),
            border_bottom: other.border_bottom.or(self.border_bottom),
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
            border_bottom: self.border_bottom,
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
            && self.border_top.is_none()
            && self.border_bottom.is_none()
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
