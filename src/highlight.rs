//! Syntax highlighting for Markdown code fences.
//!
//! Python parity: `textual/highlight.py`. Python lexes code with **pygments**
//! and maps token types to THEME-token styles (`HighlightTheme.STYLES`, e.g.
//! `Token.Keyword -> "$text-accent"`, `Token.Name.Function -> "$text-warning
//! underline"`). The colours therefore come from the app theme, not from a
//! fixed highlighter colour scheme.
//!
//! Rust lexes with **syntect** (the pygments analogue already in the
//! dependency tree via rich-rs) and maps TextMate scopes onto the same theme
//! tokens. Two deliberate emulation details:
//!
//! - pygments emits `Token.Name` (-> `$text-primary`) for EVERY bare
//!   identifier, while TextMate grammars leave plain identifiers unscoped.
//!   [`highlight_lines`] post-styles identifier runs inside unstyled spans so
//!   names render `$text-primary` exactly as Python does.
//! - Styles with fractional alpha (`$text 60%`, `$text-success 80%`) are
//!   flattened over the fence's composited surface at render time, matching
//!   Python's `Content` flattening.

use std::sync::OnceLock;

use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxSet};

/// Theme token a highlight style draws its colour from (Python `$text`,
/// `$text-primary`, … in `HighlightTheme.STYLES`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HighlightColor {
    Text,
    TextPrimary,
    TextSecondary,
    TextAccent,
    TextWarning,
    TextSuccess,
    TextError,
}

impl HighlightColor {
    /// The theme token name (without `$`).
    pub(crate) fn token(self) -> &'static str {
        match self {
            HighlightColor::Text => "text",
            HighlightColor::TextPrimary => "text-primary",
            HighlightColor::TextSecondary => "text-secondary",
            HighlightColor::TextAccent => "text-accent",
            HighlightColor::TextWarning => "text-warning",
            HighlightColor::TextSuccess => "text-success",
            HighlightColor::TextError => "text-error",
        }
    }
}

/// Resolved semantic style for one span (colour token + alpha + attributes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HighlightStyle {
    pub color: Option<HighlightColor>,
    /// Fractional alpha for `color` (e.g. `$text 60%`); flatten over the code
    /// surface at render time. `1.0` = opaque.
    pub alpha: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

impl Default for HighlightStyle {
    fn default() -> Self {
        Self {
            color: None,
            alpha: 1.0,
            bold: false,
            italic: false,
            underline: false,
        }
    }
}

impl HighlightStyle {
    const fn plain() -> Self {
        Self {
            color: None,
            alpha: 1.0,
            bold: false,
            italic: false,
            underline: false,
        }
    }
    const fn color(color: HighlightColor) -> Self {
        Self {
            color: Some(color),
            ..Self::plain()
        }
    }
    const fn alpha(mut self, alpha: f32) -> Self {
        self.alpha = alpha;
        self
    }
    const fn bold(mut self) -> Self {
        self.bold = true;
        self
    }
    const fn italic(mut self) -> Self {
        self.italic = true;
        self
    }
    const fn underline(mut self) -> Self {
        self.underline = true;
        self
    }
}

/// One styled run of text (never spans a line break).
#[derive(Clone, Debug)]
pub(crate) struct HighlightSpan {
    pub text: String,
    pub style: HighlightStyle,
}

/// Scope-selector rules mirroring Python `HighlightTheme.STYLES` (pygments
/// token -> style), expressed as TextMate scope prefixes. For a scope stack the
/// INNERMOST scope wins; within one scope the most specific (longest) matching
/// selector wins — the analogue of pygments' token-hierarchy fallback.
fn rules() -> &'static [(Scope, HighlightStyle)] {
    static RULES: OnceLock<Vec<(Scope, HighlightStyle)>> = OnceLock::new();
    RULES.get_or_init(|| {
        use HighlightColor::*;
        let mk = |s: &str| Scope::new(s).expect("valid scope selector");
        vec![
            // Token.Literal.String.Doc: "$text-success 80% italic"
            (
                mk("comment.block.documentation"),
                HighlightStyle::color(TextSuccess).alpha(0.8).italic(),
            ),
            (
                mk("string.quoted.docstring"),
                HighlightStyle::color(TextSuccess).alpha(0.8).italic(),
            ),
            // Token.Comment: "$text 60%"
            (mk("comment"), HighlightStyle::color(Text).alpha(0.6)),
            // Token.Literal.String: "$text-success 90%"
            (mk("string"), HighlightStyle::color(TextSuccess).alpha(0.9)),
            // Token.Literal.Number: "$text-warning"
            (mk("constant.numeric"), HighlightStyle::color(TextWarning)),
            // Token.Keyword.Constant (True/False/None): "bold $text-success 80%"
            (
                mk("constant.language"),
                HighlightStyle::color(TextSuccess).alpha(0.8).bold(),
            ),
            // Token.Name.Function: "$text-warning underline"
            (
                mk("entity.name.function"),
                HighlightStyle::color(TextWarning).underline(),
            ),
            // Token.Name.Class: "$text-warning bold"
            (
                mk("entity.name.class"),
                HighlightStyle::color(TextWarning).bold(),
            ),
            (
                mk("entity.name.type.class"),
                HighlightStyle::color(TextWarning).bold(),
            ),
            // Token.Name.Tag: "$text-primary bold"
            (
                mk("entity.name.tag"),
                HighlightStyle::color(TextPrimary).bold(),
            ),
            // Token.Name.Attribute: "$text-warning"
            (
                mk("entity.other.attribute-name"),
                HighlightStyle::color(TextWarning),
            ),
            // Token.Name.Builtin: "$text-accent"
            (mk("support.function"), HighlightStyle::color(TextAccent)),
            (mk("support.type"), HighlightStyle::color(TextAccent)),
            (mk("support.class"), HighlightStyle::color(TextAccent)),
            // Token.Operator.Word (and/or/not/in/is): "bold $text-error"
            (
                mk("keyword.operator.logical"),
                HighlightStyle::color(TextError).bold(),
            ),
            (
                mk("keyword.operator.word"),
                HighlightStyle::color(TextError).bold(),
            ),
            // Token.Operator: "bold"
            (mk("keyword.operator"), HighlightStyle::plain().bold()),
            // pygments lexes Python's return-annotation `->` as Token.Operator
            // (bold); syntect scopes it punctuation.separator.annotation.return.
            (
                mk("punctuation.separator.annotation.return"),
                HighlightStyle::plain().bold(),
            ),
            // Token.Keyword.Namespace (import/from): "$text-error"
            (
                mk("keyword.control.import"),
                HighlightStyle::color(TextError),
            ),
            // Token.Keyword: "$text-accent"
            (mk("keyword"), HighlightStyle::color(TextAccent)),
            (mk("storage.type"), HighlightStyle::color(TextAccent)),
            (mk("storage.modifier"), HighlightStyle::color(TextAccent)),
            // Token.Name (pygments emits it for every identifier):
            // "$text-primary". TextMate only scopes SOME identifier roles; the
            // rest are handled by the identifier post-pass in
            // `highlight_lines`.
            (mk("variable.function"), HighlightStyle::color(TextPrimary)),
            (mk("variable.parameter"), HighlightStyle::color(TextPrimary)),
            // Token.Name.Variable (shell `$var`, PHP `$var`, …):
            // "$text-secondary". (pygments' python lexer never emits it.)
            (mk("variable.other"), HighlightStyle::color(TextSecondary)),
            (mk("variable"), HighlightStyle::color(TextPrimary)),
            (mk("entity.name"), HighlightStyle::color(TextPrimary)),
        ]
    })
}

fn syntax_set() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// Style for a scope stack: walk scopes innermost-first; for the first scope
/// with any matching rule, pick the most specific (longest) matching selector.
fn style_for_stack(stack: &ScopeStack) -> Option<HighlightStyle> {
    for scope in stack.as_slice().iter().rev() {
        let mut best: Option<(u32, HighlightStyle)> = None;
        for (selector, style) in rules() {
            if selector.is_prefix_of(*scope) {
                let len = selector.len();
                if best.map(|(l, _)| len > l).unwrap_or(true) {
                    best = Some((len, *style));
                }
            }
        }
        if let Some((_, style)) = best {
            return Some(style);
        }
    }
    None
}

/// pygments emulation: bare identifiers are `Token.Name` -> `$text-primary`.
/// Split an unstyled span into identifier runs (styled `$text-primary`) and
/// the rest (default `$text`).
fn split_identifiers(text: &str, out: &mut Vec<HighlightSpan>) {
    let name_style = HighlightStyle::color(HighlightColor::TextPrimary);
    let mut rest = text;
    while !rest.is_empty() {
        let start = rest
            .char_indices()
            .find(|(_, c)| c.is_alphabetic() || *c == '_')
            .map(|(i, _)| i);
        let Some(start) = start else {
            out.push(HighlightSpan {
                text: rest.to_string(),
                style: HighlightStyle::plain(),
            });
            break;
        };
        // A digit directly before an identifier char would make it part of a
        // number literal, but numbers are scoped by the grammar already.
        if start > 0 {
            out.push(HighlightSpan {
                text: rest[..start].to_string(),
                style: HighlightStyle::plain(),
            });
        }
        let tail = &rest[start..];
        let end = tail
            .char_indices()
            .find(|(_, c)| !(c.is_alphanumeric() || *c == '_'))
            .map(|(i, _)| i)
            .unwrap_or(tail.len());
        out.push(HighlightSpan {
            text: tail[..end].to_string(),
            style: name_style,
        });
        rest = &tail[end..];
    }
}

/// Highlight `code` as `language`, returning styled spans per line (no line
/// breaks inside spans, no trailing newline spans).
///
/// Unknown/empty languages fall back to plain text (every span default-styled
/// `$text`), mirroring Python's unstyled fallback when pygments cannot find a
/// lexer.
pub(crate) fn highlight_lines(code: &str, language: &str) -> Vec<Vec<HighlightSpan>> {
    let set = syntax_set();
    let syntax = (!language.trim().is_empty())
        .then(|| set.find_syntax_by_token(language.trim()))
        .flatten();
    let plain_fallback = syntax.is_none();
    let syntax = syntax.unwrap_or_else(|| set.find_syntax_plain_text());

    let mut parse_state = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut lines: Vec<Vec<HighlightSpan>> = Vec::new();

    for line in code.split_inclusive('\n') {
        let mut spans: Vec<HighlightSpan> = Vec::new();
        let visible_len = line.trim_end_matches('\n').len();
        let ops = parse_state.parse_line(line, set).unwrap_or_default();
        let mut cursor = 0usize;
        let emit = |text: &str, stack: &ScopeStack, spans: &mut Vec<HighlightSpan>| {
            if text.is_empty() {
                return;
            }
            match style_for_stack(stack) {
                Some(style) => spans.push(HighlightSpan {
                    text: text.to_string(),
                    style,
                }),
                None if plain_fallback => spans.push(HighlightSpan {
                    text: text.to_string(),
                    style: HighlightStyle::plain(),
                }),
                None => split_identifiers(text, spans),
            }
        };
        for (offset, op) in &ops {
            let offset = (*offset).min(visible_len);
            if offset > cursor {
                emit(&line[cursor..offset], &stack, &mut spans);
                cursor = offset;
            }
            let _ = stack.apply(op);
        }
        if visible_len > cursor {
            emit(&line[cursor..visible_len], &stack, &mut spans);
        }
        lines.push(spans);
    }

    // `split_inclusive` yields no entry for a trailing newline's empty last
    // line; Python's `code.splitlines()` behaves the same. A completely empty
    // input still renders one empty line.
    if lines.is_empty() {
        lines.push(Vec::new());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(lines: &[Vec<HighlightSpan>]) -> Vec<(String, HighlightStyle)> {
        lines
            .iter()
            .flatten()
            .map(|s| (s.text.clone(), s.style))
            .collect()
    }

    fn style_of<'a>(
        spans: &'a [(String, HighlightStyle)],
        text: &str,
    ) -> Option<&'a HighlightStyle> {
        spans
            .iter()
            .find(|(t, _)| t == text)
            .map(|(_, style)| style)
    }

    /// The stopwatch of gap C: the widgets/markdown demo's python fence.
    /// Empirically measured Python (pygments + HighlightTheme) renders:
    /// `def` accent, `loop_last` warning+underline, `values`/`Iterable`
    /// primary, `->` bold, docstring success 80% italic.
    #[test]
    fn python_fence_matches_pygments_token_styles() {
        let code = "def loop_last(values: Iterable[T]) -> Iterable[Tuple[bool, T]]:\n    \"\"\"Doc.\"\"\"\n    x = 1  # note\n    s = \"hi\"\n    return True\n";
        let spans = flat(&highlight_lines(code, "python"));

        let def = style_of(&spans, "def").expect("def span");
        assert_eq!(def.color, Some(HighlightColor::TextAccent), "def keyword");

        let func = style_of(&spans, "loop_last").expect("loop_last span");
        assert_eq!(func.color, Some(HighlightColor::TextWarning));
        assert!(func.underline, "function name underlined");

        let name = style_of(&spans, "values").expect("values span");
        assert_eq!(
            name.color,
            Some(HighlightColor::TextPrimary),
            "bare identifier -> Token.Name -> $text-primary"
        );

        let num = style_of(&spans, "1").expect("number span");
        assert_eq!(num.color, Some(HighlightColor::TextWarning));

        let ret = style_of(&spans, "return").expect("return span");
        assert_eq!(ret.color, Some(HighlightColor::TextAccent));

        let doc: Vec<_> = spans
            .iter()
            .filter(|(t, _)| t.contains("Doc."))
            .collect();
        assert!(!doc.is_empty(), "docstring span exists");
        assert_eq!(doc[0].1.color, Some(HighlightColor::TextSuccess));
        assert!((doc[0].1.alpha - 0.8).abs() < 1e-6, "docstring 80%");
        assert!(doc[0].1.italic);

        let comment: Vec<_> = spans.iter().filter(|(t, _)| t.contains("note")).collect();
        assert!(!comment.is_empty(), "comment span exists");
        assert_eq!(comment[0].1.color, Some(HighlightColor::Text));
        assert!((comment[0].1.alpha - 0.6).abs() < 1e-6, "comment 60%");
    }

    #[test]
    fn unknown_language_falls_back_to_plain_text() {
        let spans = flat(&highlight_lines("hello world\n", "no-such-lang"));
        assert!(spans.iter().all(|(_, s)| s.color.is_none()));
    }

    #[test]
    fn debug_dump_python_scopes() {
        // Dev aid (kept cheap): ensures highlighting never panics on the demo
        // snippet and prints the span map under --nocapture for mapping work.
        let code = "def loop_last(values: Iterable[T]) -> Iterable[Tuple[bool, T]]:\n    \"\"\"Doc.\"\"\"\n    iter_values = iter(values)\n    yield True, previous_value\n";
        for (i, line) in highlight_lines(code, "python").iter().enumerate() {
            for span in line {
                println!("{i}: {:?} -> {:?}", span.text, span.style);
            }
        }
    }
}

