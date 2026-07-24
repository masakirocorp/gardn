pub mod client;
mod event_hub;
pub mod schema;
mod server;
mod status;
mod subscriptions;
mod wait;

pub use event_hub::EventHub;
pub use server::{
    default_server_capabilities, start_server, start_server_with_capabilities, ServerHandle,
};
pub use status::{read_runtime_status_at, RuntimeStatus};

use std::path::PathBuf;

use tokio::sync::mpsc;

use crate::api::schema::{Method, Request};

pub const SOCKET_PATH_ENV_VAR: &str = "OMH_SOCKET_PATH";

pub(crate) fn request_changes_ui(request: &Request) -> bool {
    matches!(
        &request.method,
        Method::ServerReloadConfig(_)
            | Method::ServerReloadAgentManifests(_)
            | Method::NotificationShow(_)
            | Method::GroupCreate(_)
            | Method::GroupFocus(_)
            | Method::GroupRename(_)
            | Method::GroupDelete(_)
            | Method::WorkspaceCreate(_)
            | Method::WorkspaceFocus(_)
            | Method::WorkspaceRename(_)
            | Method::WorkspaceClose(_)
            | Method::WorkspaceMoveToGroup(_)
            | Method::TabCreate(_)
            | Method::TabFocus(_)
            | Method::TabRename(_)
            | Method::TabClose(_)
            | Method::LayoutApply(_)
            | Method::AgentRename(_)
            | Method::AgentViewSet(_)
            | Method::AgentViewClear(_)
            | Method::AgentFocus(_)
            | Method::AgentStart(_)
            | Method::PaneSplit(_)
            | Method::PaneSwap(_)
            | Method::PaneRename(_)
            | Method::PaneReportAgent(_)
            | Method::PaneReportAgentSession(_)
            | Method::PaneReportMetadata(_)
            | Method::PaneClearAgentAuthority(_)
            | Method::PaneReleaseAgent(_)
            | Method::PaneClose(_)
            | Method::PluginActionInvoke(_)
            | Method::PluginPaneOpen(_)
            | Method::PluginPaneFocus(_)
            | Method::PluginPaneClose(_)
    )
}

pub struct ApiRequestMessage {
    pub request: Request,
    pub respond_to: std::sync::mpsc::Sender<String>,
    pub response_written: Option<std::sync::mpsc::Receiver<()>>,
}

/// How the app loop should finish an API request.
///
/// Most methods respond immediately. Remote resource creation may defer the
/// JSON response until worker ACK/failure. `Deferred` carries the pending
/// metadata only — dispatch attaches the real `respond_to` exactly once when
/// storing the transaction.
#[derive(Debug)]
pub enum ApiRequestDisposition {
    Respond(String),
    Deferred(DeferredRemoteCreate),
    /// Background connection.install preview and optional confirm install.
    ///
    /// Callers must spawn work with `respond_to`. No SSH I/O runs while building
    /// this disposition — preview/install happen on the worker thread so the
    /// app/headless event loop can service owner-scoped auth prompts.
    DeferredConnectionInstall {
        request_id: String,
        profile_id: String,
        profile: crate::api::schema::ConnectionProfileInfo,
        /// When false, only build and return the install preview.
        confirm: bool,
        authentication_owner: crate::execution_host::auth::AuthenticationOwner,
    },
}

/// Kind of remote create whose API response is deferred until worker ACK/failure.
#[derive(Debug, Clone)]
pub enum DeferredRemoteCreateKind {
    WorkspaceCreate { label: Option<String> },
    TabCreate { label: Option<String> },
    PaneSplit,
    AgentStart { argv: Vec<String> },
}

/// Exact deferred-focus marker installed by a focused remote create.
/// Failure/cancel cleanup compares this value before clearing so newer markers survive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingFocusMarker {
    Workspace {
        workspace_id: String,
    },
    Tab {
        workspace_id: String,
        tab_idx: usize,
    },
    Pane {
        workspace_id: String,
        tab_number: usize,
        pane_id: crate::layout::PaneId,
    },
}

/// Metadata for a deferred remote-create API transaction.
///
/// Handlers return this without a responder. Outer dispatch inserts
/// `PendingRemoteApiResponse` once with the real `respond_to`.
#[derive(Debug)]
pub struct DeferredRemoteCreate {
    pub terminal_id: crate::terminal::TerminalId,
    pub request_id: String,
    pub kind: DeferredRemoteCreateKind,
    /// Whether the requester asked to focus the created resource after ACK.
    pub focus: bool,
    /// Originating client view when routed through a view-aware invocation.
    /// `None` means ambient/default (shared) create — focus applies to AppState.
    pub client_view_id: Option<u64>,
    /// Exact pending focus marker installed for this create when focus=true.
    /// Used to clear only this marker on failure/cancel without touching replacements.
    pub pending_focus: Option<PendingFocusMarker>,
}

pub type ApiRequestSender = mpsc::UnboundedSender<ApiRequestMessage>;

pub fn socket_path() -> PathBuf {
    crate::session::active_api_socket_path()
}
