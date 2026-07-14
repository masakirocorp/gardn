use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::api::schema::{
    EventData, EventEnvelope, EventKind, ResponseResult, WorktreeCreateParams, WorktreeRemoveParams,
};
use crate::app::App;
use crate::events::{
    ApiWorktreeAddRequest, ApiWorktreeRemoveRequest, AppEvent, WorktreeAddResult,
    WorktreeRemoveResult,
};

use super::super::responses::{encode_error, encode_success};
use super::{absolute_user_path, WorktreeSource};

impl App {
    pub(crate) fn handle_deferred_worktree_api_request(
        &mut self,
        request: crate::api::schema::Request,
        respond_to: std::sync::mpsc::Sender<String>,
    ) -> bool {
        match request.method {
            crate::api::schema::Method::WorktreeCreate(params) => {
                self.start_api_worktree_create(request.id, params, respond_to);
                true
            }
            crate::api::schema::Method::WorktreeRemove(params) => {
                self.start_api_worktree_remove(request.id, params, respond_to);
                true
            }
            _ => false,
        }
    }

    fn next_api_worktree_operation_id(&mut self) -> u64 {
        let id = self.next_api_worktree_operation_id;
        self.next_api_worktree_operation_id = self.next_api_worktree_operation_id.saturating_add(1);
        id
    }

    fn start_api_worktree_create(
        &mut self,
        id: String,
        params: WorktreeCreateParams,
        respond_to: std::sync::mpsc::Sender<String>,
    ) {
        let branch = params
            .branch
            .unwrap_or_else(|| {
                let seed = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|duration| duration.as_micros().min(u128::from(u64::MAX)) as u64)
                    .unwrap_or(0);
                crate::worktree::generated_branch_slug(seed)
            })
            .trim()
            .to_string();
        if branch.is_empty() {
            let _ = respond_to.send(encode_error(id, "invalid_request", "branch is required"));
            return;
        }
        let base = params.base.unwrap_or_else(|| "HEAD".into());
        let source = match self.resolve_worktree_source(params.workspace_id, params.cwd) {
            Ok(source) => source,
            Err(err) => {
                let _ = respond_to.send(encode_error(id, err.code, err.message));
                return;
            }
        };
        let checkout_path = match params.path {
            Some(path) => match absolute_user_path(&path) {
                Ok(path) => path,
                Err(err) => {
                    let _ = respond_to.send(encode_error(id, err.code, err.message));
                    return;
                }
            },
            None => crate::worktree::default_checkout_path(
                &self.state.worktree_directory,
                &source.repo_name,
                &branch,
            ),
        };
        let checkout_key = crate::worktree::canonical_or_original(&checkout_path);
        if self
            .pending_api_worktree_creates
            .contains_key(&checkout_key)
            || self
                .pending_api_worktree_remove_paths
                .contains_key(&checkout_key)
        {
            let _ = respond_to.send(encode_error(
                id,
                "worktree_operation_in_progress",
                "another worktree operation is already in progress for this checkout",
            ));
            return;
        }
        let operation_id = self.next_api_worktree_operation_id();
        self.pending_api_worktree_creates
            .insert(checkout_key.clone(), operation_id);
        let source_workspace_id = source
            .workspace_idx
            .and_then(|idx| self.state.workspaces.get(idx))
            .map(|ws| ws.id.clone());
        let api_request = ApiWorktreeAddRequest {
            id,
            source_workspace_id,
            checkout_key,
            source_checkout_path: source.source_checkout_path.clone(),
            source_repo_root: source.source_repo_root.clone(),
            repo_key: source.repo_key.clone(),
            repo_name: source.repo_name.clone(),
            label: params.label,
            focus: params.focus,
            respond_to,
        };
        let parent_dir = checkout_path.parent().map(Path::to_path_buf);
        let source_checkout_path = source.source_checkout_path;
        let event_tx = self.event_tx.clone();
        std::thread::spawn(move || {
            let result = parent_dir
                .map(|parent| std::fs::create_dir_all(parent).map_err(|err| err.to_string()))
                .unwrap_or(Ok(()))
                .and_then(|()| {
                    crate::worktree::run_worktree_add_command(
                        &source_checkout_path,
                        &checkout_path,
                        &branch,
                        &base,
                    )
                });
            let _ = event_tx.blocking_send(AppEvent::WorktreeAddFinished(Box::new(
                WorktreeAddResult {
                    path: checkout_path,
                    api_request: Some(api_request),
                    result,
                },
            )));
        });
    }

    fn start_api_worktree_remove(
        &mut self,
        id: String,
        params: WorktreeRemoveParams,
        respond_to: std::sync::mpsc::Sender<String>,
    ) {
        let Some(ws_idx) = self.parse_workspace_id(&params.workspace_id) else {
            let _ = respond_to.send(encode_error(
                id,
                "workspace_not_found",
                format!("workspace {} not found", params.workspace_id),
            ));
            return;
        };
        let Some(space) = self
            .state
            .workspaces
            .get(ws_idx)
            .and_then(|ws| ws.worktree_space().cloned())
        else {
            let _ = respond_to.send(encode_error(
                id,
                "not_linked_worktree",
                "workspace is not a Hako-managed worktree checkout",
            ));
            return;
        };
        if !space.is_linked_worktree {
            let _ = respond_to.send(encode_error(
                id,
                "not_linked_worktree",
                "workspace is not a linked worktree checkout",
            ));
            return;
        }
        let checkout_key = crate::worktree::canonical_or_original(&space.checkout_path);
        if self
            .pending_api_worktree_removes
            .contains_key(&params.workspace_id)
            || self
                .pending_api_worktree_creates
                .contains_key(&checkout_key)
            || self
                .pending_api_worktree_remove_paths
                .contains_key(&checkout_key)
        {
            let _ = respond_to.send(encode_error(
                id,
                "worktree_operation_in_progress",
                "another worktree operation is already in progress for this checkout",
            ));
            return;
        }
        let operation_id = self.next_api_worktree_operation_id();
        self.pending_api_worktree_removes
            .insert(params.workspace_id.clone(), operation_id);
        self.pending_api_worktree_remove_paths
            .insert(checkout_key.clone(), operation_id);
        let api_request = ApiWorktreeRemoveRequest {
            id,
            checkout_key,
            respond_to,
        };
        let workspace_id = self.public_workspace_id(ws_idx);
        let workspace = self.workspace_info(ws_idx);
        let worktree = self.worktree_info_for_membership(&space, None);
        let path = space.checkout_path.clone();
        let repo_root = space.repo_root.clone();
        let forced = params.force;
        let event_tx = self.event_tx.clone();
        std::thread::spawn(move || {
            let command = crate::worktree::build_worktree_remove_command(&repo_root, &path, forced);
            let result = crate::worktree::run_worktree_command(&command);
            let _ = event_tx.blocking_send(AppEvent::WorktreeRemoveFinished(Box::new(
                WorktreeRemoveResult {
                    workspace_id,
                    path,
                    workspace: Some(workspace),
                    worktree: Box::new(worktree),
                    forced,
                    api_request: Some(api_request),
                    result,
                },
            )));
        });
    }

    pub(crate) fn handle_api_worktree_add_finished(&mut self, result: WorktreeAddResult) {
        let Some(api) = result.api_request else {
            return;
        };
        self.pending_api_worktree_creates.remove(&api.checkout_key);
        if let Err(err) = result.result {
            let _ = api
                .respond_to
                .send(encode_error(api.id, "worktree_create_failed", err));
            return;
        }
        let mut source = WorktreeSource {
            workspace_idx: api
                .source_workspace_id
                .as_ref()
                .and_then(|id| self.state.workspaces.iter().position(|ws| &ws.id == id)),
            source_checkout_path: api.source_checkout_path,
            source_repo_root: api.source_repo_root,
            repo_key: api.repo_key,
            repo_name: api.repo_name,
        };
        if let Err(err) = self.ensure_source_parent_membership(&mut source, true) {
            let _ = api
                .respond_to
                .send(encode_error(api.id, err.code, err.message));
            return;
        }
        let ws_idx = match self.create_workspace_with_options(result.path.clone(), api.focus) {
            Ok(ws_idx) => ws_idx,
            Err(err) => {
                let _ = api.respond_to.send(encode_error(
                    api.id,
                    "worktree_open_failed",
                    format!("created worktree but failed to open workspace: {err}"),
                ));
                return;
            }
        };
        self.mark_worktree_membership(&source, ws_idx, result.path, true, false);
        if let Some(label) = api.label {
            if let Some(ws) = self.state.workspaces.get_mut(ws_idx) {
                ws.set_custom_name(label);
            }
        }
        self.state.mark_session_dirty();
        self.emit_workspace_open_events(ws_idx);
        let worktree = self
            .worktree_info_for_checkout(&source, ws_idx)
            .expect("created worktree workspace should have worktree info");
        self.emit_event(EventEnvelope {
            event: EventKind::WorktreeCreated,
            data: EventData::WorktreeCreated {
                workspace: self.workspace_info(ws_idx),
                worktree: worktree.clone(),
            },
        });
        let response = encode_success(
            api.id,
            ResponseResult::WorktreeCreated {
                workspace: self.workspace_info(ws_idx),
                tab: self.tab_info(ws_idx, 0).expect("new worktree tab"),
                root_pane: self.root_pane_info(ws_idx, 0).expect("new worktree pane"),
                worktree,
            },
        );
        let _ = api.respond_to.send(response);
    }

    pub(crate) fn handle_api_worktree_remove_finished(&mut self, result: WorktreeRemoveResult) {
        let Some(api) = result.api_request else {
            return;
        };
        self.pending_api_worktree_removes
            .remove(&result.workspace_id);
        self.pending_api_worktree_remove_paths
            .remove(&api.checkout_key);
        if let Err(err) = result.result {
            let code = if !result.forced && crate::worktree::is_dirty_worktree_remove_error(&err) {
                "dirty_worktree_requires_force"
            } else {
                "worktree_remove_failed"
            };
            let _ = api.respond_to.send(encode_error(api.id, code, err));
            return;
        }
        let Some(ws_idx) = self.parse_workspace_id(&result.workspace_id) else {
            let _ = api.respond_to.send(encode_error(
                api.id,
                "workspace_not_found",
                "worktree workspace disappeared before completion",
            ));
            return;
        };
        let still_same = self.state.workspaces[ws_idx]
            .worktree_space()
            .is_some_and(|current| {
                current.is_linked_worktree && current.checkout_path == result.path
            });
        if still_same {
            self.close_removed_linked_worktree_workspace(ws_idx);
            self.shutdown_detached_terminal_runtimes();
            self.emit_event(EventEnvelope {
                event: EventKind::WorkspaceClosed,
                data: EventData::WorkspaceClosed {
                    workspace_id: result.workspace_id.clone(),
                    workspace: result.workspace,
                },
            });
        }
        self.emit_event(EventEnvelope {
            event: EventKind::WorktreeRemoved,
            data: EventData::WorktreeRemoved {
                workspace_id: result.workspace_id.clone(),
                worktree: *result.worktree,
                forced: result.forced,
            },
        });
        let response = encode_success(
            api.id,
            ResponseResult::WorktreeRemoved {
                workspace_id: result.workspace_id,
                path: result.path.display().to_string(),
                forced: result.forced,
            },
        );
        let _ = api.respond_to.send(response);
    }
}
