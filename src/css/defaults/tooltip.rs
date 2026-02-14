// Tooltip widget defaults
// DC-26: aligned with Python Textual _tooltip.py DEFAULT_CSS

pub(super) const DEFAULT_CSS: &str = r#"
Tooltip {
    layer: _tooltips;
    margin: 1 0;
    padding: 1 2;
    bg: $panel;
    width: auto;
    height: auto;
    constrain: inside inflect;
    max-width: 40;
    display: none;
}
"#;
