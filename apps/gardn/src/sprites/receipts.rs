use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use gardn_local_api::sprites::{SpriteCommand, SpriteConnection, SpriteError, SpriteRequest};
use serde_json::Value;

const RECEIPT_TIMEOUT: Duration = Duration::from_secs(90);
const RECEIPT_POLL_INTERVAL: Duration = Duration::from_millis(100);

pub(super) fn wait_for_connection_receipt(
    root: &Path,
    operation_id: &str,
    request: &SpriteRequest,
    connection: &SpriteConnection,
    canceled: &AtomicBool,
) -> Result<(SpriteConnection, &'static str), SpriteError> {
    let mode = match &request.command {
        SpriteCommand::Connect(_) => "connect",
        SpriteCommand::Start(_) => "start",
        SpriteCommand::Resume { .. } => "resume",
        SpriteCommand::Shell(_) => "shell",
        _ => {
            return Err(error(
                "invalid_connection",
                "Operation is not a connection request",
                false,
            ))
        }
    };
    if connection.attempt_id.is_empty() {
        return Err(error(
            "invalid_connection",
            "Connection plan omitted its receipt attempt identity",
            false,
        ));
    }
    let receipt = root
        .join("resources")
        .join(hash_id(&connection.sprite_id))
        .join("connections")
        .join(format!("{}.json", hash_id(operation_id)));
    let started = Instant::now();
    loop {
        if canceled.load(Ordering::Acquire) {
            return Err(error(
                "canceled",
                "Connection startup observation was canceled",
                false,
            ));
        }
        if started.elapsed() >= RECEIPT_TIMEOUT {
            return Err(error("connection_timeout", "Timed out waiting for the local Sprites connection receipt; remote session state is unknown", true));
        }
        match fs::read(&receipt) {
            Ok(bytes) => {
                let value: Value = serde_json::from_slice(&bytes).map_err(|e| {
                    error(
                        "connection_receipt_invalid",
                        format!("Invalid Sprites connection receipt: {e}"),
                        true,
                    )
                })?;
                if value.get("attempt_id").and_then(Value::as_str)
                    != Some(connection.attempt_id.as_str())
                {
                    thread::sleep(RECEIPT_POLL_INTERVAL);
                    continue;
                }
                if value.get("sprite_id").and_then(Value::as_str)
                    != Some(connection.sprite_id.as_str())
                    || value.get("operation_id").and_then(Value::as_str) != Some(operation_id)
                    || value.get("mode").and_then(Value::as_str) != Some(mode)
                {
                    return Err(error("connection_receipt_mismatch", "Sprites connection receipt did not match the requested operation, resource, and mode", false));
                }
                match value.get("status").and_then(Value::as_str) {
                    Some("starting") => {}
                    Some(status @ ("started" | "transport_open")) => {
                        let session_id = value
                            .get("session_id")
                            .and_then(Value::as_str)
                            .filter(|id| !id.is_empty());
                        if connection
                            .session_id
                            .as_deref()
                            .is_some_and(|expected| Some(expected) != session_id)
                        {
                            return Err(error(
                                "connection_receipt_mismatch",
                                "Connected session ID did not match the exact requested session",
                                false,
                            ));
                        }
                        if connection.starts_session && session_id.is_none() {
                            return Err(error(
                                "connection_receipt_incomplete",
                                "Started session receipt omitted its observed session ID",
                                true,
                            ));
                        }
                        let mut connection = connection.clone();
                        if let Some(session_id) = session_id {
                            connection.session_id = Some(session_id.to_owned());
                        }
                        return Ok((
                            connection,
                            if status == "started" {
                                "session_started"
                            } else {
                                "transport_open"
                            },
                        ));
                    }
                    Some("failed") => return Err(receipt_error(&value)),
                    _ => {
                        return Err(error(
                            "connection_receipt_invalid",
                            "Sprites connection receipt has an unknown status",
                            false,
                        ))
                    }
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(error(
                    "connection_receipt_io",
                    format!("Could not read Sprites connection receipt: {err}"),
                    true,
                ))
            }
        }
        thread::sleep(RECEIPT_POLL_INTERVAL);
    }
}

fn receipt_error(receipt: &Value) -> SpriteError {
    receipt
        .get("error")
        .cloned()
        .and_then(|value| serde_json::from_value::<SpriteError>(value).ok())
        .unwrap_or_else(|| {
            error(
                "connection_failed",
                "Sprites connection process failed before startup was confirmed",
                true,
            )
        })
}

fn hash_id(id: &str) -> String {
    super::storage::hash_id(id)
}

fn error(code: &str, message: impl Into<String>, retryable: bool) -> SpriteError {
    SpriteError {
        code: code.into(),
        message: message.into(),
        retryable,
    }
}
