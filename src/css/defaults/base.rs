// Base/shared widget defaults: Screen, ScrollView, Label, Spacer

pub(super) const DEFAULT_CSS: &str = r#"
Screen {
    layout: vertical;
    overflow: auto;
}

ScrollView {
    overflow: auto;
}

ScrollView > .scrollview--content { transition: scrollview.offset 140ms ease-out; }

Label { fg: $foreground; }
Spacer { bg: $background; }
"#;
