//! Action system foundation: types, traits, parser, and built-in declarations.
//!
//! This module implements the Python Textual–style string action system where
//! widgets declare actions they handle and keybindings/buttons invoke actions by
//! name (e.g. `"toggle_dark"`, `"app.quit"`, `"push_screen('settings')"`).
//!
//! **This is types + parsing only.** Runtime wiring (dispatch, bubble resolution)
//! is planned for P4-08.

use crate::event::EventCtx;
use crate::node_id::NodeId;
use crate::widget_tree::WidgetTree;

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
    /// The namespace this handler owns (e.g. `"app"`, `"screen"`).
    ///
    /// Used by [`resolve_action`] to route namespaced actions like `"app.quit"`.
    /// Returns `""` by default (no namespace — the handler participates in
    /// bubble resolution only).
    fn action_namespace(&self) -> &str {
        ""
    }

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
    ActionDecl {
        name: "add_class",
        namespace: "app",
        description: "Add a CSS class to widgets matched by selector",
        default_binding: None,
    },
    ActionDecl {
        name: "remove_class",
        namespace: "app",
        description: "Remove a CSS class from widgets matched by selector",
        default_binding: None,
    },
    ActionDecl {
        name: "toggle_class",
        namespace: "app",
        description: "Toggle a CSS class on widgets matched by selector",
        default_binding: None,
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

// ── Namespace resolution ────────────────────────────────────────────────────

/// Result of resolving a [`ParsedAction`] to its handler in the widget tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedAction {
    /// The tree node that will handle the action.
    pub node: NodeId,
    /// Copy of the matching action declaration.
    pub decl: ActionDecl,
}

/// Resolve a [`ParsedAction`] to the widget that should handle it.
///
/// Walks from `focused` toward the tree root:
///
/// - **Namespaced** (e.g. `"app.quit"`): finds the first ancestor whose
///   namespace matches, then looks up the action in its registry. If the
///   namespace matches but the action is not in the registry, resolution
///   stops and returns `None` (the namespace is an explicit routing
///   directive).
/// - **Unnamespaced** (e.g. `"toggle_dark"`): bubbles from `focused` to
///   root, returning the first handler whose registry contains the action
///   name.
///
/// `get_node_actions` is called once per visited node. It should return
/// `Some((namespace, registry))` if the node can handle actions, or `None`
/// to skip the node.
pub fn resolve_action<'a>(
    action: &ParsedAction,
    tree: &WidgetTree,
    focused: NodeId,
    get_node_actions: impl Fn(NodeId) -> Option<(&'a str, &'a [ActionDecl])>,
) -> Option<ResolvedAction> {
    // Walk chain: focused → parent → … → root.
    let mut chain = vec![focused];
    chain.extend(tree.ancestors(focused));

    match &action.namespace {
        Some(ns) => {
            // Namespaced: find the first node whose namespace matches.
            for &node in &chain {
                if let Some((node_ns, registry)) = get_node_actions(node)
                    && node_ns == ns.as_str()
                {
                    return find_action(registry, &action.name)
                        .map(|decl| ResolvedAction { node, decl: *decl });
                }
            }
            None
        }
        None => {
            // Unnamespaced: first handler with the action in its registry wins.
            for &node in &chain {
                if let Some((_ns, registry)) = get_node_actions(node)
                    && let Some(decl) = find_action(registry, &action.name)
                {
                    return Some(ResolvedAction { node, decl: *decl });
                }
            }
            None
        }
    }
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
        assert_eq!(APP_ACTIONS.len(), 11);
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
    fn app_actions_include_selector_class_mutations() {
        assert!(find_action(APP_ACTIONS, "add_class").is_some());
        assert!(find_action(APP_ACTIONS, "remove_class").is_some());
        assert!(find_action(APP_ACTIONS, "toggle_class").is_some());
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

    // ── ActionHandler::action_namespace default ─────────────────────────

    #[test]
    fn default_action_namespace_is_empty() {
        struct Dummy;
        impl ActionHandler for Dummy {}
        let d = Dummy;
        assert_eq!(d.action_namespace(), "");
    }

    #[test]
    fn custom_action_namespace() {
        struct AppHandler;
        impl ActionHandler for AppHandler {
            fn action_namespace(&self) -> &str {
                "app"
            }
        }
        let h = AppHandler;
        assert_eq!(h.action_namespace(), "app");
    }
}

// ── resolve_action tests ────────────────────────────────────────────────────

#[cfg(test)]
mod resolve_tests {
    use super::*;
    use crate::widget_tree::WidgetTree;
    use crate::widgets::Widget;
    use rich_rs::{Console, ConsoleOptions, Segments};
    use std::collections::HashMap;

    // -- Minimal widget for tree construction ---------------------------------

    struct DummyWidget;

    impl Widget for DummyWidget {
        fn render(&self, _: &Console, _: &ConsoleOptions) -> Segments {
            Segments::new()
        }
    }

    // -- Test action declarations ---------------------------------------------

    static APP_DECLS: &[ActionDecl] = &[
        ActionDecl {
            name: "quit",
            namespace: "app",
            description: "Quit",
            default_binding: None,
        },
        ActionDecl {
            name: "toggle_dark",
            namespace: "app",
            description: "Toggle dark",
            default_binding: None,
        },
    ];

    static WIDGET_DECLS: &[ActionDecl] = &[ActionDecl {
        name: "do_thing",
        namespace: "widget",
        description: "Do a thing",
        default_binding: None,
    }];

    // -- Helper: build a 3-node tree (root → mid → leaf) ---------------------

    fn make_tree_3() -> (WidgetTree, NodeId, NodeId, NodeId) {
        let mut tree = WidgetTree::new();
        let root = tree.set_root(Box::new(DummyWidget));
        let mid = tree.mount(root, Box::new(DummyWidget));
        let leaf = tree.mount(mid, Box::new(DummyWidget));
        (tree, root, mid, leaf)
    }

    // -- Helper: provider closure from a HashMap ------------------------------

    type ActionMap = HashMap<NodeId, (&'static str, &'static [ActionDecl])>;

    fn provider(
        map: &ActionMap,
    ) -> impl Fn(NodeId) -> Option<(&'static str, &'static [ActionDecl])> + '_ {
        move |node| map.get(&node).copied()
    }

    // -- Tests ----------------------------------------------------------------

    #[test]
    fn resolve_unnamespaced_bubbles_to_first_handler() {
        let (tree, root, _mid, leaf) = make_tree_3();
        let mut map = ActionMap::new();
        map.insert(root, ("app", APP_DECLS));

        let action = parse_action("quit").unwrap();
        let result = resolve_action(&action, &tree, leaf, provider(&map));

        let resolved = result.unwrap();
        assert_eq!(resolved.node, root);
        assert_eq!(resolved.decl.name, "quit");
    }

    #[test]
    fn resolve_namespaced_goes_to_matching_namespace() {
        let (tree, root, _mid, leaf) = make_tree_3();
        let mut map = ActionMap::new();
        map.insert(root, ("app", APP_DECLS));

        let action = parse_action("app.toggle_dark").unwrap();
        let result = resolve_action(&action, &tree, leaf, provider(&map));

        let resolved = result.unwrap();
        assert_eq!(resolved.node, root);
        assert_eq!(resolved.decl.name, "toggle_dark");
    }

    #[test]
    fn resolve_unknown_action_returns_none() {
        let (tree, root, _mid, leaf) = make_tree_3();
        let mut map = ActionMap::new();
        map.insert(root, ("app", APP_DECLS));

        let action = parse_action("nonexistent").unwrap();
        let result = resolve_action(&action, &tree, leaf, provider(&map));
        assert!(result.is_none());
    }

    #[test]
    fn resolve_multiple_handlers_first_ancestor_wins() {
        let (tree, root, mid, leaf) = make_tree_3();
        let mut map = ActionMap::new();
        // Both mid and root have "quit" in their registry.
        map.insert(mid, ("", APP_DECLS));
        map.insert(root, ("app", APP_DECLS));

        let action = parse_action("quit").unwrap();
        let result = resolve_action(&action, &tree, leaf, provider(&map));

        // mid is closer to leaf, so it wins.
        let resolved = result.unwrap();
        assert_eq!(resolved.node, mid);
    }

    #[test]
    fn resolve_namespaced_unknown_namespace_returns_none() {
        let (tree, root, _mid, leaf) = make_tree_3();
        let mut map = ActionMap::new();
        map.insert(root, ("app", APP_DECLS));

        let action = parse_action("screen.push").unwrap();
        let result = resolve_action(&action, &tree, leaf, provider(&map));
        assert!(result.is_none());
    }

    #[test]
    fn resolve_namespaced_action_not_in_registry_returns_none() {
        let (tree, root, _mid, leaf) = make_tree_3();
        let mut map = ActionMap::new();
        map.insert(root, ("app", APP_DECLS));

        // Namespace "app" matches root, but "nonexistent" is not in its registry.
        let action = parse_action("app.nonexistent").unwrap();
        let result = resolve_action(&action, &tree, leaf, provider(&map));
        assert!(result.is_none());
    }

    #[test]
    fn resolve_focused_node_itself_can_handle() {
        let (tree, _root, _mid, leaf) = make_tree_3();
        let mut map = ActionMap::new();
        map.insert(leaf, ("widget", WIDGET_DECLS));

        let action = parse_action("do_thing").unwrap();
        let result = resolve_action(&action, &tree, leaf, provider(&map));

        let resolved = result.unwrap();
        assert_eq!(resolved.node, leaf);
        assert_eq!(resolved.decl.name, "do_thing");
    }

    #[test]
    fn resolve_no_handlers_returns_none() {
        let (tree, _root, _mid, leaf) = make_tree_3();
        // Empty provider — no node handles actions.
        let action = parse_action("quit").unwrap();
        let result = resolve_action(&action, &tree, leaf, |_| None);
        assert!(result.is_none());
    }
}
