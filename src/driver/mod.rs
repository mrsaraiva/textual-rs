//! Terminal driver.
//!
//! `textual-rs` reuses the shared `TerminalDriver` from `richtui-crossterm` to avoid
//! duplicated low-level terminal lifecycle code across repos.
//!
//! A backup of the pre-reuse implementation is kept in `driver::legacy`.

use crate::Result;
use crate::debug::debug_render;

pub mod legacy;

pub use richtui_crossterm::driver::{PointerShape, Size};
use richtui_crossterm::driver::{DriverOptions, TerminalDriver as Inner};

pub struct TerminalDriver {
    inner: Inner,
}

impl TerminalDriver {
    pub fn new() -> Result<Self> {
        let mut options = DriverOptions::default();
        // Preserve textual-rs behavior: mouse capture enabled by default.
        options.enable_mouse = true;
        let inner = Inner::new(options)?;

        let size = inner.size();
        debug_render(&format!("[driver] init size={}x{}", size.width, size.height));
        Ok(Self { inner })
    }

    pub fn size(&self) -> Size {
        self.inner.size()
    }

    pub fn pointer_shapes_enabled(&self) -> bool {
        self.inner.options().enable_pointer_shapes
    }

    pub fn start(&mut self) -> Result<()> {
        debug_render("[driver] start: enter alt screen + hide cursor + enable mouse");
        Ok(self.inner.start()?)
    }

    pub fn stop(&mut self) -> Result<()> {
        debug_render("[driver] stop: leave alt screen + show cursor + disable mouse");
        Ok(self.inner.stop()?)
    }

    pub fn refresh_size(&mut self) -> Result<Size> {
        let old = self.inner.size();
        let size = self.inner.refresh_size()?;
        if old != size {
            debug_render(&format!(
                "[driver] refresh_size: {old_w}x{old_h} -> {width}x{height}",
                old_w = old.width,
                old_h = old.height,
                width = size.width,
                height = size.height
            ));
        }
        Ok(size)
    }

    pub fn set_pointer_shape(&mut self, shape: PointerShape) -> Result<()> {
        Ok(self.inner.set_pointer_shape(shape)?)
    }
}

impl Drop for TerminalDriver {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

