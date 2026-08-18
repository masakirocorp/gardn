# Oh My Herdr imperative workflows
#
# Routine build, check, lint, and test tasks live in the Turborepo graph.
#
# Build the vendored libghostty-vt source dist
build-libghostty-vt:
    scripts/build_vendored_libghostty_vt.sh

# Build, install, sign, and bundle matching local development binaries
install-local:
    scripts/install_local_binaries.sh

# Build and atomically install omh-dev with matching Linux workers
install-dev:
    scripts/install_local_binaries.sh --dev-only

# Create a merge-commit PR for upstream Herdr changes
sync-upstream:
    python3 scripts/sync_upstream.py

# Report upstream Herdr commits as ported, skipped, superseded, or pending
upstream-status:
    python3 scripts/upstream_status.py --check

# Enforce deterministic UI hot-path architecture boundaries
ui-hot-path-architecture-test:
    python3 -m unittest scripts.test_ui_hot_path_architecture

# Non-gating full-render scaling profile for background workspaces and active panes
bench-render-scale:
    cargo test --release --locked --package omh --bin omh render_scale_profile -- --ignored --nocapture --test-threads=1


# Draft a Tegami version commit, tag it, push, and trigger the GitHub Release workflow
release:
    @if [ -n "$(git status --porcelain)" ]; then \
        echo "error: commit your changes first"; \
        exit 1; \
    fi
    node scripts/check-tegami-release-scope.mts omh
    CI=true pnpm tegami version
    pnpm check
    @version="$(python3 -c 'import tomllib; print(tomllib.load(open("apps/omh/Cargo.toml", "rb"))["package"]["version"])')"; \
    tag="v$version"; \
    if git rev-parse "$tag" >/dev/null 2>&1; then \
        echo "error: tag $tag already exists"; \
        exit 1; \
    fi; \
    if git ls-remote --exit-code --tags origin "refs/tags/$tag" >/dev/null 2>&1; then \
        echo "error: origin tag $tag already exists"; \
        exit 1; \
    fi; \
    git add apps/omh/Cargo.toml Cargo.lock .tegami pnpm-lock.yaml apps/omh/CHANGELOG.md apps/docs/package.json apps/docs/CHANGELOG.md packages/nix/package.json packages/nix/CHANGELOG.md; \
    git diff --cached --quiet || git commit -m "release: v$version"; \
    git tag -a "$tag" -m "$tag"; \
    git push origin HEAD; \
    git push origin "$tag"; \
    echo "$tag released — GitHub Actions building binaries"

# Build the live agent test image
agent-test-image:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ -z "${GH_TOKEN:-${GITHUB_TOKEN:-}}" ]]; then
        export GH_TOKEN="$(gh auth token)"
    fi
    node ci/agent-tests/resolve-versions.mjs > ci/agent-tests/cohort.json
    args=()
    while IFS=$'\t' read -r key value; do
        args+=(--build-arg "$key=$value")
    done < <(jq -r '.build_args | to_entries[] | [.key, .value] | @tsv' ci/agent-tests/cohort.json)
    docker build "${args[@]}" -t omh-agent-tests:local ci/agent-tests

# Print versions from the live agent test image
agent-test-doctor:
    docker run --rm omh-agent-tests:local

# Verify live agent test environment wiring without calling providers
agent-test-verify:
    docker run --rm -e OPENROUTER_API_KEY=sk-omh-agent-test omh-agent-tests:local omh-agent-tests-env omh-agent-tests-verify-env

# Run OpenCode against the configured free OpenRouter test model
agent-test-opencode:
    docker run --rm -e OPENROUTER_API_KEY -e OMH_TEST_MODEL -e OMH_TEST_FALLBACK_MODELS omh-agent-tests:local omh-agent-tests-env omh-agent-tests-opencode

# Run OpenCode and verify Oh My Herdr status reports from the real plugin
agent-test-opencode-status:
    docker run --rm -e OPENROUTER_API_KEY -e OMH_TEST_MODEL -e OMH_TEST_FALLBACK_MODELS -v "$PWD:/repo:ro" omh-agent-tests:local omh-agent-tests-env omh-agent-tests-opencode-status

# Run Pi/OMP and verify Oh My Herdr status reports from the real plugin
agent-test-pi-omp-status:
    docker run --rm -e OPENROUTER_API_KEY -e OMH_TEST_MODEL -e OMH_TEST_FALLBACK_MODELS -v "$PWD:/repo:ro" omh-agent-tests:local omh-agent-tests-env omh-agent-tests-pi-omp-status

# Run Claude through OpenRouter and verify Oh My Herdr status reports from the real hook
agent-test-claude-status:
    docker run --rm -e OPENROUTER_API_KEY -e OMH_TEST_MODEL -e OMH_TEST_FALLBACK_MODELS -v "$PWD:/repo:ro" omh-agent-tests:local omh-agent-tests-env omh-agent-tests-claude-status

# Run Codex through OpenRouter and verify Oh My Herdr status reports from the real hook
agent-test-codex-status:
    docker run --rm -e OPENROUTER_API_KEY -e OMH_TEST_MODEL -e OMH_TEST_FALLBACK_MODELS -v "$PWD:/repo:ro" omh-agent-tests:local omh-agent-tests-env omh-agent-tests-codex-status

# Run remaining installed agents and verify Oh My Herdr status reports where hooks exist
agent-test-remaining-status:
    docker run --rm -e OPENROUTER_API_KEY -e OMH_TEST_MODEL -e OMH_TEST_FALLBACK_MODELS -v "$PWD:/repo:ro" omh-agent-tests:local omh-agent-tests-env omh-agent-tests-remaining-status

# Run Cursor through a local OpenRouter proxy and assert real hook states
agent-test-cursor-proxy-status:
    docker run --rm --user root -e OPENROUTER_API_KEY -e OMH_TEST_MODEL -e OMH_TEST_FALLBACK_MODELS -e OMH_TEST_CURSOR_MODEL -v "$PWD:/repo:ro" --add-host api2.cursor.sh:127.0.0.1 --add-host api2geo.cursor.sh:127.0.0.1 --add-host api2direct.cursor.sh:127.0.0.1 --add-host agentn.api5.cursor.sh:127.0.0.1 --add-host agent.api5.cursor.sh:127.0.0.1 omh-agent-tests:local omh-agent-tests-env omh-agent-tests-cursor-proxy-status

# Run Qoder through a local OpenRouter proxy and assert real hook states
agent-test-qoder-proxy-status:
    docker run --rm --user root -e OPENROUTER_API_KEY -e QODER_PERSONAL_ACCESS_TOKEN -e OMH_TEST_MODEL -e OMH_TEST_FALLBACK_MODELS -e OMH_TEST_QODER_PROXY_MODEL -v "$PWD:/repo:ro" --add-host api1.qoder.sh:127.0.0.1 --add-host api2.qoder.sh:127.0.0.1 omh-agent-tests:local omh-agent-tests-env omh-agent-tests-qoder-proxy-status

# Verify Pi/OMP plugin lifecycle reports without calling providers
agent-test-pi-omp-plugin-status:
    docker run --rm -v "$PWD:/repo:ro" omh-agent-tests:local node --experimental-strip-types /usr/local/bin/omh-agent-pi-omp-plugin-status-test /repo/apps/omh/src/integration/assets/pi/omh-agent-state.ts pi
    docker run --rm -v "$PWD:/repo:ro" omh-agent-tests:local node --experimental-strip-types /usr/local/bin/omh-agent-pi-omp-plugin-status-test /repo/apps/omh/src/integration/assets/omp/omh-agent-state.ts omp

# Print default config
default-config:
    cargo run --release --locked -- --default-config
