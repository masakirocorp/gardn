//! Restricted worker-local ingress for managed agent lifecycle hooks.

#[cfg(unix)]
use std::collections::{HashMap, VecDeque};
#[cfg(unix)]
use std::io::{self, Read, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
#[cfg(unix)]
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(unix)]
use std::sync::{Arc, Mutex};
#[cfg(unix)]
use std::time::Duration;

#[cfg(unix)]
use crate::api::schema::{Method, Request};
#[cfg(unix)]
use crate::execution_host::protocol::RuntimeIdentity;
#[cfg(unix)]
use crate::integration::host::WorkerHookReport;

#[cfg(unix)]
const TOKEN_BYTES: usize = 32;
#[cfg(unix)]
const MAX_REQUEST_BYTES: usize = 64 * 1024;
#[cfg(unix)]
const MAX_QUEUED_REPORTS: usize = 512;
#[cfg(unix)]
const MAX_ACTIVE_CONNECTIONS: usize = 8;
#[cfg(unix)]
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(20);

#[cfg(unix)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct QueuedHookReport {
    pub(super) identity: RuntimeIdentity,
    pub(super) report: WorkerHookReport,
}

#[cfg(unix)]
struct SharedHookState {
    accepting: Mutex<bool>,
    tokens: Mutex<HashMap<String, RuntimeIdentity>>,
    reports: Mutex<VecDeque<QueuedHookReport>>,
    #[cfg(test)]
    handler_started: AtomicBool,
    #[cfg(test)]
    before_commit: Mutex<Option<(std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>)>>,
}

#[cfg(unix)]
impl Default for SharedHookState {
    fn default() -> Self {
        Self {
            accepting: Mutex::new(true),
            tokens: Mutex::new(HashMap::new()),
            reports: Mutex::new(VecDeque::new()),
            #[cfg(test)]
            handler_started: AtomicBool::new(false),
            #[cfg(test)]
            before_commit: Mutex::new(None),
        }
    }
}

#[cfg(unix)]
pub(super) struct WorkerHookIngress {
    path: PathBuf,
    shared: Arc<SharedHookState>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[cfg(unix)]
impl WorkerHookIngress {
    pub(super) fn start(path: PathBuf) -> io::Result<Self> {
        remove_socket_if_present(&path)?;
        let listener = UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;

        let shared = Arc::new(SharedHookState::default());
        let stop = Arc::new(AtomicBool::new(false));
        let listener_shared = Arc::clone(&shared);
        let listener_stop = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            let mut handlers = Vec::<std::thread::JoinHandle<()>>::new();
            while !listener_stop.load(Ordering::Relaxed) {
                let mut live_handlers = Vec::with_capacity(handlers.len());
                for handler in handlers.drain(..) {
                    if handler.is_finished() {
                        let _ = handler.join();
                    } else {
                        live_handlers.push(handler);
                    }
                }
                handlers = live_handlers;

                match listener.accept() {
                    Ok((mut stream, _)) if handlers.len() < MAX_ACTIVE_CONNECTIONS => {
                        let connection_shared = Arc::clone(&listener_shared);
                        let connection_stop = Arc::clone(&listener_stop);
                        handlers.push(std::thread::spawn(move || {
                            #[cfg(test)]
                            connection_shared
                                .handler_started
                                .store(true, Ordering::Release);
                            let _ =
                                handle_stream(&mut stream, &connection_shared, &connection_stop);
                        }));
                    }
                    Ok((_stream, _)) => {}
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        std::thread::sleep(ACCEPT_POLL_INTERVAL);
                    }
                    Err(_) => break,
                }
            }
            for handler in handlers {
                let _ = handler.join();
            }
        });

        Ok(Self {
            path,
            shared,
            stop,
            thread: Some(thread),
        })
    }

    pub(super) fn socket_path(&self) -> &Path {
        &self.path
    }

    pub(super) fn register(&self, identity: RuntimeIdentity) -> io::Result<String> {
        let mut random = [0_u8; TOKEN_BYTES];
        std::fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
        let mut token = String::with_capacity(TOKEN_BYTES * 2);
        for byte in random {
            use std::fmt::Write as _;
            let _ = write!(token, "{byte:02x}");
        }
        random.fill(0);
        self.shared
            .tokens
            .lock()
            .map_err(|_| io::Error::other("worker hook token registry is poisoned"))?
            .insert(token.clone(), identity);
        Ok(token)
    }

    pub(super) fn unregister(&self, identity: &RuntimeIdentity) {
        if let Ok(mut tokens) = self.shared.tokens.lock() {
            tokens.retain(|_, registered| registered != identity);
        }
    }

    pub(super) fn next_report(&self) -> Option<QueuedHookReport> {
        self.shared.reports.lock().ok()?.front().cloned()
    }

    pub(super) fn confirm_report(&self, delivered: &QueuedHookReport) {
        if let Ok(mut reports) = self.shared.reports.lock() {
            if reports.front().is_some_and(|queued| queued == delivered) {
                reports.pop_front();
            }
        }
    }
}

#[cfg(unix)]
impl Drop for WorkerHookIngress {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Ok(mut accepting) = self.shared.accepting.lock() {
            *accepting = false;
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = remove_socket_if_present(&self.path);
    }
}

#[cfg(unix)]
fn handle_stream(
    stream: &mut UnixStream,
    shared: &SharedHookState,
    stop: &AtomicBool,
) -> io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    let request_bytes = read_request_line(stream)?;
    let request = serde_json::from_slice::<Request>(&request_bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let request_id = request.id.clone();

    let (token, report) = match request.method {
        Method::PaneReportAgent(mut params) => {
            let token = std::mem::take(&mut params.pane_id);
            (token, WorkerHookReport::Agent(params.into()))
        }
        Method::PaneReportAgentSession(mut params) => {
            let token = std::mem::take(&mut params.pane_id);
            (token, WorkerHookReport::Session(params.into()))
        }
        Method::PaneReleaseAgent(mut params) => {
            let token = std::mem::take(&mut params.pane_id);
            (token, WorkerHookReport::Release(params.into()))
        }
        _ => {
            write_response(stream, &request_id, false)?;
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "worker hook ingress accepts lifecycle reports only",
            ));
        }
    };

    let identity = shared
        .tokens
        .lock()
        .map_err(|_| io::Error::other("worker hook token registry is poisoned"))?
        .get(&token)
        .cloned();
    let Some(identity) = identity else {
        write_response(stream, &request_id, false)?;
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "worker hook token is invalid",
        ));
    };

    #[cfg(test)]
    if let Some((reached, release)) = shared
        .before_commit
        .lock()
        .map_err(|_| io::Error::other("worker hook test barrier is poisoned"))?
        .take()
    {
        let _ = reached.send(());
        let _ = release.recv();
    }

    let accepting = shared
        .accepting
        .lock()
        .map_err(|_| io::Error::other("worker hook ingress state is poisoned"))?;
    if !*accepting || stop.load(Ordering::Acquire) {
        write_response(stream, &request_id, false)?;
        return Err(io::Error::new(
            io::ErrorKind::ConnectionAborted,
            "worker hook ingress is shutting down",
        ));
    }

    let mut reports = shared
        .reports
        .lock()
        .map_err(|_| io::Error::other("worker hook report queue is poisoned"))?;
    if reports.len() >= MAX_QUEUED_REPORTS {
        if let Some(index) = reports
            .iter()
            .position(|queued| queued.identity == identity)
        {
            reports.remove(index);
        } else {
            reports.pop_front();
        }
    }
    reports.push_back(QueuedHookReport { identity, report });
    drop(reports);
    let response = write_response(stream, &request_id, true);
    drop(accepting);
    response
}

#[cfg(unix)]
fn read_request_line(stream: &mut UnixStream) -> io::Result<Vec<u8>> {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let count = stream.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..count]);
        if request.len() > MAX_REQUEST_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "worker hook request exceeds the size limit",
            ));
        }
        if let Some(newline) = request.iter().position(|byte| *byte == b'\n') {
            request.truncate(newline);
            break;
        }
    }
    if request.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "worker hook request is empty",
        ));
    }
    Ok(request)
}

#[cfg(unix)]
fn write_response(stream: &mut UnixStream, id: &str, accepted: bool) -> io::Result<()> {
    let response = if accepted {
        serde_json::json!({"id": id, "result": {}})
    } else {
        serde_json::json!({"id": id, "error": {"code": "unauthorized", "message": "request rejected"}})
    };
    serde_json::to_writer(&mut *stream, &response).map_err(io::Error::other)?;
    stream.write_all(b"\n")
}

#[cfg(unix)]
fn remove_socket_if_present(path: &Path) -> io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    #[test]
    fn drop_joins_stalled_connection_handlers() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = PathBuf::from("/tmp").join(format!("ohi-d-{}-{nonce}.sock", std::process::id()));
        let ingress = WorkerHookIngress::start(path.clone()).unwrap();
        let mut client = UnixStream::connect(&path).unwrap();
        client.write_all(br#"{"id":"stalled""#).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        while !ingress.shared.handler_started.load(Ordering::Acquire) {
            assert!(
                Instant::now() < deadline,
                "handler did not accept the client"
            );
            std::thread::sleep(Duration::from_millis(5));
        }

        let started = Instant::now();
        drop(ingress);
        assert!(
            started.elapsed() >= Duration::from_millis(50),
            "drop returned before the stalled handler exited"
        );

        let mut byte = [0_u8; 1];
        assert_eq!(client.read(&mut byte).unwrap(), 0);
    }

    #[test]
    fn shutdown_rejects_authenticated_report_before_commit() {
        use crate::execution_host::protocol::{
            HostBindingGeneration, RuntimeIncarnation, WorkerInstanceId, WorkerRuntimeId,
        };

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = PathBuf::from("/tmp").join(format!("ohi-s-{}-{nonce}.sock", std::process::id()));
        let ingress = WorkerHookIngress::start(path.clone()).unwrap();
        let identity = RuntimeIdentity::new(
            HostBindingGeneration::new(1),
            WorkerInstanceId::new("worker-a").unwrap(),
            WorkerRuntimeId::new("runtime-a").unwrap(),
            RuntimeIncarnation::new(1),
        );
        let token = ingress.register(identity).unwrap();
        let (reached_tx, reached_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        *ingress.shared.before_commit.lock().unwrap() = Some((reached_tx, release_rx));

        let mut client = UnixStream::connect(&path).unwrap();
        let request = serde_json::json!({
            "id": "shutdown-race",
            "method": "pane.report_agent",
            "params": {
                "pane_id": token,
                "source": "gardn:test",
                "agent": "codex",
                "state": "working",
                "seq": 1
            }
        });
        client.write_all(format!("{request}\n").as_bytes()).unwrap();
        reached_rx.recv_timeout(Duration::from_secs(1)).unwrap();

        let shared = Arc::clone(&ingress.shared);
        let stop = Arc::clone(&ingress.stop);
        let shutdown = std::thread::spawn(move || drop(ingress));
        let deadline = Instant::now() + Duration::from_secs(1);
        while !stop.load(Ordering::Acquire) {
            assert!(Instant::now() < deadline, "shutdown did not start");
            std::thread::sleep(Duration::from_millis(5));
        }
        release_tx.send(()).unwrap();

        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        shutdown.join().unwrap();
        assert!(response.contains("\"code\":\"unauthorized\""));
        assert!(shared.reports.lock().unwrap().is_empty());
    }
}
