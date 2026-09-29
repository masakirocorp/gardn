use super::{
    receipts::wait_for_connection_receipt,
    storage::*,
    worker::{self, Event, Message, Reporter},
};
use gardn_local_api::sprites::{
    SpriteCommand, SpriteConnection, SpriteError, SpriteOperation, SpriteOperationStatus,
    SpriteReply, SpriteRequest, SpriteResult, SpriteSnapshot, SpritesConfig,
};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
};
use tokio::sync::mpsc::Sender;

/// Construction is inert. Only explicit enable loads state; only requests contact the provider.
pub(crate) struct SpritesRuntime {
    config: SpritesConfig,
    snapshot: SpriteSnapshot,
    root: Option<PathBuf>,
    tx: mpsc::Sender<Message>,
    rx: mpsc::Receiver<Message>,
    controls: HashMap<String, Arc<AtomicBool>>,
    epochs: HashMap<String, u64>,
    owned: HashSet<String>,
    presentations: HashSet<String>,
    queued: VecDeque<String>,
    completed: Vec<SpriteOperation>,
    waker: Option<Sender<crate::events::AppEvent>>,
    sequence: u64,
}
impl SpritesRuntime {
    pub(crate) fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            config: SpritesConfig::default(),
            snapshot: SpriteSnapshot::default(),
            root: None,
            tx,
            rx,
            controls: HashMap::new(),
            epochs: HashMap::new(),
            owned: HashSet::new(),
            presentations: HashSet::new(),
            queued: VecDeque::new(),
            completed: Vec::new(),
            waker: None,
            sequence: 0,
        }
    }
    pub(crate) fn set_waker(&mut self, waker: Sender<crate::events::AppEvent>) {
        self.waker = Some(waker);
    }
    pub(crate) fn snapshot(&self) -> &SpriteSnapshot {
        &self.snapshot
    }
    pub(crate) fn take_completed(&mut self) -> Vec<SpriteOperation> {
        std::mem::take(&mut self.completed)
    }
    pub(crate) fn has_pending_operations(&self) -> bool {
        !self.controls.is_empty() || !self.queued.is_empty() || !self.presentations.is_empty()
    }

    pub(crate) fn configure(&mut self, config: SpritesConfig) -> Result<(), SpriteError> {
        if !config.enabled {
            // The fence precedes cancellation, so a worker cannot start its next provider command.
            let fence = self
                .root
                .as_deref()
                .map(|root| write_backend_fence(root, &config))
                .transpose();
            for control in self.controls.values() {
                control.store(true, Ordering::Release);
            }
            let mut failure = fence.err();
            if let Some(root) = &self.root {
                for operation in &mut self.snapshot.operations {
                    if self.owned.contains(&operation.id) && !is_terminal(&operation.status) {
                        operation.status = SpriteOperationStatus::Interrupted;
                        operation.stage = "interrupted".into();
                        operation.updated_unix_ms = unix_ms();
                        operation.error = Some(error("disabled", "Sprites disabled during this operation; remote work was not stopped or destroyed", true));
                        if let Err(cause) = persist_operation(root, operation) {
                            failure.get_or_insert(cause);
                        }
                        if !self.controls.contains_key(&operation.id) {
                            remove_lease(root, &operation.id);
                        }
                    }
                }
            }
            self.config = config;
            self.snapshot = SpriteSnapshot::default();
            self.queued.clear();
            self.completed.clear();
            self.presentations.clear();
            self.owned.retain(|id| self.controls.contains_key(id));
            // Retain root and controls until their workers acknowledge cancellation.
            return failure.map_or(Ok(()), Err);
        }
        crate::config::validate_sprites_config(&config)
            .map_err(|message| error("invalid_config", message, false))?;
        let root = self
            .root
            .clone()
            .unwrap_or_else(|| crate::config::state_dir().join("sprites"));
        if !self.config.enabled {
            let _lock = lock_intents(&root)?;
            self.snapshot.operations = load_operations(&root)?;
            self.snapshot.resources = load_resources(&root)?;
        }
        write_backend_fence(&root, &config)?;
        self.root = Some(root);
        self.config = config;
        self.snapshot.enabled = true;
        Ok(())
    }

    pub(crate) fn submit(
        &mut self,
        mut request: SpriteRequest,
    ) -> Result<SpriteReply, SpriteError> {
        if !self.config.enabled {
            return Err(error("disabled", "Sprites are disabled", false));
        }
        match &request.command {
            SpriteCommand::List { refresh: false } => {
                return Ok(SpriteReply::Snapshot(self.snapshot.clone()))
            }
            SpriteCommand::Operation { operation_id } => {
                return self
                    .observe_operation(operation_id)
                    .map(|operation| SpriteReply::Operation(Box::new(operation)))
            }
            SpriteCommand::Retry { operation_id } => return self.retry(operation_id),
            SpriteCommand::Cancel { operation_id } => return self.cancel(operation_id),
            _ => {}
        }
        if request.request_id.trim().is_empty() {
            self.sequence = self.sequence.wrapping_add(1);
            request.request_id = format!(
                "request-{}-{}-{}",
                std::process::id(),
                unix_ms(),
                self.sequence
            );
        }
        let root = self.storage()?.to_owned();
        let _lock = lock_intents(&root)?;
        // The request key determines its durable identity across processes and lost responses.
        let id = format!("operation-{}", hash_id(&request.request_id));
        if let Some(existing) = read_operation(&root, &id)? {
            if existing.request != request {
                return Err(error(
                    "request_conflict",
                    "This request_id already identifies a different Sprites request",
                    false,
                ));
            }
            let existing = if self.owned.contains(&id) {
                self.find(&id).cloned().unwrap_or(existing)
            } else {
                refresh_operation(&root, existing)?
            };
            self.replace(existing.clone());
            return Ok(SpriteReply::Operation(Box::new(existing)));
        }
        let now = unix_ms();
        let operation = SpriteOperation {
            id,
            request,
            status: SpriteOperationStatus::Queued,
            stage: "queued".into(),
            created_unix_ms: now,
            updated_unix_ms: now,
            result: None,
            error: None,
        };
        self.admit(&root, operation.clone())?;
        drop(_lock);
        self.start_queued();
        Ok(SpriteReply::Operation(Box::new(operation)))
    }

    fn observe_operation(&mut self, id: &str) -> Result<SpriteOperation, SpriteError> {
        if self.owned.contains(id) {
            if let Some(operation) = self.find(id) {
                return Ok(operation.clone());
            }
        }
        let root = self.storage()?;
        let _lock = lock_intents(root)?;
        let operation = read_operation(root, id)?.ok_or_else(|| {
            error(
                "operation_not_found",
                "The requested Sprites operation was not found",
                false,
            )
        })?;
        let operation = refresh_operation(root, operation)?;
        // Observing another session's completed operation must never launch a terminal here.
        self.replace(operation.clone());
        Ok(operation)
    }

    fn retry(&mut self, id: &str) -> Result<SpriteReply, SpriteError> {
        if self.controls.contains_key(id) {
            return Err(error(
                "operation_busy",
                "The previous worker is still exiting; retry after cancellation completes",
                true,
            ));
        }
        let root = self.storage()?.to_owned();
        let _lock = lock_intents(&root)?;
        let mut operation = read_operation(&root, id)?.ok_or_else(|| {
            error(
                "operation_not_found",
                "The requested Sprites operation was not found",
                false,
            )
        })?;
        operation = refresh_operation(&root, operation)?;
        if !is_terminal(&operation.status) {
            return Ok(SpriteReply::Operation(Box::new(operation)));
        }
        if !matches!(
            operation.status,
            SpriteOperationStatus::Failed | SpriteOperationStatus::Interrupted
        ) {
            return Err(error(
                "operation_not_retryable",
                "Only failed or interrupted Sprites operations can be retried",
                false,
            ));
        }
        if has_live_owner(&root, id)? && !self.owned.contains(id) {
            return Err(error(
                "operation_busy",
                "The operation is still owned by another Gardn session",
                true,
            ));
        }
        operation.status = SpriteOperationStatus::Queued;
        operation.stage = "retry_queued".into();
        operation.updated_unix_ms = unix_ms();
        operation.result = None;
        operation.error = None;
        self.presentations.remove(id);
        self.completed.retain(|item| item.id != id);
        self.admit(&root, operation.clone())?;
        drop(_lock);
        self.start_queued();
        Ok(SpriteReply::Operation(Box::new(operation)))
    }

    fn admit(&mut self, root: &Path, operation: SpriteOperation) -> Result<(), SpriteError> {
        persist_lease(root, &operation.id)?;
        if let Err(failure) = persist_operation(root, &operation) {
            remove_lease(root, &operation.id);
            return Err(failure);
        }
        self.owned.insert(operation.id.clone());
        self.queued.push_back(operation.id.clone());
        self.replace(operation);
        Ok(())
    }

    fn cancel(&mut self, id: &str) -> Result<SpriteReply, SpriteError> {
        let mut operation = self.observe_operation(id)?;
        if !self.owned.contains(id) || is_terminal(&operation.status) {
            return Err(error(
                "operation_not_running",
                "This Gardn session does not own a running operation with that ID",
                false,
            ));
        }
        if self.presentations.contains(id) {
            let child_key = format!("{id}:launch");
            if let Some(child_id) = self
                .snapshot
                .operations
                .iter()
                .find(|item| item.request.request_id == child_key)
                .map(|item| item.id.clone())
            {
                self.cancel(&child_id)?;
            }
        }
        if let Some(control) = self.controls.get(id) {
            control.store(true, Ordering::Release);
        }
        self.queued.retain(|queued| queued != id);
        operation.updated_unix_ms = unix_ms();
        operation.status = SpriteOperationStatus::Interrupted;
        operation.stage = if matches!(operation.result, Some(SpriteResult::Connection(_))) {
            "connection_canceled"
        } else {
            "canceled"
        }
        .into();
        operation.error = Some(error(
            "canceled",
            "Canceled locally; remote session and resource state were not cleaned up",
            true,
        ));
        self.record(operation.clone(), true)?;
        if !self.controls.contains_key(id) {
            self.release(id);
        }
        Ok(SpriteReply::Operation(Box::new(operation)))
    }

    pub(crate) fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Ok(message) = self.rx.try_recv() {
            if self.epochs.get(&message.id) != Some(&message.epoch) {
                continue;
            }
            changed = true;
            match message.event {
                Event::Progress(operation) if self.config.enabled => {
                    if self
                        .controls
                        .get(&message.id)
                        .is_some_and(|control| !control.load(Ordering::Acquire))
                    {
                        self.replace(operation);
                    }
                }
                Event::Resources(resources) if self.config.enabled => {
                    self.snapshot.resources = resources;
                }
                Event::Snapshot(snapshot) if self.config.enabled => {
                    self.snapshot.resources = snapshot.resources;
                    if self.find(&message.id).is_some_and(|operation| {
                        matches!(
                            operation.request.command,
                            SpriteCommand::List { refresh: true }
                        )
                    }) {
                        self.snapshot.observation_error = snapshot.observation_error;
                        if snapshot.observed_unix_ms.is_some() {
                            self.snapshot.observed_unix_ms = snapshot.observed_unix_ms;
                        }
                    }
                }
                Event::Finished(result) => {
                    let canceled = self
                        .controls
                        .remove(&message.id)
                        .is_some_and(|control| control.load(Ordering::Acquire));
                    if !self.config.enabled {
                        self.release(&message.id);
                        continue;
                    }
                    let Some(mut operation) = self.find(&message.id).cloned() else {
                        self.release(&message.id);
                        continue;
                    };
                    if canceled {
                        // Preserve explicit cancellation (and its pane identity), not late success.
                        if !is_terminal(&operation.status) {
                            operation.status = SpriteOperationStatus::Interrupted;
                            operation.stage = "canceled".into();
                            operation.error = Some(error(
                                "canceled",
                                "Sprites operation canceled locally",
                                true,
                            ));
                        }
                    } else {
                        match result {
                            Ok(finished) => operation = finished,
                            Err(failure) => {
                                operation.status = SpriteOperationStatus::Failed;
                                operation.stage = if operation.stage == "awaiting_connection" {
                                    "connection_failed"
                                } else {
                                    "failed"
                                }
                                .into();
                                operation.error = Some(failure);
                                operation.result = None;
                            }
                        }
                    }
                    operation.updated_unix_ms = unix_ms();
                    let needs_presentation = !canceled && needs_presentation(&operation);
                    if let Err(failure) = self.record(operation.clone(), true) {
                        operation.status = SpriteOperationStatus::Failed;
                        operation.stage = "persistence_failed".into();
                        operation.result = None;
                        operation.error = Some(failure);
                        self.replace(operation.clone());
                        self.completed.retain(|item| item.id != operation.id);
                        self.completed.push(operation);
                        self.release(&message.id);
                    } else if !needs_presentation {
                        self.release(&message.id);
                    }
                }
                _ => {}
            }
        }
        if self.config.enabled {
            self.start_queued();
        }
        changed
    }

    pub(crate) fn begin_presentation(&mut self, id: &str, stage: &str) -> Result<(), SpriteError> {
        let mut operation = self.find(id).cloned().ok_or_else(|| {
            error(
                "operation_not_found",
                "Sprites presentation operation was not found",
                false,
            )
        })?;
        operation.status = SpriteOperationStatus::Running;
        operation.stage = stage.into();
        operation.updated_unix_ms = unix_ms();
        persist_lease(self.storage()?, id)?;
        self.record(operation, false)?;
        self.owned.insert(id.into());
        self.presentations.insert(id.into());
        self.completed.retain(|item| item.id != id);
        Ok(())
    }
    pub(crate) fn finish_presentation(
        &mut self,
        id: &str,
        result: Result<SpriteResult, SpriteError>,
    ) -> Result<(), SpriteError> {
        let mut operation = self.find(id).cloned().ok_or_else(|| {
            error(
                "operation_not_found",
                "Sprites presentation operation was not found",
                false,
            )
        })?;
        match result {
            Ok(result) => {
                operation.status = SpriteOperationStatus::Succeeded;
                operation.stage = "presentation_succeeded".into();
                operation.result = Some(result);
                operation.error = None;
            }
            Err(failure) => {
                operation.status = SpriteOperationStatus::Failed;
                operation.stage = "presentation_failed".into();
                operation.result = None;
                operation.error = Some(failure);
            }
        }
        operation.updated_unix_ms = unix_ms();
        self.record(operation, false)?;
        self.completed.retain(|item| item.id != id);
        self.release(id);
        Ok(())
    }
    pub(crate) fn watch_connection(
        &mut self,
        id: &str,
        connection: &SpriteConnection,
    ) -> Result<(), SpriteError> {
        if self.controls.contains_key(id) {
            return Err(error(
                "operation_busy",
                "Sprites operation already has an active observer",
                true,
            ));
        }
        self.begin_presentation(id, "awaiting_connection")?;
        let mut operation = self.find(id).cloned().ok_or_else(|| {
            error(
                "operation_not_found",
                "Sprites connection operation was not found",
                false,
            )
        })?;
        operation.result = Some(SpriteResult::Connection(connection.clone()));
        self.record(operation.clone(), false)?;
        let root = self.storage()?.to_owned();
        let connection = connection.clone();
        let (canceled, reporter) = self.observer(id);
        thread::spawn(move || {
            let result = wait_for_connection_receipt(
                &root,
                &operation.id,
                &operation.request,
                &connection,
                &canceled,
            )
            .and_then(|(connection, stage)| {
                reporter.send(Event::Resources(load_resources(&root)?));
                operation.status = SpriteOperationStatus::Succeeded;
                operation.stage = stage.into();
                operation.result = Some(SpriteResult::Connection(connection));
                operation.error = None;
                Ok(operation)
            });
            reporter.send(Event::Finished(result));
        });
        Ok(())
    }
    fn observer(&mut self, id: &str) -> (Arc<AtomicBool>, Reporter) {
        let epoch = self.epochs.entry(id.into()).or_default();
        *epoch = epoch.wrapping_add(1);
        let reporter = Reporter {
            id: id.into(),
            epoch: *epoch,
            tx: self.tx.clone(),
            waker: self.waker.clone(),
        };
        let canceled = Arc::new(AtomicBool::new(false));
        self.controls.insert(id.into(), Arc::clone(&canceled));
        (canceled, reporter)
    }
    fn start_queued(&mut self) {
        while self.controls.len() < self.config.max_concurrent_operations {
            let Some(id) = self.queued.pop_front() else {
                break;
            };
            let Some(mut operation) = self
                .find(&id)
                .filter(|operation| operation.status == SpriteOperationStatus::Queued)
                .cloned()
            else {
                continue;
            };
            let Some(root) = self.root.clone() else {
                break;
            };
            operation.status = SpriteOperationStatus::Running;
            operation.stage = "starting_worker".into();
            operation.updated_unix_ms = unix_ms();
            if let Err(failure) = self.record(operation.clone(), false) {
                operation.status = SpriteOperationStatus::Failed;
                operation.stage = "persistence_failed".into();
                operation.error = Some(failure);
                self.replace(operation.clone());
                self.completed.push(operation);
                self.release(&id);
                continue;
            }
            let (canceled, reporter) = self.observer(&id);
            let config = self.config.clone();
            thread::spawn(move || worker::run(root, config, operation, canceled, reporter));
        }
    }
    fn storage(&self) -> Result<&Path, SpriteError> {
        self.root.as_deref().ok_or_else(|| {
            error(
                "storage_unavailable",
                "Sprites storage is unavailable",
                true,
            )
        })
    }
    fn find(&self, id: &str) -> Option<&SpriteOperation> {
        self.snapshot
            .operations
            .iter()
            .find(|operation| operation.id == id)
    }
    fn replace(&mut self, operation: SpriteOperation) {
        if let Some(existing) = self
            .snapshot
            .operations
            .iter_mut()
            .find(|item| item.id == operation.id)
        {
            *existing = operation;
        } else {
            self.snapshot.operations.push(operation);
        }
    }
    fn record(&mut self, operation: SpriteOperation, complete: bool) -> Result<(), SpriteError> {
        persist_operation(self.storage()?, &operation)?;
        self.replace(operation.clone());
        if complete {
            self.completed.retain(|item| item.id != operation.id);
            self.completed.push(operation);
        }
        Ok(())
    }
    fn release(&mut self, id: &str) {
        if let Some(root) = &self.root {
            remove_lease(root, id);
        }
        self.owned.remove(id);
        self.presentations.remove(id);
    }
}
impl Drop for SpritesRuntime {
    fn drop(&mut self) {
        for canceled in self.controls.values() {
            canceled.store(true, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "gardn-sprites-runtime-{}-{nonce}",
                std::process::id()
            ));
            std::fs::create_dir(&path).expect("temporary directory");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn enabled_runtime(root: &Path) -> SpritesRuntime {
        let mut runtime = SpritesRuntime::new();
        runtime.root = Some(root.to_owned());
        runtime
            .configure(SpritesConfig {
                enabled: true,
                org: "test-org".into(),
                node_bin: "gardn-sprites-test-no-such-node".into(),
                ..SpritesConfig::default()
            })
            .expect("enable isolated integration");
        runtime
    }

    fn refresh(key: &str) -> SpriteRequest {
        SpriteRequest {
            request_id: key.into(),
            command: SpriteCommand::List { refresh: true },
            open_in_workspace: None,
            focus: false,
        }
    }

    fn submitted(runtime: &mut SpritesRuntime, request: SpriteRequest) -> SpriteOperation {
        let SpriteReply::Operation(operation) = runtime.submit(request).expect("submit operation")
        else {
            panic!("expected operation reply");
        };
        *operation
    }

    fn await_failure(runtime: &mut SpritesRuntime, id: &str) -> SpriteOperation {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            runtime.poll();
            let operation = submitted(
                runtime,
                SpriteRequest {
                    request_id: String::new(),
                    command: SpriteCommand::Operation {
                        operation_id: id.into(),
                    },
                    open_in_workspace: None,
                    focus: false,
                },
            );
            if is_terminal(&operation.status) {
                return operation;
            }
            assert!(
                Instant::now() < deadline,
                "operation did not finish: {operation:?}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn sprites_can_be_disabled_with_invalid_provider_settings() {
        let mut runtime = SpritesRuntime::new();
        runtime
            .configure(SpritesConfig {
                enabled: false,
                org: "\n".into(),
                max_concurrent_operations: 0,
                ..SpritesConfig::default()
            })
            .expect("disabled integrations do not require valid provider settings");
        let failure = runtime
            .submit(refresh("off"))
            .expect_err("disabled request");
        assert_eq!(failure.code, "disabled");
        assert!(!runtime.snapshot().enabled);
    }

    #[test]
    fn sprites_lost_response_reuses_intent_across_coordinators() {
        let directory = TestDirectory::new();
        let mut first = enabled_runtime(directory.path());
        let operation = submitted(&mut first, refresh("stable-key"));
        let mut second = enabled_runtime(directory.path());
        let duplicate = submitted(&mut second, refresh("stable-key"));
        assert_eq!(duplicate.id, operation.id);
        let mut conflicting = refresh("stable-key");
        conflicting.command = SpriteCommand::Inspect(gardn_local_api::sprites::SpriteTarget {
            sprite_id: "test-org/exact-name".into(),
            session_id: None,
            checkpoint_id: None,
            approval: None,
        });
        assert_eq!(
            second.submit(conflicting).expect_err("key conflict").code,
            "request_conflict"
        );
        let failed = await_failure(&mut first, &operation.id);
        assert_eq!(
            failed.error.as_ref().expect("worker failure").code,
            "worker_spawn"
        );
        let observed = submitted(
            &mut second,
            SpriteRequest {
                request_id: String::new(),
                command: SpriteCommand::Operation {
                    operation_id: operation.id,
                },
                open_in_workspace: None,
                focus: false,
            },
        );
        assert_eq!(observed.status, SpriteOperationStatus::Failed);
        assert!(
            second.take_completed().is_empty(),
            "observation must not replay another coordinator's presentation"
        );
    }

    #[test]
    fn sprites_retry_runs_failed_intent_without_changing_identity() {
        let directory = TestDirectory::new();
        let mut runtime = enabled_runtime(directory.path());
        let original = submitted(&mut runtime, refresh("retry-key"));
        let failed = await_failure(&mut runtime, &original.id);
        assert_eq!(
            failed.error.as_ref().expect("spawn failure").code,
            "worker_spawn"
        );
        runtime.take_completed();
        let retried = submitted(
            &mut runtime,
            SpriteRequest {
                request_id: String::new(),
                command: SpriteCommand::Retry {
                    operation_id: original.id.clone(),
                },
                open_in_workspace: None,
                focus: false,
            },
        );
        assert_eq!(retried.id, original.id);
        assert_eq!(retried.request, original.request);
        assert_eq!(retried.status, SpriteOperationStatus::Queued);
        let failed_again = await_failure(&mut runtime, &original.id);
        assert_eq!(
            failed_again
                .error
                .as_ref()
                .expect("retried worker failure")
                .code,
            "worker_spawn"
        );
        assert_eq!(runtime.take_completed().len(), 1);
    }

    #[test]
    fn sprites_provider_failure_preserves_its_structured_cause() {
        let directory = TestDirectory::new();
        let mut runtime = enabled_runtime(directory.path());
        runtime
            .configure(SpritesConfig {
                enabled: true,
                org: "test-org".into(),
                node_bin: "node".into(),
                sprite_bin: directory
                    .path()
                    .join("missing-provider")
                    .to_string_lossy()
                    .into_owned(),
                ..SpritesConfig::default()
            })
            .expect("configure missing provider");
        let operation = submitted(&mut runtime, refresh("missing-provider"));
        let failed = await_failure(&mut runtime, &operation.id);
        assert_eq!(
            failed.error.as_ref().expect("provider error").code,
            "provider_unavailable"
        );
    }

    #[test]
    fn sprites_disable_relinquishes_queued_intent_ownership() {
        let directory = TestDirectory::new();
        let mut first = enabled_runtime(directory.path());
        first
            .configure(SpritesConfig {
                max_concurrent_operations: 1,
                ..first.config.clone()
            })
            .expect("one worker slot");
        submitted(&mut first, refresh("occupy-slot"));
        let queued = submitted(&mut first, refresh("transfer-ownership"));
        first
            .configure(SpritesConfig::default())
            .expect("disable first coordinator");
        let mut second = enabled_runtime(directory.path());
        let retried = submitted(
            &mut second,
            SpriteRequest {
                request_id: String::new(),
                command: SpriteCommand::Retry {
                    operation_id: queued.id.clone(),
                },
                open_in_workspace: None,
                focus: false,
            },
        );
        first
            .configure(second.config.clone())
            .expect("reenable first coordinator");
        let failure = first
            .submit(SpriteRequest {
                request_id: String::new(),
                command: SpriteCommand::Cancel {
                    operation_id: queued.id,
                },
                open_in_workspace: None,
                focus: false,
            })
            .expect_err("old owner must not cancel the new owner's operation");
        assert_eq!(failure.code, "operation_not_running");
        assert_eq!(
            await_failure(&mut second, &retried.id)
                .error
                .expect("worker error")
                .code,
            "worker_spawn"
        );
        first.poll();
    }
}
