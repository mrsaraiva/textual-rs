// Container/layout widget defaults (layout direction, sizing, overflow).
//
// These entries match the Python Textual defaults for Container, Horizontal, Vertical,
// HorizontalGroup, VerticalGroup, HorizontalScroll, VerticalScroll,
// ScrollableContainer, Center, CenterMiddle, Middle, Right, Grid, and ItemGrid.

pub(super) const DEFAULT_CSS: &str = r#"
Container {
    width: 1fr;
    height: 1fr;
    layout: vertical;
    overflow: hidden;
}

ScrollableContainer {
    width: 1fr;
    height: 1fr;
    layout: vertical;
    overflow: auto;
}

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

Center {
    align-horizontal: center;
    width: 1fr;
    height: auto;
}

Middle {
    align-vertical: middle;
    width: auto;
    height: 1fr;
}

CenterMiddle {
    align: center middle;
    width: 1fr;
    height: 1fr;
}

Right {
    align-horizontal: right;
    width: 1fr;
    height: auto;
}

Grid {
    width: 1fr;
    height: 1fr;
    layout: grid;
}

ItemGrid {
    width: 1fr;
    height: auto;
    layout: grid;
}

Row {
    layout: horizontal;
    width: 1fr;
    height: auto;
}
"#;
