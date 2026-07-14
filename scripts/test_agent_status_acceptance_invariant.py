import re
import unittest
from pathlib import Path


class AgentTestStatusAcceptanceInvariantTests(unittest.TestCase):
    provider_failure_terms = re.compile(
        r"\b(?:pricing|entitlement|forbidden)\b|"
        r"provider[^\n]*(?:failure|error|unavailable)|"
        r"(?:failure|error|unavailable)[^\n]*provider",
        re.IGNORECASE,
    )
    skip_terms = re.compile(r"\bskip(?:ped|ping)?\b", re.IGNORECASE)
    success_terms = re.compile(r"\b(?:exit|return)\s+0\b")

    def test_status_tests_do_not_successfully_skip_provider_acceptance_failures(self):
        repo_root = Path(__file__).resolve().parents[1]
        failures = []

        for script in sorted((repo_root / "ci" / "agent-test").glob("*status-test.sh")):
            executable_lines = [
                (line_number, line.strip())
                for line_number, line in enumerate(script.read_text().splitlines(), start=1)
                if line.strip() and not line.lstrip().startswith("#")
            ]

            for window_start, (line_number, line) in enumerate(executable_lines):
                if not self.success_terms.search(line):
                    continue

                window = executable_lines[max(0, window_start - 6) : window_start + 3]
                window_text = "\n".join(line for _, line in window)
                if not self.skip_terms.search(window_text):
                    continue
                if not self.provider_failure_terms.search(window_text):
                    continue

                excerpt = "\n".join(
                    f"{script.relative_to(repo_root)}:{window_line_number}: {window_line}"
                    for window_line_number, window_line in window
                )
                failures.append(
                    f"{script.relative_to(repo_root)}:{line_number}: provider/pricing/entitlement "
                    f"failure is described as a skip and can return success before status "
                    f"assertions:\n{excerpt}"
                )

        self.assertEqual([], failures)


if __name__ == "__main__":
    unittest.main()
