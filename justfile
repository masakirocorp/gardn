# hako task runner

# Run local tests with incremental compilation
test:
    cargo nextest run --locked --status-level fail --final-status-level fail --failure-output final --success-output never
    python3 -m unittest scripts.test_agent_detection_manifest_check scripts.test_vendor_libghostty_vt scripts.test_vendor_portable_pty scripts.test_testing_guidelines scripts.test_extract_release_notes scripts.test_check_tegami_release_scope scripts.test_codex_status_smoke_fallback scripts.test_opencode_status_smoke_fallback

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
    python3 -m unittest scripts.test_agent_detection_manifest_check scripts.test_vendor_libghostty_vt scripts.test_vendor_portable_pty scripts.test_testing_guidelines scripts.test_extract_release_notes scripts.test_check_tegami_release_scope scripts.test_codex_status_smoke_fallback scripts.test_opencode_status_smoke_fallback scripts.test_pi_omp_status_smoke_validation scripts.test_qoder_proxy_status_smoke_validation scripts.test_agent_smoke_status_acceptance_invariant scripts.test_smoke_model_candidates scripts.test_remaining_status_smoke_fallback
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

# Build optional real-agent smoke-test image
agent-smoke-image:
    docker build -t hako-agent-smoke:local ci/agent-smoke

# Print versions from optional real-agent smoke-test image
agent-smoke-doctor:
    docker run --rm hako-agent-smoke:local


# Verify optional real-agent smoke-test env wiring without calling providers
agent-smoke-verify:
    docker run --rm -e OPENROUTER_API_KEY=sk-hako-smoke-test hako-agent-smoke:local hako-agent-smoke-env hako-agent-smoke-verify-env


# Run OpenCode against the configured free OpenRouter smoke model
agent-smoke-opencode:
    docker run --rm -e OPENROUTER_API_KEY -e HAKO_SMOKE_FALLBACK_MODELS -e HAKO_OPENCODE_SMOKE_MODEL hako-agent-smoke:local hako-agent-smoke-env hako-agent-smoke-opencode


# Run OpenCode and verify Hako status reports from the real plugin
agent-smoke-opencode-status:
    docker run --rm -e OPENROUTER_API_KEY -e HAKO_SMOKE_MODEL -e HAKO_SMOKE_FALLBACK_MODELS -e HAKO_OPENCODE_SMOKE_MODEL -v "$PWD:/repo:ro" hako-agent-smoke:local hako-agent-smoke-env hako-agent-smoke-opencode-status



# Run Pi/OMP and verify Hako status reports from the real plugin
agent-smoke-pi-omp-status:
    docker run --rm -e OPENROUTER_API_KEY -e HAKO_SMOKE_MODEL -e HAKO_SMOKE_FALLBACK_MODELS -v "$PWD:/repo:ro" hako-agent-smoke:local hako-agent-smoke-env hako-agent-smoke-pi-omp-status

# Run Claude through OpenRouter and verify Hako status reports from the real hook
agent-smoke-claude-status:
    docker run --rm -e OPENROUTER_API_KEY -e HAKO_SMOKE_MODEL -e HAKO_SMOKE_FALLBACK_MODELS -v "$PWD:/repo:ro" hako-agent-smoke:local hako-agent-smoke-env hako-agent-smoke-claude-status

# Run Codex through OpenRouter and verify Hako status reports from the real hook
agent-smoke-codex-status:
    docker run --rm -e OPENROUTER_API_KEY -e HAKO_SMOKE_MODEL -e HAKO_SMOKE_FALLBACK_MODELS -v "$PWD:/repo:ro" hako-agent-smoke:local hako-agent-smoke-env hako-agent-smoke-codex-status

# Run remaining installed agents and verify Hako status reports where hooks exist
agent-smoke-remaining-status:
    docker run --rm -e OPENROUTER_API_KEY -e HAKO_SMOKE_MODEL -e HAKO_SMOKE_FALLBACK_MODELS -v "$PWD:/repo:ro" hako-agent-smoke:local hako-agent-smoke-env hako-agent-smoke-remaining-status


# Run Cursor through an opt-in local OpenRouter proxy; hook states stay covered by seam smoke
agent-smoke-cursor-proxy-status:
    docker run --rm --user root -e OPENROUTER_API_KEY -e HAKO_SMOKE_MODEL -e HAKO_SMOKE_FALLBACK_MODELS -e HAKO_SMOKE_CURSOR_MODEL -v "$PWD:/repo:ro" --add-host api2.cursor.sh:127.0.0.1 --add-host api2geo.cursor.sh:127.0.0.1 --add-host api2direct.cursor.sh:127.0.0.1 --add-host agentn.api5.cursor.sh:127.0.0.1 --add-host agent.api5.cursor.sh:127.0.0.1 hako-agent-smoke:local hako-agent-smoke-env hako-agent-smoke-cursor-proxy-status


# Run Qoder through an opt-in local OpenRouter proxy; hook states stay covered by seam smoke
agent-smoke-qoder-proxy-status:
    docker run --rm --user root -e OPENROUTER_API_KEY -e QODER_PERSONAL_ACCESS_TOKEN -e HAKO_SMOKE_MODEL -e HAKO_SMOKE_FALLBACK_MODELS -e HAKO_SMOKE_QODER_PROXY_MODEL -v "$PWD:/repo:ro" --add-host api1.qoder.sh:127.0.0.1 --add-host api2.qoder.sh:127.0.0.1 hako-agent-smoke:local hako-agent-smoke-env hako-agent-smoke-qoder-proxy-status

# Verify Pi/OMP plugin lifecycle reports without calling providers
agent-smoke-pi-omp-plugin-status:
    docker run --rm -v "$PWD:/repo:ro" hako-agent-smoke:local node --experimental-strip-types /usr/local/bin/hako-agent-pi-omp-plugin-status-test /repo/apps/hako/src/integration/assets/pi/hako-agent-state.ts pi
    docker run --rm -v "$PWD:/repo:ro" hako-agent-smoke:local node --experimental-strip-types /usr/local/bin/hako-agent-pi-omp-plugin-status-test /repo/apps/hako/src/integration/assets/omp/hako-agent-state.ts omp

# Print default config
default-config:
    cargo run --release --locked -- --default-config
