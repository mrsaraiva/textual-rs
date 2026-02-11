use rich_rs::{ControlType, Segment};
use textual::render::FrameBuffer;

mod support;
use support::terminal_capture::{
    assert_no_relative_cursor_controls, capture_segments_raw_bytes, escape_terminal_bytes,
    summarize_segment_stream,
};

#[test]
fn sparse_frame_update_has_absolute_cursored_deterministic_output() {
    let previous = FrameBuffer::from_lines(
        &[vec![Segment::new("abcdef")], vec![Segment::new("ghijkl")]],
        6,
        2,
        None,
    );
    let next = FrameBuffer::from_lines(
        &[vec![Segment::new("abZdef")], vec![Segment::new("ghijXl")]],
        6,
        2,
        None,
    );

    let diff = next.diff_to_segments(&previous);
    assert_no_relative_cursor_controls(&diff);

    let first_control = diff.iter().next().and_then(|seg| seg.control.as_ref());
    assert!(
        matches!(first_control, Some(ControlType::Home)),
        "expected Home as first control, got {first_control:?}"
    );

    insta::assert_snapshot!(
        "sparse_frame_update_segment_stream",
        summarize_segment_stream(&diff)
    );

    let raw = capture_segments_raw_bytes(&diff);
    insta::assert_snapshot!(
        "sparse_frame_update_raw_output",
        escape_terminal_bytes(&raw)
    );
}

#[test]
fn identical_frames_emit_home_only_raw_output() {
    let previous = FrameBuffer::from_lines(
        &[vec![Segment::new("hello")], vec![Segment::new("world")]],
        5,
        2,
        None,
    );
    let next = previous.clone();

    let diff = next.diff_to_segments(&previous);
    assert_eq!(diff.len(), 1, "identical frames should emit only Home");
    let control = diff.iter().next().and_then(|seg| seg.control.as_ref());
    assert!(
        matches!(control, Some(ControlType::Home)),
        "expected Home for identical frame diff, got {control:?}"
    );

    let raw = capture_segments_raw_bytes(&diff);
    insta::assert_snapshot!(
        "identical_frames_home_only_raw_output",
        escape_terminal_bytes(&raw)
    );
}
