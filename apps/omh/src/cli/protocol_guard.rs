use std::fmt;

use crate::api::schema::{ErrorBody, ErrorResponse};

pub(super) fn mismatch_response(
    request_id: &str,
    server_version: &str,
    server_protocol: u32,
) -> Option<ErrorResponse> {
    let client_protocol = crate::protocol::PROTOCOL_VERSION;
    if server_protocol == client_protocol {
        return None;
    }

    Some(ErrorResponse {
        id: request_id.to_owned(),
        error: ErrorBody {
            code: "protocol_mismatch".into(),
            message: format!(
                "Oh My Herdr CLI protocol {client_protocol} is incompatible with server protocol {server_protocol} (server version {server_version}). Update and restart Oh My Herdr so the CLI and server use the same release."
            ),
        },
    })
}

#[derive(Debug)]
struct ReportedProtocolMismatch;

impl fmt::Display for ReportedProtocolMismatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("protocol mismatch was reported")
    }
}

impl std::error::Error for ReportedProtocolMismatch {}

pub(super) fn reported_error() -> std::io::Error {
    std::io::Error::other(ReportedProtocolMismatch)
}

pub(super) fn was_reported(error: &std::io::Error) -> bool {
    error
        .get_ref()
        .and_then(|source| source.downcast_ref::<ReportedProtocolMismatch>())
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_protocol_is_compatible_across_server_versions() {
        assert_eq!(
            mismatch_response(
                "cli:test",
                "different-build",
                crate::protocol::PROTOCOL_VERSION,
            ),
            None
        );
    }

    #[test]
    fn mismatch_preserves_request_id_and_reports_both_protocols() {
        let server_protocol = crate::protocol::PROTOCOL_VERSION.saturating_sub(1);
        let response = mismatch_response("cli:agent:wait", "0.1.0", server_protocol)
            .expect("different protocols should be rejected");

        assert_eq!(response.id, "cli:agent:wait");
        assert_eq!(response.error.code, "protocol_mismatch");
        assert!(response.error.message.contains(&format!(
            "CLI protocol {}",
            crate::protocol::PROTOCOL_VERSION
        )));
        assert!(response
            .error
            .message
            .contains(&format!("server protocol {server_protocol}")));
        assert!(response.error.message.contains("server version 0.1.0"));
        assert!(response.error.message.contains("Update and restart Oh My Herdr"));
    }

    #[test]
    fn reported_error_is_distinguishable_from_other_io_errors() {
        assert!(was_reported(&reported_error()));
        assert!(!was_reported(&std::io::Error::other("other error")));
    }
}
