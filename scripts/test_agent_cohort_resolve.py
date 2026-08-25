import http.server
import json
import os
import socketserver
import subprocess
import tempfile
import threading
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
RESOLVER = REPO_ROOT / "ci" / "agent-tests" / "resolve-versions.mjs"

NPM_PACKAGES = {
    "@anthropic-ai/claude-code": "1.2.3",
    "@openai/codex": "0.4.5",
    "opencode-ai": "1.0.10",
    "@github/copilot": "0.0.9",
    "hermes-agent": "2.1.0",
    "droid": "3.3.3",
    "@earendil-works/pi-coding-agent": "0.7.8",
    "@qwen-code/qwen-code": "0.22.0",
    "@kilocode/cli": "7.4.23",
    "mastracode": "0.35.0",
}

GITHUB_RELEASES = {
    "MoonshotAI/kimi-code": {
        "tag_name": "@moonshot-ai/kimi-code@9.9.9",
        "assets": [
            {"name": "kimi-code-linux-x64.zip"},
            {"name": "kimi-code-linux-arm64.zip"},
            {"name": "kimi-code-linux-x64.zip.sha256"},
            {"name": "kimi-code-linux-arm64.zip.sha256"},
        ],
    },
    "tontinton/maki": {
        "tag_name": "v1.4.2",
        "assets": [
            {"name": "maki-v1.4.2-x86_64-unknown-linux-musl.tar.gz"},
            {"name": "maki-v1.4.2-aarch64-unknown-linux-musl.tar.gz"},
        ],
    },
    "can1357/oh-my-pi": {
        "tag_name": "v0.12.0",
        "assets": [
            {"name": "omp-linux-x64"},
            {"name": "omp-linux-arm64"},
        ],
    },
}


class _Handler(http.server.BaseHTTPRequestHandler):
    responses = {}
    auth_header = None
    last_auth = None
    requests = {}

    def log_message(self, format, *args):  # noqa: A003 - stdlib signature
        return

    def do_GET(self):  # noqa: N802 - stdlib signature
        _Handler.requests[self.path] = _Handler.requests.get(self.path, 0) + 1
        _Handler.last_auth = self.headers.get("Authorization")
        body = self.responses.get(self.path)
        if isinstance(body, list):
            body = body.pop(0)
        if body is None:
            self.send_response(404)
            self.end_headers()
            self.wfile.write(b'{"message":"not found"}')
            return
        if body is False:
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(b"null")
            return
        if isinstance(body, tuple):
            status, payload = body
            raw = payload if isinstance(payload, (bytes, bytearray)) else json.dumps(payload).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(raw)
            return
        raw = json.dumps(body).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(raw)


class _ReleasingTCPServer(socketserver.TCPServer):
    allow_reuse_address = True


def start_github_fixture(responses):
    _Handler.responses = responses
    _Handler.last_auth = None
    _Handler.requests = {}
    server = _ReleasingTCPServer(("127.0.0.1", 0), _Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    host, port = server.server_address
    return server, f"http://{host}:{port}"


def write_fake_npm(bin_dir, versions):
    script = bin_dir / "npm"
    mapping = json.dumps(versions)
    script.write_text(
        "#!/usr/bin/env bash\n"
        "set -euo pipefail\n"
        f"versions='{mapping}'\n"
        'if [[ "${1:-}" != "view" || "${3:-}" != "version" ]]; then\n'
        '  echo "unexpected npm invocation: $*" >&2\n'
        "  exit 2\n"
        "fi\n"
        'pkg="$2"\n'
        'version="$(python3 -c \'import json,sys; print(json.loads(sys.argv[1])[sys.argv[2]])\' "$versions" "$pkg")"\n'
        'printf \'"%s"\\n\' "$version"\n'
    )
    script.chmod(0o755)
    return script


class AgentCohortResolveTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.tmp = Path(self.temp.name)
        self.bin_dir = self.tmp / "bin"
        self.bin_dir.mkdir()
        write_fake_npm(self.bin_dir, NPM_PACKAGES)
        self.server = None

    def tearDown(self):
        if self.server is not None:
            self.server.shutdown()
            self.server.server_close()
        self.temp.cleanup()

    def run_resolver(self, *, env=None, github_responses=None, check=False):
        if github_responses is not None:
            self.server, api_base = start_github_fixture(github_responses)
        else:
            api_base = env.get("GARDN_RESOLVE_GITHUB_API") if env else None

        run_env = os.environ.copy()
        run_env["PATH"] = f"{self.bin_dir}{os.pathsep}{run_env.get('PATH', '')}"
        run_env["GARDN_RESOLVE_NPM"] = str(self.bin_dir / "npm")
        if api_base:
            run_env["GARDN_RESOLVE_GITHUB_API"] = api_base
        run_env.setdefault("GH_TOKEN", "test-token")
        run_env.setdefault("SOURCE_REVISION", "deadbeef")
        run_env.setdefault("BUILD_RUN_ID", "123")
        run_env.setdefault("BUILD_RUN_ATTEMPT", "1")
        run_env.setdefault("ANTIGRAVITY_VERSION", "1.2.3")
        run_env.setdefault("ANTIGRAVITY_DOWNLOAD_URL", "https://example.test/agy")
        run_env.setdefault("ANTIGRAVITY_SHA512", "a" * 128)
        if env:
            run_env.update(env)

        result = subprocess.run(
            ["node", str(RESOLVER)],
            cwd=REPO_ROOT,
            env=run_env,
            text=True,
            capture_output=True,
            check=False,
        )
        if check and result.returncode != 0:
            raise AssertionError(result.stdout + result.stderr)
        return result

    def default_github_responses(self):
        return {
            f"/repos/{repo}/releases/latest": payload
            for repo, payload in GITHUB_RELEASES.items()
        }

    def test_resolves_authenticated_github_and_exact_build_args(self):
        result = self.run_resolver(github_responses=self.default_github_responses())
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        cohort = json.loads(result.stdout)

        self.assertEqual(cohort["schema"], 1)
        self.assertEqual(_Handler.last_auth, "Bearer test-token")
        self.assertNotIn("latest", json.dumps(cohort))

        build_args = cohort["build_args"]
        self.assertEqual(
            build_args,
            {
                "CLAUDE_CODE_VERSION": "1.2.3",
                "CODEX_VERSION": "0.4.5",
                "OPENCODE_VERSION": "1.0.10",
                "COPILOT_VERSION": "0.0.9",
                "HERMES_VERSION": "2.1.0",
                "DROID_VERSION": "3.3.3",
                "PI_VERSION": "0.7.8",
                "QWEN_CODE_VERSION": "0.22.0",
                "KILO_VERSION": "7.4.23",
                "MASTRACODE_VERSION": "0.35.0",
                "KIMI_VERSION": "9.9.9",
                "MAKI_VERSION": "v1.4.2",
                "OMP_REF": "v0.12.0",
                "ANTIGRAVITY_VERSION": "1.2.3",
                "ANTIGRAVITY_DOWNLOAD_URL": "https://example.test/agy",
                "ANTIGRAVITY_SHA512": "a" * 128,
            },
        )
        for value in build_args.values():
            self.assertNotEqual(value, "latest")
            self.assertTrue(value)

        self.assertEqual(cohort["agents"]["claude"]["source"], "npm")
        self.assertEqual(cohort["agents"]["qwen"]["package"], "@qwen-code/qwen-code")
        self.assertEqual(cohort["agents"]["qwen"]["version"], "0.22.0")
        self.assertEqual(cohort["agents"]["kilo"]["package"], "@kilocode/cli")
        self.assertEqual(cohort["agents"]["kilo"]["version"], "7.4.23")
        self.assertEqual(cohort["agents"]["mastracode"]["package"], "mastracode")
        self.assertEqual(cohort["agents"]["mastracode"]["version"], "0.35.0")
        self.assertEqual(cohort["agents"]["antigravity"]["source"], "manifest")
        self.assertEqual(cohort["agents"]["antigravity"]["sha512"], "a" * 128)
        self.assertEqual(cohort["agents"]["kimi"]["tag"], "@moonshot-ai/kimi-code@9.9.9")
        self.assertEqual(cohort["agents"]["maki"]["version"], "1.4.2")
        self.assertEqual(cohort["agents"]["omp"]["repo"], "can1357/oh-my-pi")
        self.assertEqual(cohort["source"]["revision"], "deadbeef")
        self.assertEqual(cohort["source"]["run_id"], "123")
        self.assertEqual(cohort["source"]["run_attempt"], "1")

    def test_retries_transient_github_release_failure(self):
        responses = self.default_github_responses()
        path = "/repos/MoonshotAI/kimi-code/releases/latest"
        responses[path] = [
            (500, {"message": "temporary failure"}),
            GITHUB_RELEASES["MoonshotAI/kimi-code"],
        ]

        result = self.run_resolver(github_responses=responses)

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(_Handler.requests[path], 2)

    def test_requires_github_token(self):
        result = self.run_resolver(
            github_responses=self.default_github_responses(),
            env={"GH_TOKEN": "", "GITHUB_TOKEN": ""},
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("GH_TOKEN or GITHUB_TOKEN is required", result.stderr)

    def test_rejects_null_github_release_payload(self):
        responses = self.default_github_responses()
        responses["/repos/tontinton/maki/releases/latest"] = False
        result = self.run_resolver(github_responses=responses)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("null or non-object release payload", result.stderr)

    def test_rejects_missing_required_release_assets(self):
        responses = self.default_github_responses()
        responses["/repos/can1357/oh-my-pi/releases/latest"] = {
            "tag_name": "v0.12.0",
            "assets": [{"name": "omp-linux-x64"}],
        }
        result = self.run_resolver(github_responses=responses)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("missing required assets", result.stderr)
        self.assertIn("omp-linux-arm64", result.stderr)

    def test_manual_version_overrides_skip_remote_lookup(self):
        # Provide a GitHub fixture that would fail if contacted.
        responses = {
            "/repos/MoonshotAI/kimi-code/releases/latest": (500, {"message": "nope"}),
            "/repos/tontinton/maki/releases/latest": (500, {"message": "nope"}),
            "/repos/can1357/oh-my-pi/releases/latest": (500, {"message": "nope"}),
        }
        result = self.run_resolver(
            github_responses=responses,
            env={
                "CLAUDE_CODE_VERSION": "9.9.1",
                "CODEX_VERSION": "9.9.2",
                "OPENCODE_VERSION": "9.9.3",
                "COPILOT_VERSION": "9.9.4",
                "HERMES_VERSION": "9.9.5",
                "DROID_VERSION": "9.9.6",
                "PI_VERSION": "9.9.7",
                "QWEN_CODE_VERSION": "9.9.8",
                "KILO_VERSION": "9.9.9",
                "MASTRACODE_VERSION": "9.9.10",
                "KIMI_VERSION": "8.8.8",
                "MAKI_VERSION": "v7.7.7",
                "OMP_REF": "v6.6.6",
                "ANTIGRAVITY_VERSION": "5.5.5",
                "ANTIGRAVITY_DOWNLOAD_URL": "https://example.test/agy-exact",
                "ANTIGRAVITY_SHA512": "b" * 128,
            },
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        cohort = json.loads(result.stdout)
        self.assertEqual(cohort["build_args"]["CLAUDE_CODE_VERSION"], "9.9.1")
        self.assertEqual(cohort["build_args"]["QWEN_CODE_VERSION"], "9.9.8")
        self.assertEqual(cohort["build_args"]["KILO_VERSION"], "9.9.9")
        self.assertEqual(cohort["build_args"]["MASTRACODE_VERSION"], "9.9.10")
        self.assertEqual(cohort["build_args"]["KIMI_VERSION"], "8.8.8")
        self.assertEqual(cohort["build_args"]["MAKI_VERSION"], "v7.7.7")
        self.assertEqual(cohort["build_args"]["OMP_REF"], "v6.6.6")
        self.assertEqual(cohort["build_args"]["ANTIGRAVITY_VERSION"], "5.5.5")
        self.assertEqual(cohort["build_args"]["ANTIGRAVITY_SHA512"], "b" * 128)
        self.assertNotIn("latest", json.dumps(cohort))

    def test_rejects_latest_override(self):
        result = self.run_resolver(
            github_responses=self.default_github_responses(),
            env={"CLAUDE_CODE_VERSION": "latest"},
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("refused empty or floating version", result.stderr)


class AgentCohortWorkflowTests(unittest.TestCase):
    def setUp(self):
        self.repo_root = REPO_ROOT
        self.workflow = (
            self.repo_root / ".github" / "workflows" / "live-agent-tests.yml"
        ).read_text()
        self.dockerfile = (self.repo_root / "ci" / "agent-tests" / "Dockerfile").read_text()

    def test_run_unique_tag_construction(self):
        self.assertIn(
            'tag="${GITHUB_SHA}-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}"',
            self.workflow,
        )
        self.assertIn("image_ref: ${{ steps.digest.outputs.image_ref }}", self.workflow)
        self.assertIn("docker image inspect --format '{{index .RepoDigests 0}}'", self.workflow)
        self.assertIn('image_ref="$(docker image inspect', self.workflow)

    def test_test_jobs_consume_digest_output(self):
        self.assertIn("IMAGE_REF: ${{ needs.image.outputs.image_ref }}", self.workflow)
        self.assertIn('docker pull "$IMAGE_REF"', self.workflow)
        self.assertIn('docker run "${args[@]}" "$IMAGE_REF"', self.workflow)
        self.assertNotIn("$IMAGE:${{ github.sha }}", self.workflow)
        self.assertIn("*@sha256:*", self.workflow)

    def test_resolver_step_uses_token_outside_docker(self):
        self.assertIn("node ci/agent-tests/resolve-versions.mjs", self.workflow)
        self.assertIn("GH_TOKEN: ${{ github.token }}", self.workflow)
        self.assertIn("GITHUB_TOKEN: ${{ github.token }}", self.workflow)
        self.assertNotIn("GH_TOKEN", self.dockerfile)
        self.assertNotIn("GITHUB_TOKEN", self.dockerfile)
        self.assertNotIn("--build-arg GH_TOKEN", self.workflow)
        self.assertNotIn("--build-arg GITHUB_TOKEN", self.workflow)

    def test_dockerfile_requires_exact_versions_and_drops_kiro(self):
        self.assertIn("refusing floating or empty build arg", self.dockerfile)
        self.assertIn("COPY cohort.json /usr/local/share/gardn-agent-tests/cohort.json", self.dockerfile)
        self.assertNotIn("kiro", self.dockerfile.lower())
        self.assertNotIn("ARG CLAUDE_CODE_VERSION=latest", self.dockerfile)
        self.assertNotIn("ARG MAKI_VERSION=0.3.27", self.dockerfile)

    def test_exact_build_arg_flags_are_passed(self):
        for arg in [
            "CLAUDE_CODE_VERSION",
            "CODEX_VERSION",
            "OPENCODE_VERSION",
            "COPILOT_VERSION",
            "HERMES_VERSION",
            "DROID_VERSION",
            "PI_VERSION",
            "QWEN_CODE_VERSION",
            "KILO_VERSION",
            "MASTRACODE_VERSION",
            "KIMI_VERSION",
            "MAKI_VERSION",
            "OMP_REF",
            "ANTIGRAVITY_VERSION",
            "ANTIGRAVITY_DOWNLOAD_URL",
            "ANTIGRAVITY_SHA512",
        ]:
            self.assertIn(f'--build-arg "{arg}=', self.workflow)


if __name__ == "__main__":
    unittest.main()
