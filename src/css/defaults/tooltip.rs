// Tooltip widget defaults
// DC-26: aligned with Python Textual _tooltip.py DEFAULT_CSS
//
// Rust adaptation notes:
// - `overlay: screen` + `position: absolute` route the tooltip through the
//   shared `overlay: screen` deferred-paint escape (the mechanism that already
//   honors `constrain: inside inflect` and floats a node at the top z of the
//   screen with no clip — proven by Select). Python achieves the same top-z
//   float via the `_tooltips` LAYER; in Rust named layers give z-order only and
//   `constrain` is honored solely on the overlay:screen path, so the tooltip
//   rides that path. `layer: _tooltips` is retained for cascade fidelity (moot
//   under overlay:screen).
// - `position: absolute` lets the runtime anchor the bubble at the mouse via the
//   node's `absolute_offset` (Python `Widget._absolute_offset`); CSS `offset-x:
//   -50%` then centers it on the anchor and `margin: 1 0` gives the vertical gap.
// - Python's `display: none` default is replaced by runtime-display gating: the
//   system tooltip node is mounted with `runtime_display = false` and the hover
//   path toggles it, so no inline display override is needed.

pub(super) const DEFAULT_CSS: &str = r#"
Tooltip {
    layer: _tooltips;
    overlay: screen;
    position: absolute;
    margin: 1 0;
    padding: 1 2;
    bg: $panel;
    width: auto;
    height: auto;
    constrain: inside inflect;
    max-width: 40;
    offset-x: -50%;
}
"#;
