//! Screen system for full-page overlays with independent widget trees.
//!
//! Screens are stacked — only the topmost screen is active (receives events,
//! renders). Screens can return results when popped via `ScreenResult`.
//!
//! Lifecycle:
//! - `on_mount` — called when a screen is pushed and becomes active.
//! - `on_suspend` — called on the previously active screen when a new screen
//!   is pushed on top.
//! - `on_resume` — called when a screen becomes active again after the screen
//!   above it was popped.
//! - `on_unmount` — called when a screen is popped from the stack.

use crate::css::StyleSheet;
use crate::widget_tree::WidgetTree;
use crate::widgets::Widget;

// ---------------------------------------------------------------------------
// Screen trait
// ---------------------------------------------------------------------------

/// A screen is a full-page container that manages its own widget tree.
/// Screens are stacked — only the topmost screen is active (receives events, renders).
pub trait Screen: Send + Sync {
    /// Human-readable name for this screen (used in debug/logging).
    fn name(&self) -> &str {
        "Screen"
    }

    /// The root widget for this screen. Called once when the screen is mounted.
    fn compose(&self) -> Box<dyn Widget>;

    /// CSS stylesheet for this screen (optional).
    fn css(&self) -> Option<&str> {
        None
    }

    /// Called when the screen becomes the active (topmost) screen.
    fn on_mount(&mut self) {}

    /// Called when the screen is no longer the active screen (another pushed on top).
    fn on_suspend(&mut self) {}

    /// Called when the screen becomes active again (screen above was popped).
    fn on_resume(&mut self) {}

    /// Called when this screen is popped from the stack.
    fn on_unmount(&mut self) {}

    /// Whether this screen is modal (blocks interaction with screens below).
    /// Default: true.
    fn is_modal(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// ScreenResult
// ---------------------------------------------------------------------------

/// Result returned when a screen is popped.
pub enum ScreenResult {
    /// Screen was dismissed without a value.
    Dismissed,
    /// Screen returned a value (boxed for type erasure).
    Value(Box<dyn std::any::Any + Send>),
}

// ---------------------------------------------------------------------------
// ScreenEntry (internal)
// ---------------------------------------------------------------------------

/// Internal entry in the screen stack.
pub(crate) struct ScreenEntry {
    pub screen: Box<dyn Screen>,
    pub widget_tree: WidgetTree,
    pub stylesheet: Option<StyleSheet>,
}

// ---------------------------------------------------------------------------
// ScreenStack
// ---------------------------------------------------------------------------

/// Manages the stack of screens.
///
/// The bottom of the stack (index 0) is the first screen pushed; the top
/// (last element) is the currently active screen.
pub struct ScreenStack {
    screens: Vec<ScreenEntry>,
}

impl ScreenStack {
    /// Create an empty screen stack.
    pub fn new() -> Self {
        Self {
            screens: Vec::new(),
        }
    }

    /// Push a screen onto the stack.
    ///
    /// - Calls `on_suspend` on the previously active screen (if any).
    /// - Builds the widget tree from `screen.compose()`.
    /// - Parses the screen's CSS (if any).
    /// - Calls `on_mount` on the new screen.
    pub fn push(&mut self, mut screen: Box<dyn Screen>) {
        // Suspend the currently active screen.
        if let Some(top) = self.screens.last_mut() {
            top.screen.on_suspend();
        }

        // Build the widget tree from the screen's compose output.
        let root_widget = screen.compose();
        let mut widget_tree = WidgetTree::new();
        widget_tree.set_root(root_widget);
        // Drain initial lifecycle events (mount events from tree construction).
        let _ = widget_tree.drain_lifecycle();

        // Parse the screen's CSS stylesheet (if provided).
        let stylesheet = screen.css().map(StyleSheet::parse);

        // Mount the new screen.
        screen.on_mount();

        self.screens.push(ScreenEntry {
            screen,
            widget_tree,
            stylesheet,
        });
    }

    /// Pop the topmost screen from the stack.
    ///
    /// - Calls `on_unmount` on the popped screen.
    /// - Calls `on_resume` on the new topmost screen (if any).
    /// - Returns the popped screen and a `ScreenResult::Dismissed`.
    ///
    /// Returns `None` if the stack is empty.
    pub fn pop(&mut self) -> Option<(Box<dyn Screen>, ScreenResult)> {
        let mut entry = self.screens.pop()?;
        entry.screen.on_unmount();

        // Resume the screen that is now on top.
        if let Some(new_top) = self.screens.last_mut() {
            new_top.screen.on_resume();
        }

        Some((entry.screen, ScreenResult::Dismissed))
    }

    /// Reference to the topmost screen entry.
    pub(crate) fn top(&self) -> Option<&ScreenEntry> {
        self.screens.last()
    }

    /// Mutable reference to the topmost screen entry.
    pub(crate) fn top_mut(&mut self) -> Option<&mut ScreenEntry> {
        self.screens.last_mut()
    }

    /// Number of screens on the stack.
    pub fn len(&self) -> usize {
        self.screens.len()
    }

    /// Whether the stack is empty.
    pub fn is_empty(&self) -> bool {
        self.screens.is_empty()
    }
}

impl Default for ScreenStack {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rich_rs::{Console, ConsoleOptions, Segments};
    use std::sync::{Arc, Mutex};

    // -- Test helpers --------------------------------------------------------

    /// Tracks lifecycle calls in order for verification.
    #[derive(Debug, Clone, Default)]
    struct LifecycleLog {
        events: Arc<Mutex<Vec<String>>>,
    }

    impl LifecycleLog {
        fn new() -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn log(&self, event: &str) {
            self.events.lock().unwrap().push(event.to_string());
        }

        fn events(&self) -> Vec<String> {
            self.events.lock().unwrap().clone()
        }
    }

    /// Minimal widget for screen compose output.
    struct StubWidget;

    impl Widget for StubWidget {
        fn render(&self, _console: &Console, _options: &ConsoleOptions) -> Segments {
            Segments::new()
        }

        fn style_type(&self) -> &'static str {
            "StubWidget"
        }
    }

    /// A test screen that logs lifecycle events.
    struct TestScreen {
        screen_name: String,
        log: LifecycleLog,
        modal: bool,
        css_text: Option<String>,
    }

    impl TestScreen {
        fn new(name: &str, log: LifecycleLog) -> Self {
            Self {
                screen_name: name.to_string(),
                log,
                modal: true,
                css_text: None,
            }
        }

        fn with_modal(mut self, modal: bool) -> Self {
            self.modal = modal;
            self
        }

        fn with_css(mut self, css: &str) -> Self {
            self.css_text = Some(css.to_string());
            self
        }

        fn boxed(name: &str, log: LifecycleLog) -> Box<dyn Screen> {
            Box::new(Self::new(name, log))
        }
    }

    impl Screen for TestScreen {
        fn name(&self) -> &str {
            &self.screen_name
        }

        fn compose(&self) -> Box<dyn Widget> {
            Box::new(StubWidget)
        }

        fn css(&self) -> Option<&str> {
            self.css_text.as_deref()
        }

        fn on_mount(&mut self) {
            self.log.log(&format!("{}:mount", self.screen_name));
        }

        fn on_suspend(&mut self) {
            self.log.log(&format!("{}:suspend", self.screen_name));
        }

        fn on_resume(&mut self) {
            self.log.log(&format!("{}:resume", self.screen_name));
        }

        fn on_unmount(&mut self) {
            self.log.log(&format!("{}:unmount", self.screen_name));
        }

        fn is_modal(&self) -> bool {
            self.modal
        }
    }

    // -- ScreenStack: new is empty -------------------------------------------

    #[test]
    fn new_stack_is_empty() {
        let stack = ScreenStack::new();
        assert!(stack.is_empty());
        assert_eq!(stack.len(), 0);
        assert!(stack.top().is_none());
    }

    // -- ScreenStack: push increases len -------------------------------------

    #[test]
    fn push_increases_len() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        stack.push(TestScreen::boxed("A", log.clone()));
        assert_eq!(stack.len(), 1);
        assert!(!stack.is_empty());

        stack.push(TestScreen::boxed("B", log.clone()));
        assert_eq!(stack.len(), 2);

        stack.push(TestScreen::boxed("C", log.clone()));
        assert_eq!(stack.len(), 3);
    }

    // -- ScreenStack: pop returns screen + calls lifecycle -------------------

    #[test]
    fn pop_returns_screen_and_dismissed_result() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        stack.push(TestScreen::boxed("Main", log.clone()));
        let result = stack.pop();
        assert!(result.is_some());

        let (screen, screen_result) = result.unwrap();
        assert_eq!(screen.name(), "Main");
        assert!(matches!(screen_result, ScreenResult::Dismissed));
        assert!(stack.is_empty());
    }

    // -- ScreenStack: push calls on_suspend on previous, on_mount on new ----

    #[test]
    fn push_calls_suspend_on_previous_and_mount_on_new() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        stack.push(TestScreen::boxed("First", log.clone()));
        assert_eq!(log.events(), vec!["First:mount"]);

        stack.push(TestScreen::boxed("Second", log.clone()));
        assert_eq!(
            log.events(),
            vec!["First:mount", "First:suspend", "Second:mount"]
        );
    }

    // -- ScreenStack: pop calls on_unmount on popped, on_resume on new top --

    #[test]
    fn pop_calls_unmount_on_popped_and_resume_on_new_top() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        stack.push(TestScreen::boxed("Base", log.clone()));
        stack.push(TestScreen::boxed("Overlay", log.clone()));

        // Clear log to focus on pop behavior.
        log.events.lock().unwrap().clear();

        stack.pop();
        assert_eq!(log.events(), vec!["Overlay:unmount", "Base:resume"]);
    }

    // -- ScreenStack: pop on empty returns None ------------------------------

    #[test]
    fn pop_on_empty_returns_none() {
        let mut stack = ScreenStack::new();
        assert!(stack.pop().is_none());
    }

    // -- ScreenStack: top returns topmost ------------------------------------

    #[test]
    fn top_returns_topmost() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        stack.push(TestScreen::boxed("Bottom", log.clone()));
        stack.push(TestScreen::boxed("Top", log.clone()));

        assert_eq!(stack.top().unwrap().screen.name(), "Top");
    }

    #[test]
    fn top_mut_returns_topmost() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        stack.push(TestScreen::boxed("Bottom", log.clone()));
        stack.push(TestScreen::boxed("Top", log.clone()));

        assert_eq!(stack.top_mut().unwrap().screen.name(), "Top");
    }

    // -- ScreenResult: Dismissed and Value variants --------------------------

    #[test]
    fn screen_result_dismissed() {
        let result = ScreenResult::Dismissed;
        assert!(matches!(result, ScreenResult::Dismissed));
    }

    #[test]
    fn screen_result_value() {
        let result = ScreenResult::Value(Box::new(42i32));
        match result {
            ScreenResult::Value(val) => {
                let num = val.downcast_ref::<i32>().unwrap();
                assert_eq!(*num, 42);
            }
            _ => panic!("expected Value variant"),
        }
    }

    #[test]
    fn screen_result_value_string() {
        let result = ScreenResult::Value(Box::new("hello".to_string()));
        match result {
            ScreenResult::Value(val) => {
                let s = val.downcast_ref::<String>().unwrap();
                assert_eq!(s, "hello");
            }
            _ => panic!("expected Value variant"),
        }
    }

    // -- Modal default is true -----------------------------------------------

    #[test]
    fn modal_default_is_true() {
        let log = LifecycleLog::new();
        let screen = TestScreen::new("test", log);
        assert!(screen.is_modal());
    }

    #[test]
    fn modal_can_be_overridden() {
        let log = LifecycleLog::new();
        let screen = TestScreen::new("test", log).with_modal(false);
        assert!(!screen.is_modal());
    }

    // -- Screen lifecycle ordering -------------------------------------------

    #[test]
    fn full_lifecycle_ordering() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        // Push screen A.
        stack.push(TestScreen::boxed("A", log.clone()));
        // Push screen B (suspends A).
        stack.push(TestScreen::boxed("B", log.clone()));
        // Push screen C (suspends B).
        stack.push(TestScreen::boxed("C", log.clone()));

        // Pop C (unmounts C, resumes B).
        stack.pop();
        // Pop B (unmounts B, resumes A).
        stack.pop();
        // Pop A (unmounts A, no resume).
        stack.pop();

        assert_eq!(
            log.events(),
            vec![
                "A:mount",
                "A:suspend",
                "B:mount",
                "B:suspend",
                "C:mount",
                "C:unmount",
                "B:resume",
                "B:unmount",
                "A:resume",
                "A:unmount",
            ]
        );
    }

    // -- Widget tree is built from compose -----------------------------------

    #[test]
    fn push_builds_widget_tree() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        stack.push(TestScreen::boxed("test", log));

        let entry = stack.top().unwrap();
        // The widget tree should have a root node (from compose).
        assert!(entry.widget_tree.root().is_some());
        assert_eq!(entry.widget_tree.len(), 1);
    }

    // -- CSS stylesheet is parsed from css() --------------------------------

    #[test]
    fn push_parses_css_stylesheet() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        let screen = TestScreen::new("styled", log).with_css("Button { color: red; }");
        stack.push(Box::new(screen));

        let entry = stack.top().unwrap();
        assert!(entry.stylesheet.is_some());
    }

    #[test]
    fn push_no_css_gives_none_stylesheet() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        stack.push(TestScreen::boxed("plain", log));

        let entry = stack.top().unwrap();
        assert!(entry.stylesheet.is_none());
    }

    // -- Default trait method coverage ---------------------------------------

    #[test]
    fn screen_default_name() {
        /// Minimal screen impl that only provides compose.
        struct MinimalScreen;

        impl Screen for MinimalScreen {
            fn compose(&self) -> Box<dyn Widget> {
                Box::new(StubWidget)
            }
        }

        let screen = MinimalScreen;
        assert_eq!(screen.name(), "Screen");
        assert!(screen.css().is_none());
        assert!(screen.is_modal());
    }

    // -- Pop last screen has no resume target --------------------------------

    #[test]
    fn pop_single_screen_no_resume_called() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        stack.push(TestScreen::boxed("Only", log.clone()));
        log.events.lock().unwrap().clear();

        stack.pop();
        // Only unmount, no resume (nothing below).
        assert_eq!(log.events(), vec!["Only:unmount"]);
    }

    // -- Multiple pushes without pops ----------------------------------------

    #[test]
    fn multiple_pushes_suspend_chain() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        stack.push(TestScreen::boxed("A", log.clone()));
        stack.push(TestScreen::boxed("B", log.clone()));
        stack.push(TestScreen::boxed("C", log.clone()));
        stack.push(TestScreen::boxed("D", log.clone()));

        assert_eq!(
            log.events(),
            vec![
                "A:mount",
                "A:suspend",
                "B:mount",
                "B:suspend",
                "C:mount",
                "C:suspend",
                "D:mount",
            ]
        );
        assert_eq!(stack.len(), 4);
    }
}
