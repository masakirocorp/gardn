use std::collections::HashMap;

use super::protocol::{RequestId, WorkerError, WorkerErrorCode};
use super::{ExecutionHostId, HostPath, ResourceLocation};

/// In-flight stage-file request correlation owned outside the host manager core.
#[derive(Default)]
pub(crate) struct StageRequestTracker {
    pending: HashMap<(ExecutionHostId, RequestId), ResourceLocation>,
}

impl StageRequestTracker {
    pub(crate) fn new() -> Self {
        Self {
            pending: HashMap::new(),
        }
    }

    pub(crate) fn track(
        &mut self,
        host_id: ExecutionHostId,
        request_id: RequestId,
        location: ResourceLocation,
    ) {
        self.pending.insert((host_id, request_id), location);
    }

    /// Accept a stage-file result only when host + location match the pending request.
    pub(crate) fn complete(
        &mut self,
        host_id: &ExecutionHostId,
        request_id: RequestId,
        location: &ResourceLocation,
        path: Option<HostPath>,
        error: Option<WorkerError>,
    ) -> Option<StageFileCompletion> {
        let pending_key = (host_id.clone(), request_id);
        let expected_location = self.pending.get(&pending_key)?;
        if location != expected_location || &location.execution_host_id != host_id {
            return None;
        }
        let expected_location = self.pending.remove(&pending_key)?;
        let result = match (path, error) {
            (Some(path), None) => Ok(path),
            (_, Some(error)) => Err(error),
            (None, None) => Err(WorkerError::new(
                WorkerErrorCode::Failed,
                "execution worker returned no staged file path",
            )),
        };
        Some(StageFileCompletion {
            host_id: host_id.clone(),
            request_id,
            location: expected_location,
            result,
        })
    }

    /// Fail every in-flight stage for a disconnected host.
    pub(crate) fn fail_host(&mut self, host_id: &ExecutionHostId) -> Vec<StageFileCompletion> {
        let stale = self
            .pending
            .iter()
            .filter(|((pending_host, _), _)| pending_host == host_id)
            .map(|((pending_host, request_id), location)| {
                (pending_host.clone(), *request_id, location.clone())
            })
            .collect::<Vec<_>>();
        let mut completions = Vec::with_capacity(stale.len());
        for (pending_host, request_id, location) in stale {
            self.pending.remove(&(pending_host.clone(), request_id));
            completions.push(StageFileCompletion {
                host_id: pending_host,
                request_id,
                location,
                result: Err(WorkerError::new(
                    WorkerErrorCode::Gone,
                    "execution host disconnected before stage file completed",
                )),
            });
        }
        completions
    }
}

#[derive(Debug)]
pub(crate) struct StageFileCompletion {
    pub(crate) host_id: ExecutionHostId,
    pub(crate) request_id: RequestId,
    pub(crate) location: ResourceLocation,
    pub(crate) result: Result<HostPath, WorkerError>,
}

#[cfg(test)]
impl StageRequestTracker {
    pub(crate) fn pending_is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub(crate) fn has_pending(&self, host_id: &ExecutionHostId, request_id: RequestId) -> bool {
        self.pending.contains_key(&(host_id.clone(), request_id))
    }

    pub(crate) fn insert_for_test(
        &mut self,
        host_id: ExecutionHostId,
        request_id: RequestId,
        location: ResourceLocation,
    ) {
        self.pending.insert((host_id, request_id), location);
    }
}
