import json
import os
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
THREAD_ID = "T-00000000-0000-0000-0000-000000000001"


def run_harness(scenario):
    # Amp is the third-party process boundary. The harness, PTY, and RPC socket are real.
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        fake = root / "amp"
        fake.write_text(textwrap.dedent(r"""
            #!/usr/bin/env python3
            import json
            import os
            import socket
            import sys
            import tty
            from pathlib import Path

            thread_id = "T-00000000-0000-0000-0000-000000000001"
            marker = "GARDN_AMP_CI_OK"
            action = sys.argv[sys.argv.index("threads") + 1]
            state = Path("turn.json")
            turn = json.loads(state.read_text()) if state.exists() else 0
            scenario = os.environ["FAKE_AMP_SCENARIO"]
            if action == "new":
                print("https://ampcode.com/threads/" + thread_id)
                sys.exit(0)
            if action == "delete":
                Path(os.environ["FAKE_AMP_DELETED"]).write_text(sys.argv[-1])
                sys.exit(0)
            if action == "export":
                responses = ["GARDN_AMP_CI_OK", "GARDN_AMP_CI_OK_RESUMED"][:turn]
                if scenario == "stale-response" and turn == 2:
                    responses = responses[:1]
                print(json.dumps({"id": thread_id, "messages": [
                    {"role": "assistant", "content": [{"type": "text", "text": response}]}
                    for response in responses
                ]}))
                sys.exit(0)
            if turn == 1 and scenario != "stale-response":
                marker += "_RESUMED"

            tty.setraw(sys.stdin.fileno())
            seq = 0
            if scenario == "wrong-resume" and turn == 1:
                thread_id = "T-00000000-0000-0000-0000-000000000002"

            def rpc(method, **fields):
                global seq
                seq += 1
                params = {
                    "pane_id": "amp-status-test", "agent": "amp", "source": "gardn:amp",
                    "agent_session_id": thread_id, "seq": seq, **fields,
                }
                with socket.socket(socket.AF_UNIX) as client:
                    client.connect(os.environ["GARDN_SOCKET_PATH"])
                    client.sendall((json.dumps({"id": seq, "method": method, "params": params}) + "\n").encode())
                    client.recv(65536)

            rpc("pane.report_agent_session")
            rpc("pane.report_agent", state="idle")
            while os.read(0, 1) != b"\r":
                pass
            rpc("pane.report_agent", state="working")
            if scenario == "provider-error":
                rpc("pane.report_agent", state="blocked", message="provider error")
                sys.exit(1)
            print(marker, flush=True)
            rpc("pane.report_agent", state="idle")
            state.write_text(json.dumps(turn + 1))
            interrupts = 0
            while interrupts < 2:
                if os.read(0, 1) == b"\x03":
                    interrupts += 1
            if scenario != "missing-release":
                rpc("pane.release_agent")
            sys.exit(0)
        """).lstrip())
        fake.chmod(0o755)
        deleted = root / "deleted"
        result = subprocess.run(
            [sys.executable, REPO_ROOT / "ci/agent-tests/amp-status-test.py"],
            env={
                **os.environ,
                "PATH": f"{root}{os.pathsep}{os.environ['PATH']}",
                "AMP_API_KEY": "test-token",
                "GARDN_REPO_DIR": str(REPO_ROOT),
                "GARDN_AMP_STATUS_TIMEOUT": "2",
                "FAKE_AMP_SCENARIO": scenario,
                "FAKE_AMP_DELETED": str(deleted),
            },
            capture_output=True, text=True, timeout=15,
        )
        return result, deleted.read_text() if deleted.exists() else None


class AmpStatusValidationTests(unittest.TestCase):
    def test_accepts_completed_provider_turn_and_native_resume(self):
        result, deleted = run_harness("success")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("PASS amp native resume", result.stdout)
        self.assertEqual(deleted, THREAD_ID)

    def test_provider_error_cannot_pass_as_completion(self):
        result, deleted = run_harness("provider-error")
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("before the provider turn to complete", result.stderr)
        self.assertEqual(deleted, THREAD_ID)

    def test_resuming_a_different_native_thread_fails(self):
        result, deleted = run_harness("wrong-resume")
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("unexpected agent_session_id", result.stderr)
        self.assertEqual(deleted, THREAD_ID)

    def test_graceful_exit_without_releasing_ownership_fails(self):
        result, deleted = run_harness("missing-release")
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("did not release its native thread", result.stderr)
        self.assertEqual(deleted, THREAD_ID)

    def test_old_transcript_cannot_satisfy_the_resumed_turn(self):
        result, deleted = run_harness("stale-response")
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("expected assistant responses to be persisted", result.stderr)
        self.assertEqual(deleted, THREAD_ID)


if __name__ == "__main__":
    unittest.main()
