# hako task runner

# Run tests
test:
    cargo nextest run --locked --status-level fail --final-status-level fail --failure-output final --success-output never
    python3 -m unittest scripts.test_vendor_libghostty_vt

# Run fast local lint checks
lint:
    cargo fmt --check
    cargo clippy --all-targets --locked -- -D warnings

# Run PR CI checks
ci: lint
    cargo nextest run --locked --status-level fail --final-status-level fail --failure-output final --success-output never

# Check formatting + run unit tests + maintenance script tests
check: ci
    python3 -m unittest scripts.test_vendor_libghostty_vt
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

# Print default config
default-config:
    cargo run --release --locked -- --default-config
