use rich_rs::{Color, Style};

#[derive(Debug, Clone)]
pub struct DebugLayout {
    pub enabled: bool,
    pub show_sizes: bool,
    pub colors: Vec<u8>,
}

impl DebugLayout {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            show_sizes: false,
            colors: vec![196, 202, 208, 214, 220, 82, 45, 27, 129],
        }
    }

    pub fn enabled() -> Self {
        let mut layout = Self::disabled();
        layout.enabled = true;
        layout.show_sizes = true;
        layout
    }

    pub fn style_for(&self, index: usize) -> Style {
        let color = self.colors[index % self.colors.len()];
        Style::color(Color::from_ansi(color).into())
    }
}

impl Default for DebugLayout {
    fn default() -> Self {
        Self::disabled()
    }
}
