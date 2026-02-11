use crate::widgets::WidgetId;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{
    mpsc::{self, Sender},
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const ENV_ENABLE: &str = "TEXTUAL_DEVTOOLS";
const ENV_BIND: &str = "TEXTUAL_DEVTOOLS_BIND";
const ENV_ROOT: &str = "TEXTUAL_DEVTOOLS_ROOT";
const DEFAULT_BIND: &str = "127.0.0.1:0";

#[derive(Debug, Clone)]
pub(crate) enum DevtoolsCommand {
    Focus(WidgetId),
    SetDebugLayout(bool),
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
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?.to_string();

        let pid = std::process::id();
        let app_name = current_app_name();
        let registry_root = devtools_root();
        fs::create_dir_all(&registry_root)?;
        let registry_file = registry_root.join(format!("{pid}.instance"));
        write_instance_file(&registry_file, pid, &app_name, &addr)?;

        let shared = Arc::new(SharedState::default());
        let shared_thread = Arc::clone(&shared);
        let running = Arc::new(AtomicBool::new(true));
        let running_thread = Arc::clone(&running);
        let thread = thread::Builder::new()
            .name("textual-devtools".to_string())
            .spawn(move || {
                server_loop(listener, shared_thread, running_thread, pid, app_name, addr);
            })?;

        Ok(Some(Self {
            shared,
            running,
            thread: Some(thread),
            registry_file: Some(registry_file),
        }))
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
        "pid={pid}\napp={app}\naddr={addr}\nstarted_unix={}\n",
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
                "pid={pid}\napp={app_name}\naddr={addr}\nstarted_unix={}\n",
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
        Ok(Request::Focus(id)) => {
            if let Ok(mut pending) = shared.pending.lock() {
                pending.push(DevtoolsCommand::Focus(WidgetId::from_u64(id)));
            }
            write_ok_line(&mut stream, "queued")
        }
        Ok(Request::DebugLayout(enabled)) => {
            if let Ok(mut pending) = shared.pending.lock() {
                pending.push(DevtoolsCommand::SetDebugLayout(enabled));
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
    Focus(u64),
    DebugLayout(bool),
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
}
