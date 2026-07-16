//! In-process devtools server: a line-oriented TCP protocol consumed by the
//! external `textual-dev-rs` harness.
//!
//! Enabled via `TEXTUAL_DEVTOOLS=1` (or `TEXTUAL_DEVTOOLS_BIND=addr`). Each
//! running app writes a `<pid>.instance` discovery file (key=value lines)
//! under `TEXTUAL_DEVTOOLS_ROOT` (default: `$TMP/textual-rs-devtools`).
//!
//! Wire protocol: the client sends ONE request line, the server answers with
//! `OK <detail>\n`, `ERR <detail>\n`, or one/more `DATA <len>\n<len bytes>`
//! frames (streaming requests keep sending frames until disconnect).
//!
//! Requests (protocol revision [`PROTOCOL_VERSION`]):
//! - `PING` -> `OK PONG`
//! - `INFO` -> DATA: `key=value` lines (`pid`, `app`, `addr`, `started_unix`,
//!   `protocol`; unknown keys must be ignored by clients)
//! - `SNAPSHOT` -> DATA: latest widget-tree snapshot (its own `version` field)
//! - `WATCH` -> DATA stream: one frame per published snapshot
//! - `LOGS` (rev 3) -> DATA stream: first frame replays the bounded backlog,
//!   then one frame per log record. Records are single lines of
//!   `unix_millis<TAB>channel<TAB>message`; a frame carries one or more
//!   newline-terminated records
//! - `CHANNELS` (rev 3) -> DATA: `protocol\t<rev>` header plus one
//!   `channel\t<name>\t<streaming 0|1>\t<file-path or ->` line per debug
//!   channel
//! - `DEBUG_CHANNEL <name> <on|off>` (rev 3) -> `OK`: toggle streaming of a
//!   debug channel into `LOGS` (file logging stays env-gated)
//! - `FOCUS <id>` / `DEBUG_LAYOUT <on|off>` / `TOGGLE_DISPLAY <id>` /
//!   `HIGHLIGHT <id>` / `ADD_CLASS <id> <class>` / `REMOVE_CLASS <id> <class>`
//!   / `QUIT` -> `OK queued` (applied by the app loop next frame)
//!
//! Compatibility: revisions only ADD request types and key=value fields;
//! existing responses are never reshaped. Clients that predate a revision are
//! unaffected; newer clients read `protocol` from `INFO` (absent means rev 2).

use crate::debug::{self, DebugChannel as LogChannel};
use crate::node_id::{NodeId, node_id_from_ffi};
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Sender},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const ENV_ENABLE: &str = "TEXTUAL_DEVTOOLS";
const ENV_BIND: &str = "TEXTUAL_DEVTOOLS_BIND";
const ENV_ROOT: &str = "TEXTUAL_DEVTOOLS_ROOT";
const DEFAULT_BIND: &str = "127.0.0.1:0";

/// Devtools protocol revision, advertised as `protocol=<rev>` in `INFO`
/// responses and instance files. History:
/// - rev 1/2: snapshot-era protocol without an advertised revision (the
///   snapshot payload carries its own `version` field, currently 2)
/// - rev 3: adds the `LOGS` stream, `CHANNELS` introspection, and
///   `DEBUG_CHANNEL` toggles, plus this `protocol` field
pub(crate) const PROTOCOL_VERSION: u32 = 3;

#[derive(Debug, Clone)]
pub(crate) enum DevtoolsCommand {
    Focus(NodeId),
    SetDebugLayout(bool),
    ToggleDisplay(NodeId),
    Highlight(NodeId),
    AddClass(NodeId, String),
    RemoveClass(NodeId, String),
    Quit,
}

#[derive(Debug, Default)]
struct SharedState {
    snapshot: Mutex<String>,
    pending: Mutex<Vec<DevtoolsCommand>>,
    watchers: Mutex<Vec<Sender<String>>>,
}

pub(crate) struct DevtoolsRuntime {
    shared: Arc<SharedState>,
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    registry_file: Option<PathBuf>,
}

impl DevtoolsRuntime {
    pub(crate) fn from_env() -> io::Result<Option<Self>> {
        let enabled = std::env::var(ENV_ENABLE)
            .ok()
            .map(|value| {
                let value = value.trim().to_ascii_lowercase();
                matches!(value.as_str(), "1" | "true" | "yes" | "on")
            })
            .unwrap_or(false)
            || std::env::var(ENV_BIND).is_ok();

        if !enabled {
            return Ok(None);
        }

        let bind = std::env::var(ENV_BIND).unwrap_or_else(|_| DEFAULT_BIND.to_string());
        let listener = TcpListener::bind(&bind)?;
        Self::spawn(listener, true).map(Some)
    }

    /// Bind an ephemeral server without env gating or an instance file.
    /// Returns the runtime plus the bound address. Test-only entry point for
    /// exercising the wire protocol end-to-end.
    #[cfg(test)]
    pub(crate) fn bind_for_test() -> io::Result<(Self, String)> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?.to_string();
        Ok((Self::spawn(listener, false)?, addr))
    }

    fn spawn(listener: TcpListener, write_registry: bool) -> io::Result<Self> {
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?.to_string();

        let pid = std::process::id();
        let app_name = current_app_name();
        let registry_file = if write_registry {
            let registry_root = devtools_root();
            fs::create_dir_all(&registry_root)?;
            let registry_file = registry_root.join(format!("{pid}.instance"));
            write_instance_file(&registry_file, pid, &app_name, &addr)?;
            Some(registry_file)
        } else {
            None
        };

        // The devtools socket carries the log stream (`LOGS`); arm the debug
        // log hub so stream-enabled channels start collecting backlog.
        debug::activate_log_stream();

        let shared = Arc::new(SharedState::default());
        let shared_thread = Arc::clone(&shared);
        let running = Arc::new(AtomicBool::new(true));
        let running_thread = Arc::clone(&running);
        let thread = thread::Builder::new()
            .name("textual-devtools".to_string())
            .spawn(move || {
                server_loop(listener, shared_thread, running_thread, pid, app_name, addr);
            })?;

        Ok(Self {
            shared,
            running,
            thread: Some(thread),
            registry_file,
        })
    }

    pub(crate) fn publish_snapshot(&self, snapshot: String) {
        if let Ok(mut slot) = self.shared.snapshot.lock() {
            *slot = snapshot.clone();
        }
        if let Ok(mut watchers) = self.shared.watchers.lock() {
            watchers.retain(|watcher| watcher.send(snapshot.clone()).is_ok());
        }
    }

    pub(crate) fn drain_commands(&self) -> Vec<DevtoolsCommand> {
        if let Ok(mut pending) = self.shared.pending.lock() {
            return std::mem::take(&mut *pending);
        }
        Vec::new()
    }
}

impl Drop for DevtoolsRuntime {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
        if let Some(path) = self.registry_file.take() {
            let _ = fs::remove_file(path);
        }
    }
}

fn devtools_root() -> PathBuf {
    std::env::var_os(ENV_ROOT)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("textual-rs-devtools"))
}

fn current_app_name() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.file_name().map(|s| s.to_string_lossy().to_string()))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "textual-rs-app".to_string())
}

fn unix_ts_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn write_instance_file(path: &Path, pid: u32, app: &str, addr: &str) -> io::Result<()> {
    let body = format!(
        "pid={pid}\napp={app}\naddr={addr}\nstarted_unix={}\nprotocol={PROTOCOL_VERSION}\n",
        unix_ts_secs()
    );
    fs::write(path, body)
}

fn server_loop(
    listener: TcpListener,
    shared: Arc<SharedState>,
    running: Arc<AtomicBool>,
    pid: u32,
    app_name: String,
    addr: String,
) {
    while running.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let shared = Arc::clone(&shared);
                let app_name = app_name.clone();
                let addr = addr.clone();
                let _ = thread::Builder::new()
                    .name("textual-devtools-client".to_string())
                    .spawn(move || {
                        let _ = handle_client(stream, &shared, pid, &app_name, &addr);
                    });
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(40));
            }
            Err(_) => {
                thread::sleep(Duration::from_millis(80));
            }
        }
    }
}

fn handle_client(
    mut stream: TcpStream,
    shared: &Arc<SharedState>,
    pid: u32,
    app_name: &str,
    addr: &str,
) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;

    let mut line = String::new();
    {
        let mut reader = BufReader::new(stream.try_clone()?);
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
    }

    match parse_command(&line) {
        Ok(Request::Ping) => write_ok_line(&mut stream, "PONG"),
        Ok(Request::Info) => {
            let info = format!(
                "pid={pid}\napp={app_name}\naddr={addr}\nstarted_unix={}\nprotocol={PROTOCOL_VERSION}\n",
                unix_ts_secs()
            );
            write_data(&mut stream, info.as_bytes())
        }
        Ok(Request::Snapshot) => {
            let payload = if let Ok(snapshot) = shared.snapshot.lock() {
                snapshot.clone()
            } else {
                String::new()
            };
            write_data(&mut stream, payload.as_bytes())
        }
        Ok(Request::Watch) => stream_watch(&mut stream, shared),
        Ok(Request::Logs) => stream_logs(&mut stream),
        Ok(Request::Channels) => write_data(&mut stream, channels_payload().as_bytes()),
        Ok(Request::DebugChannel(name, enabled)) => match LogChannel::from_name(&name) {
            Some(channel) => {
                debug::set_channel_streaming(channel, enabled);
                let state = if enabled { "on" } else { "off" };
                write_ok_line(&mut stream, &format!("channel {} {state}", channel.name()))
            }
            None => write_err_line(&mut stream, &format!("unknown channel: {name}")),
        },
        Ok(Request::Focus(id)) => {
            if let Ok(mut pending) = shared.pending.lock() {
                pending.push(DevtoolsCommand::Focus(node_id_from_ffi(id)));
            }
            write_ok_line(&mut stream, "queued")
        }
        Ok(Request::DebugLayout(enabled)) => {
            if let Ok(mut pending) = shared.pending.lock() {
                pending.push(DevtoolsCommand::SetDebugLayout(enabled));
            }
            write_ok_line(&mut stream, "queued")
        }
        Ok(Request::ToggleDisplay(id)) => {
            if let Ok(mut pending) = shared.pending.lock() {
                pending.push(DevtoolsCommand::ToggleDisplay(node_id_from_ffi(id)));
            }
            write_ok_line(&mut stream, "queued")
        }
        Ok(Request::Highlight(id)) => {
            if let Ok(mut pending) = shared.pending.lock() {
                pending.push(DevtoolsCommand::Highlight(node_id_from_ffi(id)));
            }
            write_ok_line(&mut stream, "queued")
        }
        Ok(Request::AddClass(id, class)) => {
            if let Ok(mut pending) = shared.pending.lock() {
                pending.push(DevtoolsCommand::AddClass(node_id_from_ffi(id), class));
            }
            write_ok_line(&mut stream, "queued")
        }
        Ok(Request::RemoveClass(id, class)) => {
            if let Ok(mut pending) = shared.pending.lock() {
                pending.push(DevtoolsCommand::RemoveClass(node_id_from_ffi(id), class));
            }
            write_ok_line(&mut stream, "queued")
        }
        Ok(Request::Quit) => {
            if let Ok(mut pending) = shared.pending.lock() {
                pending.push(DevtoolsCommand::Quit);
            }
            write_ok_line(&mut stream, "queued")
        }
        Err(msg) => write_err_line(&mut stream, &msg),
    }
}

enum Request {
    Ping,
    Info,
    Snapshot,
    Watch,
    Logs,
    Channels,
    DebugChannel(String, bool),
    Focus(u64),
    DebugLayout(bool),
    ToggleDisplay(u64),
    Highlight(u64),
    AddClass(u64, String),
    RemoveClass(u64, String),
    Quit,
}

fn parse_command(raw: &str) -> Result<Request, String> {
    let line = raw.trim();
    if line.eq_ignore_ascii_case("PING") {
        return Ok(Request::Ping);
    }
    if line.eq_ignore_ascii_case("INFO") {
        return Ok(Request::Info);
    }
    if line.eq_ignore_ascii_case("SNAPSHOT") {
        return Ok(Request::Snapshot);
    }
    if line.eq_ignore_ascii_case("WATCH") {
        return Ok(Request::Watch);
    }
    if line.eq_ignore_ascii_case("LOGS") {
        return Ok(Request::Logs);
    }
    if line.eq_ignore_ascii_case("CHANNELS") {
        return Ok(Request::Channels);
    }
    if line.eq_ignore_ascii_case("QUIT") {
        return Ok(Request::Quit);
    }

    let mut parts = line.split_whitespace();
    let Some(head) = parts.next() else {
        return Err("empty request".to_string());
    };

    if head.eq_ignore_ascii_case("FOCUS") {
        let Some(id) = parts.next() else {
            return Err("FOCUS requires widget id".to_string());
        };
        let parsed = id
            .parse::<u64>()
            .map_err(|_| "FOCUS id must be an unsigned integer".to_string())?;
        return Ok(Request::Focus(parsed));
    }

    if head.eq_ignore_ascii_case("DEBUG_CHANNEL") {
        let Some(name) = parts.next() else {
            return Err("DEBUG_CHANNEL requires a channel name and on/off".to_string());
        };
        let Some(value) = parts.next() else {
            return Err("DEBUG_CHANNEL requires on/off".to_string());
        };
        let enabled = match value.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            _ => return Err("DEBUG_CHANNEL expects on/off".to_string()),
        };
        return Ok(Request::DebugChannel(name.to_string(), enabled));
    }

    if head.eq_ignore_ascii_case("DEBUG_LAYOUT") {
        let Some(value) = parts.next() else {
            return Err("DEBUG_LAYOUT requires on/off".to_string());
        };
        let enabled = match value.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            _ => return Err("DEBUG_LAYOUT expects on/off".to_string()),
        };
        return Ok(Request::DebugLayout(enabled));
    }

    if head.eq_ignore_ascii_case("TOGGLE_DISPLAY") {
        let Some(id) = parts.next() else {
            return Err("TOGGLE_DISPLAY requires widget id".to_string());
        };
        let parsed = id
            .parse::<u64>()
            .map_err(|_| "TOGGLE_DISPLAY id must be an unsigned integer".to_string())?;
        return Ok(Request::ToggleDisplay(parsed));
    }

    if head.eq_ignore_ascii_case("HIGHLIGHT") {
        let Some(id) = parts.next() else {
            return Err("HIGHLIGHT requires widget id".to_string());
        };
        let parsed = id
            .parse::<u64>()
            .map_err(|_| "HIGHLIGHT id must be an unsigned integer".to_string())?;
        return Ok(Request::Highlight(parsed));
    }

    if head.eq_ignore_ascii_case("ADD_CLASS") {
        let Some(id) = parts.next() else {
            return Err("ADD_CLASS requires widget id and class name".to_string());
        };
        let parsed = id
            .parse::<u64>()
            .map_err(|_| "ADD_CLASS id must be an unsigned integer".to_string())?;
        let Some(class) = parts.next() else {
            return Err("ADD_CLASS requires a class name".to_string());
        };
        return Ok(Request::AddClass(parsed, class.to_string()));
    }

    if head.eq_ignore_ascii_case("REMOVE_CLASS") {
        let Some(id) = parts.next() else {
            return Err("REMOVE_CLASS requires widget id and class name".to_string());
        };
        let parsed = id
            .parse::<u64>()
            .map_err(|_| "REMOVE_CLASS id must be an unsigned integer".to_string())?;
        let Some(class) = parts.next() else {
            return Err("REMOVE_CLASS requires a class name".to_string());
        };
        return Ok(Request::RemoveClass(parsed, class.to_string()));
    }

    Err(format!("unknown request: {head}"))
}

fn write_ok_line(stream: &mut TcpStream, detail: &str) -> io::Result<()> {
    stream.write_all(format!("OK {detail}\n").as_bytes())
}

fn write_err_line(stream: &mut TcpStream, detail: &str) -> io::Result<()> {
    stream.write_all(format!("ERR {detail}\n").as_bytes())
}

fn write_data(stream: &mut TcpStream, payload: &[u8]) -> io::Result<()> {
    stream.write_all(format!("DATA {}\n", payload.len()).as_bytes())?;
    stream.write_all(payload)
}

fn stream_watch(stream: &mut TcpStream, shared: &Arc<SharedState>) -> io::Result<()> {
    let (tx, rx) = mpsc::channel::<String>();
    if let Ok(mut watchers) = shared.watchers.lock() {
        watchers.push(tx);
    }
    let initial = if let Ok(snapshot) = shared.snapshot.lock() {
        snapshot.clone()
    } else {
        String::new()
    };
    write_data(stream, initial.as_bytes())?;
    for payload in rx {
        if write_data(stream, payload.as_bytes()).is_err() {
            break;
        }
    }
    Ok(())
}

/// Stream log records to the client: one DATA frame replaying the bounded
/// backlog, then one DATA frame per record until the client disconnects.
/// Records are `unix_millis<TAB>channel<TAB>message` lines (newline
/// terminated inside the frame payload).
fn stream_logs(stream: &mut TcpStream) -> io::Result<()> {
    let (backlog, rx) = debug::subscribe_log_stream();
    let mut initial = String::new();
    for record in backlog {
        initial.push_str(&record);
        initial.push('\n');
    }
    write_data(stream, initial.as_bytes())?;
    for record in rx {
        let mut payload = record;
        payload.push('\n');
        if write_data(stream, payload.as_bytes()).is_err() {
            break;
        }
    }
    Ok(())
}

/// `CHANNELS` payload: a `protocol` header plus one line per debug channel
/// with its stream state and env-configured log file (or `-`).
fn channels_payload() -> String {
    let mut out = format!("protocol\t{PROTOCOL_VERSION}\n");
    for (name, file, streaming) in debug::channel_states() {
        out.push_str(&format!(
            "channel\t{name}\t{}\t{}\n",
            u8::from(streaming),
            file.unwrap_or("-")
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_command_accepts_focus() {
        let request = parse_command("FOCUS 42").expect("request should parse");
        match request {
            Request::Focus(id) => assert_eq!(id, 42),
            _ => panic!("expected focus request"),
        }
    }

    #[test]
    fn parse_command_accepts_debug_layout() {
        let request = parse_command("DEBUG_LAYOUT on").expect("request should parse");
        match request {
            Request::DebugLayout(enabled) => assert!(enabled),
            _ => panic!("expected debug-layout request"),
        }
    }

    #[test]
    fn parse_command_rejects_bad_focus() {
        assert!(parse_command("FOCUS nope").is_err());
    }

    #[test]
    fn parse_command_accepts_watch() {
        let request = parse_command("WATCH").expect("watch should parse");
        assert!(matches!(request, Request::Watch));
    }

    #[test]
    fn parse_command_accepts_toggle_display() {
        let request = parse_command("TOGGLE_DISPLAY 99").expect("should parse");
        assert!(matches!(request, Request::ToggleDisplay(99)));
    }

    #[test]
    fn parse_command_accepts_highlight() {
        let request = parse_command("HIGHLIGHT 7").expect("should parse");
        assert!(matches!(request, Request::Highlight(7)));
    }

    #[test]
    fn parse_command_accepts_add_class() {
        let request = parse_command("ADD_CLASS 42 highlight").expect("should parse");
        match request {
            Request::AddClass(id, class) => {
                assert_eq!(id, 42);
                assert_eq!(class, "highlight");
            }
            _ => panic!("expected AddClass"),
        }
    }

    #[test]
    fn parse_command_accepts_remove_class() {
        let request = parse_command("REMOVE_CLASS 42 highlight").expect("should parse");
        match request {
            Request::RemoveClass(id, class) => {
                assert_eq!(id, 42);
                assert_eq!(class, "highlight");
            }
            _ => panic!("expected RemoveClass"),
        }
    }

    #[test]
    fn parse_command_rejects_add_class_missing_class() {
        assert!(parse_command("ADD_CLASS 42").is_err());
    }

    #[test]
    fn parse_command_accepts_logs_and_channels() {
        assert!(matches!(
            parse_command("LOGS").expect("logs should parse"),
            Request::Logs
        ));
        assert!(matches!(
            parse_command("CHANNELS").expect("channels should parse"),
            Request::Channels
        ));
    }

    #[test]
    fn parse_command_accepts_debug_channel_toggle() {
        match parse_command("DEBUG_CHANNEL style on").expect("should parse") {
            Request::DebugChannel(name, enabled) => {
                assert_eq!(name, "style");
                assert!(enabled);
            }
            _ => panic!("expected DebugChannel"),
        }
        match parse_command("DEBUG_CHANNEL layout off").expect("should parse") {
            Request::DebugChannel(name, enabled) => {
                assert_eq!(name, "layout");
                assert!(!enabled);
            }
            _ => panic!("expected DebugChannel"),
        }
        assert!(parse_command("DEBUG_CHANNEL style maybe").is_err());
        assert!(parse_command("DEBUG_CHANNEL style").is_err());
        assert!(parse_command("DEBUG_CHANNEL").is_err());
    }

    #[test]
    fn channels_payload_lists_every_channel_with_protocol_header() {
        let payload = channels_payload();
        let mut lines = payload.lines();
        assert_eq!(
            lines.next(),
            Some(format!("protocol\t{PROTOCOL_VERSION}").as_str())
        );
        for channel in LogChannel::ALL {
            let line = lines
                .next()
                .unwrap_or_else(|| panic!("missing channel line for {}", channel.name()));
            let mut fields = line.split('\t');
            assert_eq!(fields.next(), Some("channel"));
            assert_eq!(fields.next(), Some(channel.name()));
            assert!(matches!(fields.next(), Some("0") | Some("1")));
            assert!(fields.next().is_some(), "file field present");
        }
        assert_eq!(lines.next(), None);
    }

    // ---- end-to-end socket tests (against a real ephemeral server) ----

    use std::io::Read;

    fn connect(addr: &str) -> TcpStream {
        let stream = TcpStream::connect(addr).expect("connect to devtools server");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .expect("set read timeout");
        stream
    }

    /// Read one `DATA <len>\n<len bytes>` frame.
    fn read_frame(reader: &mut BufReader<TcpStream>) -> String {
        let mut header = String::new();
        reader.read_line(&mut header).expect("read frame header");
        let len: usize = header
            .trim()
            .strip_prefix("DATA ")
            .unwrap_or_else(|| panic!("expected DATA header, got {header:?}"))
            .parse()
            .expect("frame length");
        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf).expect("read frame payload");
        String::from_utf8_lossy(&buf).into_owned()
    }

    #[test]
    fn socket_info_advertises_protocol_revision() {
        let (_runtime, addr) = DevtoolsRuntime::bind_for_test().expect("bind test server");
        let mut stream = connect(&addr);
        stream.write_all(b"INFO\n").expect("send INFO");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
        let info = read_frame(&mut reader);
        assert!(
            info.contains(&format!("protocol={PROTOCOL_VERSION}\n")),
            "INFO must advertise the protocol revision, got: {info}"
        );
    }

    #[test]
    fn socket_channels_and_debug_channel_toggle_roundtrip() {
        let (_runtime, addr) = DevtoolsRuntime::bind_for_test().expect("bind test server");

        // Toggle a channel on over the socket.
        let mut stream = connect(&addr);
        stream
            .write_all(b"DEBUG_CHANNEL border on\n")
            .expect("send toggle");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
        let mut reply = String::new();
        reader.read_line(&mut reply).expect("read toggle reply");
        assert_eq!(reply.trim(), "OK channel border on");

        // Introspect: the CHANNELS listing reflects the toggle.
        let mut stream = connect(&addr);
        stream.write_all(b"CHANNELS\n").expect("send CHANNELS");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
        let listing = read_frame(&mut reader);
        assert!(
            listing
                .lines()
                .any(|line| line.starts_with("channel\tborder\t1\t")),
            "border channel should list as streaming, got: {listing}"
        );

        // Unknown channel names are a clean error.
        let mut stream = connect(&addr);
        stream
            .write_all(b"DEBUG_CHANNEL nonsense on\n")
            .expect("send bad toggle");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
        let mut reply = String::new();
        reader.read_line(&mut reply).expect("read error reply");
        assert!(reply.starts_with("ERR unknown channel"), "got: {reply}");

        // Restore the default to keep global state quiet for other tests.
        let mut stream = connect(&addr);
        stream
            .write_all(b"DEBUG_CHANNEL border off\n")
            .expect("send toggle off");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
        let mut reply = String::new();
        reader.read_line(&mut reply).expect("read toggle reply");
        assert_eq!(reply.trim(), "OK channel border off");
    }

    #[test]
    fn socket_logs_streams_app_records() {
        let (_runtime, addr) = DevtoolsRuntime::bind_for_test().expect("bind test server");

        let mut stream = connect(&addr);
        stream.write_all(b"LOGS\n").expect("send LOGS");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
        // Initial frame replays the (possibly empty) backlog; the subscription
        // is registered before it is sent, so records emitted after this point
        // arrive as follow-up frames.
        let _backlog = read_frame(&mut reader);

        let marker = format!("devtools-log-marker-{}", std::process::id());
        crate::debug::log(&marker);

        // Concurrent lib tests may interleave their own records; scan a
        // bounded number of frames for ours.
        let mut found = None;
        for _ in 0..50 {
            let frame = read_frame(&mut reader);
            if let Some(record) = frame.lines().find(|line| line.ends_with(marker.as_str())) {
                found = Some(record.to_string());
                break;
            }
        }
        let record = found.expect("log record should stream over the socket");
        let mut fields = record.split('\t');
        let ts: u64 = fields
            .next()
            .expect("timestamp field")
            .parse()
            .expect("numeric unix-millis timestamp");
        assert!(ts > 0);
        assert_eq!(fields.next(), Some("app"));
        assert_eq!(fields.next(), Some(marker.as_str()));
        assert_eq!(fields.next(), None);
    }
}
