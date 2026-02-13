// Collapsible widget defaults

pub(super) const DEFAULT_CSS: &str = r#"
Collapsible {
    border-top: hkey $border-blurred;
    padding: 0 0 1 1;
}

Collapsible:focus { border-top: hkey $border; }

Collapsible > .collapsible--title { fg: $foreground; text-style: bold; }

Collapsible.-collapsed > Contents { display: none; }
"#;
