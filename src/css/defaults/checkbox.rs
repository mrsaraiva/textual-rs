// Checkbox widget defaults

pub(super) const DEFAULT_CSS: &str = r#"
Checkbox {
    width: auto;
    height: auto;
    fg: $foreground;
}

Checkbox:focus { background-tint: $foreground 5%; }
Checkbox:hover { background-tint: $foreground 3%; }
Checkbox:active { background-tint: $foreground 8%; }
Checkbox:disabled { dim: true; }
"#;
