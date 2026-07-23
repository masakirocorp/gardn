use interprocess::local_socket::traits::{ListenerExt as _, Stream as _};
#[cfg(not(windows))]
use std::io::Read;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tracing::{debug, error, info, warn};

#[cfg(all(test, unix))]
use std::fs;

use crate::api::schema::{
    ErrorBody, ErrorResponse, Method, Request, ResponseResult, ServerCapabilities, SuccessResponse,
};
use crate::api::subscriptions::ActiveSubscription;
use crate::api::wait::{prompt_agent, wait_for_agent, wait_for_output};
use crate::api::{request_changes_ui, socket_path, ApiRequestMessage, ApiRequestSender, EventHub};
use crate::ipc::{
    bind_local_listener, poll_local_stream_read, remove_socket_file_if_owned,
    set_local_stream_polling, socket_file_identity, LocalStream, LocalStreamRead,
    SocketFileIdentity,
};

const SOCKET_PERMISSION_MODE: u32 = 0o600;
pub(super) const CONNECTION_POLL_INTERVAL: Duration = Duration::from_millis(100);
pub(super) const APP_RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);
const INITIAL_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const STREAM_WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_INITIAL_REQUEST_BYTES: usize = 1024 * 1024;

pub struct ServerHandle {
    _thread: std::thread::JoinHandle<()>,
    path: PathBuf,
    identity: SocketFileIdentity,
    running: Arc<AtomicBool>,
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);

        if let Err(err) = self.remove_socket_file_if_owned() {
            if err.kind() != std::io::ErrorKind::NotFound {
                warn!(path = %self.path.display(), err = %err, "failed to remove api socket on shutdown");
            }
        }
    }
}

impl ServerHandle {
    pub(crate) fn remove_socket_file_if_owned(&self) -> std::io::Result<()> {
        remove_socket_file_if_owned(&self.path, &self.identity)
    }
}

pub fn default_server_capabilities() -> Option<ServerCapabilities> {
    Some(ServerCapabilities {
        live_handoff: cfg!(unix),
    })
}

pub fn start_server(
    api_tx: ApiRequestSender,
    event_hub: EventHub,
) -> std::io::Result<ServerHandle> {
    start_server_with_capabilities(api_tx, event_hub, default_server_capabilities())
}

pub fn start_server_with_capabilities(
    api_tx: ApiRequestSender,
    event_hub: EventHub,
    capabilities: Option<ServerCapabilities>,
) -> std::io::Result<ServerHandle> {
    let path = socket_path();
    prepare_socket_path(&path)?;

    let listener = bind_local_listener(&path)?;
    restrict_socket_permissions(&path)?;
    let identity = socket_file_identity(&path)?;
    info!(path = %path.display(), "api server listening");

    let running = Arc::new(AtomicBool::new(true));
    let listener_running = Arc::clone(&running);
    let thread = std::thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let api_tx = api_tx.clone();
                    let event_hub = event_hub.clone();
                    let capabilities = capabilities.clone();
                    let connection_running = Arc::clone(&listener_running);
                    std::thread::spawn(move || {
                        if let Err(err) = handle_connection(
                            stream,
                            &api_tx,
                            &event_hub,
                            &connection_running,
                            capabilities,
                        ) {
                            warn!(err = %err, "api connection failed");
                        }
                    });
                }
                Err(err) => {
                    error!(err = %err, "api listener accept failed");
                    break;
                }
            }
        }
        debug!("api server thread exiting");
    });

    Ok(ServerHandle {
        _thread: thread,
        path,
        identity,
        running,
    })
}

fn prepare_socket_path(path: &Path) -> std::io::Result<()> {
    crate::ipc::prepare_socket_path(path, |path| {
        format!(
            "Oh My Herdr is already running (socket busy at {})",
            path.display()
        )
    })
}

fn restrict_socket_permissions(path: &Path) -> std::io::Result<()> {
    crate::ipc::restrict_socket_permissions(path, SOCKET_PERMISSION_MODE)
}

struct HandledResponse {
    body: String,
    response_written: Option<std::sync::mpsc::Sender<()>>,
}

fn handle_connection(
    mut stream: LocalStream,
    api_tx: &ApiRequestSender,
    event_hub: &EventHub,
    running: &Arc<AtomicBool>,
    capabilities: Option<ServerCapabilities>,
) -> std::io::Result<()> {
    if let Err(err) = stream.set_send_timeout(Some(STREAM_WRITE_TIMEOUT)) {
        debug!(err = %err, "api connection write timeout unavailable");
    }

    let Some(line) = read_initial_request_line(&mut stream)? else {
        return Ok(());
    };

    let line = line.trim();
    if line.is_empty() {
        return Ok(());
    }

    let request = match serde_json::from_str::<Request>(line) {
        Ok(request) => request,
        Err(err) => {
            write_json_line_allow_disconnect(
                &mut stream,
                &ErrorResponse {
                    id: String::new(),
                    error: ErrorBody {
                        code: "invalid_request".into(),
                        message: format!("invalid request: {err}"),
                    },
                },
            )?;
            return Ok(());
        }
    };

    let request_id = request.id.clone();
    let method = api_method_name(&request.method);
    let changes_ui = request_changes_ui(&request);
    crate::logging::api_request_started(&request_id, method, changes_ui);

    match request.method {
        Method::EventsSubscribe(params) => {
            let result = stream_subscriptions(
                stream,
                request_id.clone(),
                params,
                api_tx,
                event_hub,
                running,
            );
            match &result {
                Ok(()) => crate::logging::api_request_completed(
                    &request_id,
                    method,
                    "stream_closed",
                    changes_ui,
                ),
                Err(err) => {
                    crate::logging::api_request_failed(&request_id, method, &err.to_string())
                }
            }
            result
        }
        Method::AgentWait(params) => {
            let Some(response) = wait_for_agent(
                request_id.clone(),
                params,
                &mut stream,
                api_tx,
                event_hub,
                running,
            )?
            else {
                crate::logging::api_request_completed(
                    &request_id,
                    method,
                    "client_disconnected",
                    changes_ui,
                );
                return Ok(());
            };
            let result = write_text_line_allow_disconnect(&mut stream, &response);
            match &result {
                Ok(()) => crate::logging::api_request_completed(
                    &request_id,
                    method,
                    api_response_outcome(&response),
                    changes_ui,
                ),
                Err(err) => {
                    crate::logging::api_request_failed(&request_id, method, &err.to_string())
                }
            }
            result
        }
        Method::AgentPrompt(params) => {
            let Some(response) = prompt_agent(
                request_id.clone(),
                params,
                &mut stream,
                api_tx,
                event_hub,
                running,
            )?
            else {
                crate::logging::api_request_completed(
                    &request_id,
                    method,
                    "client_disconnected",
                    changes_ui,
                );
                return Ok(());
            };
            let result = write_text_line_allow_disconnect(&mut stream, &response);
            match &result {
                Ok(()) => crate::logging::api_request_completed(
                    &request_id,
                    method,
                    api_response_outcome(&response),
                    changes_ui,
                ),
                Err(err) => {
                    crate::logging::api_request_failed(&request_id, method, &err.to_string())
                }
            }
            result
        }
        Method::PaneWaitForOutput(params) => {
            let Some(response) =
                wait_for_output(request_id.clone(), params, &mut stream, api_tx, running)?
            else {
                crate::logging::api_request_completed(
                    &request_id,
                    method,
                    "client_disconnected",
                    changes_ui,
                );
                return Ok(());
            };
            let result = write_text_line_allow_disconnect(&mut stream, &response);
            match &result {
                Ok(()) => crate::logging::api_request_completed(
                    &request_id,
                    method,
                    api_response_outcome(&response),
                    changes_ui,
                ),
                Err(err) => {
                    crate::logging::api_request_failed(&request_id, method, &err.to_string())
                }
            }
            result
        }
        method_body => {
            let response = handle_request(
                Request {
                    id: request_id.clone(),
                    method: method_body,
                },
                api_tx,
                capabilities,
            );
            let result = write_text_line_allow_disconnect(&mut stream, &response.body);
            if let Some(response_written) = response.response_written {
                let _ = response_written.send(());
            }
            match &result {
                Ok(()) => crate::logging::api_request_completed(
                    &request_id,
                    method,
                    api_response_outcome(&response.body),
                    changes_ui,
                ),
                Err(err) => {
                    crate::logging::api_request_failed(&request_id, method, &err.to_string())
                }
            }
            result
        }
    }
}

fn handle_request(
    request: Request,
    api_tx: &ApiRequestSender,
    capabilities: Option<ServerCapabilities>,
) -> HandledResponse {
    match request.method {
        Method::Ping(_) => HandledResponse {
            body: serde_json::to_string(&SuccessResponse {
                id: request.id,
                result: ResponseResult::Pong {
                    version: crate::build_info::version(),
                    protocol: crate::protocol::PROTOCOL_VERSION,
                    capabilities,
                },
            })
            .unwrap_or_else(|_| {
                r#"{"id":"","error":{"code":"internal_error","message":"failed to encode response"}}"#
                    .to_string()
            }),
            response_written: None,
        },
        _ => dispatch_to_app(request, api_tx),
    }
}

fn api_method_name(method: &Method) -> &'static str {
    match method {
        Method::Ping(_) => "ping",
        Method::ServerStop(_) => "server.stop",
        Method::ServerLiveHandoff(_) => "server.live_handoff",
        Method::ServerReloadConfig(_) => "server.reload_config",
        Method::ServerAgentManifests(_) => "server.agent_manifests",
        Method::ServerReloadAgentManifests(_) => "server.reload_agent_manifests",
        Method::NotificationShow(_) => "notification.show",
        Method::ClientWindowTitleSet(_) => "client.window_title.set",
        Method::ClientWindowTitleClear(_) => "client.window_title.clear",
        Method::SessionSnapshot(_) => "session.snapshot",
        Method::WorkspaceCreate(_) => "workspace.create",
        Method::WorkspaceList(_) => "workspace.list",
        Method::WorkspaceGet(_) => "workspace.get",
        Method::WorkspaceFocus(_) => "workspace.focus",
        Method::WorkspaceRename(_) => "workspace.rename",
        Method::WorkspaceClose(_) => "workspace.close",
        Method::GroupCreate(_) => "group.create",
        Method::GroupList(_) => "group.list",
        Method::GroupFocus(_) => "group.focus",
        Method::GroupRename(_) => "group.rename",
        Method::GroupDelete(_) => "group.delete",
        Method::WorkspaceMoveToGroup(_) => "workspace.move_to_group",
        Method::TabCreate(_) => "tab.create",
        Method::TabList(_) => "tab.list",
        Method::TabGet(_) => "tab.get",
        Method::TabFocus(_) => "tab.focus",
        Method::TabRename(_) => "tab.rename",
        Method::TabClose(_) => "tab.close",
        Method::AgentList(_) => "agent.list",
        Method::AgentGet(_) => "agent.get",
        Method::AgentRead(_) => "agent.read",
        Method::AgentExplain(_) => "agent.explain",
        Method::AgentSendKeys(_) => "agent.send_keys",
        Method::AgentPrompt(_) => "agent.prompt",
        Method::AgentRename(_) => "agent.rename",
        Method::AgentViewSet(_) => "agent.view.set",
        Method::AgentViewClear(_) => "agent.view.clear",
        Method::AgentFocus(_) => "agent.focus",
        Method::AgentStart(_) => "agent.start",
        Method::AgentWait(_) => "agent.wait",
        Method::PaneFocus(_) => "pane.focus",
        Method::PaneSplit(_) => "pane.split",
        Method::PaneSwap(_) => "pane.swap",
        Method::PaneMove(_) => "pane.move",
        Method::PaneZoom(_) => "pane.zoom",
        Method::PaneLayout(_) => "pane.layout",
        Method::PaneProcessInfo(_) => "pane.process_info",
        Method::LayoutExport(_) => "layout.export",
        Method::LayoutApply(_) => "layout.apply",
        Method::PaneNeighbor(_) => "pane.neighbor",
        Method::PaneEdges(_) => "pane.edges",
        Method::PaneFocusDirection(_) => "pane.focus_direction",
        Method::PaneResize(_) => "pane.resize",
        Method::PaneList(_) => "pane.list",
        Method::PaneCurrent(_) => "pane.current",
        Method::PaneGet(_) => "pane.get",
        Method::PaneRename(_) => "pane.rename",
        Method::PaneSendText(_) => "pane.send_text",
        Method::PaneSendKeys(_) => "pane.send_keys",
        Method::PaneSendInput(_) => "pane.send_input",
        Method::PaneRead(_) => "pane.read",
        Method::PaneReportAgent(_) => "pane.report_agent",
        Method::PaneReportAgentSession(_) => "pane.report_agent_session",
        Method::PaneReportMetadata(_) => "pane.report_metadata",
        Method::PaneClearAgentAuthority(_) => "pane.clear_agent_authority",
        Method::PaneReleaseAgent(_) => "pane.release_agent",
        Method::PaneClose(_) => "pane.close",
        Method::EventsSubscribe(_) => "events.subscribe",
        Method::EventsWait(_) => "events.wait",
        Method::PaneWaitForOutput(_) => "pane.wait_for_output",
        Method::IntegrationInstall(_) => "integration.install",
        Method::IntegrationUninstall(_) => "integration.uninstall",
        Method::PluginLink(_) => "plugin.link",
        Method::PluginList(_) => "plugin.list",
        Method::PluginUnlink(_) => "plugin.unlink",
        Method::PluginEnable(_) => "plugin.enable",
        Method::PluginDisable(_) => "plugin.disable",
        Method::PluginActionList(_) => "plugin.action.list",
        Method::PluginActionInvoke(_) => "plugin.action.invoke",
        Method::PluginLogList(_) => "plugin.log.list",
        Method::PluginPaneOpen(_) => "plugin.pane.open",
        Method::PluginPaneFocus(_) => "plugin.pane.focus",
        Method::PluginPaneClose(_) => "plugin.pane.close",
    }
}

fn api_response_outcome(response: &str) -> &'static str {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(response) else {
        return "error";
    };

    match value
        .get("error")
        .and_then(|error| error.get("code"))
        .and_then(|code| code.as_str())
    {
        Some("timeout") => "timeout",
        Some(_) => "error",
        None => "ok",
    }
}

fn finish_timed_read<T>(
    result: std::io::Result<Option<T>>,
    reset: impl FnOnce() -> std::io::Result<()>,
) -> std::io::Result<Option<T>> {
    match result {
        // The peer owns this stream's lifetime. Once it has ended, restoring
        // socket options can race the OS closing the peer and report EINVAL.
        Ok(None) => Ok(None),
        Ok(value) => {
            reset()?;
            Ok(value)
        }
        Err(err) => {
            let _ = reset();
            Err(err)
        }
    }
}

fn read_initial_request_line(stream: &mut LocalStream) -> std::io::Result<Option<String>> {
    read_initial_request_line_with_timeout(stream, INITIAL_REQUEST_TIMEOUT)
}

fn read_initial_request_line_with_timeout(
    stream: &mut LocalStream,
    timeout: Duration,
) -> std::io::Result<Option<String>> {
    read_initial_request_line_with_limits(stream, timeout, MAX_INITIAL_REQUEST_BYTES)
}

fn read_initial_request_line_with_limits(
    stream: &mut LocalStream,
    timeout: Duration,
    max_bytes: usize,
) -> std::io::Result<Option<String>> {
    match set_local_stream_polling(stream, true) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::InvalidInput => return Ok(None),
        Err(err) => return Err(err),
    }
    let deadline = Instant::now() + timeout;
    let mut bytes = Vec::new();
    let mut byte = [0u8; 1];
    let result = loop {
        let read = match poll_local_stream_read(stream, &mut byte) {
            Ok(read) => read,
            Err(err) => break Err(err),
        };
        match read {
            LocalStreamRead::Closed => break Ok(None),
            LocalStreamRead::Data => {
                bytes.push(byte[0]);
                if byte[0] == b'\n' {
                    break String::from_utf8(bytes)
                        .map(Some)
                        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err));
                }
                if bytes.len() > max_bytes {
                    break Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "api request line is too large",
                    ));
                }
            }
            LocalStreamRead::Pending => {
                if Instant::now() >= deadline {
                    break Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "timed out reading api request",
                    ));
                }
                std::thread::sleep(CONNECTION_POLL_INTERVAL);
            }
        }
    };
    finish_timed_read(result, || set_local_stream_polling(stream, false))
}

fn stream_subscriptions(
    mut stream: LocalStream,
    request_id: String,
    params: crate::api::schema::EventsSubscribeParams,
    api_tx: &ApiRequestSender,
    event_hub: &EventHub,
    running: &Arc<AtomicBool>,
) -> std::io::Result<()> {
    let mut subscriptions = Vec::with_capacity(params.subscriptions.len());
    for (index, subscription) in params.subscriptions.into_iter().enumerate() {
        let active =
            match ActiveSubscription::new(subscription, &request_id, index, api_tx, event_hub) {
                Ok(active) => active,
                Err(mut response) => {
                    response.id = request_id.clone();
                    if let Err(err) = write_json_line(&mut stream, &response) {
                        if is_connection_closed_error(&err) {
                            return Ok(());
                        }
                        return Err(err);
                    }
                    return Ok(());
                }
            };
        subscriptions.push(active);
    }

    if let Err(err) = write_json_line(
        &mut stream,
        &SuccessResponse {
            id: request_id.clone(),
            result: ResponseResult::SubscriptionStarted {},
        },
    ) {
        if is_connection_closed_error(&err) {
            return Ok(());
        }
        return Err(err);
    }

    loop {
        if should_stop_connection(&mut stream, running)? {
            return Ok(());
        }
        for subscription in &mut subscriptions {
            match subscription.poll(api_tx, event_hub) {
                Ok(Some(event)) => {
                    if let Err(err) = write_json_line(&mut stream, &event) {
                        if is_connection_closed_error(&err) {
                            return Ok(());
                        }
                        return Err(err);
                    }
                }
                Ok(None) => {}
                Err(mut response) => {
                    response.id = request_id.clone();
                    if let Err(err) = write_json_line(&mut stream, &response) {
                        if is_connection_closed_error(&err) {
                            return Ok(());
                        }
                        return Err(err);
                    }
                    return Ok(());
                }
            }
        }
        std::thread::sleep(CONNECTION_POLL_INTERVAL);
    }
}

fn write_text_line(stream: &mut LocalStream, value: &str) -> std::io::Result<()> {
    stream.write_all(value.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()
}

fn write_text_line_allow_disconnect(stream: &mut LocalStream, value: &str) -> std::io::Result<()> {
    match write_text_line(stream, value) {
        Err(err) if is_connection_closed_error(&err) => Ok(()),
        result => result,
    }
}

fn write_json_line<T: serde::Serialize>(
    stream: &mut LocalStream,
    value: &T,
) -> std::io::Result<()> {
    let encoded = serde_json::to_string(value)
        .map_err(|err| std::io::Error::other(format!("failed to encode json: {err}")))?;
    write_text_line(stream, &encoded)
}

fn write_json_line_allow_disconnect<T: serde::Serialize>(
    stream: &mut LocalStream,
    value: &T,
) -> std::io::Result<()> {
    let encoded = serde_json::to_string(value)
        .map_err(|err| std::io::Error::other(format!("failed to encode json: {err}")))?;
    write_text_line_allow_disconnect(stream, &encoded)
}

pub(super) fn should_stop_connection(
    stream: &mut LocalStream,
    running: &Arc<AtomicBool>,
) -> std::io::Result<bool> {
    if !running.load(Ordering::Relaxed) {
        return Ok(true);
    }

    probe_stream_closed(stream)
}

#[cfg(windows)]
fn probe_stream_closed(_stream: &mut LocalStream) -> std::io::Result<bool> {
    // Windows named pipes do not give us the same nonblocking read semantics as
    // Unix sockets here: probing can report an empty read while the client is
    // still waiting for the long-poll response. Let writes detect disconnects
    // instead of closing wait requests early.
    Ok(false)
}

#[cfg(not(windows))]
fn probe_stream_closed(stream: &mut LocalStream) -> std::io::Result<bool> {
    match stream.set_nonblocking(true) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::InvalidInput => return Ok(true),
        Err(err) => return Err(err),
    }
    let mut probe = [0u8; 1];
    let result = match stream.read(&mut probe) {
        Ok(0) => Ok(None),
        Ok(_) => Ok(Some(true)),
        Err(err)
            if matches!(
                err.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
            ) =>
        {
            Ok(Some(false))
        }
        Err(err) if is_connection_closed_error(&err) => Ok(None),
        Err(err) => Err(err),
    };
    finish_timed_read(result, || stream.set_nonblocking(false))
        .map(|status| status.unwrap_or(true))
}

fn is_connection_closed_error(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::NotConnected
            | std::io::ErrorKind::UnexpectedEof
            | std::io::ErrorKind::WriteZero
    )
}

fn dispatch_to_app(request: Request, api_tx: &ApiRequestSender) -> HandledResponse {
    let (response_written, response_written_rx) = std::sync::mpsc::channel();
    HandledResponse {
        body: dispatch_to_app_inner(request, api_tx, None, Some(response_written_rx)),
        response_written: Some(response_written),
    }
}

pub(super) fn dispatch_to_app_with_timeout(
    request: Request,
    api_tx: &ApiRequestSender,
    timeout: Option<Duration>,
) -> String {
    dispatch_to_app_inner(request, api_tx, timeout, None)
}

fn dispatch_to_app_inner(
    request: Request,
    api_tx: &ApiRequestSender,
    timeout: Option<Duration>,
    response_written: Option<std::sync::mpsc::Receiver<()>>,
) -> String {
    let request_id = request.id.clone();
    let (respond_to, response_rx) = std::sync::mpsc::channel();
    if let Err(err) = api_tx.send(ApiRequestMessage {
        request,
        respond_to,
        response_written,
    }) {
        return error_response_json(
            request_id,
            "server_unavailable",
            format!("failed to dispatch request: {err}"),
        );
    }

    let response = match timeout {
        Some(timeout) => response_rx.recv_timeout(timeout).map_err(|err| match err {
            std::sync::mpsc::RecvTimeoutError::Timeout => std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!(
                    "timed out waiting for app response after {} ms",
                    timeout.as_millis()
                ),
            ),
            std::sync::mpsc::RecvTimeoutError::Disconnected => std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "app response channel closed",
            ),
        }),
        None => response_rx
            .recv()
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::BrokenPipe, err)),
    };

    match response {
        Ok(response) => response,
        Err(err) => error_response_json(
            request_id,
            "server_unavailable",
            format!("request handling failed: {err}"),
        ),
    }
}

fn error_response_json(id: String, code: &str, message: String) -> String {
    serde_json::to_string(&ErrorResponse {
        id,
        error: ErrorBody {
            code: code.into(),
            message,
        },
    })
    .unwrap_or_else(|_| {
        r#"{"id":"","error":{"code":"internal_error","message":"failed to encode error response"}}"#
            .to_string()
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::config::TestEnvVar;
    use interprocess::local_socket::traits::Listener as _;
    use std::io::{BufRead, BufReader};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;
    use std::sync::{Mutex, OnceLock};
    use tokio::sync::mpsc;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn unique_test_path(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("omh-{name}-{}-{nanos}", std::process::id()))
    }

    fn read_line(stream: &mut LocalStream) -> String {
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        line
    }

    fn local_stream_pair(name: &str) -> (LocalStream, LocalStream, PathBuf) {
        let path = unique_test_path(name);
        let listener = crate::ipc::bind_local_listener(&path).unwrap();
        let client = crate::ipc::connect_local_stream(&path).unwrap();
        let server = listener.accept().unwrap();
        (client, server, path)
    }
    fn agent_info(
        terminal_id: &str,
        pane_id: &str,
        status: crate::api::schema::AgentStatus,
        agent: Option<&str>,
    ) -> crate::api::schema::AgentInfo {
        crate::api::schema::AgentInfo {
            terminal_id: terminal_id.into(),
            name: None,
            agent: agent.map(str::to_string),
            title: None,
            display_agent: agent.map(str::to_string),
            agent_status: status,
            screen_detection_skipped: false,
            custom_status: None,
            tokens: std::collections::HashMap::new(),
            state_labels: std::collections::HashMap::new(),
            agent_session: None,
            workspace_id: "workspace_1".into(),
            tab_id: "tab_1".into(),
            pane_id: pane_id.into(),
            focused: true,
            cwd: None,
            foreground_cwd: None,
            revision: 0,
        }
    }

    fn agent_get_response(
        msg: &ApiRequestMessage,
        agent: crate::api::schema::AgentInfo,
    ) -> String {
        serde_json::to_string(&SuccessResponse {
            id: msg.request.id.clone(),
            result: ResponseResult::AgentInfo { agent },
        })
        .unwrap()
    }

    fn agent_prompt_response(
        msg: &ApiRequestMessage,
        agent: crate::api::schema::AgentInfo,
    ) -> String {
        serde_json::to_string(&SuccessResponse {
            id: msg.request.id.clone(),
            result: ResponseResult::AgentPrompted { agent },
        })
        .unwrap()
    }

    fn pane_lifecycle_event(data: crate::api::schema::EventData) -> crate::api::schema::EventEnvelope {
        let event = match &data {
            crate::api::schema::EventData::PaneClosed { .. } => {
                crate::api::schema::EventKind::PaneClosed
            }
            crate::api::schema::EventData::PaneExited { .. } => {
                crate::api::schema::EventKind::PaneExited
            }
            crate::api::schema::EventData::PaneAgentDetected { .. } => {
                crate::api::schema::EventKind::PaneAgentDetected
            }
            crate::api::schema::EventData::PaneAgentStatusChanged { .. } => {
                crate::api::schema::EventKind::PaneAgentStatusChanged
            }
            _ => panic!("not a pane lifecycle event"),
        };
        crate::api::schema::EventEnvelope { event, data }
    }

    fn prompt_test_agent(
        status: crate::api::schema::AgentStatus,
        terminal_id: &str,
    ) -> crate::api::schema::AgentInfo {
        crate::api::schema::AgentInfo {
            terminal_id: terminal_id.into(),
            name: Some("main".into()),
            agent: Some("pi".into()),
            title: None,
            display_agent: Some("pi".into()),
            agent_status: status,
            screen_detection_skipped: false,
            custom_status: None,
            tokens: std::collections::HashMap::new(),
            state_labels: std::collections::HashMap::new(),
            agent_session: None,
            workspace_id: "ws_1".into(),
            tab_id: "tab_1".into(),
            pane_id: "pane_1".into(),
            focused: true,
            cwd: None,
            foreground_cwd: None,
            revision: 0,
        }
    }

    fn prompt_test_response(
        request_id: String,
        status: crate::api::schema::AgentStatus,
        terminal_id: &str,
        prompted: bool,
    ) -> String {
        let agent = prompt_test_agent(status, terminal_id);
        let result = if prompted {
            ResponseResult::AgentPrompted { agent }
        } else {
            ResponseResult::AgentInfo { agent }
        };
        serde_json::to_string(&SuccessResponse {
            id: request_id,
            result,
        })
        .unwrap()
    }
    fn prompt_test_status_event(
        status: crate::api::schema::AgentStatus,
    ) -> crate::api::schema::EventEnvelope {
        crate::api::schema::EventEnvelope {
            event: crate::api::schema::EventKind::PaneAgentStatusChanged,
            data: crate::api::schema::EventData::PaneAgentStatusChanged {
                pane_id: "pane_1".into(),
                workspace_id: "ws_1".into(),
                agent_status: status,
                agent: Some("pi".into()),
                title: None,
                display_agent: Some("pi".into()),
                custom_status: None,
                state_labels: std::collections::HashMap::new(),
                tokens: std::collections::HashMap::new(),
            },
        }
    }

    #[test]
    fn agent_prompt_wait_keeps_requested_terminal_wait_after_transition() {
        let (api_tx, mut api_rx) = mpsc::unbounded_channel::<ApiRequestMessage>();
        let current_status = Arc::new(Mutex::new(crate::api::schema::AgentStatus::Idle));
        let responder_status = Arc::clone(&current_status);
        let (prompt_tx, prompt_rx) = std::sync::mpsc::channel();
        let responder = std::thread::spawn(move || {
            while let Some(msg) = api_rx.blocking_recv() {
                let status = *responder_status
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let response = match msg.request.method {
                    Method::AgentGet(_) => {
                        prompt_test_response(msg.request.id, status, "terminal_1", false)
                    }
                    Method::AgentPrompt(_) => {
                        prompt_tx.send(()).unwrap();
                        prompt_test_response(msg.request.id, status, "terminal_1", true)
                    }
                    method => panic!("unexpected method: {method:?}"),
                };
                msg.respond_to.send(response).unwrap();
            }
        });

        let (mut client, server, path) = local_stream_pair("agpt-transition");
        client
            .write_all(
                br#"{"id":"prompt_transition","method":"agent.prompt","params":{"target":"main","text":"continue","wait":{"until":["idle"],"timeout_ms":1500}}}"#,
            )
            .unwrap();
        client.write_all(b"\n").unwrap();
        client.flush().unwrap();

        let running = Arc::new(AtomicBool::new(true));
        let server_running = Arc::clone(&running);
        let event_hub = EventHub::default();
        let server_event_hub = event_hub.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let server_thread = std::thread::spawn(move || {
            let result = handle_connection(
                server,
                &api_tx,
                &server_event_hub,
                &server_running,
                None,
            );
            done_tx.send(result).unwrap();
        });

        let started = Instant::now();
        prompt_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        *current_status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) =
            crate::api::schema::AgentStatus::Working;
        event_hub.push(prompt_test_status_event(
            crate::api::schema::AgentStatus::Working,
        ));
        std::thread::sleep(Duration::from_millis(250));
        *current_status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) =
            crate::api::schema::AgentStatus::Idle;
        event_hub.push(prompt_test_status_event(
            crate::api::schema::AgentStatus::Idle,
        ));

        let response: serde_json::Value = serde_json::from_str(&read_line(&mut client)).unwrap();
        let elapsed = started.elapsed();
        assert_eq!(response["id"], "prompt_transition");
        assert_eq!(response["result"]["type"], "agent_prompted");
        assert!(
            elapsed >= Duration::from_millis(150),
            "terminal wait returned before requested transition: {elapsed:?}"
        );

        assert!(done_rx.recv_timeout(Duration::from_secs(2)).unwrap().is_ok());
        server_thread.join().unwrap();
        responder.join().unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn agent_prompt_wait_reports_closed_target() {
        let (api_tx, mut api_rx) = mpsc::unbounded_channel::<ApiRequestMessage>();
        let (prompt_tx, prompt_rx) = std::sync::mpsc::channel();
        let responder = std::thread::spawn(move || {
            while let Some(msg) = api_rx.blocking_recv() {
                let response = match msg.request.method {
                    Method::AgentGet(_) => prompt_test_response(
                        msg.request.id,
                        crate::api::schema::AgentStatus::Idle,
                        "terminal_1",
                        false,
                    ),
                    Method::AgentPrompt(_) => {
                        prompt_tx.send(()).unwrap();
                        prompt_test_response(
                            msg.request.id,
                            crate::api::schema::AgentStatus::Idle,
                            "terminal_1",
                            true,
                        )
                    }
                    method => panic!("unexpected method: {method:?}"),
                };
                msg.respond_to.send(response).unwrap();
            }
        });

        let (mut client, server, path) = local_stream_pair("agpt-close");
        client
            .write_all(
                br#"{"id":"prompt_close","method":"agent.prompt","params":{"target":"main","text":"close","wait":{"until":["idle"],"timeout_ms":6000}}}"#,
            )
            .unwrap();
        client.write_all(b"\n").unwrap();
        client.flush().unwrap();

        let running = Arc::new(AtomicBool::new(true));
        let server_running = Arc::clone(&running);
        let event_hub = EventHub::default();
        let server_event_hub = event_hub.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let server_thread = std::thread::spawn(move || {
            let result = handle_connection(
                server,
                &api_tx,
                &server_event_hub,
                &server_running,
                None,
            );
            done_tx.send(result).unwrap();
        });

        prompt_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        event_hub.push(crate::api::schema::EventEnvelope {
            event: crate::api::schema::EventKind::PaneClosed,
            data: crate::api::schema::EventData::PaneClosed {
                pane_id: "pane_1".into(),
                workspace_id: "ws_1".into(),
            },
        });

        let response: serde_json::Value = serde_json::from_str(&read_line(&mut client)).unwrap();
        assert_eq!(response["id"], "prompt_close");
        assert_eq!(response["error"]["code"], "agent_not_running");

        assert!(done_rx.recv_timeout(Duration::from_secs(2)).unwrap().is_ok());
        server_thread.join().unwrap();
        responder.join().unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn agent_prompt_wait_stops_when_client_disconnects() {
        let (api_tx, mut api_rx) = mpsc::unbounded_channel::<ApiRequestMessage>();
        let (prompt_tx, prompt_rx) = std::sync::mpsc::channel();
        let responder = std::thread::spawn(move || {
            while let Some(msg) = api_rx.blocking_recv() {
                let response = match msg.request.method {
                    Method::AgentGet(_) => prompt_test_response(
                        msg.request.id,
                        crate::api::schema::AgentStatus::Idle,
                        "terminal_1",
                        false,
                    ),
                    Method::AgentPrompt(_) => {
                        prompt_tx.send(()).unwrap();
                        prompt_test_response(
                            msg.request.id,
                            crate::api::schema::AgentStatus::Idle,
                            "terminal_1",
                            true,
                        )
                    }
                    method => panic!("unexpected method: {method:?}"),
                };
                msg.respond_to.send(response).unwrap();
            }
        });

        let (mut client, server, path) = local_stream_pair("agpt-disconnect");
        client
            .write_all(
                br#"{"id":"prompt_disconnect","method":"agent.prompt","params":{"target":"main","text":"disconnect","wait":{"until":["idle"],"timeout_ms":6000}}}"#,
            )
            .unwrap();
        client.write_all(b"\n").unwrap();
        client.flush().unwrap();

        let running = Arc::new(AtomicBool::new(true));
        let server_running = Arc::clone(&running);
        let event_hub = EventHub::default();
        let server_event_hub = event_hub.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let server_thread = std::thread::spawn(move || {
            let result = handle_connection(
                server,
                &api_tx,
                &server_event_hub,
                &server_running,
                None,
            );
            done_tx.send(result).unwrap();
        });

        prompt_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        drop(client);
        assert!(done_rx.recv_timeout(Duration::from_secs(2)).unwrap().is_ok());
        server_thread.join().unwrap();
        responder.join().unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn agent_prompt_wait_stops_when_server_shuts_down() {
        let (api_tx, mut api_rx) = mpsc::unbounded_channel::<ApiRequestMessage>();
        let (prompt_tx, prompt_rx) = std::sync::mpsc::channel();
        let responder = std::thread::spawn(move || {
            while let Some(msg) = api_rx.blocking_recv() {
                let response = match msg.request.method {
                    Method::AgentGet(_) => prompt_test_response(
                        msg.request.id,
                        crate::api::schema::AgentStatus::Idle,
                        "terminal_1",
                        false,
                    ),
                    Method::AgentPrompt(_) => {
                        prompt_tx.send(()).unwrap();
                        prompt_test_response(
                            msg.request.id,
                            crate::api::schema::AgentStatus::Idle,
                            "terminal_1",
                            true,
                        )
                    }
                    method => panic!("unexpected method: {method:?}"),
                };
                msg.respond_to.send(response).unwrap();
            }
        });

        let (mut client, server, path) = local_stream_pair("agpt-shutdown");
        client
            .write_all(
                br#"{"id":"prompt_shutdown","method":"agent.prompt","params":{"target":"main","text":"shutdown","wait":{"until":["idle"],"timeout_ms":6000}}}"#,
            )
            .unwrap();
        client.write_all(b"\n").unwrap();
        client.flush().unwrap();

        let running = Arc::new(AtomicBool::new(true));
        let server_running = Arc::clone(&running);
        let event_hub = EventHub::default();
        let server_event_hub = event_hub.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let server_thread = std::thread::spawn(move || {
            let result = handle_connection(
                server,
                &api_tx,
                &server_event_hub,
                &server_running,
                None,
            );
            done_tx.send(result).unwrap();
        });

        prompt_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        running.store(false, Ordering::Relaxed);
        assert!(done_rx.recv_timeout(Duration::from_secs(2)).unwrap().is_ok());
        server_thread.join().unwrap();
        drop(client);
        responder.join().unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn agent_prompt_wait_reports_stall_at_effect_bound() {
        let (api_tx, mut api_rx) = mpsc::unbounded_channel::<ApiRequestMessage>();
        let responder = std::thread::spawn(move || {
            while let Some(msg) = api_rx.blocking_recv() {
                let response = match msg.request.method {
                    Method::AgentGet(_) => prompt_test_response(
                        msg.request.id,
                        crate::api::schema::AgentStatus::Idle,
                        "terminal_1",
                        false,
                    ),
                    Method::AgentPrompt(_) => prompt_test_response(
                        msg.request.id,
                        crate::api::schema::AgentStatus::Idle,
                        "terminal_1",
                        true,
                    ),
                    method => panic!("unexpected method: {method:?}"),
                };
                msg.respond_to.send(response).unwrap();
            }
        });

        let (mut client, server, path) = local_stream_pair("agpt-stall");
        client
            .write_all(
                br#"{"id":"prompt_stall","method":"agent.prompt","params":{"target":"main","text":"stall","wait":{"until":["idle"],"timeout_ms":6000}}}"#,
            )
            .unwrap();
        client.write_all(b"\n").unwrap();
        client.flush().unwrap();

        let running = Arc::new(AtomicBool::new(true));
        let server_running = Arc::clone(&running);
        let event_hub = EventHub::default();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let server_thread = std::thread::spawn(move || {
            let result = handle_connection(server, &api_tx, &event_hub, &server_running, None);
            done_tx.send(result).unwrap();
        });

        let started = Instant::now();
        let response: serde_json::Value = serde_json::from_str(&read_line(&mut client)).unwrap();
        let elapsed = started.elapsed();
        assert_eq!(response["id"], "prompt_stall");
        assert_eq!(response["error"]["code"], "agent_prompt_stalled");
        assert!(
            response["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("state_change_seq remained"))
        );
        assert!(
            elapsed >= Duration::from_secs(4),
            "stall returned too early: {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_secs(6),
            "stall exceeded bound: {elapsed:?}"
        );

        assert!(done_rx.recv_timeout(Duration::from_secs(2)).unwrap().is_ok());
        server_thread.join().unwrap();
        responder.join().unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn initial_request_reader_waits_for_newline_after_partial_data() {
        let (mut client, mut server, path) = local_stream_pair("api-initial-partial");
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let reader_thread = std::thread::spawn(move || {
            done_tx.send(read_initial_request_line_with_timeout(
                &mut server,
                Duration::from_secs(1),
            ))
        });
        client
            .write_all(br#"{"id":"partial","method":"ping","params":{}}"#)
            .unwrap();
        client.flush().unwrap();
        std::thread::sleep(Duration::from_millis(100));
        assert!(done_rx.try_recv().is_err());
        client.write_all(b"\n").unwrap();
        client.flush().unwrap();
        let line = done_rx.recv().unwrap().unwrap().unwrap();
        reader_thread.join().unwrap().unwrap();
        assert!(line.ends_with('\n'));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn timed_read_skips_reset_after_stream_ends() {
        let mut reset_called = false;
        let result = finish_timed_read::<()>(Ok(None), || {
            reset_called = true;
            Ok(())
        });

        assert!(result.unwrap().is_none());
        assert!(!reset_called);
    }

    #[test]
    fn initial_request_reader_handles_disconnect_during_poll() {
        let (client, mut server, path) = local_stream_pair("api-initial-disconnect");
        drop(client);

        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let reader_thread = std::thread::spawn(move || {
            done_tx.send(read_initial_request_line_with_timeout(
                &mut server,
                Duration::from_secs(1),
            ))
        });

        let result = done_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("disconnect should finish the reader");
        assert!(result.unwrap().is_none());
        reader_thread.join().unwrap().unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn socket_path_prefers_explicit_env_override() {
        let _guard = env_lock().lock().unwrap();
        let unique = format!(
            "/tmp/omh-test-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        );
        let _session_env = TestEnvVar::remove(crate::session::SESSION_ENV_VAR);
        let _explicit_session = crate::session::explicit_session_request_guard(false);
        let _socket_env = TestEnvVar::set(crate::api::SOCKET_PATH_ENV_VAR, &unique);
        assert_eq!(socket_path(), PathBuf::from(&unique));
    }

    #[test]
    fn socket_path_defaults_to_config_dir_even_when_xdg_runtime_dir_is_set() {
        let _guard = env_lock().lock().unwrap();
        let config_home = unique_test_path("socket-default-config-home");
        let runtime_dir = unique_test_path("socket-default-runtime");
        let _socket_env = TestEnvVar::remove(crate::api::SOCKET_PATH_ENV_VAR);
        let _session_env = TestEnvVar::remove(crate::session::SESSION_ENV_VAR);
        let _explicit_session = crate::session::explicit_session_request_guard(false);
        let _config_home_env = TestEnvVar::set("XDG_CONFIG_HOME", &config_home);
        let _runtime_dir_env = TestEnvVar::set("XDG_RUNTIME_DIR", &runtime_dir);

        let expected = config_home
            .join(crate::config::app_dir_name())
            .join("omh.sock");
        assert_eq!(socket_path(), expected);
    }

    #[test]
    fn socket_path_uses_named_session_dir() {
        let _guard = env_lock().lock().unwrap();
        let config_home = unique_test_path("socket-named-config-home");
        let _socket_env = TestEnvVar::remove(crate::api::SOCKET_PATH_ENV_VAR);
        let _explicit_session = crate::session::explicit_session_request_guard(false);
        let _session_env = TestEnvVar::set(crate::session::SESSION_ENV_VAR, "work");
        let _config_home_env = TestEnvVar::set("XDG_CONFIG_HOME", &config_home);

        let expected = config_home
            .join(crate::config::app_dir_name())
            .join("sessions")
            .join("work")
            .join("omh.sock");
        assert_eq!(socket_path(), expected);
    }

    #[test]
    fn restrict_socket_permissions_sets_user_only_mode() {
        let dir = unique_test_path("socket-perms");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("api.sock");
        let _listener = UnixListener::bind(&path).unwrap();

        restrict_socket_permissions(&path).unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, SOCKET_PERMISSION_MODE);

        drop(_listener);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn api_response_outcome_uses_top_level_error_shape() {
        let ok_with_error_text = r#"{"id":"req","result":{"read":{"text":"user said \"error\": \"timeout\"","revision":1}}}"#;
        assert_eq!(api_response_outcome(ok_with_error_text), "ok");

        let timeout = r#"{"id":"req","error":{"code":"timeout","message":"timed out waiting for output match"}}"#;
        assert_eq!(api_response_outcome(timeout), "timeout");

        let generic_error =
            r#"{"id":"req","error":{"code":"server_unavailable","message":"boom"}}"#;
        assert_eq!(api_response_outcome(generic_error), "error");
    }

    #[test]
    fn ping_request_returns_pong() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let response = handle_request(
            Request {
                id: "req_1".into(),
                method: Method::Ping(crate::api::schema::PingParams::default()),
            },
            &tx,
            Some(ServerCapabilities { live_handoff: true }),
        );

        let parsed: SuccessResponse = serde_json::from_str(&response.body).unwrap();
        assert_eq!(parsed.id, "req_1");
        assert!(matches!(parsed.result, ResponseResult::Pong { .. }));
    }

    #[test]
    fn default_capabilities_match_platform_handoff_support() {
        assert_eq!(
            default_server_capabilities().unwrap().live_handoff,
            cfg!(unix)
        );
    }

    #[test]
    fn request_dispatches_to_app_channel() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let request = Request {
            id: "req_2".into(),
            method: Method::WorkspaceList(crate::api::schema::EmptyParams::default()),
        };

        let request_for_thread = request.clone();
        let thread = std::thread::spawn(move || handle_request(request_for_thread, &tx, None));

        let msg = rx.blocking_recv().unwrap();
        assert_eq!(msg.request.id, "req_2");
        msg.respond_to
            .send(
                serde_json::to_string(&SuccessResponse {
                    id: "req_2".into(),
                    result: ResponseResult::Ok {},
                })
                .unwrap(),
            )
            .unwrap();

        let response = thread.join().unwrap();
        let parsed: SuccessResponse = serde_json::from_str(&response.body).unwrap();
        assert_eq!(parsed.id, "req_2");
    }

    #[test]
    fn request_reports_when_response_reaches_socket() {
        let (api_tx, mut api_rx) = mpsc::unbounded_channel::<ApiRequestMessage>();
        let (mut client, server, _path) = local_stream_pair("api-response-written");
        client
            .write_all(br#"{"id":"req_written","method":"workspace.list","params":{}}"#)
            .unwrap();
        client.write_all(b"\n").unwrap();
        client.flush().unwrap();

        let running = Arc::new(AtomicBool::new(true));
        let server_running = Arc::clone(&running);
        let event_hub = EventHub::default();
        let server_thread = std::thread::spawn(move || {
            handle_connection(server, &api_tx, &event_hub, &server_running, None)
        });

        let msg = api_rx.blocking_recv().unwrap();
        let response_written = msg.response_written.unwrap();
        msg.respond_to
            .send(
                serde_json::to_string(&SuccessResponse {
                    id: "req_written".into(),
                    result: ResponseResult::Ok {},
                })
                .unwrap(),
            )
            .unwrap();
        response_written
            .recv_timeout(Duration::from_secs(2))
            .unwrap();

        let mut line = String::new();
        BufReader::new(client).read_line(&mut line).unwrap();
        let response: SuccessResponse = serde_json::from_str(&line).unwrap();
        assert_eq!(response.id, "req_written");
        server_thread.join().unwrap().unwrap();
    }

    #[test]
    fn wait_for_output_stops_when_client_disconnects() {
        let (api_tx, mut api_rx) = mpsc::unbounded_channel::<ApiRequestMessage>();
        let (first_read_tx, first_read_rx) = std::sync::mpsc::channel();
        let responder = std::thread::spawn(move || {
            let mut notified = false;
            while let Some(msg) = api_rx.blocking_recv() {
                assert!(matches!(msg.request.method, Method::PaneRead(_)));
                if !notified {
                    first_read_tx.send(()).unwrap();
                    notified = true;
                }
                msg.respond_to
                    .send(
                        serde_json::to_string(&SuccessResponse {
                            id: msg.request.id,
                            result: ResponseResult::PaneRead {
                                read: crate::api::schema::PaneReadResult {
                                    pane_id: "pane_1".into(),
                                    workspace_id: "ws_1".into(),
                                    tab_id: "tab_1".into(),
                                    source: crate::api::schema::ReadSource::RecentUnwrapped,
                                    format: crate::api::schema::ReadFormat::Text,
                                    text: String::new(),
                                    revision: 0,
                                    truncated: false,
                                },
                            },
                        })
                        .unwrap(),
                    )
                    .unwrap();
            }
        });

        let (mut client, server, _path) = local_stream_pair("api-wait-disconnect");
        client
            .write_all(br#"{"id":"req_wait","method":"pane.wait_for_output","params":{"pane_id":"pane_1","source":"recent","match":{"type":"substring","value":"never"}}}"#)
            .unwrap();
        client.write_all(b"\n").unwrap();
        client.flush().unwrap();

        let running = Arc::new(AtomicBool::new(true));
        let server_running = Arc::clone(&running);
        let event_hub = EventHub::default();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let server_thread = std::thread::spawn(move || {
            let result = handle_connection(server, &api_tx, &event_hub, &server_running, None);
            done_tx.send(result).unwrap();
        });

        first_read_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        drop(client);

        let result = done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(result.is_ok());

        server_thread.join().unwrap();
        drop(running);
        responder.join().unwrap();
    }

    #[test]
    fn subscriptions_stop_when_client_disconnects() {
        let (api_tx, _api_rx) = mpsc::unbounded_channel::<ApiRequestMessage>();
        let (mut client, server, _path) = local_stream_pair("api-sub-disconnect");
        client
            .write_all(
                br#"{"id":"sub_1","method":"events.subscribe","params":{"subscriptions":[{"type":"workspace.created"}]}}"#,
            )
            .unwrap();
        client.write_all(b"\n").unwrap();
        client.flush().unwrap();

        let running = Arc::new(AtomicBool::new(true));
        let server_running = Arc::clone(&running);
        let event_hub = EventHub::default();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let server_thread = std::thread::spawn(move || {
            let result = handle_connection(server, &api_tx, &event_hub, &server_running, None);
            done_tx.send(result).unwrap();
        });

        let ack = read_line(&mut client);
        let ack: serde_json::Value = serde_json::from_str(&ack).unwrap();
        assert_eq!(ack["result"]["type"], "subscription_started");

        drop(client);

        let result = done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(result.is_ok());
        server_thread.join().unwrap();
    }

    #[test]
    fn subscriptions_report_pane_not_found_when_agent_status_pane_closes() {
        let (api_tx, mut api_rx) = mpsc::unbounded_channel::<ApiRequestMessage>();
        let responder = std::thread::spawn(move || {
            let mut pane_get_count = 0;
            while let Some(msg) = api_rx.blocking_recv() {
                assert!(matches!(msg.request.method, Method::PaneGet(_)));
                pane_get_count += 1;
                let response = if pane_get_count == 1 {
                    serde_json::to_string(&SuccessResponse {
                        id: msg.request.id,
                        result: ResponseResult::PaneInfo {
                            pane: crate::api::schema::PaneInfo {
                                pane_id: "pane_1".into(),
                                terminal_id: "terminal_1".into(),
                                workspace_id: "workspace_1".into(),
                                tab_id: "tab_1".into(),
                                focused: false,
                                cwd: None,
                                foreground_cwd: None,
                                label: None,
                                agent: Some("pi".into()),
                                title: None,
                                display_agent: Some("pi".into()),
                                agent_status: crate::api::schema::AgentStatus::Unknown,
                                custom_status: None,
                                state_labels: std::collections::HashMap::new(),
                                tokens: std::collections::HashMap::new(),
                                agent_session: None,
                                scroll: None,
                                revision: 0,
                            },
                        },
                    })
                    .unwrap()
                } else {
                    error_response_json(
                        msg.request.id,
                        "pane_not_found",
                        "pane pane_1 not found".into(),
                    )
                };
                msg.respond_to.send(response).unwrap();
            }
        });

        let (mut client, server, path) = local_stream_pair("api-sub-pane-close");
        client
            .write_all(
                br#"{"id":"sub_agent_wait","method":"events.subscribe","params":{"subscriptions":[{"type":"pane.agent_status_changed","pane_id":"pane_1","agent_status":"done"}]}}"#,
            )
            .unwrap();
        client.write_all(b"\n").unwrap();
        client.flush().unwrap();

        let running = Arc::new(AtomicBool::new(true));
        handle_connection(server, &api_tx, &EventHub::default(), &running, None).unwrap();

        let mut reader = BufReader::new(client);
        let mut ack_line = String::new();
        reader.read_line(&mut ack_line).unwrap();
        let ack: serde_json::Value = serde_json::from_str(&ack_line).unwrap();
        assert_eq!(ack["id"], "sub_agent_wait");
        assert_eq!(ack["result"]["type"], "subscription_started");

        let mut error_line = String::new();
        reader.read_line(&mut error_line).unwrap();
        let response: serde_json::Value = serde_json::from_str(&error_line).unwrap();
        assert_eq!(response["id"], "sub_agent_wait");
        assert_eq!(response["error"]["code"], "pane_not_found");
        assert_eq!(response["error"]["message"], "pane pane_1 not found");

        drop(api_tx);
        responder.join().unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn subscriptions_stop_when_server_shuts_down() {
        let (api_tx, _api_rx) = mpsc::unbounded_channel::<ApiRequestMessage>();
        let (mut client, server, _path) = local_stream_pair("api-sub-shutdown");
        client
            .write_all(
                br#"{"id":"sub_2","method":"events.subscribe","params":{"subscriptions":[{"type":"workspace.created"}]}}"#,
            )
            .unwrap();
        client.write_all(b"\n").unwrap();
        client.flush().unwrap();

        let running = Arc::new(AtomicBool::new(true));
        let server_running = Arc::clone(&running);
        let event_hub = EventHub::default();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let server_thread = std::thread::spawn(move || {
            let result = handle_connection(server, &api_tx, &event_hub, &server_running, None);
            done_tx.send(result).unwrap();
        });

        let ack = read_line(&mut client);
        let ack: serde_json::Value = serde_json::from_str(&ack).unwrap();
        assert_eq!(ack["result"]["type"], "subscription_started");

        running.store(false, Ordering::Relaxed);

        let result = done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(result.is_ok());
        server_thread.join().unwrap();
    }

    fn run_agent_wait_event(
        name: &str,
        event: crate::api::schema::EventData,
        replacement: Option<crate::api::schema::AgentInfo>,
    ) -> serde_json::Value {
        let expected = agent_info(
            "terminal_1",
            "pane_1",
            crate::api::schema::AgentStatus::Working,
            Some("pi"),
        );
        let replacement_for_responder = replacement.clone();
        let (api_tx, mut api_rx) = mpsc::unbounded_channel::<ApiRequestMessage>();
        let (initial_tx, initial_rx) = std::sync::mpsc::channel();
        let responder = std::thread::spawn(move || {
            let mut request_count = 0;
            while let Some(msg) = api_rx.blocking_recv() {
                assert!(matches!(msg.request.method, Method::AgentGet(_)));
                request_count += 1;
                let response_agent = if request_count == 1 {
                    expected.clone()
                } else {
                    replacement_for_responder
                        .clone()
                        .unwrap_or_else(|| expected.clone())
                };
                msg.respond_to
                    .send(agent_get_response(&msg, response_agent))
                    .unwrap();
                if request_count == 1 {
                    initial_tx.send(()).unwrap();
                }
            }
        });

        let (mut client, server, path) = local_stream_pair(name);
        client
            .write_all(
                br#"{"id":"agent_wait_lifecycle","method":"agent.wait","params":{"target":"pi","until":["done"],"timeout_ms":500}}"#,
            )
            .unwrap();
        client.write_all(b"\n").unwrap();
        client.flush().unwrap();

        let running = Arc::new(AtomicBool::new(true));
        let server_running = Arc::clone(&running);
        let event_hub = EventHub::default();
        let responder_event_hub = event_hub.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let server_api_tx = api_tx.clone();
        let server_thread = std::thread::spawn(move || {
            let result = handle_connection(
                server,
                &server_api_tx,
                &responder_event_hub,
                &server_running,
                None,
            );
            done_tx.send(result).unwrap();
        });

        initial_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        event_hub.push(pane_lifecycle_event(event));

        let result = done_rx.recv_timeout(Duration::from_secs(2));
        if result.is_err() {
            running.store(false, Ordering::Relaxed);
            drop(client);
            let _ = done_rx.recv_timeout(Duration::from_secs(1));
            server_thread.join().unwrap();
            drop(api_tx);
            responder.join().unwrap();
            let _ = std::fs::remove_file(path);
            panic!("{name} wait did not terminate");
        }
        let response = read_line(&mut client);

        server_thread.join().unwrap();
        drop(api_tx);
        responder.join().unwrap();
        let _ = std::fs::remove_file(path);
        serde_json::from_str(&response).unwrap()
    }
    #[test]
    fn agent_prompt_wait_returns_agent_not_running_when_target_pane_closes() {
        let expected = agent_info(
            "terminal_1",
            "pane_1",
            crate::api::schema::AgentStatus::Working,
            Some("pi"),
        );
        let (api_tx, mut api_rx) = mpsc::unbounded_channel::<ApiRequestMessage>();
        let (initial_tx, initial_rx) = std::sync::mpsc::channel();
        let responder = std::thread::spawn(move || {
            while let Some(msg) = api_rx.blocking_recv() {
                let response = match msg.request.method {
                    Method::AgentGet(_) => agent_get_response(&msg, expected.clone()),
                    Method::AgentPrompt(_) => {
                        initial_tx.send(()).unwrap();
                        agent_prompt_response(&msg, expected.clone())
                    }
                    method => panic!("unexpected method: {method:?}"),
                };
                msg.respond_to.send(response).unwrap();
            }
        });

        let (mut client, server, path) = local_stream_pair("agent-prompt-wait-close");
        client
            .write_all(
                br#"{"id":"agent_prompt_wait_close","method":"agent.prompt","params":{"target":"pi","text":"hello","wait":{"until":["done"],"timeout_ms":500}}}"#,
            )
            .unwrap();
        client.write_all(b"\n").unwrap();
        client.flush().unwrap();
        let running = Arc::new(AtomicBool::new(true));
        let server_running = Arc::clone(&running);
        let event_hub = EventHub::default();
        let responder_event_hub = event_hub.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let server_api_tx = api_tx.clone();
        let server_thread = std::thread::spawn(move || {
            let result = handle_connection(
                server,
                &server_api_tx,
                &responder_event_hub,
                &server_running,
                None,
            );
            done_tx.send(result).unwrap();
        });
        initial_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        event_hub.push(pane_lifecycle_event(
            crate::api::schema::EventData::PaneClosed {
                pane_id: "pane_1".into(),
                workspace_id: "workspace_1".into(),
            },
        ));
        assert!(done_rx.recv_timeout(Duration::from_secs(2)).unwrap().is_ok());
        let response: serde_json::Value = serde_json::from_str(&read_line(&mut client)).unwrap();
        assert_eq!(response["error"]["code"], "agent_not_running");
        server_thread.join().unwrap();
        drop(api_tx);
        responder.join().unwrap();
        let _ = std::fs::remove_file(path);
    }


    #[test]
    fn agent_wait_returns_agent_not_running_when_target_pane_closes() {
        let response = run_agent_wait_event(
            "agent-wait-pane-closed",
            crate::api::schema::EventData::PaneClosed {
                pane_id: "pane_1".into(),
                workspace_id: "workspace_1".into(),
            },
            None,
        );
        assert_eq!(response["error"]["code"], "agent_not_running");
    }

    #[test]
    fn agent_wait_returns_agent_not_running_when_target_pane_exits() {
        let response = run_agent_wait_event(
            "agent-wait-pane-exited",
            crate::api::schema::EventData::PaneExited {
                pane_id: "pane_1".into(),
                workspace_id: "workspace_1".into(),
            },
            None,
        );
        assert_eq!(response["error"]["code"], "agent_not_running");
    }

    #[test]
    fn agent_wait_returns_agent_not_running_when_target_loses_agent() {
        let response = run_agent_wait_event(
            "agent-wait-agent-lost",
            crate::api::schema::EventData::PaneAgentDetected {
                pane_id: "pane_1".into(),
                workspace_id: "workspace_1".into(),
                agent: None,
            },
            None,
        );
        assert_eq!(response["error"]["code"], "agent_not_running");
    }

    #[test]
    fn agent_wait_returns_agent_not_running_when_target_is_replaced() {
        let response = run_agent_wait_event(
            "agent-wait-replaced",
            crate::api::schema::EventData::PaneAgentDetected {
                pane_id: "pane_1".into(),
                workspace_id: "workspace_1".into(),
                agent: Some("codex".into()),
            },
            Some(agent_info(
                "terminal_2",
                "pane_1",
                crate::api::schema::AgentStatus::Working,
                Some("codex"),
            )),
        );
        assert_eq!(response["error"]["code"], "agent_not_running");
    }

    #[test]
    fn agent_wait_returns_agent_not_running_when_event_probe_loses_agent() {
        let response = run_agent_wait_event(
            "agent-wait-probe-lost",
            crate::api::schema::EventData::PaneAgentDetected {
                pane_id: "pane_1".into(),
                workspace_id: "workspace_1".into(),
                agent: Some("codex".into()),
            },
            Some(agent_info(
                "terminal_1",
                "pane_1",
                crate::api::schema::AgentStatus::Working,
                None,
            )),
        );
        assert_eq!(response["error"]["code"], "agent_not_running");
    }

    #[test]
    fn agent_wait_maps_missing_event_probe_to_agent_not_running() {
        let expected = agent_info(
            "terminal_1",
            "pane_1",
            crate::api::schema::AgentStatus::Working,
            Some("pi"),
        );
        let (api_tx, mut api_rx) = mpsc::unbounded_channel::<ApiRequestMessage>();
        let (initial_tx, initial_rx) = std::sync::mpsc::channel();
        let responder = std::thread::spawn(move || {
            let mut request_count = 0;
            while let Some(msg) = api_rx.blocking_recv() {
                assert!(matches!(msg.request.method, Method::AgentGet(_)));
                request_count += 1;
                let response = if request_count == 1 {
                    agent_get_response(&msg, expected.clone())
                } else {
                    error_response_json(
                        msg.request.id.clone(),
                        "agent_not_found",
                        "agent target pi not found".into(),
                    )
                };
                msg.respond_to.send(response).unwrap();
                if request_count == 1 {
                    initial_tx.send(()).unwrap();
                }
            }
        });

        let (mut client, server, path) = local_stream_pair("agent-wait-probe-missing");
        client
            .write_all(
                br#"{"id":"agent_wait_probe_missing","method":"agent.wait","params":{"target":"pi","until":["done"],"timeout_ms":500}}"#,
            )
            .unwrap();
        client.write_all(b"\n").unwrap();
        client.flush().unwrap();
        let running = Arc::new(AtomicBool::new(true));
        let server_running = Arc::clone(&running);
        let event_hub = EventHub::default();
        let responder_event_hub = event_hub.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let server_api_tx = api_tx.clone();
        let server_thread = std::thread::spawn(move || {
            let result = handle_connection(
                server,
                &server_api_tx,
                &responder_event_hub,
                &server_running,
                None,
            );
            done_tx.send(result).unwrap();
        });
        initial_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        event_hub.push(pane_lifecycle_event(
            crate::api::schema::EventData::PaneAgentStatusChanged {
                pane_id: "pane_1".into(),
                workspace_id: "workspace_1".into(),
                agent_status: crate::api::schema::AgentStatus::Done,
                agent: Some("pi".into()),
                title: None,
                display_agent: Some("pi".into()),
                custom_status: None,
                state_labels: std::collections::HashMap::new(),
                tokens: std::collections::HashMap::new(),
            },
        ));
        assert!(done_rx.recv_timeout(Duration::from_secs(2)).unwrap().is_ok());
        let response: serde_json::Value = serde_json::from_str(&read_line(&mut client)).unwrap();
        assert_eq!(response["error"]["code"], "agent_not_running");
        server_thread.join().unwrap();
        drop(api_tx);
        responder.join().unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn agent_wait_returns_matching_status_for_same_target_identity() {
        let expected = agent_info(
            "terminal_1",
            "pane_1",
            crate::api::schema::AgentStatus::Working,
            Some("pi"),
        );
        let (api_tx, mut api_rx) = mpsc::unbounded_channel::<ApiRequestMessage>();
        let (initial_tx, initial_rx) = std::sync::mpsc::channel();
        let responder = std::thread::spawn(move || {
            let mut request_count = 0;
            while let Some(msg) = api_rx.blocking_recv() {
                assert!(matches!(msg.request.method, Method::AgentGet(_)));
                request_count += 1;
                let agent = if request_count == 1 {
                    initial_tx.send(()).unwrap();
                    expected.clone()
                } else {
                    agent_info(
                        "terminal_1",
                        "pane_1",
                        crate::api::schema::AgentStatus::Done,
                        Some("pi"),
                    )
                };
                msg.respond_to.send(agent_get_response(&msg, agent)).unwrap();
            }
        });

        let (mut client, server, path) = local_stream_pair("agent-wait-success");
        client
            .write_all(
                br#"{"id":"agent_wait_success","method":"agent.wait","params":{"target":"pi","until":["done"],"timeout_ms":500}}"#,
            )
            .unwrap();
        client.write_all(b"\n").unwrap();
        client.flush().unwrap();
        let running = Arc::new(AtomicBool::new(true));
        let server_running = Arc::clone(&running);
        let event_hub = EventHub::default();
        let responder_event_hub = event_hub.clone();
        let server_api_tx = api_tx.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let server_thread = std::thread::spawn(move || {
            let result = handle_connection(
                server,
                &server_api_tx,
                &responder_event_hub,
                &server_running,
                None,
            );
            done_tx.send(result).unwrap();
        });
        initial_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        event_hub.push(pane_lifecycle_event(
            crate::api::schema::EventData::PaneAgentStatusChanged {
                pane_id: "pane_1".into(),
                workspace_id: "workspace_1".into(),
                agent_status: crate::api::schema::AgentStatus::Done,
                agent: Some("pi".into()),
                title: None,
                display_agent: Some("pi".into()),
                custom_status: None,
                state_labels: std::collections::HashMap::new(),
                tokens: std::collections::HashMap::new(),
            },
        ));
        assert!(done_rx.recv_timeout(Duration::from_secs(2)).unwrap().is_ok());
        let response: serde_json::Value = serde_json::from_str(&read_line(&mut client)).unwrap();
        assert_eq!(response["result"]["type"], "agent_info");
        assert_eq!(response["result"]["agent"]["terminal_id"], "terminal_1");
        server_thread.join().unwrap();
        drop(api_tx);
        responder.join().unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn agent_wait_times_out_without_lifecycle_event() {
        let expected = agent_info(
            "terminal_1",
            "pane_1",
            crate::api::schema::AgentStatus::Working,
            Some("pi"),
        );
        let (api_tx, mut api_rx) = mpsc::unbounded_channel::<ApiRequestMessage>();
        let (initial_tx, initial_rx) = std::sync::mpsc::channel();
        let responder = std::thread::spawn(move || {
            while let Some(msg) = api_rx.blocking_recv() {
                assert!(matches!(msg.request.method, Method::AgentGet(_)));
                msg.respond_to
                    .send(agent_get_response(&msg, expected.clone()))
                    .unwrap();
                initial_tx.send(()).unwrap();
            }
        });

        let (mut client, server, path) = local_stream_pair("agent-wait-timeout");
        client
            .write_all(
                br#"{"id":"agent_wait_timeout","method":"agent.wait","params":{"target":"pi","until":["done"],"timeout_ms":20}}"#,
            )
            .unwrap();
        client.write_all(b"\n").unwrap();
        client.flush().unwrap();
        let running = Arc::new(AtomicBool::new(true));
        let server_running = Arc::clone(&running);
        let event_hub = EventHub::default();
        let server_api_tx = api_tx.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let server_thread = std::thread::spawn(move || {
            let result = handle_connection(server, &server_api_tx, &event_hub, &server_running, None);
            done_tx.send(result).unwrap();
        });
        initial_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(done_rx.recv_timeout(Duration::from_secs(2)).unwrap().is_ok());
        let response: serde_json::Value = serde_json::from_str(&read_line(&mut client)).unwrap();
        assert_eq!(response["error"]["code"], "timeout");
        server_thread.join().unwrap();
        drop(api_tx);
        responder.join().unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn agent_wait_stops_when_client_disconnects() {
        let expected = agent_info(
            "terminal_1",
            "pane_1",
            crate::api::schema::AgentStatus::Working,
            Some("pi"),
        );
        let (api_tx, mut api_rx) = mpsc::unbounded_channel::<ApiRequestMessage>();
        let (initial_tx, initial_rx) = std::sync::mpsc::channel();
        let responder = std::thread::spawn(move || {
            while let Some(msg) = api_rx.blocking_recv() {
                assert!(matches!(msg.request.method, Method::AgentGet(_)));
                msg.respond_to
                    .send(agent_get_response(&msg, expected.clone()))
                    .unwrap();
                initial_tx.send(()).unwrap();
            }
        });

        let (mut client, server, path) = local_stream_pair("agent-wait-disconnect");
        client
            .write_all(
                br#"{"id":"agent_wait_disconnect","method":"agent.wait","params":{"target":"pi","until":["done"],"timeout_ms":5000}}"#,
            )
            .unwrap();
        client.write_all(b"\n").unwrap();
        client.flush().unwrap();
        let running = Arc::new(AtomicBool::new(true));
        let server_running = Arc::clone(&running);
        let event_hub = EventHub::default();
        let server_api_tx = api_tx.clone();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let server_thread = std::thread::spawn(move || {
            let result = handle_connection(server, &server_api_tx, &event_hub, &server_running, None);
            done_tx.send(result).unwrap();
        });
        initial_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        drop(client);
        assert!(done_rx.recv_timeout(Duration::from_secs(2)).unwrap().is_ok());
        server_thread.join().unwrap();
        drop(api_tx);
        responder.join().unwrap();
        let _ = std::fs::remove_file(path);
    }
}
