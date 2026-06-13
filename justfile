# hako task runner

# Run local tests with incremental compilation
test:
    cargo nextest run --locked --status-level fail --final-status-level fail --failure-output final --success-output never
    python3 -m unittest scripts.test_agent_detection_manifest_check scripts.test_vendor_libghostty_vt scripts.test_testing_guidelines

# Run one nextest filter, e.g. `just test-one codex_stale_working`
test-one filter:
    cargo nextest run --locked "{{filter}}" --status-level fail --final-status-level fail --failure-output final --success-output never

# Run fast local lint checks
lint:
    cargo fmt --check
    CARGO_INCREMENTAL=0 cargo clippy --all-targets --locked -- -D warnings

# Run Rust tests with CI settings
ci-test:
    CARGO_INCREMENTAL=0 cargo nextest run -P ci --locked --status-level slow --final-status-level slow --failure-output final --success-output never

# Run PR CI checks
ci: lint ci-test

# Check formatting + run unit tests + maintenance script tests
check: ci
    python3 -m unittest scripts.test_agent_detection_manifest_check scripts.test_vendor_libghostty_vt scripts.test_testing_guidelines
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

# Bump version, commit, tag, push, and trigger the GitHub Release workflow (usage: just release 0.1.1)
release version:
    @if [ -n "$(git status --porcelain)" ]; then \
        echo "error: commit your changes first"; \
        exit 1; \
    fi
    @tag="v{{version}}"; \
    if git rev-parse "$tag" >/dev/null 2>&1; then \
        echo "error: tag $tag already exists"; \
        exit 1; \
    fi; \
    if git ls-remote --exit-code --tags origin "refs/tags/$tag" >/dev/null 2>&1; then \
        echo "error: origin tag $tag already exists"; \
        exit 1; \
    fi
    sed -i.bak 's/^version = ".*"/version = "{{version}}"/' Cargo.toml && rm -f Cargo.toml.bak
    cargo update -p hako --offline
    just check
    git add Cargo.toml Cargo.lock
    git diff --cached --quiet || git commit -m "release: v{{version}}"
    git tag -a v{{version}} -m "v{{version}}"
    git push origin HEAD
    git push origin v{{version}}
    @echo "v{{version}} released — GitHub Actions building binaries"

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
    docker run --rm -e OPENROUTER_API_KEY hako-agent-smoke:local hako-agent-smoke-env hako-agent-smoke-opencode


# Run OpenCode and verify Hako status reports from the real plugin
agent-smoke-opencode-status:
    docker run --rm -e OPENROUTER_API_KEY -e HAKO_SMOKE_MODEL -v "$PWD:/repo:ro" hako-agent-smoke:local hako-agent-smoke-env hako-agent-smoke-opencode-status

# Verify Pi/OMP plugin lifecycle reports without calling providers
agent-smoke-pi-omp-status:
    docker run --rm -v "$PWD:/repo:ro" hako-agent-smoke:local node --experimental-strip-types /usr/local/bin/hako-agent-pi-omp-plugin-status-test /repo/src/integration/assets/pi/hako-agent-state.ts pi
    docker run --rm -v "$PWD:/repo:ro" hako-agent-smoke:local node --experimental-strip-types /usr/local/bin/hako-agent-pi-omp-plugin-status-test /repo/src/integration/assets/omp/hako-agent-state.ts omp
# Print default config
default-config:
    cargo run --release --locked -- --default-config
