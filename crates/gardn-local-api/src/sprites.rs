use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct SpritesConfig {
    pub enabled: bool,
    pub org: String,
    pub sprite_bin: String,
    pub node_bin: String,
    pub name_prefix: String,
    pub max_sprites: usize,
    pub max_concurrent_operations: usize,
    pub max_transfer_mib: usize,
}

impl Default for SpritesConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            org: String::new(),
            sprite_bin: "sprite".into(),
            node_bin: "node".into(),
            name_prefix: "gardn-".into(),
            max_sprites: 4,
            max_concurrent_operations: 2,
            max_transfer_mib: 64,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SpriteSource {
    pub execution_host_id: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SpriteAgent {
    pub profile_id: String,
    pub kind: String,
    pub command: Vec<String>,
    #[serde(default)]
    pub share_credentials: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SpriteCreateParams {
    pub workspace_id: String,
    pub source: SpriteSource,
    pub agent: SpriteAgent,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SpriteTarget {
    pub sprite_id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub checkpoint_id: Option<String>,
    #[serde(default)]
    pub approval: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "action", content = "params", rename_all = "snake_case")]
pub enum SpriteCommand {
    List {
        #[serde(default)]
        refresh: bool,
    },
    Inspect(SpriteTarget),
    Preflight(SpriteCreateParams),
    Create(SpriteCreateParams),
    Connect(SpriteTarget),
    Disconnect(SpriteTarget),
    Start(SpriteTarget),
    Resume {
        target: SpriteTarget,
        conversation_ref: String,
    },
    Shell(SpriteTarget),
    Stop(SpriteTarget),
    PullPreview(SpriteTarget),
    Pull(SpriteTarget),
    Checkpoint(SpriteTarget),
    Checkpoints(SpriteTarget),
    Restore(SpriteTarget),
    Destroy(SpriteTarget),
    Forget(SpriteTarget),
    Reassociate {
        target: SpriteTarget,
        workspace_id: String,
        source: SpriteSource,
    },
    Operation {
        operation_id: String,
    },
    Cancel {
        operation_id: String,
    },
    Retry {
        operation_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SpriteRequest {
    #[serde(default)]
    pub request_id: String,
    #[serde(flatten)]
    pub command: SpriteCommand,
    #[serde(default)]
    pub open_in_workspace: Option<String>,
    #[serde(default)]
    pub focus: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SpriteSession {
    pub id: String,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub tty: bool,
    #[serde(default)]
    pub owned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SpriteRecord {
    pub id: String,
    pub org: String,
    pub name: String,
    pub managed: bool,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub source: Option<SpriteSource>,
    #[serde(default)]
    pub agent: Option<SpriteAgent>,
    pub phase: String,
    #[serde(default)]
    pub provider_state: Option<String>,
    #[serde(default)]
    pub sessions: Vec<SpriteSession>,
    #[serde(default)]
    pub checkpoint_id: Option<String>,
    pub revision: u64,
    pub updated_unix_ms: u64,
    #[serde(default)]
    pub observed_unix_ms: Option<u64>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub unpulled_changes: Option<bool>,
    #[serde(default)]
    pub attached_panes: Vec<String>,
    #[serde(default)]
    pub agent_status: Option<crate::AgentStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SpriteError {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SpriteApproval {
    pub token: String,
    pub sprite_id: String,
    pub action: String,
    pub revision: u64,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SpriteConnection {
    pub sprite_id: String,
    pub session_id: Option<String>,
    pub attempt_id: String,
    pub program: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub remote_cwd: String,
    pub agent_kind: Option<String>,
    pub starts_session: bool,
    #[serde(default)]
    pub pane_id: Option<String>,
    #[serde(default)]
    pub tab_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SpriteTransferPreview {
    pub files: usize,
    pub bytes: u64,
    pub excluded: Vec<String>,
    pub changed_paths: Vec<String>,
    pub conflicts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum SpriteResult {
    Inventory(Vec<SpriteRecord>),
    Resource(Box<SpriteRecord>),
    Connection(SpriteConnection),
    Transfer(SpriteTransferPreview),
    Checkpoints(Vec<String>),
    ApprovalRequired(SpriteApproval),
    Completed { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SpriteOperationStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Interrupted,
    Canceled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SpriteOperation {
    pub id: String,
    pub request: SpriteRequest,
    pub status: SpriteOperationStatus,
    pub stage: String,
    pub created_unix_ms: u64,
    pub updated_unix_ms: u64,
    #[serde(default)]
    pub result: Option<SpriteResult>,
    #[serde(default)]
    pub error: Option<SpriteError>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SpriteSnapshot {
    pub enabled: bool,
    pub resources: Vec<SpriteRecord>,
    pub operations: Vec<SpriteOperation>,
    pub observation_error: Option<SpriteError>,
    pub observed_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum SpriteReply {
    Snapshot(SpriteSnapshot),
    Operation(Box<SpriteOperation>),
    Disabled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprites_connect_wire_request_has_flat_target_fields() {
        let request = crate::Request {
            id: "connect".into(),
            method: crate::Method::SpritesRequest(SpriteRequest {
                request_id: "attach-once".into(),
                command: SpriteCommand::Connect(SpriteTarget {
                    sprite_id: "org/name".into(),
                    session_id: Some("session-7".into()),
                    checkpoint_id: None,
                    approval: None,
                }),
                open_in_workspace: Some("workspace-1".into()),
                focus: false,
            }),
        };
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            serde_json::json!({
                "id": "connect", "method": "sprites.request",
                "params": {
                    "request_id": "attach-once", "action": "connect",
                    "params": {"sprite_id": "org/name", "session_id": "session-7", "checkpoint_id": null, "approval": null},
                    "open_in_workspace": "workspace-1", "focus": false
                }
            })
        );
    }

    #[test]
    fn sprites_resume_wire_request_keeps_conversation_outside_target() {
        let request = SpriteRequest {
            request_id: "resume-once".into(),
            command: SpriteCommand::Resume {
                target: SpriteTarget {
                    sprite_id: "org/name".into(),
                    session_id: None,
                    checkpoint_id: None,
                    approval: None,
                },
                conversation_ref: "conversation-9".into(),
            },
            open_in_workspace: Some("workspace-1".into()),
            focus: false,
        };
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            serde_json::json!({
                "request_id": "resume-once", "action": "resume",
                "params": {
                    "target": {"sprite_id": "org/name", "session_id": null, "checkpoint_id": null, "approval": null},
                    "conversation_ref": "conversation-9"
                },
                "open_in_workspace": "workspace-1", "focus": false
            })
        );
    }
}
