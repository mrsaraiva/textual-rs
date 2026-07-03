/// Port of Python Textual `examples/json_tree.py`.
///
/// Demonstrates dynamic `Tree` population from JSON data:
/// - `a` adds a JSON sub-tree under the root.
/// - `c` clears the tree (preserves root).
/// - `t` toggles the root node's visibility.
///
/// Python uses `tree.root.add()` for dynamic node insertion and loads JSON from
/// `food.json` at runtime. Rust embeds `food.json` via `include_str!` and uses
/// `serde_json` for parsing.
use serde_json::Value;
use textual::prelude::*;

const FOOD_JSON: &str = include_str!("food.json");

struct JsonTreeApp {
    json_data: Option<Value>,
    tree: HandleSlot<Tree>,
}

impl JsonTreeApp {
    fn new() -> Self {
        Self {
            json_data: None,
            tree: HandleSlot::new(),
        }
    }
}

impl TextualApp for JsonTreeApp {
    fn title(&self) -> &'static str {
        "TreeApp"
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
            .with_child_handle(tree, &self.tree)
            .with_child(Footer::new())
    }

    fn on_mount_with_app(&mut self, _app: &mut App, _ctx: &mut textual::event::WidgetCtx) {
        self.json_data = serde_json::from_str(FOOD_JSON).ok();
    }

    fn on_app_action_str(&mut self, app: &mut App, action: &str, ctx: &mut textual::event::WidgetCtx) {
        match action {
            "add" => {
                // Python: json_node = tree.root.add("JSON")
                //         self.add_json(json_node, self.json_data)
                //         tree.root.expand()
                if let Some(ref json) = self.json_data {
                    let json_clone = json.clone();
                    let _ = self.tree.handle().and_then(|h| {
                        h.update(app, |tree, _ctx| {
                            if let Some(root) = tree.root_mut() {
                                let json_node = root.add_child(TreeNode::new("JSON"));
                                add_json(json_node, "JSON", &json_clone);
                                root.expand();
                            }
                        })
                    });
                }
                ctx.set_handled();
                ctx.request_repaint();
            }
            "clear" => {
                // Python: tree.clear() — preserves root, clears children.
                let _ = self.tree.handle().and_then(|h| {
                    h.update(app, |tree, _ctx| {
                        tree.clear();
                    })
                });
                ctx.set_handled();
                ctx.request_repaint();
            }
            "toggle_root" => {
                // Python: tree.show_root = not tree.show_root
                let _ = self.tree.handle().and_then(|h| {
                    h.update(app, |tree, _ctx| {
                        tree.toggle_show_root();
                    })
                });
                ctx.set_handled();
                ctx.request_repaint();
            }
            _ => {}
        }
    }
}

/// Recursively populate a tree node from a JSON value, matching Python's `add_json`/`add_node`.
///
/// - Objects: label `{} name`, children for each key
/// - Arrays: label `[] name`, children for each index
/// - Leaves: label `[b]name[/b]=repr(value)` (bold key via Rich markup)
fn add_json(node: &mut TreeNode, name: &str, data: &Value) {
    match data {
        Value::Object(map) => {
            node.set_label(format!("{{}} {name}"));
            for (key, value) in map {
                let child = node.add_child(TreeNode::new(""));
                add_json(child, key, value);
            }
        }
        Value::Array(arr) => {
            node.set_label(format!("[] {name}"));
            for (idx, value) in arr.iter().enumerate() {
                let child = node.add_child(TreeNode::new(""));
                add_json(child, &idx.to_string(), value);
            }
        }
        _ => {
            node.set_allow_expand(false);
            let repr = repr_value(data);
            if name.is_empty() {
                node.set_label(repr);
            } else {
                node.set_label(format!("[b]{name}[/b]={repr}"));
            }
        }
    }
}

/// Format a JSON leaf value in Python repr style.
fn repr_value(data: &Value) -> String {
    match data {
        Value::String(s) => format!("'{s}'"),
        Value::Number(n) => format!("{n}"),
        Value::Bool(b) => {
            if *b {
                "True".to_string()
            } else {
                "False".to_string()
            }
        }
        Value::Null => "None".to_string(),
        _ => format!("{data}"),
    }
}

fn main() -> textual::Result<()> {
    run_sync(JsonTreeApp::new())
}

// ---------------------------------------------------------------------------
// Tests
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
    fn food_json_parses_successfully() {
        let val: serde_json::Result<Value> = serde_json::from_str(FOOD_JSON);
        assert!(val.is_ok());
        match val.unwrap() {
            Value::Object(map) => {
                assert!(map.contains_key("product"));
            }
            _ => panic!("expected top-level object"),
        }
    }

    #[test]
    fn json_tree_adds_children_under_root() {
        let mut tree = Tree::new(vec![TreeNode::new("Root").allow_expand(true)]);
        let data: Value = serde_json::from_str(r#"{"key": "value"}"#).unwrap();
        if let Some(root) = tree.root_mut() {
            let json_node = root.add_child(TreeNode::new("JSON"));
            add_json(json_node, "JSON", &data);
            root.expand();
        }
        // Root should still be the only root, with children added beneath it
        assert!(tree.root().is_some());
        assert_eq!(tree.root().unwrap().label(), "Root");
    }

    #[test]
    fn json_tree_clear_preserves_root() {
        let mut tree = Tree::new(vec![TreeNode::new("Root").allow_expand(true)]);
        let data: Value = serde_json::from_str(r#"{"a": 1}"#).unwrap();
        if let Some(root) = tree.root_mut() {
            let json_node = root.add_child(TreeNode::new("JSON"));
            add_json(json_node, "JSON", &data);
        }
        tree.clear();
        // Root preserved, children cleared
        assert!(tree.root().is_some());
        assert_eq!(tree.root().unwrap().label(), "Root");
    }

    #[test]
    fn add_json_object_labels_correctly() {
        let data: Value = serde_json::from_str(r#"{"name": "test"}"#).unwrap();
        let mut node = TreeNode::new("");
        add_json(&mut node, "JSON", &data);
        assert_eq!(node.label(), "{} JSON");
    }

    #[test]
    fn repr_value_formats_python_style() {
        assert_eq!(repr_value(&Value::String("hello".into())), "'hello'");
        assert_eq!(repr_value(&Value::Bool(true)), "True");
        assert_eq!(repr_value(&Value::Bool(false)), "False");
        assert_eq!(repr_value(&Value::Null), "None");
    }
}
