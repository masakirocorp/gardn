use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::api::schema::{
    EmptyParams, Method, Request, SpriteAgent, SpriteCommand, SpriteCreateParams,
    SpriteOperationStatus, SpriteReply, SpriteRequest, SpriteSource, SpriteTarget,
};

pub(super) fn run_sprites_command(args: &[String]) -> std::io::Result<i32> {
    if matches!(
        args.first().map(String::as_str),
        None | Some("help" | "--help" | "-h")
    ) {
        if !crate::config::Config::load().config.sprites.enabled {
            eprintln!("Sprites is disabled. Enable it in Settings > Integrations > Sprites.");
            return Ok(0);
        }
        print_help();
        return Ok(0);
    }
    if args == ["capabilities"] {
        return super::print_response(&super::send_request(&Request {
            id: "cli:sprites:capabilities".into(),
            method: Method::SpritesCapabilities(EmptyParams::default()),
        })?);
    }
    let (request, wait) = match parse_request(args) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("{message}");
            return Ok(2);
        }
    };
    let deadline = wait.map(|timeout| Instant::now() + timeout);
    let request_id = request.request_id.clone();
    let api_request = Request {
        id: "cli:sprites".into(),
        method: Method::SpritesRequest(request),
    };
    let response = match deadline {
        Some(deadline) => match super::send_request_until(&api_request, deadline) {
            Ok(response) => response,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) =>
            {
                eprintln!(
                    "{}",
                    serde_json::json!({"request_id": request_id, "outcome": "unknown", "error": {"code": "wait_timeout", "message": "Submission acknowledgement timed out. Reuse this exact request ID and payload to recover the operation; do not create a new intent."}})
                );
                return Ok(4);
            }
            Err(error) => return Err(error),
        },
        None => super::send_request(&api_request)?,
    };
    let Some(deadline) = deadline else {
        return print_operation_response(&response);
    };
    let Some(SpriteReply::Operation(operation)) = parse_reply(&response) else {
        return print_operation_response(&response);
    };
    if !matches!(
        operation.status,
        SpriteOperationStatus::Queued | SpriteOperationStatus::Running
    ) {
        return print_operation_response(&response);
    }
    wait_for_operation(&operation.id, deadline)
}

fn wait_for_operation(id: &str, deadline: Instant) -> std::io::Result<i32> {
    loop {
        if Instant::now() >= deadline {
            eprintln!(
                "{}",
                serde_json::json!({
                    "operation_id": id,
                    "outcome": "pending",
                    "error": {"code": "wait_timeout", "message": "The wait expired. The operation may still be running; inspect this operation ID before retrying."}
                })
            );
            return Ok(4);
        }
        let request = Request {
            id: "cli:sprites:wait".into(),
            method: Method::SpritesRequest(SpriteRequest {
                request_id: String::new(),
                command: SpriteCommand::Operation {
                    operation_id: id.into(),
                },
                open_in_workspace: None,
                focus: false,
            }),
        };
        let response = match super::send_request_until(&request, deadline) {
            Ok(response) => response,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) =>
            {
                eprintln!(
                    "{}",
                    serde_json::json!({"operation_id": id, "outcome": "pending", "error": {"code": "wait_timeout", "message": "The local API wait expired. Inspect this operation ID before retrying."}})
                );
                return Ok(4);
            }
            Err(error) => return Err(error),
        };
        match parse_reply(&response) {
            Some(SpriteReply::Operation(operation))
                if matches!(
                    operation.status,
                    SpriteOperationStatus::Queued | SpriteOperationStatus::Running
                ) => {}
            _ => return print_operation_response(&response),
        }
        std::thread::sleep(
            Duration::from_millis(100).min(deadline.saturating_duration_since(Instant::now())),
        );
    }
}

fn parse_reply(response: &serde_json::Value) -> Option<SpriteReply> {
    serde_json::from_value(response.get("result")?.get("reply")?.clone()).ok()
}

fn print_operation_response(response: &serde_json::Value) -> std::io::Result<i32> {
    if response.get("error").is_some() {
        return super::print_response(response);
    }
    let exit = match parse_reply(response) {
        Some(SpriteReply::Operation(operation)) => match operation.status {
            SpriteOperationStatus::Failed
            | SpriteOperationStatus::Interrupted
            | SpriteOperationStatus::Canceled => 1,
            _ if matches!(
                operation.result,
                Some(crate::api::schema::SpriteResult::ApprovalRequired(_))
            ) =>
            {
                3
            }
            _ => 0,
        },
        Some(SpriteReply::Disabled) => 1,
        _ => 0,
    };
    println!("{}", serde_json::to_string(response)?);
    Ok(exit)
}

fn parse_request(args: &[String]) -> Result<(SpriteRequest, Option<Duration>), String> {
    let action = args.first().ok_or("missing Sprite action")?.as_str();
    let mut target = None;
    let mut workspace = None;
    let mut cwd = None;
    let mut host = "local".to_string();
    let mut profile = None;
    let mut agent_kind = None;
    let mut command = None;
    let mut name = None;
    let mut session = None;
    let mut checkpoint = None;
    let mut approval = None;
    let mut conversation = None;
    let mut request_id = None;
    let mut wait = None;
    let mut refresh = false;
    let mut open = false;
    let mut focus = false;
    let mut share_credentials = false;
    let mut index = 1;
    while index < args.len() {
        let arg = args[index].as_str();
        let applicable = match arg {
            "--refresh" => action == "list",
            "--open" | "--focus" => {
                matches!(action, "create" | "connect" | "start" | "resume" | "shell")
            }
            "--cwd" | "--host" => matches!(action, "create" | "preflight" | "reassociate"),
            "--workspace" => matches!(
                action,
                "create" | "preflight" | "reassociate" | "connect" | "start" | "resume" | "shell"
            ),
            "--profile" | "--agent" | "--command" | "--name" | "--share-credentials" => {
                matches!(action, "create" | "preflight")
            }
            "--conversation" => action == "resume",
            "--checkpoint" => action == "restore",
            "--session-id" => matches!(action, "connect" | "disconnect" | "stop"),
            "--approval" => matches!(
                action,
                "stop" | "pull" | "restore" | "destroy" | "forget" | "reassociate"
            ),
            _ => true,
        };
        if !applicable {
            return Err(format!("{arg} does not apply to {action}"));
        }
        match arg {
            "--json" => {}
            "--refresh" => refresh = true,
            "--open" => open = true,
            "--focus" => {
                focus = true;
                open = true;
            }
            "--share-credentials" => share_credentials = true,
            "--workspace" | "--cwd" | "--host" | "--profile" | "--agent" | "--command"
            | "--name" | "--session-id" | "--checkpoint" | "--approval" | "--conversation"
            | "--request-id" | "--wait" => {
                index += 1;
                let value = args
                    .get(index)
                    .filter(|value| !value.starts_with("--"))
                    .ok_or_else(|| format!("{arg} requires a value"))?
                    .clone();
                match arg {
                    "--workspace" => workspace = Some(value),
                    "--cwd" => cwd = Some(value),
                    "--host" => host = value,
                    "--profile" => profile = Some(value),
                    "--agent" => agent_kind = Some(value),
                    "--command" => {
                        command = Some(serde_json::from_str::<Vec<String>>(&value).map_err(
                            |error| format!("--command requires a JSON argv array: {error}"),
                        )?)
                    }
                    "--name" => name = Some(value),
                    "--session-id" => session = Some(value),
                    "--checkpoint" => checkpoint = Some(value),
                    "--approval" => approval = Some(value),
                    "--conversation" => conversation = Some(value),
                    "--request-id" => request_id = Some(value),
                    "--wait" => {
                        let seconds = value
                            .parse::<u64>()
                            .map_err(|_| "--wait requires 1 to 600 seconds")?;
                        if !(1..=600).contains(&seconds) {
                            return Err("--wait requires 1 to 600 seconds".into());
                        }
                        wait = Some(Duration::from_secs(seconds));
                    }
                    _ => unreachable!(),
                }
            }
            value if !value.starts_with('-') && target.is_none() => {
                target = Some(value.to_string())
            }
            value => return Err(format!("unexpected argument: {value}")),
        }
        index += 1;
    }
    if matches!(action, "start" | "resume" | "shell") && !open {
        return Err(format!(
            "{action} requires --workspace ID --open to launch its remote terminal"
        ));
    }
    if action == "list" && target.is_some() {
        return Err("list does not accept a positional target".into());
    }
    if open && workspace.is_none() {
        return Err("--open/--focus requires --workspace ID".into());
    }
    if action == "create" && request_id.is_none() {
        return Err(
            "create requires --request-id KEY; reuse that key when retrying this creation".into(),
        );
    }
    let open_in_workspace = if open { workspace.clone() } else { None };
    let sprite_target = || -> Result<SpriteTarget, String> {
        Ok(SpriteTarget {
            sprite_id: target
                .clone()
                .ok_or_else(|| format!("{action} requires a Sprite ID"))?,
            session_id: session.clone(),
            checkpoint_id: checkpoint.clone(),
            approval: approval.clone(),
        })
    };
    let source = || -> Result<SpriteSource, String> {
        Ok(SpriteSource {
            execution_host_id: host.clone(),
            path: cwd.clone().ok_or("--cwd PATH is required")?,
        })
    };
    let command = match action {
        "list" => SpriteCommand::List { refresh },
        "get" | "inspect" => SpriteCommand::Inspect(sprite_target()?),
        "create" | "preflight" => {
            if target.is_some() {
                return Err(format!(
                    "{action} accepts --name, not a positional Sprite ID"
                ));
            }
            if profile.is_none() && (agent_kind.is_none() || command.is_none()) {
                return Err(
                    "use --profile ID, or --agent KIND with --command '[\"program\",\"arg\"]'"
                        .into(),
                );
            }
            if profile.is_some() && (agent_kind.is_some() || command.is_some()) {
                return Err("--profile cannot be combined with --agent or --command".into());
            }
            let params = SpriteCreateParams {
                workspace_id: workspace.clone().ok_or("--workspace ID is required")?,
                source: source()?,
                agent: SpriteAgent {
                    profile_id: profile.unwrap_or_default(),
                    kind: agent_kind.unwrap_or_default(),
                    command: command.unwrap_or_default(),
                    share_credentials,
                },
                name,
            };
            if action == "create" {
                SpriteCommand::Create(params)
            } else {
                SpriteCommand::Preflight(params)
            }
        }
        "connect" => SpriteCommand::Connect(sprite_target()?),
        "disconnect" => SpriteCommand::Disconnect(sprite_target()?),
        "start" => SpriteCommand::Start(sprite_target()?),
        "resume" => SpriteCommand::Resume {
            target: sprite_target()?,
            conversation_ref: conversation.ok_or("resume requires --conversation REF")?,
        },
        "shell" => SpriteCommand::Shell(sprite_target()?),
        "stop" => SpriteCommand::Stop(sprite_target()?),
        "pull-preview" => SpriteCommand::PullPreview(sprite_target()?),
        "pull" => SpriteCommand::Pull(sprite_target()?),
        "checkpoint" => SpriteCommand::Checkpoint(sprite_target()?),
        "checkpoints" => SpriteCommand::Checkpoints(sprite_target()?),
        "restore" => SpriteCommand::Restore(sprite_target()?),
        "destroy" => SpriteCommand::Destroy(sprite_target()?),
        "forget" => SpriteCommand::Forget(sprite_target()?),
        "reassociate" => SpriteCommand::Reassociate {
            target: sprite_target()?,
            workspace_id: workspace.ok_or("reassociate requires --workspace ID")?,
            source: source()?,
        },
        "operation" => SpriteCommand::Operation {
            operation_id: target.ok_or("operation requires an operation ID")?,
        },
        "cancel" => SpriteCommand::Cancel {
            operation_id: target.ok_or("cancel requires an operation ID")?,
        },
        "retry" => SpriteCommand::Retry {
            operation_id: target.ok_or("retry requires an operation ID")?,
        },
        other => return Err(format!("unknown Sprite action: {other}")),
    };
    let request_id = request_id.unwrap_or_else(|| {
        format!(
            "cli-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        )
    });
    Ok((
        SpriteRequest {
            request_id,
            command,
            open_in_workspace,
            focus,
        },
        wait,
    ))
}

fn print_help() {
    eprintln!("gardn sprites commands (JSON output):");
    eprintln!("  capabilities | list [--refresh] | get ID");
    eprintln!("  preflight --workspace ID --cwd PATH --profile PROFILE");
    eprintln!("  create --request-id KEY --workspace ID --cwd PATH --profile PROFILE [--name NAME] [--share-credentials] [--open]");
    eprintln!("  connect ID --session-id ID [--workspace ID --open]");
    eprintln!("  start|shell ID --workspace ID --open");
    eprintln!("  resume ID --conversation REF --workspace ID --open");
    eprintln!("  disconnect ID [--session-id ID] | stop ID --session-id ID");
    eprintln!("  pull-preview|pull|checkpoint|checkpoints ID");
    eprintln!("  restore ID --checkpoint ID [--approval TOKEN]");
    eprintln!("  destroy|forget ID [--approval TOKEN]");
    eprintln!("  reassociate ID --workspace ID --cwd PATH [--host ID] [--approval TOKEN]");
    eprintln!("  operation ID | cancel ID | retry ID");
    eprintln!("Operations accept --wait SECONDS (1-600). Exit 3 means approval required; exit 4 means wait timed out, not operation failure.");
}
