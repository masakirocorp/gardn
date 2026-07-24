use std::collections::HashMap;
use std::hash::Hash;
use std::time::Instant;

use super::operations::{HostObservation, ObservationBegin, OBSERVATION_REQUEST_TIMEOUT};
use super::protocol::{RequestId, WorkerError, WorkerErrorCode};
use super::{ExecutionHostId, ResourceLocation};

/// Coalescing observation cache with reverse request indexing.
///
/// Production request/complete/expire paths and unit tests share this broker so
/// observation transitions are not duplicated between registry and cfg(test)
/// helpers.
#[derive(Clone, Debug)]
pub(crate) struct ObservationBroker<K, T> {
    slots: HashMap<K, HostObservation<T>>,
    pending: HashMap<(ExecutionHostId, RequestId), K>,
}

impl<K, T> ObservationBroker<K, T> {
    pub(crate) fn new() -> Self {
        Self {
            slots: HashMap::new(),
            pending: HashMap::new(),
        }
    }
}

impl<K, T> Default for ObservationBroker<K, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K, T> ObservationBroker<K, T>
where
    K: Clone + Eq + Hash,
{
    pub(crate) fn get(&self, key: &K) -> Option<&HostObservation<T>> {
        self.slots.get(key)
    }

    #[cfg(test)]
    pub(crate) fn get_mut(&mut self, key: &K) -> Option<&mut HostObservation<T>> {
        self.slots.get_mut(key)
    }

    pub(crate) fn inflight(&self, key: &K) -> Option<RequestId> {
        self.slots
            .get(key)
            .and_then(HostObservation::pending_request_id)
    }

    /// Begin tracking `request_id` for `key`, coalescing when already pending.
    ///
    /// Returns the request id that should be considered in flight (existing or
    /// newly started). Callers that already sent a worker message should only
    /// call this after send when [`Self::inflight`] was previously `None`.
    pub(crate) fn track(
        &mut self,
        key: K,
        host_id: ExecutionHostId,
        request_id: RequestId,
        now: Instant,
    ) -> ObservationBegin {
        let slot = self.slots.entry(key.clone()).or_insert_with(|| {
            // Placeholder replaced immediately by begin_or_coalesce.
            HostObservation::Failed {
                error: WorkerError::new(WorkerErrorCode::Failed, "observation placeholder"),
                request_id: RequestId::new(0),
                failed_at: now,
                previous: None,
            }
        });
        let begin = slot.begin_or_coalesce(request_id, now);
        match begin {
            ObservationBegin::Started { request_id } => {
                self.pending.insert((host_id, request_id), key);
            }
            ObservationBegin::Coalesced { .. } => {}
        }
        begin
    }

    /// Record a newly started request that the caller has already dispatched.
    pub(crate) fn track_started(
        &mut self,
        key: K,
        host_id: ExecutionHostId,
        request_id: RequestId,
        now: Instant,
    ) -> RequestId {
        match self.track(key, host_id, request_id, now) {
            ObservationBegin::Started { request_id }
            | ObservationBegin::Coalesced { request_id } => request_id,
        }
    }

    /// Take the key registered for an in-flight `(host, request)` pair.
    pub(crate) fn take_pending(
        &mut self,
        host_id: &ExecutionHostId,
        request_id: RequestId,
    ) -> Option<K> {
        self.pending.remove(&(host_id.clone(), request_id))
    }

    /// Publish a fresh snapshot under an additional key (e.g. related path).
    pub(crate) fn insert_fresh(
        &mut self,
        key: K,
        request_id: RequestId,
        value: T,
        observed_at: Instant,
    ) {
        self.slots.insert(
            key,
            HostObservation::Fresh {
                value,
                request_id,
                observed_at,
            },
        );
    }

    pub(crate) fn expire_pending(&mut self, now: Instant) {
        for observation in self.slots.values_mut() {
            let _ = observation.expire_pending(now, OBSERVATION_REQUEST_TIMEOUT);
        }
        self.pending.retain(|(_, request_id), key| {
            self.slots
                .get(key)
                .and_then(HostObservation::pending_request_id)
                == Some(*request_id)
        });
    }

    pub(crate) fn mark_stale_where(&mut self, mut belongs: impl FnMut(&K) -> bool) {
        for (key, observation) in &mut self.slots {
            if belongs(key) {
                observation.mark_stale();
            }
        }
    }

    /// Drop reverse-index entries for a disconnected host and keys matching `drop_key`.
    pub(crate) fn drop_pending_for_host(
        &mut self,
        host_id: &ExecutionHostId,
        mut drop_key: impl FnMut(&K) -> bool,
    ) {
        self.pending
            .retain(|(pending_host, _), key| pending_host != host_id && !drop_key(key));
    }

    #[cfg(test)]
    pub(crate) fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Complete an observation whose pending key was already taken by the caller.
    pub(crate) fn complete_keyed(
        &mut self,
        key: &K,
        request_id: RequestId,
        response_matches: bool,
        value: Option<T>,
        error: Option<WorkerError>,
        mismatch_message: &str,
    ) {
        let Some(slot) = self.slots.get_mut(key) else {
            return;
        };
        if slot.pending_request_id() != Some(request_id) {
            return;
        }
        let now = Instant::now();
        match (response_matches, value, error) {
            (true, Some(value), None) => {
                let _ = slot.complete_success(request_id, value, now);
            }
            (_, _, Some(error)) => {
                let _ = slot.complete_failure(request_id, error, now);
            }
            _ => {
                let _ = slot.complete_failure(
                    request_id,
                    WorkerError::new(WorkerErrorCode::Failed, mismatch_message),
                    now,
                );
            }
        }
    }
}

impl<T> ObservationBroker<ResourceLocation, T> {
    /// Complete a location-keyed observation when the response location matches.
    pub(crate) fn complete_location(
        &mut self,
        host_id: ExecutionHostId,
        request_id: RequestId,
        location: ResourceLocation,
        value: Option<T>,
        error: Option<WorkerError>,
        kind: &str,
    ) {
        let Some(expected_location) = self.take_pending(&host_id, request_id) else {
            return;
        };
        let Some(slot) = self.slots.get_mut(&expected_location) else {
            return;
        };
        if slot.pending_request_id() != Some(request_id) {
            return;
        }
        let response_matches_request =
            location == expected_location && location.execution_host_id == host_id;
        let now = Instant::now();
        match (response_matches_request, value, error) {
            (true, Some(value), None) => {
                let _ = slot.complete_success(request_id, value, now);
            }
            (_, _, Some(error)) => {
                let _ = slot.complete_failure(request_id, error, now);
            }
            _ => {
                let _ = slot.complete_failure(
                    request_id,
                    WorkerError::new(
                        WorkerErrorCode::Failed,
                        format!("{kind} response did not match the pending request"),
                    ),
                    now,
                );
            }
        }
    }

    pub(crate) fn mark_host_locations_stale(&mut self, host_id: &ExecutionHostId) {
        self.mark_stale_where(|location| &location.execution_host_id == host_id);
        self.drop_pending_for_host(host_id, |location| &location.execution_host_id == host_id);
    }
}

impl ObservationBroker<ResourceLocation, Vec<super::protocol::ProjectCommandSnapshot>> {
    /// Complete project-command discovery, accepting same-host related paths.
    pub(crate) fn complete_project_commands(
        &mut self,
        host_id: ExecutionHostId,
        request_id: RequestId,
        location: ResourceLocation,
        commands: Vec<super::protocol::ProjectCommandSnapshot>,
        error: Option<WorkerError>,
    ) {
        let Some(expected_location) = self.take_pending(&host_id, request_id) else {
            return;
        };
        let Some(slot) = self.slots.get_mut(&expected_location) else {
            return;
        };
        if slot.pending_request_id() != Some(request_id) {
            return;
        }
        let now = Instant::now();
        let host_ok = location.execution_host_id == host_id
            && commands
                .iter()
                .all(|command| command.location.execution_host_id == host_id);
        // Worker may root-qualify nested cwd discovery; accept same-host related paths.
        let related = location == expected_location
            || expected_location
                .path
                .as_path()
                .starts_with(location.path.as_path())
            || location
                .path
                .as_path()
                .starts_with(expected_location.path.as_path());
        match (host_ok && related, error) {
            (true, None) => {
                let _ = slot.complete_success(request_id, commands.clone(), now);
                // Also publish under the worker-qualified root so catalog lookups
                // by either nested cwd or project root see the same discovery.
                if location != expected_location {
                    self.insert_fresh(location, request_id, commands, now);
                }
            }
            (_, Some(error)) => {
                let _ = slot.complete_failure(request_id, error, now);
            }
            _ => {
                let _ = slot.complete_failure(
                    request_id,
                    WorkerError::new(
                        WorkerErrorCode::Failed,
                        "project commands response did not match the pending request",
                    ),
                    now,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_host::HostPath;

    #[test]
    fn broker_coalesces_inflight_without_second_pending_entry() {
        let mut broker = ObservationBroker::<u32, u32>::new();
        let host = ExecutionHostId::new("ssh:workbox:1").unwrap();
        let now = Instant::now();
        let first = broker.track(7, host.clone(), RequestId::new(1), now);
        assert_eq!(
            first,
            ObservationBegin::Started {
                request_id: RequestId::new(1)
            }
        );
        let second = broker.track(7, host, RequestId::new(2), now);
        assert_eq!(
            second,
            ObservationBegin::Coalesced {
                request_id: RequestId::new(1)
            }
        );
        assert_eq!(broker.pending_len(), 1);
        assert_eq!(broker.inflight(&7), Some(RequestId::new(1)));
    }

    #[test]
    fn location_completion_rejects_mismatched_host_path() {
        let mut broker = ObservationBroker::<ResourceLocation, u32>::new();
        let host = ExecutionHostId::new("ssh:workbox:1").unwrap();
        let expected = ResourceLocation::new(host.clone(), HostPath::new("/srv/work").unwrap());
        let other = ResourceLocation::new(host.clone(), HostPath::new("/srv/other").unwrap());
        let request_id = RequestId::new(3);
        broker.track_started(expected.clone(), host.clone(), request_id, Instant::now());
        broker.complete_location(host, request_id, other, Some(9), None, "git status");
        assert!(matches!(
            broker.get(&expected),
            Some(HostObservation::Failed { .. })
        ));
    }
}
