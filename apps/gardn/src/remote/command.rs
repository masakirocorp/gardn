use std::io::{self, Read};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use super::ConnectCancel;

pub(super) const SSH_COMMAND_TIMEOUT: Duration = Duration::from_secs(5 * 60);
pub(super) const SSH_TRANSFER_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const OUTPUT_LIMIT: usize = 16 * 1024 * 1024;
const PREAMBLE_LIMIT: usize = 64 * 1024;
static FRAME_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) fn capture_output(
    command: &mut Command,
    mut input: impl Read + Send + 'static,
    cancel: Option<&ConnectCancel>,
    timeout: Duration,
) -> io::Result<Output> {
    if let Some(cancel) = cancel {
        cancel.check()?;
    }
    let deadline = Instant::now() + timeout;
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::platform::configure_cancellable_command(command);
    let mut child = command.spawn()?;
    let (Some(mut stdin), Some(stdout), Some(stderr)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        crate::platform::terminate_cancellable_child(&mut child);
        return Err(io::Error::other("SSH command stdio was not available"));
    };
    let writer = thread::spawn(move || io::copy(&mut input, &mut stdin));
    let stdout = thread::spawn(move || capture_pipe(stdout));
    let stderr = thread::spawn(move || capture_pipe(stderr));
    let status = (|| {
        let mut status = None;
        loop {
            if let Some(cancel) = cancel {
                cancel.check()?;
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "SSH command exceeded its deadline",
                ));
            }
            if status.is_none() {
                status = child.try_wait()?;
            }
            if writer.is_finished() && stdout.is_finished() && stderr.is_finished() {
                if let Some(status) = status {
                    return Ok(status);
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
    })();
    if status.is_err() {
        crate::platform::terminate_cancellable_child(&mut child);
    }
    let written = writer
        .join()
        .map_err(|_| io::Error::other("SSH input writer panicked"));
    let stdout = stdout
        .join()
        .map_err(|_| io::Error::other("SSH stdout reader panicked"));
    let stderr = stderr
        .join()
        .map_err(|_| io::Error::other("SSH stderr reader panicked"));
    let status = status?;
    let output = Output {
        status,
        stdout: stdout??,
        stderr: stderr??,
    };
    // Authentication can fail before SSH reads stdin. Preserve its exit status and diagnostic.
    if status.success() {
        written??;
    }
    Ok(output)
}

fn capture_pipe(reader: impl Read) -> io::Result<Vec<u8>> {
    match crate::platform::read_limited_reader(reader, OUTPUT_LIMIT)? {
        crate::platform::LimitedRead::Empty => Ok(Vec::new()),
        crate::platform::LimitedRead::Complete(bytes) => Ok(bytes),
        crate::platform::LimitedRead::Oversized => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "SSH command output exceeded 16 MiB",
        )),
    }
}

fn marker() -> String {
    let sequence = FRAME_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("__GARDN_SSH_{}_{}__", std::process::id(), sequence)
}

pub(super) fn frame_remote_script(script: &str) -> (String, String, String) {
    let token = marker();
    let begin = format!("{token}_BEGIN");
    let end = format!("{token}_END");
    let framed = format!(
        "printf '\\n%s\\n' '{begin}';\n(\n{script}\n)\n__gardn_remote_status=$?\nprintf '\\n%s\\n' '{end}'; exit \"$__gardn_remote_status\"\n"
    );
    (framed, begin, end)
}

pub(super) fn frame_remote_stream_command(command: &str) -> (String, String) {
    let marker = marker();
    (format!("printf '\\n%s\\n' '{marker}'; {command}"), marker)
}

pub(super) fn unframe_remote_output(
    output: &mut Output,
    begin: &str,
    end: Option<&str>,
) -> io::Result<()> {
    let begin = format!("\n{begin}\n");
    let Some(start) = output
        .stdout
        .windows(begin.len())
        .position(|window| window == begin.as_bytes())
        .map(|index| index + begin.len())
    else {
        if !output.status.success() {
            return Ok(());
        }
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "SSH command result frame is missing",
        ));
    };
    let finish = if let Some(end) = end {
        let end = format!("\n{end}\n");
        output.stdout[start..]
            .windows(end.len())
            .position(|window| window == end.as_bytes())
            .map(|index| start + index)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "SSH command result frame is incomplete",
                )
            })?
    } else {
        output.stdout.len()
    };
    output.stdout.copy_within(start..finish, 0);
    output.stdout.truncate(finish - start);
    Ok(())
}

pub(crate) fn consume_remote_stream_preamble(
    stdout: &mut impl Read,
    marker: &str,
) -> io::Result<()> {
    let terminator = format!("\n{marker}\n");

    let mut matched = 0;
    let mut byte = [0];
    for _ in 0..PREAMBLE_LIMIT {
        stdout.read_exact(&mut byte)?;
        matched = if byte[0] == terminator.as_bytes()[matched] {
            matched + 1
        } else {
            usize::from(byte[0] == b'\n')
        };
        if matched == terminator.len() {
            return Ok(());
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "SSH stream preamble exceeded 64 KiB",
    ))
}
pub(super) fn await_stream_preamble(
    child: &mut std::process::Child,
    mut stdout: std::process::ChildStdout,
    marker: String,
    cancelled: impl Fn() -> bool,
) -> io::Result<std::process::ChildStdout> {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let reader = thread::spawn(move || {
        let result = consume_remote_stream_preamble(&mut stdout, &marker);
        let _ = sender.send(result.map(|()| stdout));
    });
    let deadline = Instant::now() + SSH_COMMAND_TIMEOUT;
    let result = loop {
        if cancelled() {
            break Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "SSH connection attempt cancelled",
            ));
        }
        if Instant::now() >= deadline {
            break Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "SSH stream setup exceeded its deadline",
            ));
        }
        match receiver.recv_timeout(Duration::from_millis(20)) {
            Ok(result) => break result,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                break Err(io::Error::other("SSH preamble reader stopped"));
            }
        }
    };
    if result.is_err() {
        crate::platform::terminate_cancellable_child(child);
    }
    let _ = reader.join();
    result
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn framed_script_ignores_banners_and_preserves_binary_output_and_exit_status() {
        let (script, begin, end) = frame_remote_script("printf '\\377\\000payload'; exit 7");
        let mut output = capture_output(
            Command::new("/bin/sh")
                .args(["-c", "printf 'Company login notice\\n'; exec /bin/sh -s"]),
            Cursor::new(script.into_bytes()),
            None,
            Duration::from_secs(3),
        )
        .unwrap();
        unframe_remote_output(&mut output, &begin, Some(&end)).unwrap();
        assert_eq!(output.status.code(), Some(7));
        assert_eq!(output.stdout, b"\xff\0payload");
    }

    #[test]
    fn capture_drains_both_output_pipes_while_feeding_large_input() {
        let output = capture_output(
            Command::new("/bin/sh").args(["-c",
                "dd if=/dev/zero bs=1024 count=512 2>/dev/null; dd if=/dev/zero bs=1024 count=512 >&2 2>/dev/null; wc -c"]),
            Cursor::new(vec![b'x'; 1024 * 1024]),
            None,
            Duration::from_secs(3),
        ).unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(&output.stdout[..512 * 1024], vec![0; 512 * 1024]);
        assert_eq!(
            String::from_utf8_lossy(&output.stdout[512 * 1024..]).trim(),
            "1048576"
        );
        assert_eq!(output.stderr, vec![0; 512 * 1024]);
    }

    #[test]
    fn deadline_covers_pipe_holding_descendants_after_command_exit() {
        let start = Instant::now();
        let error = capture_output(
            Command::new("/bin/sh").args(["-c", "sleep 30 & exit 0"]),
            io::empty(),
            None,
            Duration::from_millis(100),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "pipe holders outlived the command deadline"
        );
    }

    #[test]
    fn cancelling_a_running_command_reaps_its_pipe_holding_processes() {
        let ready = std::env::temp_dir().join(marker());
        let child_ready = ready.clone();
        let cancel = ConnectCancel::new();
        let child_cancel = cancel.clone();
        let command = thread::spawn(move || {
            capture_output(
                Command::new("/bin/sh")
                    .args(["-c", "printf ready > \"$1\"; sleep 30", "ssh-test"])
                    .arg(child_ready),
                io::empty(),
                Some(&child_cancel),
                Duration::from_secs(3),
            )
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        while !ready.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        let started = ready.exists();
        cancel.cancel();
        let result = command.join().unwrap();
        let _ = std::fs::remove_file(ready);
        assert!(started, "command did not report readiness");
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
    }

    #[test]
    fn stream_framing_leaves_binary_protocol_bytes_unread() {
        let (command, marker) = frame_remote_stream_command("printf '\\000\\377\\001payload'");
        let mut child = Command::new("/bin/sh")
            .args(["-c", &format!("printf 'login banner\\n'; {command}")])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut stdout = await_stream_preamble(&mut child, stdout, marker, || false).unwrap();
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).unwrap();
        assert!(child.wait().unwrap().success());
        assert_eq!(bytes, b"\0\xff\x01payload");
    }
}
