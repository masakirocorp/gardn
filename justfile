# hako task runner

# Run local tests with incremental compilation
test:
    cargo nextest run --locked --status-level fail --final-status-level fail --failure-output final --success-output never
    python3 -m unittest scripts.test_agent_detection_manifest_check scripts.test_vendor_libghostty_vt scripts.test_vendor_portable_pty scripts.test_testing_guidelines scripts.test_extract_release_notes scripts.test_check_tegami_release_scope scripts.test_codex_status_fallback scripts.test_opencode_status_fallback scripts.test_agent_workflows

# Run one nextest filter, e.g. `just test-one codex_stale_working`
test-one filter:
    cargo nextest run --locked "{{filter}}" --status-level fail --final-status-level fail --failure-output final --success-output never

# Run structural Rust guardrails
ast-grep:
    ast-grep scan --config sgconfig.yml apps/hako/src --report-style short --error

# Run fast local lint checks
lint:
    cargo fmt --check
    CARGO_INCREMENTAL=0 cargo clippy --all-targets --locked -- -D warnings
    just ast-grep

# Run Rust tests with CI settings
ci-test:
    CARGO_INCREMENTAL=0 cargo nextest run -P ci --locked --status-level slow --final-status-level slow --failure-output final --success-output never

# Run PR CI checks
ci: lint ci-test

# Check formatting + run unit tests + maintenance script tests
check: ci
    python3 -m unittest scripts.test_agent_detection_manifest_check scripts.test_vendor_libghostty_vt scripts.test_vendor_portable_pty scripts.test_testing_guidelines scripts.test_extract_release_notes scripts.test_check_tegami_release_scope scripts.test_codex_status_fallback scripts.test_opencode_status_fallback scripts.test_pi_omp_status_validation scripts.test_qoder_proxy_status_validation scripts.test_agent_status_acceptance_invariant scripts.test_agent_model_candidates scripts.test_remaining_status_fallback scripts.test_agent_workflows
    @echo "docs reminder: if this changes user-facing behavior, update README.md or call it out before release."


# Build release binary
build:
    cargo build --release --locked

# Build the vendored libghostty-vt source dist
build-libghostty-vt:
    scripts/build_vendored_libghostty_vt.sh



# Create a merge-commit PR for upstream Herdr changes
sync-upstream:
    python3 scripts/sync_upstream.py

# Report upstream Herdr commits as ported, skipped, superseded, or pending
upstream-status:
    python3 scripts/upstream_status.py --check

# Run Tegami release/changelog tooling
tegami *args:
    pnpm tegami {{args}}


# Draft a Tegami version commit, tag it, push, and trigger the GitHub Release workflow
release:
    @if [ -n "$(git status --porcelain)" ]; then \
        echo "error: commit your changes first"; \
        exit 1; \
    fi
    node scripts/check-tegami-release-scope.mts hako
    CI=true pnpm tegami version
    just check
    @version="$(python3 -c 'import tomllib; print(tomllib.load(open("apps/hako/Cargo.toml", "rb"))["package"]["version"])')"; \
    tag="v$version"; \
    if git rev-parse "$tag" >/dev/null 2>&1; then \
        echo "error: tag $tag already exists"; \
        exit 1; \
    fi; \
    if git ls-remote --exit-code --tags origin "refs/tags/$tag" >/dev/null 2>&1; then \
        echo "error: origin tag $tag already exists"; \
        exit 1; \
    fi; \
    git add apps/hako/Cargo.toml Cargo.lock .tegami pnpm-lock.yaml apps/hako/CHANGELOG.md apps/docs/package.json apps/docs/CHANGELOG.md packages/nix/package.json packages/nix/CHANGELOG.md; \
    git diff --cached --quiet || git commit -m "release: v$version"; \
    git tag -a "$tag" -m "$tag"; \
    git push origin HEAD; \
    git push origin "$tag"; \
    echo "$tag released — GitHub Actions building binaries"

# Build the live agent test image
agent-test-image:
    docker build -t hako-agent-tests:local ci/agent-tests

# Print versions from the live agent test image
agent-test-doctor:
    docker run --rm hako-agent-tests:local


# Verify live agent test environment wiring without calling providers
agent-test-verify:
    docker run --rm -e OPENROUTER_API_KEY=sk-hako-agent-test hako-agent-tests:local hako-agent-tests-env hako-agent-tests-verify-env


# Run OpenCode against the configured free OpenRouter test model
agent-test-opencode:
    docker run --rm -e OPENROUTER_API_KEY -e HAKO_TEST_FALLBACK_MODELS -e HAKO_OPENCODE_TEST_MODEL hako-agent-tests:local hako-agent-tests-env hako-agent-tests-opencode


# Run OpenCode and verify Hako status reports from the real plugin
agent-test-opencode-status:
    docker run --rm -e OPENROUTER_API_KEY -e HAKO_TEST_MODEL -e HAKO_TEST_FALLBACK_MODELS -e HAKO_OPENCODE_TEST_MODEL -v "$PWD:/repo:ro" hako-agent-tests:local hako-agent-tests-env hako-agent-tests-opencode-status



# Run Pi/OMP and verify Hako status reports from the real plugin
agent-test-pi-omp-status:
    docker run --rm -e OPENROUTER_API_KEY -e HAKO_TEST_MODEL -e HAKO_TEST_FALLBACK_MODELS -v "$PWD:/repo:ro" hako-agent-tests:local hako-agent-tests-env hako-agent-tests-pi-omp-status

# Run Claude through OpenRouter and verify Hako status reports from the real hook
agent-test-claude-status:
    docker run --rm -e OPENROUTER_API_KEY -e HAKO_TEST_MODEL -e HAKO_TEST_FALLBACK_MODELS -v "$PWD:/repo:ro" hako-agent-tests:local hako-agent-tests-env hako-agent-tests-claude-status

# Run Codex through OpenRouter and verify Hako status reports from the real hook
agent-test-codex-status:
    docker run --rm -e OPENROUTER_API_KEY -e HAKO_TEST_MODEL -e HAKO_TEST_FALLBACK_MODELS -v "$PWD:/repo:ro" hako-agent-tests:local hako-agent-tests-env hako-agent-tests-codex-status

# Run remaining installed agents and verify Hako status reports where hooks exist
agent-test-remaining-status:
    docker run --rm -e OPENROUTER_API_KEY -e HAKO_TEST_MODEL -e HAKO_TEST_FALLBACK_MODELS -v "$PWD:/repo:ro" hako-agent-tests:local hako-agent-tests-env hako-agent-tests-remaining-status


# Run Cursor through a local OpenRouter proxy and assert real hook states
agent-test-cursor-proxy-status:
    docker run --rm --user root -e OPENROUTER_API_KEY -e HAKO_TEST_MODEL -e HAKO_TEST_FALLBACK_MODELS -e HAKO_TEST_CURSOR_MODEL -v "$PWD:/repo:ro" --add-host api2.cursor.sh:127.0.0.1 --add-host api2geo.cursor.sh:127.0.0.1 --add-host api2direct.cursor.sh:127.0.0.1 --add-host agentn.api5.cursor.sh:127.0.0.1 --add-host agent.api5.cursor.sh:127.0.0.1 hako-agent-tests:local hako-agent-tests-env hako-agent-tests-cursor-proxy-status


# Run Qoder through a local OpenRouter proxy and assert real hook states
agent-test-qoder-proxy-status:
    docker run --rm --user root -e OPENROUTER_API_KEY -e QODER_PERSONAL_ACCESS_TOKEN -e HAKO_TEST_MODEL -e HAKO_TEST_FALLBACK_MODELS -e HAKO_TEST_QODER_PROXY_MODEL -v "$PWD:/repo:ro" --add-host api1.qoder.sh:127.0.0.1 --add-host api2.qoder.sh:127.0.0.1 hako-agent-tests:local hako-agent-tests-env hako-agent-tests-qoder-proxy-status

# Verify Pi/OMP plugin lifecycle reports without calling providers
agent-test-pi-omp-plugin-status:
    docker run --rm -v "$PWD:/repo:ro" hako-agent-tests:local node --experimental-strip-types /usr/local/bin/hako-agent-pi-omp-plugin-status-test /repo/apps/hako/src/integration/assets/pi/hako-agent-state.ts pi
    docker run --rm -v "$PWD:/repo:ro" hako-agent-tests:local node --experimental-strip-types /usr/local/bin/hako-agent-pi-omp-plugin-status-test /repo/apps/hako/src/integration/assets/omp/hako-agent-state.ts omp

# Print default config
default-config:
    cargo run --release --locked -- --default-config
