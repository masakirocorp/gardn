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
    pub action: CommandPaletteAction,
}

impl CommandPaletteCommand {
    fn new(title: impl Into<String>, group: &'static str, action: CommandPaletteAction) -> Self {
        Self {
            title: title.into(),
            group,
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
        CommandPaletteCommand::new("fullscreen pane", "panes", CommandPaletteAction::Fullscreen),
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

    commands
}

pub(crate) fn command_palette_filtered_commands(state: &AppState) -> Vec<CommandPaletteCommand> {
    let query = state.command_palette.query.as_str();
    command_palette_commands(state)
        .into_iter()
        .filter(|command| command.matches(query))
        .collect()
}
