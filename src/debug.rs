use rich_rs::{Color, Style};
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::OnceLock;

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

pub(crate) fn debug_input(line: &str) {
    static PATH: OnceLock<Option<String>> = OnceLock::new();
    let path = PATH.get_or_init(|| std::env::var("TEXTUAL_DEBUG_INPUT_FILE").ok());
    let Some(path) = path.as_ref() else {
        return;
    };
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

pub(crate) fn debug_layout(line: &str) {
    static PATH: OnceLock<Option<String>> = OnceLock::new();
    let path = PATH.get_or_init(|| std::env::var("TEXTUAL_DEBUG_LAYOUT_FILE").ok());
    let Some(path) = path.as_ref() else {
        return;
    };
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

pub(crate) fn debug_style(line: &str) {
    static PATH: OnceLock<Option<String>> = OnceLock::new();
    let path = PATH.get_or_init(|| std::env::var("TEXTUAL_DEBUG_STYLE_FILE").ok());
    let Some(path) = path.as_ref() else {
        return;
    };
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

pub(crate) fn debug_render(line: &str) {
    static PATH: OnceLock<Option<String>> = OnceLock::new();
    let path = PATH.get_or_init(|| std::env::var("TEXTUAL_DEBUG_RENDER_FILE").ok());
    let Some(path) = path.as_ref() else {
        return;
    };
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

pub(crate) fn timing_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("TEXTUAL_DEBUG_TIMING_FILE").is_ok())
}

pub(crate) fn debug_timing(line: &str) {
    static PATH: OnceLock<Option<String>> = OnceLock::new();
    let path = PATH.get_or_init(|| std::env::var("TEXTUAL_DEBUG_TIMING_FILE").ok());
    let Some(path) = path.as_ref() else {
        return;
    };
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

pub(crate) fn debug_message(line: &str) {
    static PATH: OnceLock<Option<String>> = OnceLock::new();
    let path = PATH.get_or_init(|| std::env::var("TEXTUAL_DEBUG_MESSAGE_FILE").ok());
    let Some(path) = path.as_ref() else {
        return;
    };
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

pub(crate) fn debug_border(line: &str) {
    static PATH: OnceLock<Option<String>> = OnceLock::new();
    let path = PATH.get_or_init(|| std::env::var("TEXTUAL_DEBUG_BORDER_FILE").ok());
    let Some(path) = path.as_ref() else {
        return;
    };
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

pub(crate) fn border_debug_matches(label: &str) -> bool {
    if std::env::var("TEXTUAL_DEBUG_BORDER_FILE").is_err() {
        return false;
    }
    static FILTERS: OnceLock<Vec<String>> = OnceLock::new();
    let filters = FILTERS.get_or_init(|| {
        std::env::var("TEXTUAL_DEBUG_BORDER_FILTER")
            .ok()
            .map(|value| {
                value
                    .split(',')
                    .map(|part| part.trim().to_ascii_lowercase())
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    });
    if filters.is_empty() {
        return true;
    }
    let label = label.to_ascii_lowercase();
    filters.iter().all(|filter| label.contains(filter))
}
