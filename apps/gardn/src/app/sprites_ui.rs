use ratatui::layout::Rect;
static NEXT_SPRITE_UI_REQUEST_ID: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);
use crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};

use crate::api::schema::{
    SpriteAgent, SpriteCommand, SpriteCreateParams, SpriteRequest, SpriteSource, SpriteTarget,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SpriteUiScreen {
    #[default]
    Manager,
    Create,
    Approval,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SpriteManagerPrompt {
    Search,
    Resume {
        reference: String,
    },
    SessionPicker {
        action: SpriteSessionAction,
        presentation_workspace_id: Option<String>,
    },
    CheckpointPicker {
        target: SpriteTarget,
        operation_id: String,
    },
    OperationPicker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpriteSessionAction {
    Connect,
    Stop,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SpriteUiState {
    pub(crate) screen: SpriteUiScreen,
    pub(crate) manager_prompt: Option<SpriteManagerPrompt>,
    pub(crate) search: String,
    pub(crate) detail_scroll: u16,
    pub(crate) checkpoints: Vec<String>,
    pub(crate) selected_checkpoint: Option<(String, String)>,
    pub(crate) operation_choice: usize,
    pub(crate) selected: usize,
    pub(crate) field: usize,
    pub(crate) workspace_id: Option<String>,
    pub(crate) source_host_id: String,
    pub(crate) source_is_local: bool,
    pub(crate) source_path: String,
    pub(crate) source_candidates: Vec<String>,
    pub(crate) source_index: usize,
    pub(crate) compatible_profile_ids: Vec<String>,
    pub(crate) profile_id: String,
    pub(crate) sprite_name: String,
    pub(crate) share_credentials: bool,
    pub(crate) pending_operation: Option<String>,
    pub(crate) approval: Option<crate::api::schema::SpriteApproval>,
    pub(crate) message: Option<String>,
    pub(crate) pending_command: Option<SpriteCommand>,
    pub(crate) pending_request: Option<SpriteRequest>,
    pub(crate) all_resources: bool,
    pub(crate) session_choice: usize,
    pub(crate) preview_resource_id: Option<String>,
    pub(crate) preview_revision: Option<u64>,
    pub(crate) transfer_preview: Option<crate::api::schema::SpriteTransferPreview>,
    pub(crate) operation_result_seen: bool,
}

impl SpriteUiState {
    pub(crate) fn begin_create(
        &mut self,
        state: &crate::app::state::AppState,
        workspace_index: usize,
        source_candidates: Vec<String>,
        compatible_profile_ids: Vec<String>,
    ) -> Result<(), String> {
        let workspace = state
            .workspaces
            .get(workspace_index)
            .ok_or_else(|| "Choose a Space before creating a Sprite.".to_string())?;
        self.screen = SpriteUiScreen::Create;
        self.manager_prompt = None;
        self.pending_operation = None;
        self.pending_request = None;
        self.pending_command = None;
        self.operation_result_seen = false;
        self.workspace_id = Some(workspace.id.clone());
        self.source_host_id = workspace.default_location.execution_host_id.to_string();
        self.source_path = workspace
            .default_location
            .path
            .as_path()
            .display()
            .to_string();
        self.source_candidates = source_candidates;
        if let Some(source) = self.source_candidates.first() {
            self.source_path.clone_from(source);
        }
        self.source_index = 0;
        self.compatible_profile_ids = compatible_profile_ids;
        self.profile_id = self
            .compatible_profile_ids
            .first()
            .cloned()
            .unwrap_or_default();
        self.sprite_name.clear();
        self.source_is_local = workspace.default_location.execution_host_id.is_local();
        self.share_credentials = false;
        self.field = 0;
        self.message = None;
        Ok(())
    }

    pub(crate) fn visible_records<'a>(
        &self,
        snapshot: &'a crate::api::schema::SpriteSnapshot,
    ) -> Vec<&'a crate::api::schema::SpriteRecord> {
        let query = self.search.trim().to_lowercase();
        snapshot
            .resources
            .iter()
            .filter(|record| {
                (self.all_resources
                    || self
                        .workspace_id
                        .as_deref()
                        .is_none_or(|id| record.workspace_id.as_deref() == Some(id)))
                    && (query.is_empty()
                        || record.name.to_lowercase().contains(&query)
                        || record.org.to_lowercase().contains(&query)
                        || record
                            .workspace_id
                            .as_deref()
                            .is_some_and(|id| id.to_lowercase().contains(&query))
                        || record
                            .agent
                            .as_ref()
                            .is_some_and(|agent| agent.profile_id.to_lowercase().contains(&query)))
            })
            .collect()
    }
    pub(crate) fn reconcile(&mut self, app_state: &crate::app::state::AppState) {
        if self.screen != SpriteUiScreen::Manager {
            return;
        }
        let snapshot = &app_state.sprites_snapshot;
        let Some(operation_id) = self.pending_operation.as_deref() else {
            return;
        };
        let Some(operation) = snapshot
            .operations
            .iter()
            .find(|operation| operation.id.as_str() == operation_id)
        else {
            return;
        };
        if self
            .pending_request
            .as_ref()
            .is_some_and(|request| request.request_id != operation.request.request_id)
        {
            return;
        }
        use crate::api::schema::{SpriteOperationStatus, SpriteResult};
        match &operation.status {
            SpriteOperationStatus::Queued | SpriteOperationStatus::Running => {
                self.message = Some(format!(
                    "{:?} · {}",
                    operation.status,
                    operation.stage.replace('_', " ")
                ));
            }
            _ if !self.operation_result_seen => {
                if operation.result.is_none()
                    && operation.error.is_none()
                    && !matches!(
                        &operation.status,
                        SpriteOperationStatus::Canceled | SpriteOperationStatus::Interrupted
                    )
                {
                    self.message = Some(format!(
                        "{:?} · {}",
                        operation.status,
                        operation.stage.replace('_', " ")
                    ));
                    return;
                }
                self.operation_result_seen = true;
                match operation.result.as_ref() {
                    Some(SpriteResult::ApprovalRequired(approval)) => {
                        self.pending_command = Some(operation.request.command.clone());
                        self.approval = Some(approval.clone());
                        self.screen = SpriteUiScreen::Approval;
                        self.message = Some(approval.summary.clone());
                    }
                    Some(SpriteResult::Transfer(preview)) => {
                        self.transfer_preview = Some(preview.clone());
                        self.message = Some(format!(
                            "Pull preview ready · {} files · {} bytes. Review risks before confirming.",
                            preview.files, preview.bytes
                        ));
                    }
                    Some(SpriteResult::Checkpoints(checkpoints)) => {
                        if self.manager_prompt.is_none() {
                            self.checkpoints.clone_from(checkpoints);
                            self.operation_choice = 0;
                            if let SpriteCommand::Checkpoints(target) = &operation.request.command {
                                self.manager_prompt = Some(SpriteManagerPrompt::CheckpointPicker {
                                    target: target.clone(),
                                    operation_id: operation.id.clone(),
                                });
                            }
                            self.message =
                                Some("Choose a checkpoint, then use Restore explicitly.".into());
                        }
                    }
                    Some(SpriteResult::Completed { message }) => {
                        self.message = Some(message.clone());
                    }
                    Some(_) => {
                        self.message = Some(format!(
                            "{:?} · {}",
                            operation.status,
                            operation.stage.replace('_', " ")
                        ));
                    }
                    None => {
                        self.message = Some(operation.error.as_ref().map_or_else(
                            || format!("{:?} · {}", operation.status, operation.stage),
                            |error| format!("{} · {}", error.message, error.code),
                        ));
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn action_rows(width: u16) -> Vec<Vec<(&'static str, KeyCode)>> {
        sprite_manager_action_rows(width)
    }

    pub(crate) fn is_prompt(&self) -> bool {
        self.manager_prompt.is_some()
    }

    pub(crate) fn list_selection(&self) -> usize {
        match self.manager_prompt.as_ref() {
            Some(SpriteManagerPrompt::OperationPicker)
            | Some(SpriteManagerPrompt::CheckpointPicker { .. }) => self.operation_choice,
            Some(SpriteManagerPrompt::SessionPicker { .. }) => self.session_choice,
            _ => self.selected,
        }
    }
}

const SPRITE_MANAGER_ACTIONS: &[(&str, KeyCode)] = &[
    ("Search", KeyCode::Char('/')),
    ("New", KeyCode::Char('n')),
    ("Scope", KeyCode::Char('A')),
    ("Refresh", KeyCode::Char('r')),
    ("Inspect", KeyCode::Char('i')),
    ("Connect", KeyCode::Char('c')),
    ("Detach", KeyCode::Char('D')),
    ("Start", KeyCode::Char('s')),
    ("Resume", KeyCode::Char('u')),
    ("Shell", KeyCode::Char('a')),
    ("Preview", KeyCode::Char('p')),
    ("Pull", KeyCode::Char('P')),
    ("Checks", KeyCode::Char('v')),
    ("Ckpt", KeyCode::Char('q')),
    ("Cancel", KeyCode::Char('C')),
    ("Restore", KeyCode::Char('R')),
    ("Stop", KeyCode::Char('x')),
    ("Destroy", KeyCode::Char('d')),
    ("Forget", KeyCode::Char('f')),
    ("Reassoc", KeyCode::Char('g')),
    ("Retry", KeyCode::Char('t')),
    ("Ops", KeyCode::Char('o')),
];

pub(crate) fn sprite_manager_action_rows(width: u16) -> Vec<Vec<(&'static str, KeyCode)>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut used = 0usize;
    for &(label, key) in SPRITE_MANAGER_ACTIONS {
        let len = sprite_action_key_label(key).chars().count() + label.chars().count() + 4;
        if !row.is_empty() && used + len > width as usize {
            rows.push(std::mem::take(&mut row));
            used = 0;
        }
        row.push((label, key));
        used += len;
    }
    rows.push(row);
    rows
}

pub(crate) fn sprite_manager_layout(area: Rect, state: &SpriteUiState) -> [Rect; 4] {
    use ratatui::layout::{Constraint, Layout};
    let footer = if state.is_prompt() {
        1
    } else {
        sprite_manager_action_rows(area.width).len() as u16
    };
    let detail = if matches!(
        state.manager_prompt.as_ref(),
        Some(
            SpriteManagerPrompt::SessionPicker { .. }
                | SpriteManagerPrompt::CheckpointPicker { .. }
                | SpriteManagerPrompt::OperationPicker
        )
    ) {
        1
    } else {
        area.height.saturating_sub(footer + 4).min(7)
    };
    Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(detail),
        Constraint::Length(footer),
    ])
    .areas(area)
}

fn sprite_action_key_label(key: KeyCode) -> String {
    match key {
        KeyCode::Char(key) => key.to_string(),
        KeyCode::Esc => "Esc".into(),
        _ => String::new(),
    }
}

fn sprite_manager_action_at(
    row: usize,
    column: u16,
    width: u16,
) -> Option<(&'static str, KeyCode)> {
    let actions = sprite_manager_action_rows(width);
    let mut x = 0usize;
    for &(label, key) in actions.get(row)? {
        let end = x + sprite_action_key_label(key).chars().count() + label.chars().count() + 4;
        if (x..end).contains(&(column as usize)) {
            return Some((label, key));
        }
        x = end;
    }
    None
}

fn sprite_popup_rect(area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    let width = 74.min(area.width.max(1));
    let height = 22.min(area.height.max(1));
    ratatui::layout::Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

impl crate::app::App {
    fn begin_sprite_create_for_view(
        &self,
        view: &mut crate::app::ClientViewState,
        workspace_index: usize,
    ) -> Result<(), String> {
        let source_candidates = self
            .state
            .workspaces
            .get(workspace_index)
            .filter(|workspace| workspace.default_location.execution_host_id.is_local())
            .map(|workspace| {
                let mut candidates = vec![workspace
                    .default_location
                    .path
                    .as_path()
                    .display()
                    .to_string()];
                candidates.extend(
                    self.state
                        .observed_git_repos_for_workspace(&self.terminal_runtimes, workspace_index)
                        .into_iter()
                        .map(|path| path.display().to_string()),
                );
                let mut seen = std::collections::HashSet::new();
                candidates.retain(|candidate| seen.insert(candidate.clone()));
                candidates
            })
            .unwrap_or_default();
        let compatible_profile_ids =
            crate::app::agent_profile_picker::agent_profile_picker_entries_for_workspace(
                &self.state,
                workspace_index,
            )
            .into_iter()
            .filter_map(|entry| {
                self.state
                    .agent_profiles
                    .profiles()
                    .iter()
                    .find(|profile| profile.id == entry.profile_id)
                    .filter(|profile| profile.sprite_unavailable_reason().is_none())
                    .map(|profile| profile.id.clone())
            })
            .collect();
        view.sprite_ui.begin_create(
            &self.state,
            workspace_index,
            source_candidates,
            compatible_profile_ids,
        )
    }

    pub(crate) fn open_sprites_for_view(
        &mut self,
        view: &mut crate::app::ClientViewState,
        create_workspace: Option<usize>,
    ) {
        if !self.state.sprites_config.enabled {
            return;
        }
        view.mode = crate::app::Mode::Sprites;
        view.sprite_ui = SpriteUiState::default();
        view.sprite_ui.workspace_id = view
            .active_workspace
            .and_then(|index| self.state.workspaces.get(index))
            .map(|workspace| workspace.id.clone());
        if let Some(workspace_index) = create_workspace {
            if let Err(error) = self.begin_sprite_create_for_view(view, workspace_index) {
                view.sprite_ui.message = Some(error);
            }
        }
    }

    pub(crate) fn handle_sprites_mouse_for_view(
        &mut self,
        view: &mut crate::app::ClientViewState,
        mouse: MouseEvent,
    ) -> bool {
        if view.mode != crate::app::Mode::Sprites {
            return false;
        }
        let screen = view.computed.terminal_area;
        let popup = sprite_popup_rect(screen);
        let inner = Rect {
            x: popup.x.saturating_add(1),
            y: popup.y.saturating_add(1),
            width: popup.width.saturating_sub(2),
            height: popup.height.saturating_sub(2),
        };
        if mouse.column < inner.x
            || mouse.column >= inner.x.saturating_add(inner.width)
            || mouse.row < inner.y
            || mouse.row >= inner.y.saturating_add(inner.height)
        {
            return false;
        }
        let row = mouse.row.saturating_sub(inner.y) as usize;
        if matches!(
            mouse.kind,
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
        ) {
            match view.sprite_ui.screen {
                SpriteUiScreen::Manager => {
                    let [_, _, detail, _] = sprite_manager_layout(inner, &view.sprite_ui);
                    let up = mouse.kind == MouseEventKind::ScrollUp;
                    let key = if mouse.row >= detail.y && !view.sprite_ui.is_prompt() {
                        if up {
                            KeyCode::PageUp
                        } else {
                            KeyCode::PageDown
                        }
                    } else if up {
                        KeyCode::Up
                    } else {
                        KeyCode::Down
                    };
                    self.handle_sprites_key_for_view(view, KeyEvent::from(key));
                }
                SpriteUiScreen::Create => {
                    let footer_start = inner.y.saturating_add(inner.height.saturating_sub(2));
                    if mouse.row >= footer_start {
                        return false;
                    }
                    view.sprite_ui.field = if mouse.kind == MouseEventKind::ScrollUp {
                        view.sprite_ui.field.saturating_sub(1)
                    } else {
                        view.sprite_ui.field.saturating_add(1).min(4)
                    };
                }
                _ => return false,
            }
            return true;
        }
        if !matches!(
            mouse.kind,
            MouseEventKind::Down(crossterm::event::MouseButton::Left)
        ) {
            return false;
        }
        match view.sprite_ui.screen {
            SpriteUiScreen::Manager => {
                let [search, list, _, footer] = sprite_manager_layout(inner, &view.sprite_ui);
                if mouse.row >= footer.y {
                    if view.sprite_ui.is_prompt() {
                        let key = if mouse.column < inner.x + 11 {
                            KeyCode::Enter
                        } else {
                            KeyCode::Esc
                        };
                        self.handle_sprites_key_for_view(view, KeyEvent::from(key));
                    } else if let Some((_, key)) = sprite_manager_action_at(
                        mouse.row.saturating_sub(footer.y) as usize,
                        mouse.column.saturating_sub(inner.x),
                        inner.width,
                    ) {
                        self.handle_sprites_key_for_view(view, KeyEvent::from(key));
                    }
                } else if mouse.row < search.bottom() {
                    if mouse.column >= inner.right().saturating_sub(7) {
                        self.handle_sprites_key_for_view(view, KeyEvent::from(KeyCode::Esc));
                    } else if !view.sprite_ui.is_prompt() {
                        view.sprite_ui.manager_prompt = Some(SpriteManagerPrompt::Search);
                    }
                } else if mouse.row >= list.y && mouse.row < list.bottom().saturating_sub(1) {
                    let offset = view
                        .sprite_ui
                        .list_selection()
                        .saturating_sub(list.height.saturating_sub(1) as usize / 2);
                    let index = offset + mouse.row.saturating_sub(list.y) as usize;
                    match view.sprite_ui.manager_prompt.as_ref() {
                        Some(SpriteManagerPrompt::OperationPicker) => {
                            if index < self.state.sprites_snapshot.operations.len() {
                                view.sprite_ui.operation_choice = index;
                            }
                        }
                        Some(SpriteManagerPrompt::CheckpointPicker { .. }) => {
                            if index < view.sprite_ui.checkpoints.len() {
                                view.sprite_ui.operation_choice = index;
                            }
                        }
                        Some(SpriteManagerPrompt::SessionPicker { .. }) => {
                            let count = view
                                .sprite_ui
                                .visible_records(&self.state.sprites_snapshot)
                                .get(view.sprite_ui.selected)
                                .map_or(0, |record| record.sessions.len());
                            if index < count {
                                view.sprite_ui.session_choice = index;
                            }
                        }
                        _ if index
                            < view
                                .sprite_ui
                                .visible_records(&self.state.sprites_snapshot)
                                .len() =>
                        {
                            view.sprite_ui.selected = index;
                            view.sprite_ui.detail_scroll = 0;
                        }
                        _ => {}
                    }
                }
            }
            SpriteUiScreen::Create => {
                let footer_start = inner.y.saturating_add(inner.height.saturating_sub(2));
                if mouse.row >= footer_start {
                    if mouse.column < inner.x.saturating_add(9) {
                        view.sprite_ui.field = 4;
                        self.handle_sprites_key_for_view(view, KeyEvent::from(KeyCode::Enter));
                    } else {
                        self.handle_sprites_key_for_view(view, KeyEvent::from(KeyCode::Esc));
                    }
                } else {
                    let body_height = inner.height.saturating_sub(2).min(6) as usize;
                    let scroll_offset = view
                        .sprite_ui
                        .field
                        .saturating_add(1)
                        .saturating_sub(body_height.saturating_sub(1));
                    let field = row.saturating_add(scroll_offset);
                    if field <= 5 {
                        view.sprite_ui.field = match field {
                            0 | 1 => 0,
                            2 => 1,
                            3 => 2,
                            4 => 3,
                            _ => 4,
                        };
                        match field {
                            4 => self.handle_sprites_key_for_view(
                                view,
                                KeyEvent::from(KeyCode::Char(' ')),
                            ),
                            5 => self
                                .handle_sprites_key_for_view(view, KeyEvent::from(KeyCode::Enter)),
                            1 | 2 => {
                                let direction = if mouse.column < inner.x + inner.width / 2 {
                                    KeyCode::Left
                                } else {
                                    KeyCode::Right
                                };
                                self.handle_sprites_key_for_view(view, KeyEvent::from(direction));
                            }
                            _ => {}
                        }
                    }
                }
            }
            SpriteUiScreen::Approval => {
                if row >= inner.height.saturating_sub(1) as usize {
                    let key = if mouse.column < inner.x + 16 {
                        KeyCode::Enter
                    } else {
                        KeyCode::Esc
                    };
                    self.handle_sprites_key_for_view(view, KeyEvent::from(key));
                }
            }
        }
        true
    }

    pub(crate) fn handle_sprites_key_for_view(
        &mut self,
        view: &mut crate::app::ClientViewState,
        key: KeyEvent,
    ) {
        if !self.state.sprites_config.enabled {
            view.mode = crate::app::Mode::Navigate;
            view.sprite_ui = SpriteUiState::default();
            return;
        }
        if view.sprite_ui.screen == SpriteUiScreen::Approval {
            match key.code {
                KeyCode::Esc => {
                    view.sprite_ui.screen = SpriteUiScreen::Manager;
                    view.sprite_ui.approval = None;
                    view.sprite_ui.pending_command = None;
                }
                KeyCode::Enter => {
                    let Some(approval) = view.sprite_ui.approval.take() else {
                        return;
                    };
                    let Some(command) = view.sprite_ui.pending_command.take() else {
                        return;
                    };
                    let Some(command) = apply_sprite_approval(command, approval.token) else {
                        return;
                    };
                    view.sprite_ui.screen = SpriteUiScreen::Manager;
                    self.submit_sprite_ui_request(view, command, None);
                }
                _ => {}
            }
            return;
        }
        match view.sprite_ui.manager_prompt.clone() {
            Some(SpriteManagerPrompt::Search) => {
                match key.code {
                    KeyCode::Esc | KeyCode::Enter => view.sprite_ui.manager_prompt = None,
                    KeyCode::Backspace => {
                        view.sprite_ui.search.pop();
                    }
                    KeyCode::Char(c)
                        if !key
                            .modifiers
                            .contains(crossterm::event::KeyModifiers::CONTROL) =>
                    {
                        view.sprite_ui.search.push(c);
                    }
                    _ => {}
                }
                view.sprite_ui.selected = 0;
                view.sprite_ui.detail_scroll = 0;
                return;
            }
            Some(SpriteManagerPrompt::CheckpointPicker {
                target,
                operation_id,
            }) => {
                if view.sprite_ui.pending_operation.as_deref() != Some(operation_id.as_str()) {
                    view.sprite_ui.manager_prompt = None;
                    return;
                }
                match key.code {
                    KeyCode::Esc => view.sprite_ui.manager_prompt = None,
                    KeyCode::Up => {
                        view.sprite_ui.operation_choice =
                            view.sprite_ui.operation_choice.saturating_sub(1)
                    }
                    KeyCode::Down => {
                        view.sprite_ui.operation_choice = view
                            .sprite_ui
                            .operation_choice
                            .saturating_add(1)
                            .min(view.sprite_ui.checkpoints.len().saturating_sub(1))
                    }
                    KeyCode::Enter => {
                        if let Some(checkpoint) = view
                            .sprite_ui
                            .checkpoints
                            .get(view.sprite_ui.operation_choice)
                        {
                            view.sprite_ui.selected_checkpoint =
                                Some((target.sprite_id, checkpoint.clone()));
                            view.sprite_ui.message = Some(format!(
                                "Selected checkpoint {checkpoint}. Use Restore to request confirmation."
                            ));
                        }
                        view.sprite_ui.manager_prompt = None;
                    }
                    _ => {}
                }
                return;
            }
            Some(SpriteManagerPrompt::OperationPicker) => {
                let operations = &self.state.sprites_snapshot.operations;
                match key.code {
                    KeyCode::Esc => view.sprite_ui.manager_prompt = None,
                    KeyCode::Up => {
                        view.sprite_ui.operation_choice =
                            view.sprite_ui.operation_choice.saturating_sub(1)
                    }
                    KeyCode::Down => {
                        view.sprite_ui.operation_choice = view
                            .sprite_ui
                            .operation_choice
                            .saturating_add(1)
                            .min(operations.len().saturating_sub(1))
                    }
                    KeyCode::Enter => {
                        if let Some(operation) =
                            operations.iter().rev().nth(view.sprite_ui.operation_choice)
                        {
                            view.sprite_ui.pending_operation = Some(operation.id.clone());
                            view.sprite_ui.pending_request = Some(operation.request.clone());
                            view.sprite_ui.pending_command =
                                Some(operation.request.command.clone());
                            view.sprite_ui.operation_result_seen = true;
                            view.sprite_ui.detail_scroll = 0;
                            view.sprite_ui.message = Some(operation.error.as_ref().map_or_else(
                                || format!("{:?} · {}", operation.status, operation.stage),
                                |error| format!("{} · {}", error.code, error.message),
                            ));
                        }
                        view.sprite_ui.manager_prompt = None;
                    }
                    _ => {}
                }
                return;
            }
            Some(SpriteManagerPrompt::SessionPicker {
                action,
                presentation_workspace_id,
            }) => {
                let record = view
                    .sprite_ui
                    .visible_records(&self.state.sprites_snapshot)
                    .get(view.sprite_ui.selected)
                    .map(|record| (*record).clone());
                match key.code {
                    KeyCode::Esc => view.sprite_ui.manager_prompt = None,
                    KeyCode::Up | KeyCode::Char('k') => {
                        view.sprite_ui.session_choice =
                            view.sprite_ui.session_choice.saturating_sub(1)
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if let Some(record) = &record {
                            view.sprite_ui.session_choice = view
                                .sprite_ui
                                .session_choice
                                .saturating_add(1)
                                .min(record.sessions.len().saturating_sub(1));
                        }
                    }
                    KeyCode::Enter => {
                        if let Some(record) = record {
                            if let Some(session) =
                                record.sessions.get(view.sprite_ui.session_choice)
                            {
                                if action == SpriteSessionAction::Stop && !session.owned {
                                    view.sprite_ui.message = Some(
                                        "Only a known owned session can be stopped; select an owned session."
                                            .into(),
                                    );
                                } else {
                                    let workspace = if action == SpriteSessionAction::Connect {
                                        presentation_workspace_id.as_deref().and_then(|id| {
                                            self.state.workspaces.iter().find(|workspace| {
                                                workspace.id == id
                                                    && workspace
                                                        .default_location
                                                        .execution_host_id
                                                        .is_local()
                                            })
                                        })
                                    } else {
                                        None
                                    };
                                    if action == SpriteSessionAction::Connect && workspace.is_none()
                                    {
                                        view.sprite_ui.manager_prompt = None;
                                        view.sprite_ui.message = Some(
                                            "The selected local Space is no longer available; choose a current local Space and retry."
                                                .into(),
                                        );
                                        return;
                                    }
                                    let mut exact_target = target(&record);
                                    exact_target.session_id = Some(session.id.clone());
                                    let command = match action {
                                        SpriteSessionAction::Connect => {
                                            SpriteCommand::Connect(exact_target)
                                        }
                                        SpriteSessionAction::Stop => {
                                            SpriteCommand::Stop(exact_target)
                                        }
                                    };
                                    let workspace_id =
                                        workspace.map(|workspace| workspace.id.clone());
                                    view.sprite_ui.manager_prompt = None;
                                    self.submit_sprite_ui_request(view, command, workspace_id);
                                }
                            } else {
                                view.sprite_ui.manager_prompt = None;
                                view.sprite_ui.message =
                                    Some("No session is available for this Sprite.".into());
                            }
                        }
                    }
                    _ => {}
                }
                return;
            }
            Some(SpriteManagerPrompt::Resume { mut reference }) => {
                match key.code {
                    KeyCode::Esc => {
                        view.sprite_ui.manager_prompt = None;
                    }
                    KeyCode::Backspace => {
                        reference.pop();
                        view.sprite_ui.manager_prompt =
                            Some(SpriteManagerPrompt::Resume { reference });
                    }
                    KeyCode::Enter => {
                        let conversation_ref = reference.trim().to_string();
                        let record = view
                            .sprite_ui
                            .visible_records(&self.state.sprites_snapshot)
                            .get(view.sprite_ui.selected)
                            .map(|record| (*record).clone());
                        if conversation_ref.is_empty() {
                            view.sprite_ui.message =
                                Some("Enter a supported agent conversation reference.".into());
                        } else if let Some(record) = record {
                            let workspace = record.workspace_id.clone();
                            view.sprite_ui.manager_prompt = None;
                            self.submit_sprite_ui_request(
                                view,
                                SpriteCommand::Resume {
                                    target: target(&record),
                                    conversation_ref,
                                },
                                workspace,
                            );
                        }
                    }
                    KeyCode::Char(c) => {
                        reference.push(c);
                        view.sprite_ui.manager_prompt =
                            Some(SpriteManagerPrompt::Resume { reference });
                    }
                    _ => {}
                }
                return;
            }
            None => {}
        }
        if key.code == KeyCode::Esc {
            if view.sprite_ui.screen == SpriteUiScreen::Create {
                view.sprite_ui.screen = SpriteUiScreen::Manager;
                return;
            }
            view.mode = crate::app::Mode::Navigate;
            return;
        }
        if view.sprite_ui.screen == SpriteUiScreen::Create {
            self.handle_sprite_create_key(view, key);
            return;
        }
        let resources = view.sprite_ui.visible_records(&self.state.sprites_snapshot);
        match key.code {
            KeyCode::PageUp => {
                view.sprite_ui.detail_scroll = view.sprite_ui.detail_scroll.saturating_sub(3)
            }
            KeyCode::PageDown => {
                view.sprite_ui.detail_scroll = view.sprite_ui.detail_scroll.saturating_add(3)
            }
            KeyCode::Char('o') => {
                view.sprite_ui.manager_prompt = Some(SpriteManagerPrompt::OperationPicker);
                view.sprite_ui.operation_choice = 0;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                view.sprite_ui.selected = view.sprite_ui.selected.saturating_sub(1)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                view.sprite_ui.selected = view
                    .sprite_ui
                    .selected
                    .saturating_add(1)
                    .min(resources.len().saturating_sub(1));
            }
            KeyCode::Char('/') => {
                view.sprite_ui.manager_prompt = Some(SpriteManagerPrompt::Search);
                view.sprite_ui.message =
                    Some("Search: type any text. Enter or Escape returns to actions.".into());
            }
            KeyCode::Backspace if !view.sprite_ui.search.is_empty() => {
                view.sprite_ui.search.pop();
                view.sprite_ui.selected = 0;
            }
            KeyCode::Char('r') => {
                self.submit_sprite_ui_request(view, SpriteCommand::List { refresh: true }, None)
            }
            KeyCode::Char('A') => {
                view.sprite_ui.all_resources = !view.sprite_ui.all_resources;
                view.sprite_ui.selected = 0;
                view.sprite_ui.message = Some(if view.sprite_ui.all_resources {
                    "Showing all Sprites.".into()
                } else {
                    "Showing Sprites associated with the current Space.".into()
                });
            }
            KeyCode::Char('u') => {
                view.sprite_ui.manager_prompt = Some(SpriteManagerPrompt::Resume {
                    reference: String::new(),
                });
                view.sprite_ui.message = Some(
                    "Enter the exact supported agent conversation reference, then press Enter."
                        .into(),
                );
            }
            KeyCode::Char('n') => {
                if let Some(index) = view.active_workspace {
                    if let Err(error) = self.begin_sprite_create_for_view(view, index) {
                        view.sprite_ui.message = Some(error);
                    }
                } else {
                    view.sprite_ui.message =
                        Some("Select a Space before creating a Sprite.".into());
                }
            }
            KeyCode::Char('i') | KeyCode::Enter => {
                if let Some(record) = resources.get(view.sprite_ui.selected) {
                    self.submit_sprite_ui_request(
                        view,
                        SpriteCommand::Inspect(target(record)),
                        None,
                    );
                }
            }
            KeyCode::Char('g') => {
                let selected = resources
                    .get(view.sprite_ui.selected)
                    .map(|record| (*record).clone());
                let workspace = view
                    .active_workspace
                    .and_then(|index| self.state.workspaces.get(index));
                match (selected, workspace) {
                    (Some(record), Some(_workspace)) if !record.managed => {
                        view.sprite_ui.message =
                            Some("Reassociate is unavailable for foreign Sprites.".into());
                    }
                    (Some(_), Some(workspace))
                        if !workspace.default_location.execution_host_id.is_local() =>
                    {
                        view.sprite_ui.message = Some(format!(
                            "Reassociation from source host {} is unsupported.",
                            workspace.default_location.execution_host_id
                        ));
                    }
                    (Some(record), Some(workspace)) => {
                        let workspace_id = workspace.id.clone();
                        let source = SpriteSource {
                            execution_host_id: workspace
                                .default_location
                                .execution_host_id
                                .to_string(),
                            path: workspace
                                .default_location
                                .path
                                .as_path()
                                .display()
                                .to_string(),
                        };
                        self.submit_sprite_ui_request(
                            view,
                            SpriteCommand::Reassociate {
                                target: target(&record),
                                workspace_id,
                                source,
                            },
                            None,
                        );
                    }
                    _ => {
                        view.sprite_ui.message = Some("Select a Space before reassociating.".into())
                    }
                }
            }
            KeyCode::Char('c') => self.with_selected_sprite(view, SpriteCommand::Connect),
            KeyCode::Char('D') => self.with_selected_sprite(view, SpriteCommand::Disconnect),
            KeyCode::Char('s') => self.with_selected_sprite(view, SpriteCommand::Start),
            KeyCode::Char('a') => self.with_selected_sprite(view, SpriteCommand::Shell),
            KeyCode::Char('p') => self.with_selected_sprite(view, SpriteCommand::PullPreview),
            KeyCode::Char('v') => self.with_selected_sprite(view, SpriteCommand::Checkpoints),
            KeyCode::Char('x') => self.with_selected_sprite(view, SpriteCommand::Stop),
            KeyCode::Char('q') => self.with_selected_sprite(view, SpriteCommand::Checkpoint),
            KeyCode::Char('P') => self.with_selected_sprite(view, SpriteCommand::Pull),
            KeyCode::Char('R') => {
                let checkpoint = view.sprite_ui.selected_checkpoint.clone();
                if let Some((resource_id, checkpoint_id)) = checkpoint {
                    if resources
                        .get(view.sprite_ui.selected)
                        .is_some_and(|record| record.id == resource_id)
                    {
                        self.with_selected_sprite(view, |mut target| {
                            target.checkpoint_id = Some(checkpoint_id);
                            SpriteCommand::Restore(target)
                        });
                    } else {
                        view.sprite_ui.message =
                            Some("Choose a checkpoint for this Sprite with Checks first.".into());
                    }
                } else {
                    view.sprite_ui.message =
                        Some("Choose an exact checkpoint with Checks before Restore.".into());
                }
            }
            KeyCode::Char('d') => self.with_selected_sprite(view, SpriteCommand::Destroy),
            KeyCode::Char('f') => self.with_selected_sprite(view, SpriteCommand::Forget),
            KeyCode::Char('t') => {
                if let Some(operation_id) = view.sprite_ui.pending_operation.clone() {
                    self.submit_sprite_ui_request(
                        view,
                        SpriteCommand::Retry { operation_id },
                        None,
                    );
                }
            }
            KeyCode::Char('C') => {
                if let Some(operation_id) = view.sprite_ui.pending_operation.clone() {
                    self.submit_sprite_ui_request(
                        view,
                        SpriteCommand::Cancel { operation_id },
                        None,
                    );
                }
            }
            _ => {}
        }
    }

    fn handle_sprite_create_key(&mut self, view: &mut crate::app::ClientViewState, key: KeyEvent) {
        use KeyCode::*;
        match key.code {
            Tab | Down => view.sprite_ui.field = (view.sprite_ui.field + 1) % 5,
            BackTab | Up => view.sprite_ui.field = (view.sprite_ui.field + 4) % 5,
            Left if view.sprite_ui.field == 0 && !view.sprite_ui.source_candidates.is_empty() => {
                view.sprite_ui.source_index = view.sprite_ui.source_index.saturating_sub(1);
                view.sprite_ui.source_path =
                    view.sprite_ui.source_candidates[view.sprite_ui.source_index].clone();
            }
            Right if view.sprite_ui.field == 0 && !view.sprite_ui.source_candidates.is_empty() => {
                view.sprite_ui.source_index = (view.sprite_ui.source_index + 1)
                    .min(view.sprite_ui.source_candidates.len() - 1);
                view.sprite_ui.source_path =
                    view.sprite_ui.source_candidates[view.sprite_ui.source_index].clone();
            }
            Left | Right
                if view.sprite_ui.field == 1
                    && !view.sprite_ui.compatible_profile_ids.is_empty() =>
            {
                let len = view.sprite_ui.compatible_profile_ids.len();
                let current = view
                    .sprite_ui
                    .compatible_profile_ids
                    .iter()
                    .position(|id| id == &view.sprite_ui.profile_id)
                    .unwrap_or(0);
                let next = if key.code == Left {
                    current.saturating_sub(1)
                } else {
                    (current + 1).min(len - 1)
                };
                view.sprite_ui.profile_id = view.sprite_ui.compatible_profile_ids[next].clone();
            }
            Enter if view.sprite_ui.field == 4 => {
                let workspace_id = view.sprite_ui.workspace_id.clone().unwrap_or_default();
                if workspace_id.is_empty() {
                    view.sprite_ui.message = Some("Choose the target Space.".into());
                    return;
                }
                if !view.sprite_ui.source_is_local {
                    view.sprite_ui.message = Some(format!("Sprite transfer from source host {} is not supported yet; source remains unchanged.", view.sprite_ui.source_host_id));
                    return;
                }
                let profile = self
                    .state
                    .agent_profiles
                    .profiles()
                    .iter()
                    .find(|profile| profile.id == view.sprite_ui.profile_id);
                let Some(profile) = profile else {
                    view.sprite_ui.message = Some("Choose an installed agent profile.".into());
                    return;
                };
                if let Some(reason) = profile.sprite_unavailable_reason() {
                    view.sprite_ui.message = Some(format!(
                        "Agent profile {} is unavailable for Sprites: {reason}",
                        profile.name
                    ));
                    return;
                }
                let params = SpriteCreateParams {
                    workspace_id: workspace_id.clone(),
                    source: SpriteSource {
                        execution_host_id: view.sprite_ui.source_host_id.clone(),
                        path: view.sprite_ui.source_path.clone(),
                    },
                    agent: SpriteAgent {
                        profile_id: profile.id.clone(),
                        kind: profile.kind.as_str().to_string(),
                        command: profile.argv.clone(),
                        share_credentials: view.sprite_ui.share_credentials,
                    },
                    name: (!view.sprite_ui.sprite_name.trim().is_empty())
                        .then(|| view.sprite_ui.sprite_name.trim().to_string()),
                };
                self.submit_sprite_ui_request(
                    view,
                    SpriteCommand::Create(params),
                    Some(workspace_id),
                );
                view.sprite_ui.screen = SpriteUiScreen::Manager;
            }
            Char(' ') if view.sprite_ui.field == 3 => {
                view.sprite_ui.share_credentials = !view.sprite_ui.share_credentials
            }
            Backspace => match view.sprite_ui.field {
                0 => {
                    view.sprite_ui.source_path.pop();
                }
                1 => {
                    view.sprite_ui.profile_id.pop();
                }
                2 => {
                    view.sprite_ui.sprite_name.pop();
                }
                _ => {}
            },
            Char(c) => match view.sprite_ui.field {
                0 => view.sprite_ui.source_path.push(c),
                1 => view.sprite_ui.profile_id.push(c),
                2 => view.sprite_ui.sprite_name.push(c),
                _ => {}
            },
            _ => {}
        }
    }

    fn local_presentation_workspace_id(
        &self,
        view: &crate::app::ClientViewState,
    ) -> Option<String> {
        [view.active_workspace, Some(view.selected_workspace)]
            .into_iter()
            .flatten()
            .filter_map(|index| self.state.workspaces.get(index))
            .find(|workspace| workspace.default_location.execution_host_id.is_local())
            .map(|workspace| workspace.id.clone())
    }

    fn with_selected_sprite(
        &mut self,
        view: &mut crate::app::ClientViewState,
        command: impl FnOnce(SpriteTarget) -> SpriteCommand,
    ) {
        let Some(record) = view
            .sprite_ui
            .visible_records(&self.state.sprites_snapshot)
            .get(view.sprite_ui.selected)
            .map(|record| (*record).clone())
        else {
            return;
        };
        let mut command = command(target(&record));
        match &command {
            SpriteCommand::PullPreview(_) => {
                view.sprite_ui.preview_resource_id = Some(record.id.clone());
                view.sprite_ui.preview_revision = Some(record.revision);
                view.sprite_ui.transfer_preview = None;
            }
            SpriteCommand::Pull(_)
                if view.sprite_ui.transfer_preview.is_none()
                    || view.sprite_ui.preview_resource_id.as_deref()
                        != Some(record.id.as_str())
                    || view.sprite_ui.preview_revision != Some(record.revision) =>
            {
                view.sprite_ui.message = Some(
                    "Run and review a fresh Pull preview for this Sprite before confirming with P."
                        .into(),
                );
                return;
            }
            _ => {}
        }
        if !record.managed
            && matches!(
                &command,
                SpriteCommand::Stop(_)
                    | SpriteCommand::PullPreview(_)
                    | SpriteCommand::Pull(_)
                    | SpriteCommand::Checkpoint(_)
                    | SpriteCommand::Restore(_)
                    | SpriteCommand::Destroy(_)
                    | SpriteCommand::Forget(_)
            )
        {
            let action = match &command {
                SpriteCommand::Stop(_) => "Stop",
                SpriteCommand::PullPreview(_) => "Pull preview",
                SpriteCommand::Pull(_) => "Pull",
                SpriteCommand::Checkpoint(_) => "Checkpoint",
                SpriteCommand::Restore(_) => "Restore",
                SpriteCommand::Destroy(_) => "Destroy",
                _ => "Forget",
            };
            view.sprite_ui.message = Some(format!("{action} is unavailable for foreign Sprites."));
            return;
        }
        if matches!(&command, SpriteCommand::Pull(_)) {
            view.sprite_ui.transfer_preview = None;
            view.sprite_ui.preview_resource_id = None;
            view.sprite_ui.preview_revision = None;
        }
        let presentation_workspace_id = if matches!(
            &command,
            SpriteCommand::Connect(_) | SpriteCommand::Shell(_)
        ) {
            let Some(workspace_id) = self.local_presentation_workspace_id(view) else {
                view.sprite_ui.message =
                    Some("Choose a current local Space before opening this Sprite.".into());
                return;
            };
            Some(workspace_id)
        } else {
            None
        };
        match &mut command {
            SpriteCommand::Connect(target) => {
                if record.sessions.is_empty() {
                    view.sprite_ui.message = Some(
                        "This Sprite has no existing session to connect to; use Start explicitly."
                            .into(),
                    );
                    return;
                }
                if record.sessions.len() > 1 {
                    view.sprite_ui.manager_prompt = Some(SpriteManagerPrompt::SessionPicker {
                        action: SpriteSessionAction::Connect,
                        presentation_workspace_id,
                    });
                    view.sprite_ui.session_choice = 0;
                    return;
                }
                target.session_id = Some(record.sessions[0].id.clone());
            }
            SpriteCommand::Stop(target) => {
                if record.sessions.len() > 1 {
                    view.sprite_ui.manager_prompt = Some(SpriteManagerPrompt::SessionPicker {
                        action: SpriteSessionAction::Stop,
                        presentation_workspace_id: None,
                    });
                    view.sprite_ui.session_choice = 0;
                    return;
                }
                if !record.sessions.first().is_some_and(|session| session.owned) {
                    view.sprite_ui.message =
                        Some("Stop is available only for a known owned session.".into());
                    return;
                }
                target.session_id = Some(record.sessions[0].id.clone());
            }
            _ => {}
        }
        let open_in_workspace = match &command {
            SpriteCommand::Connect(_) | SpriteCommand::Shell(_) => presentation_workspace_id,
            SpriteCommand::Start(_) | SpriteCommand::Resume { .. } => record.workspace_id.clone(),
            _ => None,
        };
        self.submit_sprite_ui_request(view, command, open_in_workspace);
    }

    fn submit_sprite_ui_request(
        &mut self,
        view: &mut crate::app::ClientViewState,
        command: SpriteCommand,
        open_in_workspace: Option<String>,
    ) {
        view.sprite_ui.detail_scroll = 0;
        let request_id = new_sprite_request_id();
        let request = SpriteRequest {
            request_id: request_id.clone(),
            command: command.clone(),
            open_in_workspace: open_in_workspace.clone(),
            focus: open_in_workspace.is_some(),
        };
        view.sprite_ui.pending_request = Some(request.clone());
        view.sprite_ui.operation_result_seen = false;
        view.sprite_ui.pending_command = Some(command);
        match self.submit_sprite_request(request) {
            Ok(crate::api::schema::SpriteReply::Operation(operation)) => {
                self.sprite_request_views
                    .insert(operation.id.clone(), view.id());
                view.sprite_ui.pending_operation = Some(operation.id.clone());
                view.sprite_ui.message = Some(match operation.stage.as_str() {
                    "transport_open" => "Local terminal opened; remote attachment is unconfirmed. Inspect terminal output.".into(),
                    "session_started" => "Remote session observed.".into(),
                    _ => format!("{}: {}", operation.stage, operation.id),
                });
                if let Some(crate::api::schema::SpriteResult::Transfer(preview)) = operation.result
                {
                    view.sprite_ui.transfer_preview = Some(preview);
                    view.sprite_ui.message = Some(
                        "Pull preview ready. Review paths and risks, then press P to confirm."
                            .into(),
                    );
                }
            }
            Ok(crate::api::schema::SpriteReply::Snapshot(snapshot)) => {
                self.state.sprites_snapshot = snapshot;
                view.sprite_ui.pending_operation = None;
                view.sprite_ui.message =
                    Some("Local attachment updated; remote sessions remain unchanged.".into());
            }
            Ok(crate::api::schema::SpriteReply::Disabled) => {
                view.sprite_ui.message =
                    Some("Sprites are disabled in Settings → Integrations.".into());
            }
            Err(error) => view.sprite_ui.message = Some(error.message),
        }
    }
}

fn apply_sprite_approval(mut command: SpriteCommand, token: String) -> Option<SpriteCommand> {
    let target = match &mut command {
        SpriteCommand::Stop(target)
        | SpriteCommand::Pull(target)
        | SpriteCommand::Restore(target)
        | SpriteCommand::Destroy(target)
        | SpriteCommand::Forget(target) => target,
        _ => return None,
    };
    target.approval = Some(token);
    Some(command)
}
fn target(record: &crate::api::schema::SpriteRecord) -> SpriteTarget {
    SpriteTarget {
        sprite_id: record.id.clone(),
        session_id: None,
        checkpoint_id: record.checkpoint_id.clone(),
        approval: None,
    }
}

fn new_sprite_request_id() -> String {
    let sequence = NEXT_SPRITE_UI_REQUEST_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    format!("ui-{}-{timestamp}-{sequence}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> crate::app::App {
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        crate::app::App::new(
            &crate::config::Config::default(),
            true,
            None,
            api_rx,
            crate::api::EventHub::default(),
        )
    }

    fn rendered_output(
        app: &crate::app::App,
        view: &mut crate::app::ClientViewState,
        width: u16,
        height: u16,
    ) -> String {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height))
                .expect("test terminal");
        terminal
            .draw(|frame| {
                view.computed.terminal_area = frame.area();
                crate::ui::render(&app.state, view, &app.terminal_runtimes, frame);
            })
            .expect("render Sprite UI");
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    fn foreign_sprite(
        sessions: Vec<crate::api::schema::SpriteSession>,
    ) -> crate::api::schema::SpriteRecord {
        crate::api::schema::SpriteRecord {
            id: "foreign-id".into(),
            org: "foreign-org".into(),
            name: "foreign-resource".into(),
            managed: false,
            workspace_id: None,
            source: None,
            agent: None,
            phase: "running".into(),
            provider_state: None,
            sessions,
            checkpoint_id: None,
            revision: 1,
            updated_unix_ms: 0,
            observed_unix_ms: None,
            last_error: None,
            unpulled_changes: None,
            attached_panes: Vec::new(),
            agent_status: None,
        }
    }

    #[test]
    fn sprite_search_treats_action_shortcuts_as_literal_text() {
        let mut app = test_app();
        app.state.sprites_config.enabled = true;
        let mut view = app.default_client_view.clone();
        app.open_sprites_for_view(&mut view, None);
        app.handle_sprites_key_for_view(&mut view, KeyEvent::from(KeyCode::Char('/')));
        for character in "dangerous".chars() {
            app.handle_sprites_key_for_view(&mut view, KeyEvent::from(KeyCode::Char(character)));
        }
        let output = rendered_output(&app, &mut view, 55, 18);
        assert!(
            output.contains("dangerous"),
            "search must retain every typed character: {output}"
        );
    }

    #[test]
    fn foreign_sprite_connect_targets_the_current_local_presentation_space() {
        let mut app = test_app();
        app.state.sprites_config.enabled = true;
        app.state
            .workspaces
            .push(crate::workspace::Workspace::test_new("presentation-space"));
        app.state
            .sprites_snapshot
            .resources
            .push(foreign_sprite(vec![
                crate::api::schema::SpriteSession {
                    id: "session-1".into(),
                    command: vec!["agent".into()],
                    tty: true,
                    owned: false,
                },
                crate::api::schema::SpriteSession {
                    id: "session-2".into(),
                    command: vec!["shell".into()],
                    tty: true,
                    owned: false,
                },
            ]));
        let mut view = app.default_client_view.clone();
        view.active_workspace = Some(0);
        view.selected_workspace = 0;
        app.open_sprites_for_view(&mut view, None);
        view.sprite_ui.all_resources = true;
        let _ = rendered_output(&app, &mut view, 100, 30);
        let popup = sprite_popup_rect(view.computed.terminal_area);
        let inner = Rect {
            x: popup.x.saturating_add(1),
            y: popup.y.saturating_add(1),
            width: popup.width.saturating_sub(2),
            height: popup.height.saturating_sub(2),
        };
        let [_, list, _, _] = sprite_manager_layout(inner, &view.sprite_ui);
        app.handle_sprites_mouse_for_view(
            &mut view,
            MouseEvent {
                kind: MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column: list.x.saturating_add(2),
                row: list.y,
                modifiers: crossterm::event::KeyModifiers::NONE,
            },
        );
        app.handle_sprites_key_for_view(&mut view, KeyEvent::from(KeyCode::Char('c')));
        let prompt_output = rendered_output(&app, &mut view, 100, 30);
        assert!(
            prompt_output.contains("Choose exact Connect session"),
            "foreign resource must open the exact-session picker: {prompt_output}"
        );
        app.handle_sprites_key_for_view(&mut view, KeyEvent::from(KeyCode::Enter));
        let request = view
            .sprite_ui
            .pending_request
            .as_ref()
            .expect("connect request was issued");
        assert_eq!(
            request.open_in_workspace.as_deref(),
            Some(app.state.workspaces[0].id.as_str())
        );
        assert!(matches!(
            &request.command,
            SpriteCommand::Connect(crate::api::schema::SpriteTarget {
                session_id: Some(_),
                ..
            })
        ));
        let output = rendered_output(&app, &mut view, 100, 30);
        assert!(
            output.contains("foreign-resource"),
            "manager continues to present the foreign resource: {output}"
        );

        let mut shell_view = app.default_client_view.clone();
        shell_view.active_workspace = Some(0);
        shell_view.selected_workspace = 0;
        app.open_sprites_for_view(&mut shell_view, None);
        shell_view.sprite_ui.all_resources = true;
        let shell_output = rendered_output(&app, &mut shell_view, 100, 30);
        app.handle_sprites_key_for_view(&mut shell_view, KeyEvent::from(KeyCode::Char('a')));
        let shell_request = shell_view
            .sprite_ui
            .pending_request
            .as_ref()
            .expect("shell request was issued");
        assert_eq!(
            shell_request.open_in_workspace.as_deref(),
            Some(app.state.workspaces[0].id.as_str())
        );
        assert!(
            shell_output.contains("foreign-resource"),
            "Shell starts from the visibly selected foreign resource: {shell_output}"
        );
    }

    #[test]
    fn late_checkpoint_results_do_not_intercept_create_input() {
        let mut app = test_app();
        app.state.sprites_config.enabled = true;
        app.state
            .workspaces
            .push(crate::workspace::Workspace::test_new("create-space"));
        let command = SpriteCommand::Checkpoints(SpriteTarget {
            sprite_id: "old-resource".into(),
            session_id: None,
            checkpoint_id: None,
            approval: None,
        });
        app.state
            .sprites_snapshot
            .operations
            .push(crate::api::schema::SpriteOperation {
                id: "delayed-checkpoints".into(),
                request: SpriteRequest {
                    request_id: "delayed-request".into(),
                    command,
                    open_in_workspace: None,
                    focus: false,
                },
                status: crate::api::schema::SpriteOperationStatus::Running,
                stage: "fetching_checkpoints".into(),
                created_unix_ms: 0,
                updated_unix_ms: 0,
                result: None,
                error: None,
            });
        let mut view = app.default_client_view.clone();
        view.active_workspace = Some(0);
        view.selected_workspace = 0;
        app.open_sprites_for_view(&mut view, None);
        view.sprite_ui.pending_operation = Some("delayed-checkpoints".into());
        view.sprite_ui.pending_request =
            Some(app.state.sprites_snapshot.operations[0].request.clone());
        app.handle_sprites_key_for_view(&mut view, KeyEvent::from(KeyCode::Char('n')));
        assert_eq!(view.sprite_ui.screen, SpriteUiScreen::Create);
        app.state.sprites_snapshot.operations[0].status =
            crate::api::schema::SpriteOperationStatus::Succeeded;
        app.state.sprites_snapshot.operations[0].result =
            Some(crate::api::schema::SpriteResult::Checkpoints(vec![
                "late-checkpoint".into(),
            ]));
        view.sprite_ui.reconcile(&app.state);
        app.handle_sprites_key_for_view(&mut view, KeyEvent::from(KeyCode::Tab));
        app.handle_sprites_key_for_view(&mut view, KeyEvent::from(KeyCode::Tab));
        for character in "new-flow".chars() {
            app.handle_sprites_key_for_view(&mut view, KeyEvent::from(KeyCode::Char(character)));
        }
        let output = rendered_output(&app, &mut view, 100, 30);
        assert_eq!(view.sprite_ui.screen, SpriteUiScreen::Create);
        assert_eq!(view.sprite_ui.sprite_name, "new-flow");
        assert!(view.sprite_ui.manager_prompt.is_none());
        assert!(
            output.contains("new-flow") && output.contains("[Create]"),
            "Create stays visible and editable after the delayed result: {output}"
        );
    }
}
