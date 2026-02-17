#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$REPO_ROOT/docs/widgets/Cargo.toml"
EXAMPLES_DIR="$REPO_ROOT/docs/widgets/examples"

if [[ $# -lt 1 ]]; then
  echo "Usage: tools/run-doc-widget.sh <example-name> [-- <extra cargo args>]"
  echo
  echo "Available examples:"
  while IFS= read -r entry; do
    echo "  - $entry"
  done < <(
    find "$EXAMPLES_DIR" -mindepth 1 -maxdepth 1 -type d \
      -exec test -f "{}/main.rs" \; -printf "%f\n" | sort
  )
  exit 1
fi

EXAMPLE="$1"
shift || true

exec cargo run --manifest-path "$MANIFEST" --example "$EXAMPLE" "$@"
