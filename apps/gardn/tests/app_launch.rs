#![cfg(unix)]

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child as ProcessChild, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

fn unique_test_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    PathBuf::from(format!("/tmp/gardn-launch-{}-{nanos}", std::process::id()))
}

fn app_dir_name() -> &'static str {
    if cfg!(debug_assertions) {
        "gardn-dev"
    } else {
        "gardn"
    }
}

struct LaunchGuard {
    base: PathBuf,
    client_master: Option<Box<dyn MasterPty + Send>>,
    client_child: Option<Box<dyn Child + Send + Sync>>,
    server_child: Option<ProcessChild>,
}

impl Drop for LaunchGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.client_child.take() {
            let _ = child.kill();
            if let Some(pid) = child.process_id() {
                wait_for_pid_exit(pid);
            }
        }
        self.client_master.take();

        if let Some(mut child) = self.server_child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn wait_for_pid_exit(pid: u32) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        let mut status = 0;
        let result = unsafe { libc::waitpid(pid as libc::pid_t, &mut status, libc::WNOHANG) };
        if result == pid as libc::pid_t || result == -1 {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn write_test_config(config_home: &Path) {
    let config_dir = config_home.join(app_dir_name());
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(config_dir.join("config.toml"), "onboarding = false\n").unwrap();
}

fn stripped_terminal_text(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            i += 1;
            if i >= bytes.len() {
                break;
            }
            match bytes[i] {
                b'[' => {
                    i += 1;
                    while i < bytes.len() && !(0x40..=0x7e).contains(&bytes[i]) {
                        i += 1;
                    }
                    i += 1;
                }
                b']' => {
                    i += 1;
                    while i < bytes.len() {
                        if bytes[i] == 0x07 {
                            i += 1;
                            break;
                        }
                        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                _ => i += 1,
            }
        } else {
            let byte = bytes[i];
            if byte.is_ascii_graphic() || byte == b' ' {
                out.push(byte as char);
            }
            i += 1;
        }
    }
    out
}

#[test]
fn app_client_launch_renders_visible_first_frame() {
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_gardn"));
    let base = unique_test_dir();
    let config_home = base.join("config");
    let runtime_dir = base.join("runtime");
    let api_socket_path = base.join("gardn.sock");
    let client_socket_path = base.join("gardn-client.sock");
    fs::create_dir_all(&runtime_dir).unwrap();
    write_test_config(&config_home);

    let server = Command::new(&bin)
        .arg("server")
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_RUNTIME_DIR", &runtime_dir)
        .env("GARDN_SOCKET_PATH", &api_socket_path)
        .env_remove("GARDN_CLIENT_SOCKET_PATH")
        .env_remove("GARDN_ENV")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut guard = LaunchGuard {
        base,
        client_master: None,
        client_child: None,
        server_child: Some(server),
    };
    let socket_deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < socket_deadline {
        if client_socket_path.exists()
            && std::os::unix::net::UnixStream::connect(&client_socket_path).is_ok()
        {
            break;
        }
        if let Some(status) = guard.server_child.as_mut().unwrap().try_wait().unwrap() {
            let mut stderr = String::new();
            if let Some(mut stream) = guard.server_child.as_mut().unwrap().stderr.take() {
                let _ = stream.read_to_string(&mut stderr);
            }
            panic!(
                "Gardn server exited before client socket was ready: {status}; stderr: {stderr}"
            );
        }
        thread::sleep(Duration::from_millis(25));
    }
    assert!(
        client_socket_path.exists()
            && std::os::unix::net::UnixStream::connect(&client_socket_path).is_ok(),
        "socket did not become ready: {}",
        client_socket_path.display()
    );

    let client_pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut reader = client_pair.master.try_clone_reader().unwrap();
    let (output_tx, output_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let _reader_thread = thread::spawn(move || {
        let mut buf = [0; 4096];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            if output_tx.send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });

    let mut client_cmd = CommandBuilder::new(&bin);
    client_cmd.arg("client");
    client_cmd.env("XDG_CONFIG_HOME", &config_home);
    client_cmd.env("XDG_RUNTIME_DIR", &runtime_dir);
    client_cmd.env("GARDN_SOCKET_PATH", &api_socket_path);
    client_cmd.env_remove("GARDN_CLIENT_SOCKET_PATH");
    client_cmd.env_remove("GARDN_ENV");
    client_cmd.env("SHELL", "/bin/sh");
    guard.client_child = Some(client_pair.slave.spawn_command(client_cmd).unwrap());
    guard.client_master = Some(client_pair.master);

    let mut output = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Ok(chunk) = output_rx.recv_timeout(Duration::from_millis(100)) {
            output.extend_from_slice(&chunk);
            let text = stripped_terminal_text(&output);
            if text.chars().filter(|ch| ch.is_ascii_alphanumeric()).count() >= 4 {
                drop(guard);
                return;
            }
        }
        if let Some(status) = guard.server_child.as_mut().unwrap().try_wait().unwrap() {
            panic!("Gardn server exited before first client frame: {status}");
        }
    }

    let text = stripped_terminal_text(&output);
    drop(guard);
    panic!("Gardn client did not render visible first frame; stripped output: {text:?}");
}
