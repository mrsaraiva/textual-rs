// Collapsible widget defaults

pub(super) const DEFAULT_CSS: &str = r#"
Collapsible {
    border-top: hkey $border-blurred;
    padding-bottom: 1;
    padding-left: 1;
}

Collapsible:focus { border-top: hkey $border; }

Collapsible > .collapsible--title { fg: $foreground; text-style: bold; }

Collapsible.-collapsed > Contents { display: none; }
"#;
