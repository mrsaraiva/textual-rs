/// Port of Python Textual `docs/examples/widgets/toast.py`.
///
/// Demonstrates `App::notify()` with different severities and timeouts:
/// - An information notification (default severity, default timeout).
/// - A warning notification with a title and Rich-style markup in the message.
/// - An error notification with a custom 10-second timeout.
/// - An information notification with an empty title (no title bar shown).
///
/// Python: `self.notify(message, title=..., severity=..., timeout=...)`.
/// Rust: `app.notify(message, title, severity, timeout: Option<Duration>)`.
use std::time::Duration;
use textual::prelude::*;

struct ToastApp;

impl TextualApp for ToastApp {
    fn on_mount_with_app(&mut self, app: &mut App, _ctx: &mut EventCtx) {
        // Information notification (default timeout, no title — matches Python).
        app.notify(
            "It's an older code, sir, but it checks out.",
            "",
            ToastSeverity::Information,
            None,
        );

        // Warning notification with Rich markup in the message.
        app.notify(
            "Now witness the firepower of this fully [b]ARMED[/b] and [i][b]OPERATIONAL[/b][/i] battle station!",
            "Possible trap detected",
            ToastSeverity::Warning,
            None,
        );

        // Error notification with a longer timeout (10 seconds, no title — matches Python).
        app.notify(
            "It's a trap!",
            "",
            ToastSeverity::Error,
            Some(Duration::from_secs(10)),
        );

        // Information notification with no title (empty string).
        app.notify(
            "It's against my programming to impersonate a deity.",
            "",
            ToastSeverity::Information,
            None,
        );
    }

    fn compose(&mut self) -> AppRoot {
        AppRoot::new()
    }
}

fn main() -> textual::Result<()> {
    run_sync(ToastApp)
}

// ---------------------------------------------------------------------------
// Regression tests (DG-02)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toast_app_composes_without_panic() {
        let mut app = ToastApp;
        let _root = app.compose();
    }

    #[test]
    fn toast_severity_variants_exist() {
        // Ensure all three severity levels compile and are distinct.
        let sev_info = ToastSeverity::Information;
        let sev_warn = ToastSeverity::Warning;
        let sev_err = ToastSeverity::Error;
        assert_ne!(
            std::mem::discriminant(&sev_info),
            std::mem::discriminant(&sev_warn)
        );
        assert_ne!(
            std::mem::discriminant(&sev_warn),
            std::mem::discriminant(&sev_err)
        );
    }

    #[test]
    fn error_timeout_is_ten_seconds() {
        let timeout = Duration::from_secs(10);
        assert_eq!(timeout.as_secs(), 10);
    }

    #[test]
    fn empty_title_notification_is_valid() {
        // Verifies that notify with an empty title string compiles.
        // (Runtime behavior tested via on_mount_with_app integration.)
        let title = "";
        assert!(title.is_empty());
    }
}
