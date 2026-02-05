use crate::Result;
use crate::debug::debug_render;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::{cursor, execute, terminal};
use std::io::stdout;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub width: u16,
    pub height: u16,
}

pub struct TerminalDriver {
    size: Size,
    started: bool,
}

impl TerminalDriver {
    pub fn new() -> Result<Self> {
        let (width, height) = terminal::size()?;
        debug_render(&format!("[driver] init size={width}x{height}"));
        Ok(Self {
            size: Size { width, height },
            started: false,
        })
    }

    pub fn size(&self) -> Size {
        self.size
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
}

impl Drop for TerminalDriver {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
