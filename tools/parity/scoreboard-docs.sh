#!/usr/bin/env bash
# Parity scoreboard for ported docs examples (default: the widgets tier).
#
# For each example that exists in BOTH Python (../textual/docs/examples/<group>/<name>.py)
# and Rust (docs/examples/<group>/examples/<name>/, built binary), this captures
# the initial screen of each via tmux (120x30, like the PTY parity harness),
# normalizes (trim trailing whitespace + trailing blank lines), and diffs them.
#
# It does NOT touch tests/pty_parity.rs — it is a read-only scoreboard to decide
# which examples are already at parity vs. need work, before promoting cases.
#
# Requirements: tmux; a Python venv with textual (default /tmp/textual-venv);
# the Rust docs examples already built (`cd docs/examples && cargo build --examples`).
#
# Usage:
#   PYTHON=/tmp/textual-venv/bin/python tools/parity/scoreboard-docs.sh [group] [name ...]
#   group defaults to "widgets". With names, only those are scored.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PYTHON="${PYTHON:-/tmp/textual-venv/bin/python}"
GROUP="${1:-widgets}"
shift || true
# PY_ROOT: optional override for the Python Textual checkout (worktrees whose
# parent dir does not contain ../textual). PY_DIR overrides the full group dir.
PY_DIR="${PY_DIR:-${PY_ROOT:-$REPO_ROOT/../textual}/docs/examples/$GROUP}"
RS_EX_DIR="$REPO_ROOT/docs/examples/$GROUP/examples"
RS_BIN_DIR="$REPO_ROOT/docs/examples/target/debug/examples"
OUT_DIR="$REPO_ROOT/target/parity-scoreboard/$GROUP"
SOCKET="parity-sb-$$"
COLS=120
ROWS=30

[ -x "$PYTHON" ] || { echo "error: PYTHON=$PYTHON not executable" >&2; exit 1; }
mkdir -p "$OUT_DIR"
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

# normalize: trim trailing ws per line, drop trailing blank lines
norm() { sed -e 's/[[:space:]]*$//' | sed -e ':a' -e '/^\n*$/{$d;N;ba}' ; }

capture() { # <session>
    local s="$1" prev="" cur="" t=0
    while [ $t -lt 24 ]; do
        cur="$(tmx capture-pane -t "$s" -p 2>/dev/null)"
        [ -n "$cur" ] && [ "$cur" = "$prev" ] && { printf '%s\n' "$cur"; return 0; }
        prev="$cur"; t=$((t+1)); sleep 0.25
    done
    printf '%s\n' "$cur"
}

run_one() { # <name>
    local name="$1"
    local py_src="$PY_DIR/$name.py"
    local rs_bin="$RS_BIN_DIR/$name"
    [ -f "$py_src" ] || { echo "SKIP  $name (no python source)"; return 2; }
    [ -x "$rs_bin" ] || { echo "SKIP  $name (no rust binary)"; return 2; }

    tmx kill-session -t "py_$name" 2>/dev/null || true
    tmx new-session -d -s "py_$name" -x "$COLS" -y "$ROWS" "cd '$PY_DIR' && '$PYTHON' '$py_src'"
    capture "py_$name" | norm > "$OUT_DIR/$name.py.txt"
    tmx kill-session -t "py_$name" 2>/dev/null || true

    tmx kill-session -t "rs_$name" 2>/dev/null || true
    tmx new-session -d -s "rs_$name" -x "$COLS" -y "$ROWS" "cd '$REPO_ROOT' && '$rs_bin'"
    capture "rs_$name" | norm > "$OUT_DIR/$name.rs.txt"
    tmx kill-session -t "rs_$name" 2>/dev/null || true

    if diff -q "$OUT_DIR/$name.py.txt" "$OUT_DIR/$name.rs.txt" >/dev/null 2>&1; then
        echo "PASS  $name"
        return 0
    else
        diff "$OUT_DIR/$name.py.txt" "$OUT_DIR/$name.rs.txt" > "$OUT_DIR/$name.diff" 2>&1
        echo "FAIL  $name"
        return 1
    fi
}

if [ $# -gt 0 ]; then NAMES=("$@"); else
    mapfile -t NAMES < <(
        comm -12 \
            <(ls "$PY_DIR"/*.py 2>/dev/null | xargs -n1 basename | sed 's/\.py$//' | sort) \
            <(ls -d "$RS_EX_DIR"/*/ 2>/dev/null | xargs -n1 basename | sort)
    )
fi

pass=0; fail=0; skip=0
for n in "${NAMES[@]}"; do
    run_one "$n"; rc=$?
    case $rc in 0) pass=$((pass+1));; 1) fail=$((fail+1));; 2) skip=$((skip+1));; esac
done
echo "=================================================="
echo "scoreboard[$GROUP]: PASS=$pass FAIL=$fail SKIP=$skip (of ${#NAMES[@]})"
echo "diffs + captures: $OUT_DIR"
