use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rich_rs::Console;
use textual::message::MessageEvent;
use textual::prelude::*;
use textual::render::FrameBuffer;

struct TempTreeDir {
    path: PathBuf,
}

impl TempTreeDir {
    fn new(label: &str) -> Self {
        let mut path = std::env::temp_dir();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        path.push(format!("textual-rs-{label}-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&path).expect("create temp test directory");
        Self { path }
    }
}

impl Drop for TempTreeDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn options_for(console: &Console, width: usize, height: usize) -> rich_rs::ConsoleOptions {
    let mut options = console.options().clone();
    options.size = (width, height);
    options.max_width = width;
    options.max_height = height;
    options
}

#[test]
fn directory_tree_renders_root_and_visible_entries() {
    let temp = TempTreeDir::new("directory-tree-visible");
    fs::create_dir_all(temp.path.join("src")).expect("create child dir");
    fs::write(temp.path.join("Cargo.toml"), "[package]\nname = \"demo\"\n").expect("write file");
    fs::write(temp.path.join(".hidden"), "hidden").expect("write hidden file");

    let console = Console::new();
    let options = options_for(&console, 48, 6);
    let mut tree = DirectoryTree::new(&temp.path);
    tree.on_layout(48, 6);

    let buf = FrameBuffer::from_renderable(&console, &options, &tree, None);
    let lines = buf.as_plain_lines();

    assert!(lines.iter().any(|line| line.contains("src")));
    assert!(lines.iter().any(|line| line.contains("Cargo.toml")));
    assert!(!lines.iter().any(|line| line.contains(".hidden")));
}

#[test]
fn directory_tree_lazy_loads_children_on_expand_message_flow() {
    let temp = TempTreeDir::new("directory-tree-expand");
    let nested_dir = temp.path.join("nested");
    fs::create_dir_all(&nested_dir).expect("create nested dir");
    fs::write(nested_dir.join("leaf.txt"), "leaf").expect("write nested file");

    let mut tree = DirectoryTree::new(&temp.path);
    tree.on_layout(60, 8);

    let mut message_ctx = EventCtx::default();
    tree.on_message(
        &MessageEvent {
            sender: tree.tree_id(),
            message: Message::TreeNodeToggled {
                index: 1,
                label: "nested".to_string(),
                expanded: true,
            },
        },
        &mut message_ctx,
    );
    assert!(message_ctx.handled());

    let console = Console::new();
    let options = options_for(&console, 60, 8);
    let buf = FrameBuffer::from_renderable(&console, &options, &tree, None);
    let lines = buf.as_plain_lines();

    assert!(lines.iter().any(|line| line.contains("leaf.txt")));
}

#[test]
fn directory_tree_handles_forwarded_selection_messages() {
    let temp = TempTreeDir::new("directory-tree-message");
    fs::write(temp.path.join("alpha.txt"), "alpha").expect("write file");

    let mut tree = DirectoryTree::new(&temp.path);
    tree.on_layout(40, 4);

    let mut message_ctx = EventCtx::default();
    tree.on_message(
        &MessageEvent {
            sender: tree.tree_id(),
            message: Message::TreeNodeSelected {
                index: 1,
                label: "alpha.txt".to_string(),
            },
        },
        &mut message_ctx,
    );

    assert!(message_ctx.handled());
}

#[test]
fn directory_tree_keyboard_navigation_is_forwarded_to_inner_tree() {
    let temp = TempTreeDir::new("directory-tree-key");
    fs::write(temp.path.join("alpha.txt"), "alpha").expect("write file");
    fs::write(temp.path.join("beta.txt"), "beta").expect("write file");

    let mut tree = DirectoryTree::new(&temp.path);
    tree.set_focus(true);
    tree.on_layout(40, 4);

    let down = KeyEventData::from_crossterm(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    let mut ctx = EventCtx::default();
    tree.on_event(&Event::Key(down), &mut ctx);
    assert!(ctx.handled());
}
