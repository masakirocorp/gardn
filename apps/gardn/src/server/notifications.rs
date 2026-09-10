use std::collections::{HashMap, VecDeque};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gardn_local_api::{
    NotificationId, NotificationSound, NotificationSource, NotificationTarget, NotificationVisual,
    PresentationOutcome, PresentationReceipt, PresentationRequest, PresenterRegistration,
    RegistrationId, StateNotification,
};

const PRESENTATION_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_OUTSTANDING_PRESENTATIONS: usize = 256;
/// Verified mobile identity passed to an outbound provider adapter.
///
/// This type and the provider trait are intentionally dormant until Gardn has
/// an authenticated mobile control plane. They keep provider credentials and
/// delivery outside the Local API transport.
#[allow(dead_code)]
pub(crate) struct AuthenticatedRecipient {
    pub(crate) subject: String,
}

/// Provider-neutral boundary for future authenticated mobile delivery.
#[allow(dead_code)]
pub(crate) trait AuthenticatedOutboundDelivery: Send {
    fn submit(
        &mut self,
        recipient: &AuthenticatedRecipient,
        notification: &StateNotification,
    ) -> Result<PresentationOutcome, String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum PresenterTransport {
    Client(u64),
    Local(u64),
    Embedded,
}

#[derive(Debug, Clone)]
struct Presenter {
    id: RegistrationId,
    transport: PresenterTransport,
    registration: PresenterRegistration,
    eligible: bool,
    order: u64,
}

impl Presenter {
    fn is_foreground_client(&self, foreground_client_id: Option<u64>) -> bool {
        matches!(
            (self.transport, foreground_client_id),
            (PresenterTransport::Client(client), Some(foreground)) if client == foreground
        )
    }

    fn supports(&self, visual: NotificationVisual, sound: NotificationSound) -> bool {
        let visual_supported = match visual {
            NotificationVisual::None => true,
            NotificationVisual::Terminal => self.registration.capabilities.terminal,
            NotificationVisual::System => self.registration.capabilities.system,
            NotificationVisual::Gardn => false,
        };
        visual_supported && (sound.is_none() || self.registration.capabilities.sound)
    }
}

#[derive(Debug)]
struct OutstandingPresentation {
    registration_id: RegistrationId,
    transport: PresenterTransport,
    deadline_unix_ms: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct NotificationDraft {
    pub(crate) source: NotificationSource,
    pub(crate) target: Option<NotificationTarget>,
    pub(crate) title: String,
    pub(crate) body: Option<String>,
    pub(crate) visual: NotificationVisual,
    pub(crate) sound: NotificationSound,
}

#[derive(Debug)]
pub(crate) struct PreparedPresentation {
    pub(crate) transport: PresenterTransport,
    pub(crate) request: PresentationRequest,
}

#[derive(Debug)]
pub(crate) struct NotificationCoordinator {
    epoch: String,
    outstanding_order: VecDeque<NotificationId>,
    next_sequence: u64,
    presenters: Vec<Presenter>,
    outstanding: HashMap<NotificationId, OutstandingPresentation>,
}

impl NotificationCoordinator {
    pub(crate) fn new(epoch: String) -> Self {
        Self {
            epoch,
            next_sequence: 1,
            presenters: Vec::new(),
            outstanding: HashMap::new(),
            outstanding_order: VecDeque::new(),
        }
    }

    fn next_id(&mut self) -> RegistrationId {
        let id = RegistrationId {
            coordinator_epoch: self.epoch.clone(),
            sequence: self.next_sequence,
        };
        self.next_sequence = self.next_sequence.saturating_add(1);
        id
    }

    pub(crate) fn register(
        &mut self,
        transport: PresenterTransport,
        registration: PresenterRegistration,
        eligible: bool,
    ) -> RegistrationId {
        self.unregister_transport(transport);
        let id = self.next_id();
        let order = id.sequence;
        self.presenters.push(Presenter {
            id: id.clone(),
            transport,
            registration,
            eligible,
            order,
        });
        id
    }
    pub(crate) fn register_local(&mut self, registration: PresenterRegistration) -> RegistrationId {
        let id = self.next_id();
        let transport = PresenterTransport::Local(id.sequence);
        let order = id.sequence;
        self.presenters.push(Presenter {
            id: id.clone(),
            transport,
            registration,
            eligible: false,
            order,
        });
        id
    }

    pub(crate) fn set_eligible(&mut self, id: &RegistrationId) {
        if let Some(presenter) = self
            .presenters
            .iter_mut()
            .find(|presenter| presenter.id == *id)
        {
            presenter.eligible = true;
        }
    }

    pub(crate) fn unregister_transport(&mut self, transport: PresenterTransport) {
        self.presenters
            .retain(|presenter| presenter.transport != transport);
        self.outstanding
            .retain(|_, outstanding| outstanding.transport != transport);
        self.outstanding_order
            .retain(|id| self.outstanding.contains_key(id));
    }

    fn select(
        &self,
        visual: NotificationVisual,
        sound: NotificationSound,
        foreground_client_id: Option<u64>,
        excluded: Option<PresenterTransport>,
    ) -> Option<(&Presenter, NotificationVisual)> {
        match visual {
            NotificationVisual::System => self
                .presenters
                .iter()
                .filter(|presenter| {
                    presenter.eligible
                        && Some(presenter.transport) != excluded
                        && matches!(
                            presenter.transport,
                            PresenterTransport::Local(_) | PresenterTransport::Embedded
                        )
                        && presenter.supports(NotificationVisual::System, sound)
                })
                .min_by_key(|presenter| presenter.order)
                .map(|presenter| (presenter, NotificationVisual::System))
                .or_else(|| {
                    self.presenters
                        .iter()
                        .find(|presenter| {
                            presenter.eligible
                                && Some(presenter.transport) != excluded
                                && presenter.is_foreground_client(foreground_client_id)
                                && presenter.supports(NotificationVisual::System, sound)
                        })
                        .map(|presenter| (presenter, NotificationVisual::System))
                })
                .or_else(|| {
                    self.presenters
                        .iter()
                        .find(|presenter| {
                            presenter.eligible
                                && Some(presenter.transport) != excluded
                                && presenter.is_foreground_client(foreground_client_id)
                                && presenter.supports(NotificationVisual::Terminal, sound)
                        })
                        .map(|presenter| (presenter, NotificationVisual::Terminal))
                }),
            NotificationVisual::Terminal => self
                .presenters
                .iter()
                .find(|presenter| {
                    presenter.eligible
                        && Some(presenter.transport) != excluded
                        && (presenter.is_foreground_client(foreground_client_id)
                            || matches!(presenter.transport, PresenterTransport::Embedded))
                        && presenter.supports(NotificationVisual::Terminal, sound)
                })
                .map(|presenter| (presenter, NotificationVisual::Terminal)),
            NotificationVisual::None if !sound.is_none() => self
                .presenters
                .iter()
                .find(|presenter| {
                    presenter.eligible
                        && Some(presenter.transport) != excluded
                        && presenter.is_foreground_client(foreground_client_id)
                        && presenter.supports(NotificationVisual::None, sound)
                })
                .map(|presenter| (presenter, NotificationVisual::None))
                .or_else(|| {
                    self.presenters
                        .iter()
                        .filter(|presenter| {
                            presenter.eligible
                                && Some(presenter.transport) != excluded
                                && matches!(
                                    presenter.transport,
                                    PresenterTransport::Local(_) | PresenterTransport::Embedded
                                )
                                && presenter.supports(NotificationVisual::None, sound)
                        })
                        .min_by_key(|presenter| presenter.order)
                        .map(|presenter| (presenter, NotificationVisual::None))
                }),
            NotificationVisual::None | NotificationVisual::Gardn => None,
        }
    }

    pub(crate) fn prepare(
        &mut self,
        draft: NotificationDraft,
        foreground_client_id: Option<u64>,
    ) -> Option<PreparedPresentation> {
        self.prepare_excluding(draft, foreground_client_id, None)
    }

    pub(crate) fn prepare_excluding(
        &mut self,
        mut draft: NotificationDraft,
        foreground_client_id: Option<u64>,
        excluded: Option<PresenterTransport>,
    ) -> Option<PreparedPresentation> {
        let (presenter, selected_visual) =
            self.select(draft.visual, draft.sound, foreground_client_id, excluded)?;
        let registration_id = presenter.id.clone();
        let transport = presenter.transport;
        draft.visual = selected_visual;
        let now = unix_time_ms();
        let notification = StateNotification {
            id: NotificationId {
                coordinator_epoch: self.epoch.clone(),
                sequence: self.next_sequence,
            },
            source: draft.source,
            target: draft.target,
            title: draft.title,
            body: draft.body,
            visual: draft.visual,
            sound: draft.sound,
            created_at_unix_ms: now,
            expires_at_unix_ms: now.saturating_add(86_400_000),
        };
        self.next_sequence = self.next_sequence.saturating_add(1);
        Some(PreparedPresentation {
            transport,
            request: PresentationRequest {
                registration_id,
                notification,
            },
        })
    }

    pub(crate) fn accept(&mut self, prepared: &PreparedPresentation) {
        let notification_id = prepared.request.notification.id.clone();
        self.outstanding.insert(
            notification_id.clone(),
            OutstandingPresentation {
                registration_id: prepared.request.registration_id.clone(),
                transport: prepared.transport,
                deadline_unix_ms: unix_time_ms()
                    .saturating_add(PRESENTATION_TIMEOUT.as_millis() as u64),
            },
        );
        self.outstanding_order.push_back(notification_id.clone());
        while self.outstanding.len() > MAX_OUTSTANDING_PRESENTATIONS {
            if let Some(oldest) = self.outstanding_order.pop_front() {
                self.outstanding.remove(&oldest);
            }
        }
    }

    pub(crate) fn record_receipt(
        &mut self,
        transport: PresenterTransport,
        receipt: &PresentationReceipt,
    ) -> bool {
        let Some(outstanding) = self.outstanding.get(&receipt.notification_id) else {
            return false;
        };
        if outstanding.transport != transport
            || outstanding.registration_id != receipt.registration_id
        {
            return false;
        }
        self.outstanding.remove(&receipt.notification_id);
        self.outstanding_order
            .retain(|id| id != &receipt.notification_id);
        true
    }

    pub(crate) fn expire(&mut self, now_unix_ms: u64) {
        self.outstanding
            .retain(|_, outstanding| outstanding.deadline_unix_ms > now_unix_ms);
        self.outstanding_order
            .retain(|id| self.outstanding.contains_key(id));
    }
}

pub(crate) fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gardn_local_api::{PresentationOutcome, PresenterCapabilities};

    fn registration(system: bool, terminal: bool, sound: bool) -> PresenterRegistration {
        PresenterRegistration {
            name: "presenter".into(),
            rendering_host_id: "host".into(),
            capabilities: PresenterCapabilities {
                terminal,
                system,
                sound,
            },
        }
    }

    fn draft(visual: NotificationVisual) -> NotificationDraft {
        NotificationDraft {
            source: NotificationSource::State,
            target: None,
            title: "done".into(),
            body: None,
            visual,
            sound: NotificationSound::None,
        }
    }

    #[test]
    fn system_prefers_oldest_eligible_local_presenter() {
        let mut coordinator = NotificationCoordinator::new("epoch".into());
        let pending = coordinator.register(
            PresenterTransport::Local(1),
            registration(true, false, true),
            false,
        );
        coordinator.register(
            PresenterTransport::Client(7),
            registration(true, true, true),
            true,
        );
        assert_eq!(
            coordinator
                .prepare(draft(NotificationVisual::System), Some(7))
                .unwrap()
                .transport,
            PresenterTransport::Client(7)
        );

        coordinator.set_eligible(&pending);
        assert_eq!(
            coordinator
                .prepare(draft(NotificationVisual::System), Some(7))
                .unwrap()
                .transport,
            PresenterTransport::Local(1)
        );
    }

    #[test]
    fn system_falls_back_to_foreground_terminal_capability() {
        let mut coordinator = NotificationCoordinator::new("epoch".into());
        coordinator.register(
            PresenterTransport::Client(7),
            registration(false, true, true),
            true,
        );
        let prepared = coordinator
            .prepare(draft(NotificationVisual::System), Some(7))
            .unwrap();
        assert_eq!(prepared.transport, PresenterTransport::Client(7));
        assert_eq!(
            prepared.request.notification.visual,
            NotificationVisual::Terminal
        );
    }

    #[test]
    fn receipt_must_match_transport_and_registration() {
        let mut coordinator = NotificationCoordinator::new("epoch".into());
        coordinator.register(
            PresenterTransport::Client(7),
            registration(false, true, true),
            true,
        );
        let prepared = coordinator
            .prepare(draft(NotificationVisual::Terminal), Some(7))
            .unwrap();
        coordinator.accept(&prepared);
        let receipt = PresentationReceipt {
            registration_id: prepared.request.registration_id.clone(),
            notification_id: prepared.request.notification.id.clone(),
            outcome: PresentationOutcome::Submitted,
        };
        assert!(!coordinator.record_receipt(PresenterTransport::Client(8), &receipt));
        assert!(coordinator.record_receipt(PresenterTransport::Client(7), &receipt));
        assert!(!coordinator.record_receipt(PresenterTransport::Client(7), &receipt));
    }

    #[test]
    fn excluding_first_presenter_selects_one_alternate() {
        let mut coordinator = NotificationCoordinator::new("epoch".into());
        coordinator.register(
            PresenterTransport::Local(7),
            registration(true, false, true),
            true,
        );
        coordinator.register(
            PresenterTransport::Local(8),
            registration(true, false, true),
            true,
        );

        let first = coordinator
            .prepare(draft(NotificationVisual::System), None)
            .unwrap();
        let alternate = coordinator
            .prepare_excluding(
                draft(NotificationVisual::System),
                None,
                Some(first.transport),
            )
            .unwrap();

        assert_eq!(first.transport, PresenterTransport::Local(7));
        assert_eq!(alternate.transport, PresenterTransport::Local(8));
    }

    #[test]
    fn embedded_presenter_handles_terminal_without_foreground_client() {
        let mut coordinator = NotificationCoordinator::new("epoch".into());
        coordinator.register(
            PresenterTransport::Embedded,
            registration(false, true, true),
            true,
        );

        let prepared = coordinator
            .prepare(draft(NotificationVisual::Terminal), None)
            .unwrap();

        assert_eq!(prepared.transport, PresenterTransport::Embedded);
    }
}
