import os
import shlex
import stat
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path


class CodexStatusTestFallbackTests(unittest.TestCase):
    def test_retries_when_codex_exec_writes_retryable_provider_output_before_exiting(self):
        repo_root = Path(__file__).resolve().parents[1]
        source_script = repo_root / "ci" / "agent-tests" / "codex-status-test.sh"
        source_models = repo_root / "ci" / "agent-tests" / "test-models.sh"

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            bin_dir = tmp_path / "bin"
            lib_dir = tmp_path / "lib"
            test_dir = tmp_path / "test"
            bin_dir.mkdir()
            lib_dir.mkdir()
            test_dir.mkdir()

            models_copy = lib_dir / "omh-agent-test-models.sh"
            models_copy.write_text(source_models.read_text())

            script_copy = bin_dir / "omh-agent-tests-codex-status"
            script_text = source_script.read_text()
            script_copy.write_text(
                script_text.replace(
                    "source /usr/local/lib/omh-agent-test-models.sh",
                    f"source {shlex.quote(str(models_copy))}",
                )
            )
            script_copy.chmod(script_copy.stat().st_mode | stat.S_IXUSR)

            attempts_log = tmp_path / "codex-attempts.txt"
            fake_codex = bin_dir / "codex"
            fake_codex.write_text(
                textwrap.dedent(
                    f"""\
                    #!/usr/bin/env bash
                    set -euo pipefail
                    if [[ "${{1:-}}" != "exec" ]]; then
                      echo "unexpected codex invocation: $*" >&2
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
                    printf '%s\\n' "$model" >> {shlex.quote(str(attempts_log))}
                    case "$model" in
                      anthropic/overloaded)
                        echo "OpenRouter provider failure: 429 no endpoint available" >&2
                        exit 1
                        ;;
                      anthropic/ok)
                        echo "provider: openrouter"
                        echo "OMH_CODEX_STATUS_OK"
                        exit 0
                        ;;
                      *)
                        echo "unexpected model: $model" >&2
                        exit 65
                        ;;
                    esac
                    """
                )
            )
            fake_codex.chmod(fake_codex.stat().st_mode | stat.S_IXUSR)

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
                "OPENROUTER_API_KEY": "sk-test-fake-openrouter-key",
                "OMH_REPO_DIR": str(repo_root),
                "OMH_CODEX_STATUS_TEST_DIR": str(test_dir),
                "OMH_CODEX_STATUS_TEST_TIMEOUT": "5",
                "OMH_TEST_MODEL": "openrouter/anthropic/overloaded",
                "OMH_TEST_FALLBACK_MODELS": "openrouter/anthropic/ok",
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

            output = result.stdout + result.stderr
            self.assertEqual(result.returncode, 0, output)
            self.assertEqual(
                attempts_log.read_text().splitlines(),
                ["anthropic/overloaded", "anthropic/ok"],
                output,
            )
            self.assertIn(
                "retrying after provider/model failure: anthropic/overloaded",
                output,
            )
            self.assertIn(
                "codex status test ok: OpenRouter real cli routes correctly",
                output,
            )


if __name__ == "__main__":
    unittest.main()
