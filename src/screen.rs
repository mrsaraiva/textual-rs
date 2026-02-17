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
use std::fs;

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

    /// Screen title (overrides the app title in the Header widget).
    /// Return `None` to use the app's default title.
    fn title(&self) -> Option<&str> {
        None
    }

    /// Screen sub-title (overrides the app sub-title in the Header widget).
    /// Return `None` to use the app's default sub-title.
    fn sub_title(&self) -> Option<&str> {
        None
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
// Result callback type
// ---------------------------------------------------------------------------

/// Type-erased callback invoked when a screen is dismissed with a result.
pub type ScreenResultCallback = Box<dyn FnOnce(ScreenResult) + Send>;

// ---------------------------------------------------------------------------
// ScreenEntry (internal)
// ---------------------------------------------------------------------------

/// Internal entry in the screen stack.
pub(crate) struct ScreenEntry {
    pub screen: Box<dyn Screen>,
    /// Needs screen switching to swap active tree (no demo uses multiple screens yet).
    #[allow(dead_code)]
    pub widget_tree: WidgetTree,
    /// Per-screen stylesheet; requires screen switching infrastructure.
    #[allow(dead_code)]
    pub stylesheet: Option<StyleSheet>,
    /// Optional callback invoked when this screen is popped.
    result_callback: Option<ScreenResultCallback>,
    /// Pending result set by `dismiss(value)` before the screen is popped.
    pending_result: Option<ScreenResult>,
    /// If this screen was pushed by `switch_mode`, this holds the mode name.
    /// Used to identify the correct screen when switching/removing modes.
    pub(crate) mode_name: Option<String>,
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
    pub fn push(&mut self, screen: Box<dyn Screen>) {
        self.push_inner(screen, None, None);
    }

    /// Push a screen onto the stack with a result callback.
    ///
    /// The callback is invoked with the `ScreenResult` when the screen is
    /// popped (either via `pop()` or via `dismiss()`).
    pub fn push_with_callback(&mut self, screen: Box<dyn Screen>, callback: ScreenResultCallback) {
        self.push_inner(screen, Some(callback), None);
    }

    /// Push a mode screen onto the stack.
    ///
    /// The mode name is stored in the entry so that `pop_mode()` can identify
    /// and remove the correct screen even if transient screens are on top.
    pub fn push_mode(&mut self, screen: Box<dyn Screen>, mode_name: String) {
        self.push_inner(screen, None, Some(mode_name));
    }

    /// Pop the screen associated with the given mode name.
    ///
    /// If the mode screen is not on top (i.e. transient screens are above it),
    /// this pops the mode screen from its position in the stack and calls its
    /// lifecycle hooks. Returns the mode name if found and popped, `None` if
    /// no screen with that mode name exists.
    pub fn pop_mode(&mut self, mode_name: &str) -> Option<String> {
        // Find the entry with the matching mode name.
        let idx = self
            .screens
            .iter()
            .position(|e| e.mode_name.as_deref() == Some(mode_name))?;

        let mut entry = self.screens.remove(idx);
        entry.screen.on_unmount();

        // If we removed the top screen and there's a new top, resume it.
        if idx == self.screens.len() {
            if let Some(new_top) = self.screens.last_mut() {
                new_top.screen.on_resume();
            }
        }

        // Invoke the result callback if one was registered.
        let result = entry.pending_result.unwrap_or(ScreenResult::Dismissed);
        if let Some(callback) = entry.result_callback {
            callback(result);
        }

        entry.mode_name
    }

    /// Return the mode name of the topmost screen (if it has one).
    pub fn top_mode_name(&self) -> Option<&str> {
        self.screens.last().and_then(|e| e.mode_name.as_deref())
    }

    fn push_inner(
        &mut self,
        mut screen: Box<dyn Screen>,
        callback: Option<ScreenResultCallback>,
        mode_name: Option<String>,
    ) {
        // Suspend the currently active screen.
        if let Some(top) = self.screens.last_mut() {
            top.screen.on_suspend();
        }

        // Build the widget tree from the screen's compose output, extracting
        // composed children/declarations into the arena like the app root path.
        let root_widget = screen.compose();
        let mut widget_tree = WidgetTree::new();
        let root_id = widget_tree.set_root(root_widget);
        let (extracted_children, compose_decls) = widget_tree
            .get_mut(root_id)
            .map(|node| (node.widget.take_composed_children(), node.widget.compose()))
            .unwrap_or_default();
        for child in extracted_children {
            crate::runtime::App::mount_extracted_recursive(&mut widget_tree, root_id, child);
        }
        if !compose_decls.is_empty() {
            crate::runtime::App::mount_declarations(&mut widget_tree, root_id, compose_decls);
        }
        // Drain initial lifecycle events (mount events from tree construction).
        let _ = widget_tree.drain_lifecycle();

        // Parse the screen's CSS stylesheet (if provided).
        // Accept either inline CSS text or a filesystem path.
        let stylesheet = screen.css().map(|css| {
            let css_text = fs::read_to_string(css).unwrap_or_else(|_| css.to_string());
            StyleSheet::parse(&css_text)
        });

        // Mount the new screen.
        screen.on_mount();

        self.screens.push(ScreenEntry {
            screen,
            widget_tree,
            stylesheet,
            result_callback: callback,
            pending_result: None,
            mode_name,
        });
    }

    /// Set a pending dismiss result on the topmost screen.
    ///
    /// This is called by the screen itself (via runtime methods) to store a
    /// result value before the screen is popped. When `pop()` is called, the
    /// pending result takes precedence over the default `Dismissed` variant.
    pub fn dismiss(&mut self, result: ScreenResult) -> bool {
        if let Some(top) = self.screens.last_mut() {
            top.pending_result = Some(result);
            true
        } else {
            false
        }
    }

    /// Pop the topmost screen from the stack.
    ///
    /// - Calls `on_unmount` on the popped screen.
    /// - Calls `on_resume` on the new topmost screen (if any).
    /// - If a result callback was registered (via `push_with_callback`), it is
    ///   invoked with the result and the returned `ScreenResult` will be
    ///   `Dismissed` (the callback owns the real value).
    /// - If no callback was registered, the actual `ScreenResult` (pending
    ///   result from `dismiss()`, or `Dismissed` by default) is returned.
    /// - The third tuple element is the mode name of the popped screen (if it
    ///   was a mode screen). Callers should clear `current_mode` when this
    ///   is `Some`.
    ///
    /// Returns `None` if the stack is empty.
    pub fn pop(&mut self) -> Option<(Box<dyn Screen>, ScreenResult, Option<String>)> {
        let mut entry = self.screens.pop()?;
        entry.screen.on_unmount();

        // Resume the screen that is now on top.
        if let Some(new_top) = self.screens.last_mut() {
            new_top.screen.on_resume();
        }

        // Determine the result: use pending_result if set, otherwise Dismissed.
        let result = entry.pending_result.unwrap_or(ScreenResult::Dismissed);

        let mode_name = entry.mode_name;

        // Invoke the result callback if one was registered.
        if let Some(callback) = entry.result_callback {
            callback(result);
            // After callback consumed the result, return Dismissed to caller
            // since the callback already handled it.
            Some((entry.screen, ScreenResult::Dismissed, mode_name))
        } else {
            Some((entry.screen, result, mode_name))
        }
    }

    /// Reference to the topmost screen entry.
    pub(crate) fn top(&self) -> Option<&ScreenEntry> {
        self.screens.last()
    }

    /// Mutable reference to the topmost screen entry.
    ///
    /// Currently unused but part of the public screen API — needed when screen
    /// switching swaps the active widget tree (no demo exercises this yet).
    #[allow(dead_code)]
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

    /// Get the title from the topmost screen (if it defines one).
    pub fn active_title(&self) -> Option<&str> {
        self.top().and_then(|e| e.screen.title())
    }

    /// Get the sub-title from the topmost screen (if it defines one).
    pub fn active_sub_title(&self) -> Option<&str> {
        self.top().and_then(|e| e.screen.sub_title())
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
        screen_title: Option<String>,
        screen_sub_title: Option<String>,
    }

    impl TestScreen {
        fn new(name: &str, log: LifecycleLog) -> Self {
            Self {
                screen_name: name.to_string(),
                log,
                modal: true,
                css_text: None,
                screen_title: None,
                screen_sub_title: None,
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

        fn with_title(mut self, title: &str) -> Self {
            self.screen_title = Some(title.to_string());
            self
        }

        fn with_sub_title(mut self, sub_title: &str) -> Self {
            self.screen_sub_title = Some(sub_title.to_string());
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

        fn title(&self) -> Option<&str> {
            self.screen_title.as_deref()
        }

        fn sub_title(&self) -> Option<&str> {
            self.screen_sub_title.as_deref()
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

        let (screen, screen_result, _mode) = result.unwrap();
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
        assert!(screen.title().is_none());
        assert!(screen.sub_title().is_none());
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

    // =========================================================================
    // P5-04: Screen results with callbacks
    // =========================================================================

    #[test]
    fn push_with_callback_invokes_on_pop() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();
        let callback_log = Arc::new(Mutex::new(Vec::<String>::new()));
        let cb_log = callback_log.clone();

        stack.push_with_callback(
            TestScreen::boxed("Dialog", log.clone()),
            Box::new(move |result| {
                let msg = match result {
                    ScreenResult::Dismissed => "dismissed".to_string(),
                    ScreenResult::Value(v) => {
                        format!("value:{}", v.downcast_ref::<i32>().unwrap())
                    }
                };
                cb_log.lock().unwrap().push(msg);
            }),
        );

        // Pop without dismiss — should get Dismissed.
        stack.pop();
        assert_eq!(callback_log.lock().unwrap().as_slice(), &["dismissed"]);
    }

    #[test]
    fn dismiss_with_value_invokes_callback_with_value() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();
        let callback_log = Arc::new(Mutex::new(Vec::<String>::new()));
        let cb_log = callback_log.clone();

        stack.push_with_callback(
            TestScreen::boxed("Dialog", log.clone()),
            Box::new(move |result| {
                let msg = match result {
                    ScreenResult::Dismissed => "dismissed".to_string(),
                    ScreenResult::Value(v) => {
                        format!("value:{}", v.downcast_ref::<String>().unwrap())
                    }
                };
                cb_log.lock().unwrap().push(msg);
            }),
        );

        // Dismiss with a value, then pop.
        stack.dismiss(ScreenResult::Value(Box::new("confirmed".to_string())));
        stack.pop();
        assert_eq!(
            callback_log.lock().unwrap().as_slice(),
            &["value:confirmed"]
        );
    }

    #[test]
    fn pop_without_callback_returns_pending_result() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        stack.push(TestScreen::boxed("Dialog", log.clone()));
        stack.dismiss(ScreenResult::Value(Box::new(42i32)));

        let (_, result, _) = stack.pop().unwrap();
        match result {
            ScreenResult::Value(v) => assert_eq!(*v.downcast_ref::<i32>().unwrap(), 42),
            ScreenResult::Dismissed => panic!("expected Value"),
        }
    }

    #[test]
    fn dismiss_on_empty_stack_returns_false() {
        let mut stack = ScreenStack::new();
        assert!(!stack.dismiss(ScreenResult::Dismissed));
    }

    #[test]
    fn callback_receives_dismissed_when_no_pending_result() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();
        let received = Arc::new(Mutex::new(false));
        let received_clone = received.clone();

        stack.push_with_callback(
            TestScreen::boxed("X", log.clone()),
            Box::new(move |result| {
                *received_clone.lock().unwrap() = matches!(result, ScreenResult::Dismissed);
            }),
        );

        stack.pop();
        assert!(*received.lock().unwrap());
    }

    // =========================================================================
    // P5-14: Screen title/sub_title
    // =========================================================================

    #[test]
    fn screen_title_default_is_none() {
        let log = LifecycleLog::new();
        let screen = TestScreen::new("test", log);
        assert!(screen.title().is_none());
        assert!(screen.sub_title().is_none());
    }

    #[test]
    fn screen_title_can_be_set() {
        let log = LifecycleLog::new();
        let screen = TestScreen::new("test", log)
            .with_title("My App")
            .with_sub_title("v1.0");
        assert_eq!(screen.title(), Some("My App"));
        assert_eq!(screen.sub_title(), Some("v1.0"));
    }

    #[test]
    fn active_title_from_topmost_screen() {
        let mut stack = ScreenStack::new();
        let log = LifecycleLog::new();

        // No screens — no title.
        assert!(stack.active_title().is_none());
        assert!(stack.active_sub_title().is_none());

        // Push screen without title.
        stack.push(TestScreen::boxed("Base", log.clone()));
        assert!(stack.active_title().is_none());

        // Push screen with title.
        let titled = TestScreen::new("Settings", log.clone())
            .with_title("Settings")
            .with_sub_title("General");
        stack.push(Box::new(titled));
        assert_eq!(stack.active_title(), Some("Settings"));
        assert_eq!(stack.active_sub_title(), Some("General"));

        // Pop titled screen — back to base with no title.
        stack.pop();
        assert!(stack.active_title().is_none());
    }
}
