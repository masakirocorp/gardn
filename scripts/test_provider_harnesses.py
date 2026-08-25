import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class ProviderHarnessStructureTests(unittest.TestCase):
    def read(self, relative):
        return (ROOT / relative).read_text()

    def test_server_owns_both_wire_protocols_and_scenarios(self):
        server = self.read("ci/agent-tests/deterministic-provider.mjs")
        self.assertIn('"/v1/chat/completions"', server)
        self.assertIn("streamGenerateContent|generateContent", server)
        for scenario in ("retry-429", "error-400", "error-500", "tool"):
            self.assertIn(scenario, server)
        self.assertIn("GARDN_PROVIDER_OK", server)
        self.assertIn("GARDN_TOOL_COMPLETE", server)
        self.assertNotIn("node_modules", server)

    def test_all_real_cli_harnesses_route_through_dispatcher(self):
        dispatcher = self.read("ci/agent-tests/run-target.sh")
        matrix = self.read("ci/agent-tests/target-matrix.sh")
        for target in ("qwen-deterministic", "kilo-deterministic", "mastracode", "antigravity", "antigravity-gemini"):
            self.assertIn(target, dispatcher)
            self.assertIn(target, matrix)
        for command in ("qwen-status", "kilo-status", "mastracode-status", "antigravity-status"):
            self.assertIn(command, dispatcher)

    def test_family_lifecycle_ownership_is_preserved(self):
        qwen = self.read("ci/agent-tests/qwen-status-test.sh")
        kilo = self.read("ci/agent-tests/kilo-status-test.sh")
        mastra = self.read("ci/agent-tests/mastracode-status-test.sh")
        antigravity = self.read("ci/agent-tests/antigravity-status-test.sh")
        self.assertIn("gardn-agent-session.sh", qwen)
        self.assertIn("initial idle -> working -> idle", qwen)
        self.assertIn("gardn-agent-state.js", kilo)
        self.assertIn('(\"working\", \"blocked\", \"working\", \"idle\")', kilo)
        self.assertIn('"permission": {"bash": "ask"}', kilo)
        self.assertIn("gardn-agent-state.sh", mastra)
        self.assertIn('(\"working\",\"blocked\",\"working\",\"idle\")', mastra)
        self.assertIn("--model mastracode/gardn/gardn-tool", mastra)
        self.assertIn("gardn-agent-session.sh", antigravity)
        self.assertIn("pane.report_agent_session", antigravity)

    def test_qwen_hook_reports_session_contract(self):
        hook = ROOT / "apps/gardn/src/integration/assets/qwen/gardn-agent-session.sh"
        with tempfile.TemporaryDirectory() as temp_dir:
            capture = Path(temp_dir) / "args.txt"
            fake_gardn = Path(temp_dir) / "gardn"
            fake_gardn.write_text(
                '#!/bin/sh\nprintf "%s\\n" "$@" > "$GARDN_CAPTURE"\n'
            )
            fake_gardn.chmod(0o755)
            env = {
                **os.environ,
                "GARDN_ENV": "1",
                "GARDN_SOCKET_PATH": "/tmp/gardn-test.sock",
                "GARDN_PANE_ID": "pane-qwen",
                "GARDN_BIN_PATH": str(fake_gardn),
                "GARDN_CAPTURE": str(capture),
            }

            subprocess.run(
                ["sh", str(hook), "session"],
                input=json.dumps(
                    {"session_id": "qwen-session", "source": "startup"}
                ),
                text=True,
                check=True,
                env=env,
            )

            args = capture.read_text().splitlines()
            self.assertEqual(args[:3], ["pane", "report-agent-session", "pane-qwen"])
            self.assertIn("gardn:qwen", args)
            self.assertIn("qwen-session", args)
            self.assertIn("--session-start-source", args)
            self.assertIn("startup", args)

    def test_mastra_and_antigravity_installs_are_immutable(self):
        resolver = self.read("ci/agent-tests/resolve-versions.mjs")
        dockerfile = self.read("ci/agent-tests/Dockerfile")
        self.assertIn('packageName: "mastracode"', resolver)
        for field in ("ANTIGRAVITY_VERSION", "ANTIGRAVITY_DOWNLOAD_URL", "ANTIGRAVITY_SHA512"):
            self.assertIn(field, resolver)
            self.assertIn(field, dockerfile)
        self.assertIn("sha512sum -c -", dockerfile)
        self.assertNotIn("antigravity.google/cli/install.sh", dockerfile)
        self.assertIn('\\"mastracode\\": \\"${MASTRACODE_VERSION}\\"', dockerfile)
        self.assertIn("ARG TARGETARCH", dockerfile)
        self.assertIn('test "$TARGETARCH" = amd64', dockerfile)
        for package in (
            "agent-browser",
            "bufferutil",
            "edgedriver",
            "geckodriver",
            "onnxruntime-node",
        ):
            self.assertIn(f'"{package}": false', dockerfile)
        for workflow in (
            self.read(".github/workflows/agent-tests.yml"),
            self.read(".github/workflows/live-agent-tests.yml"),
        ):
            self.assertIn("--platform linux/amd64", workflow)

    def test_workflows_execute_deterministic_and_secret_backed_targets(self):
        fixture = self.read(".github/workflows/agent-tests.yml")
        live = self.read(".github/workflows/live-agent-tests.yml")
        for target in ("qwen-deterministic", "kilo-deterministic", "mastracode", "antigravity"):
            self.assertIn(target, fixture)
            self.assertIn(target, live)
        self.assertIn("GEMINI_API_KEY: ${{ secrets.GEMINI_API_KEY }}", live)
        self.assertIn("GEMINI_API_KEY is required for selected Antigravity real smoke", live)
        self.assertNotIn("continue-on-error", live)
        self.assertIn(
            "antigravity-gemini)\n              args+=(-e GEMINI_API_KEY)",
            live,
        )
        self.assertIn(
            "*)\n              args+=(-e OPENROUTER_API_KEY)",
            live,
        )
        self.assertNotIn(
            "-e OPENROUTER_API_KEY\n            -e GEMINI_API_KEY",
            live,
        )

    def test_docker_context_contains_every_new_runtime_file(self):
        dockerignore = self.read("ci/agent-tests/.dockerignore")
        for name in ("deterministic-provider.mjs", "with-deterministic-provider.sh", "mastracode-status-test.sh", "antigravity-status-test.sh"):
            self.assertIn(f"!{name}", dockerignore)


if __name__ == "__main__":
    unittest.main()
