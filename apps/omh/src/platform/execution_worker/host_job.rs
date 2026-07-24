//! Host observation/command jobs and path probes.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc as std_mpsc, Arc};

use crate::execution_host::protocol::{
    CommandSpec, GitStatusSnapshot, PathCompletionEntry, PortSnapshot, ProjectCommandSnapshot,
    RequestId, RuntimeExitStatus, RuntimeIdentity, WorkerError, WorkerErrorCode, WorkerMessage,
};
use crate::execution_host::{HostPath, ResourceLocation};

// Local ops used only inside this module.
#[cfg(unix)]
use super::host_job_ops::{git_status_at, list_worktrees_at, observe_ports_at};
use super::protocol_io::write_message;
#[cfg(unix)]
use super::state::{remote_home, CreateKind, CreateRequest, WorkerState};
use super::util::{worker_error, HOST_JOB_TIMEOUT, MAX_PATH_COMPLETION_ENTRIES};

// pub use also brings names into this module's scope for spawn arms + siblings/tests.
#[cfg(unix)]
pub(super) use super::host_job_ops::{
    discover_project_commands_at, observe_runtime_process, run_command_at,
};
#[cfg(unix)]
pub(super) use super::state::HostJobKind;

#[cfg(unix)]
use std::os::unix::net::UnixStream;

#[cfg(unix)]
pub(super) enum HostJobOutcome {
    GitStatus(Result<GitStatusSnapshot, WorkerError>),
    Worktrees(Result<Vec<crate::execution_host::protocol::WorktreeSnapshot>, WorkerError>),
    Command(Result<(RuntimeExitStatus, Vec<u8>, Vec<u8>), WorkerError>),
    Ports(Result<Vec<PortSnapshot>, WorkerError>),
    ProjectCommands(Result<(ResourceLocation, Vec<ProjectCommandSnapshot>), WorkerError>),
    PathCompletion(Result<Vec<PathCompletionEntry>, WorkerError>),
    PathValidation(Result<(bool, bool), WorkerError>),
    /// Successful preflight yields the resolved launch location for on-loop commit.
    CreatePreflight(Result<ResourceLocation, WorkerError>),
}

#[cfg(unix)]
pub(super) struct HostJobResult {
    pub(super) request_id: RequestId,
    pub(super) outcome: HostJobOutcome,
}

#[cfg(unix)]
pub(super) fn spawn_create_job(
    state: &mut WorkerState,
    request_id: RequestId,
    request: CreateRequest,
    job_tx: &std_mpsc::Sender<HostJobResult>,
    stream: &mut UnixStream,
) -> io::Result<()> {
    let location = request.location.clone();
    spawn_host_job(
        state,
        request_id,
        location,
        HostJobKind::Create(request),
        None,
        job_tx,
        stream,
    )
}

#[cfg(unix)]
pub(super) fn spawn_host_job(
    state: &mut WorkerState,
    request_id: RequestId,
    location: ResourceLocation,
    kind: HostJobKind,
    command: Option<CommandSpec>,
    job_tx: &std_mpsc::Sender<HostJobResult>,
    stream: &mut UnixStream,
) -> io::Result<()> {
    // Create jobs validate location off-loop so a stalled filesystem cannot block PTY I/O.
    let cwd = if matches!(kind, HostJobKind::Create(_)) {
        PathBuf::new()
    } else {
        match state.validate_location(&location) {
            Ok(cwd) => cwd,
            Err(error) => return write_host_job_error(stream, &kind, request_id, location, error),
        }
    };
    if matches!(kind, HostJobKind::RunCommand) {
        let Some(command) = command.as_ref() else {
            return write_host_job_error(
                stream,
                &kind,
                request_id,
                location,
                worker_error(
                    WorkerErrorCode::Failed,
                    "run command is missing a command spec",
                ),
            );
        };
        if let Err(error) = command
            .validate()
            .map_err(|err| worker_error(WorkerErrorCode::Failed, err.to_string()))
        {
            return write_host_job_error(stream, &kind, request_id, location, error);
        }
    }

    let (cancel, finished) = match state.insert_host_job(request_id, kind.clone(), location.clone())
    {
        Ok(flags) => flags,
        Err(error) => {
            return write_host_job_error(stream, &kind, request_id, location, error);
        }
    };

    let job_tx = job_tx.clone();
    let host_id = state.binding().execution_host_id.clone();
    let create_request = match &kind {
        HostJobKind::Create(request) => Some(request.clone()),
        _ => None,
    };
    let path_prefix = match &kind {
        HostJobKind::CompletePath { prefix } => Some(prefix.clone()),
        _ => None,
    };
    std::thread::spawn(move || {
        let outcome = match kind {
            HostJobKind::GitStatus => {
                HostJobOutcome::GitStatus(git_status_at(&cwd, cancel.clone()))
            }
            HostJobKind::ListWorktrees => {
                HostJobOutcome::Worktrees(list_worktrees_at(&cwd, &host_id, cancel.clone()))
            }
            HostJobKind::RunCommand => {
                let command = command.expect("run command jobs carry a command spec");
                HostJobOutcome::Command(run_command_at(cwd, command, cancel.clone()))
            }
            HostJobKind::ObservePorts => {
                HostJobOutcome::Ports(observe_ports_at(&host_id, &location, cancel.clone()))
            }
            HostJobKind::DiscoverProjectCommands => HostJobOutcome::ProjectCommands(
                discover_project_commands_at(&host_id, &location, cancel.clone()),
            ),
            HostJobKind::CompletePath { .. } => {
                let prefix = path_prefix.expect("complete-path jobs carry a prefix");
                HostJobOutcome::PathCompletion(complete_path_at(&cwd, &prefix, cancel.clone()))
            }
            HostJobKind::ValidatePath => {
                HostJobOutcome::PathValidation(validate_path_at(&cwd, cancel.clone()))
            }
            HostJobKind::Create(_) => {
                // Create commit happens on the connection loop after preflight succeeds.
                let request = create_request.expect("create jobs carry a request");
                HostJobOutcome::CreatePreflight(preflight_create_location(&request, cancel.clone()))
            }
        };
        finished.store(true, Ordering::Relaxed);
        let _ = job_tx.send(HostJobResult {
            request_id,
            outcome,
        });
    });
    Ok(())
}

#[cfg(unix)]
pub(super) fn expire_host_jobs(state: &mut WorkerState, stream: &mut UnixStream) -> io::Result<()> {
    let timed_out = state.timed_out_host_jobs(HOST_JOB_TIMEOUT);
    for request_id in timed_out {
        let Some(timeout) = state.mark_host_job_timeout(request_id) else {
            continue;
        };
        write_host_job_error(
            stream,
            &timeout.kind,
            request_id,
            timeout.location,
            worker_error(
                WorkerErrorCode::TimedOut,
                format!(
                    "host job exceeded {} second limit",
                    HOST_JOB_TIMEOUT.as_secs()
                ),
            ),
        )?;
        if timeout.finished {
            state.remove_host_job(request_id);
        }
    }
    // Reap finished/responded slots that no longer need accounting.
    state.reap_completed_host_jobs();
    Ok(())
}

#[cfg(unix)]
pub(super) fn flush_host_job_results(
    state: &mut WorkerState,
    job_rx: &std_mpsc::Receiver<HostJobResult>,
    stream: &mut UnixStream,
) -> io::Result<()> {
    while let Ok(result) = job_rx.try_recv() {
        let Some(job) = state.host_job_snapshot(&result.request_id) else {
            continue;
        };
        if job.responded {
            // Timeout/disconnect already answered; only reap accounting.
            state.finish_host_job_after_response(result.request_id);
            continue;
        }

        match result.outcome {
            HostJobOutcome::CreatePreflight(Ok(resolved)) => {
                let HostJobKind::Create(request) = &job.kind else {
                    state.complete_host_job(result.request_id);
                    continue;
                };
                if job.cancelled {
                    write_host_job_error(
                        stream,
                        &HostJobKind::Create(request.clone()),
                        result.request_id,
                        resolved,
                        worker_error(
                            WorkerErrorCode::TimedOut,
                            "create preflight cancelled before spawn commit",
                        ),
                    )?;
                    state.complete_host_job(result.request_id);
                    continue;
                }
                let mut commit = request.clone();
                commit.location = resolved.clone();
                let commit_result = state.create_once(result.request_id, commit);
                write_create_result(stream, &job.kind, result.request_id, commit_result)?;
                state.complete_host_job(result.request_id);
            }
            HostJobOutcome::CreatePreflight(Err(error)) => {
                write_host_job_error(stream, &job.kind, result.request_id, job.location, error)?;
                state.complete_host_job(result.request_id);
            }
            outcome => {
                let response_location = match &outcome {
                    HostJobOutcome::ProjectCommands(Ok((resolved, _))) => resolved.clone(),
                    _ => job.location,
                };
                write_host_job_outcome(stream, result.request_id, response_location, outcome)?;
                state.complete_host_job(result.request_id);
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
pub(super) fn write_create_result(
    stream: &mut UnixStream,
    kind: &HostJobKind,
    request_id: RequestId,
    result: Result<(RuntimeIdentity, ResourceLocation), WorkerError>,
) -> io::Result<()> {
    match kind {
        HostJobKind::Create(request) => match &request.kind {
            CreateKind::Terminal => {
                let (identity, location, error) = match result {
                    Ok((identity, location)) => (Some(identity), location, None),
                    Err(err) => (None, request.location.clone(), Some(err)),
                };
                write_message(
                    stream,
                    WorkerMessage::CreateTerminalResult {
                        request_id,
                        identity,
                        location,
                        error,
                    },
                )
            }
            CreateKind::Agent(_) => {
                let (identity, location, error) = match result {
                    Ok((identity, location)) => (Some(identity), location, None),
                    Err(err) => (None, request.location.clone(), Some(err)),
                };
                write_message(
                    stream,
                    WorkerMessage::StartAgentResult {
                        request_id,
                        location,
                        identity,
                        error,
                    },
                )
            }
        },
        _ => Ok(()),
    }
}

#[cfg(unix)]
pub(super) fn write_host_job_outcome(
    stream: &mut UnixStream,
    request_id: RequestId,
    location: ResourceLocation,
    outcome: HostJobOutcome,
) -> io::Result<()> {
    match outcome {
        HostJobOutcome::GitStatus(result) => {
            let (status, error) =
                result.map_or_else(|err| (None, Some(err)), |status| (Some(status), None));
            write_message(
                stream,
                WorkerMessage::GitStatusResult {
                    request_id,
                    location,
                    status,
                    error,
                },
            )
        }
        HostJobOutcome::Worktrees(result) => {
            let (worktrees, error) =
                result.map_or_else(|err| (Vec::new(), Some(err)), |worktrees| (worktrees, None));
            write_message(
                stream,
                WorkerMessage::WorktreeListResult {
                    request_id,
                    location,
                    worktrees,
                    error,
                },
            )
        }
        HostJobOutcome::Command(result) => {
            let (exit, stdout, stderr, error) = result.map_or_else(
                |err| (None, Vec::new(), Vec::new(), Some(err)),
                |(exit, stdout, stderr)| (Some(exit), stdout, stderr, None),
            );
            write_message(
                stream,
                WorkerMessage::CommandResult {
                    request_id,
                    location,
                    exit,
                    stdout,
                    stderr,
                    error,
                },
            )
        }
        HostJobOutcome::Ports(result) => {
            let (ports, error) =
                result.map_or_else(|err| (Vec::new(), Some(err)), |ports| (ports, None));
            write_message(
                stream,
                WorkerMessage::PortObservationResult {
                    request_id,
                    location,
                    ports,
                    error,
                },
            )
        }
        HostJobOutcome::ProjectCommands(result) => {
            let (commands, error, resolved) = match result {
                Ok((resolved, commands)) => (commands, None, resolved),
                Err(err) => (Vec::new(), Some(err), location),
            };
            write_message(
                stream,
                WorkerMessage::ProjectCommandsResult {
                    request_id,
                    location: resolved,
                    commands,
                    error,
                },
            )
        }
        HostJobOutcome::PathCompletion(result) => {
            let (entries, error) =
                result.map_or_else(|err| (Vec::new(), Some(err)), |entries| (entries, None));
            write_message(
                stream,
                WorkerMessage::PathCompletion {
                    request_id,
                    location,
                    entries,
                    error,
                },
            )
        }
        HostJobOutcome::PathValidation(result) => {
            let (exists, is_dir, error) = match result {
                Ok((exists, is_dir)) => (exists, is_dir, None),
                Err(err) => (false, false, Some(err)),
            };
            write_message(
                stream,
                WorkerMessage::PathValidation {
                    request_id,
                    location,
                    exists,
                    is_dir,
                    error,
                },
            )
        }
        HostJobOutcome::CreatePreflight(_) => {
            // Create results are committed/written by flush_host_job_results.
            Ok(())
        }
    }
}

#[cfg(unix)]
pub(super) fn write_host_job_error(
    stream: &mut UnixStream,
    kind: &HostJobKind,
    request_id: RequestId,
    location: ResourceLocation,
    error: WorkerError,
) -> io::Result<()> {
    match kind {
        HostJobKind::GitStatus => write_message(
            stream,
            WorkerMessage::GitStatusResult {
                request_id,
                location,
                status: None,
                error: Some(error),
            },
        ),
        HostJobKind::ListWorktrees => write_message(
            stream,
            WorkerMessage::WorktreeListResult {
                request_id,
                location,
                worktrees: Vec::new(),
                error: Some(error),
            },
        ),
        HostJobKind::RunCommand => write_message(
            stream,
            WorkerMessage::CommandResult {
                request_id,
                location,
                exit: None,
                stdout: Vec::new(),
                stderr: Vec::new(),
                error: Some(error),
            },
        ),
        HostJobKind::ObservePorts => write_message(
            stream,
            WorkerMessage::PortObservationResult {
                request_id,
                location,
                ports: Vec::new(),
                error: Some(error),
            },
        ),
        HostJobKind::DiscoverProjectCommands => write_message(
            stream,
            WorkerMessage::ProjectCommandsResult {
                request_id,
                location,
                commands: Vec::new(),
                error: Some(error),
            },
        ),
        HostJobKind::CompletePath { .. } => write_message(
            stream,
            WorkerMessage::PathCompletion {
                request_id,
                location,
                entries: Vec::new(),
                error: Some(error),
            },
        ),
        HostJobKind::ValidatePath => write_message(
            stream,
            WorkerMessage::PathValidation {
                request_id,
                location,
                exists: false,
                is_dir: false,
                error: Some(error),
            },
        ),
        HostJobKind::Create(request) => {
            let message = match request.kind {
                CreateKind::Terminal => WorkerMessage::CreateTerminalResult {
                    request_id,
                    identity: None,
                    location,
                    error: Some(error),
                },
                CreateKind::Agent(_) => WorkerMessage::StartAgentResult {
                    request_id,
                    location,
                    identity: None,
                    error: Some(error),
                },
            };
            write_message(stream, message)
        }
    }
}

#[cfg(unix)]
pub(super) fn complete_path_at(
    location_path: &Path,
    prefix: &str,
    cancel: Arc<AtomicBool>,
) -> Result<Vec<PathCompletionEntry>, WorkerError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(worker_error(
            WorkerErrorCode::TimedOut,
            "path completion cancelled before start",
        ));
    }
    let prefix_path = Path::new(prefix);
    let resolved_prefix = if prefix_path.is_absolute() {
        prefix_path.to_path_buf()
    } else {
        location_path.join(prefix_path)
    };
    let parent = resolved_prefix
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name_prefix = resolved_prefix
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let mut entries = Vec::new();
    let read_dir = std::fs::read_dir(parent)
        .map_err(|err| worker_error(WorkerErrorCode::Failed, err.to_string()))?;
    for entry in read_dir.flatten() {
        if cancel.load(Ordering::Relaxed) {
            return Err(worker_error(
                WorkerErrorCode::TimedOut,
                "path completion cancelled",
            ));
        }
        if entries.len() >= MAX_PATH_COMPLETION_ENTRIES {
            break;
        }
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with(name_prefix) {
            continue;
        }
        let path = entry.path();
        let Ok(host_path) = HostPath::new(path.clone()) else {
            continue;
        };
        entries.push(PathCompletionEntry {
            path: host_path,
            is_dir: path.is_dir(),
        });
    }
    entries.sort_by(|left, right| left.path.as_path().cmp(right.path.as_path()));
    Ok(entries)
}

#[cfg(unix)]
pub(super) fn validate_path_at(
    path: &Path,
    cancel: Arc<AtomicBool>,
) -> Result<(bool, bool), WorkerError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(worker_error(
            WorkerErrorCode::TimedOut,
            "path validation cancelled before start",
        ));
    }
    match std::fs::metadata(path) {
        Ok(metadata) => Ok((true, metadata.is_dir())),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok((false, false)),
        Err(err) => Err(worker_error(WorkerErrorCode::Failed, err.to_string())),
    }
}

#[cfg(unix)]
pub(super) fn preflight_create_location(
    request: &CreateRequest,
    cancel: Arc<AtomicBool>,
) -> Result<ResourceLocation, WorkerError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(worker_error(
            WorkerErrorCode::TimedOut,
            "create preflight cancelled before start",
        ));
    }
    // Resolve tilde and validate directory metadata off the PTY loop.
    let path = request.location.path.as_path();
    let resolved = if path == Path::new("~") {
        remote_home()?
    } else if let Ok(suffix) = path.strip_prefix(Path::new("~")) {
        if suffix.as_os_str().is_empty() {
            remote_home()?
        } else {
            remote_home()?.join(suffix)
        }
    } else if path
        .components()
        .next()
        .is_some_and(|component| component.as_os_str().to_string_lossy().starts_with('~'))
    {
        return Err(worker_error(
            WorkerErrorCode::InvalidLocation,
            "named-user tilde expansion is not supported",
        ));
    } else {
        path.to_path_buf()
    };
    if cancel.load(Ordering::Relaxed) {
        return Err(worker_error(
            WorkerErrorCode::TimedOut,
            "create preflight cancelled during resolve",
        ));
    }
    let metadata = std::fs::metadata(&resolved)
        .map_err(|error| worker_error(WorkerErrorCode::InvalidLocation, error.to_string()))?;
    if !metadata.is_dir() {
        return Err(worker_error(
            WorkerErrorCode::InvalidLocation,
            "terminal launch location is not a directory",
        ));
    }
    if let Some(command) = &request.command {
        command
            .validate()
            .map_err(|err| worker_error(WorkerErrorCode::Failed, err.to_string()))?;
    }
    let path = HostPath::new(resolved)
        .map_err(|error| worker_error(WorkerErrorCode::InvalidLocation, error.to_string()))?;
    Ok(ResourceLocation::new(
        request.location.execution_host_id.clone(),
        path,
    ))
}
