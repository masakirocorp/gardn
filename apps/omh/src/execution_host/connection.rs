use std::time::{Duration, Instant};

const INITIAL_RECONNECT_DELAY: Duration = Duration::from_secs(1);
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ConnectionState {
    Disconnected,
    Connecting {
        attempt: u32,
    },
    Connected,
    Backoff {
        attempt: u32,
        retry_at: Instant,
        error: String,
    },
    Disconnecting,
}

/// Runtime-only connection intent and retry state for one execution host.
#[derive(Clone, Debug)]
pub(crate) struct ConnectionLifecycle {
    desired_connected: bool,
    state: ConnectionState,
}

impl Default for ConnectionLifecycle {
    fn default() -> Self {
        Self {
            desired_connected: false,
            state: ConnectionState::Disconnected,
        }
    }
}

impl ConnectionLifecycle {
    pub(crate) fn state(&self) -> &ConnectionState {
        &self.state
    }

    pub(crate) fn request_connect(&mut self) -> bool {
        self.desired_connected = true;
        if self.state == ConnectionState::Disconnected {
            self.state = ConnectionState::Connecting { attempt: 1 };
            return true;
        }
        false
    }

    pub(crate) fn mark_connected(&mut self) {
        self.state = if self.desired_connected {
            ConnectionState::Connected
        } else {
            ConnectionState::Disconnecting
        };
    }

    pub(crate) fn mark_connection_failed(&mut self, now: Instant, error: String) {
        if !self.desired_connected {
            self.state = ConnectionState::Disconnected;
            return;
        }
        let attempt = match self.state {
            ConnectionState::Connecting { attempt }
            | ConnectionState::Backoff { attempt, .. } => attempt,
            ConnectionState::Disconnected
            | ConnectionState::Connected
            | ConnectionState::Disconnecting => 1,
        };
        self.state = ConnectionState::Backoff {
            attempt,
            retry_at: now + reconnect_delay(attempt),
            error,
        };
    }

    pub(crate) fn begin_due_retry(&mut self, now: Instant) -> bool {
        let ConnectionState::Backoff {
            attempt, retry_at, ..
        } = &self.state
        else {
            return false;
        };
        if !self.desired_connected || now < *retry_at {
            return false;
        }
        self.state = ConnectionState::Connecting {
            attempt: attempt.saturating_add(1),
        };
        true
    }

    /// Returns true when a live or connecting transport must be terminated.
    pub(crate) fn request_disconnect(&mut self) -> bool {
        self.desired_connected = false;
        match self.state {
            ConnectionState::Connecting { .. }
            | ConnectionState::Connected
            | ConnectionState::Disconnecting => {
                self.state = ConnectionState::Disconnecting;
                true
            }
            ConnectionState::Disconnected | ConnectionState::Backoff { .. } => {
                self.state = ConnectionState::Disconnected;
                false
            }
        }
    }

    pub(crate) fn finish_disconnect(&mut self) {
        self.desired_connected = false;
        self.state = ConnectionState::Disconnected;
    }
}

fn reconnect_delay(attempt: u32) -> Duration {
    let exponent = attempt.saturating_sub(1).min(5);
    INITIAL_RECONNECT_DELAY
        .saturating_mul(1_u32 << exponent)
        .min(MAX_RECONNECT_DELAY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_connections_retry_with_bounded_exponential_backoff() {
        let start = Instant::now();
        let mut lifecycle = ConnectionLifecycle::default();
        assert!(lifecycle.request_connect());

        lifecycle.mark_connection_failed(start, "offline".to_string());
        assert!(matches!(
            lifecycle.state(),
            ConnectionState::Backoff { attempt: 1, retry_at, error }
                if *retry_at == start + Duration::from_secs(1) && error == "offline"
        ));
        assert!(!lifecycle.begin_due_retry(start));
        assert!(lifecycle.begin_due_retry(start + Duration::from_secs(1)));
        assert_eq!(
            lifecycle.state(),
            &ConnectionState::Connecting { attempt: 2 }
        );

        lifecycle.mark_connection_failed(start, "still offline".to_string());
        assert!(matches!(
            lifecycle.state(),
            ConnectionState::Backoff { attempt: 2, retry_at, .. }
                if *retry_at == start + Duration::from_secs(2)
        ));
        assert_eq!(reconnect_delay(u32::MAX), MAX_RECONNECT_DELAY);
    }

    #[test]
    fn explicit_disconnect_cancels_backoff_and_suppresses_reconnect() {
        let start = Instant::now();
        let mut lifecycle = ConnectionLifecycle::default();
        lifecycle.request_connect();
        lifecycle.mark_connection_failed(start, "offline".to_string());

        assert!(!lifecycle.request_disconnect());
        assert_eq!(lifecycle.state(), &ConnectionState::Disconnected);
        assert!(!lifecycle.begin_due_retry(start + Duration::from_secs(60)));
    }

    #[test]
    fn disconnecting_a_live_transport_waits_for_termination() {
        let mut lifecycle = ConnectionLifecycle::default();
        lifecycle.request_connect();
        lifecycle.mark_connected();

        assert!(lifecycle.request_disconnect());
        assert_eq!(lifecycle.state(), &ConnectionState::Disconnecting);
        lifecycle.finish_disconnect();
        assert_eq!(lifecycle.state(), &ConnectionState::Disconnected);
    }
}
