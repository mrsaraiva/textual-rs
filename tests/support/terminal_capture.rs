use rich_rs::{Console, ControlType, Segments};

pub fn capture_segments_raw_bytes(segments: &Segments) -> Vec<u8> {
    let mut console = Console::capture();
    console
        .print_segments(segments)
        .expect("captured console should accept segment output");
    console.get_captured_bytes().to_vec()
}

pub fn escape_terminal_bytes(bytes: &[u8]) -> String {
    let mut out = String::new();
    for &byte in bytes {
        match byte {
            b'\x1b' => out.push_str("\\x1b"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x20..=0x7e => out.push(byte as char),
            _ => out.push_str(&format!("\\x{byte:02x}")),
        }
    }
    out
}

pub fn summarize_segment_stream(segments: &Segments) -> String {
    let mut lines: Vec<String> = Vec::with_capacity(segments.len());
    for (idx, segment) in segments.iter().enumerate() {
        let entry = if let Some(control) = segment.control.as_ref() {
            format!("{idx}: CTRL {}", control_label(control))
        } else {
            format!("{idx}: TEXT {:?}", segment.text.as_ref())
        };
        lines.push(entry);
    }
    lines.join("\n")
}

pub fn assert_no_relative_cursor_controls(segments: &Segments) {
    for segment in segments.iter() {
        let Some(control) = segment.control.as_ref() else {
            continue;
        };
        match control {
            ControlType::CarriageReturn
            | ControlType::CursorDown(_)
            | ControlType::CursorUp(_)
            | ControlType::CursorForward(_)
            | ControlType::CursorBackward(_) => {
                panic!("segment stream used relative cursor control: {control:?}");
            }
            _ => {}
        }
    }
}

fn control_label(control: &ControlType) -> String {
    match control {
        ControlType::Home => "Home".to_string(),
        ControlType::Clear => "Clear".to_string(),
        ControlType::CarriageReturn => "CR".to_string(),
        ControlType::CursorUp(n) => format!("Up({n})"),
        ControlType::CursorDown(n) => format!("Down({n})"),
        ControlType::CursorForward(n) => format!("Right({n})"),
        ControlType::CursorBackward(n) => format!("Left({n})"),
        ControlType::MoveTo { x, y } => format!("MoveTo({x},{y})"),
        ControlType::EraseInLine(mode) => format!("EraseInLine({mode})"),
        ControlType::ShowCursor => "ShowCursor".to_string(),
        ControlType::HideCursor => "HideCursor".to_string(),
        _ => format!("{control:?}"),
    }
}
