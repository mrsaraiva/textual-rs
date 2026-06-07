use std::io::{self, Write};

use crossterm::event::{
    DisableFocusChange, DisableMouseCapture, EnableFocusChange, EnableMouseCapture,
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::{cursor, execute, terminal};

#[cfg(feature = "trace")]
use tracing::debug;

use crate::driver::{DriverOptions, KeyboardProtocol, PointerShape, Size};

use super::{CapabilityProfile, PlatformDriver};

#[derive(Default)]
pub(crate) struct WindowsPlatformDriver;

impl PlatformDriver for WindowsPlatformDriver {
    fn start(
        &mut self,
        options: DriverOptions,
        keyboard_protocol: KeyboardProtocol,
    ) -> io::Result<bool> {
        let enable_keyboard = detect_kitty_keyboard_support(keyboard_protocol);

        #[cfg(feature = "trace")]
        debug!(
            enable_mouse = options.enable_mouse,
            enable_pointer_shapes = options.enable_pointer_shapes,
            enable_focus_change = options.enable_focus_change,
            keyboard_protocol = ?keyboard_protocol,
            enable_keyboard,
            "driver.start.windows"
        );

        terminal::enable_raw_mode()?;
        if let Err(err) = execute!(
            std::io::stdout(),
            terminal::EnterAlternateScreen,
            cursor::Hide,
            terminal::DisableLineWrap
        ) {
            restore_terminal_best_effort();
            return Err(err);
        }

        if options.enable_focus_change {
            if let Err(err) = execute!(std::io::stdout(), EnableFocusChange) {
                restore_terminal_best_effort();
                return Err(err);
            }
        }

        if options.enable_mouse {
            if let Err(err) = execute!(std::io::stdout(), EnableMouseCapture) {
                restore_terminal_best_effort();
                return Err(err);
            }
        }

        let keyboard_enhanced = if enable_keyboard {
            execute!(
                std::io::stdout(),
                PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
            )
            .is_ok()
        } else {
            false
        };

        Ok(keyboard_enhanced)
    }

    fn stop(&mut self, options: DriverOptions, keyboard_enhanced: bool) -> io::Result<()> {
        #[cfg(feature = "trace")]
        debug!(keyboard_enhanced = keyboard_enhanced, "driver.stop.windows");

        let mut first_err: Option<io::Error> = None;
        let mut record = |res: io::Result<()>| {
            if let Err(err) = res {
                if first_err.is_none() {
                    first_err = Some(err);
                }
            }
        };

        if keyboard_enhanced {
            record(execute!(std::io::stdout(), PopKeyboardEnhancementFlags));
        }
        if options.enable_mouse {
            record(execute!(std::io::stdout(), DisableMouseCapture));
        }
        if options.enable_focus_change {
            record(execute!(std::io::stdout(), DisableFocusChange));
        }

        record(execute!(
            std::io::stdout(),
            cursor::Show,
            terminal::EnableLineWrap,
            terminal::LeaveAlternateScreen
        ));
        record(terminal::disable_raw_mode());

        if let Some(err) = first_err {
            return Err(err);
        }
        Ok(())
    }

    fn refresh_size(&mut self) -> io::Result<Size> {
        let (width, height) = terminal::size()?;
        #[cfg(feature = "trace")]
        debug!(width, height, "driver.refresh_size.windows");
        Ok(Size { width, height })
    }

    fn reassert_runtime_modes(&mut self, started: bool) -> io::Result<()> {
        if !started {
            return Ok(());
        }
        execute!(std::io::stdout(), terminal::DisableLineWrap, cursor::Hide)
    }

    fn set_pointer_shape(
        &mut self,
        started: bool,
        options: DriverOptions,
        shape: PointerShape,
    ) -> io::Result<()> {
        if !started || !options.enable_pointer_shapes {
            return Ok(());
        }
        #[cfg(feature = "trace")]
        debug!(
            shape = shape.as_kitty_name(),
            "driver.set_pointer_shape.windows"
        );
        let seq = format!("\x1b]22;{}\x07", shape.as_kitty_name());
        let mut out = std::io::stdout();
        out.write_all(seq.as_bytes())?;
        out.flush()?;
        Ok(())
    }
}

pub(crate) fn capability_profile() -> CapabilityProfile {
    CapabilityProfile {
        supports_dim_reliably: false,
        supports_reverse_reliably: false,
        supports_pointer_shapes: detect_pointer_shapes_enabled(),
        supports_kitty_keyboard: detect_kitty_keyboard_support(KeyboardProtocol::Auto),
        supports_focus_change: true,
        requires_mode_reassert_on_resize: true,
    }
}

pub(crate) fn detect_pointer_shapes_enabled() -> bool {
    if let Ok(value) = std::env::var("TEXTUAL_POINTER_SHAPES") {
        let v = value.to_lowercase();
        return !(v == "0" || v == "false" || v == "off" || v == "no");
    }

    // Conservative default on Windows; opt in via TEXTUAL_POINTER_SHAPES=on.
    false
}

pub(crate) fn detect_kitty_keyboard_support(protocol: KeyboardProtocol) -> bool {
    match protocol {
        KeyboardProtocol::Off => false,
        KeyboardProtocol::On => true,
        KeyboardProtocol::Auto => detect_kitty_keyboard_support_auto(),
    }
}

fn detect_kitty_keyboard_support_auto() -> bool {
    if let Ok(value) = std::env::var("TEXTUAL_KEYBOARD_PROTOCOL") {
        let v = value.to_lowercase();
        match v.as_str() {
            "off" | "0" | "false" => return false,
            "on" | "1" | "true" => return true,
            _ => {}
        }
    }
    // Auto mode is probe-first: attempt enable and rely on command success/failure.
    true
}

fn restore_terminal_best_effort() {
    let _ = execute!(
        std::io::stdout(),
        cursor::Show,
        terminal::EnableLineWrap,
        terminal::LeaveAlternateScreen
    );
    let _ = terminal::disable_raw_mode();
}
