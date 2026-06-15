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

tmx() { tmux -L "$SOCKET" "$@"; }
cleanup() { tmx kill-server 2>/dev/null || true; }
trap cleanup EXIT

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
CASE=docs_text_area_example && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "text_area_example.py"
CASE=docs_text_area_selection && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "text_area_selection.py"
CASE=docs_data_table_labels && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "data_table_labels.py"
CASE=docs_data_table_renderables && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "data_table_renderables.py"
CASE=docs_data_table_fixed && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "data_table_fixed.py"
CASE=docs_collapsible && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "collapsible.py"
CASE=docs_collapsible_nested && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "collapsible_nested.py"
CASE=docs_collapsible_custom_symbol && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "collapsible_custom_symbol.py"
CASE=docs_progress_bar   && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "progress_bar.py"
CASE=docs_progress_bar_gradient && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "progress_bar_gradient.py"
CASE=docs_sparkline_colors && want "$@" && run_case "$CASE" "$PY_DOCS/widgets" "" "sparkline_colors.py"
CASE=docs_suspend        && want "$@" && run_case "$CASE" "$PY_DOCS/app"    "" "suspend.py"
CASE=docs_suspend_process && want "$@" && run_case "$CASE" "$PY_DOCS/app"   "" "suspend_process.py"

echo "done."
