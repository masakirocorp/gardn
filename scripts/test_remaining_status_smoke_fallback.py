import os
import shlex
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path


def smoke_script(contents):
    lines = contents.splitlines()
    if lines and not lines[0].strip():
        lines = lines[1:]
    while lines and not lines[-1].strip():
        lines.pop()
    indent = min(
        (len(line) - len(line.lstrip(" ")) for line in lines if line.strip()),
        default=0,
    )
    return "\n".join(line[indent:] if line.strip() else "" for line in lines) + "\n"


class RemainingStatusSmokeFallbackTests(unittest.TestCase):
    def test_retries_when_real_cli_writes_retryable_provider_output_before_exiting(self):
        repo_root = Path(__file__).resolve().parents[1]
        source_script = repo_root / "ci" / "agent-smoke" / "remaining-status-smoke.sh"
        source_models = repo_root / "ci" / "agent-smoke" / "smoke-models.sh"

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            bin_dir = tmp_path / "bin"
            lib_dir = tmp_path / "lib"
            smoke_dir = tmp_path / "smoke"
            home_dir = tmp_path / "home"
            factory_home = tmp_path / "factory-home"
            kimi_home = tmp_path / "kimi-home"
            bin_dir.mkdir()
            lib_dir.mkdir()
            smoke_dir.mkdir()
            home_dir.mkdir()
            factory_home.mkdir()
            kimi_home.mkdir()
            (factory_home / "settings.json").write_text("{}")

            models_copy = lib_dir / "hako-agent-smoke-models.sh"
            models_copy.write_text(source_models.read_text())

            script_copy = bin_dir / "hako-agent-smoke-remaining-status"
            script_copy.write_text(source_script.read_text())
            script_copy.chmod(script_copy.stat().st_mode | stat.S_IXUSR)

            reporter = bin_dir / "hako-test-report"
            reporter.write_text(
                smoke_script(
                    r"""
                    #!/usr/bin/env python3
                    import json
                    import os
                    import socket
                    import sys

                    pane, agent, source, session_id, event, state = sys.argv[1:7]
                    method = "pane.release_agent" if event == "release" else "pane.report_agent"
                    params = {
                        "pane_id": pane,
                        "agent": agent,
                        "source": source,
                        "seq": 1,
                        "agent_session_id": session_id,
                    }
                    if event == "report":
                        params["state"] = state
                    request = {"id": f"test:{pane}:{event}:{state}", "method": method, "params": params}
                    client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                    client.settimeout(1)
                    client.connect(os.environ["HAKO_SOCKET_PATH"])
                    client.sendall((json.dumps(request) + "\n").encode("utf-8"))
                    try:
                        client.recv(4096)
                    finally:
                        client.close()
                    """
                )
            )
            reporter.chmod(reporter.stat().st_mode | stat.S_IXUSR)

            attempts_log = tmp_path / "cli-attempts.txt"
            fake_copilot = bin_dir / "copilot"
            fake_copilot.write_text(
                smoke_script(
                    fr"""
                    #!/usr/bin/env bash
                    set -euo pipefail
                    model=""
                    while (( $# )); do
                      case "$1" in
                        --model)
                          model="$2"
                          shift 2
                          ;;
                        *)
                          shift
                          ;;
                      esac
                    done
                    printf 'copilot:%s\n' "$model" >> {shlex.quote(str(attempts_log))}
                    case "$model" in
                      anthropic/overloaded)
                        echo "OpenRouter provider error: HTTP 429 no endpoint available" >&2
                        exit 1
                        ;;
                      anthropic/ok)
                        hako-test-report pane-copilot-real copilot hako:copilot copilot-real report working
                        hako-test-report pane-copilot-real copilot hako:copilot copilot-real report idle
                        echo '{{"message":"HAKO_COPILOT_STATUS_OK"}}'
                        exit 0
                        ;;
                      *)
                        echo "unexpected copilot model: $model" >&2
                        exit 65
                        ;;
                    esac
                    """
                )
            )
            fake_copilot.chmod(fake_copilot.stat().st_mode | stat.S_IXUSR)

            fake_droid = bin_dir / "droid"
            fake_droid.write_text(
                smoke_script(
                    fr"""
                    #!/usr/bin/env bash
                    set -euo pipefail
                    if [[ "${{1:-}}" != "exec" ]]; then
                      echo "unexpected droid invocation: $*" >&2
                      exit 64
                    fi
                    shift
                    model=""
                    while (( $# )); do
                      case "$1" in
                        --model)
                          model="$2"
                          shift 2
                          ;;
                        *)
                          shift
                          ;;
                      esac
                    done
                    printf 'droid:%s\n' "$model" >> {shlex.quote(str(attempts_log))}
                    if [[ "$model" != "anthropic/ok" ]]; then
                      echo "unexpected droid model: $model" >&2
                      exit 65
                    fi
                    hako-test-report pane-droid-real droid hako:droid droid-real report idle
                    hako-test-report pane-droid-real droid hako:droid droid-real release release
                    printf '%s\n' '{{"type":"result","result":"HAKO_DROID_STATUS_OK"}}'
                    """
                )
            )
            fake_droid.chmod(fake_droid.stat().st_mode | stat.S_IXUSR)

            fake_kimi = bin_dir / "kimi"
            fake_kimi.write_text(
                smoke_script(
                    fr"""
                    #!/usr/bin/env bash
                    set -euo pipefail
                    printf 'kimi:%s\n' "${{HAKO_PANE_ID:-missing-pane}}" >> {shlex.quote(str(attempts_log))}
                    hako-test-report pane-kimi-real kimi hako:kimi kimi-real report idle
                    hako-test-report pane-kimi-real kimi hako:kimi kimi-real report working
                    hako-test-report pane-kimi-real kimi hako:kimi kimi-real report idle
                    hako-test-report pane-kimi-real kimi hako:kimi kimi-real release release
                    echo "HAKO_KIMI_STATUS_OK"
                    """
                )
            )
            fake_kimi.chmod(fake_kimi.stat().st_mode | stat.S_IXUSR)

            fake_hermes = bin_dir / "hermes"
            fake_hermes.write_text(
                smoke_script(
                    fr"""
                    #!/usr/bin/env bash
                    set -euo pipefail
                    model=""
                    while (( $# )); do
                      case "$1" in
                        --model)
                          model="$2"
                          shift 2
                          ;;
                        *)
                          shift
                          ;;
                      esac
                    done
                    printf 'hermes:%s\n' "$model" >> {shlex.quote(str(attempts_log))}
                    if [[ "$model" != "anthropic/ok" ]]; then
                      echo "unexpected hermes model: $model" >&2
                      exit 65
                    fi
                    hako-test-report pane-hermes-real hermes hako:hermes hermes-real report idle
                    hako-test-report pane-hermes-real hermes hako:hermes hermes-real report working
                    hako-test-report pane-hermes-real hermes hako:hermes hermes-real report idle
                    echo "HAKO_HERMES_STATUS_OK"
                    """
                )
            )
            fake_hermes.chmod(fake_hermes.stat().st_mode | stat.S_IXUSR)

            fake_cursor = bin_dir / "cursor-agent"
            fake_cursor.write_text(
                smoke_script(
                    r"""
                    #!/usr/bin/env bash
                    set -euo pipefail
                    echo "invalid api key for OpenRouter auth contract" >&2
                    exit 1
                    """
                )
            )
            fake_cursor.chmod(fake_cursor.stat().st_mode | stat.S_IXUSR)

            fake_qoder = bin_dir / "qodercli"
            fake_qoder.write_text(
                smoke_script(
                    r"""
                    #!/usr/bin/env bash
                    set -euo pipefail
                    echo "not logged in; run login first" >&2
                    exit 1
                    """
                )
            )
            fake_qoder.chmod(fake_qoder.stat().st_mode | stat.S_IXUSR)

            fake_timeout = bin_dir / "timeout"
            fake_timeout.write_text(
                smoke_script(
                    r"""
                    #!/usr/bin/env bash
                    set -euo pipefail
                    shift
                    exec "$@"
                    """
                )
            )
            fake_timeout.chmod(fake_timeout.stat().st_mode | stat.S_IXUSR)

            env = {
                **os.environ,
                "PATH": f"{bin_dir}{os.pathsep}{os.environ['PATH']}",
                "HOME": str(home_dir),
                "OPENROUTER_API_KEY": "sk-test-fake-openrouter-key",
                "HAKO_REPO_DIR": str(repo_root),
                "HAKO_AGENT_SMOKE_MODELS_LIB": str(models_copy),
                "HAKO_REMAINING_STATUS_SMOKE_DIR": str(smoke_dir),
                "HAKO_REMAINING_STATUS_SMOKE_TIMEOUT": "5",
                "HAKO_SMOKE_MODEL": "openrouter/anthropic/overloaded",
                "HAKO_SMOKE_FALLBACK_MODELS": "openrouter/anthropic/ok",
                "DROID_HOME": str(factory_home),
                "FACTORY_HOME": str(factory_home),
                "KIMI_CODE_HOME": str(kimi_home),
            }
            env.pop("CURSOR_API_KEY", None)
            env.pop("QODER_PERSONAL_ACCESS_TOKEN", None)

            result = subprocess.run(
                [str(script_copy)],
                cwd=repo_root,
                env=env,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=20,
            )

            output = result.stdout + result.stderr
            self.assertEqual(result.returncode, 0, output)
            self.assertEqual(
                attempts_log.read_text().splitlines(),
                [
                    "copilot:anthropic/overloaded",
                    "copilot:anthropic/ok",
                    "droid:anthropic/ok",
                    "kimi:pane-kimi-real",
                    "hermes:anthropic/ok",
                ],
                output,
            )
            self.assertIn("OpenRouter provider error: HTTP 429 no endpoint available", output)
            self.assertIn(
                "retrying after provider/model failure: anthropic/overloaded",
                output,
            )
            self.assertIn("trying smoke model: anthropic/ok", output)
            self.assertIn("remaining status test ok:", output)


if __name__ == "__main__":
    unittest.main()
