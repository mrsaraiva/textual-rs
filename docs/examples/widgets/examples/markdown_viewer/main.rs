/// Port of Python Textual `docs/examples/widgets/markdown_viewer.py`.
///
/// Demonstrates `MarkdownViewer`:
/// - Shows a rich Markdown document with headings, tables, code blocks, and lists.
/// - `show_table_of_contents=true` displays the TOC sidebar.
///
/// Python: `MarkdownViewer(EXAMPLE_MARKDOWN, show_table_of_contents=True)`.
/// Rust: `MarkdownViewer::new(EXAMPLE_MARKDOWN).show_table_of_contents(true)`.
///
/// Navigation history (`go()`, `back()`, `forward()`) is not yet implemented.
/// DEFERRED: MarkdownViewer navigation history — requires async document loading
/// wired into the runtime event loop.
use textual::prelude::*;

const EXAMPLE_MARKDOWN: &str = r#"# Markdown Viewer

This is an example of Textual's `MarkdownViewer` widget.


## Features

Markdown syntax and extensions are supported.

- Typography *emphasis*, **strong**, `inline code` etc.
- Headers
- Lists (bullet and ordered)
- Syntax highlighted code blocks
- Tables!

## Tables

Tables are displayed in a DataTable widget.

| Name            | Type   | Default | Description                        |
| --------------- | ------ | ------- | ---------------------------------- |
| `show_header`   | `bool` | `True`  | Show the table header              |
| `fixed_rows`    | `int`  | `0`     | Number of fixed rows               |
| `fixed_columns` | `int`  | `0`     | Number of fixed columns            |
| `zebra_stripes` | `bool` | `False` | Display alternating colors on rows |
| `header_height` | `int`  | `1`     | Height of header row               |
| `show_cursor`   | `bool` | `True`  | Show a cell cursor                 |


## Code Blocks

Code blocks are syntax highlighted.

```python
class ListViewExample(App):
    def compose(self) -> ComposeResult:
        yield ListView(
            ListItem(Label("One")),
            ListItem(Label("Two")),
            ListItem(Label("Three")),
        )
        yield Footer()
```

## Litany Against Fear

I must not fear.
Fear is the mind-killer.
Fear is the little-death that brings total obliteration.
I will face my fear.
I will permit it to pass over me and through me.
And when it has gone past, I will turn the inner eye to see its path.
Where the fear has gone there will be nothing. Only I will remain.
"#;

struct MarkdownViewerApp;

impl TextualApp for MarkdownViewerApp {
    fn compose(&mut self) -> AppRoot {
        let viewer = MarkdownViewer::new(EXAMPLE_MARKDOWN).show_table_of_contents(true);
        AppRoot::new().with_child(viewer)
    }
}

fn main() -> textual::Result<()> {
    run_sync(MarkdownViewerApp)
}

// ---------------------------------------------------------------------------
// Regression tests (DG-02)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_viewer_app_composes_without_panic() {
        let mut app = MarkdownViewerApp;
        let _root = app.compose();
    }

    #[test]
    fn viewer_shows_toc_by_default() {
        let viewer = MarkdownViewer::new(EXAMPLE_MARKDOWN).show_table_of_contents(true);
        assert!(viewer.is_showing_table_of_contents());
    }

    #[test]
    fn example_markdown_has_multiple_headings() {
        let viewer = MarkdownViewer::new(EXAMPLE_MARKDOWN);
        let headings = viewer.extract_headings();
        // Markdown has ## Features, ## Tables, ## Code Blocks, ## Litany... (and # title)
        assert!(headings.len() >= 4);
        assert_eq!(headings[0], (1, "Markdown Viewer".to_string()));
    }

    #[test]
    fn viewer_toc_can_be_disabled() {
        let viewer = MarkdownViewer::new(EXAMPLE_MARKDOWN).show_table_of_contents(false);
        assert!(!viewer.is_showing_table_of_contents());
    }
}
