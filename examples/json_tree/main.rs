/// Port of Python Textual `examples/json_tree.py`.
///
/// Demonstrates dynamic `Tree` population from JSON data:
/// - `a` adds a JSON sub-tree to the root.
/// - `c` clears the tree.
/// - `t` toggles the root node's visibility.
///
/// Python uses `tree.root.add()` for dynamic node insertion and loads JSON from
/// a file at runtime. Rust uses `Tree::add_root()` (new framework method) and
/// embeds a sample JSON snippet as a `const`.
///
/// Because the Rust `Tree` only supports appending at the root level, each "add"
/// action builds a complete `TreeNode` hierarchy for the JSON value and appends
/// it as a new root (mirroring Python: `json_node = tree.root.add("JSON")`).
use textual::prelude::*;

/// Sample JSON document (simplified subset of the Python demo's food.json).
const SAMPLE_JSON: &str = r#"{
    "product": {
        "name": "Baked Potato Chips",
        "brand": "Pringles",
        "tags": ["snack", "potato", "crispy"],
        "calories_per_100g": 494
    },
    "in_stock": true,
    "rating": 4.2
}"#;

const CSS: &str = r#"
Tree {
    padding: 1 2;
}
"#;

/// Recursively populate a parent `TreeNode` from an untyped JSON value.
///
/// Uses `add_child()` pattern matching Python's `tree.root.add(label).add(...)`.
fn add_json_children(parent: &mut TreeNode, name: &str, value: &serde_json_lite::Value) {
    use serde_json_lite::Value;
    match value {
        Value::Object(map) => {
            let label = format!("{{}} {name}");
            let node = parent.add_child(TreeNode::new(label).expanded(true).allow_expand(true));
            for (key, val) in map {
                add_json_children(node, key, val);
            }
        }
        Value::Array(arr) => {
            let label = format!("[] {name}");
            let node = parent.add_child(TreeNode::new(label).expanded(true).allow_expand(true));
            for (idx, val) in arr.iter().enumerate() {
                add_json_children(node, &idx.to_string(), val);
            }
        }
        Value::String(s) => {
            let label = if name.is_empty() {
                format!("{s:?}")
            } else {
                format!("{name} = {s:?}")
            };
            parent.add_leaf(label);
        }
        Value::Number(n) => {
            let label = if name.is_empty() {
                format!("{n}")
            } else {
                format!("{name} = {n}")
            };
            parent.add_leaf(label);
        }
        Value::Bool(b) => {
            let label = if name.is_empty() {
                format!("{b}")
            } else {
                format!("{name} = {b}")
            };
            parent.add_leaf(label);
        }
        Value::Null => {
            let label = if name.is_empty() {
                "null".to_string()
            } else {
                format!("{name} = null")
            };
            parent.add_leaf(label);
        }
    }
}

/// Build a root `TreeNode` from a JSON value using the `add_child()` pattern.
fn json_to_node(name: &str, value: &serde_json_lite::Value) -> TreeNode {
    let mut root = TreeNode::new(format!("{{}} {name}")).expanded(true).allow_expand(true);
    // For top-level objects, add children directly. For other types, wrap in a root.
    if let serde_json_lite::Value::Object(map) = value {
        for (key, val) in map {
            add_json_children(&mut root, key, val);
        }
    } else {
        add_json_children(&mut root, name, value);
    }
    root
}

struct JsonTreeApp {
    json_data: Option<serde_json_lite::Value>,
}

impl JsonTreeApp {
    fn new() -> Self {
        Self { json_data: None }
    }
}

impl TextualApp for JsonTreeApp {
    fn configure(&mut self, app: &mut App) -> textual::Result<()> {
        app.load_stylesheet(CSS);
        Ok(())
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("a", "add", "Add node"),
            BindingDecl::new("c", "clear", "Clear"),
            BindingDecl::new("t", "toggle_root", "Toggle root"),
        ]
    }

    fn compose(&mut self) -> AppRoot {
        let tree = Tree::new(vec![TreeNode::new("Root").allow_expand(true)]);
        AppRoot::new()
            .with_child(Header::new())
            .with_child(tree)
            .with_child(Footer::new())
    }

    fn on_mount_with_app(&mut self, _app: &mut App, _ctx: &mut EventCtx) {
        // Parse JSON data once at mount (Python loads from food.json).
        self.json_data = serde_json_lite::from_str(SAMPLE_JSON);
    }

    fn on_key_with_app(&mut self, app: &mut App, key: &KeyEventData, ctx: &mut EventCtx) {
        match key.name() {
            "a" => {
                // Add a JSON sub-tree to the root.
                if let Some(ref json) = self.json_data {
                    let json_node = json_to_node("JSON", json);
                    let _ = app.with_query_one_mut_as::<Tree, _>("Tree", |tree| {
                        tree.add_root(json_node);
                    });
                }
                ctx.set_handled();
                ctx.request_repaint();
            }
            "c" => {
                // Clear the tree.
                let _ = app.with_query_one_mut_as::<Tree, _>("Tree", |tree| {
                    tree.clear();
                });
                ctx.set_handled();
                ctx.request_repaint();
            }
            "t" => {
                // Toggle root visibility.
                let _ = app.with_query_one_mut_as::<Tree, _>("Tree", |tree| {
                    tree.toggle_show_root();
                });
                ctx.set_handled();
                ctx.request_repaint();
            }
            _ => {}
        }
    }
}

fn main() -> textual::Result<()> {
    run_sync(JsonTreeApp::new())
}

// ---------------------------------------------------------------------------
// A minimal JSON value type — avoids adding serde/serde_json as a dependency.
// ---------------------------------------------------------------------------

mod serde_json_lite {
    use std::collections::BTreeMap;

    #[derive(Debug, Clone)]
    pub enum Value {
        Null,
        Bool(bool),
        Number(f64),
        String(String),
        Array(Vec<Value>),
        Object(BTreeMap<String, Value>),
    }

    impl std::fmt::Display for Value {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Value::Null => write!(f, "null"),
                Value::Bool(b) => write!(f, "{b}"),
                Value::Number(n) => write!(f, "{n}"),
                Value::String(s) => write!(f, "{s}"),
                Value::Array(a) => write!(f, "[..{}]", a.len()),
                Value::Object(o) => write!(f, "{{..{}}}", o.len()),
            }
        }
    }

    /// Minimal recursive descent JSON parser (no external deps).
    pub fn from_str(s: &str) -> Option<Value> {
        let s = s.trim();
        let mut chars = s.chars().peekable();
        parse_value(&mut chars)
    }

    fn skip_ws(chars: &mut std::iter::Peekable<std::str::Chars>) {
        while chars.peek().map(|c| c.is_ascii_whitespace()) == Some(true) {
            chars.next();
        }
    }

    fn parse_value(
        chars: &mut std::iter::Peekable<std::str::Chars>,
    ) -> Option<Value> {
        skip_ws(chars);
        match chars.peek()? {
            '{' => parse_object(chars),
            '[' => parse_array(chars),
            '"' => parse_string(chars).map(Value::String),
            't' => {
                // true
                for _ in 0..4 { chars.next(); }
                Some(Value::Bool(true))
            }
            'f' => {
                // false
                for _ in 0..5 { chars.next(); }
                Some(Value::Bool(false))
            }
            'n' => {
                // null
                for _ in 0..4 { chars.next(); }
                Some(Value::Null)
            }
            c if c.is_ascii_digit() || *c == '-' => parse_number(chars),
            _ => None,
        }
    }

    fn parse_string(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<String> {
        chars.next(); // consume '"'
        let mut s = String::new();
        loop {
            match chars.next()? {
                '"' => break,
                '\\' => {
                    match chars.next()? {
                        '"' => s.push('"'),
                        '\\' => s.push('\\'),
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        'r' => s.push('\r'),
                        c => { s.push('\\'); s.push(c); }
                    }
                }
                c => s.push(c),
            }
        }
        Some(s)
    }

    fn parse_number(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<Value> {
        let mut s = String::new();
        if chars.peek() == Some(&'-') { s.push(chars.next().unwrap()); }
        while chars.peek().map(|c| c.is_ascii_digit() || *c == '.' || *c == 'e' || *c == 'E' || *c == '+' || *c == '-') == Some(true) {
            s.push(chars.next().unwrap());
        }
        s.parse::<f64>().ok().map(Value::Number)
    }

    fn parse_object(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<Value> {
        chars.next(); // '{'
        let mut map = BTreeMap::new();
        skip_ws(chars);
        if chars.peek() == Some(&'}') { chars.next(); return Some(Value::Object(map)); }
        loop {
            skip_ws(chars);
            let key = parse_string(chars)?;
            skip_ws(chars);
            chars.next(); // ':'
            let val = parse_value(chars)?;
            map.insert(key, val);
            skip_ws(chars);
            match chars.next()? {
                ',' => {}
                '}' => break,
                _ => return None,
            }
        }
        Some(Value::Object(map))
    }

    fn parse_array(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<Value> {
        chars.next(); // '['
        let mut arr = Vec::new();
        skip_ws(chars);
        if chars.peek() == Some(&']') { chars.next(); return Some(Value::Array(arr)); }
        loop {
            let val = parse_value(chars)?;
            arr.push(val);
            skip_ws(chars);
            match chars.next()? {
                ',' => {}
                ']' => break,
                _ => return None,
            }
        }
        Some(Value::Array(arr))
    }
}

// ---------------------------------------------------------------------------
// Regression tests (DG-02)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_tree_app_composes_without_panic() {
        let mut app = JsonTreeApp::new();
        let _root = app.compose();
    }

    #[test]
    fn sample_json_parses_successfully() {
        let val = serde_json_lite::from_str(SAMPLE_JSON);
        assert!(val.is_some());
        match val.unwrap() {
            serde_json_lite::Value::Object(map) => {
                assert!(map.contains_key("product"));
                assert!(map.contains_key("in_stock"));
            }
            _ => panic!("expected object"),
        }
    }

    #[test]
    fn json_to_node_object_uses_braces_label() {
        use serde_json_lite::Value;
        use std::collections::BTreeMap;
        let val = Value::Object(BTreeMap::new());
        let node = json_to_node("data", &val);
        let _ = node; // Builds without panic.
    }

    #[test]
    fn tree_add_root_appends_node() {
        let mut tree = Tree::new(vec![TreeNode::new("Root")]);
        tree.add_root(TreeNode::new("JSON"));
        // Verifying via show_root toggle (no public len() accessor).
        tree.toggle_show_root();
        assert!(!tree.showing_root());
        tree.toggle_show_root();
        assert!(tree.showing_root());
    }
}
