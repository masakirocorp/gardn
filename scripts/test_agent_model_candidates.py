import json
import os
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path


class TestModelCandidateTests(unittest.TestCase):
    def setUp(self):
        self.repo_root = Path(__file__).resolve().parents[1]
        self.source_script = self.repo_root / "ci" / "agent-tests" / "test-models.sh"

    def test_canonical_candidates_are_validated_and_deduplicated(self):
        result = self.run_shell(
            'omh_test_unique_candidates "$1" "$2"',
            "openrouter/free",
            "nvidia/example:free,openrouter/free",
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            result.stdout.splitlines(),
            ["openrouter/free", "nvidia/example:free"],
        )

    def test_cli_prefixed_model_is_rejected_at_the_public_input_boundary(self):
        result = self.run_shell(
            'omh_test_unique_candidates "$1" ""',
            "openrouter/openrouter/free",
        )

        self.assertEqual(result.returncode, 64)
        self.assertIn("invalid canonical OpenRouter model id", result.stderr)

    def test_provider_adapter_prefixes_the_canonical_model_once(self):
        result = self.run_shell('omh_test_provider_model "$1"', "openrouter/free")

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "openrouter/openrouter/free")

    def test_catalog_preflight_skips_removed_models_and_preserves_order(self):
        with tempfile.TemporaryDirectory() as tmp:
            catalog = Path(tmp) / "models.json"
            catalog.write_text(
                json.dumps(
                    {
                        "data": [
                            {"id": "openrouter/free"},
                            {"id": "nvidia/example:free"},
                        ]
                    }
                )
            )
            result = self.run_shell(
                "omh_test_available_candidates",
                input_text="removed/model\nopenrouter/free\nnvidia/example:free\n",
                env={"OPENROUTER_MODELS_URL": catalog.as_uri()},
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            result.stdout.splitlines(),
            ["openrouter/free", "nvidia/example:free"],
        )
        self.assertIn("skip model removed/model", result.stderr)

    def test_fallback_requires_an_explicit_retry_outcome(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            attempts = tmp_path / "attempts"
            target = tmp_path / "target.sh"
            target.write_text(
                "#!/usr/bin/env bash\n"
                "printf '%s\\n' \"$OMH_TEST_ACTIVE_MODEL\" >> \"$ATTEMPTS\"\n"
                "echo 'provider error: no endpoint available' >&2\n"
                "exit 1\n"
            )
            target.chmod(target.stat().st_mode | stat.S_IXUSR)
            result = self.run_shell(
                'printf "%s\\n" first/model second/model | omh_test_run_with_fallbacks "$1"',
                str(target),
                env={"ATTEMPTS": str(attempts)},
            )
            attempted_models = attempts.read_text().splitlines()

        self.assertEqual(result.returncode, 1)
        self.assertEqual(attempted_models, ["first/model"])

    def test_fallback_reconfigures_and_runs_the_next_explicit_candidate(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            attempts = tmp_path / "attempts"
            target = tmp_path / "target.sh"
            target.write_text(
                "#!/usr/bin/env bash\n"
                "printf '%s\\n' \"$OMH_TEST_ACTIVE_MODEL\" >> \"$ATTEMPTS\"\n"
                "[[ \"$OMH_TEST_ACTIVE_MODEL\" == first/model ]] && exit 75\n"
                "exit 0\n"
            )
            target.chmod(target.stat().st_mode | stat.S_IXUSR)
            result = self.run_shell(
                'printf "%s\\n" first/model second/model | omh_test_run_with_fallbacks "$1"',
                str(target),
                env={"ATTEMPTS": str(attempts)},
            )
            attempted_models = attempts.read_text().splitlines()

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(attempted_models, ["first/model", "second/model"])

    def test_attempt_configuration_rewrites_every_model_bearing_surface(self):
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp) / "home"
            home.mkdir()
            env = {
                "HOME": str(home),
                "OPENROUTER_API_KEY": "sk-test",
                "FACTORY_HOME": str(home / "factory"),
                "KIMI_CODE_HOME": str(home / "kimi"),
                "CODEX_HOME": str(home / "codex"),
            }
            result = self.run_shell(
                'omh_test_configure_model "$1"; omh_test_configure_model "$2"',
                "removed/model",
                "openrouter/free",
                env=env,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(
                json.loads((home / "factory" / "settings.json").read_text())["model"],
                "openrouter/free",
            )
            for path in [
                home / "codex" / "config.toml",
                home / "kimi" / "config.toml",
                home / ".hermes" / "config.yaml",
            ]:
                contents = path.read_text()
                self.assertIn("openrouter/free", contents)
                self.assertNotIn("removed/model", contents)

    def run_shell(self, command, *args, input_text=None, env=None):
        return subprocess.run(
            [
                "bash",
                "-c",
                f'set -euo pipefail; source "$1"; shift; {command}',
                "bash",
                str(self.source_script),
                *args,
            ],
            input=input_text,
            text=True,
            capture_output=True,
            check=False,
            env={**os.environ, **(env or {})},
        )


if __name__ == "__main__":
    unittest.main()
