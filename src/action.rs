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
    /// bubble" at dispatch time.  Follows Python's `rpartition(".")`: for
    /// `"foo.bar.baz"` the namespace is `"foo.bar"` and the name is `"baz"`.
    pub namespace: Option<String>,
    /// The action name, e.g. `"toggle_dark"`.
    pub name: String,
    /// Typed positional arguments extracted from the parenthesised suffix.
    pub arguments: Vec<ActionArgument>,
}

// ── Typed action arguments ───────────────────────────────────────────────────

/// A typed action argument.
///
/// Python Textual evaluates action arguments with `ast.literal_eval`, so
/// `"push_screen('settings')"` carries a string while `"change_count(1)"`
/// carries an integer.  This enum is the Rust analogue of the literal values
/// that evaluation can produce.
///
/// Equality, ordering, and hashing are total: floats compare via
/// [`f64::total_cmp`] and hash via [`f64::to_bits`], so the type can be used
/// in ordered/hashed collections (e.g. binding-hint dedup keys).
#[derive(Debug, Clone)]
pub enum ActionArgument {
    /// Python `None`.
    None,
    /// Python `True` / `False`.
    Bool(bool),
    /// Integer literal, e.g. `1`, `-2`.
    Int(i64),
    /// Float literal, e.g. `1.5`, `3.15`.
    Float(f64),
    /// Quoted string literal, e.g. `'settings'`.
    Str(String),
    /// Parenthesised tuple, e.g. `(1, 2)`.
    Tuple(Vec<ActionArgument>),
    /// Bracketed list, e.g. `[1, 2]`.
    List(Vec<ActionArgument>),
}

impl ActionArgument {
    /// The string payload, if this argument is a string literal.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }

    /// The integer payload, if this argument is an integer literal.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// The numeric payload as `f64` (accepts both float and int literals).
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Self::Float(f) => Some(*f),
            Self::Int(i) => Some(*i as f64),
            _ => None,
        }
    }

    /// The boolean payload, if this argument is `True` / `False`.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Whether this argument is Python `None`.
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// The element slice, if this argument is a tuple or list.
    pub fn as_items(&self) -> Option<&[ActionArgument]> {
        match self {
            Self::Tuple(items) | Self::List(items) => Some(items),
            _ => None,
        }
    }

    /// Stable rank used to order values of different variants.
    fn variant_rank(&self) -> u8 {
        match self {
            Self::None => 0,
            Self::Bool(_) => 1,
            Self::Int(_) => 2,
            Self::Float(_) => 3,
            Self::Str(_) => 4,
            Self::Tuple(_) => 5,
            Self::List(_) => 6,
        }
    }

    fn fmt_repr(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => f.write_str("None"),
            Self::Bool(true) => f.write_str("True"),
            Self::Bool(false) => f.write_str("False"),
            Self::Int(i) => write!(f, "{i}"),
            Self::Float(x) => {
                if x.is_finite() && x.fract() == 0.0 {
                    write!(f, "{x:.1}")
                } else {
                    write!(f, "{x}")
                }
            }
            Self::Str(s) => write!(f, "{s:?}"),
            Self::Tuple(items) => {
                f.write_str("(")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    item.fmt_repr(f)?;
                }
                if items.len() == 1 {
                    f.write_str(",")?;
                }
                f.write_str(")")
            }
            Self::List(items) => {
                f.write_str("[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    item.fmt_repr(f)?;
                }
                f.write_str("]")
            }
        }
    }
}

impl std::fmt::Display for ActionArgument {
    /// Python `str()`-like rendering: strings render bare, everything else
    /// renders as a literal.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Str(s) => f.write_str(s),
            other => other.fmt_repr(f),
        }
    }
}

impl Ord for ActionArgument {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use ActionArgument::*;
        match (self, other) {
            (None, None) => std::cmp::Ordering::Equal,
            (Bool(a), Bool(b)) => a.cmp(b),
            (Int(a), Int(b)) => a.cmp(b),
            (Float(a), Float(b)) => a.total_cmp(b),
            (Str(a), Str(b)) => a.cmp(b),
            (Tuple(a), Tuple(b)) | (List(a), List(b)) => a.cmp(b),
            _ => self.variant_rank().cmp(&other.variant_rank()),
        }
    }
}

impl PartialOrd for ActionArgument {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for ActionArgument {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}

impl Eq for ActionArgument {}

impl std::hash::Hash for ActionArgument {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u8(self.variant_rank());
        match self {
            Self::None => {}
            Self::Bool(b) => b.hash(state),
            Self::Int(i) => i.hash(state),
            Self::Float(x) => x.to_bits().hash(state),
            Self::Str(s) => s.hash(state),
            Self::Tuple(items) | Self::List(items) => items.hash(state),
        }
    }
}

// ── Parse errors ─────────────────────────────────────────────────────────────

/// Error returned by [`parse_action`] for malformed action strings.
///
/// Mirrors Python Textual's `ActionError` raised by `textual.actions.parse`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionParseError {
    /// The full action string that failed to parse.
    pub action: String,
    /// Human-readable description of the failure.
    pub message: String,
}

impl std::fmt::Display for ActionParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "unable to parse action {:?}: {}",
            self.action, self.message
        )
    }
}

impl std::error::Error for ActionParseError {}

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
        name: "add_class",
        namespace: "app",
        description: "Add a CSS class to widgets matched by selector",
        default_binding: None,
    },
    ActionDecl {
        name: "back",
        namespace: "app",
        description: "Return to previous screen",
        default_binding: None,
    },
    ActionDecl {
        name: "bell",
        namespace: "app",
        description: "Ring the terminal bell",
        default_binding: None,
    },
    ActionDecl {
        name: "change_theme",
        namespace: "app",
        description: "Change application theme",
        default_binding: None,
    },
    ActionDecl {
        name: "command_palette",
        namespace: "app",
        description: "Open command palette",
        default_binding: Some("ctrl+p"),
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
        name: "help_quit",
        namespace: "app",
        description: "Show quit help",
        default_binding: None,
    },
    ActionDecl {
        name: "copy_selected_text",
        namespace: "app",
        description: "Copy selected text",
        default_binding: Some("ctrl+c"),
    },
    ActionDecl {
        name: "hide_help_panel",
        namespace: "app",
        description: "Hide help panel",
        default_binding: None,
    },
    ActionDecl {
        name: "notify",
        namespace: "app",
        description: "Show notification",
        default_binding: None,
    },
    ActionDecl {
        name: "pop_screen",
        namespace: "app",
        description: "Pop the current screen",
        default_binding: None,
    },
    ActionDecl {
        name: "push_screen",
        namespace: "app",
        description: "Push a screen",
        default_binding: None,
    },
    ActionDecl {
        name: "quit",
        namespace: "app",
        description: "Quit the application",
        default_binding: Some("ctrl+q"),
    },
    ActionDecl {
        name: "remove_class",
        namespace: "app",
        description: "Remove a CSS class from widgets matched by selector",
        default_binding: None,
    },
    ActionDecl {
        name: "screenshot",
        namespace: "app",
        description: "Save screenshot",
        default_binding: Some("ctrl+s"),
    },
    ActionDecl {
        name: "show_help_panel",
        namespace: "app",
        description: "Show help panel",
        default_binding: None,
    },
    ActionDecl {
        name: "simulate_key",
        namespace: "app",
        description: "Simulate key press",
        default_binding: None,
    },
    ActionDecl {
        name: "suspend_process",
        namespace: "app",
        description: "Suspend process",
        default_binding: None,
    },
    ActionDecl {
        name: "switch_mode",
        namespace: "app",
        description: "Switch mode",
        default_binding: None,
    },
    ActionDecl {
        name: "switch_screen",
        namespace: "app",
        description: "Switch screen",
        default_binding: None,
    },
    ActionDecl {
        name: "toggle_class",
        namespace: "app",
        description: "Toggle a CSS class on widgets matched by selector",
        default_binding: None,
    },
    ActionDecl {
        name: "toggle_dark",
        namespace: "app",
        description: "Toggle dark mode",
        default_binding: Some("ctrl+d"),
    },
];

// ── Parser ───────────────────────────────────────────────────────────────────

/// Parse an action string into a [`ParsedAction`].
///
/// Faithful to Python Textual's `textual.actions.parse`:
///
/// - The `name(args)` shape follows Python's `([\w\.]+)\((.*)\)` regex: a
///   leading run of word characters / dots, an opening paren, and a closing
///   paren later in the string (the *last* one, per greedy `.*`).
/// - Arguments are evaluated like `ast.literal_eval(f"({args},)")`, producing
///   typed [`ActionArgument`] values (ints, floats, booleans, `None`, strings,
///   nested tuples/lists).
/// - The namespace splits on the **last** dot (`rpartition(".")`), so
///   `"foo.bar.baz"` yields namespace `"foo.bar"` and name `"baz"`.
///
/// | Input                          | Namespace          | Name            | Arguments             |
/// |--------------------------------|--------------------|-----------------|-----------------------|
/// | `"toggle_dark"`                | `None`             | `"toggle_dark"` | `[]`                  |
/// | `"app.quit"`                   | `Some("app")`      | `"quit"`        | `[]`                  |
/// | `"push_screen('settings')"`    | `None`             | `"push_screen"` | `[Str("settings")]`   |
/// | `"foo.bar.baz(3, 3.15)"`       | `Some("foo.bar")`  | `"baz"`         | `[Int(3), Float(3.15)]` |
///
/// # Errors
///
/// Returns [`ActionParseError`] where Python raises `ActionError` (malformed
/// argument lists such as `"foo(,,,,,)"` or `"bar(1 2 3)"`), and additionally
/// for strings with no valid action name (empty input, `"app."`, unclosed
/// parens).  Python returns those verbatim and only fails later at dispatch
/// time; erroring at parse time keeps the same observable behaviour while
/// diagnosing earlier.
pub fn parse_action(input: &str) -> Result<ParsedAction, ActionParseError> {
    let err = |message: String| ActionParseError {
        action: input.to_string(),
        message,
    };
    let trimmed = input.trim();

    // Python: re_action_args = r"([\w\.]+)\((.*)\)" with `re.match`; greedy
    // `.*` pairs the first '(' after the name with the LAST ')' in the string.
    let head_len = trimmed
        .char_indices()
        .find(|&(_, c)| !(c.is_alphanumeric() || c == '_' || c == '.'))
        .map(|(i, _)| i)
        .unwrap_or(trimmed.len());

    let (name_part, arguments) = if head_len > 0
        && trimmed[head_len..].starts_with('(')
        && let Some(last_rparen) = trimmed.rfind(')')
        && last_rparen > head_len
    {
        let args_str = &trimmed[head_len + 1..last_rparen];
        // Python only evaluates a non-empty argument string (`if action_args_str:`),
        // so `f()` is empty args while `f(   )` is an evaluation error.
        let arguments = if args_str.is_empty() {
            Vec::new()
        } else {
            parse_arguments(args_str)
                .map_err(|detail| err(format!("unable to parse {args_str:?}: {detail}")))?
        };
        (&trimmed[..head_len], arguments)
    } else {
        (trimmed, Vec::new())
    };

    // Python: namespace, _, action_name = action_name.rpartition("."); the
    // namespace is everything before the LAST dot.
    let (namespace, name) = match name_part.rfind('.') {
        Some(pos) => {
            let ns = &name_part[..pos];
            let n = &name_part[pos + 1..];
            let namespace = if ns.is_empty() {
                None
            } else {
                Some(ns.to_string())
            };
            (namespace, n)
        }
        None => (None, name_part),
    };

    if name.is_empty() {
        return Err(err("empty action name".to_string()));
    }
    if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(err(format!("invalid action name {name:?}")));
    }
    if let Some(ns) = &namespace
        && !ns
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
    {
        return Err(err(format!("invalid action namespace {ns:?}")));
    }

    Ok(ParsedAction {
        namespace,
        name: name.to_string(),
        arguments,
    })
}

/// Parse a comma-separated argument list into typed [`ActionArgument`]s.
///
/// Equivalent to Python's `ast.literal_eval(f"({input},)")` over the accepted
/// literal subset: numbers (with unary sign), `True`/`False`/`None`, quoted
/// strings (including triple quotes and implicit concatenation), tuples with
/// Python grouping semantics (`(1)` is the int `1`, `(1,)` is a tuple), and
/// lists.  A trailing comma at the top level is an error, exactly like
/// Python's wrapping (`f(1,)` evaluates `"(1,,)"`).
fn parse_arguments(input: &str) -> Result<Vec<ActionArgument>, String> {
    let mut parser = LiteralParser::new(input);
    let mut args = vec![parser.parse_expr()?];
    loop {
        parser.skip_ws();
        match parser.peek() {
            Option::None => break,
            Some(',') => {
                parser.advance();
                args.push(parser.parse_expr()?);
            }
            Some(c) => return Err(format!("unexpected character {c:?}")),
        }
    }
    Ok(args)
}

/// Minimal recursive-descent parser for the Python literal subset accepted in
/// action arguments (`ast.literal_eval` analogue).
struct LiteralParser {
    chars: Vec<char>,
    pos: usize,
}

impl LiteralParser {
    fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.pos += 1;
        }
    }

    fn parse_expr(&mut self) -> Result<ActionArgument, String> {
        self.skip_ws();
        match self.peek() {
            Option::None => Err("unexpected end of arguments".to_string()),
            Some('(') => self.parse_parenthesised(),
            Some('[') => self.parse_list(),
            Some('\'') | Some('"') => self.parse_string_group(),
            Some('+') | Some('-') => self.parse_signed(),
            Some(c) if c.is_ascii_digit() || c == '.' => self.parse_number(false),
            Some(c) if c.is_alphabetic() || c == '_' => self.parse_keyword(),
            Some(c) => Err(format!("unexpected character {c:?}")),
        }
    }

    /// Unary `+`/`-` chains applied to a numeric literal (Python allows
    /// nesting: `literal_eval("--1")` is `1`).
    fn parse_signed(&mut self) -> Result<ActionArgument, String> {
        let mut negative = false;
        while let Some(c) = self.peek() {
            match c {
                '-' => {
                    negative = !negative;
                    self.pos += 1;
                }
                '+' => {
                    self.pos += 1;
                }
                c if c.is_whitespace() => self.skip_ws(),
                _ => break,
            }
        }
        match self.peek() {
            Some(c) if c.is_ascii_digit() || c == '.' => self.parse_number(negative),
            _ => Err("unary '+'/'-' must be followed by a number".to_string()),
        }
    }

    fn parse_number(&mut self, negative: bool) -> Result<ActionArgument, String> {
        let start = self.pos;
        let mut prev = '\0';
        while let Some(c) = self.peek() {
            let take = c.is_ascii_digit()
                || c == '.'
                || c == '_'
                || c == 'e'
                || c == 'E'
                || ((c == '+' || c == '-') && (prev == 'e' || prev == 'E'));
            if !take {
                break;
            }
            prev = c;
            self.pos += 1;
        }
        let text: String = self.chars[start..self.pos]
            .iter()
            .filter(|&&c| c != '_')
            .collect();
        if text.contains(['.', 'e', 'E']) {
            let value: f64 = text
                .parse()
                .map_err(|_| format!("invalid number literal {text:?}"))?;
            Ok(ActionArgument::Float(if negative { -value } else { value }))
        } else {
            let value: i64 = text
                .parse()
                .map_err(|_| format!("invalid number literal {text:?}"))?;
            Ok(ActionArgument::Int(if negative { -value } else { value }))
        }
    }

    fn parse_keyword(&mut self) -> Result<ActionArgument, String> {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_alphanumeric() || c == '_') {
            self.pos += 1;
        }
        let word: String = self.chars[start..self.pos].iter().collect();
        match word.as_str() {
            "True" => Ok(ActionArgument::Bool(true)),
            "False" => Ok(ActionArgument::Bool(false)),
            "None" => Ok(ActionArgument::None),
            _ => Err(format!("{word:?} is not a literal")),
        }
    }

    /// `(...)` with Python semantics: `()` is the empty tuple, `(expr)` is
    /// grouping (yields `expr` itself), `(expr,)` and `(a, b)` are tuples.
    fn parse_parenthesised(&mut self) -> Result<ActionArgument, String> {
        self.advance(); // consume '('
        self.skip_ws();
        if self.peek() == Some(')') {
            self.advance();
            return Ok(ActionArgument::Tuple(Vec::new()));
        }
        let first = self.parse_expr()?;
        self.skip_ws();
        match self.advance() {
            Some(')') => Ok(first), // grouping parens, not a tuple
            Some(',') => {
                let mut items = vec![first];
                loop {
                    self.skip_ws();
                    if self.peek() == Some(')') {
                        self.advance();
                        return Ok(ActionArgument::Tuple(items));
                    }
                    items.push(self.parse_expr()?);
                    self.skip_ws();
                    match self.advance() {
                        Some(',') => {}
                        Some(')') => return Ok(ActionArgument::Tuple(items)),
                        Some(c) => return Err(format!("expected ',' or ')', found {c:?}")),
                        Option::None => return Err("unclosed '('".to_string()),
                    }
                }
            }
            Some(c) => Err(format!("expected ',' or ')', found {c:?}")),
            Option::None => Err("unclosed '('".to_string()),
        }
    }

    fn parse_list(&mut self) -> Result<ActionArgument, String> {
        self.advance(); // consume '['
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            if self.peek() == Some(']') {
                self.advance();
                return Ok(ActionArgument::List(items));
            }
            items.push(self.parse_expr()?);
            self.skip_ws();
            match self.advance() {
                Some(',') => {}
                Some(']') => return Ok(ActionArgument::List(items)),
                Some(c) => return Err(format!("expected ',' or ']', found {c:?}")),
                Option::None => return Err("unclosed '['".to_string()),
            }
        }
    }

    /// One or more adjacent string literals (Python implicit concatenation:
    /// `'a' 'b'` is `'ab'`).
    fn parse_string_group(&mut self) -> Result<ActionArgument, String> {
        let mut out = self.parse_string()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some('\'') | Some('"') => out.push_str(&self.parse_string()?),
                _ => break,
            }
        }
        Ok(ActionArgument::Str(out))
    }

    fn parse_string(&mut self) -> Result<String, String> {
        let Some(quote) = self.advance() else {
            return Err("expected string literal".to_string());
        };
        let triple = self.peek() == Some(quote) && self.peek_at(1) == Some(quote);
        if triple {
            self.pos += 2;
        }
        let mut out = String::new();
        loop {
            let Some(c) = self.advance() else {
                return Err("unterminated string literal".to_string());
            };
            if c == '\\' {
                let Some(esc) = self.advance() else {
                    return Err("unterminated string escape".to_string());
                };
                match esc {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    '0' => out.push('\0'),
                    'a' => out.push('\x07'),
                    'b' => out.push('\x08'),
                    'f' => out.push('\x0C'),
                    'v' => out.push('\x0B'),
                    '\\' | '\'' | '"' => out.push(esc),
                    '\n' => {} // line continuation
                    'x' => {
                        let hex: String = [self.advance(), self.advance()]
                            .into_iter()
                            .flatten()
                            .collect();
                        let code = u32::from_str_radix(&hex, 16)
                            .ok()
                            .and_then(char::from_u32)
                            .ok_or_else(|| format!("invalid \\x escape {hex:?}"))?;
                        out.push(code);
                    }
                    other => {
                        // Python keeps unrecognised escapes verbatim.
                        out.push('\\');
                        out.push(other);
                    }
                }
                continue;
            }
            if c == quote {
                if !triple {
                    return Ok(out);
                }
                if self.peek() == Some(quote) && self.peek_at(1) == Some(quote) {
                    self.pos += 2;
                    return Ok(out);
                }
                out.push(c);
                continue;
            }
            out.push(c);
        }
    }
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
        assert_eq!(
            parsed.arguments,
            vec![ActionArgument::Str("settings".to_string())]
        );
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
        assert_eq!(
            parsed.arguments,
            vec![ActionArgument::Str("sidebar".to_string())]
        );
    }

    // ── parse_action: multiple arguments ─────────────────────────────────

    fn strs(items: &[&str]) -> Vec<ActionArgument> {
        items
            .iter()
            .map(|s| ActionArgument::Str(s.to_string()))
            .collect()
    }

    #[test]
    fn parse_action_multiple_args() {
        let parsed = parse_action("do_thing('a', 'b', 'c')").unwrap();
        assert_eq!(parsed.name, "do_thing");
        assert_eq!(parsed.arguments, strs(&["a", "b", "c"]));
    }

    #[test]
    fn parse_action_double_quoted_args() {
        let parsed = parse_action("do_thing(\"hello\", \"world\")").unwrap();
        assert_eq!(parsed.arguments, strs(&["hello", "world"]));
    }

    #[test]
    fn parse_action_mixed_quote_styles() {
        let parsed = parse_action("do_thing('hello', \"world\")").unwrap();
        assert_eq!(parsed.arguments, strs(&["hello", "world"]));
    }

    #[test]
    fn parse_action_typed_unquoted_args() {
        let parsed = parse_action("do_thing(42, True)").unwrap();
        assert_eq!(
            parsed.arguments,
            vec![ActionArgument::Int(42), ActionArgument::Bool(true)]
        );
    }

    #[test]
    fn parse_action_lowercase_true_is_error() {
        // Python `ast.literal_eval` only accepts `True`/`False`/`None`.
        assert!(parse_action("do_thing(true)").is_err());
    }

    // ── parse_action: namespace with arguments ───────────────────────────

    #[test]
    fn parse_namespaced_action_with_args() {
        let parsed = parse_action("screen.push('settings')").unwrap();
        assert_eq!(parsed.namespace, Some("screen".to_string()));
        assert_eq!(parsed.name, "push");
        assert_eq!(parsed.arguments, strs(&["settings"]));
    }

    #[test]
    fn parse_namespace_splits_on_last_dot() {
        // Python `rpartition(".")`: everything before the LAST dot is the
        // namespace.
        let parsed = parse_action("foo.bar.baz").unwrap();
        assert_eq!(parsed.namespace, Some("foo.bar".to_string()));
        assert_eq!(parsed.name, "baz");
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
        assert_eq!(parsed.arguments, strs(&["sidebar"]));
    }

    // ── parse_action: edge cases / error handling ────────────────────────

    #[test]
    fn parse_empty_string_is_error() {
        assert!(parse_action("").is_err());
    }

    #[test]
    fn parse_whitespace_only_is_error() {
        assert!(parse_action("   ").is_err());
    }

    #[test]
    fn parse_dot_only_is_error() {
        assert!(parse_action(".").is_err());
    }

    #[test]
    fn parse_leading_dot_is_empty_namespace() {
        // Python: ".quit".rpartition(".") gives namespace "" and name "quit".
        let parsed = parse_action(".quit").unwrap();
        assert_eq!(parsed.namespace, None);
        assert_eq!(parsed.name, "quit");
    }

    #[test]
    fn parse_trailing_dot_is_error() {
        assert!(parse_action("app.").is_err());
    }

    #[test]
    fn parse_unclosed_parens_is_error() {
        assert!(parse_action("push_screen('settings'").is_err());
    }

    #[test]
    fn parse_empty_parens() {
        let parsed = parse_action("do_thing()").unwrap();
        assert_eq!(parsed.name, "do_thing");
        assert!(parsed.arguments.is_empty());
    }

    #[test]
    fn parse_paren_only_is_error() {
        // "()" has no name before the paren
        assert!(parse_action("()").is_err());
    }

    // ── ActionDecl constants ─────────────────────────────────────────────

    #[test]
    fn app_actions_has_expected_count() {
        assert_eq!(APP_ACTIONS.len(), 24);
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
    fn app_actions_cover_python_matrix() {
        let expected = [
            "add_class",
            "back",
            "bell",
            "change_theme",
            "command_palette",
            "focus",
            "focus_next",
            "focus_previous",
            "help_quit",
            "copy_selected_text",
            "hide_help_panel",
            "notify",
            "pop_screen",
            "push_screen",
            "quit",
            "remove_class",
            "screenshot",
            "show_help_panel",
            "simulate_key",
            "suspend_process",
            "switch_mode",
            "switch_screen",
            "toggle_class",
            "toggle_dark",
        ];
        for name in expected {
            assert!(
                find_action(APP_ACTIONS, name).is_some(),
                "missing app action {name}"
            );
        }
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
    fn parse_arguments_whitespace_only_is_error() {
        // Python evaluates `(   ,)`, which is a syntax error; only the
        // exactly-empty argument string short-circuits to no arguments.
        assert!(parse_arguments("   ").is_err());
    }

    #[test]
    fn parse_arguments_single_quoted() {
        assert_eq!(parse_arguments("'hello'").unwrap(), strs(&["hello"]));
    }

    #[test]
    fn parse_arguments_multiple_with_spaces() {
        assert_eq!(
            parse_arguments(" 'a' , 'b' , 'c' ").unwrap(),
            strs(&["a", "b", "c"])
        );
    }

    #[test]
    fn parse_arguments_unquoted_numbers() {
        assert_eq!(
            parse_arguments("1, 2, 3").unwrap(),
            vec![
                ActionArgument::Int(1),
                ActionArgument::Int(2),
                ActionArgument::Int(3)
            ]
        );
    }

    #[test]
    fn parse_arguments_comma_in_quotes() {
        // Commas inside quotes should not split
        assert_eq!(parse_arguments("'a,b', 'c'").unwrap(), strs(&["a,b", "c"]));
    }

    #[test]
    fn parse_arguments_unclosed_quote_is_error() {
        assert!(parse_arguments("'hello").is_err());
    }

    #[test]
    fn parse_action_unclosed_quote_in_args_is_error() {
        assert!(parse_action("focus('sidebar)").is_err());
    }

    // ── Ported from Python tests/test_actions.py ────────────────────────

    /// `test_parse_action` (6 params).
    #[test]
    fn python_test_parse_action_matrix() {
        use ActionArgument::*;
        let cases: Vec<(&str, Option<&str>, &str, Vec<ActionArgument>)> = vec![
            ("spam", Option::None, "spam", vec![]),
            (
                "hypothetical_action()",
                Option::None,
                "hypothetical_action",
                vec![],
            ),
            (
                "another_action(1)",
                Option::None,
                "another_action",
                vec![Int(1)],
            ),
            (
                "foo(True, False)",
                Option::None,
                "foo",
                vec![Bool(true), Bool(false)],
            ),
            (
                "foo.bar.baz(3, 3.15, 'Python')",
                Some("foo.bar"),
                "baz",
                vec![Int(3), Float(3.15), Str("Python".to_string())],
            ),
            (
                "m1234.n5678(None, [1, 2])",
                Some("m1234"),
                "n5678",
                vec![None, List(vec![Int(1), Int(2)])],
            ),
        ];
        for (action_string, expected_namespace, expected_name, expected_arguments) in cases {
            let parsed = parse_action(action_string)
                .unwrap_or_else(|e| panic!("{action_string:?} should parse: {e}"));
            assert_eq!(
                parsed.namespace.as_deref(),
                expected_namespace,
                "namespace of {action_string:?}"
            );
            assert_eq!(parsed.name, expected_name, "name of {action_string:?}");
            assert_eq!(
                parsed.arguments, expected_arguments,
                "arguments of {action_string:?}"
            );
        }
    }

    /// `test_nested_and_convoluted_tuple_arguments` (9 params).
    #[test]
    fn python_test_nested_and_convoluted_tuple_arguments() {
        use ActionArgument::*;
        let cases: Vec<(&str, Vec<ActionArgument>)> = vec![
            ("f()", vec![]),
            ("f(())", vec![Tuple(vec![])]),
            ("f((1, 2, 3))", vec![Tuple(vec![Int(1), Int(2), Int(3)])]),
            (
                "f((1, 2, 3), (1, 2, 3))",
                vec![
                    Tuple(vec![Int(1), Int(2), Int(3)]),
                    Tuple(vec![Int(1), Int(2), Int(3)]),
                ],
            ),
            (
                "f(((1, 2), (), None), 0)",
                vec![
                    Tuple(vec![Tuple(vec![Int(1), Int(2)]), Tuple(vec![]), None]),
                    Int(0),
                ],
            ),
            ("f((((((1))))))", vec![Int(1)]),
            ("f(((((((((1, 2)))))))))", vec![Tuple(vec![Int(1), Int(2)])]),
            (
                "f((1, 2), (3, 4))",
                vec![Tuple(vec![Int(1), Int(2)]), Tuple(vec![Int(3), Int(4)])],
            ),
            (
                "f((((((1, 2), (3, 4))))))",
                vec![Tuple(vec![
                    Tuple(vec![Int(1), Int(2)]),
                    Tuple(vec![Int(3), Int(4)]),
                ])],
            ),
        ];
        for (action_string, expected_arguments) in cases {
            let parsed = parse_action(action_string)
                .unwrap_or_else(|e| panic!("{action_string:?} should parse: {e}"));
            assert_eq!(
                parsed.arguments, expected_arguments,
                "arguments of {action_string:?}"
            );
        }
    }

    /// `test_parse_action_nested_special_character_arguments` (7 params).
    #[test]
    fn python_test_nested_special_character_arguments() {
        let cases: Vec<(&str, &str)> = vec![
            ("f('')", ""),
            ("f(\"\")", ""),
            ("f('''''')", ""),
            ("f(\"\"\"\"\"\")", ""),
            ("f('(')", "("),
            ("f(')')", ")"), // Regression test for Textualize/textual#2088
            ("f('f()')", "f()"),
        ];
        for (action_string, expected) in cases {
            let parsed = parse_action(action_string)
                .unwrap_or_else(|e| panic!("{action_string:?} should parse: {e}"));
            assert_eq!(
                parsed.arguments,
                vec![ActionArgument::Str(expected.to_string())],
                "arguments of {action_string:?}"
            );
        }
    }

    /// `test_parse_action_raises_error` (5 params).
    #[test]
    fn python_test_parse_action_raises_error() {
        let cases = [
            "foo(,,,,,)",
            "bar(1 2 3 4 5)",
            "baz.spam(Tru, Fals, in)",
            "ham(not)",
            "cheese((((()",
        ];
        for action_string in cases {
            assert!(
                parse_action(action_string).is_err(),
                "{action_string:?} should be a parse error"
            );
        }
    }

    // ── Typed literal coverage ───────────────────────────────────────────

    #[test]
    fn parse_negative_and_float_literals() {
        let parsed = parse_action("f(-2, +3, 1.5, -0.25)").unwrap();
        assert_eq!(
            parsed.arguments,
            vec![
                ActionArgument::Int(-2),
                ActionArgument::Int(3),
                ActionArgument::Float(1.5),
                ActionArgument::Float(-0.25),
            ]
        );
    }

    #[test]
    fn action_argument_accessors() {
        assert_eq!(ActionArgument::Str("s".to_string()).as_str(), Some("s"));
        assert_eq!(ActionArgument::Int(7).as_int(), Some(7));
        assert_eq!(ActionArgument::Int(7).as_float(), Some(7.0));
        assert_eq!(ActionArgument::Float(1.5).as_float(), Some(1.5));
        assert_eq!(ActionArgument::Bool(true).as_bool(), Some(true));
        assert!(ActionArgument::None.is_none());
        assert_eq!(
            ActionArgument::Tuple(vec![ActionArgument::Int(1)]).as_items(),
            Some(&[ActionArgument::Int(1)][..])
        );
        assert_eq!(ActionArgument::Int(7).as_str(), None);
        assert_eq!(ActionArgument::Str("s".to_string()).as_int(), None);
    }

    #[test]
    fn action_parse_error_display_mentions_action() {
        let err = parse_action("foo(,,,,,)").unwrap_err();
        assert!(err.to_string().contains("foo(,,,,,)"));
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
