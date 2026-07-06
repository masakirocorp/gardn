import os
import shlex
import stat
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path


class PiOmpStatusSmokeValidationTests(unittest.TestCase):
    def test_subagent_child_session_path_under_parent_session_stem_is_accepted(self):
        result = self.run_smoke_with_subagent_session_variant("child")

        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, 0, output)
        self.assertIn(
            "pi/omp status test ok: real cli reports session root identity",
            output,
        )

    def test_openrouter_free_active_model_is_passed_to_agents_unchanged(self):
        result = self.run_smoke_with_subagent_session_variant(
            "child",
            active_model="openrouter/openrouter/free",
        )

        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, 0, output)
        self.assertIn(
            "pi/omp status test ok: real cli reports session root identity",
            output,
        )

    def test_subagent_unrelated_second_session_path_is_rejected(self):
        result = self.run_smoke_with_subagent_session_variant("unrelated")

        output = result.stdout + result.stderr
        self.assertNotEqual(result.returncode, 0, output)
        self.assertIn("expected one session identity", output)
        self.assertIn("other.jsonl", output)

    def run_smoke_with_subagent_session_variant(self, variant, active_model="test-model"):
        repo_root = Path(__file__).resolve().parents[1]
        source_script = repo_root / "ci" / "agent-smoke" / "pi-omp-status-smoke.sh"

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            bin_dir = tmp_path / "bin"
            lib_dir = tmp_path / "lib"
            smoke_dir = tmp_path / "smoke"
            bin_dir.mkdir()
            lib_dir.mkdir()
            smoke_dir.mkdir()

            models_stub = lib_dir / "hako-agent-smoke-models.sh"
            models_stub.write_text("# test stub: HAKO_SMOKE_ACTIVE_MODEL bypasses fallback lookup\n")

            script_copy = bin_dir / "hako-agent-smoke-pi-omp-status"
            script_text = source_script.read_text()
            script_copy.write_text(
                script_text.replace(
                    "source /usr/local/lib/hako-agent-smoke-models.sh",
                    f"source {shlex.quote(str(models_stub))}",
                )
                .replace("${agent^^}_STATUS_OK", "${agent}_STATUS_OK")
                .replace("${agent^^}_SUBAGENT_OK", "${agent}_SUBAGENT_OK")
            )
            script_copy.chmod(script_copy.stat().st_mode | stat.S_IXUSR)

            fake_agent = textwrap.dedent(
                """\
                #!/usr/bin/env python3
                import json
                import os
                import socket
                import sys
                from pathlib import Path


                def rpc(method, params):
                    request = {"id": rpc.next_id, "method": method, "params": params}
                    rpc.next_id += 1
                    client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                    try:
                        client.connect(os.environ["HAKO_SOCKET_PATH"])
                        client.sendall((json.dumps(request) + "\\n").encode("utf-8"))
                        response = b""
                        while not response.endswith(b"\\n"):
                            chunk = client.recv(65536)
                            if not chunk:
                                break
                            response += chunk
                    finally:
                        client.close()


                rpc.next_id = 1

                def argv_value_after(flag):
                    if sys.argv.count(flag) != 1:
                        print(
                            f"expected exactly one {flag} argument, observed {sys.argv!r}",
                            file=sys.stderr,
                        )
                        sys.exit(65)
                    value_index = sys.argv.index(flag) + 1
                    if value_index >= len(sys.argv):
                        print(f"missing value for {flag} argument in {sys.argv!r}", file=sys.stderr)
                        sys.exit(65)
                    return sys.argv[value_index]


                agent = Path(sys.argv[0]).name
                expected_model = os.environ["HAKO_EXPECTED_ACTIVE_MODEL"]
                actual_model = argv_value_after("--model")
                if actual_model != expected_model:
                    print(
                        f"expected --model {expected_model!r}, observed {actual_model!r} in {sys.argv!r}",
                        file=sys.stderr,
                    )
                    sys.exit(65)
                pane_id = os.environ["HAKO_PANE_ID"]
                scenario = "subagent" if "subagent" in pane_id else "basic"
                session_root = Path(os.environ["HAKO_TEST_SESSION_ROOT"]) / agent / "sessions" / "project"
                parent_session = session_root / "parent.jsonl"
                second_report_session = parent_session

                if scenario == "subagent":
                    variant = os.environ["HAKO_TEST_SUBAGENT_SESSION_VARIANT"]
                    if variant == "child":
                        second_report_session = parent_session.with_suffix("") / "Child.jsonl"
                    elif variant == "unrelated":
                        second_report_session = session_root / "other.jsonl"
                    else:
                        print(f"unexpected test session variant: {variant}", file=sys.stderr)
                        sys.exit(64)

                launch_env = {
                    "PI_CONFIG_DIR": os.environ["PI_CONFIG_DIR"],
                    "PI_CODING_AGENT_DIR": os.environ["PI_CODING_AGENT_DIR"],
                }
                base_params = {
                    "pane_id": pane_id,
                    "source": f"hako:{agent}",
                    "agent": agent,
                    "launch_env": launch_env,
                }

                for seq, (state, session_path) in enumerate(
                    [("idle", parent_session), ("working", second_report_session)], start=1
                ):
                    rpc(
                        "pane.report_agent",
                        {
                            **base_params,
                            "seq": seq,
                            "state": state,
                            "agent_session_path": str(session_path),
                        },
                    )

                rpc(
                    "pane.release_agent",
                    {
                        **base_params,
                        "seq": 3,
                        "agent_session_path": str(parent_session),
                    },
                )

                marker_suffix = "SUBAGENT_OK" if scenario == "subagent" else "STATUS_OK"
                print(f"HAKO_{agent.upper()}_{marker_suffix}")
                """
            )
            for agent_name in ("pi", "omp"):
                fake_path = bin_dir / agent_name
                fake_path.write_text(fake_agent)
                fake_path.chmod(fake_path.stat().st_mode | stat.S_IXUSR)

            fake_timeout = bin_dir / "timeout"
            fake_timeout.write_text(
                textwrap.dedent(
                    """\
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
                "HAKO_REPO_DIR": str(repo_root),
                "HAKO_PI_OMP_STATUS_DIR": str(smoke_dir),
                "HAKO_PI_OMP_STATUS_TIMEOUT": "5",
                "HAKO_SMOKE_ACTIVE_MODEL": active_model,
                "HAKO_TEST_SESSION_ROOT": str(tmp_path / "run" / "agent"),
                "HAKO_TEST_SUBAGENT_SESSION_VARIANT": variant,
                "HAKO_EXPECTED_ACTIVE_MODEL": active_model,
            }

            return subprocess.run(
                [str(script_copy)],
                cwd=repo_root,
                env=env,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=20,
            )


if __name__ == "__main__":
    unittest.main()
