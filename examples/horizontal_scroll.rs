use textual::prelude::*;

struct HorizontalScrollApp;

impl TextualApp for HorizontalScrollApp {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(
            Container::new()
                .with_child(Label::new(
                    "Manual QA: test horizontal and vertical scroll clamping.",
                ))
                .with_child(Label::new(
                    "Keys: h/l or Left/Right (X), j/k or Up/Down (Y), PageUp/PageDown.",
                ))
                .with_child(Label::new(
                    "Mouse: wheel over a panel; press q or Esc to quit.",
                ))
                .with_child(
                    Panel::new(
                        HorizontalScroll::new()
                            .with_child(
                                Container::new()
                                    .with_child(Static::new(
                                        "alpha-bravo-charlie-delta-echo-foxtrot-golf-hotel-india-juliet",
                                    ))
                                    .with_child(Static::new(
                                        "0123456789_abcdefghijklmnopqrstuvwxyz_ABCDEFGHIJKLMNOPQRSTUVWXYZ",
                                    ))
                                    .with_child(Static::new(
                                        "emoji-width-check: -> one two three four five six seven eight nine ten",
                                    )),
                            )
                            .height(4)
                            .scroll_step_x(4),
                    )
                    .title("HorizontalScroll (horizontal only)")
                    .padding(1),
                )
                .with_child(
                    Panel::new(
                        ScrollView::new(
                            Container::new()
                                .with_child(Static::new(
                                    "row 01: alpha-bravo-charlie-delta-echo-foxtrot-golf-hotel-india-juliet",
                                ))
                                .with_child(Static::new(
                                    "row 02: 0123456789_abcdefghijklmnopqrstuvwxyz_ABCDEFGHIJKLMNOPQRSTUVWXYZ",
                                ))
                                .with_child(Static::new(
                                    "row 03: the quick brown fox jumps over the lazy dog and keeps running",
                                ))
                                .with_child(Static::new(
                                    "row 04: left/right or h/l for horizontal, up/down or j/k for vertical",
                                ))
                                .with_child(Static::new(
                                    "row 05: page keys should jump by viewport while staying clamped",
                                ))
                                .with_child(Static::new(
                                    "row 06: use mouse wheel over this panel to validate routed scroll events",
                                ))
                                .with_child(Static::new(
                                    "row 07: if your terminal supports wheel-left/right, horizontal should use it",
                                ))
                                .with_child(Static::new(
                                    "row 08: otherwise wheel up/down over horizontal-only panel maps to X scroll",
                                ))
                                .with_child(Static::new(
                                    "row 09: extra rows guarantee vertical overflow for wheel and page key QA",
                                ))
                                .with_child(Static::new(
                                    "row 10: vertical offset should clamp at content boundaries",
                                ))
                                .with_child(Static::new(
                                    "row 11: horizontal offset should also clamp at max content width",
                                ))
                                .with_child(Static::new(
                                    "row 12: combine horizontal and vertical moves to test 2D scrolling",
                                )),
                        )
                        .height(6)
                        .scroll_step(1)
                        .scroll_step_x(4),
                    )
                    .title("ScrollView (vertical + horizontal)")
                    .padding(1),
                ),
        )
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync(HorizontalScrollApp)
}
