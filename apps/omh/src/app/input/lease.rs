use std::collections::HashMap;

use crate::app::input::TerminalKeyTarget;
use crate::input::{KeyIdentity, TerminalKey};

pub(crate) type InputSourceId = u64;
pub(crate) const LOCAL_INPUT_SOURCE: InputSourceId = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct InputLeaseKey {
    source_id: InputSourceId,
    identity: KeyIdentity,
}

impl InputLeaseKey {
    pub(crate) fn new(source_id: InputSourceId, key: &TerminalKey) -> Self {
        Self {
            source_id,
            identity: key.identity(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TerminalInputContext {
    Pane,
    Popup(crate::layout::PaneId),
    Modal,
}

impl TerminalInputContext {
    pub(crate) fn is_terminal(&self) -> bool {
        !matches!(self, Self::Modal)
    }
}

fn is_repeatable_modal_navigation_key(key: &TerminalKey) -> bool {
    key.modifiers.is_empty()
        && matches!(
            key.code,
            crossterm::event::KeyCode::Up
                | crossterm::event::KeyCode::Down
                | crossterm::event::KeyCode::Left
                | crossterm::event::KeyCode::Right
                | crossterm::event::KeyCode::PageUp
                | crossterm::event::KeyCode::PageDown
                | crossterm::event::KeyCode::Char('j')
                | crossterm::event::KeyCode::Char('k')
                | crossterm::event::KeyCode::Char('h')
                | crossterm::event::KeyCode::Char('l')
        )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ForwardedInputLease {
    pub(crate) target: TerminalKeyTarget,
    pub(crate) key: TerminalKey,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ConsumedInputLease {
    ReprocessRepeats(TerminalInputContext),
    SuppressRepeats,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InputLease {
    Forwarded(ForwardedInputLease),
    Consumed(ConsumedInputLease),
}

pub(crate) enum RepeatPlan {
    Forwarded(TerminalKeyTarget),
    Reprocess {
        context: TerminalInputContext,
        repetitions: u16,
        tracked: bool,
    },
    Ignore,
}

#[derive(Default, Clone)]
pub(crate) struct InputLeaseTable {
    leases: HashMap<InputLeaseKey, InputLease>,
}

impl InputLeaseTable {
    pub(crate) fn normalize_press(
        &mut self,
        lease_key: &InputLeaseKey,
        key: TerminalKey,
    ) -> TerminalKey {
        if key.kind != crossterm::event::KeyEventKind::Press || key.generated_text.is_some() {
            return key;
        }
        if key.has_physical_identity() && self.leases.contains_key(lease_key) {
            key.with_kind(crossterm::event::KeyEventKind::Repeat)
        } else {
            self.leases.remove(lease_key);
            key
        }
    }

    pub(crate) fn complete_press(
        &mut self,
        lease_key: InputLeaseKey,
        key: &TerminalKey,
        initial_context: Option<&TerminalInputContext>,
        resulting_context: Option<&TerminalInputContext>,
        target: Option<TerminalKeyTarget>,
    ) -> RepeatPlan {
        if key.generated_text.is_some() {
            return RepeatPlan::Ignore;
        }
        if let Some(target) = target {
            self.insert_forwarded(lease_key, target, key.clone());
            return RepeatPlan::Ignore;
        }
        if !self.leases.contains_key(&lease_key) {
            let disposition = match (initial_context, resulting_context) {
                (Some(initial), Some(resulting)) if initial == resulting => {
                    ConsumedInputLease::ReprocessRepeats(initial.clone())
                }
                _ => ConsumedInputLease::SuppressRepeats,
            };
            self.insert_consumed(lease_key, disposition);
        }
        match self.leases.get(&lease_key) {
            Some(InputLease::Consumed(ConsumedInputLease::ReprocessRepeats(context)))
                if key.repeat_count > 1 =>
            {
                RepeatPlan::Reprocess {
                    context: context.clone(),
                    repetitions: key.repeat_count - 1,
                    tracked: true,
                }
            }
            _ => RepeatPlan::Ignore,
        }
    }

    pub(crate) fn plan_repeat(
        &mut self,
        lease_key: InputLeaseKey,
        key: &TerminalKey,
        current_context: Option<&TerminalInputContext>,
    ) -> RepeatPlan {
        match self.leases.get(&lease_key) {
            Some(InputLease::Forwarded(lease)) => {
                return RepeatPlan::Forwarded(lease.target.clone());
            }
            Some(InputLease::Consumed(ConsumedInputLease::ReprocessRepeats(context)))
                if current_context == Some(context)
                    && (context != &TerminalInputContext::Modal
                        || is_repeatable_modal_navigation_key(key)) =>
            {
                return RepeatPlan::Reprocess {
                    context: context.clone(),
                    repetitions: key.repeat_count,
                    tracked: true,
                };
            }
            Some(InputLease::Consumed(ConsumedInputLease::ReprocessRepeats(_))) => {
                self.insert_consumed(lease_key, ConsumedInputLease::SuppressRepeats);
                return RepeatPlan::Ignore;
            }
            Some(InputLease::Consumed(ConsumedInputLease::SuppressRepeats)) => {
                return RepeatPlan::Ignore;
            }
            None => {}
        }
        match current_context {
            Some(TerminalInputContext::Modal) if !is_repeatable_modal_navigation_key(key) => {
                RepeatPlan::Ignore
            }
            Some(context) => RepeatPlan::Reprocess {
                context: context.clone(),
                repetitions: key.repeat_count,
                tracked: false,
            },
            None => RepeatPlan::Ignore,
        }
    }

    pub(crate) fn reprocess_allowed(
        &mut self,
        lease_key: InputLeaseKey,
        expected_context: &TerminalInputContext,
        current_context: Option<&TerminalInputContext>,
        tracked: bool,
    ) -> bool {
        let allowed = current_context == Some(expected_context);
        if tracked && !allowed {
            self.insert_consumed(lease_key, ConsumedInputLease::SuppressRepeats);
        }
        allowed
    }

    pub(crate) fn remove_forwarded(&mut self, key: &InputLeaseKey) -> Option<ForwardedInputLease> {
        match self.leases.remove(key) {
            Some(InputLease::Forwarded(lease)) => Some(lease),
            Some(InputLease::Consumed(_)) | None => None,
        }
    }

    pub(crate) fn insert_forwarded(
        &mut self,
        key: InputLeaseKey,
        target: TerminalKeyTarget,
        original: TerminalKey,
    ) {
        self.leases.insert(
            key,
            InputLease::Forwarded(ForwardedInputLease {
                target,
                key: original,
            }),
        );
    }

    pub(crate) fn insert_consumed(&mut self, key: InputLeaseKey, disposition: ConsumedInputLease) {
        self.leases.insert(key, InputLease::Consumed(disposition));
    }

    pub(crate) fn remove(&mut self, key: &InputLeaseKey) -> Option<InputLease> {
        self.leases.remove(key)
    }

    pub(crate) fn remove_source(&mut self, source_id: InputSourceId) -> Vec<ForwardedInputLease> {
        let keys = self
            .leases
            .keys()
            .filter(|key| key.source_id == source_id)
            .copied()
            .collect::<Vec<_>>();
        self.remove_keys(keys)
    }

    pub(crate) fn remove_target(&mut self, target: &TerminalKeyTarget) -> Vec<ForwardedInputLease> {
        let keys = self
            .leases
            .iter()
            .filter_map(|(key, lease)| match lease {
                InputLease::Forwarded(lease) if &lease.target == target => Some(*key),
                InputLease::Forwarded(_) | InputLease::Consumed(_) => None,
            })
            .collect::<Vec<_>>();
        self.remove_keys(keys)
    }

    fn remove_keys(
        &mut self,
        keys: impl IntoIterator<Item = InputLeaseKey>,
    ) -> Vec<ForwardedInputLease> {
        keys.into_iter()
            .filter_map(|key| match self.leases.remove(&key) {
                Some(InputLease::Forwarded(lease)) => Some(lease),
                Some(InputLease::Consumed(_)) | None => None,
            })
            .collect()
    }

    pub(crate) fn clear(&mut self) {
        self.leases.clear();
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.leases.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, key: &InputLeaseKey) -> bool {
        self.leases.contains_key(key)
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.leases.len()
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyModifiers};

    use super::*;
    use crate::layout::PaneId;

    fn target() -> TerminalKeyTarget {
        TerminalKeyTarget {
            workspace_id: "ws".into(),
            pane_id: PaneId::from_raw(1),
        }
    }

    #[test]
    fn physical_and_semantic_identities_do_not_collide() {
        let record = crate::input::WindowsKeyRecord {
            key_down: true,
            repeat_count: 1,
            virtual_key_code: 65,
            virtual_scan_code: 30,
            unicode: 97,
            control_key_state: 0,
        };
        let physical =
            TerminalKey::new(KeyCode::Char('a'), KeyModifiers::empty()).with_windows_record(record);
        let semantic = TerminalKey::new(KeyCode::Char('a'), KeyModifiers::empty());

        assert_ne!(
            InputLeaseKey::new(7, &physical),
            InputLeaseKey::new(7, &semantic)
        );
    }

    #[test]
    fn remove_source_returns_forwarded_and_discards_consumed_leases() {
        let key = TerminalKey::new(KeyCode::Esc, KeyModifiers::empty());
        let forwarded = InputLeaseKey::new(7, &key);
        let consumed = InputLeaseKey::new(
            7,
            &TerminalKey::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        );
        let other_source = InputLeaseKey::new(8, &key);
        let first_target = target();
        let mut leases = InputLeaseTable::default();
        leases.insert_forwarded(forwarded, first_target.clone(), key.clone());
        leases.insert_consumed(consumed, ConsumedInputLease::SuppressRepeats);
        leases.insert_forwarded(other_source, target(), key.clone());

        assert_eq!(
            leases.remove_source(7),
            vec![ForwardedInputLease {
                target: first_target,
                key,
            }]
        );
        assert_eq!(leases.len(), 1);
        assert!(leases.contains(&other_source));
    }

    #[test]
    fn remove_target_closes_only_forwarded_leases_for_that_pane() {
        let key = TerminalKey::new(KeyCode::Esc, KeyModifiers::empty());
        let removed_key = InputLeaseKey::new(7, &key);
        let retained_key = InputLeaseKey::new(8, &key);
        let removed_target = target();
        let retained_target = TerminalKeyTarget {
            workspace_id: "other".into(),
            pane_id: PaneId::from_raw(2),
        };
        let mut leases = InputLeaseTable::default();
        leases.insert_forwarded(removed_key, removed_target.clone(), key.clone());
        leases.insert_forwarded(retained_key, retained_target, key.clone());

        assert_eq!(
            leases.remove_target(&removed_target),
            vec![ForwardedInputLease {
                target: removed_target,
                key,
            }]
        );
        assert_eq!(leases.len(), 1);
        assert!(leases.contains(&retained_key));
    }

    #[test]
    fn duplicate_physical_press_normalizes_for_forwarded_and_consumed_leases() {
        let record = crate::input::WindowsKeyRecord {
            key_down: true,
            repeat_count: 1,
            virtual_key_code: 65,
            virtual_scan_code: 30,
            unicode: 97,
            control_key_state: 0,
        };
        let physical =
            TerminalKey::new(KeyCode::Char('a'), KeyModifiers::empty()).with_windows_record(record);
        let lease_key = InputLeaseKey::new(7, &physical);
        let mut leases = InputLeaseTable::default();

        assert_eq!(
            leases.normalize_press(&lease_key, physical.clone()).kind,
            crossterm::event::KeyEventKind::Press
        );
        leases.insert_consumed(lease_key, ConsumedInputLease::SuppressRepeats);
        assert_eq!(
            leases.normalize_press(&lease_key, physical.clone()).kind,
            crossterm::event::KeyEventKind::Repeat
        );
        leases.insert_forwarded(lease_key, target(), physical.clone());
        assert_eq!(
            leases.normalize_press(&lease_key, physical.clone()).kind,
            crossterm::event::KeyEventKind::Repeat
        );
    }

    #[test]
    fn new_semantic_press_recomputes_consumed_repeat_disposition() {
        let key = TerminalKey::new(KeyCode::Esc, KeyModifiers::empty()).with_repeat_count(3);
        let lease_key = InputLeaseKey::new(7, &key);
        let context = TerminalInputContext::Pane;
        let mut leases = InputLeaseTable::default();
        leases.insert_consumed(lease_key, ConsumedInputLease::SuppressRepeats);

        let key = leases.normalize_press(&lease_key, key);
        let plan = leases.complete_press(lease_key, &key, Some(&context), Some(&context), None);

        assert!(matches!(
            plan,
            RepeatPlan::Reprocess {
                context: TerminalInputContext::Pane,
                repetitions: 2,
                tracked: true,
            }
        ));
    }

    #[test]
    fn remove_forwarded_returns_press_target() {
        let key = TerminalKey::new(KeyCode::Esc, KeyModifiers::empty());
        let lease_key = InputLeaseKey::new(7, &key);
        let mut leases = InputLeaseTable::default();
        leases.insert_forwarded(lease_key, target(), key.clone());

        assert!(leases.contains(&lease_key));
        assert_eq!(leases.remove_forwarded(&lease_key).unwrap().key, key);
        assert!(leases.is_empty());
    }
}
