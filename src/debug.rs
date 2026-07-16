use rich_rs::{Color, Style};
use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct DebugLayout {
    pub enabled: bool,
    pub show_sizes: bool,
    pub colors: Vec<u8>,
}

impl DebugLayout {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            show_sizes: false,
            colors: vec![196, 202, 208, 214, 220, 82, 45, 27, 129],
        }
    }

    pub fn enabled() -> Self {
        let mut layout = Self::disabled();
        layout.enabled = true;
        layout.show_sizes = true;
        layout
    }

    pub fn style_for(&self, index: usize) -> Style {
        let color = self.colors[index % self.colors.len()];
        Style::color(Color::from_ansi(color).into())
    }
}

impl Default for DebugLayout {
    fn default() -> Self {
        Self::disabled()
    }
}

// ---------------------------------------------------------------------------
// Debug channels
// ---------------------------------------------------------------------------

/// A named debug channel.
///
/// Each channel maps to one `TEXTUAL_DEBUG_*_FILE` environment variable (the
/// existing file-based instrumentation) plus an optional live stream over the
/// devtools socket (the `LOGS` protocol request). [`DebugChannel::App`] is the
/// app-facing channel fed by [`log`]; it has no framework noise of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugChannel {
    Input,
    Layout,
    Style,
    Render,
    Timing,
    Message,
    Border,
    App,
}

impl DebugChannel {
    /// Every channel, in the order used by introspection listings.
    pub const ALL: [DebugChannel; 8] = [
        DebugChannel::Input,
        DebugChannel::Layout,
        DebugChannel::Style,
        DebugChannel::Render,
        DebugChannel::Timing,
        DebugChannel::Message,
        DebugChannel::Border,
        DebugChannel::App,
    ];

    /// Stable lowercase name used on the wire (`LOGS` records, `CHANNELS`
    /// listings, `DEBUG_CHANNEL` toggles).
    pub fn name(self) -> &'static str {
        match self {
            DebugChannel::Input => "input",
            DebugChannel::Layout => "layout",
            DebugChannel::Style => "style",
            DebugChannel::Render => "render",
            DebugChannel::Timing => "timing",
            DebugChannel::Message => "message",
            DebugChannel::Border => "border",
            DebugChannel::App => "app",
        }
    }

    /// Parse a wire name back into a channel (case-insensitive).
    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.trim().to_ascii_lowercase();
        Self::ALL.into_iter().find(|channel| channel.name() == name)
    }

    fn index(self) -> usize {
        match self {
            DebugChannel::Input => 0,
            DebugChannel::Layout => 1,
            DebugChannel::Style => 2,
            DebugChannel::Render => 3,
            DebugChannel::Timing => 4,
            DebugChannel::Message => 5,
            DebugChannel::Border => 6,
            DebugChannel::App => 7,
        }
    }

    fn env_var(self) -> &'static str {
        match self {
            DebugChannel::Input => "TEXTUAL_DEBUG_INPUT_FILE",
            DebugChannel::Layout => "TEXTUAL_DEBUG_LAYOUT_FILE",
            DebugChannel::Style => "TEXTUAL_DEBUG_STYLE_FILE",
            DebugChannel::Render => "TEXTUAL_DEBUG_RENDER_FILE",
            DebugChannel::Timing => "TEXTUAL_DEBUG_TIMING_FILE",
            DebugChannel::Message => "TEXTUAL_DEBUG_MESSAGE_FILE",
            DebugChannel::Border => "TEXTUAL_DEBUG_BORDER_FILE",
            DebugChannel::App => "TEXTUAL_DEBUG_APP_FILE",
        }
    }

    /// The channel's log-file path (from its `TEXTUAL_DEBUG_*_FILE` env var),
    /// read once per process. `None` when the env var is unset.
    pub fn file_path(self) -> Option<&'static str> {
        static PATHS: OnceLock<[Option<String>; 8]> = OnceLock::new();
        let paths = PATHS
            .get_or_init(|| DebugChannel::ALL.map(|channel| std::env::var(channel.env_var()).ok()));
        paths[self.index()].as_deref()
    }
}

// ---------------------------------------------------------------------------
// Devtools log stream hub
// ---------------------------------------------------------------------------

/// Maximum number of recent log records replayed to a newly attached `LOGS`
/// subscriber. Bounded so an idle devtools session never grows memory.
const LOG_BACKLOG_CAP: usize = 500;

struct LogHubState {
    backlog: VecDeque<String>,
    subscribers: Vec<Sender<String>>,
}

/// Process-wide fan-out point between the debug channels and devtools `LOGS`
/// subscribers. Inert (a single relaxed atomic load per emission) until the
/// devtools server activates it.
struct LogHub {
    /// Set when the devtools server is running (or a subscriber attached);
    /// while false, emissions skip the hub entirely.
    active: AtomicBool,
    /// Per-channel stream gate, indexed by [`DebugChannel::index`]. Defaults:
    /// a channel streams if its `TEXTUAL_DEBUG_*_FILE` env var is set (it is
    /// already active instrumentation); the `app` channel always defaults on
    /// (explicit app-facing logging). The devtools `DEBUG_CHANNEL` request
    /// toggles these at runtime.
    streaming: [AtomicBool; 8],
    state: Mutex<LogHubState>,
}

fn log_hub() -> &'static LogHub {
    static HUB: OnceLock<LogHub> = OnceLock::new();
    HUB.get_or_init(|| LogHub {
        active: AtomicBool::new(false),
        streaming: DebugChannel::ALL.map(|channel| {
            AtomicBool::new(matches!(channel, DebugChannel::App) || channel.file_path().is_some())
        }),
        state: Mutex::new(LogHubState {
            backlog: VecDeque::new(),
            subscribers: Vec::new(),
        }),
    })
}

/// Activate the log stream hub. Called by the devtools server on startup so
/// stream-enabled channels start collecting the bounded backlog; without a
/// devtools server the hub stays inert.
pub(crate) fn activate_log_stream() {
    log_hub().active.store(true, Ordering::Relaxed);
}

/// Whether `channel` currently streams to the devtools log sink.
pub fn channel_streaming(channel: DebugChannel) -> bool {
    log_hub().streaming[channel.index()].load(Ordering::Relaxed)
}

/// Enable/disable devtools streaming for `channel` (the `DEBUG_CHANNEL`
/// protocol request). File logging stays env-gated and is unaffected.
pub fn set_channel_streaming(channel: DebugChannel, enabled: bool) {
    log_hub().streaming[channel.index()].store(enabled, Ordering::Relaxed);
}

/// Introspection listing for every channel: `(name, file_path, streaming)`.
/// Backs the devtools `CHANNELS` request.
pub fn channel_states() -> Vec<(&'static str, Option<&'static str>, bool)> {
    DebugChannel::ALL
        .into_iter()
        .map(|channel| {
            (
                channel.name(),
                channel.file_path(),
                channel_streaming(channel),
            )
        })
        .collect()
}

/// Subscribe to the live log stream: returns a snapshot of the recent backlog
/// plus a receiver fed one formatted record per emission. Used by the devtools
/// `LOGS` client thread. Subscribing activates the hub.
pub(crate) fn subscribe_log_stream() -> (Vec<String>, Receiver<String>) {
    let hub = log_hub();
    hub.active.store(true, Ordering::Relaxed);
    let (tx, rx) = mpsc::channel();
    let backlog = match hub.state.lock() {
        Ok(mut state) => {
            state.subscribers.push(tx);
            state.backlog.iter().cloned().collect()
        }
        Err(_) => Vec::new(),
    };
    (backlog, rx)
}

/// One wire record: `unix_millis<TAB>channel<TAB>message` (single line).
fn format_log_record(channel: DebugChannel, line: &str) -> String {
    let ts_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let sanitized: String = line
        .chars()
        .map(|c| match c {
            '\t' => ' ',
            '\n' | '\r' => ' ',
            other => other,
        })
        .collect();
    format!("{ts_ms}\t{}\t{sanitized}", channel.name())
}

fn forward_to_log_stream(channel: DebugChannel, line: &str) {
    let hub = log_hub();
    if !hub.active.load(Ordering::Relaxed) {
        return;
    }
    if !hub.streaming[channel.index()].load(Ordering::Relaxed) {
        return;
    }
    let record = format_log_record(channel, line);
    if let Ok(mut state) = hub.state.lock() {
        state.backlog.push_back(record.clone());
        while state.backlog.len() > LOG_BACKLOG_CAP {
            state.backlog.pop_front();
        }
        state
            .subscribers
            .retain(|subscriber| subscriber.send(record.clone()).is_ok());
    }
}

/// Whether emitting on `channel` has any sink: an env-gated log file, or an
/// active devtools log stream with the channel toggled on. Producers with
/// expensive line formatting gate on this instead of a raw env check, so a
/// devtools `DEBUG_CHANNEL <name> on` toggle activates them at runtime.
pub(crate) fn channel_enabled(channel: DebugChannel) -> bool {
    if channel.file_path().is_some() {
        return true;
    }
    let hub = log_hub();
    hub.active.load(Ordering::Relaxed) && hub.streaming[channel.index()].load(Ordering::Relaxed)
}

/// Emit one line on `channel`: append to its env-gated log file (if any) and
/// forward to the devtools log stream (if the hub is active and the channel
/// streams).
fn emit(channel: DebugChannel, line: &str) {
    if let Some(path) = channel.file_path()
        && let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path)
    {
        let _ = writeln!(file, "{line}");
    }
    forward_to_log_stream(channel, line);
}

/// App-facing devtools log (the Rust analogue of Python's `self.log(...)`).
///
/// The message streams to any attached devtools `LOGS` subscriber on the
/// `app` channel and, when `TEXTUAL_DEBUG_APP_FILE` is set, is appended to
/// that file. Near-zero cost when neither sink is attached.
pub fn log(message: impl std::fmt::Display) {
    emit(DebugChannel::App, &message.to_string());
}

pub(crate) fn debug_input(line: &str) {
    emit(DebugChannel::Input, line);
}

pub(crate) fn debug_layout(line: &str) {
    emit(DebugChannel::Layout, line);
}

pub(crate) fn debug_style(line: &str) {
    emit(DebugChannel::Style, line);
}

pub(crate) fn debug_render(line: &str) {
    emit(DebugChannel::Render, line);
}

pub(crate) fn timing_enabled() -> bool {
    DebugChannel::Timing.file_path().is_some()
}

pub(crate) fn debug_timing(line: &str) {
    emit(DebugChannel::Timing, line);
}

pub(crate) fn debug_message(line: &str) {
    emit(DebugChannel::Message, line);
}

pub(crate) fn debug_border(line: &str) {
    emit(DebugChannel::Border, line);
}

pub(crate) fn border_debug_matches(label: &str) -> bool {
    if !channel_enabled(DebugChannel::Border) {
        return false;
    }
    static FILTERS: OnceLock<Vec<String>> = OnceLock::new();
    let filters = FILTERS.get_or_init(|| {
        std::env::var("TEXTUAL_DEBUG_BORDER_FILTER")
            .ok()
            .map(|value| {
                value
                    .split(',')
                    .map(|part| part.trim().to_ascii_lowercase())
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    });
    if filters.is_empty() {
        return true;
    }
    let label = label.to_ascii_lowercase();
    filters.iter().all(|filter| label.contains(filter))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn channel_names_roundtrip() {
        for channel in DebugChannel::ALL {
            assert_eq!(DebugChannel::from_name(channel.name()), Some(channel));
        }
        // Case-insensitive, trimmed.
        assert_eq!(DebugChannel::from_name(" APP "), Some(DebugChannel::App));
        assert!(DebugChannel::from_name("nope").is_none());
    }

    #[test]
    fn channel_states_lists_every_channel() {
        let states = channel_states();
        assert_eq!(states.len(), DebugChannel::ALL.len());
        for (channel, (name, _file, _streaming)) in DebugChannel::ALL.iter().zip(&states) {
            assert_eq!(channel.name(), *name);
        }
    }

    /// One test owns the `timing` channel's stream flag end to end (the flag
    /// is process-global; splitting these assertions across tests would race
    /// under the parallel test runner). The `app` channel is deliberately not
    /// toggled here: the devtools socket tests rely on its default.
    #[test]
    fn log_stream_respects_channel_toggle_and_formats_records() {
        let (_backlog, rx) = subscribe_log_stream();

        // `timing` defaults off (no env file in the test environment): an
        // emission must not stream.
        assert!(!channel_streaming(DebugChannel::Timing));
        debug_timing("timing-marker-while-off");

        // The `app` channel defaults on; use it as an ordering fence: the
        // hub preserves emission order per subscriber.
        let fence = "app-fence-after-timing-off";
        log(fence);
        let mut saw_fence = false;
        for _ in 0..50 {
            let record = rx
                .recv_timeout(Duration::from_secs(10))
                .expect("app record should stream");
            assert!(
                !record.contains("timing-marker-while-off"),
                "toggled-off channel must not stream: {record}"
            );
            if record.ends_with(fence) {
                saw_fence = true;
                break;
            }
        }
        assert!(saw_fence, "fence record should arrive");

        // Toggle on: the emission streams, formatted as ts<TAB>channel<TAB>msg
        // with tabs/newlines in the message flattened to spaces.
        set_channel_streaming(DebugChannel::Timing, true);
        assert!(channel_streaming(DebugChannel::Timing));
        assert!(channel_enabled(DebugChannel::Timing));
        debug_timing("timing\tmarker\nwhile-on");
        let mut found = None;
        for _ in 0..50 {
            let record = rx
                .recv_timeout(Duration::from_secs(10))
                .expect("timing record should stream");
            if record.contains("timing marker while-on") {
                found = Some(record);
                break;
            }
        }
        let record = found.expect("toggled-on channel should stream");
        let mut fields = record.split('\t');
        let ts: u64 = fields
            .next()
            .expect("timestamp field")
            .parse()
            .expect("numeric unix-millis timestamp");
        assert!(ts > 0);
        assert_eq!(fields.next(), Some("timing"));
        assert_eq!(fields.next(), Some("timing marker while-on"));
        assert_eq!(fields.next(), None);

        // Restore the default for other tests.
        set_channel_streaming(DebugChannel::Timing, false);
        assert!(!channel_streaming(DebugChannel::Timing));
    }
}
