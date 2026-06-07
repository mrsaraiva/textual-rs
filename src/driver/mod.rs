//! Terminal driver: terminal lifecycle, capability detection, keyboard-protocol and
//! pointer-shape control built directly on top of [`crossterm`].
//!
//! This is `textual-rs`'s own self-contained crossterm driver (no external backend crate).

use std::io;

mod platform;

pub use platform::CapabilityProfile;

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

/// Kitty keyboard protocol mode.
///
/// Controls whether the terminal reports enhanced key events that disambiguate
/// keys like Tab vs Ctrl+I, Enter vs Ctrl+M, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyboardProtocol {
    /// Do not enable keyboard enhancement (legacy mode).
    #[default]
    Off,
    /// Auto-detect: enable on terminals known to support Kitty protocol.
    Auto,
    /// Force enable keyboard enhancement.
    On,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriverOptions {
    pub enable_mouse: bool,
    pub enable_pointer_shapes: bool,
    pub enable_focus_change: bool,
    pub keyboard_protocol: KeyboardProtocol,
}

impl Default for DriverOptions {
    fn default() -> Self {
        Self {
            enable_mouse: false,
            enable_pointer_shapes: detect_pointer_shapes_enabled(),
            enable_focus_change: false,
            keyboard_protocol: KeyboardProtocol::Off,
        }
    }
}

pub struct TerminalDriver {
    size: Size,
    started: bool,
    options: DriverOptions,
    keyboard_enhanced: bool,
    capabilities: CapabilityProfile,
    platform: Box<dyn platform::PlatformDriver>,
}

impl TerminalDriver {
    pub fn new(options: DriverOptions) -> io::Result<Self> {
        let mut platform = platform::make_platform_driver();
        let size = platform.refresh_size()?;
        Ok(Self {
            size,
            started: false,
            options,
            keyboard_enhanced: false,
            capabilities: platform::capability_profile(),
            platform,
        })
    }

    pub fn size(&self) -> Size {
        self.size
    }

    pub fn started(&self) -> bool {
        self.started
    }

    pub fn options(&self) -> DriverOptions {
        self.options
    }

    /// Terminal capability profile for the active platform driver.
    pub fn capabilities(&self) -> CapabilityProfile {
        self.capabilities
    }

    /// Whether the Kitty keyboard enhancement protocol is currently active.
    pub fn keyboard_enhanced(&self) -> bool {
        self.keyboard_enhanced
    }

    pub fn start(&mut self) -> io::Result<()> {
        if self.started {
            return Ok(());
        }
        self.keyboard_enhanced = self
            .platform
            .start(self.options, self.options.keyboard_protocol)?;
        self.started = true;
        Ok(())
    }

    pub fn stop(&mut self) -> io::Result<()> {
        if !self.started {
            return Ok(());
        }
        let result = self.platform.stop(self.options, self.keyboard_enhanced);
        self.keyboard_enhanced = false;
        self.started = false;
        result
    }

    pub fn refresh_size(&mut self) -> io::Result<Size> {
        self.size = self.platform.refresh_size()?;
        Ok(self.size)
    }

    /// Re-apply runtime modes that some terminals may reset on resize.
    pub fn reassert_runtime_modes(&mut self) -> io::Result<()> {
        self.platform.reassert_runtime_modes(self.started)
    }

    /// Set the mouse pointer shape using Kitty pointer-shapes protocol.
    ///
    /// Best effort: terminals that don't support it should ignore the OSC sequence.
    ///
    /// Protocol: `ESC ] 22 ; <shape> BEL`
    pub fn set_pointer_shape(&mut self, shape: PointerShape) -> io::Result<()> {
        if !self.capabilities.supports_pointer_shapes {
            return Ok(());
        }
        self.platform
            .set_pointer_shape(self.started, self.options, shape)
    }
}

impl Drop for TerminalDriver {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn detect_pointer_shapes_enabled() -> bool {
    platform::detect_pointer_shapes_enabled()
}

#[cfg(test)]
mod tests {
    use super::platform;

    // NOTE: the upstream richtui-crossterm driver also had tests for the
    // TEXTUAL_POINTER_SHAPES env-var override. They were dropped during the port into
    // textual-rs because they mutate process env via `std::env::set_var`/`remove_var`,
    // which is `unsafe` in edition 2024, and this crate sets `unsafe_code = "forbid"`.

    #[test]
    fn capability_profile_has_required_flags() {
        let profile = platform::capability_profile();
        assert!(profile.requires_mode_reassert_on_resize);
        assert!(profile.supports_focus_change);

        #[cfg(not(target_os = "windows"))]
        {
            assert!(profile.supports_dim_reliably);
            assert!(profile.supports_reverse_reliably);
        }
    }
}
