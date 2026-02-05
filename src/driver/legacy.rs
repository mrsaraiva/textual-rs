//! Legacy terminal driver implementation (pre-`richtui-crossterm` reuse).
//!
//! Kept as a backup while `textual-rs` adopts the shared driver from `richtui`.

use crate::Result;
use crate::debug::debug_render;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::{cursor, execute, terminal};
use std::io::stdout;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerShape {
    Default,
    Pointer,
    Text,
    NotAllowed,
}

impl PointerShape {
    pub fn as_kitty_name(self) -> &'static str {
        match self {
            PointerShape::Default => "default",
            PointerShape::Pointer => "pointer",
            PointerShape::Text => "text",
            PointerShape::NotAllowed => "not-allowed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub width: u16,
    pub height: u16,
}

pub struct TerminalDriver {
    size: Size,
    started: bool,
    pointer_shapes_enabled: bool,
}

impl TerminalDriver {
    pub fn new() -> Result<Self> {
        let (width, height) = terminal::size()?;
        debug_render(&format!("[driver] init size={width}x{height}"));
        let pointer_shapes_enabled = detect_pointer_shapes_enabled();
        Ok(Self {
            size: Size { width, height },
            started: false,
            pointer_shapes_enabled,
        })
    }

    pub fn size(&self) -> Size {
        self.size
    }

    pub fn pointer_shapes_enabled(&self) -> bool {
        self.pointer_shapes_enabled
    }

    pub fn start(&mut self) -> Result<()> {
        if self.started {
            return Ok(());
        }
        terminal::enable_raw_mode()?;
        debug_render("[driver] start: enter alt screen + hide cursor + enable mouse");
        execute!(
            stdout(),
            terminal::EnterAlternateScreen,
            cursor::Hide,
            terminal::DisableLineWrap,
            EnableMouseCapture
        )?;
        self.started = true;
        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        if !self.started {
            return Ok(());
        }
        debug_render("[driver] stop: leave alt screen + show cursor + disable mouse");
        execute!(
            stdout(),
            DisableMouseCapture,
            cursor::Show,
            terminal::EnableLineWrap,
            terminal::LeaveAlternateScreen
        )?;
        terminal::disable_raw_mode()?;
        self.started = false;
        Ok(())
    }

    pub fn refresh_size(&mut self) -> Result<Size> {
        let old = self.size;
        let (width, height) = terminal::size()?;
        self.size = Size { width, height };
        if old != self.size {
            debug_render(&format!(
                "[driver] refresh_size: {old_w}x{old_h} -> {width}x{height}",
                old_w = old.width,
                old_h = old.height
            ));
        }
        Ok(self.size)
    }

    /// Set the mouse pointer shape using Kitty pointer-shapes protocol.
    ///
    /// This is best-effort: terminals that don't support it should ignore the OSC sequence.
    ///
    /// Protocol: `ESC ] 22 ; <shape> BEL`
    pub fn set_pointer_shape(&mut self, shape: PointerShape) -> Result<()> {
        if !self.started || !self.pointer_shapes_enabled {
            return Ok(());
        }
        let seq = format!("\x1b]22;{}\x07", shape.as_kitty_name());
        debug_render(&format!(
            "[driver] pointer_shape={} seq={:?}",
            shape.as_kitty_name(),
            seq
        ));
        // Avoid crossterm abstractions here; this is an OSC sequence.
        use std::io::Write;
        let mut out = stdout();
        out.write_all(seq.as_bytes())?;
        out.flush()?;
        Ok(())
    }
}

impl Drop for TerminalDriver {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn detect_pointer_shapes_enabled() -> bool {
    // User override.
    if let Ok(value) = std::env::var("TEXTUAL_POINTER_SHAPES") {
        let v = value.to_lowercase();
        return !(v == "0" || v == "false" || v == "off" || v == "no");
    }

    // Auto-detect: enable for known terminals that implement Kitty protocols.
    // Default to *disabled* for Apple Terminal, which is historically strict about unknown queries.
    // Users can always override with TEXTUAL_POINTER_SHAPES=1.
    if std::env::var("TERM_PROGRAM")
        .ok()
        .as_deref()
        .unwrap_or("")
        == "Apple_Terminal"
    {
        return false;
    }

    let term = std::env::var("TERM").unwrap_or_default().to_lowercase();
    if term.contains("kitty") {
        return true;
    }
    if std::env::var("KITTY_WINDOW_ID").is_ok() {
        return true;
    }
    if std::env::var("WEZTERM_PANE").is_ok() || std::env::var("WEZTERM_UNIX_SOCKET").is_ok() {
        return true;
    }

    // Default: enabled. OSC sequences are commonly ignored safely.
    true
}

