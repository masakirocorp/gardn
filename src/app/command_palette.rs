use crate::{
    app::{agent_profile_picker::workspace_agent_profile_ids, state::AgentPanelScope, AppState},
    layout::NavDirection,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CommandPaletteAction {
    NewWorkspace,
    RenameWorkspace,
    CloseWorkspace,
    PreviousWorkspace,
    NextWorkspace,
    SwitchWorkspace(usize),
    SwitchTab(usize),
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
    EditScrollback,
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
    OpenGitDiff,
    ToggleSidebar,
    ToggleRightSidebar,
    OpenGlobalMenu,
    OpenSettings,
    OpenKeybinds,
    ReloadConfig,
    OpenNotificationTarget,
    DetachOrQuit,
    CustomCommand(usize),
    NewAgent,
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

    fn with_key_label(mut self, key_label: Option<String>) -> Self {
        self.key_label = key_label;
        self
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
        "git" => 4,
        "agents" => 5,
        "layout" => 6,
        "app" => 7,
        "custom" => 8,
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
        CommandPaletteCommand::new(
            "edit scrollback",
            "panes",
            CommandPaletteAction::EditScrollback,
        ),
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
            "agents: follow space",
            "agents",
            CommandPaletteAction::SetAgentScope(AgentPanelScope::CurrentWorkspace),
        ),
        CommandPaletteCommand::new(
            "agents: follow group",
            "agents",
            CommandPaletteAction::SetAgentScope(AgentPanelScope::CurrentGroup),
        ),
        CommandPaletteCommand::new(
            "agents: all",
            "agents",
            CommandPaletteAction::SetAgentScope(AgentPanelScope::AllWorkspaces),
        ),
        CommandPaletteCommand::new(
            "previous agent",
            "agents",
            CommandPaletteAction::PreviousAgent,
        ),
        CommandPaletteCommand::new("next agent", "agents", CommandPaletteAction::NextAgent),
        CommandPaletteCommand::new("open git diff", "git", CommandPaletteAction::OpenGitDiff),
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

    if let Some(ws) = state.active.and_then(|idx| state.workspaces.get(idx)) {
        commands.extend(ws.tabs.iter().enumerate().map(|(idx, tab)| {
            CommandPaletteCommand::new(
                format!("switch to tab: {}", tab.display_name()),
                "tabs",
                CommandPaletteAction::SwitchTab(idx),
            )
            .with_key_label(indexed_keybind_label(&state.keybinds.switch_tab, idx))
        }));
    }

    commands.extend(
        state
            .visible_workspace_indices()
            .into_iter()
            .enumerate()
            .filter_map(|(shortcut_idx, idx)| {
                state.workspaces.get(idx).map(|workspace| {
                    CommandPaletteCommand::new(
                        format!("switch to space: {}", workspace.display_name()),
                        "spaces",
                        CommandPaletteAction::SwitchWorkspace(idx),
                    )
                    .with_key_label(indexed_keybind_label(
                        &state.keybinds.switch_workspace,
                        shortcut_idx,
                    ))
                })
            }),
    );

    commands.extend(state.groups.iter().enumerate().map(|(idx, group)| {
        CommandPaletteCommand::new(
            format!("switch to group: {} {}", group.icon, group.name),
            "groups",
            CommandPaletteAction::SwitchGroup(idx),
        )
        .with_key_label(indexed_keybind_label(&state.keybinds.switch_group, idx))
    }));

    if state
        .active
        .is_some_and(|ws_idx| workspace_agent_profile_ids(state, ws_idx).next().is_some())
    {
        commands.push(CommandPaletteCommand::new(
            "new agent",
            "agents",
            CommandPaletteAction::NewAgent,
        ));
    }

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
        if command.key_label.is_none() {
            command.key_label = command_palette_key_label(state, &command.action);
        }
    }

    commands
}

fn indexed_keybind_label(
    bindings: &[crate::config::IndexedKeybind],
    index: usize,
) -> Option<String> {
    bindings.get(index).map(|binding| binding.label.clone())
}

fn command_palette_key_label(state: &AppState, action: &CommandPaletteAction) -> Option<String> {
    let kb = &state.keybinds;
    let label = |bindings: &crate::config::ActionKeybinds| bindings.label();
    match action {
        CommandPaletteAction::NewWorkspace => label(&kb.new_workspace),
        CommandPaletteAction::RenameWorkspace => label(&kb.rename_workspace),
        CommandPaletteAction::CloseWorkspace => label(&kb.close_workspace),
        CommandPaletteAction::PreviousWorkspace => label(&kb.previous_workspace),
        CommandPaletteAction::NextWorkspace => label(&kb.next_workspace),
        CommandPaletteAction::NewTab => label(&kb.new_tab),
        CommandPaletteAction::SwitchTab(idx) => indexed_keybind_label(&kb.switch_tab, *idx),
        CommandPaletteAction::RenameTab => label(&kb.rename_tab),
        CommandPaletteAction::PreviousTab => label(&kb.previous_tab),
        CommandPaletteAction::NextTab => label(&kb.next_tab),
        CommandPaletteAction::CloseTab => label(&kb.close_tab),
        CommandPaletteAction::SplitVertical => label(&kb.split_vertical),
        CommandPaletteAction::SplitHorizontal => label(&kb.split_horizontal),
        CommandPaletteAction::ClosePane => label(&kb.close_pane),
        CommandPaletteAction::RenamePane => label(&kb.rename_pane),
        CommandPaletteAction::Fullscreen => label(&kb.zoom),
        CommandPaletteAction::EditScrollback => label(&kb.edit_scrollback),
        CommandPaletteAction::ResizeMode => label(&kb.resize_mode),
        CommandPaletteAction::FocusPane(crate::layout::NavDirection::Left) => {
            label(&kb.focus_pane_left).or_else(|| Some("h".into()))
        }
        CommandPaletteAction::FocusPane(crate::layout::NavDirection::Down) => {
            label(&kb.focus_pane_down).or_else(|| Some("j".into()))
        }
        CommandPaletteAction::FocusPane(crate::layout::NavDirection::Up) => {
            label(&kb.focus_pane_up).or_else(|| Some("k".into()))
        }
        CommandPaletteAction::FocusPane(crate::layout::NavDirection::Right) => {
            label(&kb.focus_pane_right).or_else(|| Some("l".into()))
        }
        CommandPaletteAction::CyclePaneNext => label(&kb.cycle_pane_next),
        CommandPaletteAction::CyclePanePrevious => label(&kb.cycle_pane_previous),
        CommandPaletteAction::OpenGroupMenu => label(&kb.open_group_menu),
        CommandPaletteAction::NewGroup => label(&kb.new_group),
        CommandPaletteAction::RenameGroup => label(&kb.rename_group),
        CommandPaletteAction::DeleteGroup => label(&kb.delete_group),
        CommandPaletteAction::ToggleGroupFilter => label(&kb.toggle_group_filter),
        CommandPaletteAction::PreviousGroup => label(&kb.previous_group),
        CommandPaletteAction::NextGroup => label(&kb.next_group),
        CommandPaletteAction::OpenAgentMenu => label(&kb.open_agent_menu),
        CommandPaletteAction::PreviousAgent => label(&kb.previous_agent),
        CommandPaletteAction::NextAgent => label(&kb.next_agent),
        CommandPaletteAction::OpenGitDiff => None,
        CommandPaletteAction::ToggleSidebar => label(&kb.toggle_sidebar),
        CommandPaletteAction::ToggleRightSidebar => label(&kb.toggle_right_sidebar),
        CommandPaletteAction::OpenSettings => label(&kb.settings),
        CommandPaletteAction::OpenKeybinds => label(&kb.help),
        CommandPaletteAction::ReloadConfig => label(&kb.reload_config),
        CommandPaletteAction::OpenNotificationTarget => label(&kb.open_notification_target),
        CommandPaletteAction::DetachOrQuit => label(&kb.detach),
        CommandPaletteAction::CustomCommand(idx) => kb
            .custom_commands
            .get(*idx)
            .map(|binding| binding.label.clone()),
        CommandPaletteAction::SwitchWorkspace(_)
        | CommandPaletteAction::ShowAllGroups
        | CommandPaletteAction::SwitchGroup(_)
        | CommandPaletteAction::NewAgent
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
