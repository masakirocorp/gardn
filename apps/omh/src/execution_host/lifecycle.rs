//! Fenced execution-worker daemon lifecycle protocol V2.
//!
//! Independent of the normal worker bincode protocol. Bytes are frozen: manual
//! little-endian encoding only — no serde, no bincode, no changeable enums on
//! the wire. Used by a future daemon takeover path so a newer artifact can ask
//! an incumbent whether it may keep serving or must drain.

use std::fmt;
use std::io::{self, Read, Write};

use super::protocol::{
    CoordinatorInstallationId, HostBindingGeneration, SessionNamespaceId, WorkerInstanceId,
};
use super::runtime_paths;
use super::ExecutionHostId;

/// Frozen lifecycle protocol version advertised by
/// `execution-worker --daemon-lifecycle-version`.
pub(crate) const DAEMON_LIFECYCLE_VERSION: u16 = 2;
const LEGACY_DAEMON_LIFECYCLE_VERSION: u16 = 1;

/// Four-byte length/prefix that looks like `u32::MAX` so pre-lifecycle worker
/// framing rejects the message as oversized rather than mis-decoding it.
pub(crate) const LIFECYCLE_FRAME_PREFIX: [u8; 4] = [0xff, 0xff, 0xff, 0xff];

/// ASCII magic including trailing NUL: `OMHEWLC\0`.
const FRAME_MAGIC: &[u8; 8] = b"OMHEWLC\0";

const KIND_ACTIVATE_REQUEST: u8 = 1;
const KIND_ACTIVATE_REPLY: u8 = 2;

/// Hard cap on lifecycle payload bytes (excludes the fixed header).
pub(crate) const MAX_PAYLOAD_LEN: usize = 256;

pub(crate) const MAX_APP_VERSION_LEN: usize = 64;
pub(crate) const MAX_WORKER_INSTANCE_ID_LEN: usize = 128;
pub(crate) const BINDING_DIGEST_LEN: usize = 16;
pub(crate) const ARTIFACT_DIGEST_LEN: usize = 32;

const HEADER_LEN: usize = 4 + 8 + 2 + 1 + 1 + 2;

/// Numeric decision codes on the wire. Zero is reserved/invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum LifecycleDecision {
    UseExisting = 1,
    UseExistingDeferred = 2,
    ShuttingDownIdle = 3,
    BlockedBusy = 4,
    Unsupported = 5,
}

impl LifecycleDecision {
    pub(crate) const fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::UseExisting),
            2 => Some(Self::UseExistingDeferred),
            3 => Some(Self::ShuttingDownIdle),
            4 => Some(Self::BlockedBusy),
            5 => Some(Self::Unsupported),
            _ => None,
        }
    }

    pub(crate) const fn as_u8(self) -> u8 {
        self as u8
    }
}

impl fmt::Display for LifecycleDecision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UseExisting => "use_existing",
            Self::UseExistingDeferred => "use_existing_deferred",
            Self::ShuttingDownIdle => "shutting_down_idle",
            Self::BlockedBusy => "blocked_busy",
            Self::Unsupported => "unsupported",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ActivateRequest {
    pub(crate) binding_digest: [u8; BINDING_DIGEST_LEN],
    pub(crate) artifact_digest: [u8; ARTIFACT_DIGEST_LEN],
    pub(crate) desired_worker_protocol: u32,
    pub(crate) desired_app_version: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ActivateReply {
    pub(crate) binding_digest: [u8; BINDING_DIGEST_LEN],
    pub(crate) decision: LifecycleDecision,
    pub(crate) running_worker_protocol: u32,
    pub(crate) owned_runtime_count: u64,
    pub(crate) running_app_version: String,
    pub(crate) worker_instance_id: String,
}

/// Inputs a running daemon uses to choose a lifecycle decision. Defined here so
/// `execution_worker` can call [`decide_activate`] without owning the codec.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LifecycleDecisionInput {
    pub(crate) supports_lifecycle_v1: bool,
    pub(crate) binding_digest: [u8; BINDING_DIGEST_LEN],
    pub(crate) running_artifact_digest: [u8; ARTIFACT_DIGEST_LEN],
    pub(crate) running_worker_protocol: u32,
    pub(crate) running_app_version: String,
    pub(crate) worker_instance_id: String,
    pub(crate) owned_runtime_count: u64,
    /// True when the daemon still owns live work that blocks idle drain.
    pub(crate) busy: bool,
    /// True when a compatible incumbent should keep serving but the requester
    /// must finish deferred handoff work before treating the binding as settled.
    pub(crate) defer_existing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LifecycleCodecError {
    Truncated,
    BadPrefix,
    BadMagic,
    UnsupportedVersion(u16),
    BadKind(u8),
    BadReserved(u8),
    PayloadTooLarge(u16),
    TrailingBytes,
    BadUtf8(&'static str),
    FieldTooLong(&'static str),
    InvalidDecision(u8),
    IncompletePayload,
}

impl fmt::Display for LifecycleCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => f.write_str("lifecycle frame truncated"),
            Self::BadPrefix => f.write_str("lifecycle frame prefix mismatch"),
            Self::BadMagic => f.write_str("lifecycle frame magic mismatch"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported lifecycle version {version}")
            }
            Self::BadKind(kind) => write!(f, "unknown lifecycle frame kind {kind}"),
            Self::BadReserved(value) => write!(f, "lifecycle reserved byte must be 0, got {value}"),
            Self::PayloadTooLarge(len) => {
                write!(
                    f,
                    "lifecycle payload length {len} exceeds {MAX_PAYLOAD_LEN}"
                )
            }
            Self::TrailingBytes => f.write_str("lifecycle frame has trailing bytes"),
            Self::BadUtf8(field) => write!(f, "lifecycle {field} is not valid UTF-8"),
            Self::FieldTooLong(field) => write!(f, "lifecycle {field} exceeds bound"),
            Self::InvalidDecision(code) => write!(f, "invalid lifecycle decision code {code}"),
            Self::IncompletePayload => f.write_str("lifecycle payload incomplete"),
        }
    }
}

impl std::error::Error for LifecycleCodecError {}

impl From<LifecycleCodecError> for io::Error {
    fn from(error: LifecycleCodecError) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, error.to_string())
    }
}

/// SHA-256 binding digest used both for filesystem scope paths and lifecycle
/// frames. Canonical fields are never transmitted on the wire.
pub(crate) fn binding_digest_for(
    installation: &CoordinatorInstallationId,
    namespace: &SessionNamespaceId,
    host: &ExecutionHostId,
    generation: HostBindingGeneration,
) -> [u8; BINDING_DIGEST_LEN] {
    runtime_paths::binding_scope_digest(installation, namespace, host, generation)
}

impl ActivateRequest {
    pub(crate) fn new(
        binding_digest: [u8; BINDING_DIGEST_LEN],
        artifact_digest: [u8; ARTIFACT_DIGEST_LEN],
        desired_worker_protocol: u32,
        desired_app_version: impl Into<String>,
    ) -> Result<Self, LifecycleCodecError> {
        let desired_app_version = desired_app_version.into();
        validate_app_version(&desired_app_version)?;
        Ok(Self {
            binding_digest,
            artifact_digest,
            desired_worker_protocol,
            desired_app_version,
        })
    }

    pub(crate) fn encode(&self) -> Result<Vec<u8>, LifecycleCodecError> {
        let version_bytes = self.desired_app_version.as_bytes();
        validate_app_version(&self.desired_app_version)?;
        let payload_len = BINDING_DIGEST_LEN + ARTIFACT_DIGEST_LEN + 4 + 1 + version_bytes.len();
        if payload_len > MAX_PAYLOAD_LEN {
            return Err(LifecycleCodecError::FieldTooLong(
                "activate request payload",
            ));
        }
        let mut out = Vec::with_capacity(HEADER_LEN + payload_len);
        write_header(&mut out, KIND_ACTIVATE_REQUEST, payload_len as u16);
        out.extend_from_slice(&self.binding_digest);
        out.extend_from_slice(&self.artifact_digest);
        out.extend_from_slice(&self.desired_worker_protocol.to_le_bytes());
        out.push(version_bytes.len() as u8);
        out.extend_from_slice(version_bytes);
        Ok(out)
    }

    pub(crate) fn encode_legacy_v1(&self) -> Result<Vec<u8>, LifecycleCodecError> {
        let version_bytes = self.desired_app_version.as_bytes();
        validate_app_version(&self.desired_app_version)?;
        let payload_len = BINDING_DIGEST_LEN + 4 + 1 + version_bytes.len();
        let mut out = Vec::with_capacity(HEADER_LEN + payload_len);
        write_header_version(
            &mut out,
            LEGACY_DAEMON_LIFECYCLE_VERSION,
            KIND_ACTIVATE_REQUEST,
            payload_len as u16,
        );
        out.extend_from_slice(&self.binding_digest);
        out.extend_from_slice(&self.desired_worker_protocol.to_le_bytes());
        out.push(version_bytes.len() as u8);
        out.extend_from_slice(version_bytes);
        Ok(out)
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, LifecycleCodecError> {
        let payload = decode_header(bytes, KIND_ACTIVATE_REQUEST)?;
        let mut offset = 0;
        let binding_digest = read_digest(payload, &mut offset)?;
        let artifact_digest = read_artifact_digest(payload, &mut offset)?;
        let desired_worker_protocol = read_u32(payload, &mut offset)?;
        let desired_app_version = read_bounded_string(
            payload,
            &mut offset,
            MAX_APP_VERSION_LEN,
            "desired_app_version",
        )?;
        if offset != payload.len() {
            return Err(LifecycleCodecError::TrailingBytes);
        }
        Self::new(
            binding_digest,
            artifact_digest,
            desired_worker_protocol,
            desired_app_version,
        )
    }
}

impl ActivateReply {
    pub(crate) fn new(
        binding_digest: [u8; BINDING_DIGEST_LEN],
        decision: LifecycleDecision,
        running_worker_protocol: u32,
        owned_runtime_count: u64,
        running_app_version: impl Into<String>,
        worker_instance_id: impl Into<String>,
    ) -> Result<Self, LifecycleCodecError> {
        let running_app_version = running_app_version.into();
        let worker_instance_id = worker_instance_id.into();
        validate_app_version(&running_app_version)?;
        validate_worker_instance_id(&worker_instance_id)?;
        Ok(Self {
            binding_digest,
            decision,
            running_worker_protocol,
            owned_runtime_count,
            running_app_version,
            worker_instance_id,
        })
    }

    pub(crate) fn encode(&self) -> Result<Vec<u8>, LifecycleCodecError> {
        let version_bytes = self.running_app_version.as_bytes();
        let instance_bytes = self.worker_instance_id.as_bytes();
        validate_app_version(&self.running_app_version)?;
        validate_worker_instance_id(&self.worker_instance_id)?;
        let payload_len =
            BINDING_DIGEST_LEN + 1 + 4 + 8 + 1 + version_bytes.len() + 1 + instance_bytes.len();
        if payload_len > MAX_PAYLOAD_LEN {
            return Err(LifecycleCodecError::FieldTooLong("activate reply payload"));
        }
        let mut out = Vec::with_capacity(HEADER_LEN + payload_len);
        write_header(&mut out, KIND_ACTIVATE_REPLY, payload_len as u16);
        out.extend_from_slice(&self.binding_digest);
        out.push(self.decision.as_u8());
        out.extend_from_slice(&self.running_worker_protocol.to_le_bytes());
        out.extend_from_slice(&self.owned_runtime_count.to_le_bytes());
        out.push(version_bytes.len() as u8);
        out.extend_from_slice(version_bytes);
        out.push(instance_bytes.len() as u8);
        out.extend_from_slice(instance_bytes);
        Ok(out)
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, LifecycleCodecError> {
        let payload = decode_header(bytes, KIND_ACTIVATE_REPLY)?;
        let mut offset = 0;
        let binding_digest = read_digest(payload, &mut offset)?;
        let decision_code = read_u8(payload, &mut offset)?;
        let decision = LifecycleDecision::from_u8(decision_code)
            .ok_or(LifecycleCodecError::InvalidDecision(decision_code))?;
        let running_worker_protocol = read_u32(payload, &mut offset)?;
        let owned_runtime_count = read_u64(payload, &mut offset)?;
        let running_app_version = read_bounded_string(
            payload,
            &mut offset,
            MAX_APP_VERSION_LEN,
            "running_app_version",
        )?;
        let worker_instance_id = read_bounded_string(
            payload,
            &mut offset,
            MAX_WORKER_INSTANCE_ID_LEN,
            "worker_instance_id",
        )?;
        if offset != payload.len() {
            return Err(LifecycleCodecError::TrailingBytes);
        }
        Self::new(
            binding_digest,
            decision,
            running_worker_protocol,
            owned_runtime_count,
            running_app_version,
            worker_instance_id,
        )
    }

    pub(crate) fn decode_legacy_v1(bytes: &[u8]) -> Result<Self, LifecycleCodecError> {
        Self::decode_with_version(bytes, LEGACY_DAEMON_LIFECYCLE_VERSION)
    }

    fn decode_with_version(
        bytes: &[u8],
        lifecycle_version: u16,
    ) -> Result<Self, LifecycleCodecError> {
        let payload = decode_header_version(bytes, lifecycle_version, KIND_ACTIVATE_REPLY)?;
        let mut offset = 0;
        let binding_digest = read_digest(payload, &mut offset)?;
        let decision_code = read_u8(payload, &mut offset)?;
        let decision = LifecycleDecision::from_u8(decision_code)
            .ok_or(LifecycleCodecError::InvalidDecision(decision_code))?;
        let running_worker_protocol = read_u32(payload, &mut offset)?;
        let owned_runtime_count = read_u64(payload, &mut offset)?;
        let running_app_version = read_bounded_string(
            payload,
            &mut offset,
            MAX_APP_VERSION_LEN,
            "running_app_version",
        )?;
        let worker_instance_id = read_bounded_string(
            payload,
            &mut offset,
            MAX_WORKER_INSTANCE_ID_LEN,
            "worker_instance_id",
        )?;
        if offset != payload.len() {
            return Err(LifecycleCodecError::TrailingBytes);
        }
        Self::new(
            binding_digest,
            decision,
            running_worker_protocol,
            owned_runtime_count,
            running_app_version,
            worker_instance_id,
        )
    }

    pub(crate) fn from_decision_input(
        request: &ActivateRequest,
        input: &LifecycleDecisionInput,
        decision: LifecycleDecision,
    ) -> Result<Self, LifecycleCodecError> {
        Self::new(
            input.binding_digest,
            decision,
            input.running_worker_protocol,
            input.owned_runtime_count,
            input.running_app_version.clone(),
            input.worker_instance_id.clone(),
        )
        .map(|mut reply| {
            // Prefer the request digest when the daemon is answering the same
            // binding; decide_activate already rejects mismatches.
            if decision != LifecycleDecision::Unsupported {
                reply.binding_digest = request.binding_digest;
            }
            reply
        })
    }
}

/// Pure decision helper for later `execution_worker` integration.
///
/// Does not perform I/O. Callers supply the incumbent snapshot; the helper only
/// maps that snapshot plus the activate request onto a frozen decision code.
pub(crate) fn decide_activate(
    request: &ActivateRequest,
    input: &LifecycleDecisionInput,
) -> LifecycleDecision {
    if !input.supports_lifecycle_v1 {
        return LifecycleDecision::Unsupported;
    }
    if input.binding_digest != request.binding_digest {
        return LifecycleDecision::Unsupported;
    }

    let same_protocol = input.running_worker_protocol == request.desired_worker_protocol;
    let same_build = input.running_app_version == request.desired_app_version
        && input.running_artifact_digest == request.artifact_digest;
    let busy = input.busy || input.owned_runtime_count > 0;

    if same_protocol && same_build {
        // Exact build/protocol match. `defer_existing` remains an explicit
        // caller override for handoff work that is not encoded on the wire.
        if input.defer_existing {
            return LifecycleDecision::UseExistingDeferred;
        }
        return LifecycleDecision::UseExisting;
    }

    if same_protocol {
        // The protocol is the compatibility contract. Reuse a busy incumbent
        // and defer its build update until all owned runtimes drain. An idle
        // incumbent can cooperatively release the binding immediately.
        if busy {
            return LifecycleDecision::UseExistingDeferred;
        }
        return LifecycleDecision::ShuttingDownIdle;
    }

    // Incompatible worker protocol.
    if busy {
        LifecycleDecision::BlockedBusy
    } else {
        LifecycleDecision::ShuttingDownIdle
    }
}

/// Convenience constructor for the running-side snapshot used by
/// [`decide_activate`]. `worker_instance_id` is stringly typed so the helper
/// stays usable before a typed id is in hand.
pub(crate) fn lifecycle_decision_input(
    supports_lifecycle_v1: bool,
    binding_digest: [u8; BINDING_DIGEST_LEN],
    running_artifact_digest: [u8; ARTIFACT_DIGEST_LEN],
    running_worker_protocol: u32,
    running_app_version: impl Into<String>,
    worker_instance_id: impl Into<String>,
    owned_runtime_count: u64,
    busy: bool,
    defer_existing: bool,
) -> LifecycleDecisionInput {
    LifecycleDecisionInput {
        supports_lifecycle_v1,
        binding_digest,
        running_artifact_digest,
        running_worker_protocol,
        running_app_version: running_app_version.into(),
        worker_instance_id: worker_instance_id.into(),
        owned_runtime_count,
        busy,
        defer_existing,
    }
}

pub(crate) fn worker_instance_id_string(id: &WorkerInstanceId) -> String {
    id.to_string()
}

fn write_header(out: &mut Vec<u8>, kind: u8, payload_len: u16) {
    write_header_version(out, DAEMON_LIFECYCLE_VERSION, kind, payload_len);
}

fn write_header_version(out: &mut Vec<u8>, version: u16, kind: u8, payload_len: u16) {
    out.extend_from_slice(&LIFECYCLE_FRAME_PREFIX);
    out.extend_from_slice(FRAME_MAGIC);
    out.extend_from_slice(&version.to_le_bytes());
    out.push(kind);
    out.push(0);
    out.extend_from_slice(&payload_len.to_le_bytes());
}

/// Read one complete lifecycle frame (header + payload) from `reader`.
pub(crate) fn read_lifecycle_frame<R: Read>(reader: &mut R) -> io::Result<Vec<u8>> {
    read_lifecycle_frame_version(reader, DAEMON_LIFECYCLE_VERSION)
}

pub(crate) fn read_legacy_lifecycle_frame<R: Read>(reader: &mut R) -> io::Result<Vec<u8>> {
    read_lifecycle_frame_version(reader, LEGACY_DAEMON_LIFECYCLE_VERSION)
}

fn read_lifecycle_frame_version<R: Read>(
    reader: &mut R,
    expected_version: u16,
) -> io::Result<Vec<u8>> {
    let mut header = [0u8; HEADER_LEN];
    reader.read_exact(&mut header)?;
    if header[..4] != LIFECYCLE_FRAME_PREFIX {
        return Err(LifecycleCodecError::BadPrefix.into());
    }
    if header[4..12] != *FRAME_MAGIC {
        return Err(LifecycleCodecError::BadMagic.into());
    }
    let version = u16::from_le_bytes([header[12], header[13]]);
    if version != expected_version {
        return Err(LifecycleCodecError::UnsupportedVersion(version).into());
    }
    let reserved = header[15];
    if reserved != 0 {
        return Err(LifecycleCodecError::BadReserved(reserved).into());
    }
    let payload_len = u16::from_le_bytes([header[16], header[17]]) as usize;
    if payload_len > MAX_PAYLOAD_LEN {
        return Err(LifecycleCodecError::PayloadTooLarge(payload_len as u16).into());
    }
    let mut frame = Vec::with_capacity(HEADER_LEN + payload_len);
    frame.extend_from_slice(&header);
    if payload_len > 0 {
        frame.resize(HEADER_LEN + payload_len, 0);
        reader.read_exact(&mut frame[HEADER_LEN..])?;
    }
    Ok(frame)
}

/// Finish reading a lifecycle frame after the 4-byte prefix was already consumed.
pub(crate) fn complete_lifecycle_frame<R: Read>(
    prefix: [u8; 4],
    reader: &mut R,
) -> io::Result<Vec<u8>> {
    if prefix != LIFECYCLE_FRAME_PREFIX {
        return Err(LifecycleCodecError::BadPrefix.into());
    }
    let mut rest = [0u8; HEADER_LEN - 4];
    reader.read_exact(&mut rest)?;
    let mut header = [0u8; HEADER_LEN];
    header[..4].copy_from_slice(&prefix);
    header[4..].copy_from_slice(&rest);
    if header[4..12] != *FRAME_MAGIC {
        return Err(LifecycleCodecError::BadMagic.into());
    }
    let version = u16::from_le_bytes([header[12], header[13]]);
    if version != DAEMON_LIFECYCLE_VERSION {
        return Err(LifecycleCodecError::UnsupportedVersion(version).into());
    }
    let reserved = header[15];
    if reserved != 0 {
        return Err(LifecycleCodecError::BadReserved(reserved).into());
    }
    let payload_len = u16::from_le_bytes([header[16], header[17]]) as usize;
    if payload_len > MAX_PAYLOAD_LEN {
        return Err(LifecycleCodecError::PayloadTooLarge(payload_len as u16).into());
    }
    let mut frame = Vec::with_capacity(HEADER_LEN + payload_len);
    frame.extend_from_slice(&header);
    if payload_len > 0 {
        frame.resize(HEADER_LEN + payload_len, 0);
        reader.read_exact(&mut frame[HEADER_LEN..])?;
    }
    Ok(frame)
}

/// Write raw lifecycle frame bytes and flush.
pub(crate) fn write_lifecycle_frame<W: Write>(writer: &mut W, frame: &[u8]) -> io::Result<()> {
    writer.write_all(frame)?;
    writer.flush()?;
    Ok(())
}

fn decode_header(bytes: &[u8], expected_kind: u8) -> Result<&[u8], LifecycleCodecError> {
    decode_header_version(bytes, DAEMON_LIFECYCLE_VERSION, expected_kind)
}

fn decode_header_version(
    bytes: &[u8],
    expected_version: u16,
    expected_kind: u8,
) -> Result<&[u8], LifecycleCodecError> {
    if bytes.len() < HEADER_LEN {
        return Err(LifecycleCodecError::Truncated);
    }
    if bytes[..4] != LIFECYCLE_FRAME_PREFIX {
        return Err(LifecycleCodecError::BadPrefix);
    }
    if bytes[4..12] != *FRAME_MAGIC {
        return Err(LifecycleCodecError::BadMagic);
    }
    let version = u16::from_le_bytes([bytes[12], bytes[13]]);
    if version != expected_version {
        return Err(LifecycleCodecError::UnsupportedVersion(version));
    }
    let kind = bytes[14];
    if kind != expected_kind {
        return Err(LifecycleCodecError::BadKind(kind));
    }
    let reserved = bytes[15];
    if reserved != 0 {
        return Err(LifecycleCodecError::BadReserved(reserved));
    }
    let payload_len = u16::from_le_bytes([bytes[16], bytes[17]]) as usize;
    if payload_len > MAX_PAYLOAD_LEN {
        return Err(LifecycleCodecError::PayloadTooLarge(payload_len as u16));
    }
    let end = HEADER_LEN + payload_len;
    if bytes.len() < end {
        return Err(LifecycleCodecError::Truncated);
    }
    if bytes.len() > end {
        return Err(LifecycleCodecError::TrailingBytes);
    }
    Ok(&bytes[HEADER_LEN..end])
}

fn read_digest(
    payload: &[u8],
    offset: &mut usize,
) -> Result<[u8; BINDING_DIGEST_LEN], LifecycleCodecError> {
    if payload.len() < *offset + BINDING_DIGEST_LEN {
        return Err(LifecycleCodecError::IncompletePayload);
    }
    let mut digest = [0u8; BINDING_DIGEST_LEN];
    digest.copy_from_slice(&payload[*offset..*offset + BINDING_DIGEST_LEN]);
    *offset += BINDING_DIGEST_LEN;
    Ok(digest)
}

fn read_artifact_digest(
    payload: &[u8],
    offset: &mut usize,
) -> Result<[u8; ARTIFACT_DIGEST_LEN], LifecycleCodecError> {
    if payload.len() < *offset + ARTIFACT_DIGEST_LEN {
        return Err(LifecycleCodecError::IncompletePayload);
    }
    let mut digest = [0u8; ARTIFACT_DIGEST_LEN];
    digest.copy_from_slice(&payload[*offset..*offset + ARTIFACT_DIGEST_LEN]);
    *offset += ARTIFACT_DIGEST_LEN;
    Ok(digest)
}

fn read_u8(payload: &[u8], offset: &mut usize) -> Result<u8, LifecycleCodecError> {
    let value = *payload
        .get(*offset)
        .ok_or(LifecycleCodecError::IncompletePayload)?;
    *offset += 1;
    Ok(value)
}

fn read_u32(payload: &[u8], offset: &mut usize) -> Result<u32, LifecycleCodecError> {
    if payload.len() < *offset + 4 {
        return Err(LifecycleCodecError::IncompletePayload);
    }
    let value = u32::from_le_bytes(payload[*offset..*offset + 4].try_into().unwrap());
    *offset += 4;
    Ok(value)
}

fn read_u64(payload: &[u8], offset: &mut usize) -> Result<u64, LifecycleCodecError> {
    if payload.len() < *offset + 8 {
        return Err(LifecycleCodecError::IncompletePayload);
    }
    let value = u64::from_le_bytes(payload[*offset..*offset + 8].try_into().unwrap());
    *offset += 8;
    Ok(value)
}

fn read_bounded_string(
    payload: &[u8],
    offset: &mut usize,
    max_len: usize,
    field: &'static str,
) -> Result<String, LifecycleCodecError> {
    let len = read_u8(payload, offset)? as usize;
    if len > max_len {
        return Err(LifecycleCodecError::FieldTooLong(field));
    }
    if payload.len() < *offset + len {
        return Err(LifecycleCodecError::IncompletePayload);
    }
    let bytes = &payload[*offset..*offset + len];
    *offset += len;
    let value = std::str::from_utf8(bytes).map_err(|_| LifecycleCodecError::BadUtf8(field))?;
    Ok(value.to_string())
}

fn validate_app_version(value: &str) -> Result<(), LifecycleCodecError> {
    if value.len() > MAX_APP_VERSION_LEN {
        return Err(LifecycleCodecError::FieldTooLong("app_version"));
    }
    if !value.is_empty() && std::str::from_utf8(value.as_bytes()).is_err() {
        return Err(LifecycleCodecError::BadUtf8("app_version"));
    }
    Ok(())
}

fn validate_worker_instance_id(value: &str) -> Result<(), LifecycleCodecError> {
    if value.len() > MAX_WORKER_INSTANCE_ID_LEN {
        return Err(LifecycleCodecError::FieldTooLong("worker_instance_id"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_host::protocol::PROTOCOL_VERSION;

    fn sample_digest() -> [u8; 16] {
        [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ]
    }

    fn sample_artifact_digest() -> [u8; 32] {
        [0xa5; 32]
    }

    #[test]
    fn activate_request_golden_bytes() {
        let request =
            ActivateRequest::new(sample_digest(), sample_artifact_digest(), 1, "0.1.0").unwrap();
        let bytes = request.encode().unwrap();
        assert_eq!(
            bytes,
            vec![
                // prefix u32::MAX
                0xff, 0xff, 0xff, 0xff, //
                // magic OMHEWLC\0
                b'O', b'M', b'H', b'E', b'W', b'L', b'C', 0x00, //
                // version 2 u16 LE
                0x02, 0x00, //
                // kind request
                0x01, //
                // reserved
                0x00, //
                // payload_len = 16 + 32 + 4 + 1 + 5 = 58
                0x3a, 0x00, //
                // digest
                0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
                0xee, 0xff, //
                // artifact SHA-256 digest
                0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5,
                0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5,
                0xa5, 0xa5, 0xa5, 0xa5, //
                // desired protocol 1
                0x01, 0x00, 0x00, 0x00, //
                // version len + "0.1.0"
                0x05, b'0', b'.', b'1', b'.', b'0',
            ]
        );
        assert_eq!(ActivateRequest::decode(&bytes).unwrap(), request);
    }
    #[test]
    fn legacy_v1_activate_request_golden_bytes() {
        let request =
            ActivateRequest::new(sample_digest(), sample_artifact_digest(), 3, "0.1.0").unwrap();
        assert_eq!(
            request.encode_legacy_v1().unwrap(),
            vec![
                0xff, 0xff, 0xff, 0xff, b'O', b'M', b'H', b'E', b'W', b'L', b'C', 0x00, 0x01, 0x00,
                0x01, 0x00, 0x1a, 0x00, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99,
                0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x03, 0x00, 0x00, 0x00, 0x05, b'0', b'.', b'1',
                b'.', b'0',
            ]
        );
    }

    #[test]
    fn activate_reply_golden_bytes() {
        let reply = ActivateReply::new(
            sample_digest(),
            LifecycleDecision::UseExisting,
            1,
            2,
            "0.1.0",
            "worker-1",
        )
        .unwrap();
        let bytes = reply.encode().unwrap();
        assert_eq!(
            bytes,
            vec![
                0xff, 0xff, 0xff, 0xff, //
                b'O', b'M', b'H', b'E', b'W', b'L', b'C', 0x00, //
                0x02, 0x00, // lifecycle version 2
                0x02, // reply kind
                0x00, //
                // payload_len = 16 + 1 + 4 + 8 + 1 + 5 + 1 + 8 = 44
                0x2c, 0x00, //
                0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
                0xee, 0xff, //
                0x01, // UseExisting
                0x01, 0x00, 0x00, 0x00, // protocol
                0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // owned_runtime_count
                0x05, b'0', b'.', b'1', b'.', b'0', //
                0x08, b'w', b'o', b'r', b'k', b'e', b'r', b'-', b'1',
            ]
        );
        assert_eq!(ActivateReply::decode(&bytes).unwrap(), reply);
    }

    #[test]
    fn decision_codes_are_frozen() {
        assert_eq!(LifecycleDecision::UseExisting.as_u8(), 1);
        assert_eq!(LifecycleDecision::UseExistingDeferred.as_u8(), 2);
        assert_eq!(LifecycleDecision::ShuttingDownIdle.as_u8(), 3);
        assert_eq!(LifecycleDecision::BlockedBusy.as_u8(), 4);
        assert_eq!(LifecycleDecision::Unsupported.as_u8(), 5);
        assert_eq!(LifecycleDecision::from_u8(0), None);
        assert_eq!(LifecycleDecision::from_u8(6), None);
    }

    #[test]
    fn rejects_trailing_bytes_and_oversize_version() {
        let request =
            ActivateRequest::new(sample_digest(), sample_artifact_digest(), 1, "0.1.0").unwrap();
        let mut bytes = request.encode().unwrap();
        bytes.push(0x00);
        assert_eq!(
            ActivateRequest::decode(&bytes),
            Err(LifecycleCodecError::TrailingBytes)
        );

        let too_long = "v".repeat(MAX_APP_VERSION_LEN + 1);
        assert_eq!(
            ActivateRequest::new(sample_digest(), sample_artifact_digest(), 1, too_long),
            Err(LifecycleCodecError::FieldTooLong("app_version"))
        );
    }

    #[test]
    fn busy_protocol_compatible_incumbent_is_reused_until_runtimes_drain() {
        let digest = sample_digest();
        let request =
            ActivateRequest::new(digest, sample_artifact_digest(), PROTOCOL_VERSION, "1.2.3")
                .unwrap();
        let incumbent = lifecycle_decision_input(
            true,
            digest,
            [0x5a; 32],
            PROTOCOL_VERSION,
            "1.2.2",
            "worker-a",
            2,
            true,
            false,
        );

        assert_eq!(
            decide_activate(&request, &incumbent),
            LifecycleDecision::UseExistingDeferred
        );
    }

    #[test]
    fn decide_activate_covers_primary_seams() {
        let digest = sample_digest();
        let artifact_digest = sample_artifact_digest();
        let request =
            ActivateRequest::new(digest, artifact_digest, PROTOCOL_VERSION, "1.2.3").unwrap();

        let compatible = lifecycle_decision_input(
            true,
            digest,
            artifact_digest,
            PROTOCOL_VERSION,
            "1.2.3",
            "worker-a",
            0,
            false,
            false,
        );
        assert_eq!(
            decide_activate(&request, &compatible),
            LifecycleDecision::UseExisting
        );

        let idle_other_artifact = LifecycleDecisionInput {
            running_artifact_digest: [0x5a; 32],
            ..compatible.clone()
        };
        assert_eq!(
            decide_activate(&request, &idle_other_artifact),
            LifecycleDecision::ShuttingDownIdle
        );
        let busy_other_artifact = LifecycleDecisionInput {
            busy: true,
            ..idle_other_artifact
        };
        assert_eq!(
            decide_activate(&request, &busy_other_artifact),
            LifecycleDecision::UseExistingDeferred
        );

        let deferred = LifecycleDecisionInput {
            defer_existing: true,
            ..compatible.clone()
        };
        assert_eq!(
            decide_activate(&request, &deferred),
            LifecycleDecision::UseExistingDeferred
        );

        let idle_upgrade = lifecycle_decision_input(
            true,
            digest,
            artifact_digest,
            PROTOCOL_VERSION,
            "1.2.2",
            "worker-a",
            0,
            false,
            false,
        );
        assert_eq!(
            decide_activate(&request, &idle_upgrade),
            LifecycleDecision::ShuttingDownIdle
        );

        let busy_different_build = lifecycle_decision_input(
            true,
            digest,
            artifact_digest,
            PROTOCOL_VERSION,
            "1.2.2",
            "worker-a",
            1,
            true,
            false,
        );
        assert_eq!(
            decide_activate(&request, &busy_different_build),
            LifecycleDecision::UseExistingDeferred
        );

        let busy_incompatible_protocol = lifecycle_decision_input(
            true,
            digest,
            artifact_digest,
            PROTOCOL_VERSION.saturating_add(1),
            "1.2.2",
            "worker-a",
            1,
            true,
            false,
        );
        assert_eq!(
            decide_activate(&request, &busy_incompatible_protocol),
            LifecycleDecision::BlockedBusy
        );

        let idle_incompatible_protocol = lifecycle_decision_input(
            true,
            digest,
            artifact_digest,
            PROTOCOL_VERSION.saturating_add(1),
            "1.2.2",
            "worker-a",
            0,
            false,
            false,
        );
        assert_eq!(
            decide_activate(&request, &idle_incompatible_protocol),
            LifecycleDecision::ShuttingDownIdle
        );

        let unsupported = lifecycle_decision_input(
            false,
            digest,
            artifact_digest,
            PROTOCOL_VERSION,
            "1.2.3",
            "worker-a",
            0,
            false,
            false,
        );
        assert_eq!(
            decide_activate(&request, &unsupported),
            LifecycleDecision::Unsupported
        );

        let other_binding = lifecycle_decision_input(
            true,
            [0; 16],
            artifact_digest,
            PROTOCOL_VERSION,
            "1.2.3",
            "worker-a",
            0,
            false,
            false,
        );
        assert_eq!(
            decide_activate(&request, &other_binding),
            LifecycleDecision::Unsupported
        );
    }

    #[test]
    fn binding_digest_matches_runtime_path_scope_hex() {
        let installation = CoordinatorInstallationId::new("install-a").unwrap();
        let namespace = SessionNamespaceId::new("session-a").unwrap();
        let host = ExecutionHostId::new("ssh:workbox:1").unwrap();
        let generation = HostBindingGeneration::new(3);
        let digest = binding_digest_for(&installation, &namespace, &host, generation);
        let scope = runtime_paths::binding_scope_hex(&digest);
        let paths = runtime_paths::WorkerRolePaths::for_binding(
            &installation,
            &namespace,
            &host,
            generation,
        );
        assert!(paths.socket_path().to_string_lossy().contains(&scope));
        assert_eq!(digest.len(), 16);
    }
}
