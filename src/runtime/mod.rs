use crate::driver::{Size, TerminalDriver};
use crate::render::FrameBuffer;
use crate::debug::DebugLayout;
use crate::event::{Action, ActionMap, Event, EventCtx, KeyBind};
use crate::widget::Widget;
use crate::{Error, Result};
use crossterm::event::{
    self, Event as CrosstermEvent, KeyCode, KeyEventKind, KeyModifiers,
};
use rich_rs::{Console, ConsoleOptions, Renderable};
use std::time::{Duration, Instant};

pub struct App {
    driver: TerminalDriver,
    console: Console,
    options: ConsoleOptions,
    frame: FrameBuffer,
    debug_layout: DebugLayout,
    action_map: ActionMap,
    running: bool,
}

impl App {
    pub fn new() -> Result<Self> {
        let driver = TerminalDriver::new()?;
        let console = Console::new();
        let mut options = console.options().clone();
        let size = driver.size();
        apply_size(&mut options, size);
        let frame = FrameBuffer::new(size.width as usize, size.height as usize, None);
        Ok(Self {
            driver,
            console,
            options,
            frame,
            debug_layout: DebugLayout::default(),
            action_map: default_action_map(),
            running: true,
        })
    }

    pub fn driver(&self) -> &TerminalDriver {
        &self.driver
    }

    pub fn set_debug_layout(&mut self, debug: DebugLayout) {
        self.debug_layout = debug;
    }

    pub fn enable_debug_layout(&mut self, enabled: bool) {
        self.debug_layout.enabled = enabled;
    }

    pub fn bind_key(&mut self, key: KeyBind, action: Action) {
        self.action_map.bind(key, action);
    }

    pub fn start(&mut self) -> Result<()> {
        self.driver.start()?;
        self.refresh_size()?;
        Ok(())
    }

    pub fn finish(&mut self) -> Result<()> {
        self.driver.stop()
    }

    pub fn render(&mut self, renderable: &dyn Renderable) -> Result<()> {
        self.refresh_size()?;
        let next = FrameBuffer::from_renderable(&self.console, &self.options, renderable, None);
        let diff = next.diff_to_segments(&self.frame);
        self.console.print_segments(&diff)?;
        self.frame = next;
        Ok(())
    }

    pub fn render_widget(&mut self, widget: &dyn Widget) -> Result<()> {
        self.refresh_size()?;
        let segments = if self.debug_layout.enabled {
            widget.render_with_debug(&self.console, &self.options, &self.debug_layout)
        } else {
            widget.render(&self.console, &self.options)
        };
        let (width, height) = self.options.size;
        let lines = rich_rs::Segment::split_and_crop_lines(segments, width, None, true, false);
        let next = FrameBuffer::from_lines(&lines, width, height, None);
        let diff = next.diff_to_segments(&self.frame);
        self.console.print_segments(&diff)?;
        self.frame = next;
        Ok(())
    }

    pub fn stop(&mut self) {
        self.running = false;
    }

    pub async fn run(&mut self) -> Result<()> {
        if !self.running {
            return Err(Error::RuntimeStopped);
        }
        // Placeholder event loop; real driver + frame diff will live here.
        self.start()?;
        Ok(())
    }

    pub async fn run_with<F, R>(&mut self, mut render: F) -> Result<()>
    where
        F: FnMut(&mut App, u64) -> R,
        R: Renderable,
    {
        if !self.running {
            return Err(Error::RuntimeStopped);
        }

        self.start()?;

        let mut tick: u64 = 0;
        let tick_rate = Duration::from_millis(100);
        let mut last_render = Instant::now();

        loop {
            let timeout = tick_rate.saturating_sub(last_render.elapsed());
            if event::poll(timeout)? {
                match event::read()? {
                    CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press => {
                        if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                            break;
                        }
                    }
                    CrosstermEvent::Resize(_, _) => {
                        self.refresh_size()?;
                    }
                    _ => {}
                }
            }

            if last_render.elapsed() >= tick_rate {
                let renderable = render(self, tick);
                self.render(&renderable)?;
                tick += 1;
                last_render = Instant::now();
            }
        }

        self.finish()?;
        Ok(())
    }

    pub async fn run_widget_tree(&mut self, root: &mut dyn Widget) -> Result<()> {
        if !self.running {
            return Err(Error::RuntimeStopped);
        }

        self.start()?;
        root.on_mount();

        let mut tick: u64 = 0;
        let tick_rate = Duration::from_millis(100);
        let mut last_render = Instant::now();

        loop {
            let timeout = tick_rate.saturating_sub(last_render.elapsed());
            if event::poll(timeout)? {
                match event::read()? {
                    CrosstermEvent::Key(key) if key.kind == KeyEventKind::Press => {
                        if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                            break;
                        }
                        let bind = KeyBind::from_event(&key);
                        if let Some(action) = self.action_map.lookup(&bind) {
                            dispatch_event(root, Event::Action(action));
                        } else {
                            dispatch_event(root, Event::Key(key));
                        }
                    }
                    CrosstermEvent::Resize(_, _) => {
                        self.refresh_size()?;
                        let size = self.driver.size();
                        root.on_resize(size.width, size.height);
                        dispatch_event(root, Event::Resize(size.width, size.height));
                    }
                    _ => {}
                }
            }

            if last_render.elapsed() >= tick_rate {
                root.on_tick(tick);
                dispatch_event(root, Event::Tick(tick));
                self.render_widget(root)?;
                tick += 1;
                last_render = Instant::now();
            }
        }

        root.on_unmount();
        self.finish()?;
        Ok(())
    }

    fn refresh_size(&mut self) -> Result<()> {
        let size = self.driver.refresh_size()?;
        apply_size(&mut self.options, size);
        if self.frame.width != size.width as usize || self.frame.height != size.height as usize {
            self.frame = FrameBuffer::new(size.width as usize, size.height as usize, None);
        }
        Ok(())
    }
}

fn apply_size(options: &mut ConsoleOptions, size: Size) {
    let width = size.width as usize;
    let height = size.height as usize;
    options.size = (width, height);
    options.max_width = width;
    options.max_height = height;
}

fn default_action_map() -> ActionMap {
    let mut map = ActionMap::new();
    map.bind(
        KeyBind::new(KeyCode::Tab, KeyModifiers::empty()),
        Action::FocusNext,
    );
    map.bind(
        KeyBind::new(KeyCode::BackTab, KeyModifiers::SHIFT),
        Action::FocusPrev,
    );
    map.bind(
        KeyBind::new(KeyCode::Up, KeyModifiers::empty()),
        Action::ScrollUp,
    );
    map.bind(
        KeyBind::new(KeyCode::Down, KeyModifiers::empty()),
        Action::ScrollDown,
    );
    map.bind(
        KeyBind::new(KeyCode::PageUp, KeyModifiers::empty()),
        Action::ScrollPageUp,
    );
    map.bind(
        KeyBind::new(KeyCode::PageDown, KeyModifiers::empty()),
        Action::ScrollPageDown,
    );
    map.bind(
        KeyBind::new(KeyCode::Char('k'), KeyModifiers::empty()),
        Action::ScrollUp,
    );
    map.bind(
        KeyBind::new(KeyCode::Char('j'), KeyModifiers::empty()),
        Action::ScrollDown,
    );
    map
}

fn dispatch_event(root: &mut dyn Widget, event: Event) {
    let mut ctx = EventCtx::default();
    root.on_event_capture(&event, &mut ctx);
    if !ctx.handled() {
        root.on_event(&event, &mut ctx);
    }
}
