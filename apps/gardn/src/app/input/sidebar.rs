use std::borrow::Cow;

use crate::app::state::{AgentPanelScope, AppState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorkspaceDropTarget {
    pub insert_idx: usize,
    pub group_idx: Option<usize>,
    pub indicator_row: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GroupDropTarget {
    pub insert_idx: usize,
    pub indicator_row: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GroupMenuAction {
    AllSpaces,
    Group(usize),
    Connection(crate::app::connection_scope::ConnectionScope),
    NewWorkspace,
    NewGroup,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AgentMenuAction {
    ThisSpace,
    ThisGroup,
    AllAgents,
    Connection(crate::app::connection_scope::ConnectionScope),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FilterMenuRow<A> {
    Heading(String),
    Separator,
    Item {
        label: String,
        count: Option<usize>,
        action: A,
    },
}

impl<A> FilterMenuRow<A> {
    pub(crate) fn label(&self) -> &str {
        match self {
            Self::Heading(label) | Self::Item { label, .. } => label,
            Self::Separator => "---",
        }
    }

    pub(crate) fn display_label(&self, show_counters: bool) -> Cow<'_, str> {
        match self {
            Self::Item {
                label,
                count: Some(count),
                ..
            } if show_counters => Cow::Owned(format!("{label} {count}")),
            _ => Cow::Borrowed(self.label()),
        }
    }

    pub(crate) fn action(&self) -> Option<&A> {
        match self {
            Self::Item { action, .. } => Some(action),
            Self::Heading(_) | Self::Separator => None,
        }
    }
}

pub(crate) fn group_menu_rows(
    state: &AppState,
    group_filter_enabled: bool,
    active_group: usize,
    connection_scope: &crate::app::connection_scope::ConnectionScope,
) -> Vec<FilterMenuRow<GroupMenuAction>> {
    let mut all_count = 0;
    let mut group_counts = vec![0; state.groups.len()];
    for (ws_idx, workspace) in state.workspaces.iter().enumerate() {
        if !crate::app::connection_scope::workspace_matches(state, ws_idx, connection_scope) {
            continue;
        }
        all_count += 1;
        if let Some(group_idx) = state.group_index_by_id(&workspace.group_id) {
            group_counts[group_idx] += 1;
        }
    }

    let all_marker = if group_filter_enabled { " " } else { "✓" };
    let mut rows = vec![
        FilterMenuRow::Heading("Groups".to_string()),
        FilterMenuRow::Item {
            label: format!("{all_marker} All"),
            count: Some(all_count),
            action: GroupMenuAction::AllSpaces,
        },
    ];
    rows.extend(state.groups.iter().enumerate().map(|(idx, group)| {
        let marker = if group_filter_enabled && idx == active_group {
            "✓"
        } else {
            " "
        };
        FilterMenuRow::Item {
            label: format!("{marker} {} {}", group.icon, group.name),
            count: Some(group_counts[idx]),
            action: GroupMenuAction::Group(idx),
        }
    }));
    rows.extend([
        FilterMenuRow::Separator,
        FilterMenuRow::Heading("Connections".to_string()),
    ]);
    rows.extend(
        crate::app::connection_scope::choices(state)
            .into_iter()
            .map(|(scope, label)| FilterMenuRow::Item {
                label: format!(
                    "{} {label}",
                    if &scope == connection_scope {
                        "✓"
                    } else {
                        " "
                    }
                ),
                count: None,
                action: GroupMenuAction::Connection(scope),
            }),
    );
    rows.extend([
        FilterMenuRow::Separator,
        FilterMenuRow::Heading("New".to_string()),
        FilterMenuRow::Item {
            label: "  Space".to_string(),
            count: None,
            action: GroupMenuAction::NewWorkspace,
        },
        FilterMenuRow::Item {
            label: "  Group".to_string(),
            count: None,
            action: GroupMenuAction::NewGroup,
        },
    ]);
    rows
}

pub(crate) fn agent_menu_rows(
    state: &AppState,
    agent_scope: AgentPanelScope,
    connection_scope: &crate::app::connection_scope::ConnectionScope,
) -> Vec<FilterMenuRow<AgentMenuAction>> {
    let marker = |scope| if agent_scope == scope { "✓" } else { " " };
    let mut rows = vec![
        FilterMenuRow::Heading("Agents".to_string()),
        FilterMenuRow::Item {
            label: format!("{} All", marker(AgentPanelScope::AllWorkspaces)),
            count: None,
            action: AgentMenuAction::AllAgents,
        },
        FilterMenuRow::Item {
            label: format!("{} Space", marker(AgentPanelScope::CurrentWorkspace)),
            count: None,
            action: AgentMenuAction::ThisSpace,
        },
        FilterMenuRow::Item {
            label: format!("{} Group", marker(AgentPanelScope::CurrentGroup)),
            count: None,
            action: AgentMenuAction::ThisGroup,
        },
        FilterMenuRow::Separator,
        FilterMenuRow::Heading("Connections".to_string()),
    ];
    rows.extend(
        crate::app::connection_scope::choices(state)
            .into_iter()
            .map(|(scope, label)| FilterMenuRow::Item {
                label: format!(
                    "{} {label}",
                    if &scope == connection_scope {
                        "✓"
                    } else {
                        " "
                    }
                ),
                count: None,
                action: AgentMenuAction::Connection(scope),
            }),
    );
    rows
}

#[cfg(test)]
mod tests {
    use ratatui::{backend::TestBackend, layout::Rect, Terminal};

    use super::super::app_for_mouse_test;
    use super::*;
    use crate::{
        app::state::{AgentPanelScope, Mode},
        detect::{Agent, AgentState},
        workspace::Workspace,
    };

    fn render_app(app: &mut crate::app::App, width: u16, height: u16) -> String {
        crate::ui::compute_view(
            &app.state,
            &mut app.default_client_view,
            &app.terminal_runtimes,
            Rect::new(0, 0, width, height),
            crate::kitty_graphics::HostCellSize::default(),
            crate::ui::PaneResizeAuthority::Denied,
        );
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test backend");
        terminal
            .draw(|frame| {
                crate::ui::render(
                    &app.state,
                    &app.default_client_view,
                    &app.terminal_runtimes,
                    frame,
                )
            })
            .expect("render app");
        let buffer = terminal.backend().buffer();
        let mut text = String::new();
        for y in 0..height {
            for x in 0..width {
                text.push_str(buffer[(x, y)].symbol());
            }
            text.push('\n');
        }
        text
    }

    #[test]
    fn active_agent_tab_renders_label_and_status() {
        let mut app = app_for_mouse_test();
        let mut workspace = Workspace::test_new("agent-space");
        workspace.tabs[0].set_custom_name("planner".into());
        app.state.workspaces = vec![workspace];
        app.state.ensure_test_terminals();
        let pane_id = app.state.workspaces[0]
            .terminal_tab(0)
            .expect("terminal tab")
            .root_pane;
        let terminal_id = app.state.workspaces[0].terminal_tab(0).unwrap().panes[&pane_id]
            .attached_terminal_id
            .clone();
        let terminal = app
            .state
            .terminals
            .get_mut(&terminal_id)
            .expect("test terminal");
        terminal.set_agent_name("planner".to_string());
        terminal.set_manual_label("planner".to_string());
        terminal.set_detected_state(Some(Agent::Codex), AgentState::Working);
        terminal.state = AgentState::Working;
        app.default_client_view.active_workspace = Some(0);
        app.default_client_view.selected_workspace = 0;
        app.default_client_view.mode = Mode::Terminal;

        let rendered = render_app(&mut app, 120, 30);

        assert!(rendered.contains("planner"), "rendered UI:\n{rendered}");
        assert!(rendered.contains("Working"), "rendered UI:\n{rendered}");
    }

    #[test]
    fn group_menu_keeps_row_order_with_compact_section_labels() {
        let mut app = app_for_mouse_test();
        app.state.groups[0].name = "Home".to_string();
        app.state.groups[0].icon = "*".to_string();
        let work_group = app.state.create_group("Work".to_string());
        app.state.groups[work_group].icon = "+".to_string();
        app.state.workspaces = vec![Workspace::test_new("a"), Workspace::test_new("b")];
        app.state.workspaces[1].group_id = app.state.groups[work_group].id.clone();
        app.state.ssh_connection_profiles =
            vec![crate::persist::ssh_profiles::SshConnectionProfile::new(
                "workbox", "Work box", "workbox", None,
            )
            .expect("valid SSH profile")];

        let labels = group_menu_rows(
            &app.state,
            false,
            0,
            &app.default_client_view.connection_scope,
        )
        .iter()
        .map(|row| row.display_label(true).into_owned())
        .collect::<Vec<_>>();

        assert_eq!(
            labels,
            vec![
                "Groups",
                "✓ All 2",
                "  * Home 1",
                "  + Work 1",
                "---",
                "Connections",
                "✓ All",
                "  test-host",
                "  Work box",
                "---",
                "New",
                "  Space",
                "  Group",
            ]
        );
    }

    #[test]
    fn agent_menu_retains_primary_choices_and_compact_connection_label() {
        let mut app = app_for_mouse_test();
        app.state.ssh_connection_profiles.clear();

        let labels = agent_menu_rows(
            &app.state,
            AgentPanelScope::CurrentWorkspace,
            &app.default_client_view.connection_scope,
        )
        .into_iter()
        .map(|row| row.label().to_string())
        .collect::<Vec<_>>();

        assert_eq!(
            labels,
            vec![
                "Agents",
                "  All",
                "✓ Space",
                "  Group",
                "---",
                "Connections",
                "✓ All",
                "  test-host",
            ]
        );
    }
}
