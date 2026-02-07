//! Textual-inspired reactive TUI framework built on rich-rs.

pub mod animation;
pub mod css;
pub mod debug;
pub mod demo_snapshot;
pub mod driver;
mod error;
pub mod event;
pub mod keys;
pub mod message;
pub mod render;
pub mod runtime;
pub mod style;
pub mod validation;
pub mod widgets;

pub use error::{Error, Result};
pub use event::BindingHint;
pub use keys::KeyEventData;
pub use runtime::App;
pub use style::{Color, Style, Theme};
pub use widgets::WidgetStyles;

pub mod prelude {
    pub use crate::animation::{Animator, animation_level_from_env};
    pub use crate::css::{StyleSelector, StyleSheet, set_style_context};
    pub use crate::debug::DebugLayout;
    pub use crate::event::{Action, ActionMap, BindingHint, Event, EventCtx, KeyBind};
    pub use crate::keys::{KeyEventData, format_key_display, key_to_identifier};
    pub use crate::message::{Message, MessageEvent};
    pub use crate::runtime::App;
    pub use crate::style::{Color, Style, Theme};
    pub use crate::validation::{Function, Number, ValidationResult, Validator, ValidatorRef};
    pub use crate::widgets::{
        AppRoot, BindingsTable, Button, ButtonVariant, Checkbox, Collapsible, CommandPalette,
        Constrained, Container, ContentSwitcher, CursorType, DataTable, Dock, Footer,
        FooterBinding, Frame, Grid, Header, Horizontal, HorizontalScroll, Input, InputType,
        KeyPanel, Label, LayoutConstraints, LineStyle, Link, ListView, Markdown, Node, OptionItem,
        OptionList, Overlay, PaletteCommand, Panel, Placeholder, PlaceholderVariant, Pretty,
        ProgressBar, RadioButton, RadioSet, RichLog, Row, RowAlign, Rule, RuleOrientation,
        ScrollView, Select, Selection, SelectionList, Spacer, Static, Styled, Switch, TabPane,
        TabbedContent, Tabs, TextArea, TextAreaCursor, TextAreaSelection, TextAreaTheme, Toast,
        ToastSeverity, Tree, TreeNode, VerticalScroll, Widget, WidgetId, WidgetRenderable,
        WidgetStyles, preview_root, preview_root_with_bottom, preview_root_with_top_bottom,
    };
    pub use crate::{Error, Result};
}
