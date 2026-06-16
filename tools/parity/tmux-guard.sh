#!/usr/bin/env bash
# Shared tmux-socket guard for the parity harness scripts.
#
# WHY: gen-python-goldens.sh / scoreboard-docs.sh (and ad-hoc agent triage)
# each create a `tmux -L <name>` server. Their EXIT trap kills it on a clean
# exit, but NOT on SIGKILL / crash / interrupted-agent. Leaked sockets+servers
# accumulated into the hundreds and broke tmux GLOBALLY — killing unrelated
# `dev_*` sessions and the harness itself. This guard makes that self-correcting
# and loud: it sweeps DEAD sockets every run and HARD-CAPS live leaked servers,
# aborting with a diagnostic that names the cause.
#
# Source it, then call `parity_tmux_guard` before any tmux use.
#
# NEVER touches the `default` socket or any `dev_*` session (the user's real work).

# Sockets we must never sweep or count against the cap (user-owned).
PARITY_TMUX_PROTECT_RE='^(default|dev_)'
# Max LIVE non-protected sockets allowed before we refuse to run.
PARITY_TMUX_CAP="${PARITY_TMUX_CAP:-15}"

_parity_sock_dir() { echo "/tmp/tmux-$(id -u)"; }

# Remove DEAD socket files (server already gone) — always safe, never a running
# server. Skips protected (default/dev_*) sockets entirely.
parity_tmux_sweep_dead() {
    local dir; dir="$(_parity_sock_dir)"
    [ -d "$dir" ] || return 0
    local s base removed=0
    for s in "$dir"/*; do
        [ -S "$s" ] || continue
        base="$(basename "$s")"
        [[ "$base" =~ $PARITY_TMUX_PROTECT_RE ]] && continue
        if ! tmux -S "$s" ls >/dev/null 2>&1; then
            rm -f "$s" 2>/dev/null && removed=$((removed + 1))
        fi
    done
    [ "$removed" -gt 0 ] && echo "[tmux-guard] swept $removed dead socket(s) from $dir" >&2
    return 0
}

# Count LIVE non-protected sockets.
parity_tmux_live_count() {
    local dir s base n=0
    dir="$(_parity_sock_dir)"
    [ -d "$dir" ] || { echo 0; return 0; }
    for s in "$dir"/*; do
        [ -S "$s" ] || continue
        base="$(basename "$s")"
        [[ "$base" =~ $PARITY_TMUX_PROTECT_RE ]] && continue
        tmux -S "$s" ls >/dev/null 2>&1 && n=$((n + 1))
    done
    echo "$n"
}

# Sweep dead sockets, then ABORT if too many live ones remain (a leak).
parity_tmux_guard() {
    parity_tmux_sweep_dead
    local live; live="$(parity_tmux_live_count)"
    if [ "$live" -gt "$PARITY_TMUX_CAP" ]; then
        {
            echo "ERROR: $live live tmux sockets in $(_parity_sock_dir) exceeds cap ($PARITY_TMUX_CAP)."
            echo "       This is almost certainly leaked parity/scoreboard servers — running"
            echo "       the tmux harness now risks breaking tmux globally (killing dev_* sessions)."
            echo "       Clean them up first:  tools/parity/tmux-clean.sh"
            echo "       (override the cap for this run with PARITY_TMUX_CAP=<n> if intentional.)"
        } >&2
        return 1
    fi
    return 0
}
