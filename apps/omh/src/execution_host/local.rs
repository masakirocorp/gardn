use std::ops::{Deref, DerefMut};

use crate::terminal::TerminalRuntimeRegistry;

use super::ExecutionHostId;

/// In-process execution adapter for the coordinator host.
///
/// Local terminal runtimes remain outside `AppState`; this adapter gives the
/// built-in Local execution host explicit ownership without changing the
/// terminal registry's rendering and lifecycle semantics.
pub(crate) struct LocalExecutionHost {
    id: ExecutionHostId,
    terminal_runtimes: TerminalRuntimeRegistry,
}

impl LocalExecutionHost {
    pub(crate) fn new(terminal_runtimes: TerminalRuntimeRegistry) -> Self {
        Self {
            id: ExecutionHostId::local(),
            terminal_runtimes,
        }
    }

    pub(crate) fn id(&self) -> &ExecutionHostId {
        &self.id
    }
}

impl Default for LocalExecutionHost {
    fn default() -> Self {
        Self::new(TerminalRuntimeRegistry::new())
    }
}

impl From<TerminalRuntimeRegistry> for LocalExecutionHost {
    fn from(terminal_runtimes: TerminalRuntimeRegistry) -> Self {
        Self::new(terminal_runtimes)
    }
}

impl Deref for LocalExecutionHost {
    type Target = TerminalRuntimeRegistry;

    fn deref(&self) -> &Self::Target {
        &self.terminal_runtimes
    }
}

impl DerefMut for LocalExecutionHost {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.terminal_runtimes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_adapter_owns_the_builtin_local_runtime_registry() {
        let mut host = LocalExecutionHost::default();
        let terminal_id = crate::terminal::TerminalId::alloc();
        let runtime =
            crate::terminal::TerminalRuntime::test_with_screen_bytes(80, 24, b"local shell");

        host.insert(terminal_id.clone(), runtime);

        assert!(host.id().is_local());
        assert_eq!(host.len(), 1);
        assert!(host
            .get(&terminal_id)
            .is_some_and(|runtime| runtime.visible_text().contains("local shell")));
    }
}
