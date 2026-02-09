// Header and Footer widget defaults

pub(super) const DEFAULT_CSS: &str = r#"
Header {
    bg: $primary;
    fg: $text;
    text-style: bold;
    line-pad: 1;
    height: auto;
}

Header.-tall {
    height: 3;
}

Header > .header--icon {
    fg: $text-muted;
}

Header > .header--icon.-hover {
    bg: $foreground;
    fg: $text;
}

Header > .header--title {
    fg: $text-muted;
}

Header > .header--clock {
    fg: $text-disabled;
}

Footer {
    bg: $panel;
    fg: $foreground;
    line-pad: 1;
    height: auto;
}

Footer > .footer-key--key {
    fg: $accent;
    text-style: bold;
}

Footer > .footer-key--description {
    fg: $text-disabled;
}

Footer > .footer-key--command-palette {
    fg: $text-muted;
}
"#;
