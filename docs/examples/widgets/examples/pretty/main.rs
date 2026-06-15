/// Port of Python Textual `docs/examples/widgets/pretty.py`.
///
/// Demonstrates the `Pretty` widget by displaying a pretty-printed
/// representation of a nested data structure.
///
/// The Python original passes a dict to `Pretty(DATA)`. In Rust we use
/// `Pretty::from_debug_str()` with a Python-repr-compatible string so that
/// the Rust `rich-rs` pretty printer produces the same output as Python's
/// `rich.pretty` does for the equivalent Python dict.
use textual::prelude::*;

/// Python dict-repr of the movie data (matches the Python `DATA` constant
/// in `pretty.py` exactly, using single-quoted strings as Python repr does).
const DATA: &str = concat!(
    "{'title': 'Back to the Future', ",
    "'releaseYear': 1985, ",
    "'director': 'Robert Zemeckis', ",
    "'genre': 'Adventure, Comedy, Sci-Fi', ",
    "'cast': [",
    "{'actor': 'Michael J. Fox', 'character': 'Marty McFly'}, ",
    "{'actor': 'Christopher Lloyd', 'character': 'Dr. Emmett Brown'}",
    "]}"
);

struct PrettyExample;

impl TextualApp for PrettyExample {
    fn compose(&mut self) -> AppRoot {
        AppRoot::new().with_child(Pretty::from_debug_str(DATA))
    }
}

fn main() -> textual::Result<()> {
    run_sync(PrettyExample)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pretty_example_composes_without_panic() {
        let mut app = PrettyExample;
        let _root = app.compose();
    }
}
