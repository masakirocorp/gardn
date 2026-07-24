use std::fmt;
use std::path::Path;
use std::time::{Duration, Instant};

use super::protocol::{PortSnapshot, PortTransport, RequestId, WorkerCapability, WorkerError};
use super::{ExecutionHostId, ResourceLocation};

/// How long an in-flight observation may remain pending before it is failed.
pub(crate) const OBSERVATION_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Typed observation cache entry for one host-qualified resource.
///
/// Pending and Failed preserve prior values when available. Fresh and Stale
/// retain the last successful snapshot; they never collapse into a bare
/// unknown state that discards typed failure/pending identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HostObservation<T> {
    Pending {
        request_id: RequestId,
        requested_at: Instant,
        previous: Option<T>,
    },
    Fresh {
        value: T,
        request_id: RequestId,
        observed_at: Instant,
    },
    Stale {
        value: T,
        request_id: RequestId,
        observed_at: Instant,
    },
    Failed {
        error: WorkerError,
        request_id: RequestId,
        failed_at: Instant,
        previous: Option<T>,
    },
}

/// Consumed observation view for API/runtime callers.
///
/// Maps the internal cache entry onto a four-state status so production code
/// can distinguish Pending, Ready (fresh), Stale, and Failed without matching
/// the full cache enum or collapsing failures into "missing".
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ObservationStatus<T> {
    Pending,
    Ready(T),
    Stale(T),
    Failed(WorkerError),
}

#[cfg(test)]
impl<T> ObservationStatus<T> {
    /// Fresh snapshot only — stale/pending/failed yield `None`.
    pub(crate) fn current(&self) -> Option<&T> {
        match self {
            Self::Ready(value) => Some(value),
            Self::Pending | Self::Stale(_) | Self::Failed(_) => None,
        }
    }

    /// Best available successful value, including stale snapshots.
    pub(crate) fn latest(&self) -> Option<&T> {
        match self {
            Self::Ready(value) | Self::Stale(value) => Some(value),
            Self::Pending | Self::Failed(_) => None,
        }
    }
}

impl<T> HostObservation<T> {
    #[cfg(test)]
    pub(crate) fn current(&self) -> Option<&T> {
        match self {
            Self::Fresh { value, .. } => Some(value),
            Self::Stale { .. } | Self::Pending { .. } | Self::Failed { .. } => None,
        }
    }

    /// Best available successful value, including stale or pending previous.
    #[cfg(test)]
    pub(crate) fn latest(&self) -> Option<&T> {
        match self {
            Self::Fresh { value, .. } | Self::Stale { value, .. } => Some(value),
            Self::Pending { previous, .. } | Self::Failed { previous, .. } => previous.as_ref(),
        }
    }

    /// Last known successful value, including stale or retained previous snapshots.
    #[cfg(test)]
    pub(crate) fn value(&self) -> Option<&T> {
        match self {
            Self::Fresh { value, .. } | Self::Stale { value, .. } => Some(value),
            Self::Pending { previous, .. } | Self::Failed { previous, .. } => previous.as_ref(),
        }
    }

    pub(crate) fn pending_request_id(&self) -> Option<RequestId> {
        match self {
            Self::Pending { request_id, .. } => Some(*request_id),
            _ => None,
        }
    }

    /// Project the cache entry into the consumed four-state status.
    pub(crate) fn status(&self) -> ObservationStatus<&T> {
        match self {
            Self::Pending { .. } => ObservationStatus::Pending,
            Self::Fresh { value, .. } => ObservationStatus::Ready(value),
            Self::Stale { value, .. } => ObservationStatus::Stale(value),
            Self::Failed { error, .. } => ObservationStatus::Failed(error.clone()),
        }
    }

    /// Owned projection used by API callers that need to keep the snapshot.
    pub(crate) fn to_status(&self) -> ObservationStatus<T>
    where
        T: Clone,
    {
        match self {
            Self::Pending { .. } => ObservationStatus::Pending,
            Self::Fresh { value, .. } => ObservationStatus::Ready(value.clone()),
            Self::Stale { value, .. } => ObservationStatus::Stale(value.clone()),
            Self::Failed { error, .. } => ObservationStatus::Failed(error.clone()),
        }
    }

    pub(crate) fn mark_stale(&mut self) {
        match std::mem::replace(
            self,
            Self::Failed {
                error: WorkerError::new(
                    super::protocol::WorkerErrorCode::Failed,
                    "observation placeholder",
                ),
                request_id: RequestId::new(0),
                failed_at: Instant::now(),
                previous: None,
            },
        ) {
            Self::Fresh {
                value,
                request_id,
                observed_at,
            }
            | Self::Stale {
                value,
                request_id,
                observed_at,
            } => {
                *self = Self::Stale {
                    value,
                    request_id,
                    observed_at,
                };
            }
            Self::Pending {
                request_id,
                requested_at: _,
                previous,
            } => {
                if let Some(value) = previous {
                    *self = Self::Stale {
                        value,
                        request_id,
                        observed_at: Instant::now(),
                    };
                } else {
                    *self = Self::Failed {
                        error: WorkerError::new(
                            super::protocol::WorkerErrorCode::Failed,
                            "execution host observation interrupted",
                        ),
                        request_id,
                        failed_at: Instant::now(),
                        previous: None,
                    };
                }
            }
            Self::Failed {
                error,
                request_id,
                failed_at,
                previous,
            } => {
                *self = Self::Failed {
                    error,
                    request_id,
                    failed_at,
                    previous,
                };
            }
        }
    }

    /// Begin a new in-flight request. Returns the existing request id when one
    /// is already pending so callers can coalesce without a second worker send.
    pub(crate) fn begin_or_coalesce(
        &mut self,
        request_id: RequestId,
        requested_at: Instant,
    ) -> ObservationBegin {
        if let Self::Pending {
            request_id: inflight,
            ..
        } = self
        {
            return ObservationBegin::Coalesced {
                request_id: *inflight,
            };
        }
        let previous = match std::mem::replace(
            self,
            Self::Pending {
                request_id,
                requested_at,
                previous: None,
            },
        ) {
            Self::Fresh { value, .. } | Self::Stale { value, .. } => Some(value),
            Self::Failed { previous, .. } => previous,
            Self::Pending { previous, .. } => previous,
        };
        *self = Self::Pending {
            request_id,
            requested_at,
            previous,
        };
        ObservationBegin::Started { request_id }
    }

    /// Apply a successful response when it matches the in-flight request id.
    pub(crate) fn complete_success(
        &mut self,
        request_id: RequestId,
        value: T,
        observed_at: Instant,
    ) -> bool {
        match self {
            Self::Pending {
                request_id: pending,
                ..
            } if *pending == request_id => {
                *self = Self::Fresh {
                    value,
                    request_id,
                    observed_at,
                };
                true
            }
            _ => false,
        }
    }

    /// Apply a typed failure when it matches the in-flight request id.
    pub(crate) fn complete_failure(
        &mut self,
        request_id: RequestId,
        error: WorkerError,
        failed_at: Instant,
    ) -> bool {
        match std::mem::replace(
            self,
            Self::Failed {
                error: error.clone(),
                request_id,
                failed_at,
                previous: None,
            },
        ) {
            Self::Pending {
                request_id: pending,
                previous,
                ..
            } if pending == request_id => {
                *self = Self::Failed {
                    error,
                    request_id,
                    failed_at,
                    previous,
                };
                true
            }
            other => {
                *self = other;
                false
            }
        }
    }

    /// Expire a pending request that has exceeded `timeout`.
    pub(crate) fn expire_pending(&mut self, now: Instant, timeout: Duration) -> bool {
        let Self::Pending {
            request_id,
            requested_at,
            previous,
        } = self
        else {
            return false;
        };
        if now.saturating_duration_since(*requested_at) < timeout {
            return false;
        }
        let request_id = *request_id;
        let previous = previous.take();
        *self = Self::Failed {
            error: WorkerError::new(
                super::protocol::WorkerErrorCode::TimedOut,
                "execution host observation timed out",
            ),
            request_id,
            failed_at: now,
            previous,
        };
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ObservationBegin {
    Started { request_id: RequestId },
    Coalesced { request_id: RequestId },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HostOperationError {
    Unavailable {
        host_id: ExecutionHostId,
    },
    Unsupported {
        host_id: ExecutionHostId,
        capability: WorkerCapability,
    },
    InvalidLocation(String),
    Failed(String),
}

impl fmt::Display for HostOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable { host_id } => {
                write!(formatter, "execution host {host_id} is unavailable")
            }
            Self::Unsupported {
                host_id,
                capability,
            } => write!(
                formatter,
                "execution host {host_id} does not support {capability:?} operations"
            ),
            Self::InvalidLocation(message) | Self::Failed(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for HostOperationError {}

pub(crate) fn validate_local_location(
    location: &ResourceLocation,
) -> Result<&Path, HostOperationError> {
    if !location.is_local() {
        return Err(HostOperationError::InvalidLocation(format!(
            "resource location belongs to execution host {}",
            location.execution_host_id
        )));
    }
    Ok(location.path.as_path())
}

pub(crate) fn local_ports() -> Vec<PortSnapshot> {
    crate::platform::active_tcp_listeners()
        .into_iter()
        .map(|listener| PortSnapshot {
            execution_host_id: ExecutionHostId::local(),
            transport: PortTransport::Tcp,
            bind_address: listener.bind_addr.to_string(),
            port: listener.port,
            pid: Some(listener.pid),
            command: listener.command,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_host::protocol::WorkerErrorCode;

    #[test]
    fn stale_observation_is_not_exposed_as_current() {
        let mut observation = HostObservation::Fresh {
            value: PortSnapshot {
                execution_host_id: ExecutionHostId::local(),
                transport: PortTransport::Tcp,
                bind_address: "127.0.0.1".into(),
                port: 3000,
                pid: Some(42),
                command: None,
            },
            request_id: RequestId::new(1),
            observed_at: Instant::now(),
        };

        observation.mark_stale();

        assert!(observation.current().is_none());
        assert!(matches!(observation, HostObservation::Stale { .. }));
        assert!(observation.value().is_some());
    }

    #[test]
    fn status_projects_failed_separately_from_pending_and_ready() {
        let ready = HostObservation::Fresh {
            value: 1u32,
            request_id: RequestId::new(1),
            observed_at: Instant::now(),
        };
        assert!(matches!(
            ready.status(),
            ObservationStatus::Ready(value) if *value == 1
        ));

        let pending = HostObservation::Pending {
            request_id: RequestId::new(2),
            requested_at: Instant::now(),
            previous: Some(1u32),
        };
        assert!(matches!(pending.status(), ObservationStatus::Pending));
        // latest() keeps previous; status() does not collapse Failed/Pending into Ready.
        assert_eq!(pending.latest().copied(), Some(1));

        let failed = HostObservation::Failed {
            error: WorkerError::new(WorkerErrorCode::TimedOut, "timed out"),
            request_id: RequestId::new(3),
            failed_at: Instant::now(),
            previous: Some(9u32),
        };
        match failed.status() {
            ObservationStatus::Failed(error) => {
                assert_eq!(error.code, WorkerErrorCode::TimedOut);
                assert_eq!(error.message, "timed out");
            }
            other => panic!("expected Failed status, got {other:?}"),
        }
        assert!(failed.status().current().is_none());
        assert!(failed.status().latest().is_none());
        assert_eq!(failed.latest().copied(), Some(9));
    }

    #[test]
    fn pending_coalesce_reuses_inflight_request_id() {
        let mut observation = HostObservation::Fresh {
            value: 1u32,
            request_id: RequestId::new(1),
            observed_at: Instant::now(),
        };
        let first = observation.begin_or_coalesce(RequestId::new(2), Instant::now());
        assert_eq!(
            first,
            ObservationBegin::Started {
                request_id: RequestId::new(2)
            }
        );
        let second = observation.begin_or_coalesce(RequestId::new(3), Instant::now());
        assert_eq!(
            second,
            ObservationBegin::Coalesced {
                request_id: RequestId::new(2)
            }
        );
        assert!(matches!(
            observation,
            HostObservation::Pending {
                request_id,
                ..
            } if request_id == RequestId::new(2)
        ));
    }

    #[test]
    fn stale_response_is_rejected_after_newer_request() {
        let mut observation = HostObservation::Pending {
            request_id: RequestId::new(2),
            requested_at: Instant::now(),
            previous: Some(1u32),
        };
        assert!(!observation.complete_success(RequestId::new(1), 9, Instant::now()));
        assert!(observation.complete_success(RequestId::new(2), 3, Instant::now()));
        assert_eq!(observation.current().copied(), Some(3));
    }

    #[test]
    fn timeout_preserves_typed_failed_state() {
        let requested_at = Instant::now() - OBSERVATION_REQUEST_TIMEOUT - Duration::from_secs(1);
        let mut observation = HostObservation::Pending {
            request_id: RequestId::new(4),
            requested_at,
            previous: Some(7u32),
        };
        assert!(observation.expire_pending(Instant::now(), OBSERVATION_REQUEST_TIMEOUT));
        assert!(observation.current().is_none());
        match &observation {
            HostObservation::Failed {
                error,
                previous,
                request_id,
                ..
            } => {
                assert_eq!(error.code, WorkerErrorCode::TimedOut);
                assert_eq!(previous.as_ref(), Some(&7));
                assert_eq!(*request_id, RequestId::new(4));
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }
}
