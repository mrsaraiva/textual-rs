use crate::Result;
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
        execute!(
            stdout(),
            terminal::EnterAlternateScreen,
            cursor::Hide,
            EnableMouseCapture
        )?;
        self.started = true;
        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        if !self.started {
            return Ok(());
        }
        execute!(
            stdout(),
            DisableMouseCapture,
            cursor::Show,
            terminal::LeaveAlternateScreen
        )?;
        terminal::disable_raw_mode()?;
        self.started = false;
        Ok(())
    }

    pub fn refresh_size(&mut self) -> Result<Size> {
        let (width, height) = terminal::size()?;
        self.size = Size { width, height };
        Ok(self.size)
    }
}

impl Drop for TerminalDriver {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
