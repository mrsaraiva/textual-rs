#!/usr/bin/env bash
# Human-in-the-loop side-by-side parity verification for demos the automated
# plain-text scoreboard can't fairly judge (animation, interactivity, timing,
# cwd-sensitive). Launches ONE tmux window per demo, split into a HORIZONTAL
# divider (top = Python Textual, bottom = Rust textual-rs), each pane spanning
# the FULL terminal width (wide apps need width more than height):
#
#     ┌──────────────── PYTHON: <demo> ────────────────┐
#     │  (../textual/docs/examples/<group>/<name>.py)   │
#     ├──────────────── RUST:   <demo> ────────────────┤
#     │  (docs/examples/target/debug/examples/<name>)   │
#     └─────────────────────────────────────────────────┘
#
# Both panes are LIVE and identically sized — interact with either. When done,
# DETACH with `Ctrl-b d`; the script then asks you to mark the demo and add an
# optional note, and moves on. Verdicts append to a markdown log and the walk
# resumes where you left off (already-marked demos are skipped).
#
# Usage:
#   tools/parity/manual-verify.sh <group> [name ...]     # a group (or named demos in it)
#   tools/parity/manual-verify.sh --from <worklist>      # a cross-group worklist file
#
# <group> is a scoreboard group, incl. nested guide groups: widgets, styles,
# tutorial, guide/layout, guide/reactivity, ...
#
# Worklist file: one demo per line, `<group> <name>` (whitespace-separated).
# Blank lines and `#`-comments ignored. Example:
#     widgets     progress_bar_isolated
#     guide/reactivity  world_clock01
#
# Verdict keys: p=parity  n=non-parity  b=broken/crash  r=redo(re-open)  s=skip  q=quit
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PYTHON="${PYTHON:-/home/msaraiva/dev/mark/Proj/Libs/textual/.venv/bin/python}"
RS_BIN_DIR="$REPO_ROOT/docs/examples/target/debug/examples"
SOCKET="mv-$$"

[ -x "$PYTHON" ] || { echo "error: PYTHON=$PYTHON not executable" >&2; exit 1; }
command -v tmux >/dev/null || { echo "error: tmux not found" >&2; exit 1; }

tmx() { tmux -L "$SOCKET" "$@"; }
cleanup() { tmx kill-server 2>/dev/null; rm -f "/tmp/tmux-$(id -u)/$SOCKET" 2>/dev/null; }
trap cleanup EXIT INT TERM

# ── Build the (group, name) work items ──────────────────────────────────────
declare -a GROUPS NAMES
if [ "${1:-}" = "--from" ]; then
    LIST="${2:?usage: manual-verify.sh --from <worklist>}"
    [ -f "$LIST" ] || { echo "error: no worklist $LIST" >&2; exit 1; }
    RESULTS="$REPO_ROOT/target/parity-manual/manual-review.md"
    LABEL="worklist $(basename "$LIST")"
    while read -r g n _rest; do
        [ -z "${g:-}" ] && continue
        case "$g" in \#*) continue;; esac
        GROUPS+=("$g"); NAMES+=("$n")
    done < "$LIST"
else
    G="${1:?usage: manual-verify.sh <group> [name ...] | --from <worklist>}"
    shift || true
    RESULTS="$REPO_ROOT/target/parity-manual/${G//\//_}.md"
    LABEL="group $G"
    if [ $# -gt 0 ]; then
        for n in "$@"; do GROUPS+=("$G"); NAMES+=("$n"); done
    else
        while read -r n; do GROUPS+=("$G"); NAMES+=("$n"); done \
            < <(ls "$REPO_ROOT/../textual/docs/examples/$G"/*.py 2>/dev/null | xargs -n1 basename | sed 's/\.py$//' | sort)
    fi
fi

mkdir -p "$(dirname "$RESULTS")"
if [ ! -f "$RESULTS" ]; then
    printf '# Manual parity review — %s\n\n' "$LABEL" > "$RESULTS"
    printf '_Side-by-side (Python top / Rust bottom) via `manual-verify.sh`._\n\n' >> "$RESULTS"
    printf '| group | demo | verdict | note |\n|---|---|---|---|\n' >> "$RESULTS"
fi
already() { grep -qE "^\| ${1//\//\\/} \| $2 \|" "$RESULTS" 2>/dev/null; }

open_pair() { # <group> <name> <py_src> <rs_bin>
    local g="$1" name="$2" py_src="$3" rs_bin="$4"
    local py_dir; py_dir="$(dirname "$py_src")"
    tmx kill-session -t mv 2>/dev/null || true
    tmx new-session -d -s mv \
        "cd '$py_dir' && '$PYTHON' '$py_src'; ec=\$?; printf '\n[python exited (%s) — Ctrl-b d to continue]' \"\$ec\"; read -r _"
    tmx split-window -v -t mv \
        "cd '$REPO_ROOT' && '$rs_bin'; ec=\$?; printf '\n[rust exited (%s) — Ctrl-b d to continue]' \"\$ec\"; read -r _"
    tmx select-layout -t mv even-vertical
    tmx set-option -t mv pane-border-status top 2>/dev/null || true
    tmx set-option -t mv pane-border-format " #{?#{==:#{pane_index},0},PYTHON,RUST}: ${g}/${name} " 2>/dev/null || true
    tmx select-pane -t mv.0 2>/dev/null || true
    echo ">>> ${g}/${name} — inspect both panes; interact if needed; Ctrl-b then d to detach when done."
    tmx attach -t mv
    tmx kill-session -t mv 2>/dev/null || true
}

total=${#NAMES[@]}; done_count=0
for i in "${!NAMES[@]}"; do
    g="${GROUPS[$i]}"; name="${NAMES[$i]}"; n=$((i+1))
    py_src="$REPO_ROOT/../textual/docs/examples/$g/$name.py"; rs_bin="$RS_BIN_DIR/$name"
    [ -f "$py_src" ] || { echo "[$n/$total] skip $g/$name (no python source)"; continue; }
    [ -x "$rs_bin" ] || { echo "[$n/$total] skip $g/$name (no rust binary — build it first)"; continue; }
    if already "$g" "$name"; then echo "[$n/$total] already marked: $g/$name"; continue; fi

    echo; echo "=== [$n/$total] $g/$name ==="
    while :; do
        open_pair "$g" "$name" "$py_src" "$rs_bin"
        printf 'verdict for %s/%s — [p]arity [n]on-parity [b]roken [r]edo [s]kip [q]uit: ' "$g" "$name"
        read -r v
        case "$v" in
            r|R) continue ;;
            q|Q) echo "quit — results in $RESULTS"; exit 0 ;;
            s|S) echo "skipped (not recorded)"; break ;;
            p|P) verdict='✅ parity' ;;
            n|N) verdict='❌ non-parity' ;;
            b|B) verdict='💥 broken' ;;
            *)   echo "  ? unrecognized '$v' — try again"; continue ;;
        esac
        printf 'note (optional, Enter to skip): '; read -r note
        printf '| %s | %s | %s | %s |\n' "$g" "$name" "$verdict" "${note//|/\\|}" >> "$RESULTS"
        echo "  recorded: $g/$name → $verdict"
        done_count=$((done_count+1))
        break
    done
done
echo; echo "done — marked $done_count this run. Full log: $RESULTS"
