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

    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut EventCtx) {
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
    /// DEAD — currently `#[ignore]`d. ROOT: app-level `BINDINGS` are not consulted
    /// while a screen/mode is active. The key dispatch only matches bindings in
    /// `App::active_widget_tree()` (the top *screen* tree — `runtime/mod.rs:1051`),
    /// so the app-root's `s`/`h`/`d` bindings are never in the match chain once a
    /// mode screen covers the app. `current_mode()` stays "dashboard" after `s`.
    /// Python keeps App.BINDINGS in the binding chain below the active screen.
    /// TODO: include app-root bindings in `match_binding_tree` resolution when a
    /// screen is active; then drop `#[ignore]` — this probe flips to LIVE.
    #[ignore = "DEAD: app-level bindings not consulted while a mode screen is active"]
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
            let help = pilot.app().frame_fingerprint();
            assert_ne!(settings, help, "pressing 'h' must switch to Help");

            pilot.press(&["d"])?; // switch_mode('dashboard')
            let back = pilot.app().frame_fingerprint();
            assert_ne!(help, back, "pressing 'd' must switch back to Dashboard");
            assert_eq!(dashboard, back, "Dashboard frame must match the initial mount");
            Ok(())
        })
        .expect("modes01 switch-mode harness should run");
    }
}
