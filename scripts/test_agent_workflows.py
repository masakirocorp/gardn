import json
import os
import re
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


TARGETS = [
    "opencode",
    "pi",
    "omp",
    "claude",
    "codex",
    "copilot",
    "cursor",
    "qoder",
    "devin",
    "droid",
    "kimi",
    "hermes",
    "maki",
    "qwen",
    "kilo",
    "qwen-deterministic",
    "kilo-deterministic",
    "mastracode",
    "amp",
    "amp-deterministic",
    "antigravity",
    "antigravity-gemini",
]


class AgentTestWorkflowTests(unittest.TestCase):
    def setUp(self):
        self.repo_root = Path(__file__).resolve().parents[1]
        self.bash = shutil.which("bash")
        if self.bash is None:
            self.skipTest("bash is required")

    def target_matrix(self, target):
        result = subprocess.run(
            [self.bash, "ci/agent-tests/target-matrix.sh", target],
            cwd=self.repo_root,
            check=True,
            capture_output=True,
            text=True,
        )
        return json.loads(result.stdout)

    def test_live_test_matrix_is_never_empty(self):
        self.assertEqual(TARGETS, self.target_matrix("all"))
        for target in TARGETS:
            with self.subTest(target=target):
                self.assertEqual([target], self.target_matrix(target))

    def test_live_workflow_requires_the_matrix_to_pass(self):
        workflow = (self.repo_root / ".github/workflows/live-agent-tests.yml").read_text()

        self.assertIn("name: Live Agent Tests", workflow)
        self.assertIn("branches: [master]", workflow)
        self.assertIn("target: ${{ fromJSON(needs.plan.outputs.targets) }}", workflow)
        self.assertIn("needs: [plan, image, test]", workflow)
        self.assertIn("TEST_RESULT: ${{ needs.test.result }}", workflow)
        self.assertIn('test "$TEST_RESULT" = success', workflow)
        self.assertNotIn("type: boolean", workflow)

        cursor_hosts = [
            "api2.cursor.sh",
            "api2geo.cursor.sh",
            "api2direct.cursor.sh",
            "agentn.api5.cursor.sh",
            "agent.api5.cursor.sh",
        ]
        for host in cursor_hosts:
            self.assertIn(f"--add-host {host}:127.0.0.1", workflow)

        matrix = (self.repo_root / "ci/agent-tests/target-matrix.sh").read_text()
        for target in ("qwen", "kilo"):
            self.assertIn(f"          - {target}", workflow)
            self.assertIn(f'"{target}"', matrix)
            self.assertIn(f"gardn-agent-tests-{target}-status", (self.repo_root / "ci/agent-tests/run-target.sh").read_text())

    def test_release_reuses_fork_safe_workflows_before_publication(self):
        ci_workflow = (self.repo_root / ".github/workflows/ci.yml").read_text()
        fixture_workflow = (
            self.repo_root / ".github/workflows/agent-tests.yml"
        ).read_text()
        release_workflow = (
            self.repo_root / ".github/workflows/release.yml"
        ).read_text()

        for workflow in (ci_workflow, fixture_workflow):
            self.assertIn("  pull_request:", workflow)
            self.assertIn("  workflow_call:", workflow)
            self.assertNotIn("pull_request_target:", workflow)
            self.assertIn("permissions:\n  contents: read", workflow)
            self.assertEqual(
                workflow.count("uses: actions/checkout@"),
                workflow.count("persist-credentials: false"),
            )

        self.assertNotIn("${{ secrets.", fixture_workflow)
        self.assertRegex(
            release_workflow,
            r"(?m)^  required-ci:\n    uses: \./\.github/workflows/ci\.yml$",
        )
        self.assertRegex(
            release_workflow,
            r"(?m)^  agent-fixtures:\n    uses: \./\.github/workflows/agent-tests\.yml$",
        )
        self.assertNotIn("secrets: inherit", release_workflow)

        publication = re.search(
            r"(?m)^  release:\n    needs: \[([^\]]+)\]$",
            release_workflow,
        )
        self.assertIsNotNone(publication)
        self.assertEqual(
            {
                dependency.strip()
                for dependency in publication.group(1).split(",")
            },
            {"build", "flake-check", "macos-app", "required-ci", "agent-fixtures"},
        )


    def test_qwen_and_kilo_image_contract_is_complete(self):
        dockerfile = (self.repo_root / "ci/agent-tests/Dockerfile").read_text()
        doctor = (self.repo_root / "ci/agent-tests/doctor.sh").read_text()
        model_helpers = (self.repo_root / "ci/agent-tests/test-models.sh").read_text()
        fixture_workflow = (
            self.repo_root / ".github/workflows/agent-tests.yml"
        ).read_text()
        dockerignore = (self.repo_root / "ci/agent-tests/.dockerignore").read_text()
        expected = {
            "qwen": ("@qwen-code/qwen-code", "QWEN_CODE_VERSION"),
            "kilo": ("@kilocode/cli", "KILO_VERSION"),
        }
        for target, (package, build_arg) in expected.items():
            script = (self.repo_root / f"ci/agent-tests/{target}-status-test.sh").read_text()
            self.assertIn(package, dockerfile)
            self.assertIn(f"ARG {build_arg}", dockerfile)
            self.assertIn(f"COPY {target}-status-test.sh /usr/local/bin/gardn-agent-tests-{target}-status", dockerfile)
            self.assertIn(f"gardn-agent-tests-{target}-status", dockerfile)
            self.assertIn(target, doctor)
            self.assertIn("gardn_test_run_with_fallbacks", script)
            self.assertIn("exit 75", script)
            self.assertIn(f"!{target}-status-test.sh", dockerignore)
            self.assertIn(
                f"echo \"{build_arg}=$(jq -r '.build_args.{build_arg}'",
                fixture_workflow,
            )
            self.assertIn(
                f"{build_arg}: ${{{{ steps.cohort.outputs.{build_arg} }}}}",
                fixture_workflow,
            )
            self.assertIn(f'--build-arg "{build_arg}=', fixture_workflow)
        self.assertIn('export OPENAI_BASE_URL="$openrouter_base"', model_helpers)
        self.assertIn('export KILO_AUTH_CONTENT="$OPENCODE_AUTH_CONTENT"', model_helpers)
        self.assertIn('"@qwen-code/audio-capture": false', dockerfile)
        self.assertIn('"esbuild": true', dockerfile)


    def test_grouped_runner_can_isolate_each_agent(self):
        script = self.repo_root / "ci/agent-tests/remaining-status-test.sh"
        grouped_targets = ["copilot", "qoder", "cursor", "devin", "droid", "kimi", "hermes"]

        with tempfile.TemporaryDirectory() as tmp:
            for target in grouped_targets:
                with self.subTest(target=target):
                    env = os.environ.copy()
                    env.update(
                        {
                            "GARDN_REMAINING_STATUS_TARGET": target,
                            "GARDN_REMAINING_STATUS_SEAM_ONLY": "1",
                            "GARDN_REPO_DIR": str(self.repo_root),
                            "GARDN_REMAINING_STATUS_TEST_DIR": str(Path(tmp) / target),
                        }
                    )
                    result = subprocess.run(
                        [self.bash, script],
                        cwd=self.repo_root,
                        env=env,
                        check=True,
                        capture_output=True,
                        text=True,
                    )
                    self.assertIn(f"target={target}; mode=seam", result.stdout)


if __name__ == "__main__":
    unittest.main()
