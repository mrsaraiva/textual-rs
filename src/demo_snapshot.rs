use std::path::{Path, PathBuf};

use rich_rs::Console;

use crate::prelude::Result;
use crate::prelude::Widget;
use crate::widget::{StyleSheet, WidgetRenderable, default_widget_stylesheet, set_style_context};

#[derive(Debug, Clone)]
pub struct SnapshotArgs {
    pub path: PathBuf,
    pub width: usize,
    pub height: usize,
    pub title: String,
}

impl SnapshotArgs {
    pub fn parse() -> Option<Self> {
        let mut args = std::env::args().skip(1);
        let mut snapshot: Option<PathBuf> = None;
        let mut width: usize = 120;
        let mut height: usize = 30;
        let mut title: Option<String> = None;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--snapshot" => {
                    if let Some(value) = args.next() {
                        snapshot = Some(PathBuf::from(value));
                    }
                }
                "--width" => {
                    if let Some(value) = args.next() {
                        width = value.parse().unwrap_or(width);
                    }
                }
                "--height" => {
                    if let Some(value) = args.next() {
                        height = value.parse().unwrap_or(height);
                    }
                }
                "--title" => {
                    if let Some(value) = args.next() {
                        title = Some(value);
                    }
                }
                _ => {}
            }
        }

        snapshot.map(|path| SnapshotArgs {
            path,
            width,
            height,
            title: title.unwrap_or_else(|| "textual-rs demo".to_string()),
        })
    }
}

pub fn snapshot_widget(
    widget: &dyn Widget,
    args: &SnapshotArgs,
    css_path: Option<&Path>,
) -> Result<()> {
    let mut stylesheet = default_widget_stylesheet();
    if let Some(path) = css_path {
        if let Ok(css) = std::fs::read_to_string(path) {
            stylesheet.extend(&StyleSheet::parse(&css));
        }
    }
    let _guard = set_style_context(stylesheet);

    let mut console = Console::new_with_record();
    {
        let options = console.options_mut();
        options.size = (args.width, args.height);
        options.max_width = args.width;
        options.max_height = args.height;
        console.sync_from_options();
    }

    let renderable = WidgetRenderable::new(widget);
    console.print(&renderable, None, None, None, false, "")?;
    console.save_svg(
        args.path.to_string_lossy().as_ref(),
        &args.title,
        None,
        true,
        0.61,
        None,
    )?;
    Ok(())
}
