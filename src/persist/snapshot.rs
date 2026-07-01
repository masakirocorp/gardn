use std::collections::HashMap;
use std::path::PathBuf;

use ratatui::layout::Direction;
use serde::{Deserialize, Serialize};

use crate::layout::Node;
use crate::terminal::TerminalRuntimeRegistry;
use crate::workspace::Workspace;

/// Current snapshot format version.
pub(super) const SNAPSHOT_VERSION: u32 = 3;

/// Serializable snapshot of the entire hako session.
// Legacy mirror fields stay on the in-memory struct so old snapshots migrate
// through one parser shape; new snapshots serialize `default_view` instead.
#[allow(dead_code)]
#[derive(Serialize, Deserialize)]
pub struct SessionSnapshot {
    /// Format version — used to detect incompatible changes.
    #[serde(default)]
    pub version: u32,
    #[serde(default = "default_groups")]
    pub groups: Vec<GroupSnapshot>,
    #[serde(default)]
    pub active_group: usize,
    #[serde(default = "default_true")]
    pub group_filter_enabled: bool,
    #[serde(default)]
    pub default_view: SessionDefaultViewSnapshot,
    pub workspaces: Vec<WorkspaceSnapshot>,
    #[serde(default, skip_serializing)]
    pub active: Option<usize>,
    #[serde(default, skip_serializing)]
    pub selected: usize,
    #[serde(default, skip_serializing)]
    pub agent_panel_scope: crate::app::state::AgentPanelScope,
    #[serde(default, skip_serializing)]
    pub sidebar_width: Option<u16>,
    #[serde(default, skip_serializing)]
    pub sidebar_collapsed: bool,
    #[serde(default, skip_serializing)]
    pub sidebar_section_split: Option<f32>,
    #[serde(default, skip_serializing)]
    pub right_sidebar_width: Option<u16>,
    #[serde(default, skip_serializing)]
    pub right_sidebar_collapsed: bool,
    #[serde(default, skip_serializing)]
    pub ui: SessionUiSnapshot,
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub pane_id_aliases: std::collections::HashMap<u32, u32>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SessionDefaultViewSnapshot {
    #[serde(default)]
    pub active: Option<usize>,
    #[serde(default)]
    pub selected: usize,
    #[serde(default)]
    pub agent_panel_scope: crate::app::state::AgentPanelScope,
    #[serde(default)]
    pub sidebar_width: Option<u16>,
    #[serde(default)]
    pub sidebar_collapsed: bool,
    #[serde(default)]
    pub sidebar_section_split: Option<f32>,
    #[serde(default)]
    pub right_sidebar_width: Option<u16>,
    #[serde(default)]
    pub right_sidebar_collapsed: bool,
    #[serde(default)]
    pub ui: SessionUiSnapshot,
}

impl Default for SessionDefaultViewSnapshot {
    fn default() -> Self {
        Self {
            active: None,
            selected: 0,
            agent_panel_scope: crate::app::state::AgentPanelScope::default(),
            sidebar_width: None,
            sidebar_collapsed: false,
            sidebar_section_split: None,
            right_sidebar_width: None,
            right_sidebar_collapsed: false,
            ui: SessionUiSnapshot::default(),
        }
    }
}

impl SessionDefaultViewSnapshot {
    fn from_legacy(raw: &RawSessionSnapshot) -> Self {
        Self {
            active: raw.active,
            selected: raw.selected,
            agent_panel_scope: raw.agent_panel_scope,
            sidebar_width: raw.sidebar_width,
            sidebar_collapsed: raw.sidebar_collapsed,
            sidebar_section_split: raw.sidebar_section_split,
            right_sidebar_width: raw.right_sidebar_width,
            right_sidebar_collapsed: raw.right_sidebar_collapsed,
            ui: raw.ui.clone(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SessionUiSnapshot {
    #[serde(default)]
    pub workspace_scroll: usize,
    #[serde(default)]
    pub agent_panel_scroll: usize,
    #[serde(default)]
    pub tab_scroll: usize,
    #[serde(default)]
    pub mobile_switcher_scroll: usize,
    #[serde(default = "default_true")]
    pub activity_agents_expanded: bool,
    #[serde(default)]
    pub activity_commands_expanded: bool,
    #[serde(default)]
    pub activity_ports_expanded: bool,
    #[serde(default)]
    pub collapsed_agent_sections: Vec<String>,
    #[serde(default)]
    pub collapsed_command_groups: Vec<String>,
    #[serde(default)]
    pub collapsed_command_status_groups: Vec<String>,
    #[serde(default)]
    pub collapsed_workspace_groups: Vec<String>,
}

impl Default for SessionUiSnapshot {
    fn default() -> Self {
        Self {
            workspace_scroll: 0,
            agent_panel_scroll: 0,
            tab_scroll: 0,
            mobile_switcher_scroll: 0,
            activity_agents_expanded: true,
            activity_commands_expanded: false,
            activity_ports_expanded: false,
            collapsed_agent_sections: Vec::new(),
            collapsed_command_groups: Vec::new(),
            collapsed_command_status_groups: Vec::new(),
            collapsed_workspace_groups: Vec::new(),
        }
    }
}

impl SessionUiSnapshot {
    pub fn from_app_state(state: &crate::app::state::AppState) -> Self {
        Self {
            workspace_scroll: state.workspace_scroll,
            agent_panel_scroll: state.agent_panel_scroll,
            tab_scroll: state.tab_scroll,
            mobile_switcher_scroll: state.mobile_switcher_scroll,
            activity_agents_expanded: state.activity_agents_expanded,
            activity_commands_expanded: state.activity_commands_expanded,
            activity_ports_expanded: state.activity_ports_expanded,
            collapsed_agent_sections: state.collapsed_agent_sections.clone(),
            collapsed_command_groups: state.collapsed_command_groups.clone(),
            collapsed_command_status_groups: state.collapsed_command_status_groups.clone(),
            collapsed_workspace_groups: state.collapsed_workspace_groups.clone(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct SessionHistorySnapshot {
    /// Format version follows the matching session snapshot version.
    #[serde(default)]
    pub version: u32,
    pub workspaces: Vec<WorkspaceHistorySnapshot>,
}

#[derive(Serialize, Deserialize)]
pub struct WorkspaceHistorySnapshot {
    pub tabs: Vec<TabHistorySnapshot>,
}

#[derive(Serialize, Deserialize)]
pub struct TabHistorySnapshot {
    pub panes: HashMap<u32, PaneHistorySnapshot>,
}

#[derive(Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub custom_name: Option<String>,
    #[serde(default = "default_group_id")]
    pub group_id: String,
    pub identity_cwd: PathBuf,
    pub default_cwd: PathBuf,
    #[serde(default)]
    pub public_pane_numbers: HashMap<u32, usize>,
    #[serde(default)]
    pub next_public_pane_number: usize,
    #[serde(default)]
    pub public_tab_numbers: Vec<usize>,
    #[serde(default)]
    pub next_public_tab_number: usize,
    pub tabs: Vec<TabSnapshot>,
    #[serde(default)]
    pub active_tab: usize,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GroupSnapshot {
    pub id: String,
    pub name: String,
    #[serde(default = "default_group_icon")]
    pub icon: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent: Option<crate::config::TerminalAccent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_directory: Option<std::path::PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub favorite_agent_profile_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_agent_profile_id: Option<String>,
}

fn default_group_id() -> String {
    crate::workspace::DEFAULT_GROUP_ID.to_string()
}

fn default_group_icon() -> String {
    crate::app::state::DEFAULT_GROUP_ICON.to_string()
}

fn default_groups() -> Vec<GroupSnapshot> {
    vec![GroupSnapshot {
        id: crate::workspace::DEFAULT_GROUP_ID.to_string(),
        name: "group 1".to_string(),
        icon: default_group_icon(),
        accent: None,
        default_directory: None,
        favorite_agent_profile_ids: Vec::new(),
        default_agent_profile_id: None,
    }]
}

#[derive(Deserialize)]
struct LegacyWorkspaceSnapshot {
    #[serde(default)]
    custom_name: Option<String>,
    layout: LayoutSnapshot,
    panes: HashMap<u32, PaneSnapshot>,
    zoomed: bool,
    #[serde(default)]
    focused: Option<u32>,
    #[serde(default)]
    root_pane: Option<u32>,
}

#[derive(Serialize, Deserialize)]
pub struct TabSnapshot {
    #[serde(default)]
    pub custom_name: Option<String>,
    pub layout: LayoutSnapshot,
    pub panes: HashMap<u32, PaneSnapshot>,
    pub zoomed: bool,
    #[serde(default)]
    pub focused: Option<u32>,
    #[serde(default)]
    pub root_pane: Option<u32>,
}

#[derive(Serialize, Deserialize)]
pub struct PaneSnapshot {
    pub cwd: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env_pane_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session: Option<PaneAgentSessionSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_argv: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub launch_env: Vec<(String, String)>,
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub seen: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_semantics: Option<crate::terminal::TerminalSemanticSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_diff: Option<NativeDiffPaneSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeDiffPaneSnapshot {
    pub repo_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneAgentSessionSnapshot {
    pub source: String,
    pub agent: String,
    pub kind: crate::agent_resume::AgentSessionRefKind,
    pub value: String,
}

fn default_true() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

#[derive(Serialize, Deserialize)]
pub struct PaneHistorySnapshot {
    pub ansi: String,
    pub lines: usize,
}

/// Serializable BSP tree.
#[derive(Serialize, Deserialize)]
pub enum LayoutSnapshot {
    Pane(u32),
    Split {
        direction: DirectionSnapshot,
        ratio: f32,
        first: Box<LayoutSnapshot>,
        second: Box<LayoutSnapshot>,
    },
}

#[derive(Serialize, Deserialize)]
pub enum DirectionSnapshot {
    Horizontal,
    Vertical,
}

impl From<LegacyWorkspaceSnapshot> for WorkspaceSnapshot {
    fn from(snap: LegacyWorkspaceSnapshot) -> Self {
        let identity_cwd = legacy_identity_cwd(&snap);
        let tab = TabSnapshot {
            custom_name: None,
            layout: snap.layout,
            panes: snap.panes,
            zoomed: snap.zoomed,
            focused: snap.focused,
            root_pane: snap.root_pane,
        };

        Self {
            id: None,
            custom_name: snap.custom_name,
            group_id: default_group_id(),
            identity_cwd: identity_cwd.clone(),
            default_cwd: identity_cwd,
            public_pane_numbers: HashMap::new(),
            next_public_pane_number: 0,
            public_tab_numbers: Vec::new(),
            next_public_tab_number: 0,
            tabs: vec![tab],
            active_tab: 0,
        }
    }
}

#[derive(Deserialize)]
struct RawSessionSnapshot {
    #[serde(default)]
    version: u32,
    #[serde(default = "default_groups")]
    groups: Vec<GroupSnapshot>,
    #[serde(default)]
    active_group: usize,
    #[serde(default = "default_true")]
    group_filter_enabled: bool,
    #[serde(default)]
    default_view: Option<SessionDefaultViewSnapshot>,
    #[serde(default)]
    workspaces: Vec<serde_json::Value>,
    #[serde(default)]
    active: Option<usize>,
    #[serde(default)]
    selected: usize,
    #[serde(default)]
    agent_panel_scope: crate::app::state::AgentPanelScope,
    #[serde(default)]
    sidebar_width: Option<u16>,
    #[serde(default)]
    sidebar_collapsed: bool,
    #[serde(default)]
    sidebar_section_split: Option<f32>,
    #[serde(default)]
    right_sidebar_width: Option<u16>,
    #[serde(default)]
    right_sidebar_collapsed: bool,
    #[serde(default)]
    ui: SessionUiSnapshot,
    #[serde(default)]
    pane_id_aliases: std::collections::HashMap<u32, u32>,
}

fn migrate_snapshot(raw: RawSessionSnapshot) -> Result<SessionSnapshot, String> {
    let default_view = raw
        .default_view
        .clone()
        .unwrap_or_else(|| SessionDefaultViewSnapshot::from_legacy(&raw));
    Ok(SessionSnapshot {
        version: raw.version,
        groups: if raw.groups.is_empty() {
            default_groups()
        } else {
            raw.groups
        },
        active_group: raw.active_group,
        group_filter_enabled: raw.group_filter_enabled,
        workspaces: raw
            .workspaces
            .into_iter()
            .map(migrate_workspace)
            .collect::<Result<Vec<_>, _>>()?,
        active: default_view.active,
        selected: default_view.selected,
        agent_panel_scope: default_view.agent_panel_scope,
        sidebar_width: default_view.sidebar_width,
        sidebar_collapsed: default_view.sidebar_collapsed,
        sidebar_section_split: default_view.sidebar_section_split,
        right_sidebar_width: default_view.right_sidebar_width,
        right_sidebar_collapsed: default_view.right_sidebar_collapsed,
        ui: default_view.ui.clone(),
        default_view,
        pane_id_aliases: raw.pane_id_aliases,
    })
}

fn migrate_workspace(mut raw: serde_json::Value) -> Result<WorkspaceSnapshot, String> {
    if raw.get("identity_cwd").is_some() {
        if raw.get("default_cwd").is_none() {
            if let Some(identity_cwd) = raw.get("identity_cwd").cloned() {
                if let Some(object) = raw.as_object_mut() {
                    object.insert("default_cwd".to_string(), identity_cwd);
                }
            }
        }
        return serde_json::from_value(raw).map_err(|e| e.to_string());
    }

    if raw.get("layout").is_some() {
        let legacy =
            serde_json::from_value::<LegacyWorkspaceSnapshot>(raw).map_err(|e| e.to_string())?;
        return Ok(legacy.into());
    }

    Err("workspace snapshot is neither current nor legacy format".to_string())
}

fn legacy_identity_cwd(snap: &LegacyWorkspaceSnapshot) -> PathBuf {
    let root_pane = snap
        .root_pane
        .or_else(|| first_pane_id_in_layout(&snap.layout));

    root_pane
        .and_then(|pane_id| snap.panes.get(&pane_id))
        .map(|pane| pane.cwd.clone())
        .or_else(|| {
            first_pane_id_in_layout(&snap.layout)
                .and_then(|pane_id| snap.panes.get(&pane_id))
                .map(|pane| pane.cwd.clone())
        })
        .or_else(|| {
            snap.panes
                .keys()
                .min()
                .and_then(|pane_id| snap.panes.get(pane_id))
                .map(|pane| pane.cwd.clone())
        })
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| "/".into()))
}

fn first_pane_id_in_layout(layout: &LayoutSnapshot) -> Option<u32> {
    match layout {
        LayoutSnapshot::Pane(id) => Some(*id),
        LayoutSnapshot::Split { first, second, .. } => {
            first_pane_id_in_layout(first).or_else(|| first_pane_id_in_layout(second))
        }
    }
}

/// Capture the current app state into a serializable snapshot.
#[allow(clippy::too_many_arguments)]
pub fn capture(
    groups: &[crate::app::state::Group],
    active_group: usize,
    group_filter_enabled: bool,
    workspaces: &[Workspace],
    terminals: &std::collections::HashMap<
        crate::terminal::TerminalId,
        crate::terminal::TerminalState,
    >,
    terminal_runtimes: &TerminalRuntimeRegistry,
    active: Option<usize>,
    selected: usize,
    agent_panel_scope: crate::app::state::AgentPanelScope,
    sidebar_width: u16,
    sidebar_collapsed: bool,
    sidebar_section_split: f32,
    right_sidebar_width: u16,
    right_sidebar_collapsed: bool,
) -> SessionSnapshot {
    capture_inner(
        groups,
        active_group,
        group_filter_enabled,
        workspaces,
        terminals,
        terminal_runtimes,
        active,
        selected,
        agent_panel_scope,
        sidebar_width,
        sidebar_collapsed,
        sidebar_section_split,
        right_sidebar_width,
        right_sidebar_collapsed,
        false,
    )
}

/// Capture a handoff snapshot, including live terminal semantics that should
/// survive a server replacement but should not be treated as durable session
/// state after a cold restart.
#[allow(clippy::too_many_arguments)]
pub fn capture_handoff(
    groups: &[crate::app::state::Group],
    active_group: usize,
    group_filter_enabled: bool,
    workspaces: &[Workspace],
    terminals: &std::collections::HashMap<
        crate::terminal::TerminalId,
        crate::terminal::TerminalState,
    >,
    terminal_runtimes: &TerminalRuntimeRegistry,
    active: Option<usize>,
    selected: usize,
    agent_panel_scope: crate::app::state::AgentPanelScope,
    sidebar_width: u16,
    sidebar_collapsed: bool,
    sidebar_section_split: f32,
    right_sidebar_width: u16,
    right_sidebar_collapsed: bool,
) -> SessionSnapshot {
    capture_inner(
        groups,
        active_group,
        group_filter_enabled,
        workspaces,
        terminals,
        terminal_runtimes,
        active,
        selected,
        agent_panel_scope,
        sidebar_width,
        sidebar_collapsed,
        sidebar_section_split,
        right_sidebar_width,
        right_sidebar_collapsed,
        true,
    )
}

#[allow(clippy::too_many_arguments)]
fn capture_inner(
    groups: &[crate::app::state::Group],
    active_group: usize,
    group_filter_enabled: bool,
    workspaces: &[Workspace],
    terminals: &std::collections::HashMap<
        crate::terminal::TerminalId,
        crate::terminal::TerminalState,
    >,
    terminal_runtimes: &TerminalRuntimeRegistry,
    active: Option<usize>,
    selected: usize,
    agent_panel_scope: crate::app::state::AgentPanelScope,
    sidebar_width: u16,
    sidebar_collapsed: bool,
    sidebar_section_split: f32,
    right_sidebar_width: u16,
    right_sidebar_collapsed: bool,
    include_terminal_semantics: bool,
) -> SessionSnapshot {
    let default_view = SessionDefaultViewSnapshot {
        active,
        selected,
        agent_panel_scope,
        sidebar_width: Some(sidebar_width),
        sidebar_collapsed,
        sidebar_section_split: Some(sidebar_section_split),
        right_sidebar_width: Some(right_sidebar_width),
        right_sidebar_collapsed,
        ui: SessionUiSnapshot::default(),
    };

    SessionSnapshot {
        version: SNAPSHOT_VERSION,
        groups: groups.iter().map(capture_group).collect(),
        active_group,
        group_filter_enabled,
        default_view: default_view.clone(),
        workspaces: workspaces
            .iter()
            .map(|workspace| {
                capture_workspace(
                    workspace,
                    terminals,
                    terminal_runtimes,
                    include_terminal_semantics,
                )
            })
            .collect(),
        active: default_view.active,
        selected: default_view.selected,
        ui: default_view.ui.clone(),
        pane_id_aliases: std::collections::HashMap::new(),
        agent_panel_scope: default_view.agent_panel_scope,
        sidebar_width: default_view.sidebar_width,
        sidebar_collapsed: default_view.sidebar_collapsed,
        sidebar_section_split: default_view.sidebar_section_split,
        right_sidebar_width: default_view.right_sidebar_width,
        right_sidebar_collapsed: default_view.right_sidebar_collapsed,
    }
}

fn capture_group(group: &crate::app::state::Group) -> GroupSnapshot {
    GroupSnapshot {
        id: group.id.clone(),
        name: group.name.clone(),
        icon: group.icon.clone(),
        accent: group.accent,
        default_directory: group.default_directory.clone(),
        favorite_agent_profile_ids: group.favorite_agent_profile_ids.clone(),
        default_agent_profile_id: group.default_agent_profile_id.clone(),
    }
}

fn capture_workspace(
    ws: &Workspace,
    terminals: &std::collections::HashMap<
        crate::terminal::TerminalId,
        crate::terminal::TerminalState,
    >,
    terminal_runtimes: &TerminalRuntimeRegistry,
    include_terminal_semantics: bool,
) -> WorkspaceSnapshot {
    WorkspaceSnapshot {
        id: Some(ws.id.clone()),
        custom_name: ws.custom_name.clone(),
        group_id: ws.group_id.clone(),
        identity_cwd: ws.identity_cwd.clone(),
        default_cwd: ws.default_cwd.clone(),
        public_pane_numbers: ws
            .public_pane_numbers
            .iter()
            .map(|(pane_id, number)| (pane_id.raw(), *number))
            .collect(),
        next_public_pane_number: ws.next_public_pane_number,
        public_tab_numbers: ws.tabs.iter().map(|tab| tab.number).collect(),
        next_public_tab_number: ws.next_public_tab_number,
        tabs: ws
            .tabs
            .iter()
            .map(|tab| {
                capture_tab(
                    tab,
                    terminals,
                    terminal_runtimes,
                    include_terminal_semantics,
                )
            })
            .collect(),
        active_tab: ws.active_tab,
    }
}

fn capture_tab(
    tab: &crate::workspace::Tab,
    terminals: &std::collections::HashMap<
        crate::terminal::TerminalId,
        crate::terminal::TerminalState,
    >,
    terminal_runtimes: &TerminalRuntimeRegistry,
    include_terminal_semantics: bool,
) -> TabSnapshot {
    let mut panes = HashMap::new();
    for id in tab.panes.keys() {
        let cwd = tab
            .cwd_for_pane(*id, terminals, terminal_runtimes)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| "/".into()));
        let pane = tab.panes.get(id);
        let terminal = pane.and_then(|pane| terminals.get(&pane.attached_terminal_id));
        let label = terminal.and_then(|terminal| terminal.manual_label.clone());
        let agent_name = terminal.and_then(|terminal| terminal.agent_name.clone());
        let launch_argv = terminal.and_then(|terminal| terminal.launch_argv.clone());
        let launch_env = terminal
            .map(|terminal| terminal.launch_env.clone())
            .unwrap_or_default();
        let agent_session = terminal.and_then(|terminal| {
            if let Some(authority) = terminal.hook_authority.as_ref() {
                if let Some(session_ref) = authority.session_ref.as_ref() {
                    return Some(PaneAgentSessionSnapshot {
                        source: authority.source.clone(),
                        agent: authority.agent_label.clone(),
                        kind: session_ref.kind,
                        value: session_ref.value.clone(),
                    });
                }
            }
            terminal
                .persisted_agent_session
                .as_ref()
                .map(|session| PaneAgentSessionSnapshot {
                    source: session.source.clone(),
                    agent: session.agent.clone(),
                    kind: session.session_ref.kind,
                    value: session.session_ref.value.clone(),
                })
        });
        let seen = pane.is_none_or(|pane| pane.seen);
        let terminal_semantics = include_terminal_semantics
            .then(|| terminal.and_then(|terminal| terminal.capture_semantic_snapshot()))
            .flatten();
        let native_diff = pane.and_then(|pane| {
            pane.native_diff().map(|diff| NativeDiffPaneSnapshot {
                repo_root: diff.session.repo_root.clone(),
            })
        });
        panes.insert(
            id.raw(),
            PaneSnapshot {
                env_pane_id: pane
                    .and_then(|pane| pane.env_pane_id_raw)
                    .filter(|env_pane_id| *env_pane_id != id.raw()),
                cwd,
                label,
                agent_name,
                agent_session,
                launch_argv,
                launch_env,
                seen,
                terminal_semantics,
                native_diff,
            },
        );
    }
    TabSnapshot {
        custom_name: tab.custom_name.clone(),
        layout: capture_node(tab.layout.root()),
        panes,
        zoomed: tab.zoomed,
        focused: Some(tab.layout.focused().raw()),
        root_pane: Some(tab.root_pane.raw()),
    }
}

/// Capture pane screen history separately from the structural session snapshot.
pub fn capture_history(
    workspaces: &[Workspace],
    terminal_runtimes: &TerminalRuntimeRegistry,
) -> SessionHistorySnapshot {
    SessionHistorySnapshot {
        version: SNAPSHOT_VERSION,
        workspaces: workspaces
            .iter()
            .map(|workspace| WorkspaceHistorySnapshot {
                tabs: workspace
                    .tabs
                    .iter()
                    .map(|tab| TabHistorySnapshot {
                        panes: capture_tab_history(tab, terminal_runtimes),
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn capture_tab_history(
    tab: &crate::workspace::Tab,
    terminal_runtimes: &TerminalRuntimeRegistry,
) -> HashMap<u32, PaneHistorySnapshot> {
    let mut panes = HashMap::new();
    for (id, pane) in &tab.panes {
        if let Some(history) = capture_pane_history(Some(pane), terminal_runtimes) {
            panes.insert(id.raw(), history);
        }
    }
    panes
}

fn capture_pane_history(
    pane: Option<&crate::pane::PaneState>,
    terminal_runtimes: &TerminalRuntimeRegistry,
) -> Option<PaneHistorySnapshot> {
    let ansi = terminal_runtimes
        .get(&pane?.attached_terminal_id)?
        .snapshot_history()?;
    let lines = ansi.lines().count();
    Some(PaneHistorySnapshot { ansi, lines })
}

pub(super) fn capture_node(node: &Node) -> LayoutSnapshot {
    match node {
        Node::Pane(id) => LayoutSnapshot::Pane(id.raw()),
        Node::Split {
            direction,
            ratio,
            first,
            second,
        } => LayoutSnapshot::Split {
            direction: match direction {
                Direction::Horizontal => DirectionSnapshot::Horizontal,
                Direction::Vertical => DirectionSnapshot::Vertical,
            },
            ratio: *ratio,
            first: Box::new(capture_node(first)),
            second: Box::new(capture_node(second)),
        },
    }
}

pub(super) fn parse_snapshot(content: &str) -> Result<SessionSnapshot, String> {
    let raw = serde_json::from_str::<RawSessionSnapshot>(content).map_err(|e| e.to_string())?;
    if raw.version > SNAPSHOT_VERSION {
        return Err(format!(
            "snapshot version {} is newer than supported {}",
            raw.version, SNAPSHOT_VERSION
        ));
    }
    migrate_snapshot(raw)
}

pub(super) fn parse_history_snapshot(content: &str) -> Result<SessionHistorySnapshot, String> {
    let snapshot =
        serde_json::from_str::<SessionHistorySnapshot>(content).map_err(|e| e.to_string())?;
    if snapshot.version > SNAPSHOT_VERSION {
        return Err(format!(
            "history snapshot version {} is newer than supported {}",
            snapshot.version, SNAPSHOT_VERSION
        ));
    }
    Ok(snapshot)
}

pub(super) fn snapshot_file_version(content: &str) -> Option<u32> {
    serde_json::from_str::<RawSessionSnapshot>(content)
        .ok()
        .map(|raw| raw.version)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use ratatui::layout::{Direction, Rect};

    use super::*;
    use crate::app::{state::AgentPanelScope, AppState, Mode};
    use crate::layout::NavDirection;
    use crate::workspace::Workspace;

    fn session_fixture(name: &str) -> &'static str {
        match name {
            "current-hako" => {
                include_str!("../../tests/fixtures/session/current-hako-session.json")
            }
            "current-hako-dev" => {
                include_str!("../../tests/fixtures/session/current-hako-dev-session.json")
            }
            "legacy-pre-tabs-v2" => {
                include_str!("../../tests/fixtures/session/legacy-pre-tabs-v2.json")
            }
            other => panic!("unknown session fixture: {other}"),
        }
    }

    fn state_with_workspaces(names: &[&str]) -> AppState {
        let mut state = AppState::test_new();
        state.workspaces = names.iter().map(|name| Workspace::test_new(name)).collect();
        state.ensure_test_terminals();
        if !state.workspaces.is_empty() {
            state.active = Some(0);
            state.selected = 0;
            state.mode = Mode::Terminal;
        }
        state
    }

    fn capture_from_state(state: &AppState) -> SessionSnapshot {
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        capture_from_state_with_runtimes(state, &terminal_runtimes)
    }

    fn capture_from_state_with_runtimes(
        state: &AppState,
        terminal_runtimes: &TerminalRuntimeRegistry,
    ) -> SessionSnapshot {
        capture(
            &state.groups,
            state.active_group,
            state.group_filter_enabled,
            &state.workspaces,
            &state.terminals,
            terminal_runtimes,
            state.active,
            state.selected,
            state.agent_panel_scope,
            state.sidebar_width,
            state.sidebar_collapsed,
            state.sidebar_section_split,
            state.right_sidebar_width,
            state.right_sidebar_collapsed,
        )
    }

    fn capture_history_from_state_with_runtimes(
        state: &AppState,
        terminal_runtimes: &TerminalRuntimeRegistry,
    ) -> SessionHistorySnapshot {
        capture_history(&state.workspaces, terminal_runtimes)
    }

    #[test]
    fn capture_persists_native_diff_repo_root() {
        let mut state = state_with_workspaces(&["space"]);
        let repo_root = PathBuf::from("/hako-test/repo");
        state.workspaces[0]
            .create_native_diff_tab(crate::native_diff::NativeDiffSession {
                repo_root: repo_root.clone(),
                files: Vec::new(),
            })
            .expect("create native diff tab");

        let snap = capture_from_state(&state);
        let native_tab = &snap.workspaces[0].tabs[1];
        let pane = native_tab
            .panes
            .values()
            .find(|pane| pane.native_diff.is_some())
            .expect("native diff pane");

        assert_eq!(
            pane.native_diff
                .as_ref()
                .map(|diff| diff.repo_root.as_path()),
            Some(repo_root.as_path())
        );
    }

    #[test]
    fn capture_keeps_space_identity_separate_from_runtime_cwd() {
        let mut state = state_with_workspaces(&["space"]);
        state.workspaces[0].custom_name = None;
        state.workspaces[0].identity_cwd = PathBuf::from("/hako-test/space");
        state.workspaces[0].default_cwd = PathBuf::from("/hako-test/default");
        let root_pane = state.workspaces[0].tabs[0].root_pane;
        let terminal_id = state.workspaces[0].terminal_id(root_pane).unwrap().clone();
        state.terminals.get_mut(&terminal_id).unwrap().cwd = PathBuf::from("/hako-test/runtime");
        state.workspaces[0].tabs[0]
            .panes
            .get_mut(&root_pane)
            .unwrap()
            .env_pane_id_raw = Some(6);

        let snap = capture_from_state(&state);

        assert_eq!(
            snap.workspaces[0].identity_cwd,
            PathBuf::from("/hako-test/space")
        );
        assert_eq!(
            snap.workspaces[0].default_cwd,
            PathBuf::from("/hako-test/default")
        );
        assert_eq!(
            snap.workspaces[0].tabs[0].panes[&root_pane.raw()].cwd,
            PathBuf::from("/hako-test/runtime")
        );
        assert_eq!(
            snap.workspaces[0].tabs[0].panes[&root_pane.raw()].env_pane_id,
            Some(6)
        );
    }

    #[test]
    fn capture_handoff_keeps_terminal_semantics_out_of_durable_snapshot() {
        let mut state = state_with_workspaces(&["space"]);
        let root_pane = state.workspaces[0].tabs[0].root_pane;
        let terminal_id = state.workspaces[0].terminal_id(root_pane).unwrap().clone();
        let terminal = state.terminals.get_mut(&terminal_id).unwrap();
        let _ = terminal.set_hook_authority_with_session_ref(
            "hako:omp".to_string(),
            "omp".to_string(),
            crate::detect::AgentState::Working,
            Some("processing".to_string()),
            Some("reading".to_string()),
            Some(crate::agent_resume::AgentSessionRef {
                kind: crate::agent_resume::AgentSessionRefKind::Id,
                value: "session-1".to_string(),
            }),
            Some(7),
        );
        let _ = terminal.set_agent_metadata(crate::terminal::AgentMetadataReport {
            source: "hako:omp:metadata".to_string(),
            agent_label: Some("omp".to_string()),
            applies_to_source: Some("hako:omp".to_string()),
            title: Some("Oracle".to_string()),
            display_agent: Some("OMP".to_string()),
            custom_status: Some("thinking".to_string()),
            state_labels: HashMap::from([("working".to_string(), "busy".to_string())]),
            clear_title: false,
            clear_display_agent: false,
            clear_custom_status: false,
            clear_state_labels: false,
            ttl: None,
            seq: Some(9),
        });
        state.workspaces[0].tabs[0]
            .panes
            .get_mut(&root_pane)
            .unwrap()
            .seen = false;

        let durable = capture_from_state(&state);
        let terminal_runtimes = TerminalRuntimeRegistry::new();
        let handoff = capture_handoff(
            &state.groups,
            state.active_group,
            state.group_filter_enabled,
            &state.workspaces,
            &state.terminals,
            &terminal_runtimes,
            state.active,
            state.selected,
            state.agent_panel_scope,
            state.sidebar_width,
            state.sidebar_collapsed,
            state.sidebar_section_split,
            state.right_sidebar_width,
            state.right_sidebar_collapsed,
        );
        let durable_pane = &durable.workspaces[0].tabs[0].panes[&root_pane.raw()];
        let handoff_pane = &handoff.workspaces[0].tabs[0].panes[&root_pane.raw()];

        assert!(!durable_pane.seen);
        assert!(durable_pane.terminal_semantics.is_none());
        let semantics = handoff_pane
            .terminal_semantics
            .as_ref()
            .expect("handoff should include live terminal semantics");
        assert_eq!(
            semantics
                .hook_authority
                .as_ref()
                .map(|authority| authority.agent_label.as_str()),
            Some("omp")
        );
        assert_eq!(semantics.state, crate::detect::AgentState::Working);
        assert_eq!(semantics.agent_metadata.len(), 1);
        assert_eq!(semantics.hook_report_sequences["hako:omp"], 7);
        assert_eq!(semantics.metadata_report_sequences["hako:omp:metadata"], 9);
    }

    #[test]
    fn capture_tracks_public_identity_counters() {
        let mut state = state_with_workspaces(&["one"]);
        let second = state.workspaces[0].test_split(Direction::Horizontal);
        let third = state.workspaces[0].test_split(Direction::Vertical);
        let second_tab = state.workspaces[0].test_add_tab(None);

        state.workspaces[0].close_pane(second);

        let snapshot = capture_from_state(&state);
        let workspace = &snapshot.workspaces[0];
        assert_eq!(
            workspace.public_pane_numbers,
            HashMap::from([
                (state.workspaces[0].tabs[0].root_pane.raw(), 1),
                (third.raw(), 3),
                (state.workspaces[0].tabs[second_tab].root_pane.raw(), 4),
            ])
        );
        assert_eq!(workspace.next_public_pane_number, 5);
        assert_eq!(workspace.public_tab_numbers, vec![1, 2]);
        assert_eq!(workspace.next_public_tab_number, 3);
    }

    fn root_split_ratio(tab: &TabSnapshot) -> Option<f32> {
        match &tab.layout {
            LayoutSnapshot::Split { ratio, .. } => Some(*ratio),
            LayoutSnapshot::Pane(_) => None,
        }
    }

    #[test]
    fn round_trip_empty_session() {
        let snap = SessionSnapshot {
            version: SNAPSHOT_VERSION,
            groups: default_groups(),
            active_group: 0,
            group_filter_enabled: true,
            default_view: SessionDefaultViewSnapshot {
                active: None,
                selected: 0,
                agent_panel_scope: AgentPanelScope::CurrentWorkspace,
                sidebar_width: Some(26),
                sidebar_collapsed: false,
                sidebar_section_split: Some(0.5),
                right_sidebar_width: Some(28),
                right_sidebar_collapsed: false,
                ui: SessionUiSnapshot::default(),
            },
            workspaces: vec![],
            active: None,
            selected: 0,
            agent_panel_scope: AgentPanelScope::CurrentWorkspace,
            sidebar_width: Some(26),
            sidebar_collapsed: false,
            sidebar_section_split: Some(0.5),
            right_sidebar_width: Some(28),
            right_sidebar_collapsed: false,
            ui: SessionUiSnapshot::default(),
            pane_id_aliases: HashMap::new(),
        };
        let json = serde_json::to_string(&snap).unwrap();
        let restored = parse_snapshot(&json).unwrap();
        assert!(restored.workspaces.is_empty());
        assert_eq!(restored.active, None);
        assert_eq!(restored.sidebar_width, Some(26));
        assert!(!restored.sidebar_collapsed);
        assert_eq!(restored.sidebar_section_split, Some(0.5));
        assert_eq!(restored.right_sidebar_width, Some(28));
        assert!(!restored.right_sidebar_collapsed);
    }

    #[test]
    fn round_trip_groups_and_workspace_membership() {
        let mut state = state_with_workspaces(&["one", "two"]);
        let group_id = crate::app::state::generate_group_id();
        state.groups.push(crate::app::state::Group {
            id: group_id.clone(),
            name: "Side".to_string(),
            icon: "⚓".to_string(),
            accent: Some(crate::config::TerminalAccent::Cyan),
            default_directory: None,
            favorite_agent_profile_ids: Vec::new(),
            default_agent_profile_id: None,
        });
        state.active_group = 1;
        state.group_filter_enabled = false;
        state.workspaces[1].group_id = group_id.clone();

        let json = serde_json::to_string(&capture_from_state(&state)).unwrap();
        let restored = parse_snapshot(&json).unwrap();

        assert_eq!(restored.groups.len(), 2);
        assert_eq!(restored.groups[1].name, "Side");
        assert_eq!(restored.groups[1].icon, "⚓");
        assert_eq!(
            restored.groups[1].accent,
            Some(crate::config::TerminalAccent::Cyan)
        );
        assert_eq!(restored.active_group, 1);
        assert!(!restored.group_filter_enabled);
        assert_eq!(restored.workspaces[1].group_id, group_id);
    }

    #[test]
    fn round_trip_layout_snapshot() {
        let layout = LayoutSnapshot::Split {
            direction: DirectionSnapshot::Horizontal,
            ratio: 0.6,
            first: Box::new(LayoutSnapshot::Pane(0)),
            second: Box::new(LayoutSnapshot::Split {
                direction: DirectionSnapshot::Vertical,
                ratio: 0.5,
                first: Box::new(LayoutSnapshot::Pane(1)),
                second: Box::new(LayoutSnapshot::Pane(2)),
            }),
        };
        let json = serde_json::to_string(&layout).unwrap();
        let restored: LayoutSnapshot = serde_json::from_str(&json).unwrap();

        match restored {
            LayoutSnapshot::Split { ratio, .. } => assert!((ratio - 0.6).abs() < 0.01),
            _ => panic!("expected split"),
        }
    }

    #[test]
    fn round_trip_full_workspace_snapshot() {
        let mut panes = HashMap::new();
        panes.insert(
            0,
            PaneSnapshot {
                env_pane_id: None,
                cwd: PathBuf::from("/home/can/Projects/hako"),
                label: None,
                agent_name: None,
                agent_session: None,
                launch_argv: None,
                launch_env: Vec::new(),
                seen: true,
                terminal_semantics: None,
                native_diff: None,
            },
        );
        panes.insert(
            1,
            PaneSnapshot {
                env_pane_id: None,
                cwd: PathBuf::from("/home/can/Projects/website"),
                label: Some("website".into()),
                agent_name: None,
                agent_session: None,
                launch_argv: None,
                launch_env: Vec::new(),
                seen: true,
                terminal_semantics: None,
                native_diff: None,
            },
        );

        let snap = SessionSnapshot {
            groups: default_groups(),
            active_group: 0,
            group_filter_enabled: true,
            default_view: SessionDefaultViewSnapshot {
                active: Some(0),
                selected: 0,
                agent_panel_scope: AgentPanelScope::CurrentWorkspace,
                sidebar_width: Some(26),
                sidebar_collapsed: false,
                sidebar_section_split: Some(0.5),
                right_sidebar_width: Some(28),
                right_sidebar_collapsed: false,
                ui: SessionUiSnapshot::default(),
            },
            workspaces: vec![WorkspaceSnapshot {
                id: Some("wproj".to_string()),
                custom_name: Some("pi-mono".to_string()),
                group_id: default_group_id(),
                identity_cwd: PathBuf::from("/home/can/Projects/hako"),
                default_cwd: PathBuf::from("/home/can/Projects/hako"),
                public_pane_numbers: HashMap::from([(0, 1), (1, 2)]),
                next_public_pane_number: 3,
                public_tab_numbers: vec![1],
                next_public_tab_number: 2,
                tabs: vec![TabSnapshot {
                    custom_name: Some("api".to_string()),
                    layout: LayoutSnapshot::Split {
                        direction: DirectionSnapshot::Horizontal,
                        ratio: 0.5,
                        first: Box::new(LayoutSnapshot::Pane(0)),
                        second: Box::new(LayoutSnapshot::Pane(1)),
                    },
                    panes,
                    zoomed: false,
                    focused: Some(0),
                    root_pane: Some(0),
                }],
                active_tab: 0,
            }],
            active: Some(0),
            selected: 0,
            agent_panel_scope: AgentPanelScope::CurrentWorkspace,
            sidebar_width: Some(26),
            sidebar_collapsed: false,
            sidebar_section_split: Some(0.5),
            right_sidebar_width: Some(28),
            right_sidebar_collapsed: false,
            ui: SessionUiSnapshot::default(),
            pane_id_aliases: HashMap::new(),
            version: SNAPSHOT_VERSION,
        };

        let json = serde_json::to_string_pretty(&snap).unwrap();
        let restored = parse_snapshot(&json).unwrap();

        assert_eq!(restored.workspaces.len(), 1);
        assert_eq!(restored.workspaces[0].id.as_deref(), Some("wproj"));
        assert_eq!(
            restored.workspaces[0].custom_name.as_deref(),
            Some("pi-mono")
        );
        assert_eq!(restored.workspaces[0].tabs.len(), 1);
        assert_eq!(restored.workspaces[0].tabs[0].panes.len(), 2);
        assert_eq!(
            restored.workspaces[0].tabs[0].panes[&0].cwd,
            PathBuf::from("/home/can/Projects/hako")
        );
        assert_eq!(
            restored.workspaces[0].tabs[0].panes[&1].label.as_deref(),
            Some("website")
        );
        assert_eq!(
            restored.agent_panel_scope,
            AgentPanelScope::CurrentWorkspace
        );
        assert_eq!(restored.sidebar_width, Some(26));
        assert_eq!(restored.sidebar_section_split, Some(0.5));
        assert_eq!(restored.right_sidebar_width, Some(28));
    }

    #[test]
    fn current_session_fixture_parses() {
        let snap = parse_snapshot(session_fixture("current-hako")).unwrap();

        assert_eq!(snap.version, 3);
        assert_eq!(snap.workspaces.len(), 2);
        assert_eq!(snap.active, Some(0));
        assert_eq!(snap.selected, 0);
        assert_eq!(snap.agent_panel_scope, AgentPanelScope::CurrentWorkspace);
        assert_eq!(snap.sidebar_width, None);
        assert!(!snap.sidebar_collapsed);
        assert_eq!(snap.sidebar_section_split, None);
        assert_eq!(snap.right_sidebar_width, None);
        assert!(!snap.right_sidebar_collapsed);
        assert_eq!(snap.workspaces[0].tabs.len(), 2);
        assert_eq!(
            snap.workspaces[1].identity_cwd,
            PathBuf::from("/home/test/projects/project-b")
        );
    }

    #[test]
    fn current_dev_session_fixture_parses_additive_fields() {
        let snap = parse_snapshot(session_fixture("current-hako-dev")).unwrap();

        assert_eq!(snap.version, 3);
        assert_eq!(snap.workspaces.len(), 2);
        assert_eq!(snap.agent_panel_scope, AgentPanelScope::CurrentWorkspace);
        assert_eq!(snap.sidebar_section_split, Some(0.4));
        assert_eq!(snap.workspaces[0].active_tab, 1);
        assert_eq!(snap.workspaces[1].tabs[0].panes.len(), 2);
    }

    #[test]
    fn old_snapshot_defaults_agent_panel_scope() {
        let json = serde_json::json!({
            "version": SNAPSHOT_VERSION,
            "workspaces": [],
            "active": null,
            "selected": 0
        })
        .to_string();

        let restored = parse_snapshot(&json).unwrap();

        assert_eq!(
            restored.agent_panel_scope,
            AgentPanelScope::CurrentWorkspace
        );
        assert_eq!(restored.sidebar_width, None);
        assert!(!restored.sidebar_collapsed);
        assert_eq!(restored.sidebar_section_split, None);
        assert_eq!(restored.right_sidebar_width, None);
        assert!(!restored.right_sidebar_collapsed);
    }

    #[test]
    fn old_pane_snapshot_with_embedded_history_is_ignored() {
        let json = serde_json::json!({
            "version": SNAPSHOT_VERSION,
            "workspaces": [{
                "id": "wtest",
                "identity_cwd": "/tmp",
                "tabs": [{
                    "layout": { "Pane": 0 },
                    "panes": {
                        "0": {
                            "cwd": "/tmp",
                            "history": {
                                "ansi": "legacy-secret",
                                "lines": 1
                            }
                        }
                    },
                    "zoomed": false,
                    "focused": 0,
                    "root_pane": 0
                }],
                "active_tab": 0
            }],
            "active": 0,
            "selected": 0
        })
        .to_string();

        let restored = parse_snapshot(&json).unwrap();

        let encoded = serde_json::to_string(&restored).unwrap();
        assert!(!encoded.contains("legacy-secret"));
        assert!(!encoded.contains("\"history\""));
    }

    #[test]
    fn legacy_workspace_snapshot_migrates_to_single_tab() {
        let snap = parse_snapshot(session_fixture("legacy-pre-tabs-v2")).unwrap();
        let ws = &snap.workspaces[0];

        assert_eq!(snap.version, 2);
        assert_eq!(snap.workspaces.len(), 1);
        assert_eq!(ws.custom_name.as_deref(), Some("legacy"));
        assert_eq!(ws.identity_cwd, PathBuf::from("/tmp/pion"));
        assert_eq!(ws.active_tab, 0);
        assert_eq!(ws.tabs.len(), 1);
        assert_eq!(ws.tabs[0].focused, Some(1));
        assert_eq!(ws.tabs[0].root_pane, Some(0));
        assert_eq!(ws.tabs[0].panes[&0].cwd, PathBuf::from("/tmp/pion"));
        assert_eq!(ws.tabs[0].panes[&1].cwd, PathBuf::from("/tmp/hako"));
    }

    #[test]
    fn capture_contract_tracks_workspace_order_active_and_selected() {
        let mut state = state_with_workspaces(&["a", "b", "c"]);
        state.active = Some(1);
        state.selected = 2;

        state.move_workspace(1, 0);

        let snapshot = capture_from_state(&state);
        let ids: Vec<_> = state.workspaces.iter().map(|ws| ws.id.clone()).collect();
        let captured_ids: Vec<_> = snapshot
            .workspaces
            .iter()
            .map(|ws| ws.id.clone().unwrap())
            .collect();
        assert_eq!(captured_ids, ids);
        assert_eq!(snapshot.active, state.active);
        assert_eq!(snapshot.selected, state.selected);
    }

    #[test]
    fn capture_contract_tracks_workspace_and_tab_names_and_active_tab() {
        let mut state = state_with_workspaces(&["one"]);
        state.workspaces[0].set_custom_name("renamed-workspace".into());
        let second_tab = state.workspaces[0].test_add_tab(Some("logs"));
        state.workspaces[0].switch_tab(second_tab);
        state.workspaces[0].tabs[0].set_custom_name("main".into());

        let snapshot = capture_from_state(&state);
        let workspace = &snapshot.workspaces[0];
        assert_eq!(workspace.custom_name.as_deref(), Some("renamed-workspace"));
        assert_eq!(workspace.active_tab, second_tab);
        assert_eq!(workspace.tabs[0].custom_name.as_deref(), Some("main"));
        assert_eq!(workspace.tabs[1].custom_name.as_deref(), Some("logs"));
    }

    #[test]
    fn capture_contract_tracks_workspace_closure() {
        let mut state = state_with_workspaces(&["one", "two"]);
        state.selected = 1;
        state.active = Some(1);

        state.close_selected_workspace();

        let snapshot = capture_from_state(&state);
        assert_eq!(snapshot.workspaces.len(), 1);
        assert_eq!(snapshot.workspaces[0].custom_name.as_deref(), Some("one"));
        assert_eq!(snapshot.active, Some(0));
        assert_eq!(snapshot.selected, 0);
    }

    #[test]
    fn capture_contract_tracks_sidebar_state() {
        let mut state = state_with_workspaces(&["one"]);
        state.sidebar_width = 31;
        state.sidebar_collapsed = true;
        state.sidebar_section_split = 0.4;
        state.right_sidebar_width = 34;
        state.right_sidebar_collapsed = true;
        state.agent_panel_scope = AgentPanelScope::AllWorkspaces;

        let snapshot = capture_from_state(&state);
        assert_eq!(snapshot.sidebar_width, Some(31));
        assert!(snapshot.sidebar_collapsed);
        assert_eq!(snapshot.sidebar_section_split, Some(0.4));
        assert_eq!(snapshot.right_sidebar_width, Some(34));
        assert!(snapshot.right_sidebar_collapsed);
        assert_eq!(snapshot.agent_panel_scope, AgentPanelScope::AllWorkspaces);
    }

    #[test]
    fn capture_contract_tracks_layout_focus_zoom_and_root_pane() {
        let mut state = state_with_workspaces(&["one"]);
        let root = state.workspaces[0].tabs[0].root_pane;
        let second = state.workspaces[0].test_split(Direction::Horizontal);
        state.workspaces[0].tabs[0].layout.focus_pane(second);
        state.toggle_zoom();

        let snapshot = capture_from_state(&state);
        let tab = &snapshot.workspaces[0].tabs[0];
        assert!(matches!(tab.layout, LayoutSnapshot::Split { .. }));
        assert_eq!(tab.focused, Some(second.raw()));
        assert_eq!(tab.root_pane, Some(root.raw()));
        assert!(tab.zoomed);
        assert_eq!(tab.panes.len(), 2);
    }

    #[test]
    fn capture_contract_tracks_focus_navigation() {
        let mut state = state_with_workspaces(&["one"]);
        let root = state.workspaces[0].tabs[0].root_pane;
        let second = state.workspaces[0].test_split(Direction::Horizontal);
        crate::ui::compute_view(&mut state, Rect::new(0, 0, 106, 20));

        state.navigate_pane(NavDirection::Right);

        let snapshot = capture_from_state(&state);
        assert_eq!(snapshot.workspaces[0].tabs[0].focused, Some(second.raw()));
        assert_ne!(snapshot.workspaces[0].tabs[0].focused, Some(root.raw()));
    }

    #[test]
    fn capture_contract_tracks_resize_ratio_changes() {
        let mut state = state_with_workspaces(&["one"]);
        state.workspaces[0].test_split(Direction::Horizontal);
        crate::ui::compute_view(&mut state, Rect::new(0, 0, 106, 20));
        let before = capture_from_state(&state);

        state.resize_pane(NavDirection::Right);

        let after = capture_from_state(&state);
        let before_ratio = root_split_ratio(&before.workspaces[0].tabs[0]).unwrap();
        let after_ratio = root_split_ratio(&after.workspaces[0].tabs[0]).unwrap();
        assert_ne!(before_ratio, after_ratio);
    }

    #[test]
    fn capture_contract_tracks_last_tab_closure_as_empty_workspace() {
        let mut state = state_with_workspaces(&["one"]);

        state.close_tab();

        let snapshot = capture_from_state(&state);
        let workspace = &snapshot.workspaces[0];
        assert!(workspace.tabs.is_empty());
        assert_eq!(workspace.active_tab, 0);
        assert_eq!(snapshot.active, Some(0));
    }

    #[test]
    fn capture_contract_tracks_non_last_tab_closure() {
        let mut state = state_with_workspaces(&["one"]);
        let second_tab = state.workspaces[0].test_add_tab(Some("logs"));
        state.switch_tab(second_tab);

        state.close_tab();

        let snapshot = capture_from_state(&state);
        let workspace = &snapshot.workspaces[0];
        assert_eq!(workspace.tabs.len(), 1);
        assert_eq!(workspace.active_tab, 0);
        assert!(workspace.tabs[0].custom_name.is_none());
    }

    #[test]
    fn capture_contract_tracks_pane_closure() {
        let mut state = state_with_workspaces(&["one"]);
        state.workspaces[0].test_split(Direction::Horizontal);

        state.close_pane();

        let snapshot = capture_from_state(&state);
        let tab = &snapshot.workspaces[0].tabs[0];
        assert_eq!(tab.panes.len(), 1);
        assert!(matches!(tab.layout, LayoutSnapshot::Pane(_)));
        assert!(!tab.zoomed);
    }

    #[test]
    fn capture_contract_tracks_workspace_identity_and_pane_cwds() {
        let mut state = state_with_workspaces(&["one"]);
        let root = state.workspaces[0].tabs[0].root_pane;
        state.workspaces[0].identity_cwd = PathBuf::from("/tmp/pion");
        let second = state.workspaces[0].test_split(Direction::Horizontal);
        state.ensure_test_terminals();
        let root_terminal_id = state.workspaces[0].tabs[0].panes[&root]
            .attached_terminal_id
            .clone();
        state.terminals.get_mut(&root_terminal_id).unwrap().cwd = PathBuf::from("/tmp/pion");
        let second_terminal_id = state.workspaces[0].tabs[0].panes[&second]
            .attached_terminal_id
            .clone();
        state.terminals.get_mut(&second_terminal_id).unwrap().cwd = PathBuf::from("/tmp/hako");

        let snapshot = capture_from_state(&state);
        let workspace = &snapshot.workspaces[0];
        let tab = &workspace.tabs[0];
        assert_eq!(workspace.identity_cwd, PathBuf::from("/tmp/pion"));
        assert_eq!(tab.panes[&root.raw()].cwd, PathBuf::from("/tmp/pion"));
        assert_eq!(tab.panes[&second.raw()].cwd, PathBuf::from("/tmp/hako"));
    }

    #[tokio::test]
    async fn capture_contract_tracks_pane_history_from_runtime() {
        let state = state_with_workspaces(&["one"]);
        let root = state.workspaces[0].tabs[0].root_pane;
        let terminal_id = state.workspaces[0].tabs[0].panes[&root]
            .attached_terminal_id
            .clone();
        let mut terminal_runtimes = TerminalRuntimeRegistry::new();
        terminal_runtimes.insert(
            terminal_id,
            crate::terminal::TerminalRuntime::test_with_scrollback_bytes(
                20,
                3,
                4096,
                b"alpha\r\nbeta\r\ngamma\r\n",
            ),
        );

        let snapshot = capture_from_state_with_runtimes(&state, &terminal_runtimes);
        let encoded = serde_json::to_string(&snapshot).unwrap();
        assert!(!encoded.contains("alpha"));
        assert!(!encoded.contains("\"history\""));

        let history_snapshot = capture_history_from_state_with_runtimes(&state, &terminal_runtimes);
        let history = &history_snapshot.workspaces[0].tabs[0].panes[&root.raw()];

        assert!(history.ansi.contains("alpha"));
        assert!(history.ansi.contains("gamma"));
        assert!(history.lines >= 3);
    }

    #[tokio::test]
    async fn capture_contract_tracks_history_for_each_pane() {
        let mut state = state_with_workspaces(&["one"]);
        let first = state.workspaces[0].tabs[0].root_pane;
        let second = state.workspaces[0].test_split(Direction::Horizontal);
        let first_terminal_id = state.workspaces[0].tabs[0].panes[&first]
            .attached_terminal_id
            .clone();
        let second_terminal_id = state.workspaces[0].tabs[0].panes[&second]
            .attached_terminal_id
            .clone();
        let mut terminal_runtimes = TerminalRuntimeRegistry::new();
        terminal_runtimes.insert(
            first_terminal_id,
            crate::terminal::TerminalRuntime::test_with_scrollback_bytes(
                20,
                3,
                4096,
                b"first-pane-history\r\n",
            ),
        );
        terminal_runtimes.insert(
            second_terminal_id,
            crate::terminal::TerminalRuntime::test_with_scrollback_bytes(
                20,
                3,
                4096,
                b"second-pane-history\r\n",
            ),
        );

        let snapshot = capture_from_state_with_runtimes(&state, &terminal_runtimes);
        let encoded = serde_json::to_string(&snapshot).unwrap();
        assert!(!encoded.contains("first-pane-history"));
        assert!(!encoded.contains("second-pane-history"));

        let history_snapshot = capture_history_from_state_with_runtimes(&state, &terminal_runtimes);
        let tab = &history_snapshot.workspaces[0].tabs[0];
        let first_history = &tab.panes[&first.raw()];
        let second_history = &tab.panes[&second.raw()];

        assert!(first_history.ansi.contains("first-pane-history"));
        assert!(second_history.ansi.contains("second-pane-history"));
    }

    #[test]
    fn capture_contract_tracks_hook_authority_agent_session() {
        let mut state = state_with_workspaces(&["one"]);
        let root = state.workspaces[0].tabs[0].root_pane;
        state.ensure_test_terminals();
        let terminal_id = state.workspaces[0].tabs[0].panes[&root]
            .attached_terminal_id
            .clone();
        state
            .terminals
            .get_mut(&terminal_id)
            .unwrap()
            .set_hook_authority_with_session_ref(
                "hako:pi".into(),
                "pi".into(),
                crate::detect::AgentState::Working,
                None,
                None,
                crate::agent_resume::AgentSessionRef::path("/tmp/pi-session.jsonl"),
                Some(20),
            );

        let snapshot = capture_from_state(&state);
        let agent_session = snapshot.workspaces[0].tabs[0].panes[&root.raw()]
            .agent_session
            .as_ref()
            .expect("agent session should be captured");

        assert_eq!(agent_session.source, "hako:pi");
        assert_eq!(agent_session.agent, "pi");
        assert_eq!(
            agent_session.kind,
            crate::agent_resume::AgentSessionRefKind::Path
        );
        assert_eq!(agent_session.value, "/tmp/pi-session.jsonl");
    }

    #[test]
    fn capture_contract_preserves_restored_agent_session() {
        let mut state = state_with_workspaces(&["one"]);
        let root = state.workspaces[0].tabs[0].root_pane;
        state.ensure_test_terminals();
        let terminal_id = state.workspaces[0].tabs[0].panes[&root]
            .attached_terminal_id
            .clone();
        state
            .terminals
            .get_mut(&terminal_id)
            .unwrap()
            .set_persisted_agent_session(crate::agent_resume::PersistedAgentSession {
                source: "hako:opencode".into(),
                agent: "opencode".into(),
                session_ref: crate::agent_resume::AgentSessionRef::id("opencode-session").unwrap(),
            });

        let snapshot = capture_from_state(&state);
        let agent_session = snapshot.workspaces[0].tabs[0].panes[&root.raw()]
            .agent_session
            .as_ref()
            .expect("persisted agent session should be captured");

        assert_eq!(agent_session.source, "hako:opencode");
        assert_eq!(agent_session.agent, "opencode");
        assert_eq!(
            agent_session.kind,
            crate::agent_resume::AgentSessionRefKind::Id
        );
        assert_eq!(agent_session.value, "opencode-session");
    }

    #[test]
    fn old_unversioned_snapshot_loads_as_version_0() {
        let json = r#"{"workspaces":[],"active":null,"selected":0}"#;
        let snap = parse_snapshot(json).unwrap();
        assert_eq!(snap.version, 0);
    }

    #[test]
    fn future_version_is_rejected() {
        let json = r#"{"version":999,"workspaces":[],"active":null,"selected":0}"#;
        let err = match parse_snapshot(json) {
            Ok(_) => panic!("future snapshot version should be rejected"),
            Err(err) => err,
        };
        assert!(
            err.contains("snapshot version 999 is newer than supported"),
            "error should identify unsupported future version: {err}"
        );
    }

    #[test]
    fn active_tab_default_is_zero() {
        let json = r#"{"custom_name":"test","identity_cwd":"/tmp","default_cwd":"/tmp","tabs":[]}"#;
        let ws: WorkspaceSnapshot = serde_json::from_str(json).unwrap();
        assert_eq!(ws.active_tab, 0);
    }
}
