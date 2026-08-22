import os
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path


class MakiStatusFallbackTests(unittest.TestCase):
    def test_provider_failure_requests_the_next_candidate(self):
        repo_root = Path(__file__).resolve().parents[1]
        source_script = repo_root / "ci" / "agent-tests" / "maki-status-test.sh"
        model_lib = repo_root / "ci" / "agent-tests" / "test-models.sh"

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            bin_dir = tmp_path / "bin"
            home_dir = tmp_path / "home"
            test_dir = tmp_path / "test"
            bin_dir.mkdir()
            home_dir.mkdir()
            test_dir.mkdir()

            fake_maki = bin_dir / "maki"
            fake_maki.write_text(
                "#!/usr/bin/env bash\n"
                "echo 'OpenRouter provider error: HTTP 429 no endpoint available' >&2\n"
                "exit 1\n"
            )
            fake_maki.chmod(fake_maki.stat().st_mode | stat.S_IXUSR)

            result = subprocess.run(
                [str(source_script)],
                cwd=repo_root,
                env={
                    **os.environ,
                    "PATH": f"{bin_dir}{os.pathsep}{os.environ['PATH']}",
                    "HOME": str(home_dir),
                    "OPENROUTER_API_KEY": "sk-test",
                    "GARDN_AGENT_TEST_MODELS_LIB": str(model_lib),
                    "GARDN_TEST_ACTIVE_MODEL": "vendor/unavailable",
                    "GARDN_MAKI_STATUS_TEST_DIR": str(test_dir),
                    "GARDN_MAKI_WORKDIR": str(test_dir),
                },
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=20,
            )

        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, 75, output)
        self.assertIn("HTTP 429 no endpoint available", output)
        self.assertIn("retryable Maki/OpenRouter provider failure", output)


if __name__ == "__main__":
    unittest.main()
