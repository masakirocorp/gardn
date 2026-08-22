use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
    time::{Duration, Instant},
};

use crate::{execution_host::ExecutionHostId, layout::PaneId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum PortTransport {
    Tcp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PortExposure {
    Loopback,
    Lan,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PortScheme {
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PortState {
    Active,
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PortOwnerConfidence {
    ProcessTree,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PortOwner {
    pub pid: u32,
    pub command: Option<String>,
    pub workspace_id: String,
    pub tab_idx: usize,
    pub pane_id: PaneId,
    pub confidence: PortOwnerConfidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PortObservation {
    pub execution_host_id: ExecutionHostId,
    pub transport: PortTransport,
    pub bind_addr: IpAddr,
    pub port: u16,
    pub pid: u32,
    pub command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PortKey {
    execution_host_id: ExecutionHostId,
    transport: PortTransport,
    bind_addr: IpAddr,
    port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PortEndpoint {
    pub execution_host_id: ExecutionHostId,
    pub transport: PortTransport,
    pub bind_addr: IpAddr,
    pub port: u16,
    pub exposure: PortExposure,
    pub scheme: PortScheme,
    pub url: Option<String>,
    pub owners: Vec<PortOwner>,
    pub first_seen_at: Instant,
    pub last_seen_at: Instant,
    pub state: PortState,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PortRegistry {
    endpoints: HashMap<PortKey, PortEndpoint>,
}

impl PortObservation {
    fn key(&self) -> PortKey {
        PortKey {
            execution_host_id: self.execution_host_id.clone(),
            transport: self.transport,
            bind_addr: self.bind_addr,
            port: self.port,
        }
    }
}

impl PortEndpoint {
    fn new(observation: &PortObservation, mut owners: Vec<PortOwner>, now: Instant) -> Self {
        owners.sort_by_key(|owner| (owner.tab_idx, owner.pane_id.raw(), owner.pid));
        owners.dedup_by_key(|owner| (owner.pid, owner.pane_id));
        Self {
            execution_host_id: observation.execution_host_id.clone(),
            transport: observation.transport,
            bind_addr: observation.bind_addr,
            port: observation.port,
            exposure: port_exposure(observation.bind_addr),
            scheme: PortScheme::Unknown,
            url: None,
            owners,
            first_seen_at: now,
            last_seen_at: now,
            state: PortState::Active,
        }
    }

    fn refresh(&mut self, mut owners: Vec<PortOwner>, now: Instant) {
        self.last_seen_at = now;
        self.state = PortState::Active;
        owners.sort_by_key(|owner| (owner.tab_idx, owner.pane_id.raw(), owner.pid));
        owners.dedup_by_key(|owner| (owner.pid, owner.pane_id));
        self.owners = owners;
    }
}

impl PortRegistry {
    pub(crate) fn sync_observations(
        &mut self,
        now: Instant,
        observations: impl IntoIterator<Item = PortObservation>,
        mut owner_for_pid: impl FnMut(&ExecutionHostId, u32) -> Option<PortOwner>,
    ) {
        let mut observed = HashMap::<PortKey, (PortObservation, Vec<PortOwner>)>::new();

        for observation in observations {
            let Some(mut owner) = owner_for_pid(&observation.execution_host_id, observation.pid)
            else {
                continue;
            };
            if owner.command.is_none() {
                owner.command = observation.command.clone();
            }

            let key = observation.key();
            let entry = observed
                .entry(key)
                .or_insert_with(|| (observation.clone(), Vec::new()));
            entry.1.push(owner);
        }

        let active_keys = observed.keys().cloned().collect::<HashSet<_>>();
        for (key, (observation, mut owners)) in observed {
            owners.sort_by_key(|owner| (owner.tab_idx, owner.pane_id.raw(), owner.pid));
            owners.dedup_by_key(|owner| (owner.pid, owner.pane_id));
            if owners.is_empty() {
                continue;
            }
            self.endpoints
                .entry(key)
                .and_modify(|endpoint| endpoint.refresh(owners.clone(), now))
                .or_insert_with(|| PortEndpoint::new(&observation, owners, now));
        }

        for (key, endpoint) in &mut self.endpoints {
            if !active_keys.contains(key) {
                endpoint.state = PortState::Stale;
            }
        }
    }

    pub(crate) fn prune_stale(&mut self, now: Instant, ttl: Duration) {
        self.endpoints.retain(|_, endpoint| {
            endpoint.state == PortState::Active || now.duration_since(endpoint.last_seen_at) < ttl
        });
    }

    pub(crate) fn endpoints(&self) -> Vec<PortEndpoint> {
        let mut endpoints: Vec<_> = self.endpoints.values().cloned().collect();
        endpoints.sort_by(|left, right| {
            (left.execution_host_id.as_str(), left.port, left.bind_addr).cmp(&(
                right.execution_host_id.as_str(),
                right.port,
                right.bind_addr,
            ))
        });
        endpoints
    }
}

impl From<crate::platform::TcpListenerInfo> for PortObservation {
    fn from(listener: crate::platform::TcpListenerInfo) -> Self {
        Self {
            execution_host_id: ExecutionHostId::local(),
            transport: PortTransport::Tcp,
            bind_addr: listener.bind_addr,
            port: listener.port,
            pid: listener.pid,
            command: listener.command,
        }
    }
}

impl PortObservation {
    pub(crate) fn from_worker_snapshot(
        snapshot: crate::execution_host::protocol::PortSnapshot,
    ) -> Option<Self> {
        let pid = snapshot.pid?;
        Some(Self {
            execution_host_id: snapshot.execution_host_id,
            transport: match snapshot.transport {
                crate::execution_host::protocol::PortTransport::Tcp => PortTransport::Tcp,
            },
            bind_addr: snapshot.bind_address.parse().ok()?,
            port: snapshot.port,
            pid,
            command: snapshot.command,
        })
    }
}

fn port_exposure(addr: IpAddr) -> PortExposure {
    if addr.is_loopback() {
        PortExposure::Loopback
    } else if addr.is_unspecified() {
        PortExposure::All
    } else {
        PortExposure::Lan
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner(pid: u32, pane: u32) -> PortOwner {
        PortOwner {
            pid,
            command: None,
            workspace_id: "workspace".to_string(),
            tab_idx: 0,
            pane_id: PaneId::from_raw(pane),
            confidence: PortOwnerConfidence::ProcessTree,
        }
    }

    fn observation(addr: &str, port: u16, pid: u32) -> PortObservation {
        PortObservation {
            execution_host_id: ExecutionHostId::local(),
            transport: PortTransport::Tcp,
            bind_addr: addr.parse().expect("test IP address"),
            port,
            pid,
            command: Some("vite".to_string()),
        }
    }

    #[test]
    fn registry_tracks_owned_active_ports() {
        let now = Instant::now();
        let mut registry = PortRegistry::default();

        registry.sync_observations(now, [observation("127.0.0.1", 5173, 42)], |_, pid| {
            (pid == 42).then(|| owner(pid, 7))
        });

        let endpoints = registry.endpoints();
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].port, 5173);
        assert_eq!(endpoints[0].exposure, PortExposure::Loopback);
        assert_eq!(endpoints[0].state, PortState::Active);
        assert_eq!(endpoints[0].owners[0].pane_id, PaneId::from_raw(7));
        assert_eq!(endpoints[0].owners[0].command.as_deref(), Some("vite"));
    }

    #[test]
    fn registry_hides_unowned_ports() {
        let mut registry = PortRegistry::default();

        registry.sync_observations(
            Instant::now(),
            [observation("127.0.0.1", 3000, 99)],
            |_, _| None,
        );

        assert!(registry.endpoints().is_empty());
    }

    #[test]
    fn registry_marks_missing_ports_stale_then_prunes_them() {
        let now = Instant::now();
        let mut registry = PortRegistry::default();

        registry.sync_observations(now, [observation("0.0.0.0", 8787, 10)], |_, pid| {
            Some(owner(pid, 1))
        });
        registry.sync_observations(now + Duration::from_secs(1), [], |_, _| None);

        let stale = registry.endpoints();
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].exposure, PortExposure::All);
        assert_eq!(stale[0].state, PortState::Stale);

        registry.prune_stale(now + Duration::from_secs(5), Duration::from_secs(2));

        assert!(registry.endpoints().is_empty());
    }

    #[test]
    fn registry_keeps_multiple_owners_for_shared_listener() {
        let now = Instant::now();
        let mut registry = PortRegistry::default();

        registry.sync_observations(
            now,
            [
                observation("192.168.1.10", 3000, 1),
                observation("192.168.1.10", 3000, 2),
            ],
            |_, pid| Some(owner(pid, pid)),
        );

        let endpoints = registry.endpoints();
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].exposure, PortExposure::Lan);
        assert_eq!(endpoints[0].owners.len(), 2);
    }

    #[test]
    fn registry_replaces_owners_for_active_listener() {
        let now = Instant::now();
        let mut registry = PortRegistry::default();

        registry.sync_observations(now, [observation("127.0.0.1", 5173, 42)], |_, pid| {
            Some(owner(pid, 1))
        });
        registry.sync_observations(
            now + Duration::from_secs(1),
            [observation("127.0.0.1", 5173, 84)],
            |_, pid| Some(owner(pid, 2)),
        );

        let endpoints = registry.endpoints();
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].state, PortState::Active);
        assert_eq!(endpoints[0].owners.len(), 1);
        assert_eq!(endpoints[0].owners[0].pid, 84);
        assert_eq!(endpoints[0].owners[0].pane_id, PaneId::from_raw(2));
    }
    #[test]
    fn same_endpoint_on_two_hosts_remains_distinct() {
        let now = Instant::now();
        let mut registry = PortRegistry::default();
        let remote_host = ExecutionHostId::new("ssh:workbox").expect("remote host id");
        let mut remote = observation("127.0.0.1", 5173, 42);
        remote.execution_host_id = remote_host.clone();

        registry.sync_observations(
            now,
            [observation("127.0.0.1", 5173, 42), remote],
            |host_id, pid| Some(owner(pid, if host_id.is_local() { 1 } else { 2 })),
        );

        let endpoints = registry.endpoints();
        assert_eq!(endpoints.len(), 2);
        assert!(endpoints
            .iter()
            .any(|endpoint| endpoint.execution_host_id.is_local()));
        assert!(endpoints
            .iter()
            .any(|endpoint| endpoint.execution_host_id == remote_host));
    }
}
