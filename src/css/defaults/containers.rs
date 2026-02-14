// Container/layout widget defaults (layout direction, sizing, overflow).
//
// These entries match the Python Textual defaults for Horizontal, Vertical,
// HorizontalGroup, VerticalGroup, HorizontalScroll, VerticalScroll,
// ScrollableContainer, Center, CenterMiddle, Middle, and Right.

pub(super) const DEFAULT_CSS: &str = r#"
Horizontal {
    width: 1fr;
    height: 1fr;
    layout: horizontal;
    overflow: hidden;
}

HorizontalGroup {
    width: 1fr;
    height: auto;
    layout: horizontal;
    overflow: hidden;
}

HorizontalScroll {
    layout: horizontal;
    overflow-y: hidden;
    overflow-x: auto;
}

Vertical {
    width: 1fr;
    height: 1fr;
    layout: vertical;
    overflow: hidden;
}

VerticalGroup {
    width: 1fr;
    height: auto;
    layout: vertical;
    overflow: hidden;
}

VerticalScroll {
    layout: vertical;
    overflow-x: hidden;
    overflow-y: auto;
}

ScrollableContainer {
    layout: vertical;
    overflow-y: auto;
    overflow-x: auto;
}

Center {
    align: center top;
    width: 1fr;
    height: 1fr;
}

Middle {
    align: left middle;
    width: 1fr;
    height: 1fr;
}

CenterMiddle {
    align: center middle;
    width: 1fr;
    height: 1fr;
}

Right {
    align: right top;
    width: 1fr;
    height: 1fr;
}

Container {
    width: 1fr;
    height: 1fr;
    layout: vertical;
}

Row {
    layout: horizontal;
    width: 1fr;
    height: auto;
}
"#;
