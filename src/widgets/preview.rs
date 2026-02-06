use super::{AppRoot, Dock, Header, Widget};

pub fn preview_root(title: Option<&str>, body: impl Widget + 'static) -> AppRoot {
    let content = Dock::new().push_fill(body);
    let layout = with_title(content, title);
    AppRoot::new().with_child(layout)
}

pub fn preview_root_with_bottom(
    title: Option<&str>,
    body: impl Widget + 'static,
    bottom_height: Option<usize>,
    bottom: impl Widget + 'static,
) -> AppRoot {
    let content = Dock::new()
        .push_fill(body)
        .push_bottom(bottom_height, bottom);
    let layout = with_title(content, title);
    AppRoot::new().with_child(layout)
}

pub fn preview_root_with_top_bottom(
    title: Option<&str>,
    top_height: Option<usize>,
    top: impl Widget + 'static,
    body: impl Widget + 'static,
    bottom_height: Option<usize>,
    bottom: impl Widget + 'static,
) -> AppRoot {
    let content = Dock::new()
        .push_top(top_height, top)
        .push_fill(body)
        .push_bottom(bottom_height, bottom);
    let layout = with_title(content, title);
    AppRoot::new().with_child(layout)
}

fn with_title(content: Dock, title: Option<&str>) -> Dock {
    match title {
        Some(value) => Dock::new()
            .push_top(None, Header::new().title(value))
            .push_fill(content),
        None => Dock::new().push_fill(content),
    }
}
