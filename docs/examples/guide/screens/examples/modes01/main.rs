use textual::compose;
use textual::prelude::*;

// ---------------------------------------------------------------------------
// Dashboard Screen
// ---------------------------------------------------------------------------

struct DashboardScreen;

impl Screen for DashboardScreen {
    fn name(&self) -> &str {
        "DashboardScreen"
    }

    fn compose(&self) -> Box<dyn Widget> {
        Box::new(
            Vertical::new().with_compose(compose![
                Placeholder::new("Dashboard Screen"),
                Footer::new(),
            ]),
        )
    }
}

// ---------------------------------------------------------------------------
// Settings Screen
// ---------------------------------------------------------------------------

struct SettingsScreen;

impl Screen for SettingsScreen {
    fn name(&self) -> &str {
        "SettingsScreen"
    }

    fn compose(&self) -> Box<dyn Widget> {
        Box::new(
            Vertical::new().with_compose(compose![
                Placeholder::new("Settings Screen"),
                Footer::new(),
            ]),
        )
    }
}

// ---------------------------------------------------------------------------
// Help Screen
// ---------------------------------------------------------------------------

struct HelpScreen;

impl Screen for HelpScreen {
    fn name(&self) -> &str {
        "HelpScreen"
    }

    fn compose(&self) -> Box<dyn Widget> {
        Box::new(
            Vertical::new().with_compose(compose![
                Placeholder::new("Help Screen"),
                Footer::new(),
            ]),
        )
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

struct ModesApp;

impl TextualApp for ModesApp {
    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("d", "app.switch_mode('dashboard')", "Dashboard"),
            BindingDecl::new("s", "app.switch_mode('settings')", "Settings"),
            BindingDecl::new("h", "app.switch_mode('help')", "Help"),
        ]
    }

    fn configure(&mut self, app: &mut App) -> Result<()> {
        app.add_mode("dashboard", || Box::new(DashboardScreen));
        app.add_mode("settings", || Box::new(SettingsScreen));
        app.add_mode("help", || Box::new(HelpScreen));
        Ok(())
    }

    fn compose(&mut self) -> AppRoot {
        // The app itself has no direct content — all content is rendered
        // through the active mode screen.
        AppRoot::new()
    }

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut textual::event::WidgetCtx) {
        app.switch_mode("dashboard");
    }
}

fn main() -> Result<()> {
    run_sync(ModesApp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes01_registers_three_modes() {
        let mut definition = ModesApp;
        let mut app = App::new().expect("app should initialize");
        definition
            .configure(&mut app)
            .expect("modes01 configure should succeed");

        // Verify each mode can be switched to.
        assert!(app.switch_mode("dashboard"), "dashboard mode should exist");
        assert!(app.switch_mode("settings"), "settings mode should exist");
        assert!(app.switch_mode("help"), "help mode should exist");
    }

    #[test]
    fn modes01_unknown_mode_returns_false() {
        let mut definition = ModesApp;
        let mut app = App::new().expect("app should initialize");
        definition
            .configure(&mut app)
            .expect("modes01 configure should succeed");

        assert!(!app.switch_mode("nonexistent"), "unknown mode should return false");
    }

    #[test]
    fn modes01_screen_names() {
        assert_eq!(DashboardScreen.name(), "DashboardScreen");
        assert_eq!(SettingsScreen.name(), "SettingsScreen");
        assert_eq!(HelpScreen.name(), "HelpScreen");
    }

    #[test]
    fn modes01_screens_are_modal_by_default() {
        // Mode screens are treated as full screens (modal = true by default).
        assert!(DashboardScreen.is_modal());
        assert!(SettingsScreen.is_modal());
        assert!(HelpScreen.is_modal());
    }

    /// LIVENESS probe (Pilot, headless): on mount the Dashboard mode is active;
    /// pressing the bound `s` / `h` keys switches modes (`app.switch_mode(...)`)
    /// and the rendered frame changes each time (different placeholder content).
    /// Guards that mode-switch key bindings actually swap the active screen.
    ///
    /// LIVE: app-level `BINDINGS` are consulted while a mode screen is active.
    /// `match_binding_chain` (`runtime/routing.rs`) walks the active screen tree
    /// *and* the app-root tree (`App::app_root_tree_when_screen_active`), so the
    /// app's `s`/`h`/`d` bindings stay in the chain beneath the active mode
    /// screen — matching Python `App._check_bindings`, which always appends
    /// `App._bindings` after the screen chain.
    ///
    /// Asserts each bound key actually swaps the active mode (`current_mode()`)
    /// and changes the rendered frame. Note: re-entering a mode does NOT
    /// reproduce the original frame byte-for-byte — `Placeholder` assigns colors
    /// from a process-global counter on each construction, and `switch_mode`
    /// rebuilds the screen, so the dashboard's placeholder color advances. That
    /// is faithful to Python (its `Placeholder` cycles colors globally too) and
    /// is independent of binding resolution; the probe therefore checks mode +
    /// frame-change, not round-trip frame equality.
    #[test]
    fn modes01_switch_mode_is_live() {
        run_test(ModesApp, |pilot| {
            let dashboard = pilot.app().frame_fingerprint();
            assert_eq!(pilot.app().current_mode(), Some("dashboard"));

            pilot.press(&["s"])?; // switch_mode('settings')
            assert_eq!(pilot.app().current_mode(), Some("settings"), "s must switch mode to settings");
            let settings = pilot.app().frame_fingerprint();
            assert_ne!(dashboard, settings, "pressing 's' must switch to Settings");

            pilot.press(&["h"])?; // switch_mode('help')
            assert_eq!(pilot.app().current_mode(), Some("help"), "h must switch mode to help");
            let help = pilot.app().frame_fingerprint();
            assert_ne!(settings, help, "pressing 'h' must switch to Help");

            pilot.press(&["d"])?; // switch_mode('dashboard')
            assert_eq!(pilot.app().current_mode(), Some("dashboard"), "d must switch mode back to dashboard");
            let back = pilot.app().frame_fingerprint();
            assert_ne!(help, back, "pressing 'd' must switch back to Dashboard");
            Ok(())
        })
        .expect("modes01 switch-mode harness should run");
    }
}
