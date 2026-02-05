//! Textual-inspired reactive TUI framework built on rich-rs.

pub mod debug;
pub mod demo_snapshot;
pub mod driver;
mod error;
pub mod css;
pub mod event;
pub mod render;
pub mod runtime;
pub mod style;
pub mod validation;
pub mod widgets;

pub use error::{Error, Result};
pub use runtime::App;
pub use style::{Color, Style, Theme};
pub use widgets::WidgetStyles;

pub mod prelude {
    pub use crate::debug::DebugLayout;
    pub use crate::css::{StyleSelector, StyleSheet, set_style_context};
    pub use crate::event::{Action, ActionMap, Event, EventCtx, KeyBind};
    pub use crate::runtime::App;
    pub use crate::style::{Color, Style, Theme};
    pub use crate::validation::{Function, Number, ValidationResult, Validator, ValidatorRef};
    pub use crate::widgets::{
        AppRoot, Button, ButtonVariant, Checkbox, Constrained, Container, CursorType, DataTable,
        Dock, Frame,
        Grid, Horizontal, Input, InputType, Label, LayoutConstraints, ListView, Markdown, Node,
        Overlay,
        Panel, Pretty, Row, RowAlign, ScrollView, Spacer, Static, Styled, Tabs, TextArea,
        TextAreaCursor, TextAreaSelection, TextAreaTheme,
        Tree, TreeNode, VerticalScroll, Widget, WidgetId, WidgetRenderable, WidgetStyles,
    };
    pub use crate::{Error, Result};
}
