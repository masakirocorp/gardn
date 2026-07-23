use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::api;
use crate::api::client::{ApiClient, ApiClientError};
use crate::api::schema::{
    AgentPromptParams, AgentReadParams, AgentRenameParams, AgentSendKeysParams, AgentStartParams,
    AgentStatus, AgentTarget, ClientWindowTitleSetParams, EmptyParams, GroupCreateParams,
    GroupRenameParams, GroupTarget, IntegrationTarget, Method, NotificationShowParams,
    NotificationShowSound, OutputMatch, PaneAgentState, PaneTarget, PaneWaitForOutputParams,
    PingParams, ReadFormat, ReadSource, Request, ResponseResult, ServerLiveHandoffParams,
    SplitDirection, Subscription,
};

#[path = "cli/api.rs"]
mod api_cli;
mod pane;
mod plugin;
mod protocol_guard;
mod tab;
mod workspace;

pub(crate) fn parse_env_assignment(raw: &str) -> Result<(String, String), String> {
    let Some((key, value)) = raw.split_once('=') else {
        return Err("env must use KEY=VALUE".into());
    };
    if key.is_empty() {
        return Err("env key must not be empty".into());
    }
    if key.contains('\0') || value.contains('\0') {
        return Err("env must not contain NUL bytes".into());
    }
    Ok((key.to_string(), value.to_string()))
}
pub enum CommandOutcome {
    Handled(i32),
    NotCli,
}

pub fn maybe_run(args: &[String]) -> std::io::Result<CommandOutcome> {
    let Some(command) = args.get(1).map(|arg| arg.as_str()) else {
        return Ok(CommandOutcome::NotCli);
    };

    let exit_code = match command {
        "server" => {
            let Some(exit_code) = run_server_command(&args[2..])? else {
                return Ok(CommandOutcome::NotCli);
            };
            exit_code
        }
        "api" => api_cli::run_api_command(&args[2..])?,
        "status" => run_status_command(&args[2..])?,
        "group" => run_group_command(&args[2..])?,
        "config" => run_config_command(&args[2..])?,
        "workspace" => workspace::run_workspace_command(&args[2..])?,
        "notification" => run_notification_command(&args[2..])?,
        "tab" => tab::run_tab_command(&args[2..])?,
        "agent" => run_agent_command(&args[2..])?,
        "terminal" => run_terminal_command(&args[2..])?,
        "pane" => pane::run_pane_command(&args[2..])?,
        "plugin" => plugin::run_plugin_command(&args[2..])?,
        "wait" => run_wait_command(&args[2..])?,
        "integration" => run_integration_command(&args[2..])?,
        "session" => run_session_command(&args[2..])?,
        _ => return Ok(CommandOutcome::NotCli),
    };

    Ok(CommandOutcome::Handled(exit_code))
}

fn run_server_command(args: &[String]) -> std::io::Result<Option<i32>> {
    let Some(subcommand) = args.first().map(|arg| arg.as_str()) else {
        return Ok(None);
    };

    match subcommand {
        "stop" => server_stop(&args[1..]).map(Some),
        "live-handoff" => server_live_handoff(&args[1..]).map(Some),
        "--handoff-import" => Ok(None),
        "reload-config" => server_reload_config(&args[1..]).map(Some),
        "agent-manifests" => server_agent_manifests(&args[1..]).map(Some),
        "reload-agent-manifests" => server_reload_agent_manifests(&args[1..]).map(Some),
        "help" | "--help" | "-h" => {
            print_server_help();
            Ok(Some(0))
        }
        _ => {
            print_server_help();
            Ok(Some(2))
        }
    }
}

fn run_status_command(args: &[String]) -> std::io::Result<i32> {
    match args.first().map(|arg| arg.as_str()) {
        None => print_full_status(StatusFormat::Text),
        Some("--json") if args.len() == 1 => print_full_status(StatusFormat::Json),
        Some("server") => {
            let Ok(format) = parse_status_format(&args[1..], "usage: omh status server [--json]")
            else {
                return Ok(2);
            };
            print_server_status(format)
        }
        Some("client") => {
            let Ok(format) = parse_status_format(&args[1..], "usage: omh status client [--json]")
            else {
                return Ok(2);
            };
            print_client_status(format);
            Ok(0)
        }
        Some("help" | "--help" | "-h") => {
            print_status_help();
            Ok(0)
        }
        Some(_) => {
            print_status_help();
            Ok(2)
        }
    }
}
fn run_config_command(args: &[String]) -> std::io::Result<i32> {
    let Some(subcommand) = args.first().map(|arg| arg.as_str()) else {
        print_config_help();
        return Ok(2);
    };

    match subcommand {
        "check" => config_check(&args[1..]),
        "reset-keys" => config_reset_keys(&args[1..]),
        "help" | "--help" | "-h" => {
            print_config_help();
            Ok(0)
        }
        _ => {
            print_config_help();
            Ok(2)
        }
    }
}

fn config_check(args: &[String]) -> std::io::Result<i32> {
    match args {
        [] => {}
        [flag] if matches!(flag.as_str(), "help" | "--help" | "-h") => {
            eprintln!("usage: omh config check");
            return Ok(0);
        }
        _ => {
            eprintln!("usage: omh config check");
            return Ok(2);
        }
    }

    let diagnostics = crate::config::Config::load().diagnostics;
    if diagnostics.is_empty() {
        println!("config: ok");
    } else {
        println!("config: issues found");
        for diagnostic in &diagnostics {
            println!("{diagnostic}");
        }
    }

    Ok(i32::from(!diagnostics.is_empty()))
}

fn config_reset_keys(args: &[String]) -> std::io::Result<i32> {
    if !args.is_empty() {
        eprintln!("usage: omh config reset-keys");
        return Ok(2);
    }

    let path = crate::config::config_path();
    if !path.exists() {
        println!(
            "No config file found at {}. Built-in v2 keybindings already apply.",
            path.display()
        );
        return Ok(0);
    }

    let content = std::fs::read_to_string(&path)?;
    let table = match content.parse::<toml::Table>() {
        Ok(table) => table,
        Err(err) => {
            eprintln!(
                "config file at {} is invalid TOML: {err}. Fix it manually or move it aside to use defaults.",
                path.display()
            );
            return Ok(1);
        }
    };

    if !table.contains_key("keys") {
        println!(
            "No [keys] config found in {}. Built-in v2 keybindings already apply.",
            path.display()
        );
        return Ok(0);
    }

    let (updated, removed) = crate::config::remove_keybinding_config_sections(&content);
    if !removed {
        eprintln!(
            "could not safely remove keybinding config from {} without rewriting comments; edit the file manually or remove the top-level keys setting.",
            path.display()
        );
        return Ok(1);
    }
    if let Err(err) = updated.parse::<toml::Table>() {
        eprintln!(
            "removing keybinding config would make {} invalid TOML: {err}; leaving config unchanged",
            path.display()
        );
        return Ok(1);
    }

    let backup_path = key_config_backup_path(&path);
    std::fs::copy(&path, &backup_path)?;
    std::fs::write(&path, updated)?;

    println!("Created backup: {}", backup_path.display());
    println!(
        "Removed [keys], [keys.indexed], and [[keys.command]] from {}.",
        path.display()
    );
    println!("Built-in v2 keybindings will apply after Oh My Herdr restarts or reloads config.");
    println!(
        "If an Oh My Herdr server is running, run `omh server reload-config` to apply this now."
    );
    println!(
        "To restore: cp {} {}",
        backup_path.display(),
        path.display()
    );
    Ok(0)
}

fn key_config_backup_path(path: &std::path::Path) -> std::path::PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config.toml");
    path.with_file_name(format!("{file_name}.bak-keybind-v2-{timestamp}"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ServerRuntimeStatus {
    Running {
        version: Option<String>,
        protocol: Option<u32>,
        capabilities: Option<crate::api::schema::ServerCapabilities>,
    },
    NotRunning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatusFormat {
    Text,
    Json,
}

fn parse_status_format(args: &[String], usage: &str) -> Result<StatusFormat, i32> {
    match args {
        [] => Ok(StatusFormat::Text),
        [flag] if flag == "--json" => Ok(StatusFormat::Json),
        _ => {
            eprintln!("{usage}");
            Err(2)
        }
    }
}

fn print_full_status(format: StatusFormat) -> std::io::Result<i32> {
    let server = read_server_runtime_status()?;

    match format {
        StatusFormat::Text => {
            println!("client:");
            println!("  version: {}", crate::build_info::version());
            println!("  protocol: {}", crate::protocol::PROTOCOL_VERSION);
            println!();
            println!("server:");
            print_server_status_body(&server, "  ");
            println!();
            println!("update:");
            println!("  restart_needed: {}", restart_needed_label(&server));
        }
        StatusFormat::Json => {
            println!(
                "{}",
                serde_json::json!({
                    "client": client_status_json(),
                    "server": server_status_json(&server),
                    "update": {
                        "restart_needed": restart_needed(&server),
                    },
                })
            );
        }
    }

    Ok(0)
}

fn print_server_status(format: StatusFormat) -> std::io::Result<i32> {
    let server = read_server_runtime_status()?;
    match format {
        StatusFormat::Text => print_server_status_body(&server, ""),
        StatusFormat::Json => println!("{}", server_status_json(&server)),
    }
    Ok(0)
}

fn print_client_status(format: StatusFormat) {
    match format {
        StatusFormat::Text => {
            println!("version: {}", crate::build_info::version());
            println!("protocol: {}", crate::protocol::PROTOCOL_VERSION);
            println!("binary: {}", current_exe_label());
        }
        StatusFormat::Json => println!("{}", client_status_json()),
    }
}

fn print_server_status_body(server: &ServerRuntimeStatus, indent: &str) {
    match server {
        ServerRuntimeStatus::Running {
            version, protocol, ..
        } => {
            println!("{indent}status: running");
            println!("{indent}version: {}", option_label(version.as_deref()));
            println!("{indent}protocol: {}", protocol_label(*protocol));
            println!("{indent}compatible: {}", compatibility_label(*protocol));
            println!("{indent}socket: {}", api::socket_path().display());
        }
        ServerRuntimeStatus::NotRunning => {
            println!("{indent}status: not running");
            println!("{indent}socket: {}", api::socket_path().display());
        }
    }
}

fn server_status_json(server: &ServerRuntimeStatus) -> serde_json::Value {
    match server {
        ServerRuntimeStatus::Running {
            version,
            protocol,
            capabilities,
        } => {
            serde_json::json!({
                "status": "running",
                "running": true,
                "version": version,
                "protocol": protocol,
                "compatible": compatible_protocol(*protocol),
                "capabilities": capabilities.as_ref().map(|capabilities| serde_json::json!({
                    "live_handoff": capabilities.live_handoff,
                })),
                "restart_needed": restart_needed(server),
                "socket": api::socket_path().display().to_string(),
            })
        }
        ServerRuntimeStatus::NotRunning => {
            serde_json::json!({
                "status": "not_running",
                "running": false,
                "restart_needed": false,
                "socket": api::socket_path().display().to_string(),
            })
        }
    }
}

fn client_status_json() -> serde_json::Value {
    serde_json::json!({
        "version": crate::build_info::version(),
        "protocol": crate::protocol::PROTOCOL_VERSION,
        "binary": current_exe_label(),
    })
}

fn read_server_runtime_status() -> std::io::Result<ServerRuntimeStatus> {
    match send_request_unchecked(&Request {
        id: "cli:status:server".into(),
        method: Method::Ping(PingParams::default()),
    }) {
        Ok(response) => {
            if response.get("error").is_some() {
                return Err(std::io::Error::other(format!(
                    "server status request failed: {}",
                    response
                )));
            }

            let result = &response["result"];
            Ok(ServerRuntimeStatus::Running {
                version: result
                    .get("version")
                    .and_then(|value| value.as_str())
                    .map(str::to_owned),
                protocol: result
                    .get("protocol")
                    .and_then(|value| value.as_u64())
                    .and_then(|value| u32::try_from(value).ok()),
                capabilities: serde_json::from_value(result["capabilities"].clone()).ok(),
            })
        }
        Err(err) if server_not_running_error(&err) => Ok(ServerRuntimeStatus::NotRunning),
        Err(err) => Err(err),
    }
}

fn server_not_running_error(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
    )
}

fn option_label(value: Option<&str>) -> &str {
    value.unwrap_or("unknown")
}

fn protocol_label(protocol: Option<u32>) -> String {
    protocol
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn compatibility_label(protocol: Option<u32>) -> &'static str {
    match protocol {
        Some(protocol) if protocol == crate::protocol::PROTOCOL_VERSION => "yes",
        Some(_) => "no",
        None => "unknown",
    }
}

fn compatible_protocol(protocol: Option<u32>) -> bool {
    protocol == Some(crate::protocol::PROTOCOL_VERSION)
}

fn restart_needed(server: &ServerRuntimeStatus) -> bool {
    match server {
        ServerRuntimeStatus::Running { version, .. } => {
            version.as_deref() != Some(crate::build_info::version().as_str())
        }
        ServerRuntimeStatus::NotRunning => false,
    }
}

fn restart_needed_label(server: &ServerRuntimeStatus) -> &'static str {
    match server {
        ServerRuntimeStatus::Running { version, .. } => match version.as_deref() {
            Some(version) if version == crate::build_info::version() => "no",
            Some(_) => "yes",
            None => "unknown",
        },
        ServerRuntimeStatus::NotRunning => "no",
    }
}

fn current_exe_label() -> String {
    std::env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|err| format!("unknown ({err})"))
}

fn run_notification_command(args: &[String]) -> std::io::Result<i32> {
    let Some(subcommand) = args.first().map(|arg| arg.as_str()) else {
        print_notification_help();
        return Ok(2);
    };

    match subcommand {
        "show" => notification_show(&args[1..]),
        "help" | "--help" | "-h" => {
            print_notification_help();
            Ok(0)
        }
        _ => {
            print_notification_help();
            Ok(2)
        }
    }
}

fn print_notification_help() {
    eprintln!(
        "usage: omh notification show <title> [--body TEXT] [--position top-left|top-right|bottom-left|bottom-right] [--sound none|done|request]"
    );
}

fn notification_show(args: &[String]) -> std::io::Result<i32> {
    let Some(title) = args.first() else {
        print_notification_help();
        return Ok(2);
    };

    let mut body = None;
    let mut position = None;
    let mut sound = NotificationShowSound::None;
    let mut idx = 1;
    while idx < args.len() {
        match args[idx].as_str() {
            "--body" => {
                let Some(value) = args.get(idx + 1) else {
                    print_notification_help();
                    return Ok(2);
                };
                body = Some(value.clone());
                idx += 2;
            }
            "--position" => {
                let Some(value) = args.get(idx + 1) else {
                    print_notification_help();
                    return Ok(2);
                };
                position = Some(parse_omh_toast_position(value)?);
                idx += 2;
            }
            "--sound" => {
                let Some(value) = args.get(idx + 1) else {
                    print_notification_help();
                    return Ok(2);
                };
                sound = parse_notification_sound(value)?;
                idx += 2;
            }
            _ => {
                print_notification_help();
                return Ok(2);
            }
        }
    }

    let response = send_request(&Request {
        id: "cli:notification:show".into(),
        method: Method::NotificationShow(NotificationShowParams {
            title: title.clone(),
            body,
            position,
            sound,
        }),
    })?;
    print_response(&response)
}

fn parse_omh_toast_position(value: &str) -> std::io::Result<crate::config::ToastOmhPosition> {
    match value {
        "top-left" => Ok(crate::config::ToastOmhPosition::TopLeft),
        "top-right" => Ok(crate::config::ToastOmhPosition::TopRight),
        "bottom-left" => Ok(crate::config::ToastOmhPosition::BottomLeft),
        "bottom-right" => Ok(crate::config::ToastOmhPosition::BottomRight),
        _ => Err(std::io::Error::other(
            "invalid notification position: expected top-left, top-right, bottom-left, or bottom-right",
        )),
    }
}

fn parse_notification_sound(value: &str) -> std::io::Result<NotificationShowSound> {
    match value {
        "none" => Ok(NotificationShowSound::None),
        "done" => Ok(NotificationShowSound::Done),
        "request" => Ok(NotificationShowSound::Request),
        _ => Err(std::io::Error::other(
            "invalid notification sound: expected none, done, or request",
        )),
    }
}

fn run_group_command(args: &[String]) -> std::io::Result<i32> {
    let Some(subcommand) = args.first().map(|arg| arg.as_str()) else {
        print_group_help();
        return Ok(2);
    };

    match subcommand {
        "list" => group_list(&args[1..]),
        "create" => group_create(&args[1..]),
        "focus" | "switch" => group_focus(&args[1..]),
        "rename" => group_rename(&args[1..]),
        "delete" => group_delete(&args[1..]),
        "help" | "--help" | "-h" => {
            print_group_help();
            Ok(0)
        }
        _ => {
            print_group_help();
            Ok(2)
        }
    }
}

fn run_agent_command(args: &[String]) -> std::io::Result<i32> {
    let Some(subcommand) = args.first().map(|arg| arg.as_str()) else {
        print_agent_help();
        return Ok(2);
    };

    match subcommand {
        "list" => agent_list(&args[1..]),
        "get" => agent_get(&args[1..]),
        "read" => agent_read(&args[1..]),
        "prompt" => agent_prompt(&args[1..]),
        "send-keys" => agent_send_keys(&args[1..]),
        "rename" => agent_rename(&args[1..]),
        "focus" => agent_focus(&args[1..]),
        "wait" => agent_wait(&args[1..]),
        "attach" => agent_attach(&args[1..]),
        "start" => agent_start(&args[1..]),
        "explain" => agent_explain(&args[1..]),
        "help" | "--help" | "-h" => {
            print_agent_help();
            Ok(0)
        }
        _ => {
            print_agent_help();
            Ok(2)
        }
    }
}
fn agent_subcommand_help(args: &[String], usage: &str) -> Option<i32> {
    match args.first().map(String::as_str) {
        Some("help" | "--help" | "-h") => {
            eprintln!("{usage}");
            Some(0)
        }
        _ => None,
    }
}

fn agent_explain(args: &[String]) -> std::io::Result<i32> {
    let mut file = None;
    let mut agent = None;
    let mut json = false;
    let mut target = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--file" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --file");
                    return Ok(2);
                };
                file = Some(value.clone());
                index += 2;
            }
            "--agent" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --agent");
                    return Ok(2);
                };
                agent = Some(value.clone());
                index += 2;
            }
            "--json" => {
                json = true;
                index += 1;
            }
            "--format" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --format");
                    return Ok(2);
                };
                match value.as_str() {
                    "json" => json = true,
                    "text" => json = false,
                    other => {
                        eprintln!("invalid --format: {other} (expected text or json)");
                        return Ok(2);
                    }
                }
                index += 2;
            }
            "help" | "--help" | "-h" => {
                eprintln!("usage: omh agent explain <target> [--json]");
                eprintln!("usage: omh agent explain --file PATH --agent LABEL [--json]");
                return Ok(0);
            }
            value if value.starts_with('-') => {
                eprintln!("unknown option: {value}");
                return Ok(2);
            }
            value => {
                if target.is_some() {
                    eprintln!("usage: omh agent explain <target> [--json]");
                    return Ok(2);
                }
                target = Some(value.to_string());
                index += 1;
            }
        }
    }

    let explain = if let Some(path) = file {
        if target.is_some() {
            eprintln!("usage: omh agent explain --file PATH --agent LABEL [--json]");
            return Ok(2);
        }
        let Some(agent_label) = agent else {
            eprintln!("omh agent explain --file requires --agent LABEL");
            return Ok(2);
        };
        let content = std::fs::read_to_string(path)?;
        crate::detect::manifest::explain_to_json_value(&crate::detect::manifest::explain_for_label(
            &agent_label,
            &content,
        ))
    } else {
        let Some(target) = target else {
            eprintln!("usage: omh agent explain <target> [--json]");
            eprintln!("usage: omh agent explain --file PATH --agent LABEL [--json]");
            return Ok(2);
        };
        if agent.is_some() {
            eprintln!("--agent is only valid with --file");
            return Ok(2);
        }

        let response = send_request(&Request {
            id: "cli:agent:explain".into(),
            method: Method::AgentExplain(AgentTarget {
                target: target.to_owned(),
            }),
        })?;
        if response.get("error").is_some() {
            eprintln!("{}", serde_json::to_string(&response).unwrap());
            return Ok(1);
        }
        response["result"]["explain"].clone()
    };

    if json {
        println!("{explain}");
    } else {
        print_agent_explain_text(&explain);
    }
    Ok(0)
}

fn print_agent_explain_text(explain: &serde_json::Value) {
    println!("agent: {}", explain["agent"].as_str().unwrap_or("unknown"));
    println!("state: {}", explain["state"].as_str().unwrap_or("unknown"));
    println!(
        "screen_detection_skipped: {}",
        explain["screen_detection_skipped"]
            .as_bool()
            .unwrap_or(false)
    );
    if let Some(reason) = explain["screen_detection_skip_reason"].as_str() {
        println!("screen_detection_skip_reason: {reason}");
    }
    println!(
        "manifest: {}",
        explain["manifest_source"].as_str().unwrap_or("none")
    );
    println!(
        "manifest_version: {}",
        explain["manifest_version"].as_str().unwrap_or("unknown")
    );
    println!(
        "cached_remote_version: {}",
        explain["cached_remote_version"].as_str().unwrap_or("none")
    );
    println!(
        "local_override_shadowing_remote: {}",
        explain["local_override_shadowing_remote"]
            .as_bool()
            .unwrap_or(false)
    );
    if let Some(status) = explain["remote_update_status"].as_str() {
        println!("remote_update_status: {status}");
    }
    if let Some(error) = explain["remote_update_error"].as_str() {
        println!("remote_update_error: {error}");
    }
    if let Some(rule) = explain["matched_rule"].as_object() {
        println!(
            "matched_rule: {} priority={} region={} state={}",
            rule.get("id")
                .and_then(|value| value.as_str())
                .unwrap_or("-"),
            rule.get("priority")
                .and_then(|value| value.as_i64())
                .unwrap_or(0),
            rule.get("region")
                .and_then(|value| value.as_str())
                .unwrap_or("-"),
            rule.get("state")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown")
        );
    } else {
        println!("matched_rule: none");
    }
    println!(
        "visible: idle={} blocker={} working={}",
        explain["visible_idle"].as_bool().unwrap_or(false),
        explain["visible_blocker"].as_bool().unwrap_or(false),
        explain["visible_working"].as_bool().unwrap_or(false)
    );
    if let Some(reason) = explain["fallback_reason"].as_str() {
        println!("fallback_reason: {reason}");
    }
    if let Some(reason) = explain["skipped_update_reason"].as_str() {
        println!("skipped_update_reason: {reason}");
    }
    if let Some(warning) = explain["warning"].as_str() {
        println!("warning: {warning}");
    }
    if let Some(evaluated_rules) = explain["evaluated_rules"]
        .as_array()
        .filter(|rules| !rules.is_empty())
    {
        println!("evaluated_rules:");
        for rule in evaluated_rules {
            println!(
                "  {} matched={} priority={} region={} state={}",
                rule["id"].as_str().unwrap_or("-"),
                rule["matched"].as_bool().unwrap_or(false),
                rule["priority"].as_i64().unwrap_or(0),
                rule["region"].as_str().unwrap_or("-"),
                rule["state"].as_str().unwrap_or("unknown")
            );
            let evidence = &rule["evidence"];
            println!(
                "    matchers: contains={:?} regex={:?} line_regex={:?} all={} any={} not={}",
                evidence["contains"],
                evidence["regex"],
                evidence["line_regex"],
                evidence["all_count"].as_u64().unwrap_or(0),
                evidence["any_count"].as_u64().unwrap_or(0),
                evidence["not_count"].as_u64().unwrap_or(0)
            );
            println!(
                "    region: bytes={} preview={:?}",
                evidence["region_bytes"].as_u64().unwrap_or(0),
                evidence["region_preview"].as_str().unwrap_or("")
            );
        }
    }
}

fn run_terminal_command(args: &[String]) -> std::io::Result<i32> {
    let Some(subcommand) = args.first().map(|arg| arg.as_str()) else {
        print_terminal_help();
        return Ok(2);
    };

    match subcommand {
        "attach" => terminal_attach(&args[1..]),
        "title" => terminal_title(&args[1..]),
        "help" | "--help" | "-h" => {
            print_terminal_help();
            Ok(0)
        }
        _ => {
            print_terminal_help();
            Ok(2)
        }
    }
}

fn run_wait_command(args: &[String]) -> std::io::Result<i32> {
    let Some(subcommand) = args.first().map(|arg| arg.as_str()) else {
        print_wait_help();
        return Ok(2);
    };

    match subcommand {
        "output" => wait_output(&args[1..]),
        "agent-status" => wait_agent_status(&args[1..]),
        "help" | "--help" | "-h" => {
            print_wait_help();
            Ok(0)
        }
        _ => {
            print_wait_help();
            Ok(2)
        }
    }
}

fn run_session_command(args: &[String]) -> std::io::Result<i32> {
    let Some(subcommand) = args.first().map(|arg| arg.as_str()) else {
        print_session_help();
        return Ok(2);
    };

    match subcommand {
        "list" => session_list(&args[1..]),
        "attach" => session_attach_help(&args[1..]),
        "stop" => session_stop(&args[1..]),
        "delete" => session_delete(&args[1..]),
        "help" | "--help" | "-h" => {
            print_session_help();
            Ok(0)
        }
        _ => {
            print_session_help();
            Ok(2)
        }
    }
}

fn server_stop(args: &[String]) -> std::io::Result<i32> {
    if !args.is_empty() {
        eprintln!("usage: omh server stop");
        return Ok(2);
    }

    match crate::session::stop_active_server() {
        Ok(()) => Ok(0),
        Err(err) => {
            eprintln!("{err}");
            Ok(1)
        }
    }
}

fn server_reload_config(args: &[String]) -> std::io::Result<i32> {
    if !args.is_empty() {
        eprintln!("usage: omh server reload-config");
        return Ok(2);
    }

    print_response(&send_request(&Request {
        id: "cli:server:reload-config".into(),
        method: Method::ServerReloadConfig(EmptyParams::default()),
    })?)
}

fn server_agent_manifests(args: &[String]) -> std::io::Result<i32> {
    if !args.is_empty() {
        eprintln!("usage: omh server agent-manifests");
        return Ok(2);
    }

    print_response(&send_request(&Request {
        id: "cli:server:agent-manifests".into(),
        method: Method::ServerAgentManifests(EmptyParams::default()),
    })?)
}

fn server_reload_agent_manifests(args: &[String]) -> std::io::Result<i32> {
    if !args.is_empty() {
        eprintln!("usage: omh server reload-agent-manifests");
        return Ok(2);
    }

    print_response(&send_request(&Request {
        id: "cli:server:reload-agent-manifests".into(),
        method: Method::ServerReloadAgentManifests(EmptyParams::default()),
    })?)
}

fn server_live_handoff(args: &[String]) -> std::io::Result<i32> {
    let Some(params) = parse_live_handoff_params(args) else {
        eprintln!(
            "usage: omh server live-handoff [--import-exe <path>] [--expected-protocol <n>] [--expected-version <version>]"
        );
        return Ok(2);
    };

    let response = send_request_unchecked(&Request {
        id: "cli:server:live-handoff".into(),
        method: Method::ServerLiveHandoff(params),
    })?;
    if response.get("error").is_some() {
        let rendered = serde_json::to_string(&response).unwrap_or_else(|err| {
            format!(
                "{{\"error\":{{\"code\":\"render_failed\",\"message\":\"failed to render error response: {err}\"}}}}"
            )
        });
        eprintln!("{rendered}");
        return Ok(1);
    }

    eprintln!(
        "live handoff complete; server log: {}",
        crate::session::data_dir().join("omh-server.log").display()
    );
    Ok(0)
}

fn parse_live_handoff_params(args: &[String]) -> Option<ServerLiveHandoffParams> {
    let mut params = ServerLiveHandoffParams::default();
    let mut idx = 0;
    while idx < args.len() {
        let arg = &args[idx];
        let (flag, value) = if let Some((flag, value)) = arg.split_once('=') {
            (flag, Some(value.to_string()))
        } else {
            let value = args.get(idx + 1).cloned();
            idx += 1;
            (arg.as_str(), value)
        };
        let value = value?;
        match flag {
            "--import-exe" => params.import_exe = Some(value),
            "--expected-protocol" => {
                params.expected_protocol = Some(value.parse().ok()?);
            }
            "--expected-version" => params.expected_version = Some(value),
            _ => return None,
        }
        idx += 1;
    }
    Some(params)
}
fn session_attach_help(args: &[String]) -> std::io::Result<i32> {
    if matches!(
        args.first().map(String::as_str),
        Some("help" | "--help" | "-h")
    ) {
        eprintln!("usage: omh session attach <name>");
        return Ok(0);
    }
    eprintln!("usage: omh session attach <name>");
    Ok(2)
}

fn session_list(args: &[String]) -> std::io::Result<i32> {
    let json = match parse_session_json_only(args, "usage: omh session list [--json]") {
        Ok(json) => json,
        Err(code) => return Ok(code),
    };

    let sessions = crate::session::list_sessions()?;
    if json {
        _print_json(&serde_json::json!({
            "sessions": sessions,
        }));
    } else {
        print_session_table(&sessions);
    }
    Ok(0)
}

fn session_stop(args: &[String]) -> std::io::Result<i32> {
    let (name, json) =
        match parse_session_name_and_json(args, "usage: omh session stop <name> [--json]") {
            Ok(parsed) => parsed,
            Err(code) => return Ok(code),
        };

    let target = match crate::session::parse_target_name(&name) {
        Ok(target) => target,
        Err(message) => {
            print_session_error("invalid_session_name", &message);
            return Ok(1);
        }
    };
    match crate::session::stop_session(target.as_deref()) {
        Ok(session) => {
            if json {
                _print_json(&serde_json::json!({
                    "stopped": true,
                    "session": session,
                }));
            } else {
                println!("stopped session {}", session.name);
            }
            Ok(0)
        }
        Err(message) => {
            print_session_error("session_stop_failed", &message);
            Ok(1)
        }
    }
}

fn session_delete(args: &[String]) -> std::io::Result<i32> {
    let (name, json) =
        match parse_session_name_and_json(args, "usage: omh session delete <name> [--json]") {
            Ok(parsed) => parsed,
            Err(code) => return Ok(code),
        };

    match crate::session::delete_session(&name) {
        Ok(session) => {
            if json {
                _print_json(&serde_json::json!({
                    "deleted": true,
                    "session": session,
                }));
            } else {
                println!("deleted session {}", session.name);
            }
            Ok(0)
        }
        Err(message) => {
            print_session_error("session_delete_failed", &message);
            Ok(1)
        }
    }
}

fn group_list(args: &[String]) -> std::io::Result<i32> {
    if !args.is_empty() {
        eprintln!("usage: omh group list");
        return Ok(2);
    }

    print_response(&send_request(&Request {
        id: "cli:group:list".into(),
        method: Method::GroupList(EmptyParams::default()),
    })?)
}

fn group_create(args: &[String]) -> std::io::Result<i32> {
    if args.is_empty() {
        eprintln!("usage: omh group create <name>");
        return Ok(2);
    }

    print_response(&send_request(&Request {
        id: "cli:group:create".into(),
        method: Method::GroupCreate(GroupCreateParams {
            name: args.join(" "),
        }),
    })?)
}

fn group_focus(args: &[String]) -> std::io::Result<i32> {
    let Some(raw_group_id) = args.first() else {
        eprintln!("usage: omh group focus <group_id>");
        return Ok(2);
    };
    if args.len() != 1 {
        eprintln!("usage: omh group focus <group_id>");
        return Ok(2);
    }

    print_response(&send_request(&Request {
        id: "cli:group:focus".into(),
        method: Method::GroupFocus(GroupTarget {
            group_id: normalize_group_id(raw_group_id),
        }),
    })?)
}

fn group_rename(args: &[String]) -> std::io::Result<i32> {
    if args.len() < 2 {
        eprintln!("usage: omh group rename <group_id> <name>");
        return Ok(2);
    }

    print_response(&send_request(&Request {
        id: "cli:group:rename".into(),
        method: Method::GroupRename(GroupRenameParams {
            group_id: normalize_group_id(&args[0]),
            name: args[1..].join(" "),
        }),
    })?)
}

fn group_delete(args: &[String]) -> std::io::Result<i32> {
    let Some(raw_group_id) = args.first() else {
        eprintln!("usage: omh group delete <group_id>");
        return Ok(2);
    };
    if args.len() != 1 {
        eprintln!("usage: omh group delete <group_id>");
        return Ok(2);
    }

    print_response(&send_request(&Request {
        id: "cli:group:delete".into(),
        method: Method::GroupDelete(GroupTarget {
            group_id: normalize_group_id(raw_group_id),
        }),
    })?)
}

fn agent_start(args: &[String]) -> std::io::Result<i32> {
    if let Some(code) = agent_subcommand_help(
        args,
        "usage: omh agent start <name> [--cwd PATH] [--workspace ID] [--tab ID] [--split right|down] [--focus|--no-focus] -- <argv...>",
    ) {
        return Ok(code);
    }

    let Some(name) = args.first() else {
        eprintln!("usage: omh agent start <name> [--cwd PATH] [--workspace ID] [--tab ID] [--split right|down] [--focus|--no-focus] -- <argv...>");
        return Ok(2);
    };

    let Some(separator) = args.iter().position(|arg| arg == "--") else {
        eprintln!("usage: omh agent start <name> [--cwd PATH] [--workspace ID] [--tab ID] [--split right|down] [--focus|--no-focus] -- <argv...>");
        return Ok(2);
    };
    if separator == args.len() - 1 {
        eprintln!("agent start requires argv after --");
        return Ok(2);
    }

    let mut cwd = None;
    let mut workspace_id = None;
    let mut tab_id = None;
    let mut split = None;
    let mut focus = false;

    let mut index = 1;
    while index < separator {
        match args[index].as_str() {
            "--cwd" => {
                let Some(value) = args.get(index + 1).filter(|_| index + 1 < separator) else {
                    eprintln!("missing value for --cwd");
                    return Ok(2);
                };
                cwd = Some(value.clone());
                index += 2;
            }
            "--workspace" => {
                let Some(value) = args.get(index + 1).filter(|_| index + 1 < separator) else {
                    eprintln!("missing value for --workspace");
                    return Ok(2);
                };
                workspace_id = Some(normalize_workspace_id(value));
                index += 2;
            }
            "--tab" => {
                let Some(value) = args.get(index + 1).filter(|_| index + 1 < separator) else {
                    eprintln!("missing value for --tab");
                    return Ok(2);
                };
                tab_id = Some(normalize_tab_id(value));
                index += 2;
            }
            "--split" => {
                let Some(value) = args.get(index + 1).filter(|_| index + 1 < separator) else {
                    eprintln!("missing value for --split");
                    return Ok(2);
                };
                split = Some(parse_split_direction(value)?);
                index += 2;
            }
            "--focus" => {
                focus = true;
                index += 1;
            }
            "--no-focus" => {
                focus = false;
                index += 1;
            }
            other => {
                eprintln!("unknown option: {other}");
                return Ok(2);
            }
        }
    }

    print_response(&send_request(&Request {
        id: "cli:agent:start".into(),
        method: Method::AgentStart(AgentStartParams {
            name: name.clone(),
            cwd,
            workspace_id,
            tab_id,
            split,
            focus,
            env: Default::default(),
            argv: args[separator + 1..].to_vec(),
        }),
    })?)
}

fn agent_list(args: &[String]) -> std::io::Result<i32> {
    if let Some(code) = agent_subcommand_help(args, "usage: omh agent list") {
        return Ok(code);
    }

    if !args.is_empty() {
        eprintln!("usage: omh agent list");
        return Ok(2);
    }

    print_response(&send_request(&Request {
        id: "cli:agent:list".into(),
        method: Method::AgentList(EmptyParams::default()),
    })?)
}

fn agent_get(args: &[String]) -> std::io::Result<i32> {
    if let Some(code) = agent_subcommand_help(args, "usage: omh agent get <target>") {
        return Ok(code);
    }

    let Some(target) = args.first() else {
        eprintln!("usage: omh agent get <target>");
        return Ok(2);
    };
    if args.len() != 1 {
        eprintln!("usage: omh agent get <target>");
        return Ok(2);
    }

    print_response(&send_request(&Request {
        id: "cli:agent:get".into(),
        method: Method::AgentGet(AgentTarget {
            target: target.clone(),
        }),
    })?)
}

fn agent_focus(args: &[String]) -> std::io::Result<i32> {
    if let Some(code) = agent_subcommand_help(args, "usage: omh agent focus <target>") {
        return Ok(code);
    }

    let Some(target) = args.first() else {
        eprintln!("usage: omh agent focus <target>");
        return Ok(2);
    };
    if args.len() != 1 {
        eprintln!("usage: omh agent focus <target>");
        return Ok(2);
    }

    print_response(&send_request(&Request {
        id: "cli:agent:focus".into(),
        method: Method::AgentFocus(AgentTarget {
            target: target.clone(),
        }),
    })?)
}

fn agent_attach(args: &[String]) -> std::io::Result<i32> {
    if let Some(code) = agent_subcommand_help(args, "usage: omh agent attach <target> [--takeover]")
    {
        return Ok(code);
    }

    let (target, takeover) =
        match parse_attach_target(args, "usage: omh agent attach <target> [--takeover]") {
            Ok(parsed) => parsed,
            Err(code) => return Ok(code),
        };

    let response = resolve_agent_target(&target, "cli:agent:attach:resolve")?;
    if response.get("error").is_some() {
        eprintln!("{}", serde_json::to_string(&response).unwrap());
        return Ok(1);
    }
    let Some(terminal_id) = response["result"]["agent"]["terminal_id"].as_str() else {
        eprintln!("agent attach failed: response did not include terminal_id");
        return Ok(1);
    };
    crate::client::run_terminal_attach(terminal_id.to_owned(), takeover)?;
    Ok(0)
}

fn agent_wait(args: &[String]) -> std::io::Result<i32> {
    if let Some(code) = agent_subcommand_help(
        args,
        "usage: omh agent wait <target> --status <idle|working|blocked|unknown> [--timeout MS]",
    ) {
        return Ok(code);
    }

    let Some(target) = args.first() else {
        eprintln!(
            "usage: omh agent wait <target> --status <idle|working|blocked|unknown> [--timeout MS]"
        );
        return Ok(2);
    };

    let mut timeout_ms = None;
    let mut desired_status = None;

    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--status" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --status");
                    return Ok(2);
                };
                desired_status = Some(parse_agent_wait_status(value)?);
                index += 2;
            }
            "--timeout" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --timeout");
                    return Ok(2);
                };
                timeout_ms = Some(parse_u64_flag("--timeout", value)?);
                index += 2;
            }
            "help" | "--help" | "-h" => {
                eprintln!("usage: omh agent wait <target> --status <idle|working|blocked|unknown> [--timeout MS]");
                return Ok(0);
            }
            other => {
                eprintln!("unknown option: {other}");
                return Ok(2);
            }
        }
    }

    let Some(agent_status) = desired_status else {
        eprintln!("missing required --status");
        return Ok(2);
    };

    print_response(&send_request(&Request {
        id: "cli:agent:wait".into(),
        method: Method::AgentWait(crate::api::schema::AgentWaitParams {
            target: target.clone(),
            until: vec![agent_status],
            timeout_ms,
        }),
    })?)
}

fn resolve_agent_target(target: &str, request_id: &str) -> std::io::Result<serde_json::Value> {
    send_request(&Request {
        id: request_id.into(),
        method: Method::AgentGet(AgentTarget {
            target: target.to_owned(),
        }),
    })
}
fn terminal_attach(args: &[String]) -> std::io::Result<i32> {
    let (terminal_id, takeover) = match parse_attach_target(
        args,
        "usage: omh terminal attach <terminal_id> [--takeover]",
    ) {
        Ok(parsed) => parsed,
        Err(code) => return Ok(code),
    };
    crate::client::run_terminal_attach(terminal_id, takeover)?;
    Ok(0)
}

fn terminal_title(args: &[String]) -> std::io::Result<i32> {
    match args.first().map(|arg| arg.as_str()) {
        Some("set") => {
            if args.len() != 2 {
                eprintln!("usage: omh terminal title set <title>");
                return Ok(2);
            }
            print_response(&send_request(&Request {
                id: "cli:terminal:title:set".into(),
                method: Method::ClientWindowTitleSet(ClientWindowTitleSetParams {
                    title: args[1].clone(),
                }),
            })?)
        }
        Some("clear") => {
            if args.len() != 1 {
                eprintln!("usage: omh terminal title clear");
                return Ok(2);
            }
            print_response(&send_request(&Request {
                id: "cli:terminal:title:clear".into(),
                method: Method::ClientWindowTitleClear(EmptyParams::default()),
            })?)
        }
        Some("help" | "--help" | "-h") => {
            eprintln!("usage: omh terminal title set <title>");
            eprintln!("       omh terminal title clear");
            Ok(0)
        }
        _ => {
            eprintln!("usage: omh terminal title set <title>");
            eprintln!("       omh terminal title clear");
            Ok(2)
        }
    }
}

pub(super) fn parse_attach_target(args: &[String], usage: &str) -> Result<(String, bool), i32> {
    let Some(target) = args.first() else {
        eprintln!("{usage}");
        return Err(2);
    };
    let mut takeover = false;
    for arg in &args[1..] {
        match arg.as_str() {
            "--takeover" => takeover = true,
            "help" | "--help" | "-h" => {
                eprintln!("{usage}");
                return Err(0);
            }
            other => {
                eprintln!("unknown option: {other}");
                return Err(2);
            }
        }
    }
    Ok((target.clone(), takeover))
}

fn agent_rename(args: &[String]) -> std::io::Result<i32> {
    if let Some(code) =
        agent_subcommand_help(args, "usage: omh agent rename <target> <name>|--clear")
    {
        return Ok(code);
    }

    let Some(target) = args.first() else {
        eprintln!("usage: omh agent rename <target> <name>|--clear");
        return Ok(2);
    };
    if args.len() < 2 {
        eprintln!("usage: omh agent rename <target> <name>|--clear");
        return Ok(2);
    }
    let name = if args.len() == 2 && args[1] == "--clear" {
        None
    } else {
        Some(args[1..].join(" "))
    };

    print_response(&send_request(&Request {
        id: "cli:agent:rename".into(),
        method: Method::AgentRename(AgentRenameParams {
            target: target.clone(),
            name,
        }),
    })?)
}
fn agent_prompt(args: &[String]) -> std::io::Result<i32> {
    if let Some(code) = agent_subcommand_help(
        args,
        "usage: omh agent prompt <target> <text> [--wait-for STATUS] [--timeout MS]",
    ) {
        return Ok(code);
    }

    let Some(target) = args.first() else {
        eprintln!("usage: omh agent prompt <target> <text> [--wait-for STATUS] [--timeout MS]");
        return Ok(2);
    };
    let mut text = Vec::new();
    let mut wait = None;
    let mut timeout_ms = None;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--wait-for" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("--wait-for requires STATUS");
                    return Ok(2);
                };
                wait = Some(parse_agent_wait_status(value)?);
                index += 2;
            }
            "--timeout" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("--timeout requires MS");
                    return Ok(2);
                };
                timeout_ms = Some(parse_u64_flag("--timeout", value)?);
                index += 2;
            }
            value => {
                text.push(value.to_string());
                index += 1;
            }
        }
    }
    if text.is_empty() {
        eprintln!("usage: omh agent prompt <target> <text> [--wait-for STATUS] [--timeout MS]");
        return Ok(2);
    }
    print_response(&send_request(&Request {
        id: "cli:agent:prompt".into(),
        method: Method::AgentPrompt(AgentPromptParams {
            target: target.clone(),
            text: text.join(" "),
            wait: wait.map(|status| crate::api::schema::AgentPromptWaitOptions {
                until: vec![status],
                timeout_ms,
            }),
        }),
    })?)
}

fn agent_send_keys(args: &[String]) -> std::io::Result<i32> {
    if let Some(code) = agent_subcommand_help(args, "usage: omh agent send-keys <target> <key>...")
    {
        return Ok(code);
    }

    let Some(target) = args.first() else {
        eprintln!("usage: omh agent send-keys <target> <key>...");
        return Ok(2);
    };
    if args.len() < 2 {
        eprintln!("usage: omh agent send-keys <target> <key>...");
        return Ok(2);
    }
    print_response(&send_request(&Request {
        id: "cli:agent:send-keys".into(),
        method: Method::AgentSendKeys(AgentSendKeysParams {
            target: target.clone(),
            keys: args[1..].to_vec(),
        }),
    })?)
}

fn agent_read(args: &[String]) -> std::io::Result<i32> {
    if let Some(code) = agent_subcommand_help(
        args,
        "usage: omh agent read <target> [--source visible|recent|recent-unwrapped] [--lines N] [--format text|ansi] [--ansi]",
    ) {
        return Ok(code);
    }

    let Some(target) = args.first() else {
        eprintln!("usage: omh agent read <target> [--source visible|recent|recent-unwrapped] [--lines N] [--format text|ansi] [--ansi]");
        return Ok(2);
    };

    let mut source = ReadSource::Recent;
    let mut lines = None;
    let mut format = ReadFormat::Text;
    let mut strip_ansi = true;

    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--source" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --source");
                    return Ok(2);
                };
                source = parse_read_source(value)?;
                index += 2;
            }
            "--lines" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --lines");
                    return Ok(2);
                };
                lines = Some(parse_u32_flag("--lines", value)?);
                index += 2;
            }
            "--format" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --format");
                    return Ok(2);
                };
                format = parse_read_format(value)?;
                strip_ansi = !matches!(format, ReadFormat::Ansi);
                index += 2;
            }
            "--ansi" => {
                format = ReadFormat::Ansi;
                strip_ansi = false;
                index += 1;
            }
            other => {
                eprintln!("unknown option: {other}");
                return Ok(2);
            }
        }
    }

    print_response(&send_request(&Request {
        id: "cli:agent:read".into(),
        method: Method::AgentRead(AgentReadParams {
            target: target.clone(),
            source,
            lines,
            format,
            strip_ansi,
        }),
    })?)
}

fn run_integration_command(args: &[String]) -> std::io::Result<i32> {
    match args.first().map(|arg| arg.as_str()) {
        Some("status") => integration_status(&args[1..]),
        Some("install") => integration_install(&args[1..]),
        Some("uninstall") => integration_uninstall(&args[1..]),
        Some("help" | "--help" | "-h") => {
            print_integration_help();
            Ok(0)
        }
        _ => {
            print_integration_help();
            Ok(2)
        }
    }
}
fn integration_status(args: &[String]) -> std::io::Result<i32> {
    let outdated_only = match args {
        [] => false,
        [flag] if flag == "--outdated-only" => true,
        _ => {
            eprintln!("usage: omh integration status [--outdated-only]");
            return Ok(2);
        }
    };

    if outdated_only {
        crate::integration::print_outdated_update_notice();
        return Ok(0);
    }

    for status in crate::integration::installed_integration_statuses() {
        let target = crate::integration::integration_target_label(status.target);
        let version = match status.installed_version {
            Some(version) => format!("v{version}"),
            None => "legacy".to_string(),
        };
        let state = match status.state {
            crate::integration::IntegrationStatusKind::NotInstalled => "not installed".to_string(),
            crate::integration::IntegrationStatusKind::Current => {
                format!("current ({version})")
            }
            crate::integration::IntegrationStatusKind::Outdated => {
                format!(
                    "outdated ({version}; expected v{})",
                    status.expected_version
                )
            }
        };
        println!("{target}: {state} ({})", status.path.display());
    }

    Ok(0)
}

fn integration_install(args: &[String]) -> std::io::Result<i32> {
    let Some(target) = parse_integration_target(args, "install")? else {
        return Ok(2);
    };

    let loaded_config = crate::config::Config::load();
    let agent_profiles = crate::agent_profiles::AgentProfileCatalog::from_config(
        &loaded_config.config.agent_profiles,
    );
    match crate::integration::install_target_for_agent_profiles(target, &agent_profiles) {
        Ok(messages) => {
            print_integration_messages(messages);
            Ok(0)
        }
        Err(err) => {
            eprintln!("{err}");
            Ok(1)
        }
    }
}

fn integration_uninstall(args: &[String]) -> std::io::Result<i32> {
    let Some(target) = parse_integration_target(args, "uninstall")? else {
        return Ok(2);
    };

    match crate::integration::uninstall_target(target) {
        Ok(messages) => {
            print_integration_messages(messages);
            Ok(0)
        }
        Err(err) => {
            eprintln!("{err}");
            Ok(1)
        }
    }
}

fn print_integration_messages(messages: Vec<String>) {
    for message in messages {
        println!("{message}");
    }
}

fn parse_integration_target(
    args: &[String],
    action: &str,
) -> std::io::Result<Option<IntegrationTarget>> {
    let Some(target) = args.first().map(|arg| arg.as_str()) else {
        eprintln!(
            "usage: omh integration {action} <pi|omp|claude|codex|devin|opencode|hermes|grok>"
        );
        return Ok(None);
    };
    if args.len() != 1 {
        eprintln!(
            "usage: omh integration {action} <pi|omp|claude|codex|devin|opencode|hermes|grok>"
        );
        return Ok(None);
    }

    let parsed = match target {
        "pi" => IntegrationTarget::Pi,
        "omp" => IntegrationTarget::Omp,
        "claude" => IntegrationTarget::Claude,
        "codex" => IntegrationTarget::Codex,
        "copilot" => IntegrationTarget::Copilot,
        "devin" => IntegrationTarget::Devin,
        "opencode" => IntegrationTarget::Opencode,
        "hermes" => IntegrationTarget::Hermes,
        "qodercli" => IntegrationTarget::Qodercli,
        "grok" => IntegrationTarget::Grok,
        _ => {
            eprintln!("unknown integration target: {target}");
            eprintln!(
                "currently supported: pi, omp, claude, codex, devin, copilot, opencode, hermes, qodercli, grok"
            );
            return Ok(None);
        }
    };

    Ok(Some(parsed))
}
fn wait_output(args: &[String]) -> std::io::Result<i32> {
    let Some(raw_pane_id) = args.first() else {
        eprintln!("usage: omh wait output <pane_id> --match <text> [--source visible|recent|recent-unwrapped] [--lines N] [--timeout MS] [--regex]");
        return Ok(2);
    };

    let pane_id = normalize_pane_id(raw_pane_id);
    let mut source = ReadSource::Recent;
    let mut lines = None;
    let mut timeout_ms = None;
    let mut strip_ansi = true;
    let mut regex = false;
    let mut match_value = None;

    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--match" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --match");
                    return Ok(2);
                };
                match_value = Some(value.clone());
                index += 2;
            }
            "--source" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --source");
                    return Ok(2);
                };
                source = parse_read_source(value)?;
                index += 2;
            }
            "--lines" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --lines");
                    return Ok(2);
                };
                lines = Some(parse_u32_flag("--lines", value)?);
                index += 2;
            }
            "--timeout" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --timeout");
                    return Ok(2);
                };
                timeout_ms = Some(parse_u64_flag("--timeout", value)?);
                index += 2;
            }
            "--regex" => {
                regex = true;
                index += 1;
            }
            "--raw" => {
                strip_ansi = false;
                index += 1;
            }
            other => {
                eprintln!("unknown option: {other}");
                return Ok(2);
            }
        }
    }

    let Some(match_value) = match_value else {
        eprintln!("missing required --match");
        return Ok(2);
    };

    let matcher = if regex {
        OutputMatch::Regex { value: match_value }
    } else {
        OutputMatch::Substring { value: match_value }
    };

    let response = send_request(&Request {
        id: "cli:wait:output".into(),
        method: Method::PaneWaitForOutput(PaneWaitForOutputParams {
            pane_id,
            source,
            lines,
            r#match: matcher,
            timeout_ms,
            strip_ansi,
        }),
    })?;

    if response.get("error").is_some() {
        eprintln!("{}", serde_json::to_string(&response).unwrap());
        return Ok(1);
    }

    println!("{}", serde_json::to_string(&response).unwrap());
    Ok(0)
}

fn wait_agent_status(args: &[String]) -> std::io::Result<i32> {
    let Some(raw_pane_id) = args.first() else {
        eprintln!("usage: omh wait agent-status <pane_id> --status <idle|working|blocked|done|unknown> [--timeout MS]");
        return Ok(2);
    };

    let pane_id = normalize_pane_id(raw_pane_id);
    let mut timeout_ms = None;
    let mut desired_status = None;

    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--status" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --status");
                    return Ok(2);
                };
                desired_status = Some(parse_agent_status(value)?);
                index += 2;
            }
            "--timeout" => {
                let Some(value) = args.get(index + 1) else {
                    eprintln!("missing value for --timeout");
                    return Ok(2);
                };
                timeout_ms = Some(parse_u64_flag("--timeout", value)?);
                index += 2;
            }
            other => {
                eprintln!("unknown option: {other}");
                return Ok(2);
            }
        }
    }

    let Some(agent_status) = desired_status else {
        eprintln!("missing required --status");
        return Ok(2);
    };

    let current = send_request(&Request {
        id: "cli:wait:agent-status:current".into(),
        method: Method::PaneGet(PaneTarget {
            pane_id: pane_id.clone(),
        }),
    })?;
    if current.get("error").is_some() {
        eprintln!("{}", serde_json::to_string(&current).unwrap());
        return Ok(1);
    }
    if current["result"]["pane"]["agent_status"]
        .as_str()
        .is_some_and(|current| agent_status_matches(agent_status, current))
    {
        let pane = &current["result"]["pane"];
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "event": "pane.agent_status_changed",
                "data": {
                    "pane_id": pane["pane_id"].clone(),
                    "workspace_id": pane["workspace_id"].clone(),
                    "agent_status": pane["agent_status"].clone(),
                    "agent": pane["agent"].clone(),
                    "title": pane["title"].clone(),
                    "display_agent": pane["display_agent"].clone(),
                    "custom_status": pane["custom_status"].clone(),
                    "state_labels": pane["state_labels"].clone(),
                    "agent_session": pane["agent_session"].clone(),
                    "cwd": pane["cwd"].clone(),
                    "foreground_cwd": pane["foreground_cwd"].clone(),
                    "revision": pane["revision"].clone()
                }
            }))
            .unwrap()
        );
        return Ok(0);
    }

    let subscriptions = if agent_status == AgentStatus::Idle {
        vec![
            Subscription::PaneAgentStatusChanged {
                pane_id: pane_id.clone(),
                agent_status: Some(AgentStatus::Idle),
            },
            Subscription::PaneAgentStatusChanged {
                pane_id,
                agent_status: Some(AgentStatus::Done),
            },
        ]
    } else {
        vec![Subscription::PaneAgentStatusChanged {
            pane_id,
            agent_status: Some(agent_status),
        }]
    };

    wait_for_agent_change(
        Request {
            id: "cli:wait:agent-status".into(),
            method: Method::EventsSubscribe(crate::api::schema::EventsSubscribeParams {
                subscriptions,
            }),
        },
        timeout_ms,
        "timed out waiting for agent status change",
    )
}

pub(super) fn wait_for_agent_change(
    request: Request,
    timeout_ms: Option<u64>,
    timeout_message: &str,
) -> std::io::Result<i32> {
    let read_timeout = timeout_ms.map(Duration::from_millis);
    let client = ApiClient::local();
    ensure_server_protocol_compatible(&client, &request.id)?;
    let (ack, stream) = client
        .subscribe_value(&request, read_timeout)
        .map_err(api_client_error_to_io)?;
    if let Err(err) = crate::api::client::parse_response_value(ack) {
        if let ApiClientError::ErrorResponse(response) = err {
            eprintln!("{}", serde_json::to_string(&response).unwrap());
            return Ok(1);
        }
        return Err(api_client_error_to_io(err));
    }

    match next_event_with_timeout(stream, timeout_ms) {
        Ok(None) => {
            eprintln!("subscription closed before event arrived");
            Ok(1)
        }
        Ok(Some(event_value)) => {
            println!("{}", serde_json::to_string(&event_value).unwrap());
            Ok(0)
        }
        Err(ApiClientError::Io(err)) if api_timeout_error(&err) => {
            eprintln!("{timeout_message}");
            Ok(1)
        }
        Err(err) => Err(api_client_error_to_io(err)),
    }
}

fn next_event_with_timeout(
    mut stream: crate::api::client::EventStream,
    timeout_ms: Option<u64>,
) -> Result<Option<crate::api::schema::SubscriptionEventEnvelope>, ApiClientError> {
    #[cfg(windows)]
    {
        let Some(timeout_ms) = timeout_ms else {
            return stream.next_event();
        };
        let timeout = Duration::from_millis(timeout_ms);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(stream.next_event());
        });
        return match rx.recv_timeout(timeout) {
            Ok(result) => result,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                Err(ApiClientError::Io(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "timed out waiting for subscription event",
                )))
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Ok(None),
        };
    }

    #[cfg(not(windows))]
    {
        let _ = timeout_ms;
        stream.next_event()
    }
}

pub(super) fn print_response(response: &serde_json::Value) -> std::io::Result<i32> {
    if response.get("error").is_some() {
        eprintln!("{}", serde_json::to_string(response).unwrap());
        return Ok(1);
    }

    println!("{}", serde_json::to_string(response).unwrap());
    Ok(0)
}

pub(super) fn send_ok_request(method: Method) -> std::io::Result<i32> {
    let response = send_request(&Request {
        id: "cli:request".into(),
        method,
    })?;

    if response.get("error").is_some() {
        eprintln!("{}", serde_json::to_string(&response).unwrap());
        return Ok(1);
    }

    Ok(0)
}

pub(super) fn send_request(request: &Request) -> std::io::Result<serde_json::Value> {
    let client = ApiClient::local();
    ensure_server_protocol_compatible(&client, &request.id)?;
    client
        .request_value(request)
        .map_err(api_client_error_to_io)
}

fn send_request_unchecked(request: &Request) -> std::io::Result<serde_json::Value> {
    ApiClient::local()
        .request_value(request)
        .map_err(api_client_error_to_io)
}

fn ensure_server_protocol_compatible(client: &ApiClient, request_id: &str) -> std::io::Result<()> {
    let ping = Request {
        id: "cli:protocol-check".into(),
        method: Method::Ping(PingParams::default()),
    };
    let response = client
        .request_value(&ping)
        .map_err(api_client_error_to_io)
        .and_then(|value| {
            crate::api::client::parse_response_value(value).map_err(api_client_error_to_io)
        })?;
    let ResponseResult::Pong {
        version, protocol, ..
    } = response.result
    else {
        return Err(std::io::Error::other(
            "server protocol check returned an unexpected response",
        ));
    };
    let Some(mismatch) = protocol_guard::mismatch_response(request_id, &version, protocol) else {
        return Ok(());
    };

    eprintln!(
        "{}",
        serde_json::to_string(&mismatch).map_err(std::io::Error::other)?
    );
    Err(protocol_guard::reported_error())
}

pub(crate) fn protocol_mismatch_was_reported(error: &std::io::Error) -> bool {
    protocol_guard::was_reported(error)
}

fn api_timeout_error(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
    )
}

fn api_client_error_to_io(err: ApiClientError) -> std::io::Error {
    match err {
        ApiClientError::Io(err) => err,
        err => std::io::Error::other(err),
    }
}

pub(super) fn normalize_workspace_id(value: &str) -> String {
    value.to_string()
}

fn normalize_group_id(value: &str) -> String {
    value.to_string()
}

fn normalize_tab_id(value: &str) -> String {
    value.to_string()
}

pub(super) fn normalize_pane_id(value: &str) -> String {
    value.to_string()
}

pub(super) fn parse_split_direction(value: &str) -> std::io::Result<SplitDirection> {
    match value {
        "right" => Ok(SplitDirection::Right),
        "down" => Ok(SplitDirection::Down),
        _ => Err(std::io::Error::other(format!(
            "invalid split direction: {value}"
        ))),
    }
}

pub(super) fn parse_read_source(value: &str) -> std::io::Result<ReadSource> {
    match value {
        "visible" => Ok(ReadSource::Visible),
        "recent" => Ok(ReadSource::Recent),
        "recent-unwrapped" | "recent_unwrapped" => Ok(ReadSource::RecentUnwrapped),
        "detection" => Ok(ReadSource::Detection),
        _ => Err(std::io::Error::other(format!(
            "invalid read source: {value}"
        ))),
    }
}

pub(super) fn parse_read_format(value: &str) -> std::io::Result<ReadFormat> {
    match value {
        "text" => Ok(ReadFormat::Text),
        "ansi" => Ok(ReadFormat::Ansi),
        _ => Err(std::io::Error::other(format!(
            "invalid read format: {value}"
        ))),
    }
}

fn parse_agent_status(value: &str) -> std::io::Result<AgentStatus> {
    match value {
        "idle" => Ok(AgentStatus::Idle),
        "working" => Ok(AgentStatus::Working),
        "blocked" => Ok(AgentStatus::Blocked),
        "done" => Ok(AgentStatus::Done),
        "unknown" => Ok(AgentStatus::Unknown),
        _ => Err(std::io::Error::other(format!(
            "invalid agent status: {value} (expected idle, working, blocked, done, or unknown)"
        ))),
    }
}

fn agent_status_matches(desired: AgentStatus, current: &str) -> bool {
    match desired {
        AgentStatus::Idle => matches!(current, "idle" | "done"),
        AgentStatus::Working => current == "working",
        AgentStatus::Blocked => current == "blocked",
        AgentStatus::Unknown => current == "unknown",
        AgentStatus::Done => current == "done",
    }
}

fn parse_agent_wait_status(value: &str) -> std::io::Result<AgentStatus> {
    match value {
        "idle" => Ok(AgentStatus::Idle),
        "working" => Ok(AgentStatus::Working),
        "blocked" => Ok(AgentStatus::Blocked),
        "unknown" => Ok(AgentStatus::Unknown),
        "done" => Err(std::io::Error::other(
            "done is a UI attention state; use idle for CLI agent completion waits",
        )),
        _ => Err(std::io::Error::other(format!(
            "invalid agent status: {value} (expected idle, working, blocked, or unknown)"
        ))),
    }
}

pub(super) fn parse_pane_agent_state(value: &str) -> std::io::Result<PaneAgentState> {
    match value {
        "idle" => Ok(PaneAgentState::Idle),
        "working" => Ok(PaneAgentState::Working),
        "blocked" => Ok(PaneAgentState::Blocked),
        "unknown" => Ok(PaneAgentState::Unknown),
        _ => Err(std::io::Error::other(format!(
            "invalid pane agent state: {value} (expected idle, working, blocked, or unknown)"
        ))),
    }
}

pub(super) fn parse_u32_flag(flag: &str, value: &str) -> std::io::Result<u32> {
    value
        .parse::<u32>()
        .map_err(|_| std::io::Error::other(format!("invalid value for {flag}: {value}")))
}

pub(super) fn parse_u64_flag(flag: &str, value: &str) -> std::io::Result<u64> {
    value
        .parse::<u64>()
        .map_err(|_| std::io::Error::other(format!("invalid value for {flag}: {value}")))
}

fn parse_session_json_only(args: &[String], usage: &str) -> Result<bool, i32> {
    match args {
        [] => Ok(false),
        [flag] if flag == "--json" => Ok(true),
        _ => {
            eprintln!("{usage}");
            Err(2)
        }
    }
}

fn parse_session_name_and_json(args: &[String], usage: &str) -> Result<(String, bool), i32> {
    let mut name = None;
    let mut json = false;
    for arg in args {
        if arg == "--json" {
            json = true;
        } else if name.is_none() {
            name = Some(arg.clone());
        } else {
            eprintln!("{usage}");
            return Err(2);
        }
    }

    let Some(name) = name else {
        eprintln!("{usage}");
        return Err(2);
    };
    Ok((name, json))
}

fn print_session_table(sessions: &[crate::session::SessionInfo]) {
    println!("{:<20} {:<8} {:<48} socket", "name", "status", "directory");
    for session in sessions {
        println!(
            "{:<20} {:<8} {:<48} {}",
            session.name,
            if session.running {
                "running"
            } else {
                "stopped"
            },
            session.session_dir,
            session.socket_path
        );
    }
}

fn print_session_error(code: &str, message: &str) {
    eprintln!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "error": {
                "code": code,
                "message": message,
            }
        }))
        .unwrap()
    );
}

fn print_server_help() {
    eprintln!("omh server commands:");
    eprintln!("  omh server                run as headless server");
    eprintln!("  omh server stop           stop the running server via the API socket");
    eprintln!("  omh server live-handoff   hand off live panes to a new local server");
    eprintln!("  omh server reload-config  reload config.toml in the running server");
    eprintln!("  omh server agent-manifests         list active agent detection manifests");
    eprintln!("  omh server reload-agent-manifests  reload local agent detection manifests");
}

fn print_status_help() {
    eprintln!("omh status commands:");
    eprintln!("  omh status [--json]                 show local client and running server status");
    eprintln!("  omh status server [--json]          show running server status");
    eprintln!("  omh status client [--json]          show local client binary status");
}
fn print_config_help() {
    eprintln!("omh config commands:");
    eprintln!("  omh config reset-keys  back up config.toml and remove custom keybindings");
    eprintln!("  omh config check  validate config.toml and print diagnostics");
}

fn print_group_help() {
    eprintln!("omh group commands:");
    eprintln!("  omh group list");
    eprintln!("  omh group create <name>");
    eprintln!("  omh group focus <group_id>");
    eprintln!("  omh group switch <group_id>");
    eprintln!("  omh group rename <group_id> <name>");
    eprintln!("  omh group delete <group_id>");
}

fn print_agent_help() {
    eprintln!("omh agent commands:");
    eprintln!("  omh agent list");
    eprintln!("  omh agent get <target>");
    eprintln!("  omh agent read <target> [--source visible|recent|recent-unwrapped] [--lines N] [--format text|ansi] [--ansi]");
    eprintln!("  omh agent prompt <target> <text> [--wait-for STATUS] [--timeout MS]");
    eprintln!("  omh agent send-keys <target> <key>...");
    eprintln!("  omh agent rename <target> <name>|--clear");
    eprintln!("  omh agent focus <target>");
    eprintln!("  omh agent wait <target> --status <idle|working|blocked|unknown> [--timeout MS]");
    eprintln!("  omh agent attach <target> [--takeover]");
    eprintln!("  omh agent start <name> [--cwd PATH] [--workspace ID] [--tab ID] [--split right|down] [--focus|--no-focus] -- <argv...>");
    eprintln!("  omh agent explain <target> [--json]");
    eprintln!("  omh agent explain --file PATH --agent LABEL [--json]");
    eprintln!("  targets accept agent terminal ids, unique agent names, detected/reported agent labels, and legacy pane ids");
    eprintln!(
        "  agent prompt appends Enter atomically; send-keys accepts encoded terminal key names"
    );
}
fn print_terminal_help() {
    eprintln!("omh terminal commands:");
    eprintln!("  omh terminal attach <terminal_id> [--takeover]");
    eprintln!("  omh terminal title set <title>");
    eprintln!("  omh terminal title clear");
    eprintln!("  detach from direct attach with ctrl+b q; send literal ctrl+b with ctrl+b ctrl+b");
}

fn print_wait_help() {
    eprintln!("omh wait commands:");
    eprintln!("  omh wait output <pane_id> --match <text> [--source visible|recent|recent-unwrapped] [--lines N] [--timeout MS] [--regex] [--raw]");
    eprintln!(
        "  omh wait agent-status <pane_id> --status <idle|working|blocked|done|unknown> [--timeout MS]"
    );
}

fn print_integration_help() {
    eprintln!("omh integration commands:");
    eprintln!("  omh integration install pi");
    eprintln!("  omh integration install omp");
    eprintln!("  omh integration install claude");
    eprintln!("  omh integration install codex");
    eprintln!("  omh integration install opencode");
    eprintln!("  omh integration install hermes");
    eprintln!("  omh integration uninstall pi");
    eprintln!("  omh integration uninstall omp");
    eprintln!("  omh integration uninstall claude");
    eprintln!("  omh integration uninstall codex");
    eprintln!("  omh integration uninstall opencode");
    eprintln!("  omh integration uninstall hermes");
    eprintln!("  omh integration status [--outdated-only]");
}
fn print_session_help() {
    eprintln!("omh session commands:");
    eprintln!("  omh session list [--json]");
    eprintln!("  omh session attach <name>");
    eprintln!("  omh session stop <name> [--json]");
    eprintln!("  omh session delete <name> [--json]");
    eprintln!("  use 'default' as <name> to target the default session for stop");
}

fn _print_json<T: Serialize>(value: &T) {
    println!("{}", serde_json::to_string(value).unwrap());
}

#[cfg(test)]
mod tests {

    #[test]
    fn parse_env_assignment_accepts_empty_values() {
        assert_eq!(
            super::parse_env_assignment("OMH_ROLE=").unwrap(),
            ("OMH_ROLE".to_string(), String::new())
        );
    }

    #[test]
    fn parse_env_assignment_requires_key_value_separator() {
        assert_eq!(
            super::parse_env_assignment("OMH_ROLE").unwrap_err(),
            "env must use KEY=VALUE"
        );
    }
}
