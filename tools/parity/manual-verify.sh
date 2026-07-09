#!/usr/bin/env bash
# Human-in-the-loop side-by-side parity verification for demos that can't be
# judged by the automated plain-text scoreboard (animation, interactivity,
# timing, cwd-sensitive). Launches ONE tmux window per demo, split vertically:
#
#     ┌──────────────── PYTHON: <demo> ────────────────┐
#     │  (Python Textual — ../textual/docs/examples)    │
#     ├──────────────── RUST:   <demo> ────────────────┤
#     │  (Rust textual-rs — built example binary)       │
#     └─────────────────────────────────────────────────┘
#
# Both panes are LIVE — interact with either (they get identical width+height,
# so it's a fair comparison). When you're done inspecting a demo, DETACH with
# `Ctrl-b d`; the script then asks you to mark it and (optionally) add a note,
# and moves to the next demo. Verdicts are appended to a markdown log and the
# script resumes where you left off (already-marked demos are skipped).
#
# Usage:
#   tools/parity/manual-verify.sh <group> [name ...]      # named demos, or all in the group
#   PYTHON=/path/to/python tools/parity/manual-verify.sh widgets progress_bar_isolated switch
#
# Keys at the verdict prompt: p=parity  n=non-parity  b=broken/crash  r=redo(re-open)  s=skip  q=quit
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PYTHON="${PYTHON:-/home/msaraiva/dev/mark/Proj/Libs/textual/.venv/bin/python}"
GROUP="${1:?usage: manual-verify.sh <group> [name ...]}"
shift || true
PY_DIR="$REPO_ROOT/../textual/docs/examples/$GROUP"
RS_BIN_DIR="$REPO_ROOT/docs/examples/target/debug/examples"
RESULTS="$REPO_ROOT/target/parity-manual/$GROUP.md"
SOCKET="mv-$$"
mkdir -p "$(dirname "$RESULTS")"

[ -x "$PYTHON" ] || { echo "error: PYTHON=$PYTHON not executable" >&2; exit 1; }
[ -d "$PY_DIR" ] || { echo "error: no Python group dir $PY_DIR" >&2; exit 1; }
command -v tmux >/dev/null || { echo "error: tmux not found" >&2; exit 1; }

tmx() { tmux -L "$SOCKET" "$@"; }
cleanup() { tmx kill-server 2>/dev/null; rm -f "/tmp/tmux-$(id -u)/$SOCKET" 2>/dev/null; }
trap cleanup EXIT INT TERM

# Demo list: explicit args, else every <name>.py in the Python group dir.
if [ $# -gt 0 ]; then
    NAMES=("$@")
else
    mapfile -t NAMES < <(ls "$PY_DIR"/*.py 2>/dev/null | xargs -n1 basename | sed 's/\.py$//' | sort)
fi

# Results log (markdown table), created once; resumable across runs.
if [ ! -f "$RESULTS" ]; then
    printf '# Manual parity review — %s\n\n' "$GROUP" > "$RESULTS"
    printf '_Side-by-side (Python top / Rust bottom) via `manual-verify.sh`._\n\n' >> "$RESULTS"
    printf '| demo | verdict | note |\n|---|---|---|\n' >> "$RESULTS"
fi
already() { grep -qE "^\| $1 \|" "$RESULTS" 2>/dev/null; }

open_pair() { # <name> — build the split-pane window; returns after user detaches
    local name="$1" py_src="$2" rs_bin="$3"
    tmx kill-session -t mv 2>/dev/null || true
    # Top pane = Python. `exec` so the app owns the pane; on exit the pane shows
    # a hold prompt so a crash/immediate-exit is visible rather than vanishing.
    tmx new-session -d -s mv \
        "cd '$PY_DIR' && '$PYTHON' '$py_src'; ec=\$?; printf '\n[python exited (%s) — Ctrl-b d to continue]' \"\$ec\"; read -r _"
    tmx split-window -v -t mv \
        "cd '$REPO_ROOT' && '$rs_bin'; ec=\$?; printf '\n[rust exited (%s) — Ctrl-b d to continue]' \"\$ec\"; read -r _"
    tmx select-layout -t mv even-vertical
    tmx set-option -t mv pane-border-status top 2>/dev/null || true
    tmx set-option -t mv pane-border-format " #{?#{==:#{pane_index},0},PYTHON,RUST}: ${name} " 2>/dev/null || true
    tmx select-pane -t mv.0 2>/dev/null || true
    echo ">>> $name — inspect both panes; interact if needed; press Ctrl-b then d to detach when done."
    tmx attach -t mv
    tmx kill-session -t mv 2>/dev/null || true
}

total=${#NAMES[@]}; i=0; done_count=0
for name in "${NAMES[@]}"; do
    i=$((i+1))
    py_src="$PY_DIR/$name.py"; rs_bin="$RS_BIN_DIR/$name"
    [ -f "$py_src" ] || { echo "[$i/$total] skip $name (no python source)"; continue; }
    [ -x "$rs_bin" ] || { echo "[$i/$total] skip $name (no rust binary — build it first)"; continue; }
    if already "$name"; then echo "[$i/$total] already marked: $name (edit $RESULTS to redo)"; continue; fi

    echo; echo "=== [$i/$total] $name ==="
    while :; do
        open_pair "$name" "$py_src" "$rs_bin"
        printf 'verdict for %s — [p]arity [n]on-parity [b]roken [r]edo [s]kip [q]uit: ' "$name"
        read -r v
        case "$v" in
            r|R) continue ;;                 # re-open the pair
            q|Q) echo "quit — results in $RESULTS"; exit 0 ;;
            s|S) echo "skipped (not recorded)"; break ;;
            p|P) verdict='✅ parity' ;;
            n|N) verdict='❌ non-parity' ;;
            b|B) verdict='💥 broken' ;;
            *)   echo "  ? unrecognized '$v' — try again"; continue ;;
        esac
        printf 'note (optional, Enter to skip): '; read -r note
        printf '| %s | %s | %s |\n' "$name" "$verdict" "${note//|/\\|}" >> "$RESULTS"
        echo "  recorded: $name → $verdict"
        done_count=$((done_count+1))
        break
    done
done
echo; echo "done — marked $done_count this run. Full log: $RESULTS"
