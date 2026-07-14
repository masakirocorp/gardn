import os
import shlex
import stat
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path


class QoderProxyStatusTestValidationTests(unittest.TestCase):
    def test_openrouter_proxy_routes_only_inference_requests(self):
        repo_root = Path(__file__).resolve().parents[1]
        result = subprocess.run(
            ["node", str(repo_root / "ci" / "agent-tests" / "qoder-openrouter-proxy-test.mjs")],
            cwd=repo_root,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )

        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, 0, output)
        self.assertIn("qoder proxy inference URL matching test ok", output)

    def test_forbidden_pricing_response_is_retryable_status_acceptance_failure(self):
        result = self.run_test_with_fake_qodercli("entitlement_forbidden")

        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, 75, output)
        self.assertIn("qoder proxy test did not return expected marker", output)
        self.assertIn("Qoder API error: FORBIDDEN", output)
        self.assertIn("pricingUrl", output)
        self.assertNotIn("status test skipped", output)

    def test_missing_marker_output_still_fails(self):
        result = self.run_test_with_fake_qodercli("missing_marker")

        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, 75, output)
        self.assertIn("qoder proxy test did not return expected marker", output)
        self.assertIn("ordinary qoder output without expected marker", output)

    def test_default_qoder_cli_model_is_documented_efficient(self):
        result = self.run_test_with_fake_qodercli(
            "success",
            expected_qoder_cli_model="efficient",
        )

        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, 0, output)
        self.assertIn(
            "qoder proxy status test ok: real Qoder CLI completed through deterministic local proxy",
            output,
        )

    def run_test_with_fake_qodercli(self, scenario, expected_qoder_cli_model="efficient"):
        repo_root = Path(__file__).resolve().parents[1]
        source_script = repo_root / "ci" / "agent-tests" / "qoder-proxy-status-test.sh"

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            bin_dir = tmp_path / "bin"
            lib_dir = tmp_path / "lib"
            test_dir = tmp_path / "test"
            repo_dir = tmp_path / "repo"
            hook_dir = repo_dir / "apps" / "hako" / "src" / "integration" / "assets" / "qodercli"
            bin_dir.mkdir()
            lib_dir.mkdir()
            test_dir.mkdir()
            hook_dir.mkdir(parents=True)

            models_stub = lib_dir / "hako-agent-test-models.sh"
            models_stub.write_text("# test stub: HAKO_TEST_ACTIVE_MODEL bypasses fallback lookup\n")

            script_copy = bin_dir / "hako-agent-tests-qoder-proxy-status"
            script_text = source_script.read_text()
            script_copy.write_text(
                script_text.replace(
                    "source /usr/local/lib/hako-agent-test-models.sh",
                    f"source {shlex.quote(str(models_stub))}",
                )
            )
            script_copy.chmod(script_copy.stat().st_mode | stat.S_IXUSR)

            hook_path = hook_dir / "hako-agent-state.sh"
            hook_path.write_text("#!/usr/bin/env bash\nexit 0\n")
            hook_path.chmod(hook_path.stat().st_mode | stat.S_IXUSR)

            fake_id = bin_dir / "id"
            fake_id.write_text(
                textwrap.dedent(
                    """\
                    #!/usr/bin/env bash
                    set -euo pipefail
                    if [[ "${1:-}" == "-u" ]]; then
                      echo 0
                      exit 0
                    fi
                    /usr/bin/id "$@"
                    """
                )
            )
            fake_id.chmod(fake_id.stat().st_mode | stat.S_IXUSR)

            fake_getent = bin_dir / "getent"
            fake_getent.write_text(
                textwrap.dedent(
                    """\
                    #!/usr/bin/env bash
                    set -euo pipefail
                    if [[ "${1:-}" == "hosts" && "${2:-}" == "api1.qoder.sh" ]]; then
                      echo "127.0.0.1 api1.qoder.sh"
                      exit 0
                    fi
                    exit 2
                    """
                )
            )
            fake_getent.chmod(fake_getent.stat().st_mode | stat.S_IXUSR)

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

            fake_node = bin_dir / "node"
            fake_node.write_text(
                textwrap.dedent(
                    """\
                    #!/usr/bin/env python3
                    import os
                    import signal
                    import time

                    with open(os.environ["HAKO_QODER_PROXY_LOG"], "a", encoding="utf-8") as log:
                        log.write("qoder-proxy-listening\\n")
                        log.flush()

                    def raise_system_exit():
                        raise SystemExit(0)

                    signal.signal(signal.SIGTERM, lambda signum, frame: raise_system_exit())

                    while True:
                        time.sleep(60)
                    """
                )
            )
            fake_node.chmod(fake_node.stat().st_mode | stat.S_IXUSR)

            fake_qodercli = bin_dir / "qodercli"
            fake_qodercli.write_text(
                textwrap.dedent(
                    """\
                    #!/usr/bin/env python3
                    import json
                    import os
                    import socket
                    import sys


                    def value_after(flag):
                        try:
                            index = sys.argv.index(flag)
                        except ValueError:
                            return None
                        if index + 1 >= len(sys.argv):
                            print(f"missing value after {flag}: {sys.argv!r}", file=sys.stderr)
                            sys.exit(64)
                        return sys.argv[index + 1]


                    def rpc(method, params):
                        request = {
                            "id": f"test:{method}:{params.get('state', 'release')}",
                            "method": method,
                            "params": params,
                        }
                        client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                        client.settimeout(1)
                        try:
                            client.connect(os.environ["HAKO_SOCKET_PATH"])
                            client.sendall((json.dumps(request) + "\\n").encode("utf-8"))
                            client.recv(4096)
                        finally:
                            client.close()


                    scenario = os.environ["HAKO_TEST_QODER_SCENARIO"]
                    if os.environ.get("QODER_MODEL_TRANSPORT") != "http":
                        print(
                            f"expected QODER_MODEL_TRANSPORT='http', observed {os.environ.get('QODER_MODEL_TRANSPORT')!r}",
                            file=sys.stderr,
                        )
                        sys.exit(65)

                    if os.environ.get("QODER_MODEL_SERVER_HOST") != "localhost":
                        print(
                            f"expected QODER_MODEL_SERVER_HOST='localhost', observed {os.environ.get('QODER_MODEL_SERVER_HOST')!r}",
                            file=sys.stderr,
                        )
                        sys.exit(65)

                    if scenario == "entitlement_forbidden":
                        print(
                            '{"type":"result","subtype":"error","errors":["Qoder API error: FORBIDDEN - Access denied {\\\\"pricingUrl\\\\":\\\\"https://qoder.com/pricing\\\\"}"]}'
                        )
                        sys.exit(0)

                    if scenario == "missing_marker":
                        print("ordinary qoder output without expected marker")
                        sys.exit(0)

                    if scenario == "success":
                        expected_model = os.environ["HAKO_EXPECTED_QODER_CLI_MODEL"]
                        actual_model = value_after("--model")
                        if actual_model != expected_model:
                            print(
                                f"expected --model {expected_model!r}, observed {actual_model!r} in {sys.argv!r}",
                                file=sys.stderr,
                            )
                            sys.exit(65)

                        with open("qoder-proxy.log", "a", encoding="utf-8") as log:
                            log.write("request POST /model/v1/chat/completions bytes=75815\\nstatic-complete\\n")

                        base_params = {
                            "pane_id": os.environ["HAKO_PANE_ID"],
                            "source": "hako:qodercli",
                            "agent": "qodercli",
                            "seq": 1,
                        }
                        rpc("pane.report_agent", {**base_params, "state": "working"})
                        rpc("pane.report_agent", {**base_params, "seq": 2, "state": "idle"})
                        rpc("pane.release_agent", {**base_params, "seq": 3})
                        print("HAKO_QODER_PROXY_OK")
                        sys.exit(0)

                    print(f"unexpected HAKO_TEST_QODER_SCENARIO={scenario}", file=sys.stderr)
                    sys.exit(64)
                    """
                )
            )
            fake_qodercli.chmod(fake_qodercli.stat().st_mode | stat.S_IXUSR)

            env = {
                **os.environ,
                "PATH": f"{bin_dir}{os.pathsep}{os.environ['PATH']}",
                "OPENROUTER_API_KEY": "sk-test-fake-openrouter-key",
                "QODER_PERSONAL_ACCESS_TOKEN": "qoder-test-token",
                "HAKO_REPO_DIR": str(repo_dir),
                "HAKO_QODER_PROXY_STATUS_TEST_DIR": str(test_dir),
                "HAKO_QODER_PROXY_STATUS_TEST_TIMEOUT": "5",
                "HAKO_TEST_ACTIVE_MODEL": "test-model",
                "HAKO_TEST_QODER_SCENARIO": scenario,
                "HAKO_EXPECTED_QODER_CLI_MODEL": expected_qoder_cli_model,
            }
            env.pop("HAKO_TEST_QODER_CLI_MODEL", None)

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
