import os
import shlex
import stat
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path


class QoderProxyStatusSmokeValidationTests(unittest.TestCase):
    def test_forbidden_pricing_response_skips_successfully(self):
        result = self.run_smoke_with_fake_qodercli("entitlement_forbidden")

        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, 0, output)
        self.assertIn(
            "qoder proxy status smoke skipped: Qoder token lacks required entitlement",
            output,
        )

    def test_missing_marker_output_still_fails(self):
        result = self.run_smoke_with_fake_qodercli("missing_marker")

        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, 75, output)
        self.assertIn("qoder proxy smoke did not return expected marker", output)
        self.assertIn("ordinary qoder output without expected marker", output)

    def run_smoke_with_fake_qodercli(self, scenario):
        repo_root = Path(__file__).resolve().parents[1]
        source_script = repo_root / "ci" / "agent-smoke" / "qoder-proxy-status-smoke.sh"

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            bin_dir = tmp_path / "bin"
            lib_dir = tmp_path / "lib"
            smoke_dir = tmp_path / "smoke"
            repo_dir = tmp_path / "repo"
            hook_dir = repo_dir / "src" / "integration" / "assets" / "qodercli"
            bin_dir.mkdir()
            lib_dir.mkdir()
            smoke_dir.mkdir()
            hook_dir.mkdir(parents=True)

            models_stub = lib_dir / "hako-agent-smoke-models.sh"
            models_stub.write_text("# test stub: HAKO_SMOKE_ACTIVE_MODEL bypasses fallback lookup\n")

            script_copy = bin_dir / "hako-agent-smoke-qoder-proxy-status"
            script_text = source_script.read_text()
            script_copy.write_text(
                script_text.replace(
                    "source /usr/local/lib/hako-agent-smoke-models.sh",
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
                    #!/usr/bin/env bash
                    set -euo pipefail
                    case "$HAKO_TEST_QODER_SCENARIO" in
                      entitlement_forbidden)
                        cat <<'JSON'
                    {"type":"result","subtype":"error","errors":["Qoder API error: FORBIDDEN - Access denied {\\"pricingUrl\\":\\"https://qoder.com/pricing\\"}"]}
                    JSON
                        ;;
                      missing_marker)
                        echo "ordinary qoder output without expected marker"
                        ;;
                      *)
                        echo "unexpected HAKO_TEST_QODER_SCENARIO=$HAKO_TEST_QODER_SCENARIO" >&2
                        exit 64
                        ;;
                    esac
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
                "HAKO_QODER_PROXY_STATUS_SMOKE_DIR": str(smoke_dir),
                "HAKO_QODER_PROXY_STATUS_SMOKE_TIMEOUT": "5",
                "HAKO_SMOKE_ACTIVE_MODEL": "test-model",
                "HAKO_TEST_QODER_SCENARIO": scenario,
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
