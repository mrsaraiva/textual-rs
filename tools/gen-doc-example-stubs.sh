#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INDEX="$REPO_ROOT/tools/doc_examples_index.toml"
TEMPLATE="$REPO_ROOT/tools/templates/doc_example_stub_main.rs"
PY_ROOT_DEFAULT="$REPO_ROOT/../textual/docs/examples"

DRY_RUN=0
OVERWRITE=0
PY_ROOT="$PY_ROOT_DEFAULT"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --overwrite)
      OVERWRITE=1
      shift
      ;;
    --python-root)
      if [[ $# -lt 2 ]]; then
        echo "--python-root requires a value" >&2
        exit 1
      fi
      PY_ROOT="$2"
      shift 2
      ;;
    *)
      echo "Unknown argument: $1" >&2
      echo "Usage: tools/gen-doc-example-stubs.sh [--dry-run] [--overwrite] [--python-root <path>]" >&2
      exit 1
      ;;
  esac
done

if [[ ! -f "$INDEX" ]]; then
  echo "Missing index file: $INDEX" >&2
  exit 1
fi
if [[ ! -f "$TEMPLATE" ]]; then
  echo "Missing template file: $TEMPLATE" >&2
  exit 1
fi
if [[ ! -d "$PY_ROOT" ]]; then
  echo "Python docs examples root not found: $PY_ROOT" >&2
  exit 1
fi

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

created=0
updated=0
skipped=0
missing_category=0

while IFS= read -r py; do
  rel="${py#${PY_ROOT}/}"
  dir="$(dirname "$rel")"
  if [[ "$dir" == "guide" ]]; then
    category="guide/core"
  else
    category="$dir"
  fi

  example="$(basename "$rel" .py)"
  manifest_rel="$(manifest_for "$category")"
  if [[ -z "$manifest_rel" ]]; then
    echo "[warn] no category mapping for '$rel' (category '$category')" >&2
    missing_category=$((missing_category + 1))
    continue
  fi

  crate_dir="$REPO_ROOT/$(dirname "$manifest_rel")"
  dest="$crate_dir/examples/$example/main.rs"

  if [[ -f "$dest" && "$OVERWRITE" -ne 1 ]]; then
    skipped=$((skipped + 1))
    continue
  fi

  mkdir -p "$(dirname "$dest")"

  if [[ "$DRY_RUN" -eq 1 ]]; then
    if [[ -f "$dest" ]]; then
      echo "[dry-run] update $dest"
    else
      echo "[dry-run] create $dest"
    fi
    continue
  fi

  sed \
    -e "s|__PY_SOURCE__|$rel|g" \
    -e "s|__CATEGORY__|$category|g" \
    -e "s|__EXAMPLE__|$example|g" \
    "$TEMPLATE" > "$dest"

  if [[ -f "$dest" && "$OVERWRITE" -eq 1 ]]; then
    updated=$((updated + 1))
  else
    created=$((created + 1))
  fi
done < <(find "$PY_ROOT" -type f -name '*.py' | sort)

if [[ "$DRY_RUN" -eq 1 ]]; then
  echo "dry-run complete"
  exit 0
fi

echo "stubs created: $created"
echo "stubs updated: $updated"
echo "stubs skipped: $skipped"
if [[ "$missing_category" -gt 0 ]]; then
  echo "stubs missing category mapping: $missing_category"
  exit 1
fi
