use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
    time::{Duration, Instant},
};

use crate::layout::PaneId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    Http,
    Https,
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
    pub bind_addr: IpAddr,
    pub port: u16,
    pub pid: u32,
    pub command: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PortKey {
    bind_addr: IpAddr,
    port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PortEndpoint {
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

#[derive(Debug, Default)]
pub(crate) struct PortRegistry {
    endpoints: HashMap<PortKey, PortEndpoint>,
}

impl PortObservation {
    fn key(&self) -> PortKey {
        PortKey {
            bind_addr: self.bind_addr,
            port: self.port,
        }
    }
}

impl PortEndpoint {
    fn new(observation: &PortObservation, owner: PortOwner, now: Instant) -> Self {
        Self {
            transport: PortTransport::Tcp,
            bind_addr: observation.bind_addr,
            port: observation.port,
            exposure: port_exposure(observation.bind_addr),
            scheme: PortScheme::Unknown,
            url: None,
            owners: vec![owner],
            first_seen_at: now,
            last_seen_at: now,
            state: PortState::Active,
        }
    }

    fn refresh(&mut self, owner: PortOwner, now: Instant) {
        self.last_seen_at = now;
        self.state = PortState::Active;
        if !self
            .owners
            .iter()
            .any(|existing| existing.pid == owner.pid && existing.pane_id == owner.pane_id)
        {
            self.owners.push(owner);
            self.owners.sort_by_key(|owner| (owner.tab_idx, owner.pane_id.raw(), owner.pid));
        }
    }
}

impl PortRegistry {
    pub(crate) fn sync_observations(
        &mut self,
        now: Instant,
        observations: impl IntoIterator<Item = PortObservation>,
        mut owner_for_pid: impl FnMut(u32) -> Option<PortOwner>,
    ) {
        let mut active_keys = HashSet::new();

        for observation in observations {
            let Some(mut owner) = owner_for_pid(observation.pid) else {
                continue;
            };
            if owner.command.is_none() {
                owner.command = observation.command.clone();
            }

            let key = observation.key();
            active_keys.insert(key);
            self.endpoints
                .entry(key)
                .and_modify(|endpoint| endpoint.refresh(owner.clone(), now))
                .or_insert_with(|| PortEndpoint::new(&observation, owner, now));
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
        endpoints.sort_by_key(|endpoint| (endpoint.port, endpoint.bind_addr));
        endpoints
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

        registry.sync_observations(
            now,
            [observation("127.0.0.1", 5173, 42)],
            |pid| (pid == 42).then(|| owner(pid, 7)),
        );

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
            |_| None,
        );

        assert!(registry.endpoints().is_empty());
    }

    #[test]
    fn registry_marks_missing_ports_stale_then_prunes_them() {
        let now = Instant::now();
        let mut registry = PortRegistry::default();

        registry.sync_observations(now, [observation("0.0.0.0", 8787, 10)], |pid| {
            Some(owner(pid, 1))
        });
        registry.sync_observations(now + Duration::from_secs(1), [], |_| None);

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
            |pid| Some(owner(pid, pid)),
        );

        let endpoints = registry.endpoints();
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].exposure, PortExposure::Lan);
        assert_eq!(endpoints[0].owners.len(), 2);
    }
}
