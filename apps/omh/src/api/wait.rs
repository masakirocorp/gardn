use crate::ipc::LocalStream;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use regex::Regex;

use crate::api::schema::{
    ErrorBody, ErrorResponse, Method, Request, ResponseResult, SuccessResponse,
};
use crate::api::server::{
    dispatch_to_app_with_timeout, should_stop_connection, APP_RESPONSE_TIMEOUT,
    CONNECTION_POLL_INTERVAL,
};
use crate::api::subscriptions::{match_output, output_match_read_source};
use crate::api::ApiRequestSender;
const AGENT_PROMPT_EFFECT_TIMEOUT_MS: u64 = 5_000;

pub(super) fn wait_for_output(
    request_id: String,
    params: crate::api::schema::PaneWaitForOutputParams,
    stream: &mut LocalStream,
    api_tx: &ApiRequestSender,
    running: &Arc<AtomicBool>,
) -> std::io::Result<Option<String>> {
    crate::logging::api_wait_started(&request_id, &params.pane_id, params.timeout_ms);
    let deadline = params
        .timeout_ms
        .map(|ms| std::time::Instant::now() + std::time::Duration::from_millis(ms));

    let regex = match &params.r#match {
        crate::api::schema::OutputMatch::Regex { value } => match Regex::new(value) {
            Ok(regex) => Some(regex),
            Err(err) => {
                return Ok(Some(
                    serde_json::to_string(&ErrorResponse {
                        id: request_id,
                        error: ErrorBody {
                            code: "invalid_regex".into(),
                            message: err.to_string(),
                        },
                    })
                    .unwrap(),
                ));
            }
        },
        crate::api::schema::OutputMatch::Substring { .. } => None,
    };

    loop {
        if should_stop_connection(stream, running)? {
            crate::logging::api_wait_completed(&request_id, &params.pane_id, "client_disconnected");
            return Ok(None);
        }

        let read_request = Request {
            id: format!("{request_id}:read"),
            method: Method::PaneRead(crate::api::schema::PaneReadParams {
                pane_id: params.pane_id.clone(),
                source: output_match_read_source(&params.source),
                lines: params.lines,
                format: crate::api::schema::ReadFormat::Text,
                strip_ansi: params.strip_ansi,
            }),
        };
        let response =
            dispatch_to_app_with_timeout(read_request, api_tx, Some(APP_RESPONSE_TIMEOUT));
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&response) else {
            return Ok(Some(response));
        };
        if value.get("error").is_some() {
            let mut value = value;
            value["id"] = serde_json::Value::String(request_id.clone());
            return Ok(Some(serde_json::to_string(&value).unwrap()));
        }

        let read_value = value["result"]["read"].clone();
        let Ok(read) = serde_json::from_value::<crate::api::schema::PaneReadResult>(read_value)
        else {
            return Ok(Some(
                serde_json::to_string(&ErrorResponse {
                    id: request_id,
                    error: ErrorBody {
                        code: "internal_error".into(),
                        message: "failed to decode pane read result".into(),
                    },
                })
                .unwrap(),
            ));
        };

        let matched_line = match_output(&read.text, &params.r#match, regex.as_ref());
        if matched_line.is_some() {
            let revision = read.revision;
            crate::logging::api_wait_completed(&request_id, &params.pane_id, "matched");
            return Ok(Some(
                serde_json::to_string(&SuccessResponse {
                    id: request_id,
                    result: ResponseResult::OutputMatched {
                        pane_id: params.pane_id,
                        revision,
                        matched_line,
                        read,
                    },
                })
                .unwrap(),
            ));
        }

        if deadline.is_some_and(|deadline| std::time::Instant::now() >= deadline) {
            crate::logging::api_wait_timed_out(&request_id, &params.pane_id);
            return Ok(Some(
                serde_json::to_string(&ErrorResponse {
                    id: request_id,
                    error: ErrorBody {
                        code: "timeout".into(),
                        message: "timed out waiting for output match".into(),
                    },
                })
                .unwrap(),
            ));
        }

        std::thread::sleep(CONNECTION_POLL_INTERVAL);
    }
}

pub(super) fn wait_for_agent(
    request_id: String,
    params: crate::api::schema::AgentWaitParams,
    stream: &mut LocalStream,
    api_tx: &ApiRequestSender,
    event_hub: &crate::api::EventHub,
    running: &Arc<AtomicBool>,
) -> std::io::Result<Option<String>> {
    let last_event_sequence = event_hub.current_sequence();
    let initial = match agent_get(&request_id, &params.target, api_tx) {
        Ok(agent) => agent,
        Err(response) => return encode_agent_response(response),
    };
    wait_for_agent_state(
        request_id,
        params.target,
        params.until,
        params.timeout_ms,
        initial,
        last_event_sequence,
        false,
        stream,
        api_tx,
        event_hub,
        running,
    )
}

pub(super) fn prompt_agent(
    request_id: String,
    params: crate::api::schema::AgentPromptParams,
    stream: &mut LocalStream,
    api_tx: &ApiRequestSender,
    event_hub: &crate::api::EventHub,
    running: &Arc<AtomicBool>,
) -> std::io::Result<Option<String>> {
    let Some(wait) = params.wait.clone() else {
        return Ok(Some(dispatch_to_app_with_timeout(
            Request {
                id: request_id,
                method: Method::AgentPrompt(params),
            },
            api_tx,
            None,
        )));
    };

    let last_event_sequence = event_hub.current_sequence();
    let before_prompt = match agent_get(&request_id, &params.target, api_tx) {
        Ok(agent) => agent,
        Err(response) => return encode_agent_response(response),
    };
    let target = params.target.clone();
    let prompt_response = dispatch_to_app_with_timeout(
        Request {
            id: request_id.clone(),
            method: Method::AgentPrompt(crate::api::schema::AgentPromptParams {
                wait: None,
                ..params
            }),
        },
        api_tx,
        None,
    );
    let prompted = match agent_from_response(&request_id, &prompt_response) {
        Ok(agent) => agent,
        Err(_) => return Ok(Some(prompt_response)),
    };
    if prompted.terminal_id != before_prompt.terminal_id
        || prompted.pane_id != before_prompt.pane_id
    {
        return agent_wait_not_running(request_id).map(Some);
    }
    let wait_started = std::time::Instant::now();

    let until = wait.until;
    let timeout_ms = wait.timeout_ms;
    let mut initial = prompted;
    let mut require_transition = true;
    let mut last_event_sequence = last_event_sequence;
    if initial.agent_status != crate::api::schema::AgentStatus::Working {
        let effect_timeout_ms = timeout_ms.map_or(AGENT_PROMPT_EFFECT_TIMEOUT_MS, |timeout_ms| {
            timeout_ms.min(AGENT_PROMPT_EFFECT_TIMEOUT_MS)
        });
        let phase_result = wait_for_agent_state(
            request_id.clone(),
            target.clone(),
            all_agent_statuses(),
            Some(effect_timeout_ms),
            initial.clone(),
            last_event_sequence,
            true,
            stream,
            api_tx,
            event_hub,
            running,
        )?;
        let Some(phase_response) = phase_result else {
            return Ok(None);
        };
        let phase_value = match serde_json::from_str::<serde_json::Value>(&phase_response) {
            Ok(value) => value,
            Err(_) => return Ok(Some(phase_response)),
        };
        if phase_value
            .get("error")
            .and_then(|error| error.get("code"))
            .and_then(serde_json::Value::as_str)
            == Some("timeout")
            && timeout_ms.is_none_or(|timeout_ms| timeout_ms > AGENT_PROMPT_EFFECT_TIMEOUT_MS)
        {
            let current = match agent_get(&request_id, &target, api_tx) {
                Ok(agent) => agent,
                Err(response) => return encode_agent_response(response),
            };
            if current.terminal_id != initial.terminal_id || current.pane_id != initial.pane_id {
                return agent_wait_not_running(request_id).map(Some);
            }
            return agent_prompt_stalled(
                request_id,
                current,
                last_event_sequence,
                effect_timeout_ms,
            )
            .map(Some);
        }
        if phase_value.get("error").is_some() {
            return Ok(Some(phase_response));
        }
        initial = match agent_from_response(&request_id, &phase_response) {
            Ok(agent) => agent,
            Err(_) => return Ok(Some(phase_response)),
        };
        require_transition = false;
        last_event_sequence = event_hub.current_sequence();
    }

    let result = wait_for_agent_state(
        request_id.clone(),
        target,
        until,
        remaining_timeout_ms(timeout_ms, wait_started),
        initial,
        last_event_sequence,
        require_transition,
        stream,
        api_tx,
        event_hub,
        running,
    )?;
    let Some(response) = result else {
        return Ok(None);
    };
    let value = match serde_json::from_str::<serde_json::Value>(&response) {
        Ok(value) => value,
        Err(_) => return Ok(Some(response)),
    };
    if value.get("error").is_some() {
        return Ok(Some(response));
    }
    let agent = match serde_json::from_value(value["result"]["agent"].clone()) {
        Ok(agent) => agent,
        Err(_) => return Ok(Some(response)),
    };
    agent_prompt_success(request_id, agent).map(Some)
}

fn remaining_timeout_ms(total_ms: Option<u64>, started: std::time::Instant) -> Option<u64> {
    total_ms.map(|total_ms| {
        let elapsed_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        total_ms.saturating_sub(elapsed_ms)
    })
}

fn agent_prompt_success(
    request_id: String,
    agent: crate::api::schema::AgentInfo,
) -> std::io::Result<String> {
    serde_json::to_string(&SuccessResponse {
        id: request_id,
        result: ResponseResult::AgentPrompted { agent },
    })
    .map_err(std::io::Error::other)
}

fn all_agent_statuses() -> Vec<crate::api::schema::AgentStatus> {
    vec![
        crate::api::schema::AgentStatus::Idle,
        crate::api::schema::AgentStatus::Working,
        crate::api::schema::AgentStatus::Blocked,
        crate::api::schema::AgentStatus::Done,
        crate::api::schema::AgentStatus::Unknown,
    ]
}

#[allow(clippy::too_many_arguments)]
fn wait_for_agent_state(
    request_id: String,
    target: String,
    until: Vec<crate::api::schema::AgentStatus>,
    timeout_ms: Option<u64>,
    initial: crate::api::schema::AgentInfo,
    initial_event_sequence: u64,
    require_transition: bool,
    stream: &mut LocalStream,
    api_tx: &ApiRequestSender,
    event_hub: &crate::api::EventHub,
    running: &Arc<AtomicBool>,
) -> std::io::Result<Option<String>> {
    let until = if until.is_empty() {
        vec![
            crate::api::schema::AgentStatus::Idle,
            crate::api::schema::AgentStatus::Done,
            crate::api::schema::AgentStatus::Blocked,
        ]
    } else {
        until
    };
    if !require_transition && until.contains(&initial.agent_status) {
        return agent_wait_success(request_id, initial).map(Some);
    }

    let deadline =
        timeout_ms.map(|ms| std::time::Instant::now() + std::time::Duration::from_millis(ms));
    let expected_terminal_id = initial.terminal_id.clone();
    let expected_pane_id = initial.pane_id.clone();
    let mut last_event_sequence = initial_event_sequence;
    loop {
        if should_stop_connection(stream, running)? {
            return Ok(None);
        }

        for (sequence, event) in event_hub.events_after(last_event_sequence) {
            last_event_sequence = sequence;
            let status = match event.data {
                crate::api::schema::EventData::PaneAgentStatusChanged {
                    pane_id,
                    agent_status,
                    ..
                } if pane_id == expected_pane_id => Some(agent_status),
                crate::api::schema::EventData::PaneExited { pane_id, .. }
                    if pane_id == expected_pane_id =>
                {
                    return agent_wait_not_running(request_id).map(Some);
                }
                crate::api::schema::EventData::PaneClosed { pane_id, .. }
                    if pane_id == expected_pane_id =>
                {
                    return agent_wait_not_running(request_id).map(Some);
                }
                crate::api::schema::EventData::PaneAgentDetected { pane_id, agent, .. }
                    if pane_id == expected_pane_id && agent.is_none() =>
                {
                    return agent_wait_not_running(request_id).map(Some);
                }
                crate::api::schema::EventData::PaneAgentDetected { pane_id, .. }
                    if pane_id == expected_pane_id =>
                {
                    None
                }
                _ => continue,
            };

            let current = match agent_get(&request_id, &target, api_tx) {
                Ok(agent) => agent,
                Err(response)
                    if matches!(
                        response.error.code.as_str(),
                        "pane_not_found" | "agent_not_found"
                    ) =>
                {
                    return agent_wait_not_running(request_id).map(Some);
                }
                Err(response) => return encode_agent_response(response),
            };
            if current.terminal_id != expected_terminal_id || current.pane_id != expected_pane_id {
                return agent_wait_not_running(request_id).map(Some);
            }
            if current.agent.is_none() {
                return agent_wait_not_running(request_id).map(Some);
            }
            if status.is_some_and(|status| until.contains(&status)) {
                return agent_wait_success(request_id, current).map(Some);
            }
        }

        if deadline.is_some_and(|deadline| std::time::Instant::now() >= deadline) {
            return agent_wait_timeout(request_id).map(Some);
        }
        std::thread::sleep(CONNECTION_POLL_INTERVAL);
    }
}

fn agent_get(
    request_id: &str,
    target: &str,
    api_tx: &ApiRequestSender,
) -> Result<crate::api::schema::AgentInfo, ErrorResponse> {
    let response = dispatch_to_app_with_timeout(
        Request {
            id: format!("{request_id}:agent"),
            method: Method::AgentGet(crate::api::schema::AgentTarget {
                target: target.to_string(),
            }),
        },
        api_tx,
        Some(APP_RESPONSE_TIMEOUT),
    );
    agent_from_response(request_id, &response)
}

fn agent_from_response(
    request_id: &str,
    response: &str,
) -> Result<crate::api::schema::AgentInfo, ErrorResponse> {
    let value = serde_json::from_str::<serde_json::Value>(response).map_err(|_| ErrorResponse {
        id: request_id.into(),
        error: ErrorBody {
            code: "internal_error".into(),
            message: "failed to decode agent response".into(),
        },
    })?;
    if value.get("error").is_some() {
        let error = serde_json::from_value(value["error"].clone()).map_err(|_| ErrorResponse {
            id: request_id.into(),
            error: ErrorBody {
                code: "internal_error".into(),
                message: "failed to decode agent error".into(),
            },
        })?;
        return Err(ErrorResponse {
            id: request_id.into(),
            error,
        });
    }
    serde_json::from_value(value["result"]["agent"].clone()).map_err(|_| ErrorResponse {
        id: request_id.into(),
        error: ErrorBody {
            code: "internal_error".into(),
            message: "failed to decode agent result".into(),
        },
    })
}

fn encode_agent_response(response: ErrorResponse) -> std::io::Result<Option<String>> {
    serde_json::to_string(&response)
        .map(Some)
        .map_err(std::io::Error::other)
}

fn agent_wait_success(
    request_id: String,
    agent: crate::api::schema::AgentInfo,
) -> std::io::Result<String> {
    serde_json::to_string(&SuccessResponse {
        id: request_id,
        result: ResponseResult::AgentInfo { agent },
    })
    .map_err(std::io::Error::other)
}

fn agent_wait_not_running(request_id: String) -> std::io::Result<String> {
    serde_json::to_string(&ErrorResponse {
        id: request_id,
        error: ErrorBody {
            code: "agent_not_running".into(),
            message: "agent is no longer running in the target pane".into(),
        },
    })
    .map_err(std::io::Error::other)
}

fn agent_prompt_stalled(
    request_id: String,
    current: crate::api::schema::AgentInfo,
    baseline: u64,
    timeout_ms: u64,
) -> std::io::Result<String> {
    let status = format!("{:?}", current.agent_status).to_ascii_lowercase();
    serde_json::to_string(&ErrorResponse {
        id: request_id,
        error: ErrorBody {
            code: "agent_prompt_stalled".into(),
            message: format!(
                "agent prompt produced no observed state change within {timeout_ms} ms; status is {status} and state_change_seq remained {baseline}"
            ),
        },
    })
    .map_err(std::io::Error::other)
}

fn agent_wait_timeout(request_id: String) -> std::io::Result<String> {
    serde_json::to_string(&ErrorResponse {
        id: request_id,
        error: ErrorBody {
            code: "timeout".into(),
            message: "timed out waiting for agent status".into(),
        },
    })
    .map_err(std::io::Error::other)
}
