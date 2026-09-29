use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::app::sprites_ui::SpriteUiScreen;
use crate::app::state::Palette;
use crate::app::{AppState, ClientViewState};

pub(crate) fn render_sprites_overlay(
    app: &AppState,
    view: &ClientViewState,
    frame: &mut Frame,
    area: Rect,
) {
    let popup = centered(area, 74, 22);
    let palette = &app.palette;
    let background = Block::default()
        .borders(Borders::ALL)
        .title(match view.sprite_ui.screen {
            SpriteUiScreen::Manager => " Sprites · search / actions ",
            SpriteUiScreen::Create => " New Sprite · local source only ",
            SpriteUiScreen::Settings => " Settings → Integrations → Sprites ",
            SpriteUiScreen::DisableConfirm => " Disable Sprites? ",
            SpriteUiScreen::Approval => " Confirm exact Sprite action ",
        })
        .border_style(Style::default().fg(palette.accent))
        .style(Style::default().bg(palette.panel_bg));
    let inner = background.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(background, popup);
    match view.sprite_ui.screen {
        SpriteUiScreen::Manager => render_manager(app, view, frame, inner, palette),
        SpriteUiScreen::Approval => render_approval(view, frame, inner, palette),
        SpriteUiScreen::Create => render_create(app, view, frame, inner, palette),
        SpriteUiScreen::Settings => render_settings(app, view, frame, inner, palette),
        SpriteUiScreen::DisableConfirm => render_disable_confirm(view, frame, inner, palette),
    }
}
fn render_approval(view: &ClientViewState, frame: &mut Frame, area: Rect, palette: &Palette) {
    let approval = view.sprite_ui.approval.as_ref();
    let lines = [
        format!("Action: {}", approval.map_or("unknown", |value| value.action.as_str())),
        format!("Resource: {}", approval.map_or("unknown", |value| value.sprite_id.as_str())),
        format!("Revision: {}", approval.map_or(0, |value| value.revision)),
        match &view.sprite_ui.pending_command {
            Some(crate::api::schema::SpriteCommand::Restore(target)) =>
                format!("Checkpoint: {}", target.checkpoint_id.as_deref().unwrap_or("unknown")),
            _ => String::new(),
        },
        approval.map_or("No approval is pending.".to_string(), |value| value.summary.clone()),
        "This single-use approval expires and is bound to this exact action, resource, and revision.".to_string(),
    ];
    let [body, footer] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
    frame.render_widget(
        Paragraph::new(lines.join("\n"))
            .style(Style::default().fg(palette.text))
            .wrap(Wrap { trim: true }),
        body,
    );
    frame.render_widget(
        Paragraph::new("[Approve]       [Cancel]").style(Style::default().fg(palette.accent)),
        footer,
    );
}

fn render_manager(
    app: &AppState,
    view: &ClientViewState,
    frame: &mut Frame,
    area: Rect,
    palette: &Palette,
) {
    let state = &view.sprite_ui;
    let [search, list, detail, footer] = crate::app::sprites_ui::sprite_manager_layout(area, state);
    let records = state.visible_records(&app.sprites_snapshot);
    let record = records.get(state.selected).copied();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64);
    let observed = record.map_or_else(
        || freshness_label(now, app.sprites_snapshot.observed_unix_ms, "Inventory"),
        |record| freshness_label(now, record.observed_unix_ms, "Sprite"),
    );
    let heading = match state.manager_prompt.as_ref() {
        Some(crate::app::sprites_ui::SpriteManagerPrompt::OperationPicker) => {
            "Operations · newest first".to_string()
        }
        Some(crate::app::sprites_ui::SpriteManagerPrompt::CheckpointPicker { .. }) => {
            "Choose exact checkpoint".to_string()
        }
        Some(crate::app::sprites_ui::SpriteManagerPrompt::SessionPicker { action, .. }) => {
            format!("Choose exact {action:?} session")
        }
        Some(crate::app::sprites_ui::SpriteManagerPrompt::Resume { reference }) => {
            format!("Conversation reference: {reference}")
        }
        Some(crate::app::sprites_ui::SpriteManagerPrompt::Search) | None => format!(
            "{} · {}: {}",
            if state.all_resources { "All" } else { "Space" },
            if matches!(
                state.manager_prompt.as_ref(),
                Some(crate::app::sprites_ui::SpriteManagerPrompt::Search)
            ) {
                "Typing search"
            } else {
                "/ Search"
            },
            state.search
        ),
    };
    let [search_text, back] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(7)]).areas(search);
    frame.render_widget(
        Paragraph::new(format!("{heading}\n{observed}")).style(Style::default().fg(palette.text)),
        search_text,
    );
    frame.render_widget(
        Paragraph::new("[Back]").style(Style::default().fg(palette.accent)),
        back,
    );

    let selection = state.list_selection();
    let visible_rows = list.height.saturating_sub(1) as usize;
    let offset = selection.saturating_sub(visible_rows / 2);
    let mut rows: Vec<String> = match state.manager_prompt.as_ref() {
        Some(crate::app::sprites_ui::SpriteManagerPrompt::OperationPicker) => app
            .sprites_snapshot
            .operations
            .iter()
            .rev()
            .skip(offset)
            .take(visible_rows)
            .map(|op| format!("{:?} · {} · {}", op.status, op.stage, op.request.request_id))
            .collect(),
        Some(crate::app::sprites_ui::SpriteManagerPrompt::CheckpointPicker { .. }) => state
            .checkpoints
            .iter()
            .skip(offset)
            .take(visible_rows)
            .cloned()
            .collect(),
        Some(crate::app::sprites_ui::SpriteManagerPrompt::SessionPicker { .. }) => record
            .into_iter()
            .flat_map(|record| record.sessions.iter())
            .skip(offset)
            .take(visible_rows)
            .map(|session| {
                format!(
                    "{} · {} · {}",
                    session.id,
                    if session.owned { "owned" } else { "foreign" },
                    session.command.join(" ")
                )
            })
            .collect(),
        _ => records
            .iter()
            .skip(offset)
            .take(visible_rows)
            .map(|record| {
                format!(
                    "{} · {} · {}",
                    record.name,
                    record.phase,
                    if record.attached_panes.is_empty() {
                        "detached"
                    } else {
                        "attached"
                    }
                )
            })
            .collect(),
    };
    if rows.is_empty() {
        rows.push("No matching items. Refresh or create explicitly.".into());
    }
    let lines: Vec<Line<'_>> = rows
        .into_iter()
        .enumerate()
        .map(|(index, row)| {
            let selected = offset + index == selection;
            Line::from(Span::styled(
                format!("{} {row}", marker(selected)),
                if selected {
                    Style::default()
                        .fg(palette.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(palette.text)
                },
            ))
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(palette.surface_dim)),
        ),
        list,
    );

    let tracked = state.pending_operation.as_ref().and_then(|id| {
        app.sprites_snapshot
            .operations
            .iter()
            .find(|operation| &operation.id == id)
    });
    let mut details = Vec::new();
    if let Some(error) = &app.sprites_snapshot.observation_error {
        details.push(format!("Inventory error: {}", error.message));
    }
    if let Some(message) = &state.message {
        details.push(message.clone());
    }
    if let Some(operation) = tracked {
        details.push(sprite_operation_summary(operation));
        details.push(format!("Operation: {}", operation.id));
    }
    if let Some(record) = record {
        if state.preview_resource_id.as_deref() == Some(record.id.as_str())
            && state.preview_revision == Some(record.revision)
        {
            if let Some(preview) = &state.transfer_preview {
                details.push(format!(
                    "Preview: {} files · {} bytes · P confirms",
                    preview.files, preview.bytes
                ));
                details.extend(
                    preview
                        .conflicts
                        .iter()
                        .map(|path| format!("CONFLICT: {path}")),
                );
                details.extend(
                    preview
                        .changed_paths
                        .iter()
                        .map(|path| format!("Change: {path}")),
                );
                details.extend(
                    preview
                        .excluded
                        .iter()
                        .map(|path| format!("Excluded: {path}")),
                );
            }
        }
        details.push(format!(
            "{} · {}",
            record.id,
            if record.managed {
                "Gardn-managed"
            } else {
                "foreign"
            }
        ));
        details.push(format!(
            "Space: {} · {} session(s)",
            record.workspace_id.as_deref().unwrap_or("unassociated"),
            record.sessions.len()
        ));
        if let Some(source) = &record.source {
            details.push(format!(
                "Source: {} · {}",
                source.execution_host_id, source.path
            ));
        }
        if let Some(agent) = &record.agent {
            details.push(format!(
                "Agent: {} · {:?}",
                if agent.profile_id.is_empty() {
                    &agent.kind
                } else {
                    &agent.profile_id
                },
                record
                    .agent_status
                    .unwrap_or(crate::api::schema::AgentStatus::Unknown)
            ));
        }
        details.push(
            match record.unpulled_changes {
                Some(true) => "Unpulled changes exist.",
                Some(false) => "No changes found by the last pull preview.",
                None => "Unpulled changes unknown. Preview before cleanup.",
            }
            .into(),
        );
        if let Some(error) = &record.last_error {
            details.push(error.clone());
        }
        if let Some((id, checkpoint)) = &state.selected_checkpoint {
            if id == &record.id {
                details.push(format!("Selected checkpoint: {checkpoint}"));
            }
        }
    }
    let detail_text = Paragraph::new(details.join("\n"))
        .style(Style::default().fg(palette.subtext0))
        .wrap(Wrap { trim: true });
    let max_scroll = detail_text
        .line_count(detail.width)
        .saturating_sub(detail.height as usize)
        .min(u16::MAX as usize) as u16;
    frame.render_widget(
        detail_text.scroll((state.detail_scroll.min(max_scroll), 0)),
        detail,
    );

    if state.is_prompt() {
        frame.render_widget(
            Paragraph::new("[Confirm]  [Cancel]").style(Style::default().fg(palette.accent)),
            footer,
        );
    } else {
        let lines: Vec<Line<'_>> = crate::app::sprites_ui::SpriteUiState::action_rows(area.width)
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|(label, key)| {
                        let key = match key {
                            crossterm::event::KeyCode::Char(key) => key.to_string(),
                            _ => String::new(),
                        };
                        Span::styled(
                            format!("[{key} {label}] "),
                            Style::default().fg(palette.accent),
                        )
                    })
                    .collect()
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), footer);
    }
}
fn render_create(
    app: &AppState,
    view: &ClientViewState,
    frame: &mut Frame,
    area: Rect,
    palette: &Palette,
) {
    let source_choice = if view.sprite_ui.source_candidates.is_empty() {
        view.sprite_ui.source_path.clone()
    } else {
        format!(
            "{} ({}/{})",
            view.sprite_ui.source_path,
            view.sprite_ui.source_index + 1,
            view.sprite_ui.source_candidates.len()
        )
    };
    let profile_choice = app
        .agent_profiles
        .profiles()
        .iter()
        .find(|profile| profile.id == view.sprite_ui.profile_id)
        .map_or_else(
            || "(no compatible installed profile)".to_string(),
            |profile| {
                format!(
                    "{} · {} ({})",
                    profile.name,
                    profile.kind.display_name(),
                    profile.id
                )
            },
        );
    let lines = [
        format!(
            "{} Source host: {}",
            marker(view.sprite_ui.field == 0),
            view.sprite_ui.source_host_id
        ),
        format!(
            "{} Source repository: {}",
            marker(view.sprite_ui.field == 0),
            source_choice
        ),
        format!(
            "{} Agent profile: {}",
            marker(view.sprite_ui.field == 1),
            profile_choice
        ),
        format!(
            "{} Sprite name: {}",
            marker(view.sprite_ui.field == 2),
            if view.sprite_ui.sprite_name.is_empty() {
                "(automatic)"
            } else {
                view.sprite_ui.sprite_name.as_str()
            }
        ),
        format!(
            "{} Share credentials (explicit): {}",
            marker(view.sprite_ui.field == 3),
            if view.sprite_ui.share_credentials {
                "yes"
            } else {
                "no"
            }
        ),
        format!("{} Create", marker(view.sprite_ui.field == 4)),
    ];
    let notes = [
        view.sprite_ui.message.clone().unwrap_or_default(),
        "Use ←/→ to select a repo/profile. Compatible favorites are first.".to_string(),
        "Local environment/wrappers and SSH source transfer are unavailable.".to_string(),
    ];
    let [fields, notes_area, footer] = Layout::vertical([
        Constraint::Length(6),
        Constraint::Min(0),
        Constraint::Length(2),
    ])
    .areas(area);
    let scroll_offset = view
        .sprite_ui
        .field
        .saturating_add(1)
        .saturating_sub(fields.height.saturating_sub(1) as usize);
    frame.render_widget(
        Paragraph::new(lines.join("\n"))
            .scroll((scroll_offset as u16, 0))
            .style(Style::default().fg(palette.text)),
        fields,
    );
    frame.render_widget(
        Paragraph::new(notes.join("\n"))
            .style(Style::default().fg(palette.subtext0))
            .wrap(Wrap { trim: true }),
        notes_area,
    );
    frame.render_widget(
        Paragraph::new("[Create]    [Cancel]").style(Style::default().fg(palette.accent)),
        footer,
    );
}
fn render_settings(
    app: &AppState,
    view: &ClientViewState,
    frame: &mut Frame,
    area: Rect,
    palette: &Palette,
) {
    let lines = [
        format!(
            "{} Enabled: {}",
            marker(view.sprite_ui.field == 0),
            if view.sprite_ui.enabled { "yes" } else { "no" }
        ),
        format!(
            "{} Sprite organization: {}",
            marker(view.sprite_ui.field == 1),
            view.sprite_ui.org
        ),
        format!(
            "{} Sprite CLI: {}",
            marker(view.sprite_ui.field == 2),
            view.sprite_ui.sprite_bin
        ),
        format!(
            "{} Node executable: {}",
            marker(view.sprite_ui.field == 3),
            view.sprite_ui.node_bin
        ),
        format!(
            "{} Managed-name prefix (lowercase): {}",
            marker(view.sprite_ui.field == 4),
            view.sprite_ui.name_prefix
        ),
        format!(
            "{} Maximum managed Sprites: {}",
            marker(view.sprite_ui.field == 5),
            view.sprite_ui.max_sprites
        ),
        format!(
            "{} Concurrent operations (1–64): {}",
            marker(view.sprite_ui.field == 6),
            view.sprite_ui.max_concurrent_operations
        ),
        format!(
            "{} Maximum transfer (MiB, 1–512): {}",
            marker(view.sprite_ui.field == 7),
            view.sprite_ui.max_transfer_mib
        ),
        format!("{} Save and reload", marker(view.sprite_ui.field == 8)),
    ];
    let notes = [
        view.sprite_ui.message.clone().unwrap_or_default(),
        "Authentication is manual; credentials are never persisted here.".to_string(),
        "Disable/unlink any legacy Sprites plugin separately before using native launch."
            .to_string(),
    ];
    let [body, notes_area, footer] = Layout::vertical([
        Constraint::Length(9),
        Constraint::Min(0),
        Constraint::Length(2),
    ])
    .areas(area);
    let scroll_offset = view
        .sprite_ui
        .field
        .saturating_sub(body.height.saturating_sub(1) as usize);
    frame.render_widget(
        Paragraph::new(lines.join("\n"))
            .scroll((scroll_offset as u16, 0))
            .style(Style::default().fg(palette.text)),
        body,
    );
    frame.render_widget(
        Paragraph::new(notes.join("\n"))
            .style(Style::default().fg(palette.subtext0))
            .wrap(Wrap { trim: true }),
        notes_area,
    );
    frame.render_widget(
        Paragraph::new("[Save]    [Back]").style(Style::default().fg(palette.accent)),
        footer,
    );
    let _ = app;
}

fn render_disable_confirm(
    view: &ClientViewState,
    frame: &mut Frame,
    area: Rect,
    palette: &Palette,
) {
    let lines = [
        "Disabling fences new work and stops local workers.".to_string(),
        "Remote Sprites and their sessions continue living; nothing is deleted.".to_string(),
        "You can explicitly Destroy or Forget resources later by re-enabling the manager."
            .to_string(),
        view.sprite_ui
            .message
            .clone()
            .unwrap_or_else(|| "Disable Sprites?".into()),
    ];
    let [body, footer] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
    frame.render_widget(
        Paragraph::new(lines.join("\n"))
            .style(Style::default().fg(palette.text))
            .wrap(Wrap { trim: true }),
        body,
    );
    frame.render_widget(
        Paragraph::new("[Disable locally]     [Keep enabled]")
            .style(Style::default().fg(palette.accent)),
        footer,
    );
}
fn freshness_label(now: u64, observed: Option<u64>, subject: &str) -> String {
    observed.map_or_else(
        || format!("{subject} observation unknown"),
        |time| {
            let age = now.saturating_sub(time);
            if age > 30_000 {
                format!("{subject} stale · {}s ago", age / 1_000)
            } else {
                format!("{subject} fresh · {}s ago", age / 1_000)
            }
        },
    )
}
fn sprite_operation_summary(operation: &crate::api::schema::SpriteOperation) -> String {
    let result = match operation.stage.as_str() {
        "transport_open" => {
            "Local terminal opened; remote attachment is unconfirmed. Inspect terminal output."
        }
        "session_started" => "Remote session observed.",
        stage => stage,
    };
    match &operation.error {
        Some(error) => format!("{:?} · {} · {}", operation.status, result, error.message),
        None => format!("{:?} · {}", operation.status, result),
    }
}

fn marker(selected: bool) -> &'static str {
    if selected {
        ">"
    } else {
        " "
    }
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.max(1));
    let height = height.min(area.height.max(1));
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}
