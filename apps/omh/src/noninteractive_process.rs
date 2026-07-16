use std::{ffi::OsStr, process::Command};

/// Builds a subprocess whose stdio is controlled by the caller and which never opens a Windows console.
pub(crate) fn command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    crate::platform::configure_background_command(&mut command);
    command
}

pub(crate) fn curl_command() -> Command {
    command("curl")
}

#[cfg(test)]
mod tests {
    use std::process::Stdio;

    use super::command;

    const CHILD_ENV: &str = "OMH_NONINTERACTIVE_PROCESS_TEST_CHILD";

    #[test]
    fn constructor_runs_a_captured_child() {
        if std::env::var_os(CHILD_ENV).is_some() {
            println!("captured child output");
            return;
        }

        let test_exe = std::env::current_exe().expect("resolve test executable");
        let output = command(test_exe)
            .args([
                "noninteractive_process::tests::constructor_runs_a_captured_child",
                "--exact",
                "--nocapture",
            ])
            .env(CHILD_ENV, "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("spawn captured child");

        assert!(output.status.success(), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("captured child output"),
            "{output:?}"
        );
    }
}
