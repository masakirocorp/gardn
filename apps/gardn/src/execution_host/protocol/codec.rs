//! Handshake validation and length-prefixed framing helpers.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::coordinator::CoordinatorMessage;
use super::identity::{MAX_FRAME_SIZE, PROTOCOL_VERSION};
use super::types::WorkerError;
use super::worker::WorkerMessage;
use crate::protocol::{self, FramingError};

// ---------------------------------------------------------------------------
// Handshake validation
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HandshakeError {
    ExpectedHello,
    ExpectedHelloAck,
    ProtocolMismatch { expected: u32, received: u32 },
    Rejected(WorkerError),
}

impl fmt::Display for HandshakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExpectedHello => f.write_str("first coordinator message must be Hello"),
            Self::ExpectedHelloAck => f.write_str("first worker message must be HelloAck"),
            Self::ProtocolMismatch { expected, received } => {
                write!(
                    f,
                    "worker protocol mismatch: expected {expected}, received {received}"
                )
            }
            Self::Rejected(error) => {
                write!(
                    f,
                    "worker handshake rejected: {:?} {}",
                    error.code, error.message
                )
            }
        }
    }
}

impl std::error::Error for HandshakeError {}

/// Reject any non-Hello message as the first coordinator→worker frame.
pub(crate) fn validate_first_coordinator_message(
    message: &CoordinatorMessage,
) -> Result<(), HandshakeError> {
    match message {
        CoordinatorMessage::Hello { version, .. } => {
            if *version != PROTOCOL_VERSION {
                return Err(HandshakeError::ProtocolMismatch {
                    expected: PROTOCOL_VERSION,
                    received: *version,
                });
            }
            Ok(())
        }
        _ => Err(HandshakeError::ExpectedHello),
    }
}

/// Reject any non-HelloAck message as the first worker→coordinator frame.
pub(crate) fn validate_first_worker_message(message: &WorkerMessage) -> Result<(), HandshakeError> {
    match message {
        WorkerMessage::HelloAck { version, error, .. } => {
            if *version != PROTOCOL_VERSION {
                return Err(HandshakeError::ProtocolMismatch {
                    expected: PROTOCOL_VERSION,
                    received: *version,
                });
            }
            if let Some(error) = error {
                return Err(HandshakeError::Rejected(error.clone()));
            }
            Ok(())
        }
        _ => Err(HandshakeError::ExpectedHelloAck),
    }
}

// ---------------------------------------------------------------------------
// Framing helpers (public seams over wire framing)
// ---------------------------------------------------------------------------

/// Write one worker-protocol message using length-prefixed bincode framing.
///
/// Encoded payloads larger than [`MAX_FRAME_SIZE`] are rejected before any
/// bytes are written so callers can return a typed bounded-output error
/// without breaking the transport.
pub(crate) fn write_worker_message<W: std::io::Write, M: Serialize>(
    writer: &mut W,
    message: &M,
) -> Result<(), FramingError> {
    let payload = bincode::serde::encode_to_vec(message, bincode::config::standard())
        .map_err(|error| FramingError::Bincode(error.to_string()))?;
    let len = payload.len();
    if len > MAX_FRAME_SIZE {
        return Err(FramingError::Oversized {
            claimed: len,
            max: MAX_FRAME_SIZE,
        });
    }
    if len > u32::MAX as usize {
        return Err(FramingError::Bincode(format!(
            "payload length {len} exceeds u32::MAX ({}), would be truncated by length prefix",
            u32::MAX
        )));
    }
    writer.write_all(&(len as u32).to_le_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()?;
    Ok(())
}

/// Read one worker-protocol message, enforcing [`MAX_FRAME_SIZE`].
pub(crate) fn read_worker_message<R: std::io::Read, M: for<'de> Deserialize<'de>>(
    reader: &mut R,
) -> Result<M, FramingError> {
    protocol::read_message(reader, MAX_FRAME_SIZE)
}
