use textual::compose;
use textual::prelude::*;

const EVENTS_MD: &str = r#"## Recent Events

| Time  | Event            | Status  |
|-------|------------------|---------|
| 14:23 | Deploy v2.1.0    | Success |
| 14:15 | Health check     | OK      |
| 14:02 | Config reload    | Success |
| 13:58 | Backup started   | Running |
"#;

struct MissionControl;

impl TextualApp for MissionControl {
    fn compose(&mut self) -> AppRoot {
        // -- Sidebar: System Metrics ------------------------------------------
        let cpu_data = vec![
            45.0, 52.0, 48.0, 67.0, 72.0, 58.0, 63.0, 55.0, 70.0, 61.0, 49.0, 75.0, 68.0, 53.0,
            60.0,
        ];

        let mut disk_bar = ProgressBar::new(Some(100.0));
        disk_bar.advance(73.0);

        let mut proc_table = DataTable::empty();
        proc_table.add_columns(["PID", "Name", "CPU%", "Mem"]);
        proc_table.add_rows([
            &["1842", "rustc", "24.3", "512M"],
            &["3201", "cargo", "18.7", "256M"],
            &["1024", "tmux", "2.1", "48M"],
            &["2048", "nvim", "5.4", "128M"],
            &["4096", "zsh", "0.3", "32M"],
        ]);

        let sidebar = Node::new(Container::new().with_compose(compose![
            Static::new("System Metrics").class("section-title"),
            Sparkline::new(cpu_data),
            Static::new("Disk Usage").class("section-title"),
            disk_bar,
            Rule::horizontal(),
            ScrollView::new(proc_table),
        ]))
        .class("sidebar");

        // -- Right column: Tabbed Content -------------------------------------
        let events_pane = TabPane::new("Events", Markdown::new(EVENTS_MD)).id("events");

        let config_pane = TabPane::new(
            "Config",
            Node::new(Container::new().with_compose(compose![
                Input::new().with_placeholder("Hostname"),
                Checkbox::new("Enable notifications"),
                Static::new("Dark mode"),
                Switch::new(false),
                Button::primary("Apply"),
            ]))
            .class("config-form"),
        )
        .id("config");

        let tabs = TabbedContent::new()
            .with_pane(events_pane)
            .with_pane(config_pane);

        let body = Horizontal::new().with_compose(compose![sidebar, tabs]);

        AppRoot::new()
            .with_child(Header::new().title("Mission Control — textual-rs"))
            .with_child(body)
            .with_child(Footer::new())
    }

    fn css_path(&self) -> Option<&'static str> {
        Some(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/hello/hello.tcss"
        ))
    }

    fn bindings(&self) -> Vec<BindingDecl> {
        vec![
            BindingDecl::new("e", "show_tab('events')", "Events"),
            BindingDecl::new("c", "show_tab('config')", "Config"),
        ]
    }
}

fn main() -> Result<()> {
    if cfg!(test) {
        return Ok(());
    }
    run_sync(MissionControl)
}

#[cfg(test)]
mod liveness {
    use super::*;
    use textual::run_test;

    /// LIVENESS: the showcase contains a `TabbedContent` with two panes
    /// ("events" + "config"). Clicking the second tab switches the active pane,
    /// changing the rendered body. The frame must change. Proves the
    /// TabbedContent tab-switch interaction is wired.
    ///
    /// We discover the second tab's screen position from the `Tab` nodes' rects
    /// and click its centre (rather than a hard-coded coordinate).
    #[test]
    fn clicking_tab_switches_pane() {
        run_test(MissionControl, |pilot| {
            let before = pilot.app().frame_fingerprint();
            let tab_ids: Vec<_> = pilot
                .app()
                .query("Tab")
                .map(|q| q.into_ids())
                .unwrap_or_default();
            assert!(
                tab_ids.len() >= 2,
                "TabbedContent must expose at least two Tab nodes, got {}",
                tab_ids.len()
            );
            // Click the centre of the second tab.
            let rect = pilot
                .app()
                .node_screen_rect(tab_ids[1])
                .expect("second tab must have a rendered region");
            let cx = rect.0 + (rect.2.saturating_sub(rect.0)) / 2;
            let cy = rect.1 + (rect.3.saturating_sub(rect.1)) / 2;
            pilot.click_at(cx, cy)?;
            let after = pilot.app().frame_fingerprint();
            assert_ne!(
                before, after,
                "clicking the second tab must switch the active pane and change the frame"
            );
            Ok(())
        })
        .unwrap();
    }
}
