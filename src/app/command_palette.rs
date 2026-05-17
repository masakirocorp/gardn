use crate::{app::state::AgentPanelScope, layout::NavDirection};

use super::AppState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CommandPaletteAction {
    NewWorkspace,
    RenameWorkspace,
    CloseWorkspace,
    PreviousWorkspace,
    NextWorkspace,
    SwitchWorkspace(usize),
    NewTab,
    RenameTab,
    PreviousTab,
    NextTab,
    CloseTab,
    SplitVertical,
    SplitHorizontal,
    ClosePane,
    RenamePane,
    Fullscreen,
    ResizeMode,
    FocusPane(NavDirection),
    CyclePaneNext,
    CyclePanePrevious,
    OpenGroupMenu,
    ShowAllGroups,
    NewGroup,
    RenameGroup,
    DeleteGroup,
    ToggleGroupFilter,
    PreviousGroup,
    NextGroup,
    SwitchGroup(usize),
    OpenAgentMenu,
    SetAgentScope(AgentPanelScope),
    PreviousAgent,
    NextAgent,
    ToggleSidebar,
    ToggleRightSidebar,
    OpenGlobalMenu,
    OpenSettings,
    OpenKeybinds,
    ReloadConfig,
    OpenNotificationTarget,
    DetachOrQuit,
    CustomCommand(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandPaletteCommand {
    pub title: String,
    pub group: &'static str,
    pub key_label: Option<String>,
    pub action: CommandPaletteAction,
}

impl CommandPaletteCommand {
    fn new(title: impl Into<String>, group: &'static str, action: CommandPaletteAction) -> Self {
        Self {
            title: title.into(),
            group,
            key_label: None,
            action,
        }
    }

    fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_ascii_lowercase();
        if query.is_empty() {
            return true;
        }

        let haystack = format!("{} {}", self.title, self.group).to_ascii_lowercase();
        query.split_whitespace().all(|term| haystack.contains(term))
    }
}

fn command_palette_group_order(group: &str) -> usize {
    match group {
        "spaces" => 0,
        "tabs" => 1,
        "panes" => 2,
        "groups" => 3,
        "agents" => 4,
        "layout" => 5,
        "app" => 6,
        "custom" => 7,
        _ => 8,
    }
}

pub(crate) fn command_palette_commands(state: &AppState) -> Vec<CommandPaletteCommand> {
    let mut commands = vec![
        CommandPaletteCommand::new("new space", "spaces", CommandPaletteAction::NewWorkspace),
        CommandPaletteCommand::new(
            "rename selected space",
            "spaces",
            CommandPaletteAction::RenameWorkspace,
        ),
        CommandPaletteCommand::new(
            "close selected space",
            "spaces",
            CommandPaletteAction::CloseWorkspace,
        ),
        CommandPaletteCommand::new(
            "previous space",
            "spaces",
            CommandPaletteAction::PreviousWorkspace,
        ),
        CommandPaletteCommand::new("next space", "spaces", CommandPaletteAction::NextWorkspace),
        CommandPaletteCommand::new("new tab", "tabs", CommandPaletteAction::NewTab),
        CommandPaletteCommand::new("rename tab", "tabs", CommandPaletteAction::RenameTab),
        CommandPaletteCommand::new("previous tab", "tabs", CommandPaletteAction::PreviousTab),
        CommandPaletteCommand::new("next tab", "tabs", CommandPaletteAction::NextTab),
        CommandPaletteCommand::new("close tab", "tabs", CommandPaletteAction::CloseTab),
        CommandPaletteCommand::new(
            "split pane vertical",
            "panes",
            CommandPaletteAction::SplitVertical,
        ),
        CommandPaletteCommand::new(
            "split pane horizontal",
            "panes",
            CommandPaletteAction::SplitHorizontal,
        ),
        CommandPaletteCommand::new("close pane", "panes", CommandPaletteAction::ClosePane),
        CommandPaletteCommand::new("rename pane", "panes", CommandPaletteAction::RenamePane),
        CommandPaletteCommand::new("zoom pane", "panes", CommandPaletteAction::Fullscreen),
        CommandPaletteCommand::new("resize panes", "panes", CommandPaletteAction::ResizeMode),
        CommandPaletteCommand::new(
            "focus pane left",
            "panes",
            CommandPaletteAction::FocusPane(NavDirection::Left),
        ),
        CommandPaletteCommand::new(
            "focus pane down",
            "panes",
            CommandPaletteAction::FocusPane(NavDirection::Down),
        ),
        CommandPaletteCommand::new(
            "focus pane up",
            "panes",
            CommandPaletteAction::FocusPane(NavDirection::Up),
        ),
        CommandPaletteCommand::new(
            "focus pane right",
            "panes",
            CommandPaletteAction::FocusPane(NavDirection::Right),
        ),
        CommandPaletteCommand::new(
            "cycle pane next",
            "panes",
            CommandPaletteAction::CyclePaneNext,
        ),
        CommandPaletteCommand::new(
            "cycle pane previous",
            "panes",
            CommandPaletteAction::CyclePanePrevious,
        ),
        CommandPaletteCommand::new(
            "open group menu",
            "groups",
            CommandPaletteAction::OpenGroupMenu,
        ),
        CommandPaletteCommand::new(
            "show all spaces",
            "groups",
            CommandPaletteAction::ShowAllGroups,
        ),
        CommandPaletteCommand::new("new group", "groups", CommandPaletteAction::NewGroup),
        CommandPaletteCommand::new("rename group", "groups", CommandPaletteAction::RenameGroup),
        CommandPaletteCommand::new("delete group", "groups", CommandPaletteAction::DeleteGroup),
        CommandPaletteCommand::new(
            "toggle current/all groups",
            "groups",
            CommandPaletteAction::ToggleGroupFilter,
        ),
        CommandPaletteCommand::new(
            "previous group",
            "groups",
            CommandPaletteAction::PreviousGroup,
        ),
        CommandPaletteCommand::new("next group", "groups", CommandPaletteAction::NextGroup),
        CommandPaletteCommand::new(
            "open agent menu",
            "agents",
            CommandPaletteAction::OpenAgentMenu,
        ),
        CommandPaletteCommand::new(
            "agents: this space",
            "agents",
            CommandPaletteAction::SetAgentScope(AgentPanelScope::CurrentWorkspace),
        ),
        CommandPaletteCommand::new(
            "agents: this group",
            "agents",
            CommandPaletteAction::SetAgentScope(AgentPanelScope::CurrentGroup),
        ),
        CommandPaletteCommand::new(
            "agents: all agents",
            "agents",
            CommandPaletteAction::SetAgentScope(AgentPanelScope::AllWorkspaces),
        ),
        CommandPaletteCommand::new(
            "previous agent",
            "agents",
            CommandPaletteAction::PreviousAgent,
        ),
        CommandPaletteCommand::new("next agent", "agents", CommandPaletteAction::NextAgent),
        CommandPaletteCommand::new(
            "toggle sidebar",
            "layout",
            CommandPaletteAction::ToggleSidebar,
        ),
        CommandPaletteCommand::new(
            "toggle right sidebar",
            "layout",
            CommandPaletteAction::ToggleRightSidebar,
        ),
        CommandPaletteCommand::new(
            "open global menu",
            "app",
            CommandPaletteAction::OpenGlobalMenu,
        ),
        CommandPaletteCommand::new("open settings", "app", CommandPaletteAction::OpenSettings),
        CommandPaletteCommand::new("open keybinds", "app", CommandPaletteAction::OpenKeybinds),
        CommandPaletteCommand::new("reload config", "app", CommandPaletteAction::ReloadConfig),
        CommandPaletteCommand::new(
            "open notification target",
            "app",
            CommandPaletteAction::OpenNotificationTarget,
        ),
        CommandPaletteCommand::new("detach / quit", "app", CommandPaletteAction::DetachOrQuit),
    ];

    commands.extend(
        state
            .visible_workspace_indices()
            .into_iter()
            .filter_map(|idx| {
                state.workspaces.get(idx).map(|workspace| {
                    CommandPaletteCommand::new(
                        format!("switch to space: {}", workspace.display_name()),
                        "spaces",
                        CommandPaletteAction::SwitchWorkspace(idx),
                    )
                })
            }),
    );

    commands.extend(state.groups.iter().enumerate().map(|(idx, group)| {
        CommandPaletteCommand::new(
            format!("switch to group: {} {}", group.icon, group.name),
            "groups",
            CommandPaletteAction::SwitchGroup(idx),
        )
    }));

    commands.extend(
        state
            .keybinds
            .custom_commands
            .iter()
            .enumerate()
            .map(|(idx, binding)| {
                CommandPaletteCommand::new(
                    format!("run command: {}", binding.command),
                    "custom",
                    CommandPaletteAction::CustomCommand(idx),
                )
            }),
    );

    for command in &mut commands {
        command.key_label = command_palette_key_label(state, &command.action);
    }

    commands
}

fn command_palette_key_label(state: &AppState, action: &CommandPaletteAction) -> Option<String> {
    let kb = &state.keybinds;
    match action {
        CommandPaletteAction::NewWorkspace => Some(kb.new_workspace_label.clone()),
        CommandPaletteAction::RenameWorkspace => Some(kb.rename_workspace_label.clone()),
        CommandPaletteAction::CloseWorkspace => Some(kb.close_workspace_label.clone()),
        CommandPaletteAction::PreviousWorkspace => kb.previous_workspace_label.clone(),
        CommandPaletteAction::NextWorkspace => kb.next_workspace_label.clone(),
        CommandPaletteAction::NewTab => Some(kb.new_tab_label.clone()),
        CommandPaletteAction::RenameTab => kb.rename_tab_label.clone(),
        CommandPaletteAction::PreviousTab => kb.previous_tab_label.clone(),
        CommandPaletteAction::NextTab => kb.next_tab_label.clone(),
        CommandPaletteAction::CloseTab => kb.close_tab_label.clone(),
        CommandPaletteAction::SplitVertical => Some(kb.split_vertical_label.clone()),
        CommandPaletteAction::SplitHorizontal => Some(kb.split_horizontal_label.clone()),
        CommandPaletteAction::ClosePane => Some(kb.close_pane_label.clone()),
        CommandPaletteAction::RenamePane => kb.rename_pane_label.clone(),
        CommandPaletteAction::Fullscreen => Some(kb.zoom_label.clone()),
        CommandPaletteAction::ResizeMode => Some(kb.resize_mode_label.clone()),
        CommandPaletteAction::FocusPane(crate::layout::NavDirection::Left) => kb
            .focus_pane_left_label
            .clone()
            .or_else(|| Some("h".into())),
        CommandPaletteAction::FocusPane(crate::layout::NavDirection::Down) => kb
            .focus_pane_down_label
            .clone()
            .or_else(|| Some("j".into())),
        CommandPaletteAction::FocusPane(crate::layout::NavDirection::Up) => {
            kb.focus_pane_up_label.clone().or_else(|| Some("k".into()))
        }
        CommandPaletteAction::FocusPane(crate::layout::NavDirection::Right) => kb
            .focus_pane_right_label
            .clone()
            .or_else(|| Some("l".into())),
        CommandPaletteAction::CyclePaneNext => Some("tab".into()),
        CommandPaletteAction::CyclePanePrevious => Some("shift+tab".into()),
        CommandPaletteAction::OpenGroupMenu => kb.open_group_menu_label.clone(),
        CommandPaletteAction::NewGroup => kb.new_group_label.clone(),
        CommandPaletteAction::RenameGroup => kb.rename_group_label.clone(),
        CommandPaletteAction::DeleteGroup => kb.delete_group_label.clone(),
        CommandPaletteAction::ToggleGroupFilter => kb.toggle_group_filter_label.clone(),
        CommandPaletteAction::PreviousGroup => kb.previous_group_label.clone(),
        CommandPaletteAction::NextGroup => kb.next_group_label.clone(),
        CommandPaletteAction::OpenAgentMenu => kb.open_agent_menu_label.clone(),
        CommandPaletteAction::PreviousAgent => kb.previous_agent_label.clone(),
        CommandPaletteAction::NextAgent => kb.next_agent_label.clone(),
        CommandPaletteAction::ToggleSidebar => Some(kb.toggle_sidebar_label.clone()),
        CommandPaletteAction::ToggleRightSidebar => kb.toggle_right_sidebar_label.clone(),
        CommandPaletteAction::OpenSettings => Some("s".into()),
        CommandPaletteAction::OpenKeybinds => Some("?".into()),
        CommandPaletteAction::ReloadConfig => kb.reload_config_label.clone(),
        CommandPaletteAction::OpenNotificationTarget => kb.open_notification_target_label.clone(),
        CommandPaletteAction::DetachOrQuit => kb.detach_label.clone(),
        CommandPaletteAction::CustomCommand(idx) => kb
            .custom_commands
            .get(*idx)
            .map(|binding| binding.label.clone()),
        CommandPaletteAction::SwitchWorkspace(_)
        | CommandPaletteAction::ShowAllGroups
        | CommandPaletteAction::SwitchGroup(_)
        | CommandPaletteAction::SetAgentScope(_)
        | CommandPaletteAction::OpenGlobalMenu => None,
    }
}

pub(crate) fn command_palette_filtered_commands(state: &AppState) -> Vec<CommandPaletteCommand> {
    let query = state.command_palette.query.as_str();
    let mut commands = command_palette_commands(state)
        .into_iter()
        .enumerate()
        .filter(|(_, command)| command.matches(query))
        .collect::<Vec<_>>();

    commands.sort_by_key(|(idx, command)| (command_palette_group_order(command.group), *idx));
    commands.into_iter().map(|(_, command)| command).collect()
}
