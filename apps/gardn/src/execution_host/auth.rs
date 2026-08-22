use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::ExecutionHostId;

pub(crate) const ASKPASS_ROLE_ENV: &str = "GARDN_SSH_ASKPASS_ROLE";
const ASKPASS_SOCKET_ENV: &str = "GARDN_SSH_ASKPASS_SOCKET";
const ASKPASS_TOKEN_ENV: &str = "GARDN_SSH_ASKPASS_TOKEN";
const ASKPASS_MAGIC: &[u8; 7] = b"GARDN1\0";
const ASKPASS_TOKEN_BYTES: usize = 32;
const MAX_PROMPT_BYTES: usize = 8 * 1024;
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const SERVER_POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Runtime-only owner of an interactive OpenSSH authentication attempt.
///
/// This is a client-view identity, not durable coordinator state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct AuthenticationOwner(u64);

impl AuthenticationOwner {
    pub(crate) const SYSTEM: Self = Self(0);

    pub(crate) const fn new(client_view_id: u64) -> Self {
        Self(client_view_id)
    }

    pub(crate) const fn client_view_id(self) -> u64 {
        self.0
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct AuthenticationChallenge {
    pub(crate) id: u64,
    pub(crate) execution_host_id: ExecutionHostId,
    pub(crate) prompt: String,
}

impl fmt::Debug for AuthenticationChallenge {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthenticationChallenge")
            .field("id", &self.id)
            .field("execution_host_id", &self.execution_host_id)
            .field("prompt", &"[REDACTED]")
            .finish()
    }
}

/// A response whose debug representation and drop behavior do not expose its bytes.
pub(crate) struct AuthenticationResponse(Vec<u8>);

impl AuthenticationResponse {
    pub(crate) fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub(crate) fn into_bytes(mut self) -> Vec<u8> {
        std::mem::take(&mut self.0)
    }
}

impl fmt::Debug for AuthenticationResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AuthenticationResponse([REDACTED])")
    }
}

impl Drop for AuthenticationResponse {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AuthenticationCancelled;

impl fmt::Display for AuthenticationCancelled {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("authentication challenge was cancelled")
    }
}

impl std::error::Error for AuthenticationCancelled {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AuthenticationResponseError {
    UnknownChallenge,
}

impl fmt::Display for AuthenticationResponseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("authentication challenge is not owned by this client")
    }
}

impl std::error::Error for AuthenticationResponseError {}

enum Resolution {
    Response(AuthenticationResponse),
    Cancelled,
}

struct PendingChallenge {
    scope: u64,
    challenge: AuthenticationChallenge,
    resolution: mpsc::Sender<Resolution>,
}

#[derive(Default)]
struct ChallengeState {
    active: HashMap<AuthenticationOwner, PendingChallenge>,
    queued: HashMap<AuthenticationOwner, VecDeque<PendingChallenge>>,
}

/// Runtime-only, owner-isolated serialization point for OpenSSH prompts.
///
/// At most one challenge is visible for each client owner. Responses are passed
/// directly to the waiting askpass request and are never retained in snapshots,
/// AppState, logs, or request history.
pub(crate) struct AuthenticationChallengeChannel {
    next_id: AtomicU64,
    state: Mutex<ChallengeState>,
}

impl Default for AuthenticationChallengeChannel {
    fn default() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            state: Mutex::new(ChallengeState::default()),
        }
    }
}

impl AuthenticationChallengeChannel {
    pub(crate) fn challenge_for(
        &self,
        owner: AuthenticationOwner,
    ) -> Option<AuthenticationChallenge> {
        self.state
            .lock()
            .ok()?
            .active
            .get(&owner)
            .map(|pending| pending.challenge.clone())
    }

    pub(crate) fn respond(
        &self,
        owner: AuthenticationOwner,
        challenge_id: u64,
        response: AuthenticationResponse,
    ) -> Result<(), AuthenticationResponseError> {
        let pending = self.take_owned(owner, challenge_id)?;
        let _ = pending.resolution.send(Resolution::Response(response));
        Ok(())
    }

    pub(crate) fn cancel(
        &self,
        owner: AuthenticationOwner,
        challenge_id: u64,
    ) -> Result<(), AuthenticationResponseError> {
        let pending = self.take_owned(owner, challenge_id)?;
        let _ = pending.resolution.send(Resolution::Cancelled);
        Ok(())
    }

    pub(crate) fn cancel_owner(&self, owner: AuthenticationOwner) {
        let mut cancelled = Vec::new();
        if let Ok(mut state) = self.state.lock() {
            if let Some(active) = state.active.remove(&owner) {
                cancelled.push(active);
            }
            if let Some(mut queued) = state.queued.remove(&owner) {
                cancelled.extend(queued.drain(..));
            }
        }
        for pending in cancelled {
            let _ = pending.resolution.send(Resolution::Cancelled);
        }
    }

    pub(crate) fn cancel_host(
        &self,
        owner: AuthenticationOwner,
        execution_host_id: &ExecutionHostId,
    ) {
        let cancelled = self
            .state
            .lock()
            .map(|mut state| {
                Self::remove_matching(&mut state, owner, |pending| {
                    &pending.challenge.execution_host_id == execution_host_id
                })
            })
            .unwrap_or_default();
        Self::notify_cancelled(cancelled);
    }

    fn cancel_scope(&self, scope: u64) {
        let cancelled = self
            .state
            .lock()
            .map(|mut state| {
                let mut owners = state.active.keys().copied().collect::<Vec<_>>();
                for owner in state.queued.keys().copied() {
                    if !owners.contains(&owner) {
                        owners.push(owner);
                    }
                }
                owners
                    .into_iter()
                    .flat_map(|owner| {
                        Self::remove_matching(&mut state, owner, |pending| pending.scope == scope)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Self::notify_cancelled(cancelled);
    }

    fn remove_matching(
        state: &mut ChallengeState,
        owner: AuthenticationOwner,
        mut matches: impl FnMut(&PendingChallenge) -> bool,
    ) -> Vec<PendingChallenge> {
        let mut pending = VecDeque::new();
        if let Some(active) = state.active.remove(&owner) {
            pending.push_back(active);
        }
        if let Some(mut queued) = state.queued.remove(&owner) {
            pending.append(&mut queued);
        }
        let mut cancelled = Vec::new();
        let mut retained = VecDeque::new();
        while let Some(challenge) = pending.pop_front() {
            if matches(&challenge) {
                cancelled.push(challenge);
            } else {
                retained.push_back(challenge);
            }
        }
        if let Some(active) = retained.pop_front() {
            state.active.insert(owner, active);
        }
        if !retained.is_empty() {
            state.queued.insert(owner, retained);
        }
        cancelled
    }

    fn notify_cancelled(cancelled: Vec<PendingChallenge>) {
        for pending in cancelled {
            let _ = pending.resolution.send(Resolution::Cancelled);
        }
    }

    pub(crate) fn request(
        &self,
        owner: AuthenticationOwner,
        scope: u64,
        execution_host_id: ExecutionHostId,
        prompt: String,
    ) -> Result<AuthenticationResponse, AuthenticationCancelled> {
        let challenge = AuthenticationChallenge {
            id: self.next_id.fetch_add(1, Ordering::Relaxed),
            execution_host_id,
            prompt,
        };
        let (resolution, receiver) = mpsc::channel();
        let pending = PendingChallenge {
            scope,
            challenge,
            resolution,
        };
        let mut state = self.state.lock().map_err(|_| AuthenticationCancelled)?;
        match state.active.entry(owner) {
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(pending);
            }
            std::collections::hash_map::Entry::Occupied(_) => {
                state.queued.entry(owner).or_default().push_back(pending);
            }
        }
        drop(state);
        match receiver.recv() {
            Ok(Resolution::Response(response)) => Ok(response),
            Ok(Resolution::Cancelled) | Err(_) => Err(AuthenticationCancelled),
        }
    }

    fn take_owned(
        &self,
        owner: AuthenticationOwner,
        challenge_id: u64,
    ) -> Result<PendingChallenge, AuthenticationResponseError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| AuthenticationResponseError::UnknownChallenge)?;
        if state
            .active
            .get(&owner)
            .is_none_or(|pending| pending.challenge.id != challenge_id)
        {
            return Err(AuthenticationResponseError::UnknownChallenge);
        }
        let pending = state
            .active
            .remove(&owner)
            .ok_or(AuthenticationResponseError::UnknownChallenge)?;
        Self::promote_next(&mut state, owner);
        Ok(pending)
    }

    fn promote_next(state: &mut ChallengeState, owner: AuthenticationOwner) {
        let next = state.queued.get_mut(&owner).and_then(VecDeque::pop_front);
        if state.queued.get(&owner).is_some_and(VecDeque::is_empty) {
            state.queued.remove(&owner);
        }
        if let Some(next) = next {
            state.active.insert(owner, next);
        }
    }
}

#[cfg(unix)]
#[derive(Clone)]
pub(crate) struct AskpassCommandConfig {
    socket_path: std::path::PathBuf,
    askpass_path: std::path::PathBuf,
    token: String,
}

#[cfg(unix)]
impl AskpassCommandConfig {
    pub(crate) fn configure(&self, command: &mut std::process::Command) {
        command
            .env(ASKPASS_ROLE_ENV, "1")
            .env(ASKPASS_SOCKET_ENV, &self.socket_path)
            .env(ASKPASS_TOKEN_ENV, &self.token)
            .env("SSH_ASKPASS", &self.askpass_path)
            .env("SSH_ASKPASS_REQUIRE", "force")
            .env("DISPLAY", "gardn-askpass");
    }
}

#[cfg(unix)]
impl Drop for AskpassCommandConfig {
    fn drop(&mut self) {
        self.token.replace_range(.., "");
    }
}

#[cfg(unix)]
impl fmt::Debug for AskpassCommandConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AskpassCommandConfig")
            .field("socket_path", &self.socket_path)
            .field("token", &"[REDACTED]")
            .finish()
    }
}

/// Private Unix askpass endpoint used only by one bounded OpenSSH connection attempt.
#[cfg(unix)]
pub(crate) struct AskpassServer {
    socket_dir: std::path::PathBuf,
    socket_path: std::path::PathBuf,
    token: [u8; ASKPASS_TOKEN_BYTES],
    scope: u64,
    stop: Arc<AtomicBool>,
    channel: Arc<AuthenticationChallengeChannel>,
    thread: Option<JoinHandle<()>>,
}

#[cfg(unix)]
impl AskpassServer {
    pub(crate) fn start(
        channel: Arc<AuthenticationChallengeChannel>,
        owner: AuthenticationOwner,
        execution_host_id: ExecutionHostId,
    ) -> io::Result<Self> {
        use std::os::unix::fs::PermissionsExt as _;
        use std::os::unix::net::UnixListener;

        let mut token = [0_u8; ASKPASS_TOKEN_BYTES];
        std::fs::File::open("/dev/urandom")?.read_exact(&mut token)?;
        let scope = random_scope(&token);
        let socket_dir =
            std::env::temp_dir().join(format!("gardn-askpass-{}-{scope:016x}", std::process::id()));
        std::fs::create_dir(&socket_dir)?;
        std::fs::set_permissions(&socket_dir, std::fs::Permissions::from_mode(0o700))?;
        let socket_path = socket_dir.join("challenge.sock");
        let listener = UnixListener::bind(&socket_path)?;
        std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;

        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread_channel = channel.clone();
        let thread_host_id = execution_host_id;
        let thread = std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = serve_askpass_request(
                            &mut stream,
                            &token,
                            &thread_channel,
                            owner,
                            scope,
                            &thread_host_id,
                        );
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        std::thread::sleep(SERVER_POLL_INTERVAL);
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            socket_dir,
            socket_path,
            token,
            scope,
            stop,
            channel,
            thread: Some(thread),
        })
    }

    pub(crate) fn command_config(&self) -> io::Result<AskpassCommandConfig> {
        Ok(AskpassCommandConfig {
            socket_path: self.socket_path.clone(),
            askpass_path: std::env::current_exe()?,
            token: hex_token(&self.token),
        })
    }
}

#[cfg(unix)]
impl Drop for AskpassServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.channel.cancel_scope(self.scope);
        let _ = std::os::unix::net::UnixStream::connect(&self.socket_path);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        self.token.fill(0);
        let _ = std::fs::remove_dir_all(&self.socket_dir);
    }
}

#[cfg(unix)]
fn serve_askpass_request(
    stream: &mut std::os::unix::net::UnixStream,
    expected_token: &[u8; ASKPASS_TOKEN_BYTES],
    channel: &AuthenticationChallengeChannel,
    owner: AuthenticationOwner,
    scope: u64,
    execution_host_id: &ExecutionHostId,
) -> io::Result<()> {
    let mut magic = [0_u8; ASKPASS_MAGIC.len()];
    stream.read_exact(&mut magic)?;
    let mut token = [0_u8; ASKPASS_TOKEN_BYTES];
    stream.read_exact(&mut token)?;
    if magic != *ASKPASS_MAGIC || !constant_time_eq(&token, expected_token) {
        token.fill(0);
        write_cancelled(stream)?;
        return Ok(());
    }
    token.fill(0);
    let prompt_len = read_u32(stream)? as usize;
    if prompt_len > MAX_PROMPT_BYTES {
        write_cancelled(stream)?;
        return Ok(());
    }
    let mut prompt = vec![0_u8; prompt_len];
    stream.read_exact(&mut prompt)?;
    let prompt = String::from_utf8(prompt)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "askpass prompt is not UTF-8"))?;
    match channel.request(owner, scope, execution_host_id.clone(), prompt) {
        Ok(response) => write_response(stream, response),
        Err(_) => write_cancelled(stream),
    }
}

#[cfg(unix)]
fn write_response(
    stream: &mut std::os::unix::net::UnixStream,
    response: AuthenticationResponse,
) -> io::Result<()> {
    let mut bytes = response.into_bytes();
    if bytes.len() > MAX_RESPONSE_BYTES {
        bytes.fill(0);
        return write_cancelled(stream);
    }
    stream.write_all(&[1])?;
    write_u32(stream, bytes.len() as u32)?;
    let result = stream.write_all(&bytes);
    bytes.fill(0);
    result
}

#[cfg(unix)]
fn write_cancelled(stream: &mut std::os::unix::net::UnixStream) -> io::Result<()> {
    stream.write_all(&[0])?;
    write_u32(stream, 0)
}

/// Hidden SSH_ASKPASS process role. This runs before normal CLI/session parsing.
pub(crate) fn run_ssh_askpass(args: &[String]) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::net::UnixStream;

        let socket = std::env::var_os(ASKPASS_SOCKET_ENV).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                "askpass socket is unavailable",
            )
        })?;
        let mut token_text = std::env::var(ASKPASS_TOKEN_ENV).map_err(|_| {
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                "askpass token is unavailable",
            )
        })?;
        let mut token = decode_token(&token_text)?;
        token_text.replace_range(.., "");
        let prompt = args
            .first()
            .cloned()
            .unwrap_or_else(|| "SSH authentication required".to_string());
        if prompt.len() > MAX_PROMPT_BYTES {
            token.fill(0);
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "askpass prompt is too large",
            ));
        }
        let mut stream = UnixStream::connect(socket)?;
        stream.write_all(ASKPASS_MAGIC)?;
        stream.write_all(&token)?;
        token.fill(0);
        write_u32(&mut stream, prompt.len() as u32)?;
        stream.write_all(prompt.as_bytes())?;
        drop(prompt);

        let mut outcome = [0_u8; 1];
        stream.read_exact(&mut outcome)?;
        let response_len = read_u32(&mut stream)? as usize;
        if outcome[0] != 1 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "SSH authentication was cancelled",
            ));
        }
        if response_len > MAX_RESPONSE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "askpass response is too large",
            ));
        }
        let mut response = vec![0_u8; response_len];
        stream.read_exact(&mut response)?;
        let mut stdout = io::stdout().lock();
        let write_result = stdout
            .write_all(&response)
            .and_then(|()| stdout.write_all(b"\n"))
            .and_then(|()| stdout.flush());
        response.fill(0);
        write_result
    }
    #[cfg(not(unix))]
    {
        let _ = args;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "SSH askpass is unavailable on this platform",
        ))
    }
}

fn write_u32(writer: &mut impl Write, value: u32) -> io::Result<()> {
    writer.write_all(&value.to_be_bytes())
}

fn read_u32(reader: &mut impl Read) -> io::Result<u32> {
    let mut bytes = [0_u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_be_bytes(bytes))
}

fn hex_token(token: &[u8; ASKPASS_TOKEN_BYTES]) -> String {
    let mut encoded = String::with_capacity(ASKPASS_TOKEN_BYTES * 2);
    for byte in token {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn decode_token(encoded: &str) -> io::Result<[u8; ASKPASS_TOKEN_BYTES]> {
    if encoded.len() != ASKPASS_TOKEN_BYTES * 2 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "askpass token is invalid",
        ));
    }
    let mut token = [0_u8; ASKPASS_TOKEN_BYTES];
    for (index, destination) in token.iter_mut().enumerate() {
        let offset = index * 2;
        *destination = u8::from_str_radix(&encoded[offset..offset + 2], 16).map_err(|_| {
            io::Error::new(io::ErrorKind::PermissionDenied, "askpass token is invalid")
        })?;
    }
    Ok(token)
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0_u8;
    for (left, right) in left.iter().zip(right) {
        difference |= left ^ right;
    }
    difference == 0
}

fn random_scope(token: &[u8; ASKPASS_TOKEN_BYTES]) -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let mut scope = now ^ u64::from(std::process::id());
    for chunk in token.as_chunks::<8>().0 {
        scope ^= u64::from_ne_bytes(*chunk).rotate_left(13);
    }
    scope
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait_for_challenge(
        channel: &AuthenticationChallengeChannel,
        owner: AuthenticationOwner,
    ) -> AuthenticationChallenge {
        for _ in 0..100 {
            if let Some(challenge) = channel.challenge_for(owner) {
                return challenge;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("authentication challenge did not become visible");
    }

    #[test]
    fn challenge_response_is_visible_only_to_its_owner() {
        let channel = Arc::new(AuthenticationChallengeChannel::default());
        let owner = AuthenticationOwner::new(7);
        let other = AuthenticationOwner::new(8);
        let requester = channel.clone();
        let request = std::thread::spawn(move || {
            requester.request(
                owner,
                1,
                ExecutionHostId::new("ssh:test").unwrap(),
                "Password:".to_string(),
            )
        });

        let challenge = wait_for_challenge(&channel, owner);
        assert_eq!(channel.challenge_for(other), None);
        assert_eq!(
            channel.respond(
                other,
                challenge.id,
                AuthenticationResponse::new(b"not-owned".to_vec())
            ),
            Err(AuthenticationResponseError::UnknownChallenge)
        );
        channel
            .respond(
                owner,
                challenge.id,
                AuthenticationResponse::new(b"owner-secret".to_vec()),
            )
            .unwrap();
        let mut response = request.join().unwrap().unwrap().into_bytes();
        assert_eq!(response, b"owner-secret");
        response.fill(0);
    }

    #[test]
    fn challenges_for_one_owner_are_serialized_and_cancelled_together() {
        let channel = Arc::new(AuthenticationChallengeChannel::default());
        let owner = AuthenticationOwner::new(9);
        let first_channel = channel.clone();
        let first = std::thread::spawn(move || {
            first_channel.request(
                owner,
                1,
                ExecutionHostId::new("ssh:first").unwrap(),
                "First:".to_string(),
            )
        });
        let first_challenge = wait_for_challenge(&channel, owner);

        let second_channel = channel.clone();
        let second = std::thread::spawn(move || {
            second_channel.request(
                owner,
                2,
                ExecutionHostId::new("ssh:second").unwrap(),
                "Second:".to_string(),
            )
        });
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(
            channel.challenge_for(owner).map(|challenge| challenge.id),
            Some(first_challenge.id)
        );

        channel
            .cancel(owner, first_challenge.id)
            .expect("owner can cancel its challenge");
        let second_challenge = wait_for_challenge(&channel, owner);
        assert_ne!(second_challenge.id, first_challenge.id);
        channel.cancel_owner(owner);
        assert!(matches!(
            first.join().unwrap(),
            Err(AuthenticationCancelled)
        ));
        assert!(matches!(
            second.join().unwrap(),
            Err(AuthenticationCancelled)
        ));
        assert_eq!(channel.challenge_for(owner), None);
    }

    #[test]
    fn authentication_challenge_debug_never_contains_prompt() {
        let challenge = AuthenticationChallenge {
            id: 7,
            execution_host_id: ExecutionHostId::new("ssh:test").unwrap(),
            prompt: "Password for secret-user:".to_string(),
        };
        let rendered = format!("{challenge:?}");
        assert!(rendered.contains("ssh:test"));
        assert!(!rendered.contains("Password"));
        assert!(!rendered.contains("secret-user"));
    }

    #[test]
    fn authentication_response_debug_never_contains_response_bytes() {
        let response = AuthenticationResponse::new(b"do-not-leak".to_vec());
        let rendered = format!("{response:?}");
        assert_eq!(rendered, "AuthenticationResponse([REDACTED])");
        assert!(!rendered.contains("do-not-leak"));
    }
}
