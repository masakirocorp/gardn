use super::storage::{atomic_write, create_private_dir, error, is_terminal};
use gardn_local_api::sprites::{
    SpriteError, SpriteOperation, SpriteRecord, SpriteSnapshot, SpritesConfig,
};
use sha2::{Digest, Sha256};
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};
use tokio::sync::mpsc::Sender;

const TIMEOUT: Duration = Duration::from_secs(600);
const OUTPUT_LIMIT: u64 = 16 * 1024 * 1024;
const ERROR_LIMIT: u64 = 64 * 1024;
const ASSETS: &[(&str, &[u8])] = &[
    ("worker.mjs", include_bytes!("assets/worker.mjs")),
    ("connect.mjs", include_bytes!("assets/connect.mjs")),
    ("provider.mjs", include_bytes!("assets/provider.mjs")),
    ("store.mjs", include_bytes!("assets/store.mjs")),
    ("transfer.mjs", include_bytes!("assets/transfer.mjs")),
    ("auth.mjs", include_bytes!("assets/auth.mjs")),
    ("LICENSE", include_bytes!("assets/LICENSE")),
];

pub(super) enum Event {
    Progress(SpriteOperation),
    Snapshot(SpriteSnapshot),
    Resources(Vec<SpriteRecord>),
    Finished(Result<SpriteOperation, SpriteError>),
}
pub(super) struct Message {
    pub id: String,
    pub epoch: u64,
    pub event: Event,
}
#[derive(Clone)]
pub(super) struct Reporter {
    pub id: String,
    pub epoch: u64,
    pub tx: mpsc::Sender<Message>,
    pub waker: Option<Sender<crate::events::AppEvent>>,
}
impl Reporter {
    pub fn send(&self, event: Event) {
        if self
            .tx
            .send(Message {
                id: self.id.clone(),
                epoch: self.epoch,
                event,
            })
            .is_ok()
        {
            if let Some(waker) = &self.waker {
                let _ = waker.try_send(crate::events::AppEvent::SpritesUpdated);
            }
        }
    }
}

struct WorkerChild(Child);
impl Drop for WorkerChild {
    fn drop(&mut self) {
        crate::platform::terminate_cancellable_child(&mut self.0);
    }
}

pub(super) fn run(
    root: PathBuf,
    config: SpritesConfig,
    operation: SpriteOperation,
    canceled: Arc<AtomicBool>,
    reporter: Reporter,
) {
    reporter.send(Event::Finished(invoke(
        &root, &config, &operation, &canceled, &reporter,
    )));
}
fn invoke(
    root: &Path,
    config: &SpritesConfig,
    operation: &SpriteOperation,
    canceled: &AtomicBool,
    reporter: &Reporter,
) -> Result<SpriteOperation, SpriteError> {
    if canceled.load(Ordering::Acquire) {
        return Err(error(
            "canceled",
            "Sprites operation canceled before startup",
            true,
        ));
    }
    let assets = extract_assets(root)?;
    let input = serde_json::to_vec(&serde_json::json!({"config":config,"state_dir":root,"operation":operation,"owner_pid":std::process::id()}))
        .map_err(|cause| error("worker_input", format!("Could not encode Sprites request: {cause}"), false))?;
    let mut command = crate::noninteractive_process::command(&config.node_bin);
    command
        .arg(assets.join("worker.mjs"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::platform::configure_cancellable_command(&mut command);
    let mut child = WorkerChild(command.spawn().map_err(|cause| {
        error(
            "worker_spawn",
            format!("Could not start Sprites worker: {cause}"),
            true,
        )
    })?);
    let mut stdin = child
        .0
        .stdin
        .take()
        .ok_or_else(|| error("worker_input", "Sprites worker stdin is unavailable", true))?;
    stdin.write_all(&input).map_err(|cause| {
        error(
            "worker_input",
            format!("Could not send Sprites request: {cause}"),
            true,
        )
    })?;
    drop(stdin);
    let stdout = child.0.stdout.take().ok_or_else(|| {
        error(
            "worker_protocol",
            "Sprites worker stdout is unavailable",
            true,
        )
    })?;
    let stderr = child.0.stderr.take().ok_or_else(|| {
        error(
            "worker_protocol",
            "Sprites worker stderr is unavailable",
            true,
        )
    })?;
    let (output_tx, output_rx) = mpsc::channel();
    let expected = operation.clone();
    let progress = reporter.clone();
    let stdout_reader = thread::spawn(move || {
        let _ = output_tx.send(read_frames(stdout, &expected, &progress));
    });
    let (error_tx, error_rx) = mpsc::channel();
    let stderr_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stderr
            .take(ERROR_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|cause| {
                error(
                    "worker_output",
                    format!("Could not read Sprites worker diagnostics: {cause}"),
                    true,
                )
            })
            .and_then(|_| {
                if bytes.len() as u64 > ERROR_LIMIT {
                    Err(output_limit())
                } else {
                    Ok(())
                }
            });
        let _ = error_tx.send(result);
    });
    let deadline = Instant::now() + TIMEOUT;
    let result = (|| {
        let mut status = None;
        let mut output = None;
        let mut stderr_done = false;
        loop {
            if canceled.load(Ordering::Acquire) {
                return Err(error(
                    "canceled",
                    "Sprites operation canceled locally; remote state may require reconciliation",
                    true,
                ));
            }
            if Instant::now() >= deadline {
                return Err(error(
                    "worker_timeout",
                    "Sprites worker exceeded its 10 minute execution limit",
                    true,
                ));
            }
            if output.is_none() {
                match output_rx.try_recv() {
                    Ok(result) => output = Some(result?),
                    Err(mpsc::TryRecvError::Disconnected) => {
                        return Err(error(
                            "worker_protocol",
                            "Sprites output reader stopped unexpectedly",
                            true,
                        ))
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
            }
            if !stderr_done {
                match error_rx.try_recv() {
                    Ok(result) => {
                        result?;
                        stderr_done = true;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        return Err(error(
                            "worker_protocol",
                            "Sprites diagnostics reader stopped unexpectedly",
                            true,
                        ))
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
            }
            if status.is_none() {
                status = child.0.try_wait().map_err(|cause| {
                    error(
                        "worker_wait",
                        format!("Could not inspect Sprites worker: {cause}"),
                        true,
                    )
                })?;
            }
            if let Some(status) = status {
                if stderr_done {
                    if let Some(operation) = output.take() {
                        if !status.success() && operation.error.is_none() {
                            return Err(error("worker_exit", format!("Sprites worker exited with {status} without a structured failure"), true));
                        }
                        return Ok(operation);
                    }
                }
            }
            thread::sleep(Duration::from_millis(20));
        }
    })();
    drop(child);
    let _ = stdout_reader.join();
    let _ = stderr_reader.join();
    result
}

fn read_frames(
    reader: impl Read,
    expected: &SpriteOperation,
    reporter: &Reporter,
) -> Result<SpriteOperation, SpriteError> {
    let mut reader = BufReader::new(reader.take(OUTPUT_LIMIT + 1));
    let mut bytes = Vec::new();
    let mut count = 0;
    let mut terminal = None;
    loop {
        bytes.clear();
        let length = reader.read_until(b'\n', &mut bytes).map_err(|cause| {
            error(
                "worker_protocol",
                format!("Could not read Sprites progress: {cause}"),
                true,
            )
        })?;
        if length == 0 {
            break;
        }
        count += length as u64;
        if count > OUTPUT_LIMIT {
            return Err(output_limit());
        }
        if bytes.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let frame: serde_json::Value = serde_json::from_slice(&bytes).map_err(|cause| {
            error(
                "worker_protocol",
                format!("Invalid Sprites progress frame: {cause}"),
                true,
            )
        })?;
        match frame.get("type").and_then(serde_json::Value::as_str) {
            Some("operation") => {
                let operation: SpriteOperation =
                    serde_json::from_value(frame.get("operation").cloned().unwrap_or_default())
                        .map_err(|cause| {
                            error(
                                "worker_protocol",
                                format!("Invalid Sprites operation frame: {cause}"),
                                true,
                            )
                        })?;
                if operation.id != expected.id || operation.request != expected.request {
                    return Err(error(
                        "worker_protocol",
                        "Sprites worker changed the operation identity or request",
                        false,
                    ));
                }
                if terminal.is_some() {
                    return Err(error(
                        "worker_protocol",
                        "Sprites worker emitted progress after completion",
                        false,
                    ));
                }
                if is_terminal(&operation.status) {
                    terminal = Some(operation);
                } else {
                    reporter.send(Event::Progress(operation));
                }
            }
            Some("snapshot") => {
                let snapshot =
                    serde_json::from_value(frame.get("snapshot").cloned().unwrap_or_default())
                        .map_err(|cause| {
                            error(
                                "worker_protocol",
                                format!("Invalid Sprites inventory frame: {cause}"),
                                true,
                            )
                        })?;
                reporter.send(Event::Snapshot(snapshot));
            }
            _ => {
                return Err(error(
                    "worker_protocol",
                    "Unknown Sprites progress frame",
                    false,
                ))
            }
        }
    }
    terminal.ok_or_else(|| {
        error(
            "worker_protocol",
            "Sprites worker exited without a final operation",
            true,
        )
    })
}
fn output_limit() -> SpriteError {
    error(
        "worker_output_limit",
        "Sprites worker output exceeded its safety limit",
        true,
    )
}

fn extract_assets(root: &Path) -> Result<PathBuf, SpriteError> {
    let mut digest = Sha256::new();
    for (name, bytes) in ASSETS {
        digest.update(name.as_bytes());
        digest.update(bytes);
    }
    let directory = root
        .join("assets")
        .join(crate::checksum::to_lower_hex(&digest.finalize()));
    create_private_dir(&directory)?;
    for (name, bytes) in ASSETS {
        let path = directory.join(name);
        if !path.exists() {
            atomic_write(&path, bytes)?;
        }
    }
    Ok(directory)
}
