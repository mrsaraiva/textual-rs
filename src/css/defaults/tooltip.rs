// Tooltip widget defaults

pub(super) const DEFAULT_CSS: &str = r#"
Tooltip {
    layer: _tooltips;
    margin: 1 0;
    padding: 1 2;
    bg: $panel;
    fg: $foreground;
    width: auto;
    height: auto;
    max-width: 40;
    constrain: inside;
}
"#;
