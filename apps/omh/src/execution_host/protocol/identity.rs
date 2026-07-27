//! Identity and monotonic protocol newtypes.
//!
//! Validated opaque string ids, numeric request/revision counters, and the
//! full adoption key for a worker-owned runtime.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

// ---------------------------------------------------------------------------
// Protocol constants
// ---------------------------------------------------------------------------

/// Execution Worker Protocol compatibility marker.
///
/// Exact-match negotiation. Distinct from the thin-client
/// [`crate::protocol::PROTOCOL_VERSION`].
pub(crate) const PROTOCOL_VERSION: u32 = 2;

/// Maximum worker-protocol frame payload (16 MiB).
///
/// Sized so a canonical terminal checkpoint can cover configured scrollback
/// without borrowing the thin-client graphics exception.
pub(crate) const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024;

const MAX_INSTALLATION_ID_LEN: usize = 128;
const MAX_SESSION_NAMESPACE_ID_LEN: usize = 64;
const MAX_WORKER_INSTANCE_ID_LEN: usize = 128;
const MAX_WORKER_RUNTIME_ID_LEN: usize = 128;
const MAX_AUTH_CHALLENGE_ID_LEN: usize = 128;
const MAX_AUTH_PROOF_LEN: usize = 512;

// ---------------------------------------------------------------------------
// Monotonic numeric newtypes
// ---------------------------------------------------------------------------

macro_rules! u64_newtype {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub(crate) struct $name(u64);

        impl $name {
            pub(crate) const fn new(value: u64) -> Self {
                Self(value)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

u64_newtype!(
    /// Correlates a coordinator request with worker replies.
    RequestId
);
u64_newtype!(
    /// Monotonic ordered output revision for one runtime incarnation.
    OutputRevision
);
u64_newtype!(
    /// Generation of the coordinator's host binding for this logical host.
    HostBindingGeneration
);
u64_newtype!(
    /// Worker-assigned incarnation for one live runtime identity.
    RuntimeIncarnation
);
u64_newtype!(
    /// Per-runtime operation sequence for ordered input/signal/resize.
    RuntimeOpSeq
);

impl OutputRevision {
    pub(crate) const fn get(self) -> u64 {
        self.0
    }
}

impl HostBindingGeneration {
    pub(crate) const fn get(self) -> u64 {
        self.0
    }
}

impl RuntimeOpSeq {
    pub(crate) const fn get(self) -> u64 {
        self.0
    }
}

// ---------------------------------------------------------------------------
// Validated opaque string newtypes
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProtocolIdError {
    Empty,
    TooLong,
    InvalidCharacter,
}

impl fmt::Display for ProtocolIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "value must not be empty",
            Self::TooLong => "value is too long",
            Self::InvalidCharacter => "value contains an invalid character",
        })
    }
}

pub(super) fn validate_token(
    value: &str,
    max_len: usize,
    allow_extra: &[u8],
) -> Result<(), ProtocolIdError> {
    if value.is_empty() {
        return Err(ProtocolIdError::Empty);
    }
    if value.len() > max_len {
        return Err(ProtocolIdError::TooLong);
    }
    if !value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
            || allow_extra.contains(&byte)
    }) {
        return Err(ProtocolIdError::InvalidCharacter);
    }
    Ok(())
}

macro_rules! string_id {
    ($(#[$meta:meta])* $name:ident, $max:expr) => {
        string_id!($(#[$meta])* $name, $max, &[]);
    };
    ($(#[$meta:meta])* $name:ident, $max:expr, $extra:expr) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub(crate) struct $name(String);

        impl $name {
            pub(crate) fn new(value: impl Into<String>) -> Result<Self, ProtocolIdError> {
                let value = value.into();
                validate_token(&value, $max, $extra)?;
                Ok(Self(value))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

string_id!(
    /// Durable coordinator installation identity scoping worker leases.
    CoordinatorInstallationId,
    MAX_INSTALLATION_ID_LEN
);
string_id!(
    /// Session Namespace UUID (opaque string form) for this coordinator session.
    SessionNamespaceId,
    MAX_SESSION_NAMESPACE_ID_LEN
);
string_id!(
    /// Worker process/instance identity assigned at worker boot.
    WorkerInstanceId,
    MAX_WORKER_INSTANCE_ID_LEN
);
string_id!(
    /// Worker-local runtime id. Never a public coordinator id.
    WorkerRuntimeId,
    MAX_WORKER_RUNTIME_ID_LEN
);
string_id!(
    /// Ephemeral authentication challenge identifier.
    AuthChallengeId,
    MAX_AUTH_CHALLENGE_ID_LEN
);
string_id!(
    /// Ephemeral authentication proof material (not a private key).
    AuthProof,
    MAX_AUTH_PROOF_LEN,
    b"+=/"
);

impl CoordinatorInstallationId {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl SessionNamespaceId {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

// ---------------------------------------------------------------------------
// Runtime identity
// ---------------------------------------------------------------------------

/// Full adoption key for a worker-owned runtime.
///
/// Adoption matches the complete tuple — never PID alone.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RuntimeIdentity {
    pub(crate) host_binding_generation: HostBindingGeneration,
    pub(crate) worker_instance_id: WorkerInstanceId,
    pub(crate) runtime_id: WorkerRuntimeId,
    pub(crate) incarnation: RuntimeIncarnation,
}

impl RuntimeIdentity {
    pub(crate) fn new(
        host_binding_generation: HostBindingGeneration,
        worker_instance_id: WorkerInstanceId,
        runtime_id: WorkerRuntimeId,
        incarnation: RuntimeIncarnation,
    ) -> Self {
        Self {
            host_binding_generation,
            worker_instance_id,
            runtime_id,
            incarnation,
        }
    }
}
