import subprocess
import unittest
from pathlib import Path


class TestModelCandidateTests(unittest.TestCase):
    def test_openrouter_api_candidates_preserve_openrouter_models_and_strip_outer_provider_prefix(self):
        candidates = self.run_candidate_function(
            "hako_test_openrouter_api_candidates",
            [
                "openrouter/free",
                "openrouter/owl-alpha",
                "openrouter/nvidia/foo",
            ],
        )

        self.assertEqual(
            candidates,
            [
                "openrouter/free",
                "openrouter/owl-alpha",
                "nvidia/foo",
            ],
        )

    def test_opencode_candidates_double_prefix_openrouter_owned_models(self):
        candidates = self.run_candidate_function(
            "hako_test_opencode_candidates",
            [
                "openrouter/free",
                "openrouter/owl-alpha",
            ],
        )

        self.assertEqual(
            candidates,
            [
                "openrouter/openrouter/free",
                "openrouter/openrouter/owl-alpha",
            ],
        )

    def run_candidate_function(self, function_name, models):
        repo_root = Path(__file__).resolve().parents[1]
        source_script = repo_root / "ci" / "agent-tests" / "test-models.sh"
        input_models = "\n".join(models) + "\n"

        result = subprocess.run(
            [
                "bash",
                "-c",
                "set -euo pipefail; source \"$1\"; \"$2\"",
                "bash",
                str(source_script),
                function_name,
            ],
            input=input_models,
            text=True,
            capture_output=True,
            check=False,
        )

        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, 0, output)
        return result.stdout.splitlines()


if __name__ == "__main__":
    unittest.main()
