# Docs Examples Layout

This tree mirrors Python Textual docs examples.

- Python docs source: `../textual/docs/examples/**`
- Rust docs target: `docs/examples/**`

Mapping rules:

- Docs demos from Python map to `docs/examples/**` crates.
- App demos from `../textual/examples/**` map to `examples/**` (outside this tree).
- `guide/*.py` root files map to `docs/examples/guide/core/**`.

Run any docs demo with:

```bash
tools/run-doc-example.sh <category-path> <example>
# e.g.
tools/run-doc-example.sh guide/screens modal01
```

Generate missing stub examples from Python docs sources:

```bash
tools/gen-doc-example-stubs.sh
```
