#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INDEX="$REPO_ROOT/tools/doc_examples_index.toml"

manifest_for() {
  local category="$1"
  awk -F'"' -v want="$category" '
    /^name = "/ {name = $2; next}
    /^manifest = "/ {
      if (name == want) {
        print $2
        exit
      }
    }
  ' "$INDEX"
}

list_categories() {
  awk -F'"' '/^name = "/ {print $2}' "$INDEX"
}

list_examples_for_category() {
  local category="$1"
  local manifest_rel
  manifest_rel="$(manifest_for "$category")"
  if [[ -z "$manifest_rel" ]]; then
    return 1
  fi
  local crate_dir="$REPO_ROOT/$(dirname "$manifest_rel")"
  if [[ ! -d "$crate_dir/examples" ]]; then
    return 0
  fi
  find "$crate_dir/examples" -mindepth 1 -maxdepth 1 -type d \
    -exec test -f "{}/main.rs" \; -printf "%f\n" | sort
}

usage() {
  cat <<USAGE
Usage:
  tools/run-doc-example.sh --list
  tools/run-doc-example.sh <category-path> <example> [-- <extra cargo args>]

Examples:
  tools/run-doc-example.sh widgets buttons
  tools/run-doc-example.sh guide/screens modal01
USAGE
}

if [[ ! -f "$INDEX" ]]; then
  echo "Missing index file: $INDEX" >&2
  exit 1
fi

if [[ $# -eq 1 && "$1" == "--list" ]]; then
  while IFS= read -r category; do
    while IFS= read -r example; do
      [[ -n "$example" ]] && echo "$category/$example"
    done < <(list_examples_for_category "$category")
  done < <(list_categories)
  exit 0
fi

if [[ $# -lt 2 ]]; then
  usage
  echo
  echo "Known categories:"
  list_categories | sed 's/^/  - /'
  exit 1
fi

CATEGORY="$1"
EXAMPLE="$2"
shift 2 || true
if [[ "${1:-}" == "--" ]]; then
  shift
fi

MANIFEST_REL="$(manifest_for "$CATEGORY")"
if [[ -z "$MANIFEST_REL" ]]; then
  echo "Unknown docs category: $CATEGORY" >&2
  echo "Known categories:" >&2
  list_categories | sed 's/^/  - /' >&2
  exit 1
fi

MANIFEST="$REPO_ROOT/$MANIFEST_REL"
CRATE_DIR="$REPO_ROOT/$(dirname "$MANIFEST_REL")"
EXAMPLE_SRC="$CRATE_DIR/examples/$EXAMPLE/main.rs"

if [[ ! -f "$EXAMPLE_SRC" ]]; then
  echo "Example '$EXAMPLE' was not found in category '$CATEGORY'." >&2
  echo "Available examples:" >&2
  list_examples_for_category "$CATEGORY" | sed 's/^/  - /' >&2
  exit 1
fi

exec cargo run --manifest-path "$MANIFEST" --example "$EXAMPLE" "$@"
