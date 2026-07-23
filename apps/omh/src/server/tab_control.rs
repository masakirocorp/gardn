//! Transient ownership of tab geometry and input control for normal clients.
//!
//! A [`TabControlKey`] uses the workspace's stable public tab number rather
//! than a tab's current vector position or pane root. Ownership is deliberately
//! in-memory: disconnecting a client or deleting a tab releases its claim.

use std::collections::HashMap;
use std::fmt;

/// Stable identity of a tab for the lifetime of its public workspace identity.
///
/// `tab_number` is the tab's stable public number, not the current index in
/// `Workspace::tabs`; it therefore survives tab moves and neighboring closes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct TabControlKey {
    pub(crate) workspace_id: String,
    pub(crate) tab_number: usize,
}

impl TabControlKey {
    pub(crate) fn new(workspace_id: impl Into<String>, tab_number: usize) -> Self {
        Self {
            workspace_id: workspace_id.into(),
            tab_number,
        }
    }
}

/// The observable state of one tab's transient control claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TabControlStatus {
    /// Changes on every successful claim transfer or release.
    pub(crate) epoch: u64,
    /// The normal client controlling this tab, if any.
    pub(crate) controller: Option<u64>,
}

impl TabControlStatus {
    pub(crate) const fn free() -> Self {
        Self {
            epoch: 0,
            controller: None,
        }
    }

    pub(crate) const fn is_free(self) -> bool {
        self.controller.is_none()
    }

    pub(crate) const fn is_controlled_by(self, client_id: u64) -> bool {
        matches!(self.controller, Some(owner) if owner == client_id)
    }
}

/// Failure to change a tab's transient control state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TabControlError {
    /// A free-only acquisition cannot displace another client.
    Occupied { epoch: u64 },
    /// The caller's observation predates the current tab state.
    StaleEpoch {
        observed_epoch: u64,
        current_epoch: u64,
    },
    /// A normal client may control at most one tab.
    ClientAlreadyControlsTab { tab: TabControlKey },
    /// The caller already owns the requested tab.
    AlreadyController,
    /// Only the current controller may publish canonical terminal geometry.
    NotController,
    /// No further state transition can be represented by the epoch type.
    EpochExhausted,
}

impl fmt::Display for TabControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Occupied { .. } => f.write_str("another client controls this tab"),
            Self::StaleEpoch {
                observed_epoch,
                current_epoch,
            } => write!(
                f,
                "tab control epoch is stale (observed {observed_epoch}, current {current_epoch})"
            ),
            Self::ClientAlreadyControlsTab { .. } => {
                f.write_str("client already controls another tab")
            }
            Self::AlreadyController => f.write_str("caller already controls this tab"),
            Self::NotController => f.write_str("caller does not control this tab"),
            Self::EpochExhausted => f.write_str("tab control epoch exhausted"),
        }
    }
}

impl std::error::Error for TabControlError {}

#[derive(Debug, Default)]
pub(crate) struct TabControlCoordinator {
    tabs: HashMap<TabControlKey, TabControlStatus>,
    controlled_tabs: HashMap<u64, TabControlKey>,
    canvas_sizes: HashMap<TabControlKey, (u16, u16)>,
}

impl TabControlCoordinator {
    pub(crate) fn tab_keys(&self) -> impl Iterator<Item = &TabControlKey> {
        self.tabs.keys()
    }
}

impl TabControlCoordinator {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Returns free state for a tab that has not been claimed yet.
    pub(crate) fn status(&self, tab: &TabControlKey) -> TabControlStatus {
        self.tabs
            .get(tab)
            .copied()
            .unwrap_or_else(TabControlStatus::free)
    }

    /// Reverse lookup for the one tab controlled by a normal client.
    pub(crate) fn controlled_tab_for_client(&self, client_id: u64) -> Option<TabControlKey> {
        self.controlled_tabs.get(&client_id).cloned()
    }

    pub(crate) fn canvas_size(&self, tab: &TabControlKey) -> Option<(u16, u16)> {
        self.canvas_sizes.get(tab).copied()
    }

    pub(crate) fn set_canvas_size(
        &mut self,
        client_id: u64,
        tab: &TabControlKey,
        size: (u16, u16),
    ) -> Result<bool, TabControlError> {
        if !self.status(tab).is_controlled_by(client_id) {
            return Err(TabControlError::NotController);
        }
        Ok(self.canvas_sizes.insert(tab.clone(), size) != Some(size))
    }

    /// Claims a free tab. It never displaces an existing controller.
    pub(crate) fn acquire_free(
        &mut self,
        client_id: u64,
        tab: &TabControlKey,
    ) -> Result<TabControlStatus, TabControlError> {
        let current = self.status(tab);
        if let Some(controller) = current.controller {
            if controller == client_id {
                return Err(TabControlError::AlreadyController);
            }
            return Err(TabControlError::Occupied {
                epoch: current.epoch,
            });
        }

        self.ensure_client_is_free(client_id, tab)?;
        let next = Self::controlled_status(current, client_id)?;
        self.tabs.insert(tab.clone(), next);
        self.controlled_tabs.insert(client_id, tab.clone());
        Ok(next)
    }

    /// Takes over a tab after validating the caller's observed epoch.
    ///
    /// Takeover is also valid for a matching free observation, which makes an
    /// explicit user action race-safe without requiring a separate branch.
    pub(crate) fn takeover(
        &mut self,
        client_id: u64,
        tab: &TabControlKey,
        observed_epoch: u64,
    ) -> Result<TabControlStatus, TabControlError> {
        let current = self.status(tab);
        if current.epoch != observed_epoch {
            return Err(TabControlError::StaleEpoch {
                observed_epoch,
                current_epoch: current.epoch,
            });
        }
        if current.is_controlled_by(client_id) {
            return Err(TabControlError::AlreadyController);
        }

        self.ensure_client_is_free(client_id, tab)?;
        let next = Self::controlled_status(current, client_id)?;
        if let Some(previous_controller) = current.controller {
            self.controlled_tabs.remove(&previous_controller);
        }
        self.tabs.insert(tab.clone(), next);
        self.controlled_tabs.insert(client_id, tab.clone());
        Ok(next)
    }

    /// Releases a claim only when it belongs to `client_id`.
    ///
    /// A missing claim, a claim owned by another client, and a duplicate
    /// release all return `Ok(false)` without changing state.
    pub(crate) fn release(
        &mut self,
        client_id: u64,
        tab: &TabControlKey,
    ) -> Result<bool, TabControlError> {
        let Some(current) = self.tabs.get(tab).copied() else {
            return Ok(false);
        };
        if !current.is_controlled_by(client_id) {
            return Ok(false);
        }

        let next = Self::free_status(current)?;
        self.tabs.insert(tab.clone(), next);
        if self.controlled_tabs.get(&client_id) == Some(tab) {
            self.controlled_tabs.remove(&client_id);
        }
        Ok(true)
    }

    /// Releases the client's claim during disconnect. Duplicate disconnects
    /// are harmless and return `Ok(None)`.
    pub(crate) fn release_client(
        &mut self,
        client_id: u64,
    ) -> Result<Option<TabControlKey>, TabControlError> {
        let Some(tab) = self.controlled_tabs.get(&client_id).cloned() else {
            return Ok(None);
        };
        self.release(client_id, &tab)?;
        Ok(Some(tab))
    }

    /// Removes all state for a deleted tab and forgets its reverse lookup.
    /// Repeating cleanup for the same tab is harmless.
    pub(crate) fn remove_tab(&mut self, tab: &TabControlKey) -> bool {
        let Some(status) = self.tabs.remove(tab) else {
            return false;
        };
        if let Some(client_id) = status.controller {
            if self.controlled_tabs.get(&client_id) == Some(tab) {
                self.controlled_tabs.remove(&client_id);
            }
        }
        self.canvas_sizes.remove(tab);
        true
    }

    fn ensure_client_is_free(
        &self,
        client_id: u64,
        requested_tab: &TabControlKey,
    ) -> Result<(), TabControlError> {
        let Some(existing_tab) = self.controlled_tabs.get(&client_id) else {
            return Ok(());
        };
        if existing_tab == requested_tab {
            return Err(TabControlError::AlreadyController);
        }
        Err(TabControlError::ClientAlreadyControlsTab {
            tab: existing_tab.clone(),
        })
    }

    fn controlled_status(
        current: TabControlStatus,
        client_id: u64,
    ) -> Result<TabControlStatus, TabControlError> {
        Ok(TabControlStatus {
            epoch: current
                .epoch
                .checked_add(1)
                .ok_or(TabControlError::EpochExhausted)?,
            controller: Some(client_id),
        })
    }

    fn free_status(current: TabControlStatus) -> Result<TabControlStatus, TabControlError> {
        Ok(TabControlStatus {
            epoch: current
                .epoch
                .checked_add(1)
                .ok_or(TabControlError::EpochExhausted)?,
            controller: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(workspace_id: &str, tab_number: usize) -> TabControlKey {
        TabControlKey::new(workspace_id, tab_number)
    }

    #[test]
    fn acquisition_claims_free_tab_and_records_reverse_lookup() {
        let mut controls = TabControlCoordinator::new();
        let tab = tab("workspace", 7);

        assert_eq!(controls.status(&tab), TabControlStatus::free());
        assert!(controls.status(&tab).is_free());
        let claimed = controls
            .acquire_free(11, &tab)
            .expect("free tab is claimable");

        assert_eq!(
            claimed,
            TabControlStatus {
                epoch: 1,
                controller: Some(11)
            }
        );
        assert!(!claimed.is_free());
        assert_eq!(controls.status(&tab), claimed);
        assert_eq!(controls.controlled_tab_for_client(11), Some(tab));
    }

    #[test]
    fn free_acquisition_rejects_an_occupied_tab_without_mutating_it() {
        let mut controls = TabControlCoordinator::new();
        let tab = tab("workspace", 1);
        let first = controls
            .acquire_free(1, &tab)
            .expect("first client claims tab");

        assert_eq!(
            controls.acquire_free(2, &tab),
            Err(TabControlError::Occupied { epoch: first.epoch })
        );
        assert_eq!(controls.status(&tab), first);
        assert_eq!(controls.controlled_tab_for_client(2), None);
    }

    #[test]
    fn explicit_takeover_transfers_occupied_tab_and_advances_epoch() {
        let mut controls = TabControlCoordinator::new();
        let tab = tab("workspace", 1);
        let first = controls
            .acquire_free(1, &tab)
            .expect("first client claims tab");

        let second = controls
            .takeover(2, &tab, first.epoch)
            .expect("matching explicit takeover succeeds");

        assert_eq!(
            second,
            TabControlStatus {
                epoch: 2,
                controller: Some(2)
            }
        );
        assert_eq!(controls.controlled_tab_for_client(1), None);
        assert_eq!(controls.controlled_tab_for_client(2), Some(tab));
    }

    #[test]
    fn takeover_rejects_stale_epoch_without_displacing_controller() {
        let mut controls = TabControlCoordinator::new();
        let tab = tab("workspace", 1);
        let first = controls
            .acquire_free(1, &tab)
            .expect("first client claims tab");

        assert_eq!(
            controls.takeover(2, &tab, first.epoch - 1),
            Err(TabControlError::StaleEpoch {
                observed_epoch: 0,
                current_epoch: first.epoch,
            })
        );
        assert_eq!(controls.status(&tab), first);
    }

    #[test]
    fn release_by_tab_and_client_is_idempotent() {
        let mut controls = TabControlCoordinator::new();
        let tab = tab("workspace", 1);
        controls.acquire_free(9, &tab).expect("client claims tab");

        assert_eq!(controls.release(9, &tab), Ok(true));
        assert_eq!(
            controls.status(&tab),
            TabControlStatus {
                epoch: 2,
                controller: None
            }
        );
        assert_eq!(controls.release(9, &tab), Ok(false));

        controls
            .acquire_free(9, &tab)
            .expect("client can reclaim released tab");
        assert_eq!(controls.release_client(9), Ok(Some(tab.clone())));
        assert_eq!(controls.release_client(9), Ok(None));
        assert_eq!(controls.status(&tab).controller, None);
    }

    #[test]
    fn one_client_cannot_switch_tabs_without_releasing_first() {
        let mut controls = TabControlCoordinator::new();
        let first_tab = tab("workspace", 1);
        let second_tab = tab("workspace", 2);
        controls
            .acquire_free(4, &first_tab)
            .expect("client claims first tab");

        assert_eq!(
            controls.acquire_free(4, &second_tab),
            Err(TabControlError::ClientAlreadyControlsTab {
                tab: first_tab.clone(),
            })
        );
        assert_eq!(
            controls.takeover(4, &second_tab, 0),
            Err(TabControlError::ClientAlreadyControlsTab {
                tab: first_tab.clone(),
            })
        );
        controls
            .release(4, &first_tab)
            .expect("client releases first tab");
        controls
            .acquire_free(4, &second_tab)
            .expect("client claims second tab after release");
        assert_eq!(controls.controlled_tab_for_client(4), Some(second_tab));
    }

    #[test]
    fn canonical_canvas_is_controller_owned_and_survives_release() {
        let mut controls = TabControlCoordinator::new();
        let tab = tab("workspace", 2);
        controls.acquire_free(4, &tab).expect("client claims tab");

        assert_eq!(
            controls.set_canvas_size(9, &tab, (80, 24)),
            Err(TabControlError::NotController)
        );
        assert_eq!(controls.set_canvas_size(4, &tab, (120, 40)), Ok(true));
        assert_eq!(controls.set_canvas_size(4, &tab, (120, 40)), Ok(false));
        controls.release_client(4).expect("controller releases tab");
        assert_eq!(controls.canvas_size(&tab), Some((120, 40)));
    }

    #[test]
    fn tab_cleanup_forgets_owner_and_is_safe_to_repeat() {
        let mut controls = TabControlCoordinator::new();
        let tab = tab("workspace", 3);
        controls.acquire_free(8, &tab).expect("client claims tab");

        assert!(controls.remove_tab(&tab));
        assert_eq!(controls.controlled_tab_for_client(8), None);
        assert_eq!(controls.status(&tab), TabControlStatus::free());
        assert!(!controls.remove_tab(&tab));

        controls
            .acquire_free(12, &tab)
            .expect("deleted tab identity can be tracked if recreated");
        assert_eq!(controls.controlled_tab_for_client(12), Some(tab.clone()));
        controls
            .release_client(12)
            .expect("client releases recreated tab");
        assert!(controls.remove_tab(&tab));
        assert_eq!(controls.tab_keys().count(), 0);
    }
}
