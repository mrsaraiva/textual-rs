//! Action system foundation: types, traits, parser, and built-in declarations.
//!
//! This module implements the Python Textual–style string action system where
//! widgets declare actions they handle and keybindings/buttons invoke actions by
//! name (e.g. `"toggle_dark"`, `"app.quit"`, `"push_screen('settings')"`).
//!
//! **This is types + parsing only.** Runtime wiring (dispatch, bubble resolution)
//! is planned for P4-08.

use crate::event::EventCtx;

// ── Core types ───────────────────────────────────────────────────────────────

/// Static declaration of an action that a widget or app can handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionDecl {
    /// Action name, e.g. `"toggle_dark"`.
    pub name: &'static str,
    /// Namespace that owns the action, e.g. `"app"`, `"button"`.
    pub namespace: &'static str,
    /// Human-readable description shown in help / command palette.
    pub description: &'static str,
    /// Optional default key binding string, e.g. `"ctrl+d"`.
    pub default_binding: Option<&'static str>,
}

/// A parsed action string ready for dispatch.
///
/// Produced by [`parse_action`] from strings like `"app.quit"` or
/// `"push_screen('settings')"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedAction {
    /// Namespace prefix, if explicitly provided.  `None` means "resolve via
    /// bubble" at dispatch time.
    pub namespace: Option<String>,
    /// The action name, e.g. `"toggle_dark"`.
    pub name: String,
    /// Positional arguments extracted from parenthesised suffix.
    pub arguments: Vec<String>,
}

// ── ActionHandler trait ──────────────────────────────────────────────────────

/// Trait for widgets/apps that handle string-based actions.
///
/// Implementors declare which actions they support via [`action_registry`] and
/// execute them in [`execute_action`].
pub trait ActionHandler {
    /// Return the list of actions this widget/app handles.
    fn action_registry(&self) -> &[ActionDecl] {
        &[]
    }

    /// Execute a parsed action. Returns `true` if the action was handled.
    fn execute_action(&mut self, _action: &ParsedAction, _ctx: &mut EventCtx) -> bool {
        false
    }
}

// ── Built-in app action declarations ─────────────────────────────────────────

/// Standard application-level actions mirroring Python Textual's built-in set.
pub const APP_ACTIONS: &[ActionDecl] = &[
    ActionDecl {
        name: "quit",
        namespace: "app",
        description: "Quit the application",
        default_binding: Some("ctrl+q"),
    },
    ActionDecl {
        name: "toggle_dark",
        namespace: "app",
        description: "Toggle dark mode",
        default_binding: Some("ctrl+d"),
    },
    ActionDecl {
        name: "bell",
        namespace: "app",
        description: "Ring the terminal bell",
        default_binding: None,
    },
    ActionDecl {
        name: "push_screen",
        namespace: "app",
        description: "Push a screen",
        default_binding: None,
    },
    ActionDecl {
        name: "pop_screen",
        namespace: "app",
        description: "Pop the current screen",
        default_binding: None,
    },
    ActionDecl {
        name: "focus",
        namespace: "app",
        description: "Focus a widget by ID",
        default_binding: None,
    },
    ActionDecl {
        name: "focus_next",
        namespace: "app",
        description: "Focus next widget",
        default_binding: Some("tab"),
    },
    ActionDecl {
        name: "focus_previous",
        namespace: "app",
        description: "Focus previous widget",
        default_binding: Some("shift+tab"),
    },
];

// ── Parser ───────────────────────────────────────────────────────────────────

/// Parse an action string into a [`ParsedAction`].
///
/// Accepted formats (mirroring Python Textual):
///
/// | Input                          | Namespace        | Name            | Arguments        |
/// |--------------------------------|------------------|-----------------|------------------|
/// | `"toggle_dark"`                | `None`           | `"toggle_dark"` | `[]`             |
/// | `"app.quit"`                   | `Some("app")`    | `"quit"`        | `[]`             |
/// | `"push_screen('settings')"`    | `None`           | `"push_screen"` | `["settings"]`   |
/// | `"app.bell"`                   | `Some("app")`    | `"bell"`        | `[]`             |
/// | `"focus('sidebar')"`           | `None`           | `"focus"`       | `["sidebar"]`    |
///
/// Returns `None` for empty/whitespace-only strings or strings that contain no
/// valid action name.
pub fn parse_action(input: &str) -> Option<ParsedAction> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    // Split off arguments in parentheses, if any.
    let (before_paren, arguments) = if let Some(paren_start) = input.find('(') {
        // Must end with ')'
        if !input.ends_with(')') {
            return None;
        }
        let args_str = &input[paren_start + 1..input.len() - 1];
        let args = parse_arguments(args_str)?;
        (&input[..paren_start], args)
    } else {
        (input, Vec::new())
    };

    let before_paren = before_paren.trim();
    if before_paren.is_empty() {
        return None;
    }

    // Split namespace from name on the first dot.
    let (namespace, name) = if let Some(dot_pos) = before_paren.find('.') {
        let ns = &before_paren[..dot_pos];
        let n = &before_paren[dot_pos + 1..];
        if ns.is_empty() || n.is_empty() {
            return None;
        }
        (Some(ns.to_string()), n.to_string())
    } else {
        (None, before_paren.to_string())
    };

    Some(ParsedAction {
        namespace,
        name,
        arguments,
    })
}

/// Parse a comma-separated argument list, stripping surrounding quotes from
/// each argument. Returns `None` if the input contains malformed syntax (e.g.
/// unclosed quotes).
fn parse_arguments(input: &str) -> Option<Vec<String>> {
    let input = input.trim();
    if input.is_empty() {
        return Some(Vec::new());
    }

    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quote: Option<char> = None;

    for ch in input.chars() {
        match in_quote {
            Some(q) if ch == q => {
                // Closing quote — don't include the quote character itself.
                in_quote = None;
            }
            Some(_) => {
                // Inside a quoted string.
                current.push(ch);
            }
            None if ch == '\'' || ch == '"' => {
                // Opening quote.
                in_quote = Some(ch);
            }
            None if ch == ',' => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    args.push(trimmed);
                }
                current.clear();
            }
            None => {
                current.push(ch);
            }
        }
    }

    // Reject unclosed quotes.
    if in_quote.is_some() {
        return None;
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        args.push(trimmed);
    }

    Some(args)
}

/// Look up an [`ActionDecl`] by name within a registry slice.
pub fn find_action<'a>(registry: &'a [ActionDecl], name: &str) -> Option<&'a ActionDecl> {
    registry.iter().find(|a| a.name == name)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_action: basic cases ────────────────────────────────────────

    #[test]
    fn parse_simple_action() {
        let parsed = parse_action("toggle_dark").unwrap();
        assert_eq!(parsed.namespace, None);
        assert_eq!(parsed.name, "toggle_dark");
        assert!(parsed.arguments.is_empty());
    }

    #[test]
    fn parse_namespaced_action() {
        let parsed = parse_action("app.quit").unwrap();
        assert_eq!(parsed.namespace, Some("app".to_string()));
        assert_eq!(parsed.name, "quit");
        assert!(parsed.arguments.is_empty());
    }

    #[test]
    fn parse_action_with_single_arg() {
        let parsed = parse_action("push_screen('settings')").unwrap();
        assert_eq!(parsed.namespace, None);
        assert_eq!(parsed.name, "push_screen");
        assert_eq!(parsed.arguments, vec!["settings"]);
    }

    #[test]
    fn parse_namespaced_action_no_args() {
        let parsed = parse_action("app.bell").unwrap();
        assert_eq!(parsed.namespace, Some("app".to_string()));
        assert_eq!(parsed.name, "bell");
        assert!(parsed.arguments.is_empty());
    }

    #[test]
    fn parse_action_focus_with_arg() {
        let parsed = parse_action("focus('sidebar')").unwrap();
        assert_eq!(parsed.namespace, None);
        assert_eq!(parsed.name, "focus");
        assert_eq!(parsed.arguments, vec!["sidebar"]);
    }

    // ── parse_action: multiple arguments ─────────────────────────────────

    #[test]
    fn parse_action_multiple_args() {
        let parsed = parse_action("do_thing('a', 'b', 'c')").unwrap();
        assert_eq!(parsed.name, "do_thing");
        assert_eq!(parsed.arguments, vec!["a", "b", "c"]);
    }

    #[test]
    fn parse_action_double_quoted_args() {
        let parsed = parse_action("do_thing(\"hello\", \"world\")").unwrap();
        assert_eq!(parsed.arguments, vec!["hello", "world"]);
    }

    #[test]
    fn parse_action_mixed_quote_styles() {
        let parsed = parse_action("do_thing('hello', \"world\")").unwrap();
        assert_eq!(parsed.arguments, vec!["hello", "world"]);
    }

    #[test]
    fn parse_action_unquoted_args() {
        let parsed = parse_action("do_thing(42, true)").unwrap();
        assert_eq!(parsed.arguments, vec!["42", "true"]);
    }

    // ── parse_action: namespace with arguments ───────────────────────────

    #[test]
    fn parse_namespaced_action_with_args() {
        let parsed = parse_action("screen.push('settings')").unwrap();
        assert_eq!(parsed.namespace, Some("screen".to_string()));
        assert_eq!(parsed.name, "push");
        assert_eq!(parsed.arguments, vec!["settings"]);
    }

    // ── parse_action: whitespace handling ────────────────────────────────

    #[test]
    fn parse_action_with_leading_trailing_whitespace() {
        let parsed = parse_action("  toggle_dark  ").unwrap();
        assert_eq!(parsed.namespace, None);
        assert_eq!(parsed.name, "toggle_dark");
        assert!(parsed.arguments.is_empty());
    }

    #[test]
    fn parse_action_with_spaces_in_args() {
        let parsed = parse_action("focus( 'sidebar' )").unwrap();
        assert_eq!(parsed.arguments, vec!["sidebar"]);
    }

    // ── parse_action: edge cases / error handling ────────────────────────

    #[test]
    fn parse_empty_string_returns_none() {
        assert!(parse_action("").is_none());
    }

    #[test]
    fn parse_whitespace_only_returns_none() {
        assert!(parse_action("   ").is_none());
    }

    #[test]
    fn parse_dot_only_returns_none() {
        assert!(parse_action(".").is_none());
    }

    #[test]
    fn parse_leading_dot_returns_none() {
        assert!(parse_action(".quit").is_none());
    }

    #[test]
    fn parse_trailing_dot_returns_none() {
        assert!(parse_action("app.").is_none());
    }

    #[test]
    fn parse_unclosed_parens_returns_none() {
        assert!(parse_action("push_screen('settings'").is_none());
    }

    #[test]
    fn parse_empty_parens() {
        let parsed = parse_action("do_thing()").unwrap();
        assert_eq!(parsed.name, "do_thing");
        assert!(parsed.arguments.is_empty());
    }

    #[test]
    fn parse_paren_only_returns_none() {
        // "()" has no name before the paren
        assert!(parse_action("()").is_none());
    }

    // ── ActionDecl constants ─────────────────────────────────────────────

    #[test]
    fn app_actions_has_expected_count() {
        assert_eq!(APP_ACTIONS.len(), 8);
    }

    #[test]
    fn app_actions_quit_has_binding() {
        let quit = find_action(APP_ACTIONS, "quit").unwrap();
        assert_eq!(quit.namespace, "app");
        assert_eq!(quit.default_binding, Some("ctrl+q"));
    }

    #[test]
    fn app_actions_bell_has_no_binding() {
        let bell = find_action(APP_ACTIONS, "bell").unwrap();
        assert_eq!(bell.default_binding, None);
    }

    #[test]
    fn app_actions_focus_next() {
        let action = find_action(APP_ACTIONS, "focus_next").unwrap();
        assert_eq!(action.default_binding, Some("tab"));
        assert_eq!(action.description, "Focus next widget");
    }

    #[test]
    fn app_actions_focus_previous() {
        let action = find_action(APP_ACTIONS, "focus_previous").unwrap();
        assert_eq!(action.default_binding, Some("shift+tab"));
    }

    #[test]
    fn find_action_nonexistent_returns_none() {
        assert!(find_action(APP_ACTIONS, "nonexistent_action").is_none());
    }

    // ── ActionHandler default impl ───────────────────────────────────────

    #[test]
    fn default_action_handler_returns_empty_registry() {
        struct Dummy;
        impl ActionHandler for Dummy {}
        let d = Dummy;
        assert!(d.action_registry().is_empty());
    }

    #[test]
    fn default_execute_action_returns_false() {
        struct Dummy;
        impl ActionHandler for Dummy {}
        let mut d = Dummy;
        let action = parse_action("toggle_dark").unwrap();
        let mut ctx = EventCtx::default();
        assert!(!d.execute_action(&action, &mut ctx));
    }

    // ── ParsedAction equality / Debug ────────────────────────────────────

    #[test]
    fn parsed_action_equality() {
        let a = parse_action("app.quit").unwrap();
        let b = ParsedAction {
            namespace: Some("app".to_string()),
            name: "quit".to_string(),
            arguments: vec![],
        };
        assert_eq!(a, b);
    }

    #[test]
    fn parsed_action_debug_format() {
        let a = parse_action("toggle_dark").unwrap();
        let dbg = format!("{:?}", a);
        assert!(dbg.contains("toggle_dark"));
    }

    // ── Argument parsing edge cases ──────────────────────────────────────

    #[test]
    fn parse_arguments_empty() {
        assert!(parse_arguments("").unwrap().is_empty());
    }

    #[test]
    fn parse_arguments_whitespace_only() {
        assert!(parse_arguments("   ").unwrap().is_empty());
    }

    #[test]
    fn parse_arguments_single_quoted() {
        assert_eq!(parse_arguments("'hello'").unwrap(), vec!["hello"]);
    }

    #[test]
    fn parse_arguments_multiple_with_spaces() {
        assert_eq!(
            parse_arguments(" 'a' , 'b' , 'c' ").unwrap(),
            vec!["a", "b", "c"]
        );
    }

    #[test]
    fn parse_arguments_unquoted_numbers() {
        assert_eq!(parse_arguments("1, 2, 3").unwrap(), vec!["1", "2", "3"]);
    }

    #[test]
    fn parse_arguments_comma_in_quotes() {
        // Commas inside quotes should not split
        assert_eq!(parse_arguments("'a,b', 'c'").unwrap(), vec!["a,b", "c"]);
    }

    #[test]
    fn parse_arguments_unclosed_quote_returns_none() {
        assert!(parse_arguments("'hello").is_none());
    }

    #[test]
    fn parse_action_unclosed_quote_in_args_returns_none() {
        assert!(parse_action("focus('sidebar)").is_none());
    }
}
