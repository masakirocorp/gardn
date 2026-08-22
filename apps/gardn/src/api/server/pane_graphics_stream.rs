use std::io::{self, Read};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use interprocess::local_socket::traits::Stream as _;

use crate::api::schema::{
    ErrorBody, ErrorResponse, Method, PaneGraphicsSetParams, PaneGraphicsStreamParams, Request,
    ResponseResult, SuccessResponse,
};
use crate::api::ApiRequestSender;
use crate::ipc::LocalStream;

use super::{
    api_response_outcome, dispatch_stream_frame, dispatch_stream_open,
    dispatch_to_app_with_timeout, is_connection_closed_error, write_json_line,
    write_json_line_allow_disconnect, write_text_line_allow_disconnect, APP_RESPONSE_TIMEOUT,
    CONNECTION_POLL_INTERVAL,
};

const MAX_STREAM_FRAME_HEADER_BYTES: usize = 64 * 1024;
const STREAM_FRAME_BODY_CHUNK_BYTES: usize = 64 * 1024;
const STREAM_FRAME_HEADER_IDLE_TIMEOUT: Duration = Duration::from_secs(5);
const STREAM_FRAME_HEADER_TIMEOUT: Duration = Duration::from_secs(30);
const STREAM_FRAME_BODY_IDLE_TIMEOUT: Duration = Duration::from_secs(5);
const STREAM_FRAME_BODY_TIMEOUT: Duration = Duration::from_secs(30);
const STREAM_FALLBACK_POLL_INTERVAL: Duration = Duration::from_millis(1);
const STREAM_FALLBACK_FAST_POLLS: u8 = 32;
static NEXT_PANE_GRAPHICS_STREAM_OWNER: AtomicU64 = AtomicU64::new(1);

#[derive(serde::Deserialize)]
struct FrameFile {
    path: String,
}

#[derive(serde::Deserialize)]
struct FrameHeader {
    format: crate::api::schema::PaneGraphicsFormat,
    image_width: u32,
    image_height: u32,
    #[serde(default)]
    data_length: Option<usize>,
    #[serde(default)]
    file: Option<FrameFile>,
    #[serde(default)]
    sequence: u64,
    #[serde(default)]
    revision: u64,
    #[serde(default)]
    placement: crate::api::schema::PaneGraphicsPlacementParams,
}

#[derive(Clone, Copy)]
struct ReadTimeouts {
    header_idle: Duration,
    header_total: Duration,
    body_idle: Duration,
    body_total: Duration,
}

const READ_TIMEOUTS: ReadTimeouts = ReadTimeouts {
    header_idle: STREAM_FRAME_HEADER_IDLE_TIMEOUT,
    header_total: STREAM_FRAME_HEADER_TIMEOUT,
    body_idle: STREAM_FRAME_BODY_IDLE_TIMEOUT,
    body_total: STREAM_FRAME_BODY_TIMEOUT,
};

pub(super) fn serve(
    stream: LocalStream,
    request_id: String,
    params: PaneGraphicsStreamParams,
    api_tx: &ApiRequestSender,
    running: &Arc<AtomicBool>,
) -> std::io::Result<()> {
    serve_with_timeouts(
        stream,
        request_id,
        params,
        api_tx,
        running,
        APP_RESPONSE_TIMEOUT,
        READ_TIMEOUTS,
    )
}

fn serve_with_timeouts(
    mut stream: LocalStream,
    request_id: String,
    mut params: PaneGraphicsStreamParams,
    api_tx: &ApiRequestSender,
    running: &Arc<AtomicBool>,
    open_timeout: Duration,
    read_timeouts: ReadTimeouts,
) -> std::io::Result<()> {
    let pane_id = params.pane_id.clone();
    let layer_id = params.layer_id.clone();
    let z_index = params.z_index;
    let owner = next_owner();
    params.owner = owner.clone();
    let stream_active = Arc::new(AtomicBool::new(true));
    let open_response = dispatch_stream_open(
        Request {
            id: request_id.clone(),
            method: Method::PaneGraphicsStreamOpen(params),
        },
        api_tx,
        open_timeout,
        Arc::clone(&stream_active),
    );
    if api_response_outcome(&open_response) != "ok" {
        stream_active.store(false, Ordering::Release);
        let write_result = write_text_line_allow_disconnect(&mut stream, &open_response);
        clear_layer(&pane_id, layer_id.as_deref(), z_index, &owner, api_tx);
        write_result?;
        return Ok(());
    }

    if let Err(err) = write_json_line(
        &mut stream,
        &SuccessResponse {
            id: request_id.clone(),
            result: ResponseResult::Ok {},
        },
    ) {
        stream_active.store(false, Ordering::Release);
        clear_layer(&pane_id, layer_id.as_deref(), z_index, &owner, api_tx);
        if is_connection_closed_error(&err) {
            return Ok(());
        }
        return Err(err);
    }

    let result = serve_frames(
        &mut stream,
        &request_id,
        &owner,
        &pane_id,
        layer_id.as_deref(),
        z_index,
        api_tx,
        running,
        &stream_active,
        read_timeouts,
    );
    stream_active.store(false, Ordering::Release);
    clear_layer(&pane_id, layer_id.as_deref(), z_index, &owner, api_tx);
    result
}

fn serve_frames(
    stream: &mut LocalStream,
    request_id: &str,
    owner: &str,
    pane_id: &str,
    layer_id: Option<&str>,
    z_index: i32,
    api_tx: &ApiRequestSender,
    running: &Arc<AtomicBool>,
    stream_active: &Arc<AtomicBool>,
    timeouts: ReadTimeouts,
) -> std::io::Result<()> {
    let mut frame_seq = 0_u64;
    while stream_is_running(running, stream_active) {
        let Some(header_line) = read_line(
            stream,
            running,
            stream_active,
            MAX_STREAM_FRAME_HEADER_BYTES,
            timeouts.header_idle,
            timeouts.header_total,
        )?
        else {
            return Ok(());
        };
        let header_line = header_line.trim();
        if header_line.is_empty() {
            continue;
        }
        let header = match serde_json::from_str::<FrameHeader>(header_line) {
            Ok(header) => header,
            Err(err) => {
                write_json_line_allow_disconnect(
                    stream,
                    &ErrorResponse {
                        id: request_id.to_string(),
                        error: ErrorBody {
                            code: "invalid_frame".into(),
                            message: format!("invalid frame header: {err}"),
                        },
                    },
                )?;
                return Ok(());
            }
        };
        if let Some(file) = header.file {
            if !matches!(
                header.format,
                crate::api::schema::PaneGraphicsFormat::Rgba
                    | crate::api::schema::PaneGraphicsFormat::Bgra
            ) {
                write_json_line_allow_disconnect(
                    stream,
                    &ErrorResponse {
                        id: request_id.to_string(),
                        error: ErrorBody {
                            code: "invalid_frame".into(),
                            message: "file frames require rgba or bgra".into(),
                        },
                    },
                )?;
                return Ok(());
            }
            let response = dispatch_stream_frame(
                Request {
                    id: format!("{request_id}:file:{}", header.sequence),
                    method: Method::PaneGraphicsStreamDirect(
                        crate::api::schema::PaneGraphicsDirectParams {
                            pane_id: pane_id.to_owned(),
                            layer_id: layer_id.map(str::to_owned),
                            z_index,
                            owner: owner.to_owned(),
                            image_width: header.image_width,
                            image_height: header.image_height,
                            format: header.format,
                            path: file.path,
                            sequence: header.sequence,
                            revision: header.revision,
                            placement: header.placement,
                        },
                    ),
                },
                api_tx,
                Arc::clone(stream_active),
            );
            write_text_line_allow_disconnect(stream, &response)?;
            if api_response_outcome(&response) != "ok" {
                return Ok(());
            }
            continue;
        }

        let Some(data_length) = header.data_length else {
            write_json_line_allow_disconnect(
                stream,
                &ErrorResponse {
                    id: request_id.to_string(),
                    error: ErrorBody {
                        code: "invalid_frame".into(),
                        message: "frame requires data_length or file".into(),
                    },
                },
            )?;
            return Ok(());
        };
        if data_length == 0 {
            write_json_line_allow_disconnect(
                stream,
                &ErrorResponse {
                    id: request_id.to_string(),
                    error: ErrorBody {
                        code: "invalid_frame".into(),
                        message: "frame data_length must be greater than zero".into(),
                    },
                },
            )?;
            return Ok(());
        }
        if data_length > crate::api::schema::PANE_GRAPHICS_STREAM_MAX_BYTES {
            write_json_line_allow_disconnect(
                stream,
                &ErrorResponse {
                    id: request_id.to_string(),
                    error: ErrorBody {
                        code: "image_too_large".into(),
                        message: "frame data is too large".into(),
                    },
                },
            )?;
            return Ok(());
        }

        let Some(data) = read_exact(
            stream,
            data_length,
            running,
            stream_active,
            timeouts.body_idle,
            timeouts.body_total,
        )?
        else {
            return Ok(());
        };

        frame_seq = frame_seq.saturating_add(1);
        let frame_id = format!("{request_id}:frame:{frame_seq}");
        let response = dispatch_to_app_with_timeout(
            Request {
                id: frame_id,
                method: Method::PaneGraphicsStreamSet(PaneGraphicsSetParams {
                    pane_id: pane_id.to_string(),
                    layer_id: layer_id.map(str::to_owned),
                    z_index,
                    owner: owner.to_string(),
                    format: header.format,
                    image_width: header.image_width,
                    image_height: header.image_height,
                    data: Some(data),
                    data_base64: String::new(),
                    placement: header.placement,
                }),
            },
            api_tx,
            Some(APP_RESPONSE_TIMEOUT),
        );
        if api_response_outcome(&response) != "ok" {
            write_text_line_allow_disconnect(stream, &response)?;
            return Ok(());
        }
    }

    Ok(())
}

fn stream_is_running(running: &AtomicBool, stream_active: &AtomicBool) -> bool {
    running.load(Ordering::Relaxed) && stream_active.load(Ordering::Acquire)
}

fn next_owner() -> String {
    let id = NEXT_PANE_GRAPHICS_STREAM_OWNER.fetch_add(1, Ordering::Relaxed);
    format!("pane.graphics.stream:{}:{id}", std::process::id())
}

fn clear_layer(
    pane_id: &str,
    layer_id: Option<&str>,
    z_index: i32,
    owner: &str,
    api_tx: &ApiRequestSender,
) {
    let _response = dispatch_to_app_with_timeout(
        Request {
            id: format!("pane.graphics.stream.clear:{pane_id}"),
            method: Method::PaneGraphicsStreamClose(PaneGraphicsStreamParams {
                pane_id: pane_id.to_string(),
                layer_id: layer_id.map(str::to_owned),
                z_index,
                owner: owner.to_string(),
            }),
        },
        api_tx,
        Some(APP_RESPONSE_TIMEOUT),
    );
}

fn read_line(
    stream: &mut LocalStream,
    running: &Arc<AtomicBool>,
    stream_active: &Arc<AtomicBool>,
    max_bytes: usize,
    idle_timeout: Duration,
    total_timeout: Duration,
) -> std::io::Result<Option<String>> {
    with_timed_reads(stream, |stream, mut wait| {
        let mut bytes = Vec::new();
        let mut byte = [0_u8; 1];
        let mut total_deadline = None;
        let mut idle_deadline = None;

        loop {
            if !stream_is_running(running, stream_active) {
                return Ok(None);
            }
            ensure_before_deadlines(
                idle_deadline,
                total_deadline,
                "timed out reading stream frame header",
            )?;
            match stream.read(&mut byte) {
                Ok(0) => return Ok(None),
                Ok(_) => {
                    wait.on_progress();
                    let now = Instant::now();
                    let total_deadline_at =
                        *total_deadline.get_or_insert_with(|| now + total_timeout);
                    idle_deadline = Some(now + idle_timeout);
                    if now >= total_deadline_at {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "timed out reading stream frame header",
                        ));
                    }
                    bytes.push(byte[0]);
                    if byte[0] == b'\n' {
                        return String::from_utf8(bytes)
                            .map(Some)
                            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err));
                    }
                    if bytes.len() > max_bytes {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "stream frame header is too large",
                        ));
                    }
                }
                Err(err) if read_should_retry(&err) => {
                    wait.after_retry(idle_deadline, total_deadline);
                }
                Err(err) if is_connection_closed_error(&err) => return Ok(None),
                Err(err) => return Err(err),
            }
        }
    })
}

fn read_exact(
    stream: &mut LocalStream,
    len: usize,
    running: &Arc<AtomicBool>,
    stream_active: &Arc<AtomicBool>,
    idle_timeout: Duration,
    total_timeout: Duration,
) -> std::io::Result<Option<Vec<u8>>> {
    with_timed_reads(stream, |stream, mut wait| {
        let mut data = Vec::new();
        let mut chunk = vec![0_u8; STREAM_FRAME_BODY_CHUNK_BYTES.min(len)];
        let total_deadline = Instant::now() + total_timeout;
        let mut idle_deadline = Instant::now() + idle_timeout;

        while data.len() < len {
            if !stream_is_running(running, stream_active) {
                return Ok(None);
            }
            ensure_before_deadlines(
                Some(idle_deadline),
                Some(total_deadline),
                "timed out reading stream frame body",
            )?;
            let remaining = len - data.len();
            let read_len = remaining.min(chunk.len());
            match stream.read(&mut chunk[..read_len]) {
                Ok(0) if data.is_empty() => return Ok(None),
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "stream ended mid-frame",
                    ));
                }
                Ok(n) => {
                    wait.on_progress();
                    let now = Instant::now();
                    if now >= total_deadline {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "timed out reading stream frame body",
                        ));
                    }
                    data.extend_from_slice(&chunk[..n]);
                    idle_deadline = now + idle_timeout;
                }
                Err(err) if read_should_retry(&err) => {
                    wait.after_retry(Some(idle_deadline), Some(total_deadline));
                }
                Err(err) if is_connection_closed_error(&err) && data.is_empty() => return Ok(None),
                Err(err) => return Err(err),
            }
        }

        Ok(Some(data))
    })
}

#[derive(Clone, Copy)]
enum ReadWait {
    SocketTimeout,
    Poll(PollBackoff),
}

impl ReadWait {
    fn after_retry(&mut self, idle_deadline: Option<Instant>, total_deadline: Option<Instant>) {
        if let Self::Poll(backoff) = self {
            sleep_until_poll(idle_deadline, total_deadline, backoff.interval);
            backoff.advance();
        }
    }

    fn on_progress(&mut self) {
        if let Self::Poll(backoff) = self {
            backoff.reset();
        }
    }
}

#[derive(Clone, Copy)]
struct PollBackoff {
    interval: Duration,
    fast_polls_remaining: u8,
}

impl PollBackoff {
    fn new() -> Self {
        Self {
            interval: STREAM_FALLBACK_POLL_INTERVAL,
            fast_polls_remaining: STREAM_FALLBACK_FAST_POLLS,
        }
    }

    fn advance(&mut self) {
        if self.fast_polls_remaining > 0 {
            self.fast_polls_remaining -= 1;
            return;
        }
        self.interval = (self.interval * 2).min(CONNECTION_POLL_INTERVAL);
    }

    fn reset(&mut self) {
        *self = Self::new();
    }
}

fn with_timed_reads<T>(
    stream: &mut LocalStream,
    read: impl FnOnce(&mut LocalStream, ReadWait) -> std::io::Result<Option<T>>,
) -> std::io::Result<Option<T>> {
    match stream.set_recv_timeout(Some(CONNECTION_POLL_INTERVAL)) {
        Ok(()) => {
            let result = read(stream, ReadWait::SocketTimeout);
            finish_timed_read(result, || stream.set_recv_timeout(None))
        }
        Err(err) if err.kind() == io::ErrorKind::Unsupported => {
            stream.set_nonblocking(true)?;
            let result = read(stream, ReadWait::Poll(PollBackoff::new()));
            finish_timed_read(result, || stream.set_nonblocking(false))
        }
        Err(err) if err.kind() == io::ErrorKind::InvalidInput => Ok(None),
        Err(err) => Err(err),
    }
}

fn finish_timed_read<T>(
    result: std::io::Result<Option<T>>,
    reset: impl FnOnce() -> std::io::Result<()>,
) -> std::io::Result<Option<T>> {
    match result {
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

fn ensure_before_deadlines(
    idle_deadline: Option<Instant>,
    total_deadline: Option<Instant>,
    message: &str,
) -> std::io::Result<()> {
    let now = Instant::now();
    if idle_deadline.is_some_and(|deadline| now >= deadline)
        || total_deadline.is_some_and(|deadline| now >= deadline)
    {
        return Err(io::Error::new(io::ErrorKind::TimedOut, message));
    }
    Ok(())
}

fn sleep_until_poll(
    idle_deadline: Option<Instant>,
    total_deadline: Option<Instant>,
    poll_interval: Duration,
) {
    let now = Instant::now();
    let until_deadline = [idle_deadline, total_deadline]
        .into_iter()
        .flatten()
        .filter_map(|deadline| deadline.checked_duration_since(now))
        .min()
        .unwrap_or(poll_interval);
    std::thread::sleep(poll_interval.min(until_deadline));
}

fn read_should_retry(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
    )
}
