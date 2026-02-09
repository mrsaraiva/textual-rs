// Base/shared widget defaults: ScrollView, Label, Spacer

pub(super) const DEFAULT_CSS: &str = r#"
ScrollView > .scrollview--content { transition: scrollview.offset 140ms ease-out; }

Label { fg: $foreground; }
Spacer { bg: $background; }
"#;
