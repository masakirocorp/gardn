import os
import shlex
import stat
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path


def run_opencode_status_test(primary_model: str, fallback_model: str):
    repo_root = Path(__file__).resolve().parents[1]
    source_script = repo_root / "ci" / "agent-tests" / "opencode-status-test.sh"
    source_models = repo_root / "ci" / "agent-tests" / "test-models.sh"

    temp_dir = tempfile.TemporaryDirectory()
    tmp_path = Path(temp_dir.name)
    bin_dir = tmp_path / "bin"
    lib_dir = tmp_path / "lib"
    bin_dir.mkdir()
    lib_dir.mkdir()

    models_copy = lib_dir / "omh-agent-test-models.sh"
    models_copy.write_text(source_models.read_text())

    script_copy = bin_dir / "omh-agent-tests-opencode-status"
    script_copy.write_text(
        source_script.read_text().replace(
            "source /usr/local/lib/omh-agent-test-models.sh",
            f"source {shlex.quote(str(models_copy))}",
        )
    )
    script_copy.chmod(script_copy.stat().st_mode | stat.S_IXUSR)

    plugin_test = bin_dir / "omh-agent-opencode-plugin-status-test"
    plugin_test.write_text("#!/usr/bin/env bash\nexit 0\n")
    plugin_test.chmod(plugin_test.stat().st_mode | stat.S_IXUSR)

    fake_timeout = bin_dir / "timeout"
    fake_timeout.write_text("#!/usr/bin/env bash\nshift\nexec \"$@\"\n")
    fake_timeout.chmod(fake_timeout.stat().st_mode | stat.S_IXUSR)

    attempts_log = tmp_path / "opencode-attempts.txt"
    fake_opencode = bin_dir / "opencode"
    fake_opencode.write_text(
        textwrap.dedent(
            f"""\
            #!/usr/bin/env python3
            import json
            import os
            import socket
            import sys

            args = sys.argv[1:]
            model = args[args.index("--model") + 1]
            title = args[args.index("--title") + 1]
            scenarios = {{
                "omh-opencode-status-working-idle": ("pane-opencode-allowed", ["working"]),
                "omh-opencode-status-blocked": ("pane-opencode-blocked", ["working", "blocked"]),
                "omh-opencode-status-subagent": ("pane-opencode-subagent", ["working"]),
            }}
            pane_id, states = scenarios[title]
            if pane_id == "pane-opencode-allowed":
                with open({str(attempts_log)!r}, "a", encoding="utf-8") as attempts:
                    attempts.write(model + "\\n")

            source = "wrong:source" if model == "openrouter/omh-broken" else "omh:opencode"
            session_id = f"{{model}}-{{pane_id}}"
            requests = [{{
                "id": 1,
                "method": "pane.report_agent_session",
                "params": {{
                    "pane_id": pane_id,
                    "agent_session_id": session_id,
                }},
            }}]
            for seq, state in enumerate(states, start=1):
                requests.append({{
                    "id": seq + 1,
                    "method": "pane.report_agent",
                    "params": {{
                        "pane_id": pane_id,
                        "source": source,
                        "agent": "opencode",
                        "seq": seq,
                        "state": state,
                        "agent_session_id": session_id,
                    }},
                }})

            for request in requests:
                with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
                    client.connect(os.environ["OMH_SOCKET_PATH"])
                    client.sendall((json.dumps(request) + "\\n").encode())
                    client.recv(4096)

            if pane_id == "pane-opencode-allowed":
                print("OMH_OPENCODE_STATUS_WORKING")
                if model != "openrouter/noncompliant":
                    print("OMH_OPENCODE_STATUS_IDLE")
            elif pane_id == "pane-opencode-subagent":
                print("OMH_OPENCODE_SUBAGENT_OK")
                print("OMH_OPENCODE_SUBAGENT_DONE")
            else:
                print("OMH_OPENCODE_STATUS_BLOCKED")
            """
        )
    )
    fake_opencode.chmod(fake_opencode.stat().st_mode | stat.S_IXUSR)

    env = {
        **os.environ,
        "PATH": f"{bin_dir}{os.pathsep}{os.environ['PATH']}",
        "OMH_REPO_DIR": str(repo_root),
        "TMPDIR": str(tmp_path),
        "OMH_OPENCODE_STATUS_TEST_TIMEOUT": "5",
        "OMH_OPENCODE_TEST_MODEL": primary_model,
        "OMH_TEST_FALLBACK_MODELS": fallback_model,
    }
    result = subprocess.run(
        [str(script_copy)],
        cwd=repo_root,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=20,
    )
    attempts = attempts_log.read_text().splitlines() if attempts_log.exists() else []
    return temp_dir, result, attempts


class OpenCodeStatusTestFallbackTests(unittest.TestCase):
    def test_retries_another_free_model_when_the_first_omits_completion_marker(self):
        temp_dir, result, attempts = run_opencode_status_test(
            "openrouter/noncompliant", "openrouter/ok"
        )
        self.addCleanup(temp_dir.cleanup)

        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, 0, output)
        self.assertEqual(attempts, ["openrouter/noncompliant", "openrouter/ok"], output)
        self.assertIn(
            "retrying after provider/model failure: openrouter/noncompliant", output
        )
        self.assertIn("opencode status test ok:", output)

    def test_does_not_retry_omh_status_assertion_failures(self):
        temp_dir, result, attempts = run_opencode_status_test(
            "openrouter/omh-broken", "openrouter/ok"
        )
        self.addCleanup(temp_dir.cleanup)

        output = result.stdout + result.stderr
        self.assertNotEqual(result.returncode, 0, output)
        self.assertEqual(attempts, ["openrouter/omh-broken"], output)
        self.assertNotIn("trying test model: openrouter/ok", output)
        self.assertIn("wrong:source", output)


if __name__ == "__main__":
    unittest.main()
