//! Tests for the MODES system and CommandPaletteScreen integration.

use textual::screen::{Screen, ScreenStack};
use textual::widgets::{CommandPaletteScreen, PaletteCommand, SystemModalScreen};

use rich_rs::{Console, ConsoleOptions, Segments};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Minimal widget for screen compose output.
struct StubWidget;

impl textual::widgets::Widget for StubWidget {
    fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
        Segments::new()
    }

    fn style_type(&self) -> &'static str {
        "StubWidget"
    }
}

/// A test screen with a configurable name.
struct NamedScreen {
    name: String,
}

impl NamedScreen {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }

    fn boxed(name: &str) -> Box<dyn Screen> {
        Box::new(Self::new(name))
    }
}

impl Screen for NamedScreen {
    fn name(&self) -> &str {
        &self.name
    }

    fn compose(&self) -> Box<dyn textual::widgets::Widget> {
        Box::new(StubWidget)
    }
}

// ---------------------------------------------------------------------------
// Mode registry tests (using ScreenStack directly, since App::new() needs terminal)
// ---------------------------------------------------------------------------

/// Mode factories create screens correctly.
#[test]
fn mode_factory_creates_screen() {
    let factory: Box<dyn Fn() -> Box<dyn Screen>> =
        Box::new(|| NamedScreen::boxed("HelpMode"));

    let screen = factory();
    assert_eq!(screen.name(), "HelpMode");

    // Factory is reusable.
    let screen2 = factory();
    assert_eq!(screen2.name(), "HelpMode");
}

/// Multiple mode factories can coexist in a HashMap (simulating App.modes).
#[test]
fn mode_registry_map() {
    use std::collections::HashMap;

    let mut modes: HashMap<String, Box<dyn Fn() -> Box<dyn Screen>>> = HashMap::new();
    modes.insert(
        "help".to_string(),
        Box::new(|| NamedScreen::boxed("HelpScreen")),
    );
    modes.insert(
        "settings".to_string(),
        Box::new(|| NamedScreen::boxed("SettingsScreen")),
    );

    let help_screen = modes["help"]();
    assert_eq!(help_screen.name(), "HelpScreen");

    let settings_screen = modes["settings"]();
    assert_eq!(settings_screen.name(), "SettingsScreen");
}

/// Mode switch: pop old mode screen, push new one.
#[test]
fn mode_switch_pops_old_and_pushes_new() {
    let mut stack = ScreenStack::new();

    // Simulate switching to mode "help"
    let help_screen = NamedScreen::boxed("HelpScreen");
    stack.push(help_screen);
    assert_eq!(stack.len(), 1);

    // Verify we can pop the help screen
    let (popped, _, _) = stack.pop().unwrap();
    assert_eq!(popped.name(), "HelpScreen");

    // Push settings screen
    let settings_screen = NamedScreen::boxed("SettingsScreen");
    stack.push(settings_screen);
    assert_eq!(stack.len(), 1);

    // Verify settings screen is on top
    let (popped, _, _) = stack.pop().unwrap();
    assert_eq!(popped.name(), "SettingsScreen");
}

/// Mode switch does not affect non-mode screens below.
#[test]
fn mode_switch_preserves_base_screens() {
    let mut stack = ScreenStack::new();

    // Push a base screen (not a mode screen).
    stack.push(NamedScreen::boxed("BaseScreen"));
    assert_eq!(stack.len(), 1);

    // Push a mode screen on top.
    stack.push(NamedScreen::boxed("ModeA"));
    assert_eq!(stack.len(), 2);

    // Switch mode: pop ModeA, push ModeB
    let (popped, _, _) = stack.pop().unwrap();
    assert_eq!(popped.name(), "ModeA");
    stack.push(NamedScreen::boxed("ModeB"));
    assert_eq!(stack.len(), 2);

    // Pop ModeB, base screen should still be there.
    let (popped, _, _) = stack.pop().unwrap();
    assert_eq!(popped.name(), "ModeB");
    assert_eq!(stack.len(), 1);

    let (popped, _, _) = stack.pop().unwrap();
    assert_eq!(popped.name(), "BaseScreen");
}

/// Switching to the same mode is a no-op (tested via current_mode tracking).
#[test]
fn same_mode_noop() {
    let mut current_mode: Option<String> = None;
    let mut stack = ScreenStack::new();

    let mode_name = "help";

    // First switch
    assert_ne!(current_mode.as_deref(), Some(mode_name));
    let screen = NamedScreen::boxed("HelpScreen");
    stack.push(screen);
    current_mode = Some(mode_name.to_string());
    assert_eq!(stack.len(), 1);

    // Second switch to same mode — should be detected as no-op
    assert_eq!(current_mode.as_deref(), Some(mode_name));
    // No push/pop happens.
    assert_eq!(stack.len(), 1);
}

// ---------------------------------------------------------------------------
// CommandPaletteScreen tests
// ---------------------------------------------------------------------------

/// CommandPaletteScreen has correct name.
#[test]
fn command_palette_screen_name() {
    let screen = CommandPaletteScreen::new();
    assert_eq!(screen.name(), "CommandPaletteScreen");
}

/// CommandPaletteScreen is modal.
#[test]
fn command_palette_screen_is_modal() {
    let screen = CommandPaletteScreen::new();
    assert!(screen.is_modal());
}

/// CommandPaletteScreen composes a widget tree.
#[test]
fn command_palette_screen_composes_widget() {
    let screen = CommandPaletteScreen::new();
    let widget = screen.compose();
    assert_eq!(widget.style_type(), "CommandPalette");
}

/// CommandPaletteScreen with custom commands.
#[test]
fn command_palette_screen_with_commands() {
    let commands = vec![
        PaletteCommand::new("test", "Test Command", "Run tests"),
        PaletteCommand::new("deploy", "Deploy", "Deploy to production"),
    ];
    let screen = CommandPaletteScreen::with_commands(commands);
    assert_eq!(screen.name(), "CommandPaletteScreen");

    let widget = screen.compose();
    assert_eq!(widget.style_type(), "CommandPalette");
}

/// CommandPaletteScreen Default impl.
#[test]
fn command_palette_screen_default() {
    let screen = CommandPaletteScreen::default();
    assert_eq!(screen.name(), "CommandPaletteScreen");
}

// ---------------------------------------------------------------------------
// SystemModalScreen trait tests
// ---------------------------------------------------------------------------

/// SystemModalScreen default inherit_css is false.
#[test]
fn system_modal_screen_no_inherit_css() {
    let screen = CommandPaletteScreen::new();
    assert!(!screen.inherit_css());
}

/// CommandPaletteScreen can be pushed to screen stack.
#[test]
fn command_palette_screen_on_stack() {
    let mut stack = ScreenStack::new();
    stack.push(Box::new(CommandPaletteScreen::new()));
    assert_eq!(stack.len(), 1);

    let (popped, _, _) = stack.pop().unwrap();
    assert_eq!(popped.name(), "CommandPaletteScreen");
}

/// CommandPaletteScreen can be pushed and popped.
#[test]
fn command_palette_screen_push_pop() {
    let mut stack = ScreenStack::new();
    stack.push(NamedScreen::boxed("Base"));
    stack.push(Box::new(CommandPaletteScreen::new()));
    assert_eq!(stack.len(), 2);

    let (screen, _result, _) = stack.pop().unwrap();
    assert_eq!(screen.name(), "CommandPaletteScreen");
    assert_eq!(stack.len(), 1);

    let (base, _, _) = stack.pop().unwrap();
    assert_eq!(base.name(), "Base");
}

/// Mode factory for CommandPaletteScreen.
#[test]
fn command_palette_as_mode_factory() {
    use std::collections::HashMap;

    let mut modes: HashMap<String, Box<dyn Fn() -> Box<dyn Screen>>> = HashMap::new();
    modes.insert(
        "command_palette".to_string(),
        Box::new(|| Box::new(CommandPaletteScreen::new())),
    );

    let screen = modes["command_palette"]();
    assert_eq!(screen.name(), "CommandPaletteScreen");
    assert!(screen.is_modal());
}
