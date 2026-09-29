//! Bounded output log for worker-owned terminal runtimes.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::execution_host::protocol::OutputRevision;

#[cfg(unix)]
#[derive(Clone)]
pub(super) struct OutputLog {
    inner: Arc<Mutex<OutputLogInner>>,
    limit_bytes: usize,
}

#[cfg(unix)]
#[derive(Default)]
struct OutputLogInner {
    revision: u64,
    retained_bytes: usize,
    chunks: VecDeque<(u64, u64, Vec<u8>)>,
}

#[cfg(unix)]
impl OutputLog {
    pub(super) fn new(limit_bytes: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(OutputLogInner::default())),
            limit_bytes: limit_bytes.max(1),
        }
    }

    pub(super) fn observer(&self) -> crate::pane::PaneOutputObserver {
        let inner = self.inner.clone();
        let limit_bytes = self.limit_bytes;
        Arc::new(move |bytes| {
            if bytes.is_empty() {
                return;
            }
            let Ok(mut log) = inner.lock() else {
                return;
            };
            let base = log.revision;
            log.revision = log.revision.saturating_add(1);
            let revision = log.revision;
            let bytes = bytes.to_vec();
            log.retained_bytes = log.retained_bytes.saturating_add(bytes.len());
            log.chunks.push_back((base, revision, bytes));
            while log.retained_bytes > limit_bytes {
                let Some((_, _, evicted)) = log.chunks.pop_front() else {
                    break;
                };
                log.retained_bytes = log.retained_bytes.saturating_sub(evicted.len());
            }
        })
    }

    pub(super) fn revision(&self) -> OutputRevision {
        let revision = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .revision;
        OutputRevision::new(revision)
    }

    pub(super) fn deltas_after(&self, revision: u64) -> Option<Vec<(u64, u64, Vec<u8>)>> {
        let log = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if revision > log.revision {
            return None;
        }
        if revision == log.revision {
            return Some(Vec::new());
        }
        let mut expected_base = revision;
        let mut deltas = Vec::new();
        for (base, current, data) in log
            .chunks
            .iter()
            .filter(|(_, current, _)| *current > revision)
        {
            if *base != expected_base {
                return None;
            }
            deltas.push((*base, *current, data.clone()));
            expected_base = *current;
        }
        (expected_base == log.revision).then_some(deltas)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::OutputLog;

    #[test]
    fn replay_requires_a_contiguous_retained_revision() {
        let output = OutputLog::new(6);
        let observe = output.observer();
        observe(b"ab");
        observe(b"cd");
        observe(b"efgh");

        assert_eq!(output.deltas_after(0), None);
        assert_eq!(
            output.deltas_after(1),
            Some(vec![(1, 2, b"cd".to_vec()), (2, 3, b"efgh".to_vec())])
        );
        assert_eq!(output.deltas_after(3), Some(Vec::new()));
        assert_eq!(output.deltas_after(4), None);
    }
}
