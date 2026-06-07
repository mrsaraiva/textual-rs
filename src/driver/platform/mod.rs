use std::io;

use crate::driver::{DriverOptions, KeyboardProtocol, PointerShape, Size};

#[cfg(not(target_os = "windows"))]
mod posix;
#[cfg(target_os = "windows")]
mod windows;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityProfile {
    pub supports_dim_reliably: bool,
    pub supports_reverse_reliably: bool,
    pub supports_pointer_shapes: bool,
    pub supports_kitty_keyboard: bool,
    pub supports_focus_change: bool,
    pub requires_mode_reassert_on_resize: bool,
}

pub(crate) trait PlatformDriver {
    fn start(
        &mut self,
        options: DriverOptions,
        keyboard_protocol: KeyboardProtocol,
    ) -> io::Result<bool>;

    fn stop(&mut self, options: DriverOptions, keyboard_enhanced: bool) -> io::Result<()>;

    fn refresh_size(&mut self) -> io::Result<Size>;

    fn reassert_runtime_modes(&mut self, started: bool) -> io::Result<()>;

    fn set_pointer_shape(
        &mut self,
        started: bool,
        options: DriverOptions,
        shape: PointerShape,
    ) -> io::Result<()>;
}

pub(crate) fn make_platform_driver() -> Box<dyn PlatformDriver> {
    #[cfg(target_os = "windows")]
    {
        return Box::new(windows::WindowsPlatformDriver);
    }

    #[cfg(not(target_os = "windows"))]
    {
        Box::new(posix::PosixPlatformDriver)
    }
}

pub(crate) fn detect_pointer_shapes_enabled() -> bool {
    #[cfg(target_os = "windows")]
    {
        return windows::detect_pointer_shapes_enabled();
    }

    #[cfg(not(target_os = "windows"))]
    {
        posix::detect_pointer_shapes_enabled()
    }
}

pub(crate) fn capability_profile() -> CapabilityProfile {
    #[cfg(target_os = "windows")]
    {
        return windows::capability_profile();
    }

    #[cfg(not(target_os = "windows"))]
    {
        posix::capability_profile()
    }
}
