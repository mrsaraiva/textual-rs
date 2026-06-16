#!/usr/bin/env bash
# Manually sweep leaked parity/scoreboard tmux sockets.
#
# Kills LIVE non-protected tmux servers and removes their sockets, plus any dead
# socket files, in /tmp/tmux-$UID. NEVER touches the `default` socket or any
# `dev_*` session (the user's real work).
#
# Use this when the harness aborts with "live tmux sockets ... exceeds cap", or
# proactively after a crashed/interrupted parity run.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=tools/parity/tmux-guard.sh
source "$here/tmux-guard.sh"

dir="$(_parity_sock_dir)"
[ -d "$dir" ] || { echo "no tmux socket dir ($dir); nothing to do"; exit 0; }

killed=0
for s in "$dir"/*; do
    [ -S "$s" ] || continue
    base="$(basename "$s")"
    if [[ "$base" =~ $PARITY_TMUX_PROTECT_RE ]]; then
        echo "  keep (protected): $base"
        continue
    fi
    tmux -S "$s" kill-server 2>/dev/null || true
    rm -f "$s" 2>/dev/null || true
    killed=$((killed + 1))
done
echo "swept $killed non-protected socket(s); remaining:"
ls -1 "$dir" 2>/dev/null | sed 's/^/  /' || true
