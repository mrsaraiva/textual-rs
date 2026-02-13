// Header and Footer widget defaults

pub(super) const DEFAULT_CSS: &str = r#"
Header {
    dock: top;
    width: 100%;
    bg: $panel;
    fg: $foreground;
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
    layout: horizontal;
    dock: bottom;
    bg: $footer-background;
    fg: $footer-foreground;
    line-pad: 1;
    height: auto;
}

Footer > .footer-key--key {
    fg: $footer-key-foreground;
    bg: $footer-key-background;
    text-style: bold;
}

Footer > .footer-key--description {
    fg: $footer-description-foreground;
    bg: $footer-description-background;
}

Footer > .footer-key--command-palette {
    fg: $text-muted;
}
"#;
