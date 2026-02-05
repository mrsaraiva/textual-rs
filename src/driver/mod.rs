//! Terminal driver re-exports.
//!
//! `textual-rs` shares a single terminal lifecycle implementation with `richtui-crossterm`
//! to avoid parallel driver forks across repos.

pub use richtui_crossterm::driver::{
    DriverOptions, KeyboardProtocol, PointerShape, Size, TerminalDriver,
};
