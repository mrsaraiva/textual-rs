use crate::validation::ValidationResult;
use crate::widgets::WidgetId;

#[derive(Debug, Clone)]
pub enum Message {
    ClearRequested,
    InputChanged {
        value: String,
        validation: ValidationResult,
    },
    InputSubmitted {
        value: String,
    },
    ButtonPressed {
        description: String,
    },
    CheckboxChanged {
        checked: bool,
    },
    DataTableCursorMoved {
        row: usize,
        column: usize,
    },
    DataTableHeaderSelected {
        column: usize,
    },
    DataTableCellActivated {
        row: usize,
        column: usize,
    },
}

#[derive(Debug, Clone)]
pub struct MessageEvent {
    pub sender: WidgetId,
    pub message: Message,
}
