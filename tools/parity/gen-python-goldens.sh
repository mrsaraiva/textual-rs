#!/usr/bin/env bash
# Generate golden screens for the real-PTY parity harness (tests/pty_parity.rs)
# from PYTHON Textual. The goldens define parity; they must never be regenerated
# from Rust output.
#
# Requirements:
#   - tmux
#   - a Python interpreter with `textual` (and `httpx` for the dictionary case)
#     installed, ideally from the local ../textual checkout:
#       uv venv /tmp/textual-venv
#       VIRTUAL_ENV=/tmp/textual-venv uv pip install -e ../textual httpx
#   - the Python Textual examples directory (default: ../textual/examples)
#
# Usage:
#   PYTHON=/tmp/textual-venv/bin/python tools/parity/gen-python-goldens.sh [case ...]
#
# Case definitions here MUST stay in sync with the manifest in tests/pty_parity.rs.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PYTHON="${PYTHON:-/tmp/textual-venv/bin/python}"
PY_EXAMPLES="${TEXTUAL_PY_EXAMPLES:-$REPO_ROOT/../textual/examples}"
PY_DOCS="${TEXTUAL_PY_DOCS:-$REPO_ROOT/../textual/docs/examples}"
GOLDEN_DIR="$REPO_ROOT/tests/pty_parity/golden"
FIXTURE_DIR="$REPO_ROOT/tests/pty_parity/fixtures/sample_dir"
SOCKET="parity-golden-$$"
COLS=120
ROWS=30

[ -x "$PYTHON" ] || { echo "error: PYTHON=$PYTHON is not executable" >&2; exit 1; }
"$PYTHON" -c 'import textual' || { echo "error: textual not importable from $PYTHON" >&2; exit 1; }
[ -d "$PY_EXAMPLES" ] || { echo "error: Python examples dir not found: $PY_EXAMPLES" >&2; exit 1; }
mkdir -p "$GOLDEN_DIR"

# Refuse to run (and auto-sweep dead sockets) if leaked tmux servers have piled
# up — prevents the socket leak from breaking tmux globally. See tmux-guard.sh.
# shellcheck source=tools/parity/tmux-guard.sh
source "$(dirname "${BASH_SOURCE[0]}")/tmux-guard.sh"
parity_tmux_guard || exit 1

tmx() { tmux -L "$SOCKET" "$@"; }
cleanup() {
    tmx kill-server 2>/dev/null || true
    rm -f "/tmp/tmux-$(id -u)/$SOCKET" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

# capture_stable <session> -> stdout (waits until two consecutive captures match)
capture_stable() {
    local session="$1" prev="" cur="" tries=0
    while [ $tries -lt 40 ]; do
        cur="$(tmx capture-pane -t "$session" -p)"
        if [ -n "$cur" ] && [ "$cur" = "$prev" ]; then
            printf '%s\n' "$cur"
            return 0
        fi
        prev="$cur"
        tries=$((tries + 1))
        sleep 0.25
    done
    echo "error: screen for $session did not stabilize" >&2
    return 1
}

# run_case <name> <cwd> <keys> <script...>
run_case() {
    local name="$1" workdir="$2" keys="$3"
    shift 3
    local session="g_$name"
    echo "==> $name"
    tmx kill-session -t "$session" 2>/dev/null || true
    tmx new-session -d -s "$session" -x "$COLS" -y "$ROWS" \
        "cd '$workdir' && '$PYTHON' $*"
    capture_stable "$session" >/dev/null
    if [ -n "$keys" ]; then
        tmx send-keys -t "$session" "$keys"
        sleep 0.5
        capture_stable "$session" >/dev/null
    fi
    tmx capture-pane -t "$session" -p | sed -e 's/[[:space:]]*$//' \
        > "$GOLDEN_DIR/$name.txt"
    tmx kill-session -t "$session" 2>/dev/null || true
    echo "    wrote golden/$name.txt"
}

want() {
    [ $# -eq 0 ] && return 0
    local c
    for c in "$@"; do [ "$c" = "$CASE" ] && return 0; done
    return 1
}

CASE=markdown_initial      && want "$@" && run_case "$CASE" "$PY_EXAMPLES" ""  "markdown.py"
CASE=markdown_toc_toggle   && want "$@" && run_case "$CASE" "$PY_EXAMPLES" "t" "markdown.py"
CASE=five_by_five_initial  && want "$@" && run_case "$CASE" "$PY_EXAMPLES" ""  "five_by_five.py"
CASE=json_tree_initial     && want "$@" && run_case "$CASE" "$PY_EXAMPLES" ""  "json_tree.py"
CASE=json_tree_add_node    && want "$@" && run_case "$CASE" "$PY_EXAMPLES" "a" "json_tree.py"
CASE=dictionary_initial    && want "$@" && run_case "$CASE" "$PY_EXAMPLES" ""  "dictionary.py"
CASE=code_browser_initial  && want "$@" && run_case "$CASE" "$FIXTURE_DIR" ""  "$PY_EXAMPLES/code_browser.py" "./"

# Docs examples (../textual/docs/examples/<group>/<name>.py). Golden name is the
# manifest case name (docs_<name>); the manifest's `example` field is <name>.
CASE=docs_center02       && want "$@" && run_case "$CASE" "$PY_DOCS/how-to" "" "center02.py"
CASE=docs_center03       && want "$@" && run_case "$CASE" "$PY_DOCS/how-to" "" "center03.py"
CASE=docs_center04       && want "$@" && run_case "$CASE" "$PY_DOCS/how-to" "" "center04.py"
CASE=docs_center06       && want "$@" && run_case "$CASE" "$PY_DOCS/how-to" "" "center06.py"
CASE=docs_center07       && want "$@" && run_case "$CASE" "$PY_DOCS/how-to" "" "center07.py"
CASE=docs_center08       && want "$@" && run_case "$CASE" "$PY_DOCS/how-to" "" "center08.py"
CASE=docs_center09       && want "$@" && run_case "$CASE" "$PY_DOCS/how-to" "" "center09.py"
CASE=docs_center10       && want "$@" && run_case "$CASE" "$PY_DOCS/how-to" "" "center10.py"
CASE=docs_containers01 && want "$@" && run_case "$CASE" "$PY_DOCS/how-to" "" "containers01.py"
CASE=docs_containers02 && want "$@" && run_case "$CASE" "$PY_DOCS/how-to" "" "containers02.py"
CASE=docs_containers03 && want "$@" && run_case "$CASE" "$PY_DOCS/how-to" "" "containers03.py"
CASE=docs_containers04 && want "$@" && run_case "$CASE" "$PY_DOCS/how-to" "" "containers04.py"
CASE=docs_containers05 && want "$@" && run_case "$CASE" "$PY_DOCS/how-to" "" "containers05.py"
CASE=docs_containers07 && want "$@" && run_case "$CASE" "$PY_DOCS/how-to" "" "containers07.py"
CASE=docs_containers08 && want "$@" && run_case "$CASE" "$PY_DOCS/how-to" "" "containers08.py"
CASE=docs_containers09 && want "$@" && run_case "$CASE" "$PY_DOCS/how-to" "" "containers09.py"
CASE=docs_layout02 && want "$@" && run_case "$CASE" "$PY_DOCS/how-to" "" "layout02.py"
CASE=docs_layout03 && want "$@" && run_case "$CASE" "$PY_DOCS/how-to" "" "layout03.py"
CASE=docs_layout04 && want "$@" && run_case "$CASE" "$PY_DOCS/how-to" "" "layout04.py"
CASE=docs_muted_backgrounds && want "$@" && run_case "$CASE" "$PY_DOCS/themes" "" "muted_backgrounds.py"
CASE=docs_render_compose && want "$@" && run_case "$CASE" "$PY_DOCS/how-to" "" "render_compose.py"
CASE=docs_layout01 && want "$@" && run_case "$CASE" "$PY_DOCS/how-to" "" "layout01.py"
CASE=docs_layout06 && want "$@" && run_case "$CASE" "$PY_DOCS/how-to" "" "layout06.py"
CASE=docs_layout05 && want "$@" && run_case "$CASE" "$PY_DOCS/how-to" "" "layout05.py"
CASE=docs_containers06 && want "$@" && run_case "$CASE" "$PY_DOCS/how-to" "" "containers06.py"
CASE=docs_modal01 && want "$@" && run_case "$CASE" "$PY_DOCS/guide/screens" "" "modal01.py"
CASE=docs_modal02 && want "$@" && run_case "$CASE" "$PY_DOCS/guide/screens" "" "modal02.py"
CASE=docs_modal03 && want "$@" && run_case "$CASE" "$PY_DOCS/guide/screens" "" "modal03.py"
CASE=docs_align_all && want "$@" && run_case "$CASE" "$PY_DOCS/styles" "" "align_all.py"
CASE=docs_background && want "$@" && run_case "$CASE" "$PY_DOCS/styles" "" "background.py"
CASE=docs_background_tint && want "$@" && run_case "$CASE" "$PY_DOCS/styles" "" "background_tint.py"
CASE=docs_background_transparency && want "$@" && run_case "$CASE" "$PY_DOCS/styles" "" "background_transparency.py"
CASE=docs_border && want "$@" && run_case "$CASE" "$PY_DOCS/styles" "" "border.py"
CASE=docs_border01 && want "$@" && run_case "$CASE" "$PY_DOCS/guide/styles" "" "border01.py"
CASE=docs_box_sizing01 && want "$@" && run_case "$CASE" "$PY_DOCS/guide/styles" "" "box_sizing01.py"
CASE=docs_color && want "$@" && run_case "$CASE" "$PY_DOCS/styles" "" "color.py"
CASE=docs_color_auto && want "$@" && run_case "$CASE" "$PY_DOCS/styles" "" "color_auto.py"
CASE=docs_colors && want "$@" && run_case "$CASE" "$PY_DOCS/guide/styles" "" "colors.py"
CASE=docs_colors01 && want "$@" && run_case "$CASE" "$PY_DOCS/guide/styles" "" "colors01.py"
CASE=docs_colors02 && want "$@" && run_case "$CASE" "$PY_DOCS/guide/styles" "" "colors02.py"
CASE=docs_content_align_all && want "$@" && run_case "$CASE" "$PY_DOCS/styles" "" "content_align_all.py"
CASE=docs_dimensions01 && want "$@" && run_case "$CASE" "$PY_DOCS/guide/styles" "" "dimensions01.py"
CASE=docs_dimensions02 && want "$@" && run_case "$CASE" "$PY_DOCS/guide/styles" "" "dimensions02.py"
CASE=docs_dimensions03 && want "$@" && run_case "$CASE" "$PY_DOCS/guide/styles" "" "dimensions03.py"
CASE=docs_dimensions04 && want "$@" && run_case "$CASE" "$PY_DOCS/guide/styles" "" "dimensions04.py"
CASE=docs_grid && want "$@" && run_case "$CASE" "$PY_DOCS/styles" "" "grid.py"
CASE=docs_grid_columns && want "$@" && run_case "$CASE" "$PY_DOCS/styles" "" "grid_columns.py"
CASE=docs_grid_gutter && want "$@" && run_case "$CASE" "$PY_DOCS/styles" "" "grid_gutter.py"
CASE=docs_grid_layout1 && want "$@" && run_case "$CASE" "$PY_DOCS/guide/layout" "" "grid_layout1.py"
CASE=docs_grid_layout2 && want "$@" && run_case "$CASE" "$PY_DOCS/guide/layout" "" "grid_layout2.py"
CASE=docs_grid_layout3_row_col_adjust && want "$@" && run_case "$CASE" "$PY_DOCS/guide/layout" "" "grid_layout3_row_col_adjust.py"
CASE=docs_grid_layout5_col_span && want "$@" && run_case "$CASE" "$PY_DOCS/guide/layout" "" "grid_layout5_col_span.py"
CASE=docs_grid_layout6_row_span && want "$@" && run_case "$CASE" "$PY_DOCS/guide/layout" "" "grid_layout6_row_span.py"
CASE=docs_grid_layout7_gutter && want "$@" && run_case "$CASE" "$PY_DOCS/guide/layout" "" "grid_layout7_gutter.py"
CASE=docs_grid_size_both && want "$@" && run_case "$CASE" "$PY_DOCS/styles" "" "grid_size_both.py"
CASE=docs_grid_size_columns && want "$@" && run_case "$CASE" "$PY_DOCS/styles" "" "grid_size_columns.py"
CASE=docs_horizontal_layout && want "$@" && run_case "$CASE" "$PY_DOCS/guide/layout" "" "horizontal_layout.py"
CASE=docs_link_style_hover && want "$@" && run_case "$CASE" "$PY_DOCS/styles" "" "link_style_hover.py"
CASE=docs_margin && want "$@" && run_case "$CASE" "$PY_DOCS/styles" "" "margin.py"
CASE=docs_margin01 && want "$@" && run_case "$CASE" "$PY_DOCS/guide/styles" "" "margin01.py"
CASE=docs_margin_all && want "$@" && run_case "$CASE" "$PY_DOCS/styles" "" "margin_all.py"
CASE=docs_padding && want "$@" && run_case "$CASE" "$PY_DOCS/styles" "" "padding.py"
CASE=docs_padding01 && want "$@" && run_case "$CASE" "$PY_DOCS/guide/styles" "" "padding01.py"
CASE=docs_screen && want "$@" && run_case "$CASE" "$PY_DOCS/guide/styles" "" "screen.py"
CASE=docs_scrollbar_gutter && want "$@" && run_case "$CASE" "$PY_DOCS/styles" "" "scrollbar_gutter.py"
CASE=docs_text_overflow && want "$@" && run_case "$CASE" "$PY_DOCS/styles" "" "text_overflow.py"
CASE=docs_text_style_all && want "$@" && run_case "$CASE" "$PY_DOCS/styles" "" "text_style_all.py"
CASE=docs_text_wrap && want "$@" && run_case "$CASE" "$PY_DOCS/styles" "" "text_wrap.py"
CASE=docs_tint && want "$@" && run_case "$CASE" "$PY_DOCS/styles" "" "tint.py"
CASE=docs_vertical_layout && want "$@" && run_case "$CASE" "$PY_DOCS/guide/layout" "" "vertical_layout.py"
CASE=docs_vertical_layout_scrolled && want "$@" && run_case "$CASE" "$PY_DOCS/guide/layout" "" "vertical_layout_scrolled.py"
CASE=docs_visibility && want "$@" && run_case "$CASE" "$PY_DOCS/styles" "" "visibility.py"
CASE=docs_widget && want "$@" && run_case "$CASE" "$PY_DOCS/guide/styles" "" "widget.py"
# NOTE: app examples event01 (bg-color only), simple01/02 (empty App), and
# widgets02/03/04 (Welcome mounts only on keypress) have EMPTY initial plain-text
# screens, so they aren't strict-harness cases (the harness needs a non-empty
# stable screen). They remain faithful ports under docs/examples/app.
CASE=docs_question01     && want "$@" && run_case "$CASE" "$PY_DOCS/app"    "" "question01.py"
CASE=docs_question02     && want "$@" && run_case "$CASE" "$PY_DOCS/app"    "" "question02.py"
CASE=docs_question03     && want "$@" && run_case "$CASE" "$PY_DOCS/app"    "" "question03.py"
CASE=docs_question_title01 && want "$@" && run_case "$CASE" "$PY_DOCS/app"  "" "question_title01.py"
CASE=docs_question_title02 && want "$@" && run_case "$CASE" "$PY_DOCS/app"  "" "question_title02.py"
# Widgets group (../textual/docs/examples/widgets/<name>.py).
CASE=docs_button         && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "button.py"
CASE=docs_checkbox       && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "checkbox.py"
CASE=docs_log            && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "log.py"
CASE=docs_rich_log       && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "rich_log.py"
CASE=docs_option_list_options && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "option_list_options.py"
CASE=docs_list_view      && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "list_view.py"
CASE=docs_text_area_example && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "text_area_example.py"
CASE=docs_text_area_selection && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "text_area_selection.py"
CASE=docs_select_widget_no_blank && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "select_widget_no_blank.py"
CASE=docs_data_table_labels && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "data_table_labels.py"
CASE=docs_data_table_renderables && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "data_table_renderables.py"
CASE=docs_data_table_fixed && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "data_table_fixed.py"
CASE=docs_collapsible && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "collapsible.py"
CASE=docs_collapsible_nested && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "collapsible_nested.py"
CASE=docs_collapsible_custom_symbol && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "collapsible_custom_symbol.py"
CASE=docs_progress_bar   && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "progress_bar.py"
CASE=docs_progress_bar_gradient && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "progress_bar_gradient.py"
CASE=docs_sparkline_colors && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "sparkline_colors.py"
CASE=docs_horizontal_rules && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "horizontal_rules.py"
CASE=docs_vertical_rules && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "vertical_rules.py"
CASE=docs_suspend        && want "$@" && run_case "$CASE" "$PY_DOCS/app"    "" "suspend.py"
CASE=docs_suspend_process && want "$@" && run_case "$CASE" "$PY_DOCS/app"   "" "suspend_process.py"

CASE=docs_prevent        && want "$@" && run_case "$CASE" "$PY_DOCS/events" "" "prevent.py"
CASE=docs_on_decorator01 && want "$@" && run_case "$CASE" "$PY_DOCS/events" "" "on_decorator01.py"
CASE=docs_on_decorator02 && want "$@" && run_case "$CASE" "$PY_DOCS/events" "" "on_decorator02.py"
CASE=docs_colored_text   && want "$@" && run_case "$CASE" "$PY_DOCS/themes" "" "colored_text.py"
CASE=docs_stopwatch01    && want "$@" && run_case "$CASE" "$PY_DOCS/tutorial" "" "stopwatch01.py"
CASE=docs_stopwatch02    && want "$@" && run_case "$CASE" "$PY_DOCS/tutorial" "" "stopwatch02.py"

CASE=docs_stopwatch03    && want "$@" && run_case "$CASE" "$PY_DOCS/tutorial" "" "stopwatch03.py"
CASE=docs_stopwatch04    && want "$@" && run_case "$CASE" "$PY_DOCS/tutorial" "" "stopwatch04.py"
CASE=docs_option_list_tables && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "option_list_tables.py"

echo "done."
