// DataTable widget defaults
//
// Simplified from Python Textual's DataTable.DEFAULT_CSS.
// The Rust DataTable currently handles cursor/hover/fixed styling
// directly in its render method using theme tokens, so we only set
// base surface and layout properties here.

pub(super) const DEFAULT_CSS: &str = r#"
DataTable {
    bg: $surface;
    fg: $foreground;
    height: auto;
    max-height: 100%;
}
"#;
