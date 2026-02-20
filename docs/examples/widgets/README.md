# Docs Widgets Examples

This crate hosts docs-sourced examples mirrored from Python Textual docs (`../textual/docs/examples/**`).

- Docs lane mapping: `../textual/docs/examples/**` -> `docs/examples/**`.
- Current crate-backed location in this repo: `docs/examples/widgets/examples/<name>/main.rs`.
- App lane mapping: `../textual/examples/**` -> `examples/**` (not in this crate).

Run examples with:

```bash
cargo run --manifest-path docs/examples/widgets/Cargo.toml --example <name>
tools/run-doc-example.sh widgets <name>
```

Current migrated examples:

- `buttons`
- `buttons_advanced`
- `data_table`
- `hello`
- `input`
- `input_types`
- `input_validation`
- `keys`
- `modal`
- `rich_log`
- `tabbed_content`
- `tabbed_content_label_color`
- `text_area_custom_language`
- `text_area_custom_theme`
- `text_area_example`
- `text_area_extended`
- `text_area_selection`
- `tick`
