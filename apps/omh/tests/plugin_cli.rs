#![cfg(unix)]

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use support::{cleanup_test_base, register_runtime_dir, wait_for_socket};

static NEXT_TEST_ID: AtomicUsize = AtomicUsize::new(0);

fn unique_test_dir() -> PathBuf {
    let id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
    PathBuf::from(format!("/tmp/oc{}-{id}", std::process::id()))
}

fn app_dir_name() -> &'static str {
    if cfg!(debug_assertions) {
        "omh-dev"
    } else {
        "omh"
    }
}

fn run_cli(config_home: &Path, runtime_dir: &Path, state_home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_omh"))
        .args(args)
        .env("XDG_CONFIG_HOME", config_home)
        .env("XDG_RUNTIME_DIR", runtime_dir)
        .env("XDG_STATE_HOME", state_home)
        .env_remove("OMH_CLIENT_SOCKET_PATH")
        .env_remove("OMH_SOCKET_PATH")
        .env_remove("OMH_ENV")
        .output()
        .expect("run omh CLI")
}

fn parse_json(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_slice(bytes).expect("valid JSON output")
}

fn write_manifest(root: &Path, content: &str) -> PathBuf {
    fs::create_dir_all(root).expect("create plugin root");
    let manifest = root.join("omh-plugin.toml");
    fs::write(&manifest, content).expect("write plugin manifest");
    manifest
}

fn valid_manifest() -> &'static str {
    r#"
id = "example.offline"
name = "Offline Plugin"
version = "0.1.0"
min_omh_version = "0.2.0"
platforms = ["linux", "macos", "windows"]
"#
}

struct RunningServer {
    child: Child,
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn spawn_server(
    config_home: &Path,
    runtime_dir: &Path,
    state_home: &Path,
    session: &str,
) -> RunningServer {
    fs::create_dir_all(config_home.join(app_dir_name())).expect("create config directory");
    fs::create_dir_all(runtime_dir).expect("create runtime directory");
    fs::create_dir_all(state_home).expect("create state directory");
    register_runtime_dir(runtime_dir);
    fs::write(
        config_home.join(app_dir_name()).join("config.toml"),
        "onboarding = false\n",
    )
    .expect("write test config");

    let child = Command::new(env!("CARGO_BIN_EXE_omh"))
        .args(["--session", session, "server"])
        .env("XDG_CONFIG_HOME", config_home)
        .env("XDG_RUNTIME_DIR", runtime_dir)
        .env("XDG_STATE_HOME", state_home)
        .env_remove("OMH_CLIENT_SOCKET_PATH")
        .env_remove("OMH_SOCKET_PATH")
        .env_remove("OMH_ENV")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn omh server");
    let socket = config_home
        .join(app_dir_name())
        .join("sessions")
        .join(session)
        .join("omh.sock");
    wait_for_socket(&socket, Duration::from_secs(5));
    RunningServer { child }
}

#[test]
fn plugin_link_offline_persists_global_registry_and_replaces_existing() {
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let state_home = base.join("state");
    let manifest = write_manifest(&base.join("plugin"), valid_manifest());

    let first = run_cli(
        &config_home,
        &runtime_dir,
        &state_home,
        &[
            "--session",
            "offline",
            "plugin",
            "link",
            manifest.to_str().unwrap(),
        ],
    );
    assert!(
        first.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_json = parse_json(&first.stdout);
    assert_eq!(first_json["result"]["type"], "plugin_linked");
    assert_eq!(
        first_json["result"]["plugin"]["plugin_id"],
        "example.offline"
    );
    assert_eq!(first_json["result"]["plugin"]["enabled"], true);
    assert_eq!(first_json["result"]["plugin"]["source"]["kind"], "local");

    let second = run_cli(
        &config_home,
        &runtime_dir,
        &state_home,
        &[
            "--session",
            "offline",
            "plugin",
            "link",
            manifest.to_str().unwrap(),
            "--disabled",
        ],
    );
    assert!(
        second.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_json = parse_json(&second.stdout);
    assert_eq!(second_json["result"]["plugin"]["enabled"], false);

    let listed = run_cli(
        &config_home,
        &runtime_dir,
        &state_home,
        &["--session", "other", "plugin", "list", "--json"],
    );
    assert!(
        listed.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let listed_json = parse_json(&listed.stdout);
    let plugins = listed_json["result"]["plugins"].as_array().unwrap();
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0]["plugin_id"], "example.offline");
    assert_eq!(plugins[0]["enabled"], false);
    assert!(config_home
        .join(app_dir_name())
        .join("plugins.json")
        .exists());

    cleanup_test_base(&base);
}

#[test]
fn plugin_link_invalid_manifest_matches_live_server_error() {
    let base = unique_test_dir();
    let offline_config = base.join("offline-config");
    let offline_runtime = base.join("offline-runtime");
    let offline_state = base.join("offline-state");
    let manifest = write_manifest(&base.join("invalid-plugin"), "id = [");

    let offline = run_cli(
        &offline_config,
        &offline_runtime,
        &offline_state,
        &[
            "--session",
            "offline",
            "plugin",
            "link",
            manifest.to_str().unwrap(),
        ],
    );
    assert_eq!(offline.status.code(), Some(1));
    let offline_error = parse_json(&offline.stderr);
    assert_eq!(
        offline_error["error"]["code"],
        "plugin_manifest_parse_failed"
    );

    let live_config = base.join("live-config");
    let live_runtime = base.join("live-runtime");
    let live_state = base.join("live-state");
    let _server = spawn_server(&live_config, &live_runtime, &live_state, "live");
    let live = run_cli(
        &live_config,
        &live_runtime,
        &live_state,
        &[
            "--session",
            "live",
            "plugin",
            "link",
            manifest.to_str().unwrap(),
        ],
    );
    assert_eq!(live.status.code(), Some(1));
    assert_eq!(offline_error, parse_json(&live.stderr));
    assert!(!live_config
        .join(app_dir_name())
        .join("plugins.json")
        .exists());

    cleanup_test_base(&base);
}

#[test]
fn plugin_link_offline_response_matches_live_server_response() {
    let base = unique_test_dir();
    let manifest = write_manifest(&base.join("plugin"), valid_manifest());

    let offline_config = base.join("offline-config");
    let offline_runtime = base.join("offline-runtime");
    let offline_state = base.join("offline-state");
    let offline = run_cli(
        &offline_config,
        &offline_runtime,
        &offline_state,
        &[
            "--session",
            "offline",
            "plugin",
            "link",
            manifest.to_str().unwrap(),
            "--disabled",
        ],
    );
    assert!(
        offline.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&offline.stderr)
    );

    let live_config = base.join("live-config");
    let live_runtime = base.join("live-runtime");
    let live_state = base.join("live-state");
    let _server = spawn_server(&live_config, &live_runtime, &live_state, "live");
    let live = run_cli(
        &live_config,
        &live_runtime,
        &live_state,
        &[
            "--session",
            "live",
            "plugin",
            "link",
            manifest.to_str().unwrap(),
            "--disabled",
        ],
    );
    assert!(
        live.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&live.stderr)
    );
    assert_eq!(parse_json(&offline.stdout), parse_json(&live.stdout));

    cleanup_test_base(&base);
}
