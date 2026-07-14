import json
import os
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

    def test_target_dispatcher_runs_exactly_one_agent(self):
        dispatcher = self.repo_root / "ci/agent-tests/run-target.sh"
        commands = {
            "opencode": "hako-agent-tests-opencode-status",
            "pi": "hako-agent-tests-pi-omp-status",
            "omp": "hako-agent-tests-pi-omp-status",
            "claude": "hako-agent-tests-claude-status",
            "codex": "hako-agent-tests-codex-status",
            "copilot": "hako-agent-tests-remaining-status",
            "cursor": "hako-agent-tests-cursor-proxy-status",
            "qoder": "hako-agent-tests-qoder-proxy-status",
            "devin": "hako-agent-tests-remaining-status",
            "droid": "hako-agent-tests-remaining-status",
            "kimi": "hako-agent-tests-remaining-status",
            "hermes": "hako-agent-tests-remaining-status",
            "maki": "hako-agent-tests-maki-status",
        }

        with tempfile.TemporaryDirectory() as tmp:
            bin_dir = Path(tmp) / "bin"
            output = Path(tmp) / "dispatch"
            bin_dir.mkdir()
            for command in set(commands.values()):
                fake = bin_dir / command
                fake.write_text(
                    "#!/bin/sh\n"
                    'printf "%s|%s|%s\\n" "$0" "${HAKO_PI_OMP_STATUS_TARGET:-}" '
                    '"${HAKO_REMAINING_STATUS_TARGET:-}" > "$OUTPUT"\n'
                )
                fake.chmod(0o755)

            for target, expected_command in commands.items():
                with self.subTest(target=target):
                    env = os.environ.copy()
                    env["PATH"] = f"{bin_dir}{os.pathsep}{env['PATH']}"
                    env["OUTPUT"] = str(output)
                    subprocess.run(
                        [self.bash, dispatcher, target],
                        cwd=self.repo_root,
                        env=env,
                        check=True,
                    )
                    command, pi_omp_target, remaining_target = output.read_text().strip().split("|")
                    self.assertEqual(expected_command, Path(command).name)
                    self.assertEqual(target if target in {"pi", "omp"} else "", pi_omp_target)
                    self.assertEqual(
                        target
                        if expected_command == "hako-agent-tests-remaining-status"
                        else "",
                        remaining_target,
                    )

    def test_grouped_runner_can_isolate_each_agent(self):
        script = self.repo_root / "ci/agent-tests/remaining-status-test.sh"
        grouped_targets = ["copilot", "qoder", "cursor", "devin", "droid", "kimi", "hermes"]

        with tempfile.TemporaryDirectory() as tmp:
            for target in grouped_targets:
                with self.subTest(target=target):
                    env = os.environ.copy()
                    env.update(
                        {
                            "HAKO_REMAINING_STATUS_TARGET": target,
                            "HAKO_REMAINING_STATUS_SEAM_ONLY": "1",
                            "HAKO_REPO_DIR": str(self.repo_root),
                            "HAKO_REMAINING_STATUS_TEST_DIR": str(Path(tmp) / target),
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
