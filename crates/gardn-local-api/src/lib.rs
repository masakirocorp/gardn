//! Local API compatibility contract: request/response/event DTOs and JSON Schema.
//!
//! This crate is intentionally free of runtime, transport, filesystem, and UI
//! dependencies. Host crates inject product/protocol versions and provide
//! adapters for domain types (sound, resource locations, ratatui geometry).

use std::borrow::Cow;
use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

fn is_false(value: &bool) -> bool {
    !*value
}

/// Toast corner position for `notification.show` (wire: kebab-case).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum ToastPosition {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
}

/// Result status for `server.reload_config` (wire: snake_case).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConfigReloadStatus {
    Applied,
    Partial,
    Failed,
}

/// How an agent session is referenced on the wire (wire: snake_case).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentSessionRefKind {
    Id,
    Path,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Request {
    pub id: String,
    #[serde(flatten)]
    pub method: Method,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "method", content = "params")]
// Request enums are short-lived wire values; keeping variants direct preserves
// the simple serde shape and avoids boxing churn across every caller.
#[allow(clippy::large_enum_variant)]
pub enum Method {
    #[serde(rename = "ping")]
    Ping(PingParams),
    #[serde(rename = "server.stop")]
    ServerStop(EmptyParams),
    #[serde(rename = "server.live_handoff")]
    ServerLiveHandoff(ServerLiveHandoffParams),
    #[serde(rename = "server.reload_config")]
    ServerReloadConfig(EmptyParams),
    #[serde(rename = "server.agent_manifests")]
    ServerAgentManifests(EmptyParams),
    #[serde(rename = "server.reload_agent_manifests")]
    ServerReloadAgentManifests(EmptyParams),
    #[serde(rename = "connection.list")]
    ConnectionList(EmptyParams),
    #[serde(rename = "connection.save")]
    ConnectionSave(ConnectionSaveParams),
    #[serde(rename = "connection.delete")]
    ConnectionDelete(ConnectionTarget),
    #[serde(rename = "connection.test")]
    ConnectionTest(ConnectionTarget),
    #[serde(rename = "connection.connect")]
    ConnectionConnect(ConnectionTarget),
    #[serde(rename = "connection.disconnect")]
    ConnectionDisconnect(ConnectionTarget),
    #[serde(rename = "connection.retire.start")]
    ConnectionRetireStart(ConnectionRetireParams),
    #[serde(rename = "connection.retire.status")]
    ConnectionRetireStatus(ConnectionRetireParams),
    #[serde(rename = "notification.show")]
    NotificationShow(NotificationShowParams),
    #[serde(rename = "group.create")]
    GroupCreate(GroupCreateParams),
    #[serde(rename = "group.list")]
    GroupList(EmptyParams),
    #[serde(rename = "group.focus")]
    GroupFocus(GroupTarget),
    #[serde(rename = "group.rename")]
    GroupRename(GroupRenameParams),
    #[serde(rename = "group.delete")]
    GroupDelete(GroupTarget),
    #[serde(rename = "client.window_title.set")]
    ClientWindowTitleSet(ClientWindowTitleSetParams),
    #[serde(rename = "client.window_title.clear")]
    ClientWindowTitleClear(EmptyParams),
    #[serde(rename = "session.snapshot")]
    SessionSnapshot(EmptyParams),
    #[serde(rename = "workspace.create")]
    WorkspaceCreate(WorkspaceCreateParams),
    #[serde(rename = "workspace.list")]
    WorkspaceList(EmptyParams),
    #[serde(rename = "workspace.get")]
    WorkspaceGet(WorkspaceTarget),
    #[serde(rename = "workspace.focus")]
    WorkspaceFocus(WorkspaceTarget),
    #[serde(rename = "workspace.rename")]
    WorkspaceRename(WorkspaceRenameParams),
    #[serde(rename = "workspace.close")]
    WorkspaceClose(WorkspaceTarget),
    #[serde(rename = "workspace.move_to_group")]
    WorkspaceMoveToGroup(WorkspaceMoveToGroupParams),
    #[serde(rename = "tab.create")]
    TabCreate(TabCreateParams),
    #[serde(rename = "tab.list")]
    TabList(TabListParams),
    #[serde(rename = "tab.get")]
    TabGet(TabTarget),
    #[serde(rename = "tab.focus")]
    TabFocus(TabTarget),
    #[serde(rename = "tab.rename")]
    TabRename(TabRenameParams),
    #[serde(rename = "tab.close")]
    TabClose(TabTarget),
    #[serde(rename = "agent.list")]
    AgentList(EmptyParams),
    #[serde(rename = "agent.get")]
    AgentGet(AgentTarget),
    #[serde(rename = "agent.read")]
    AgentRead(AgentReadParams),
    #[serde(rename = "agent.explain")]
    AgentExplain(AgentTarget),
    #[serde(rename = "agent.send_keys")]
    AgentSendKeys(AgentSendKeysParams),
    #[serde(rename = "agent.prompt")]
    AgentPrompt(AgentPromptParams),
    #[serde(rename = "agent.rename")]
    AgentRename(AgentRenameParams),
    #[serde(rename = "agent.view.set")]
    AgentViewSet(AgentViewSetParams),
    #[serde(rename = "agent.view.clear")]
    AgentViewClear(AgentViewClearParams),
    #[serde(rename = "agent.focus")]
    AgentFocus(AgentTarget),
    #[serde(rename = "agent.start")]
    AgentStart(AgentStartParams),
    #[serde(rename = "agent.wait")]
    AgentWait(AgentWaitParams),
    #[serde(rename = "pane.split")]
    PaneSplit(PaneSplitParams),
    #[serde(rename = "pane.focus")]
    PaneFocus(PaneTarget),
    #[serde(rename = "pane.swap")]
    PaneSwap(PaneSwapParams),
    #[serde(rename = "pane.move")]
    PaneMove(PaneMoveParams),
    #[serde(rename = "pane.zoom")]
    PaneZoom(PaneZoomParams),
    #[serde(rename = "pane.layout")]
    PaneLayout(PaneLayoutParams),
    #[serde(rename = "pane.process_info")]
    PaneProcessInfo(PaneProcessInfoParams),
    #[serde(rename = "layout.export")]
    LayoutExport(LayoutExportParams),
    #[serde(rename = "layout.apply")]
    LayoutApply(LayoutApplyParams),
    #[serde(rename = "pane.neighbor")]
    PaneNeighbor(PaneNeighborParams),
    #[serde(rename = "pane.edges")]
    PaneEdges(PaneEdgesParams),
    #[serde(rename = "pane.focus_direction")]
    PaneFocusDirection(PaneFocusDirectionParams),
    #[serde(rename = "pane.resize")]
    PaneResize(PaneResizeParams),
    #[serde(rename = "pane.list")]
    PaneList(PaneListParams),
    #[serde(rename = "pane.current")]
    PaneCurrent(PaneCurrentParams),
    #[serde(rename = "pane.get")]
    PaneGet(PaneTarget),
    #[serde(rename = "pane.rename")]
    PaneRename(PaneRenameParams),
    #[serde(rename = "pane.send_text")]
    PaneSendText(PaneSendTextParams),
    #[serde(rename = "pane.send_keys")]
    PaneSendKeys(PaneSendKeysParams),
    #[serde(rename = "pane.send_input")]
    PaneSendInput(PaneSendInputParams),
    #[serde(rename = "pane.read")]
    PaneRead(PaneReadParams),
    #[serde(rename = "pane.graphics.set")]
    PaneGraphicsSet(PaneGraphicsSetParams),
    #[serde(rename = "pane.graphics.clear")]
    PaneGraphicsClear(PaneGraphicsClearParams),
    #[serde(rename = "pane.graphics.info")]
    PaneGraphicsInfo(PaneTarget),
    #[serde(rename = "pane.graphics.stream")]
    #[schemars(skip)]
    PaneGraphicsStream(PaneGraphicsStreamParams),
    #[serde(skip)]
    #[schemars(skip)]
    PaneGraphicsStreamSet(PaneGraphicsSetParams),
    #[serde(skip)]
    #[schemars(skip)]
    PaneGraphicsStreamOpen(PaneGraphicsStreamParams),
    #[serde(skip)]
    #[schemars(skip)]
    PaneGraphicsStreamClose(PaneGraphicsStreamParams),
    #[serde(skip)]
    #[schemars(skip)]
    PaneGraphicsStreamDirect(PaneGraphicsDirectParams),
    #[serde(rename = "pane.report_agent")]
    PaneReportAgent(PaneReportAgentParams),
    #[serde(rename = "pane.report_agent_session")]
    PaneReportAgentSession(PaneReportAgentSessionParams),
    #[serde(rename = "pane.report_metadata")]
    PaneReportMetadata(PaneReportMetadataParams),
    #[serde(rename = "pane.clear_agent_authority")]
    PaneClearAgentAuthority(PaneClearAgentAuthorityParams),
    #[serde(rename = "pane.release_agent")]
    PaneReleaseAgent(PaneReleaseAgentParams),
    #[serde(rename = "pane.close")]
    PaneClose(PaneTarget),
    #[serde(rename = "events.subscribe")]
    EventsSubscribe(EventsSubscribeParams),
    #[serde(rename = "events.wait")]
    EventsWait(EventsWaitParams),
    #[serde(rename = "pane.wait_for_output")]
    PaneWaitForOutput(PaneWaitForOutputParams),
    #[serde(rename = "integration.install")]
    IntegrationInstall(IntegrationInstallParams),
    #[serde(rename = "integration.uninstall")]
    IntegrationUninstall(IntegrationUninstallParams),
    #[serde(rename = "plugin.link")]
    PluginLink(PluginLinkParams),
    #[serde(rename = "plugin.list")]
    PluginList(PluginListParams),
    #[serde(rename = "plugin.unlink")]
    PluginUnlink(PluginUnlinkParams),
    #[serde(rename = "plugin.enable")]
    PluginEnable(PluginSetEnabledParams),
    #[serde(rename = "plugin.disable")]
    PluginDisable(PluginSetEnabledParams),
    #[serde(rename = "plugin.action.list")]
    PluginActionList(PluginActionListParams),
    #[serde(rename = "plugin.action.invoke")]
    PluginActionInvoke(PluginActionInvokeParams),
    #[serde(rename = "plugin.log.list")]
    PluginLogList(PluginLogListParams),
    #[serde(rename = "plugin.pane.open")]
    PluginPaneOpen(PluginPaneOpenParams),
    #[serde(rename = "plugin.pane.focus")]
    PluginPaneFocus(PluginPaneFocusParams),
    #[serde(rename = "plugin.pane.close")]
    PluginPaneClose(PluginPaneCloseParams),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct EmptyParams {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct PingParams {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NotificationShowParams {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<ToastPosition>,
    #[serde(default, skip_serializing_if = "NotificationShowSound::is_none")]
    pub sound: NotificationShowSound,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum NotificationShowSound {
    #[default]
    None,
    Done,
    Request,
}

impl NotificationShowSound {
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotificationShowReason {
    Shown,
    Disabled,
    RateLimited,
    NoForegroundClient,
    Busy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClientWindowTitleReason {
    Set,
    Cleared,
    NoForegroundClient,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorkspaceTarget {
    pub workspace_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GroupTarget {
    pub group_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneTarget {
    pub pane_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TabTarget {
    pub tab_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ClientWindowTitleSetParams {
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ResourceLocationParams {
    pub execution_host_id: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ConnectionTarget {
    pub profile_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ConnectionRetireParams {
    pub profile_id: String,
    pub execution_host_id: String,
    #[serde(default)]
    pub local_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ConnectionSaveParams {
    pub profile_id: String,
    pub name: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_directory: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionAction {
    Test,
    Connect,
    Disconnect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatusKind {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    Disconnecting,
    AuthenticationRequired,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ConnectionProfileInfo {
    pub profile_id: String,
    pub name: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_directory: Option<String>,
    pub execution_host_id: String,
    pub status: ConnectionStatusKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorkspaceCreateParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<ResourceLocationParams>,
    #[serde(default)]
    pub focus: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GroupCreateParams {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_location: Option<ResourceLocationParams>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GroupRenameParams {
    pub group_id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorkspaceMoveToGroupParams {
    pub workspace_id: String,
    pub group_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorkspaceRenameParams {
    pub workspace_id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TabCreateParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<ResourceLocationParams>,
    #[serde(default)]
    pub focus: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct TabListParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TabRenameParams {
    pub tab_id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentTarget {
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentReadParams {
    pub target: String,
    pub source: ReadSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<u32>,
    #[serde(default)]
    pub format: ReadFormat,
    #[serde(default = "default_true")]
    pub strip_ansi: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentSendKeysParams {
    pub target: String,
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentWaitParams {
    pub target: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub until: Vec<AgentStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentPromptWaitOptions {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub until: Vec<AgentStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentPromptParams {
    pub target: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait: Option<AgentPromptWaitOptions>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentRenameParams {
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentViewSetParams {
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<AgentViewFilter>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sort: Vec<AgentViewSort>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct AgentViewClearParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum AgentViewFilter {
    All {
        filters: Vec<AgentViewFilter>,
    },
    Any {
        filters: Vec<AgentViewFilter>,
    },
    Not {
        filter: Box<AgentViewFilter>,
    },
    Eq {
        field: AgentViewField,
        value: AgentViewValue,
    },
    In {
        field: AgentViewField,
        values: Vec<AgentViewValue>,
    },
    Exists {
        field: AgentViewField,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum AgentViewField {
    Builtin(AgentViewBuiltinField),
    Token { token: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentViewBuiltinField {
    Status,
    WorkspaceId,
    TabId,
    PaneId,
    Agent,
    Seen,
    StateChangeSeq,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum AgentViewValue {
    String(String),
    Bool(bool),
    Number(u64),
    Context { context: AgentViewContext },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentViewContext {
    CurrentWorkspaceId,
    CurrentTabId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentViewSort {
    pub field: AgentViewSortField,
    #[serde(default)]
    pub order: AgentViewSortOrder,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum AgentViewSortField {
    Builtin(AgentViewBuiltinSortField),
    Token { token: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentViewBuiltinSortField {
    WorkspaceOrder,
    TabOrder,
    PaneOrder,
    Attention,
    Status,
    Agent,
    Seen,
    StateChangeSeq,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum AgentViewSortOrder {
    #[default]
    Asc,
    Desc,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentStartParams {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<ResourceLocationParams>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split: Option<SplitDirection>,
    #[serde(default)]
    pub focus: bool,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
    pub argv: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneSplitParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_pane_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<ResourceLocationParams>,
    pub direction: SplitDirection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ratio: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default)]
    pub focus: bool,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SplitDirection {
    Right,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PaneDirection {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct PaneSwapParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<PaneDirection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_pane_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_pane_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneMoveParams {
    pub pane_id: String,
    pub destination: PaneMoveDestination,
    #[serde(default)]
    pub focus: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PaneMoveDestination {
    Tab {
        tab_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target_pane_id: Option<String>,
        split: SplitDirection,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ratio: Option<f32>,
    },
    NewTab {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    NewWorkspace {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tab_label: Option<String>,
    },
}

impl Eq for PaneMoveParams {}
impl Eq for PaneMoveDestination {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct PaneZoomParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    #[serde(default)]
    pub mode: PaneZoomMode,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PaneZoomMode {
    #[default]
    Toggle,
    On,
    Off,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct PaneLayoutParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct PaneProcessInfoParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct LayoutExportParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LayoutApplyParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_label: Option<String>,
    #[serde(default)]
    pub focus: bool,
    pub root: LayoutNode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LayoutDescription {
    pub workspace_id: String,
    pub tab_id: String,
    pub zoomed: bool,
    pub focused_pane_id: String,
    pub root: LayoutNode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LayoutNode {
    Pane {
        #[serde(flatten)]
        pane: LayoutPane,
    },
    Split {
        direction: SplitDirection,
        ratio: f32,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct LayoutPane {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneNeighborParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    pub direction: PaneDirection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct PaneEdgesParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneFocusDirectionParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    pub direction: PaneDirection,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneResizeParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    pub direction: PaneDirection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct PaneListParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct PaneCurrentParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller_pane_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneRenameParams {
    pub pane_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneSendTextParams {
    pub pane_id: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneSendKeysParams {
    pub pane_id: String,
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneSendInputParams {
    pub pane_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ServerLiveHandoffParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub import_exe: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_protocol: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneReadParams {
    pub pane_id: String,
    pub source: ReadSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<u32>,
    #[serde(default)]
    pub format: ReadFormat,
    #[serde(default = "default_true")]
    pub strip_ansi: bool,
    #[serde(skip)]
    #[schemars(skip)]
    pub intent: ReadIntent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneReportAgentParams {
    pub pane_id: String,
    pub source: String,
    pub agent: String,
    pub state: PaneAgentState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_path: Option<String>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub launch_env: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneReportAgentSessionParams {
    pub pane_id: String,
    pub source: String,
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_start_source: Option<String>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub launch_env: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneReportMetadataParams {
    pub pane_id: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applies_to_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_status: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub state_labels: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub tokens: HashMap<String, Option<String>>,
    #[serde(default)]
    pub clear_title: bool,
    #[serde(default)]
    pub clear_display_agent: bool,
    #[serde(default)]
    pub clear_custom_status: bool,
    #[serde(default)]
    pub clear_state_labels: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneClearAgentAuthorityParams {
    pub pane_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneReleaseAgentParams {
    pub pane_id: String,
    pub source: String,
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReadSource {
    Visible,
    Recent,
    RecentUnwrapped,
    Detection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReadIntent {
    #[default]
    Interactive,
    Passive,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ReadFormat {
    #[default]
    Text,
    Ansi,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EventsSubscribeParams {
    pub subscriptions: Vec<Subscription>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type")]
pub enum Subscription {
    #[serde(rename = "workspace.created")]
    WorkspaceCreated {},
    #[serde(rename = "workspace.updated")]
    WorkspaceUpdated {},
    #[serde(rename = "workspace.renamed")]
    WorkspaceRenamed {},
    #[serde(rename = "workspace.closed")]
    WorkspaceClosed {},
    #[serde(rename = "workspace.focused")]
    WorkspaceFocused {},
    #[serde(rename = "tab.created")]
    TabCreated {},
    #[serde(rename = "tab.closed")]
    TabClosed {},
    #[serde(rename = "tab.focused")]
    TabFocused {},
    #[serde(rename = "tab.renamed")]
    TabRenamed {},
    #[serde(rename = "pane.created")]
    PaneCreated {},
    #[serde(rename = "pane.closed")]
    PaneClosed {},
    #[serde(rename = "pane.focused")]
    PaneFocused {},
    #[serde(rename = "pane.moved")]
    PaneMoved {},
    #[serde(rename = "pane.exited")]
    PaneExited {},
    #[serde(rename = "pane.agent_detected")]
    PaneAgentDetected {},
    #[serde(rename = "pane.output_matched")]
    PaneOutputMatched {
        pane_id: String,
        source: ReadSource,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lines: Option<u32>,
        r#match: OutputMatch,
        #[serde(default = "default_true")]
        strip_ansi: bool,
    },
    #[serde(rename = "pane.agent_status_changed")]
    PaneAgentStatusChanged {
        pane_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent_status: Option<AgentStatus>,
    },
    #[serde(rename = "pane.scroll_changed")]
    PaneScrollChanged { pane_id: String },
    #[serde(rename = "layout.updated")]
    LayoutUpdated {},
    #[serde(rename = "connection.status_changed")]
    ConnectionStatusChanged {},
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EventsWaitParams {
    pub match_event: EventMatch,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneWaitForOutputParams {
    pub pane_id: String,
    pub source: ReadSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<u32>,
    pub r#match: OutputMatch,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default = "default_true")]
    pub strip_ansi: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct IntegrationInstallParams {
    pub target: IntegrationTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct IntegrationUninstallParams {
    pub target: IntegrationTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationTarget {
    Pi,
    Omp,
    Claude,
    Codex,
    Copilot,
    Devin,
    Kimi,
    Droid,
    Opencode,
    Hermes,
    Qodercli,
    Cursor,
    Grok,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PluginLinkParams {
    pub path: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<PluginSourceInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct PluginListParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PluginUnlinkParams {
    pub plugin_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PluginSetEnabledParams {
    pub plugin_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct InstalledPluginInfo {
    pub plugin_id: String,
    pub name: String,
    pub version: String,
    pub min_gardn_version: String,
    #[serde(default)]
    pub manifest_dialect: PluginManifestDialect,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub manifest_path: String,
    pub plugin_root: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platforms: Option<Vec<PluginPlatform>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub build: Vec<PluginManifestBuild>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub startup: Vec<PluginManifestStartup>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<PluginManifestAction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<PluginManifestEventHook>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub panes: Vec<PluginManifestPane>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_handlers: Vec<PluginManifestLinkHandler>,
    #[serde(default)]
    pub source: PluginSourceInfo,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PluginSourceInfo {
    #[serde(default)]
    pub kind: PluginSourceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subdir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub managed_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_unix_ms: Option<u64>,
}

impl Default for PluginSourceInfo {
    fn default() -> Self {
        Self {
            kind: PluginSourceKind::Local,
            owner: None,
            repo: None,
            subdir: None,
            requested_ref: None,
            resolved_commit: None,
            managed_path: None,
            installed_unix_ms: None,
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PluginSourceKind {
    #[default]
    Local,
    Github,
}

pub fn plugin_managed_path_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupSize {
    Cells(u16),
    Percent(u8),
}

impl PopupSize {
    pub fn resolve(self, available: u16) -> u16 {
        match self {
            Self::Cells(cells) => cells,
            Self::Percent(percent) => ((available as u32 * percent as u32) / 100) as u16,
        }
    }

    pub fn parse_cli(value: &str) -> Result<Self, String> {
        if let Some(percent) = value.strip_suffix('%') {
            let percent = percent
                .parse::<u8>()
                .map_err(|_| "must be a number of cells or a percentage like 80%".to_string())?;
            if !(1..=100).contains(&percent) {
                return Err("percentage must be between 1% and 100%".to_string());
            }
            return Ok(Self::Percent(percent));
        }
        value
            .parse::<u16>()
            .map(Self::Cells)
            .map_err(|_| "must be a number of cells or a percentage like 80%".to_string())
    }

    fn parse_percent_string(value: &str) -> Result<Self, String> {
        if value.ends_with('%') {
            return Self::parse_cli(value);
        }
        Err("string sizes must be percentages like 80%; use a number for cells".to_string())
    }
}

impl Serialize for PopupSize {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Cells(cells) => serializer.serialize_u16(*cells),
            Self::Percent(percent) => serializer.serialize_str(&format!("{percent}%")),
        }
    }
}

impl<'de> Deserialize<'de> for PopupSize {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct PopupSizeVisitor;

        impl serde::de::Visitor<'_> for PopupSizeVisitor {
            type Value = PopupSize;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a cell count or percentage string like 80%")
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let value =
                    u16::try_from(value).map_err(|_| E::custom("cell count must fit in u16"))?;
                Ok(PopupSize::Cells(value))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let value = u16::try_from(value)
                    .map_err(|_| E::custom("cell count must be between 0 and 65535"))?;
                Ok(PopupSize::Cells(value))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                PopupSize::parse_percent_string(value).map_err(E::custom)
            }
        }

        deserializer.deserialize_any(PopupSizeVisitor)
    }
}

impl schemars::JsonSchema for PopupSize {
    fn schema_name() -> Cow<'static, str> {
        "PopupSize".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "oneOf": [
                {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": 65535,
                    "description": "Outer popup size in terminal cells, including the border."
                },
                {
                    "type": "string",
                    "pattern": "^(100|[1-9][0-9]?)%$",
                    "description": "Outer popup size as a percentage of the terminal area, for example 80%."
                }
            ]
        })
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PluginManifestDialect {
    #[default]
    Gardn,
    HerdrV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PluginManifestBuild {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platforms: Option<Vec<PluginPlatform>>,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PluginManifestStartup {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platforms: Option<Vec<PluginPlatform>>,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PluginManifestAction {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contexts: Vec<PluginActionContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platforms: Option<Vec<PluginPlatform>>,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PluginManifestEventHook {
    pub on: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platforms: Option<Vec<PluginPlatform>>,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PluginManifestPane {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platforms: Option<Vec<PluginPlatform>>,
    #[serde(default)]
    pub placement: PluginPanePlacement,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<PopupSize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<PopupSize>,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PluginManifestLinkHandler {
    pub id: String,
    pub title: String,
    pub pattern: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platforms: Option<Vec<PluginPlatform>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct PluginActionListParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema)]
pub struct PluginLogListParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PluginActionInvokeParams {
    pub action_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<PluginInvocationContext>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PluginCommandLogInfo {
    pub log_id: String,
    pub plugin_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    pub command: Vec<String>,
    pub status: PluginCommandStatus,
    pub started_unix_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_unix_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PluginCommandStatus {
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PluginPlatform {
    Linux,
    Macos,
    Windows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PluginActionContext {
    Global,
    Workspace,
    Tab,
    Pane,
    Selection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PluginInvocationContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused_pane_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused_pane_cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused_pane_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused_pane_status: Option<AgentStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invocation_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clicked_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_handler_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PluginActionInfo {
    pub plugin_id: String,
    pub action_id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contexts: Vec<PluginActionContext>,
    pub command: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platforms: Option<Vec<PluginPlatform>>,
}

impl PluginActionInfo {
    pub fn qualified_id(&self) -> String {
        format!("{}.{}", self.plugin_id, self.action_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PluginPaneOpenParams {
    pub plugin_id: String,
    pub entrypoint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<PluginPanePlacement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<PopupSize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<PopupSize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_pane_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<SplitDirection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default)]
    pub focus: bool,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PluginPanePlacement {
    #[default]
    Overlay,
    Popup,
    Split,
    Tab,
    Zoomed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PluginPaneFocusParams {
    pub pane_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PluginPaneCloseParams {
    pub pane_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PluginPaneInfo {
    pub plugin_id: String,
    pub entrypoint: String,
    pub pane: PaneInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputMatch {
    Substring { value: String },
    Regex { value: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum EventMatch {
    WorkspaceCreated {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace_id: Option<String>,
    },
    WorkspaceUpdated {
        workspace_id: String,
    },
    WorkspaceClosed {
        workspace_id: String,
    },
    WorkspaceRenamed {
        workspace_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    WorkspaceFocused {
        workspace_id: String,
    },
    TabCreated {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tab_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace_id: Option<String>,
    },
    TabClosed {
        tab_id: String,
    },
    TabRenamed {
        tab_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    TabFocused {
        tab_id: String,
    },
    PaneCreated {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pane_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace_id: Option<String>,
    },
    PaneClosed {
        pane_id: String,
    },
    PaneFocused {
        pane_id: String,
    },
    PaneMoved {
        pane_id: String,
    },
    PaneOutputChanged {
        pane_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min_revision: Option<u64>,
    },
    PaneExited {
        pane_id: String,
    },
    PaneAgentDetected {
        pane_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent: Option<String>,
    },
    PaneAgentStatusChanged {
        pane_id: String,
        agent_status: AgentStatus,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    WorkspaceCreated,
    WorkspaceUpdated,
    WorkspaceClosed,
    WorkspaceRenamed,
    WorkspaceFocused,
    TabCreated,
    TabClosed,
    TabRenamed,
    TabFocused,
    PaneCreated,
    PaneClosed,
    PaneFocused,
    PaneMoved,
    PaneOutputChanged,
    PaneExited,
    PaneAgentDetected,
    PaneAgentStatusChanged,
    ConnectionStatusChanged,
    LayoutUpdated,
}

impl EventKind {
    pub fn dot_name(self) -> &'static str {
        match self {
            EventKind::WorkspaceCreated => "workspace.created",
            EventKind::WorkspaceUpdated => "workspace.updated",
            EventKind::WorkspaceClosed => "workspace.closed",
            EventKind::WorkspaceRenamed => "workspace.renamed",
            EventKind::WorkspaceFocused => "workspace.focused",
            EventKind::TabCreated => "tab.created",
            EventKind::TabClosed => "tab.closed",
            EventKind::TabRenamed => "tab.renamed",
            EventKind::TabFocused => "tab.focused",
            EventKind::PaneCreated => "pane.created",
            EventKind::PaneClosed => "pane.closed",
            EventKind::PaneFocused => "pane.focused",
            EventKind::PaneMoved => "pane.moved",
            EventKind::PaneOutputChanged => "pane.output_changed",
            EventKind::PaneExited => "pane.exited",
            EventKind::PaneAgentDetected => "pane.agent_detected",
            EventKind::LayoutUpdated => "layout.updated",
            EventKind::PaneAgentStatusChanged => "pane.agent_status_changed",
            EventKind::ConnectionStatusChanged => "connection.status_changed",
        }
    }
}

pub const PLUGIN_HOOK_EVENT_KINDS: &[EventKind] = &[
    EventKind::WorkspaceCreated,
    EventKind::WorkspaceUpdated,
    EventKind::WorkspaceClosed,
    EventKind::WorkspaceRenamed,
    EventKind::WorkspaceFocused,
    EventKind::TabCreated,
    EventKind::TabClosed,
    EventKind::TabRenamed,
    EventKind::TabFocused,
    EventKind::PaneCreated,
    EventKind::PaneClosed,
    EventKind::PaneFocused,
    EventKind::PaneMoved,
    EventKind::PaneExited,
    EventKind::PaneAgentDetected,
    EventKind::PaneAgentStatusChanged,
];

pub fn plugin_hook_event_names() -> Vec<&'static str> {
    PLUGIN_HOOK_EVENT_KINDS
        .iter()
        .copied()
        .map(EventKind::dot_name)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SuccessResponse {
    pub id: String,
    pub result: ResponseResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ErrorResponse {
    pub id: String,
    pub error: ErrorBody,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ServerCapabilities {
    pub live_handoff: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseResult {
    Pong {
        version: String,
        protocol: u32,
        #[serde(default)]
        capabilities: Option<ServerCapabilities>,
    },
    SessionSnapshot {
        snapshot: Box<SessionSnapshot>,
    },
    WorkspaceInfo {
        workspace: WorkspaceInfo,
    },
    WorkspaceCreated {
        workspace: WorkspaceInfo,
        tab: TabInfo,
        root_pane: PaneInfo,
    },
    WorkspaceList {
        workspaces: Vec<WorkspaceInfo>,
    },
    ConnectionProfile {
        profile: ConnectionProfileInfo,
    },
    ConnectionList {
        profiles: Vec<ConnectionProfileInfo>,
    },
    ConnectionDeleted {
        profile_id: String,
        removed: bool,
    },
    ConnectionActionQueued {
        profile: ConnectionProfileInfo,
        action: ConnectionAction,
    },
    ConnectionRetireStart {
        profile_id: String,
        execution_host_id: String,
        accepted: bool,
        remaining_panes: usize,
        remaining_terminals: usize,
        pending_terminations: usize,
    },
    ConnectionRetireStatus {
        profile_id: String,
        execution_host_id: String,
        ready: bool,
        remaining_panes: usize,
        remaining_terminals: usize,
        pending_terminations: usize,
    },
    GroupInfo {
        group: GroupInfo,
    },
    GroupList {
        groups: Vec<GroupInfo>,
    },
    TabInfo {
        tab: TabInfo,
    },
    TabCreated {
        tab: TabInfo,
        root_pane: PaneInfo,
    },
    TabList {
        tabs: Vec<TabInfo>,
    },
    AgentInfo {
        agent: AgentInfo,
    },
    AgentStarted {
        agent: AgentInfo,
        argv: Vec<String>,
    },
    AgentPrompted {
        agent: AgentInfo,
    },
    AgentList {
        agents: Vec<AgentInfo>,
    },
    AgentView {
        active: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    PaneInfo {
        pane: PaneInfo,
    },
    PaneList {
        panes: Vec<PaneInfo>,
    },
    PaneCurrent {
        pane: PaneInfo,
    },
    PaneSwap {
        swap: PaneSwapResult,
    },
    PaneZoom {
        zoom: PaneZoomResult,
    },
    PaneLayout {
        layout: PaneLayoutSnapshot,
    },
    PaneProcessInfo {
        process_info: PaneProcessInfo,
    },
    LayoutExport {
        layout: LayoutDescription,
    },
    LayoutApply {
        layout: LayoutDescription,
    },
    PaneNeighbor {
        neighbor: PaneNeighborResult,
    },
    PaneEdges {
        edges: PaneEdgesResult,
    },
    PaneFocusDirection {
        focus: PaneFocusDirectionResult,
    },
    PaneResize {
        resize: PaneResizeResult,
    },
    PaneMove {
        move_result: PaneMoveResult,
    },
    PaneRead {
        read: PaneReadResult,
    },
    PaneGraphicsInfo {
        cell_width_px: u32,
        cell_height_px: u32,
        pane_visible: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file_frame_directory: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        file_frame_formats: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file_frame_max_bytes: Option<usize>,
        file_frame_damage: bool,
        max_layers_per_pane: usize,
        pixel_mouse: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file_frame_transport: Option<String>,
    },
    PaneGraphicsFrameAck {
        sequence: u64,
        revision: u64,
    },
    AgentExplain {
        explain: serde_json::Value,
    },
    SubscriptionStarted {},
    WaitMatched {
        event: EventEnvelope,
    },
    OutputMatched {
        pane_id: String,
        revision: u64,
        matched_line: Option<String>,
        read: PaneReadResult,
    },
    NotificationShow {
        shown: bool,
        reason: NotificationShowReason,
    },
    ClientWindowTitle {
        changed: bool,
        reason: ClientWindowTitleReason,
    },
    IntegrationInstall {
        target: IntegrationTarget,
        details: IntegrationInstallResult,
    },
    IntegrationUninstall {
        target: IntegrationTarget,
        details: IntegrationUninstallResult,
    },
    AgentManifestReload {
        manifests: Vec<AgentManifestInfo>,
    },
    AgentManifestStatus {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        last_check_unix: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        last_result: Option<String>,
        manifests: Vec<AgentManifestInfo>,
    },
    PluginLinked {
        plugin: InstalledPluginInfo,
    },
    PluginList {
        plugins: Vec<InstalledPluginInfo>,
    },
    PluginUnlinked {
        plugin_id: String,
        removed: bool,
    },
    PluginEnabled {
        plugin: InstalledPluginInfo,
    },
    PluginDisabled {
        plugin: InstalledPluginInfo,
    },
    PluginActionList {
        actions: Vec<PluginActionInfo>,
    },
    PluginActionInvoked {
        action: PluginActionInfo,
        context: PluginInvocationContext,
        log: PluginCommandLogInfo,
    },
    PluginLogList {
        logs: Vec<PluginCommandLogInfo>,
    },
    PluginPaneOpened {
        plugin_pane: PluginPaneInfo,
    },
    PluginPaneFocused {
        plugin_pane: PluginPaneInfo,
    },
    PluginPaneClosed {
        pane_id: String,
    },
    ConfigReload {
        status: ConfigReloadStatus,
        diagnostics: Vec<String>,
    },
    Ok {},
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentManifestInfo {
    pub agent: String,
    pub source: String,
    pub source_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_remote_version: Option<String>,
    pub local_override_shadowing_remote: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_update_result: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_update_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_last_checked_unix: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorkspaceInfo {
    pub workspace_id: String,
    pub group_id: String,
    pub default_location: ResourceLocationParams,
    pub number: usize,
    pub label: String,
    pub focused: bool,
    pub pane_count: usize,
    pub tab_count: usize,
    pub active_tab_id: String,
    pub agent_status: AgentStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GroupInfo {
    pub group_id: String,
    pub number: usize,
    pub name: String,
    pub icon: String,
    pub focused: bool,
    pub workspace_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_location: Option<ResourceLocationParams>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TabInfo {
    pub tab_id: String,
    pub workspace_id: String,
    pub number: usize,
    pub label: String,
    pub focused: bool,
    pub pane_count: usize,
    pub agent_status: AgentStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentInfo {
    pub terminal_id: String,
    pub location: ResourceLocationParams,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_agent: Option<String>,
    pub agent_status: AgentStatus,
    #[serde(default, skip_serializing_if = "is_false")]
    pub screen_detection_skipped: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_status: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub tokens: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub state_labels: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session: Option<AgentSessionInfo>,
    pub workspace_id: String,
    pub tab_id: String,
    pub pane_id: String,
    pub focused: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreground_cwd: Option<String>,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneInfo {
    pub pane_id: String,
    pub terminal_id: String,
    pub location: ResourceLocationParams,
    pub workspace_id: String,
    pub tab_id: String,
    pub focused: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreground_cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_agent: Option<String>,
    pub agent_status: AgentStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_status: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub state_labels: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub tokens: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session: Option<AgentSessionInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scroll: Option<PaneScrollInfo>,
    pub revision: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneScrollInfo {
    pub offset_from_bottom: u64,
    pub max_offset_from_bottom: u64,
    pub viewport_rows: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentSessionInfo {
    pub source: String,
    pub agent: String,
    pub kind: AgentSessionRefKind,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneProcessInfo {
    pub pane_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreground_process_group_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tty: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub foreground_processes: Vec<PaneProcessInfoProcess>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneProcessInfoProcess {
    pub pid: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub argv0: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub argv: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cmdline: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

pub const PANE_GRAPHICS_SET_MAX_BYTES: usize = 512 * 1024;
pub const PANE_GRAPHICS_STREAM_MAX_BYTES: usize = 16 * 1024 * 1024;
pub const PANE_GRAPHICS_MAX_LAYERS_PER_PANE: usize = 16;
pub const PANE_GRAPHICS_MAX_LAYERS_TOTAL: usize = 64;
pub const PANE_GRAPHICS_MAX_INLINE_BYTES_TOTAL: usize = 64 * 1024 * 1024;
pub const PANE_GRAPHICS_PRIMARY_LAYER_ID: &str = "primary";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PaneGraphicsFormat {
    Png,
    Rgb,
    Rgba,
    Bgra,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneGraphicsSetParams {
    pub pane_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer_id: Option<String>,
    #[serde(default)]
    pub z_index: i32,
    #[serde(skip)]
    #[schemars(skip)]
    pub owner: String,
    pub format: PaneGraphicsFormat,
    pub image_width: u32,
    pub image_height: u32,
    #[serde(skip)]
    pub data: Option<Vec<u8>>,
    #[serde(default)]
    pub data_base64: String,
    #[serde(default)]
    pub placement: PaneGraphicsPlacementParams,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default,
)]
pub struct PaneGraphicsPlacementParams {
    #[serde(default)]
    pub viewport_col: i32,
    #[serde(default)]
    pub viewport_row: i32,
    #[serde(default)]
    pub grid_cols: u32,
    #[serde(default)]
    pub grid_rows: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneGraphicsClearParams {
    pub pane_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneGraphicsDirectParams {
    pub pane_id: String,
    pub layer_id: Option<String>,
    pub z_index: i32,
    pub owner: String,
    pub image_width: u32,
    pub image_height: u32,
    pub format: PaneGraphicsFormat,
    pub path: String,
    pub sequence: u64,
    pub revision: u64,
    pub placement: PaneGraphicsPlacementParams,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneGraphicsStreamParams {
    pub pane_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer_id: Option<String>,
    #[serde(default)]
    pub z_index: i32,
    #[serde(skip)]
    #[schemars(skip)]
    pub owner: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneSwapResult {
    pub changed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<PaneSwapReason>,
    pub source_pane_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_pane_id: Option<String>,
    pub focused_pane_id: String,
    pub layout: PaneLayoutSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PaneSwapReason {
    NoNeighbor,
    SamePane,
    NotFound,
    CrossTab,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneMoveResult {
    pub changed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<PaneMoveReason>,
    pub previous_pane_id: String,
    pub previous_workspace_id: String,
    pub previous_tab_id: String,
    pub pane: Box<PaneInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_layout: Option<Box<PaneLayoutSnapshot>>,
    pub target_layout: Box<PaneLayoutSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_workspace: Option<WorkspaceInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_tab: Option<TabInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_tab_id: Option<String>,
    pub focused_pane_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PaneMoveReason {
    SameTab,
    ZoomedTab,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneLayoutSnapshot {
    pub workspace_id: String,
    pub tab_id: String,
    pub zoomed: bool,
    pub area: PaneLayoutRect,
    pub focused_pane_id: String,
    pub panes: Vec<PaneLayoutPane>,
    pub splits: Vec<PaneLayoutSplit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneLayoutRect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneLayoutPane {
    pub pane_id: String,
    pub focused: bool,
    pub rect: PaneLayoutRect,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneLayoutSplit {
    pub id: String,
    pub direction: SplitDirection,
    pub ratio: f32,
    pub rect: PaneLayoutRect,
}

impl Eq for PaneMoveResult {}
impl Eq for PaneLayoutSnapshot {}
impl Eq for PaneLayoutSplit {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneZoomResult {
    pub changed: bool,
    pub zoom_changed: bool,
    pub focus_changed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<PaneZoomReason>,
    pub pane_id: String,
    pub focused_pane_id: String,
    pub zoomed: bool,
    pub layout: PaneLayoutSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PaneZoomReason {
    SinglePane,
    AlreadyZoomed,
    AlreadyUnzoomed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneNeighborResult {
    pub pane_id: String,
    pub direction: PaneDirection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub neighbor_pane_id: Option<String>,
    pub layout: PaneLayoutSnapshot,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneEdgesResult {
    pub pane_id: String,
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    pub layout: PaneLayoutSnapshot,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneFocusDirectionResult {
    pub changed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<PaneFocusDirectionReason>,
    pub source_pane_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused_pane_id: Option<String>,
    pub layout: PaneLayoutSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PaneFocusDirectionReason {
    NoNeighbor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneResizeResult {
    pub changed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<PaneResizeReason>,
    pub pane_id: String,
    pub focused_pane_id: String,
    pub layout: PaneLayoutSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PaneResizeReason {
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneReadResult {
    pub pane_id: String,
    pub workspace_id: String,
    pub tab_id: String,
    pub source: ReadSource,
    pub format: ReadFormat,
    pub text: String,
    pub revision: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct IntegrationInstallResult {
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct IntegrationUninstallResult {
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EventEnvelope {
    pub event: EventKind,
    pub data: EventData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum SubscriptionEventKind {
    #[serde(rename = "pane.output_matched")]
    PaneOutputMatched,
    #[serde(rename = "pane.agent_status_changed")]
    PaneAgentStatusChanged,
    #[serde(rename = "pane.scroll_changed")]
    ScrollChanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SubscriptionEventEnvelope {
    pub event: SubscriptionEventKind,
    pub data: SubscriptionEventData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum SubscriptionEventData {
    PaneOutputMatched(PaneOutputMatchedEvent),
    PaneAgentStatusChanged(PaneAgentStatusChangedEvent),
    ScrollChanged(PaneScrollChangedEvent),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneOutputMatchedEvent {
    pub pane_id: String,
    pub matched_line: String,
    pub read: PaneReadResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneAgentStatusChangedEvent {
    pub pane_id: String,
    pub workspace_id: String,
    pub agent_status: AgentStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_agent: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub state_labels: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub tokens: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PaneScrollChangedEvent {
    pub pane_id: String,
    pub workspace_id: String,
    pub scroll: PaneScrollInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventData {
    WorkspaceCreated {
        workspace: WorkspaceInfo,
    },
    WorkspaceUpdated {
        workspace: WorkspaceInfo,
    },
    WorkspaceClosed {
        workspace_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace: Option<WorkspaceInfo>,
    },
    WorkspaceRenamed {
        workspace_id: String,
        label: String,
    },
    WorkspaceFocused {
        workspace_id: String,
    },
    TabCreated {
        tab: TabInfo,
    },
    TabClosed {
        tab_id: String,
        workspace_id: String,
    },
    TabRenamed {
        tab_id: String,
        workspace_id: String,
        label: String,
    },
    TabFocused {
        tab_id: String,
        workspace_id: String,
    },
    PaneCreated {
        pane: PaneInfo,
    },
    PaneClosed {
        pane_id: String,
        workspace_id: String,
    },
    PaneFocused {
        pane_id: String,
        workspace_id: String,
    },
    PaneMoved {
        previous_pane_id: String,
        previous_workspace_id: String,
        previous_tab_id: String,
        pane: Box<PaneInfo>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        created_workspace: Option<WorkspaceInfo>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        created_tab: Option<TabInfo>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        closed_workspace_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        closed_tab_id: Option<String>,
    },
    PaneOutputChanged {
        pane_id: String,
        workspace_id: String,
        revision: u64,
    },
    PaneExited {
        pane_id: String,
        workspace_id: String,
    },
    PaneAgentDetected {
        pane_id: String,
        workspace_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent: Option<String>,
    },
    PaneAgentStatusChanged {
        pane_id: String,
        workspace_id: String,
        agent_status: AgentStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display_agent: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        custom_status: Option<String>,
        #[serde(default, skip_serializing_if = "HashMap::is_empty")]
        state_labels: HashMap<String, String>,
        #[serde(default, skip_serializing_if = "HashMap::is_empty")]
        tokens: HashMap<String, String>,
    },
    ConnectionStatusChanged {
        execution_host_id: String,
        status: ConnectionStatusKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    LayoutUpdated {
        layout: PaneLayoutSnapshot,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PaneAgentState {
    Idle,
    Working,
    Blocked,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Idle,
    Working,
    Blocked,
    Done,
    Unknown,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SessionSnapshot {
    pub version: String,
    pub protocol: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused_workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused_tab_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused_pane_id: Option<String>,
    pub workspaces: Vec<WorkspaceInfo>,
    pub tabs: Vec<TabInfo>,
    pub panes: Vec<PaneInfo>,
    pub layouts: Vec<PaneLayoutSnapshot>,
    pub agents: Vec<AgentInfo>,
}

fn generated_request_schema() -> serde_json::Value {
    let mut schema = match serde_json::to_value(schemars::schema_for!(Request)) {
        Ok(schema) => schema,
        Err(error) => panic!("failed to serialize generated Local API request schema: {error}"),
    };
    let Some(variants) = schema
        .get_mut("oneOf")
        .and_then(serde_json::Value::as_array_mut)
    else {
        panic!("generated Local API request schema is missing request variants");
    };

    for variant in variants {
        let Some(properties) = variant
            .get_mut("properties")
            .and_then(serde_json::Value::as_object_mut)
        else {
            panic!("generated Local API request variant is missing properties");
        };
        properties.insert(
            "id".to_string(),
            serde_json::json!({
                "description": "Caller-provided identifier echoed by the response.",
                "type": "string",
            }),
        );

        let Some(required) = variant
            .get_mut("required")
            .and_then(serde_json::Value::as_array_mut)
        else {
            panic!("generated Local API request variant is missing required properties");
        };
        required.push(serde_json::Value::String("id".to_string()));
        required.sort_by(|left, right| left.as_str().cmp(&right.as_str()));
    }

    schema
}

/// Return the socket API schema used by CLI and tooling consumers.
///
/// `product_version` and `protocol_version` are injected by the host so this
/// crate stays free of build-identity and protocol runtime modules.
pub fn generated_schema(product_version: &str, protocol_version: u32) -> serde_json::Value {
    serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "product_version": product_version,
        "protocol": protocol_version,
        "schema_version": 1,
        "schemas": {
            "error_response": schemars::schema_for!(ErrorResponse),
            "event": schemars::schema_for!(EventEnvelope),
            "request": generated_request_schema(),
            "response": schemars::schema_for!(SuccessResponse),
            "subscription_event": schemars::schema_for!(SubscriptionEventEnvelope),
        },
    })
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_schema_describes_request_ids_and_product_version() {
        let schema = generated_schema("0.3.2-test", 6);
        assert_eq!(
            schema
                .get("product_version")
                .and_then(serde_json::Value::as_str),
            Some("0.3.2-test")
        );
        assert_eq!(
            schema.get("protocol").and_then(serde_json::Value::as_u64),
            Some(6)
        );

        let variants = schema["schemas"]["request"]["oneOf"]
            .as_array()
            .expect("request schema should contain method variants");
        assert!(!variants.is_empty());
        for variant in variants {
            assert_eq!(variant["properties"]["id"]["type"], "string");
            assert!(variant["required"]
                .as_array()
                .expect("request variant should list required properties")
                .iter()
                .any(|property| property == "id"));
        }
    }

    #[test]
    fn request_round_trips_for_pane_read() {
        let request = Request {
            id: "req_1".into(),
            method: Method::PaneRead(PaneReadParams {
                pane_id: "p_1".into(),
                source: ReadSource::Recent,
                lines: Some(80),
                format: ReadFormat::Text,
                strip_ansi: true,
                intent: ReadIntent::Interactive,
            }),
        };

        let json = serde_json::to_string(&request).unwrap();
        let serialized: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(serialized["params"].get("intent").is_none());
        let restored: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, request);
        let Method::PaneRead(params) = restored.method else {
            panic!("expected pane read");
        };
        assert_eq!(params.intent, ReadIntent::Interactive);
    }

    #[test]
    fn notification_show_request_parses() {
        let json = r#"{"id":"req_1","method":"notification.show","params":{"title":"build failed","body":"api workspace","position":"top-left","sound":"request"}}"#;
        let request: Request = serde_json::from_str(json).unwrap();
        let Method::NotificationShow(params) = request.method else {
            panic!("wrong method parsed");
        };
        assert_eq!(params.title, "build failed");
        assert_eq!(params.body.as_deref(), Some("api workspace"));
        assert_eq!(params.position, Some(ToastPosition::TopLeft));
        assert_eq!(params.sound, NotificationShowSound::Request);
    }

    #[test]
    fn notification_show_sound_defaults_to_none() {
        let json =
            r#"{"id":"req_1","method":"notification.show","params":{"title":"build failed"}}"#;
        let request: Request = serde_json::from_str(json).unwrap();
        let Method::NotificationShow(params) = request.method else {
            panic!("wrong method parsed");
        };

        assert_eq!(params.sound, NotificationShowSound::None);
    }

    #[test]
    fn request_round_trips_for_pane_report_agent() {
        let request = Request {
            id: "req_hook".into(),
            method: Method::PaneReportAgent(PaneReportAgentParams {
                pane_id: "1-1".into(),
                source: "gardn:pi".into(),
                agent: "pi".into(),
                state: PaneAgentState::Working,
                message: Some("thinking".into()),
                custom_status: Some("indexing".into()),
                seq: Some(42),
                agent_session_id: Some("pi-session".into()),
                agent_session_path: Some("/tmp/pi-session.jsonl".into()),
                launch_env: std::collections::BTreeMap::new(),
            }),
        };

        let json = serde_json::to_string(&request).unwrap();
        let restored: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, request);
    }

    #[test]
    fn request_round_trips_for_pane_report_agent_session() {
        let request = Request {
            id: "req_session".into(),
            method: Method::PaneReportAgentSession(PaneReportAgentSessionParams {
                pane_id: "1-1".into(),
                source: "gardn:claude".into(),
                agent: "claude".into(),
                seq: Some(42),
                agent_session_id: Some("claude-session".into()),
                agent_session_path: None,
                session_start_source: None,
                launch_env: std::collections::BTreeMap::new(),
            }),
        };

        let json = serde_json::to_string(&request).unwrap();
        let restored: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, request);
    }

    #[test]
    fn request_round_trips_for_pane_report_metadata() {
        let request = Request {
            id: "req_metadata".into(),
            method: Method::PaneReportMetadata(PaneReportMetadataParams {
                pane_id: "1-1".into(),
                source: "user:claude-title".into(),
                agent: Some("claude".into()),
                applies_to_source: Some("gardn:claude".into()),
                title: Some("Refactor auth".into()),
                display_agent: Some("Claude auth".into()),
                custom_status: Some("refactor auth".into()),
                state_labels: HashMap::from([("working".into(), "deep in the mines".into())]),
                tokens: HashMap::new(),
                clear_title: false,
                clear_display_agent: false,
                clear_custom_status: false,
                clear_state_labels: false,
                seq: Some(42),
                ttl_ms: Some(3_600_000),
            }),
        };

        let json = serde_json::to_string(&request).unwrap();
        let restored: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, request);
    }

    #[test]
    fn request_round_trips_for_pane_clear_agent_authority() {
        let request = Request {
            id: "req_clear".into(),
            method: Method::PaneClearAgentAuthority(PaneClearAgentAuthorityParams {
                pane_id: "1-1".into(),
                source: Some("gardn:pi".into()),
                seq: Some(42),
            }),
        };

        let json = serde_json::to_string(&request).unwrap();
        let restored: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, request);
    }

    #[test]
    fn request_round_trips_for_pane_release_agent() {
        let request = Request {
            id: "req_release".into(),
            method: Method::PaneReleaseAgent(PaneReleaseAgentParams {
                pane_id: "1-1".into(),
                source: "gardn:pi".into(),
                agent: "pi".into(),
                agent_session_id: Some("session-1".into()),
                agent_session_path: None,
                seq: Some(42),
            }),
        };

        let json = serde_json::to_string(&request).unwrap();
        let restored: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, request);
    }

    #[test]
    fn request_uses_dot_method_names() {
        let request = Request {
            id: "req_1".into(),
            method: Method::WorkspaceCreate(WorkspaceCreateParams {
                cwd: Some("/tmp".into()),
                location: None,
                focus: true,
                label: Some("api".into()),
                env: HashMap::new(),
            }),
        };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["method"], "workspace.create");
    }

    #[test]
    fn request_round_trips_for_server_stop() {
        let request = Request {
            id: "req_stop".into(),
            method: Method::ServerStop(EmptyParams::default()),
        };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["method"], "server.stop");
        let restored: Request = serde_json::from_value(json).unwrap();
        assert_eq!(restored, request);
    }

    #[test]
    fn request_round_trips_for_server_reload_config() {
        let request = Request {
            id: "req_reload".into(),
            method: Method::ServerReloadConfig(EmptyParams::default()),
        };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["method"], "server.reload_config");
        let restored: Request = serde_json::from_value(json).unwrap();
        assert_eq!(restored, request);
    }

    #[test]
    fn request_round_trips_for_server_reload_agent_manifests() {
        let request = Request {
            id: "req_reload_agent_manifests".into(),
            method: Method::ServerReloadAgentManifests(EmptyParams::default()),
        };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["method"], "server.reload_agent_manifests");
        let restored: Request = serde_json::from_value(json).unwrap();
        assert_eq!(restored, request);
    }

    #[test]
    fn request_round_trips_for_server_agent_manifests() {
        let request = Request {
            id: "req_agent_manifests".into(),
            method: Method::ServerAgentManifests(EmptyParams::default()),
        };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["method"], "server.agent_manifests");
        let restored: Request = serde_json::from_value(json).unwrap();
        assert_eq!(restored, request);
    }

    #[test]
    fn request_round_trips_for_agent_explain() {
        let request = Request {
            id: "req_agent_explain".into(),
            method: Method::AgentExplain(AgentTarget {
                target: "agent-1".into(),
            }),
        };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["method"], "agent.explain");
        let restored: Request = serde_json::from_value(json).unwrap();
        assert_eq!(restored, request);
    }

    #[test]
    fn unknown_method_is_rejected() {
        let json = r#"{"id":"req_1","method":"nope","params":{}}"#;
        let err = serde_json::from_str::<Request>(json)
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown variant"));
    }

    #[test]
    fn missing_required_params_are_rejected() {
        let json = r#"{"id":"req_1","method":"pane.send_text","params":{"pane_id":"p_1"}}"#;
        let err = serde_json::from_str::<Request>(json)
            .unwrap_err()
            .to_string();
        assert!(err.contains("text"));
    }

    #[test]
    fn pane_send_input_defaults_to_empty_text_and_keys() {
        let json = r#"
        {
            "id": "req_1",
            "method": "pane.send_input",
            "params": {
                "pane_id": "p_1"
            }
        }
        "#;

        let request: Request = serde_json::from_str(json).unwrap();
        let Method::PaneSendInput(params) = request.method else {
            panic!("wrong method parsed");
        };
        assert_eq!(params.pane_id, "p_1");
        assert!(params.text.is_empty());
        assert!(params.keys.is_empty());
    }

    #[test]
    fn pane_wait_for_output_defaults_strip_ansi_to_true() {
        let json = r#"
        {
            "id": "req_1",
            "method": "pane.wait_for_output",
            "params": {
                "pane_id": "p_1",
                "source": "recent",
                "match": { "type": "substring", "value": "ready" }
            }
        }
        "#;

        let request: Request = serde_json::from_str(json).unwrap();
        let Method::PaneWaitForOutput(params) = request.method else {
            panic!("wrong method parsed");
        };
        assert!(params.strip_ansi);
    }

    #[test]
    fn pane_read_defaults_to_text_format() {
        let json = r#"
        {
            "id": "req_1",
            "method": "pane.read",
            "params": {
                "pane_id": "p_1",
                "source": "visible"
            }
        }
        "#;

        let request: Request = serde_json::from_str(json).unwrap();
        let Method::PaneRead(params) = request.method else {
            panic!("wrong method parsed");
        };
        assert_eq!(params.format, ReadFormat::Text);
    }

    #[test]
    fn event_envelope_round_trips() {
        let event = EventEnvelope {
            event: EventKind::PaneOutputChanged,
            data: EventData::PaneOutputChanged {
                pane_id: "p_1".into(),
                workspace_id: "w_1".into(),
                revision: 42,
            },
        };

        let json = serde_json::to_string(&event).unwrap();
        let restored: EventEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, event);
    }

    #[test]
    fn subscribe_request_parses_parameterized_subscriptions() {
        let json = r#"
        {
            "id": "sub_1",
            "method": "events.subscribe",
            "params": {
                "subscriptions": [
                    {
                        "type": "pane.output_matched",
                        "pane_id": "p_1_1",
                        "source": "recent",
                        "lines": 200,
                        "match": { "type": "substring", "value": "auth: received" }
                    },
                    {
                        "type": "pane.agent_status_changed",
                        "pane_id": "p_1_1",
                        "agent_status": "done"
                    }
                ]
            }
        }
        "#;

        let request: Request = serde_json::from_str(json).unwrap();
        let Method::EventsSubscribe(params) = request.method else {
            panic!("wrong method parsed");
        };
        assert_eq!(params.subscriptions.len(), 2);
        assert!(matches!(
            &params.subscriptions[0],
            Subscription::PaneOutputMatched {
                pane_id,
                source: ReadSource::Recent,
                lines: Some(200),
                r#match: OutputMatch::Substring { value },
                strip_ansi: true,
            } if pane_id == "p_1_1" && value == "auth: received"
        ));
        assert!(matches!(
            &params.subscriptions[1],
            Subscription::PaneAgentStatusChanged {
                pane_id,
                agent_status: Some(AgentStatus::Done),
            } if pane_id == "p_1_1"
        ));
    }

    #[test]
    fn subscription_event_envelope_round_trips() {
        let event = SubscriptionEventEnvelope {
            event: SubscriptionEventKind::PaneOutputMatched,
            data: SubscriptionEventData::PaneOutputMatched(PaneOutputMatchedEvent {
                pane_id: "p_1_1".into(),
                matched_line: "auth: received".into(),
                read: PaneReadResult {
                    pane_id: "p_1_1".into(),
                    workspace_id: "w_1".into(),
                    tab_id: "t_1_1".into(),
                    source: ReadSource::Recent,
                    format: ReadFormat::Text,
                    text: "auth: received\n".into(),
                    revision: 0,
                    truncated: false,
                },
            }),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event\":\"pane.output_matched\""));
        let restored: SubscriptionEventEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, event);
    }

    #[test]
    fn scroll_changed_subscription_event_round_trips() {
        let event = SubscriptionEventEnvelope {
            event: SubscriptionEventKind::ScrollChanged,
            data: SubscriptionEventData::ScrollChanged(PaneScrollChangedEvent {
                pane_id: "p_1_1".into(),
                workspace_id: "w_1".into(),
                scroll: PaneScrollInfo {
                    offset_from_bottom: 12,
                    max_offset_from_bottom: 240,
                    viewport_rows: 30,
                },
            }),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event\":\"pane.scroll_changed\""));
        assert_eq!(
            serde_json::from_str::<SubscriptionEventEnvelope>(&json).unwrap(),
            event
        );
    }

    #[test]
    fn session_snapshot_request_and_response_round_trip() {
        let request = Request {
            id: "req_snapshot".into(),
            method: Method::SessionSnapshot(EmptyParams::default()),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"method\":\"session.snapshot\""));
        assert_eq!(serde_json::from_str::<Request>(&json).unwrap(), request);

        let response = SuccessResponse {
            id: "req_snapshot".into(),
            result: ResponseResult::SessionSnapshot {
                snapshot: Box::new(SessionSnapshot {
                    version: "0.3.2-test".into(),
                    protocol: 6,
                    focused_workspace_id: None,
                    focused_tab_id: None,
                    focused_pane_id: None,
                    workspaces: Vec::new(),
                    tabs: Vec::new(),
                    panes: Vec::new(),
                    layouts: Vec::new(),
                    agents: Vec::new(),
                }),
            },
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"type\":\"session_snapshot\""));
        assert_eq!(
            serde_json::from_str::<SuccessResponse>(&json).unwrap(),
            response
        );
    }

    #[test]
    fn success_response_round_trips() {
        let response = SuccessResponse {
            id: "req_1".into(),
            result: ResponseResult::Pong {
                version: "0.1.2".into(),
                protocol: 6,
                capabilities: Some(ServerCapabilities { live_handoff: true }),
            },
        };

        let json = serde_json::to_string(&response).unwrap();
        let restored: SuccessResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, response);
    }

    #[test]
    fn create_response_round_trips_with_root_pane() {
        let response = SuccessResponse {
            id: "req_2".into(),
            result: ResponseResult::TabCreated {
                tab: TabInfo {
                    tab_id: "w_1:2".into(),
                    workspace_id: "w_1".into(),
                    number: 2,
                    label: "review".into(),
                    focused: false,
                    pane_count: 1,
                    agent_status: AgentStatus::Unknown,
                },
                root_pane: PaneInfo {
                    pane_id: "w_1-3".into(),
                    terminal_id: "term_example".into(),
                    location: ResourceLocationParams {
                        execution_host_id: "local".into(),
                        path: "/tmp/review".into(),
                    },
                    workspace_id: "w_1".into(),
                    tab_id: "w_1:2".into(),
                    focused: false,
                    cwd: Some("/tmp/review".into()),
                    foreground_cwd: None,
                    label: None,
                    agent: None,
                    title: None,
                    display_agent: None,
                    agent_status: AgentStatus::Unknown,
                    custom_status: None,
                    state_labels: HashMap::new(),
                    tokens: HashMap::new(),
                    agent_session: None,
                    scroll: None,
                    revision: 0,
                },
            },
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"type\":\"tab_created\""));
        assert!(json.contains("\"root_pane\""));
        let restored: SuccessResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, response);
    }

    #[test]
    fn public_resource_info_exposes_host_ids_via_location_only() {
        let workspace = WorkspaceInfo {
            workspace_id: "w_1".into(),
            group_id: "g_1".into(),
            default_location: ResourceLocationParams {
                execution_host_id: "ssh:workbox:1".into(),
                path: "/srv/work".into(),
            },
            number: 1,
            label: "work".into(),
            focused: true,
            pane_count: 2,
            tab_count: 1,
            active_tab_id: "w_1:1".into(),
            agent_status: AgentStatus::Unknown,
        };
        let group = GroupInfo {
            group_id: "g_1".into(),
            number: 1,
            name: "Work".into(),
            icon: "☀".into(),
            focused: true,
            workspace_count: 1,
            default_location: Some(workspace.default_location.clone()),
        };
        let pane = PaneInfo {
            pane_id: "w_1-3".into(),
            terminal_id: "term_example".into(),
            location: ResourceLocationParams {
                execution_host_id: "ssh:workbox:1".into(),
                path: "/srv/work".into(),
            },
            workspace_id: "w_1".into(),
            tab_id: "w_1:1".into(),
            focused: false,
            cwd: Some("/srv/work".into()),
            foreground_cwd: None,
            label: None,
            agent: None,
            title: None,
            display_agent: None,
            agent_status: AgentStatus::Unknown,
            custom_status: None,
            state_labels: HashMap::new(),
            tokens: HashMap::new(),
            agent_session: None,
            scroll: None,
            revision: 0,
        };

        let workspace_json = serde_json::to_value(workspace).expect("workspace serializes");
        let group_json = serde_json::to_value(group).expect("group serializes");
        let pane_json = serde_json::to_value(pane).expect("pane serializes");

        assert!(workspace_json.get("default_execution_host_id").is_none());
        assert_eq!(
            workspace_json["default_location"]["execution_host_id"],
            "ssh:workbox:1"
        );
        assert_eq!(workspace_json["default_location"]["path"], "/srv/work");
        assert_eq!(
            group_json["default_location"]["execution_host_id"],
            "ssh:workbox:1"
        );
        assert!(pane_json.get("execution_host_id").is_none());
        assert_eq!(pane_json["location"]["execution_host_id"], "ssh:workbox:1");
    }

    #[test]
    fn error_response_round_trips() {
        let response = ErrorResponse {
            id: "req_1".into(),
            error: ErrorBody {
                code: "pane_not_found".into(),
                message: "pane p_1 not found".into(),
            },
        };

        let json = serde_json::to_string(&response).unwrap();
        let restored: ErrorResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, response);
    }

    #[test]
    fn event_wait_parses_typed_match() {
        let json = r#"
        {
            "id": "req_9",
            "method": "events.wait",
            "params": {
                "match_event": {
                    "event": "pane_agent_status_changed",
                    "pane_id": "p_1",
                    "agent_status": "done"
                },
                "timeout_ms": 30000
            }
        }
        "#;

        let request: Request = serde_json::from_str(json).unwrap();
        let Method::EventsWait(params) = request.method else {
            panic!("wrong method parsed");
        };
        assert_eq!(
            params.match_event,
            EventMatch::PaneAgentStatusChanged {
                pane_id: "p_1".into(),
                agent_status: AgentStatus::Done,
            }
        );
    }

    #[test]
    fn plugin_request_and_response_round_trip() {
        let request = Request {
            id: "plugin_link".into(),
            method: Method::PluginLink(PluginLinkParams {
                path: "/plugins/workspace-bootstrap".into(),
                enabled: true,
                source: None,
            }),
        };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["method"], "plugin.link");
        assert_eq!(json["params"]["path"], "/plugins/workspace-bootstrap");
        let restored: Request = serde_json::from_value(json).unwrap();
        assert_eq!(restored, request);

        let plugin = InstalledPluginInfo {
            plugin_id: "example.workspace-bootstrap".into(),
            name: "Workspace Bootstrap".into(),
            version: "1.0.0".into(),
            min_gardn_version: "0.1.0".into(),
            manifest_dialect: PluginManifestDialect::HerdrV1,
            description: Some("Create useful workspaces".into()),
            manifest_path: "/plugins/workspace-bootstrap/gardn-plugin.toml".into(),
            plugin_root: "/plugins/workspace-bootstrap".into(),
            enabled: true,
            platforms: None,
            build: Vec::new(),
            startup: Vec::new(),
            actions: vec![PluginManifestAction {
                id: "bootstrap".into(),
                title: "Bootstrap".into(),
                description: None,
                contexts: vec![PluginActionContext::Workspace],
                platforms: None,
                command: vec!["gardn".into(), "workspace".into(), "create".into()],
            }],
            events: Vec::new(),
            panes: Vec::new(),
            link_handlers: Vec::new(),
            source: PluginSourceInfo::default(),
            warnings: Vec::new(),
        };
        let response = SuccessResponse {
            id: "plugin_link".into(),
            result: ResponseResult::PluginLinked { plugin },
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"type\":\"plugin_linked\""));
        let restored: SuccessResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, response);
    }
    #[test]
    fn popup_pane_request_round_trips_dimensions() {
        let request = PluginPaneOpenParams {
            plugin_id: "example.popup".into(),
            entrypoint: "status".into(),
            placement: Some(PluginPanePlacement::Popup),
            width: Some(PopupSize::Percent(80)),
            height: Some(PopupSize::Cells(12)),
            workspace_id: None,
            target_pane_id: None,
            direction: None,
            cwd: None,
            focus: true,
            env: HashMap::new(),
        };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["placement"], "popup");
        assert_eq!(json["width"], "80%");
        assert_eq!(json["height"], 12);
        assert_eq!(
            serde_json::from_value::<PluginPaneOpenParams>(json).unwrap(),
            request
        );
    }
}
