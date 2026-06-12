# Real-PTY parity harness

`tests/pty_parity.rs` runs the example binaries in a genuine pseudo-terminal
(via `portable-pty`), drives them with key input, captures the emulated screen
(via `vt100`), and compares the plain text against golden screens generated
from **Python Textual**. It is a blocking CI gate (`.github/workflows/ci.yml`
job `pty-parity`, and a release gate in `release.yml`).

## Ground rules

- **Goldens define parity.** `tests/pty_parity/golden/*.txt` are generated only
  from Python output via `gen-python-goldens.sh`. There is deliberately no way
  to bless goldens from Rust output.
- **XFail is strict.** Known gaps are declared `Status::XFail("reason")` in the
  manifest in `tests/pty_parity.rs`:
  - a `Pass` case that stops matching fails CI (regression);
  - an `XFail` case that starts matching also fails CI (XPASS) until you
    promote it to `Status::Pass` — fixes get locked in explicitly.
- Comparison is plain text with trailing whitespace trimmed. Color/attribute
  parity is out of scope for this harness version.
- Intentional Rust/Python content differences (e.g. `markdown.rs` vs
  `markdown.py` inside `demo.md`) are handled per-case via
  `golden_replacements`, not by editing goldens.

## Fixing a parity gap

1. Fix the framework (root-cause-first; never patch the demo to match).
2. Run `cargo test --test pty_parity`. The fixed case fails with XPASS.
3. Promote its manifest entry to `Status::Pass`. Done — it is now guarded.

On mismatch the harness writes the Rust screen to
`target/pty-parity-actual/<case>.txt` and prints a per-line diff against the
golden.

## Adding a case

1. Add the case to `tests/pty_parity.rs` (`CASES`) **and** to
   `gen-python-goldens.sh` with the same name, size, keys, and working
   directory. (`manifest_matches_golden_files` keeps manifest and golden files
   in sync.)
2. Generate the golden (see below). Inspect it before committing.
3. New cases must be deterministic: fixed PTY size (120×30), stable working
   directory (use `tests/pty_parity/fixtures/` for filesystem-dependent cases),
   no clocks/network/randomness on the captured screen.

## Regenerating goldens

Requires `tmux`, the Python Textual checkout at `../textual`, and a venv with
`textual` (+ `httpx` for the dictionary case):

```bash
uv venv /tmp/textual-venv
VIRTUAL_ENV=/tmp/textual-venv uv pip install -e ../textual httpx

PYTHON=/tmp/textual-venv/bin/python tools/parity/gen-python-goldens.sh           # all cases
PYTHON=/tmp/textual-venv/bin/python tools/parity/gen-python-goldens.sh markdown_initial  # one case
```

Only regenerate when the Python reference itself changes (new upstream version)
or when adding cases — and review the golden diff like any other code change.
