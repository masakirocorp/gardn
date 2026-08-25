# syntax=docker/dockerfile:1.7
# Runtime image for live agent tests.
# Secrets are never baked in; pass OPENROUTER_API_KEY at docker run time.
# Agent CLI versions are resolved once outside the build (see resolve-versions.mjs)
# and passed as exact build args. Do not pass floating dist-tags such as latest.
FROM node:24-bookworm-slim

ARG CLAUDE_CODE_VERSION
ARG CODEX_VERSION
ARG OPENCODE_VERSION
ARG COPILOT_VERSION
ARG HERMES_VERSION
ARG DROID_VERSION
ARG PI_VERSION
ARG QWEN_CODE_VERSION
ARG KILO_VERSION
ARG PNPM_VERSION=11.6.0
ARG KIMI_VERSION
ARG MAKI_VERSION
ARG OMP_REF
ENV DEBIAN_FRONTEND=noninteractive \
    PIP_BREAK_SYSTEM_PACKAGES=1 \
    COREPACK_ENABLE_DOWNLOAD_PROMPT=0 \
    PNPM_STORE_DIR=/pnpm/store \
    pnpm_config_update_notifier=false \
    PATH=/usr/local/bin:/usr/local/sbin:/usr/sbin:/usr/bin:/sbin:/bin

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        git \
        jq \
        libsecret-1-0 \
        python3 \
        python3-pip \
        unzip \
        tini \
    && rm -rf /var/lib/apt/lists/*

RUN --mount=type=cache,id=gardn-agent-tests-pnpm,target=/pnpm/store \
    set -eux; \
    for required in \
      CLAUDE_CODE_VERSION \
      CODEX_VERSION \
      OPENCODE_VERSION \
      COPILOT_VERSION \
      HERMES_VERSION \
      DROID_VERSION \
      PI_VERSION \
      QWEN_CODE_VERSION \
      KILO_VERSION \
      KIMI_VERSION \
      MAKI_VERSION \
      OMP_REF; do \
      eval "value=\${$required:-}"; \
      if [ -z "$value" ] || [ "$value" = "latest" ]; then \
        echo "refusing floating or empty build arg: $required=${value:-}" >&2; \
        exit 1; \
      fi; \
    done; \
    corepack enable pnpm \
    && corepack prepare "pnpm@${PNPM_VERSION}" --activate \
    && mkdir -p /opt/agent-tests-node \
    && cd /opt/agent-tests-node \
    && printf '%s\n' \
        'minimumReleaseAge: 0' \
        'allowBuilds:' \
        '  "@anthropic-ai/claude-code": true' \
        '  "@kilocode/cli": true' \
        '  "@qwen-code/qwen-code": true' \
        '  "@qwen-code/audio-capture": false' \
        '  "@google/genai": false' \
        '  "droid": true' \
        '  "hermes-agent": true' \
        '  "opencode-ai": true' \
        '  "protobufjs": false' \
        > pnpm-workspace.yaml \
    && printf '%s\n' \
        '{' \
        '  "private": true,' \
        '  "dependencies": {' \
        "    \"@anthropic-ai/claude-code\": \"${CLAUDE_CODE_VERSION}\"," \
        "    \"@openai/codex\": \"${CODEX_VERSION}\"," \
        "    \"opencode-ai\": \"${OPENCODE_VERSION}\"," \
        "    \"@github/copilot\": \"${COPILOT_VERSION}\"," \
        "    \"hermes-agent\": \"${HERMES_VERSION}\"," \
        "    \"droid\": \"${DROID_VERSION}\"," \
        "    \"@earendil-works/pi-coding-agent\": \"${PI_VERSION}\"," \
        "    \"@qwen-code/qwen-code\": \"${QWEN_CODE_VERSION}\"," \
        "    \"@kilocode/cli\": \"${KILO_VERSION}\"" \
        '  }' \
        '}' \
        > package.json \
    && pnpm install --prod \
    && for bin in claude codex opencode copilot hermes droid pi qwen kilo kilocode; do \
        printf '%s\n' '#!/bin/sh' "exec /opt/agent-tests-node/node_modules/.bin/${bin} \"\$@\"" > "/usr/local/bin/${bin}"; \
        chmod +x "/usr/local/bin/${bin}"; \
    done \
    && ln -s /usr/local/bin/kilo /usr/local/bin/kilo-code \
    && pnpm store prune

RUN set -eux; \
    arch="$(dpkg --print-architecture)"; \
    case "$arch" in \
      amd64) kimi_arch=x64 ;; \
      arm64) kimi_arch=arm64 ;; \
      *) echo "unsupported Kimi arch: $arch" >&2; exit 1 ;; \
    esac; \
    version="$KIMI_VERSION"; \
    case "$version" in \
      latest|"") echo "KIMI_VERSION must be an exact resolved version" >&2; exit 1 ;; \
    esac; \
    base_url="https://github.com/MoonshotAI/kimi-code/releases/download/%40moonshot-ai%2Fkimi-code%40${version}"; \
    curl -fsSL "$base_url/kimi-code-linux-${kimi_arch}.zip" -o "/tmp/kimi-code-linux-${kimi_arch}.zip"; \
    curl -fsSL "$base_url/kimi-code-linux-${kimi_arch}.zip.sha256" -o "/tmp/kimi-code-linux-${kimi_arch}.zip.sha256"; \
    cd /tmp; \
    sha256sum -c "kimi-code-linux-${kimi_arch}.zip.sha256"; \
    unzip -q "/tmp/kimi-code-linux-${kimi_arch}.zip" -d /opt/kimi-code; \
    kimi_bin="$(find /opt/kimi-code -type f -name kimi | sort | tail -n 1)"; \
    chmod +x "$kimi_bin"; \
    ln -s "$kimi_bin" /usr/local/bin/kimi; \
    rm -f "/tmp/kimi-code-linux-${kimi_arch}.zip" "/tmp/kimi-code-linux-${kimi_arch}.zip.sha256"

# Maki publishes static musl binaries for the two architectures used by CI.
RUN set -eux; \
    arch="$(dpkg --print-architecture)"; \
    case "$arch" in \
      amd64) maki_arch=x86_64 ;; \
      arm64) maki_arch=aarch64 ;; \
      *) echo "unsupported Maki arch: $arch" >&2; exit 1 ;; \
    esac; \
    case "$MAKI_VERSION" in \
      latest|"") echo "MAKI_VERSION must be an exact resolved tag" >&2; exit 1 ;; \
      v*) tag="$MAKI_VERSION" ;; \
      *) tag="v$MAKI_VERSION" ;; \
    esac; \
    curl -fsSL "https://github.com/tontinton/maki/releases/download/${tag}/maki-${tag}-${maki_arch}-unknown-linux-musl.tar.gz" \
      | tar xz -C /usr/local/bin; \
    chmod +x /usr/local/bin/maki

RUN curl -fsSL https://cursor.com/install | bash \
    && mkdir -p /opt \
    && cp -a /root/.local/share/cursor-agent /opt/cursor-agent \
    && cursor_bin="$(find /opt/cursor-agent/versions -mindepth 2 -maxdepth 2 -type f -name cursor-agent | sort | tail -n 1)" \
    && chmod -R a+rX /opt/cursor-agent \
    && ln -s "$cursor_bin" /usr/local/bin/cursor-agent \
    && ln -s "$cursor_bin" /usr/local/bin/agent \
    && rm -rf /root/.local/share/cursor-agent /root/.local/bin/agent /root/.local/bin/cursor-agent

RUN curl -fsSL https://qoder.com/install | bash \
    && qoder_bin="$(find /root/.qoder/bin/qodercli -type f -name 'qodercli-*' | sort | tail -n 1)" \
    && install -m 0755 "$qoder_bin" /usr/local/bin/qodercli \
    && ln -s /usr/local/bin/qodercli /usr/local/bin/qoder \
    && rm -rf /root/.qoder /root/.local/bin/qodercli /tmp/qodercli-natives-* /tmp/.setup-qodercli-natives-*

# OMP is distributed as a compact release binary. Pi is installed separately above so
# each integration smoke exercises the current release of the CLI it claims to cover.
RUN set -eux; \
    arch="$(dpkg --print-architecture)"; \
    case "$arch" in \
      amd64) omp_arch=x64 ;; \
      arm64) omp_arch=arm64 ;; \
      *) echo "unsupported OMP arch: $arch" >&2; exit 1 ;; \
    esac; \
    case "$OMP_REF" in \
      latest|"") echo "OMP_REF must be an exact resolved release tag" >&2; exit 1 ;; \
    esac; \
    curl -fsSL "https://github.com/can1357/oh-my-pi/releases/download/${OMP_REF}/omp-linux-${omp_arch}" -o /usr/local/bin/omp; \
    chmod +x /usr/local/bin/omp

COPY test-env.sh /usr/local/bin/gardn-agent-tests-env
COPY doctor.sh /usr/local/bin/gardn-agent-tests-doctor
COPY verify-env.sh /usr/local/bin/gardn-agent-tests-verify-env
COPY opencode-cli-test.sh /usr/local/bin/gardn-agent-tests-opencode
COPY test-models.sh /usr/local/lib/gardn-agent-test-models.sh
COPY opencode-status-test.sh /usr/local/bin/gardn-agent-tests-opencode-status
COPY opencode-plugin-status-test.mjs /usr/local/bin/gardn-agent-opencode-plugin-status-test
COPY pi-omp-plugin-status-test.mjs /usr/local/bin/gardn-agent-pi-omp-plugin-status-test
COPY pi-omp-status-test.sh /usr/local/bin/gardn-agent-tests-pi-omp-status
COPY claude-status-test.sh /usr/local/bin/gardn-agent-tests-claude-status
COPY codex-status-test.sh /usr/local/bin/gardn-agent-tests-codex-status
COPY remaining-status-test.sh /usr/local/bin/gardn-agent-tests-remaining-status
COPY cursor-openrouter-proxy.mjs /usr/local/bin/gardn-agent-cursor-openrouter-proxy
COPY cursor-proxy-status-test.sh /usr/local/bin/gardn-agent-tests-cursor-proxy-status
COPY qoder-openrouter-proxy.mjs /usr/local/bin/gardn-agent-qoder-openrouter-proxy
COPY qoder-proxy-status-test.sh /usr/local/bin/gardn-agent-tests-qoder-proxy-status
COPY maki-status-test.sh /usr/local/bin/gardn-agent-tests-maki-status
COPY qwen-status-test.sh /usr/local/bin/gardn-agent-tests-qwen-status
COPY kilo-status-test.sh /usr/local/bin/gardn-agent-tests-kilo-status
COPY run-target.sh /usr/local/bin/gardn-agent-tests-target
# Pre-resolved cohort manifest written by resolve-versions.mjs before docker build.
COPY cohort.json /usr/local/share/gardn-agent-tests/cohort.json
RUN chmod +x /usr/local/bin/gardn-agent-tests-env /usr/local/bin/gardn-agent-tests-doctor /usr/local/bin/gardn-agent-tests-verify-env /usr/local/bin/gardn-agent-tests-opencode /usr/local/bin/gardn-agent-tests-opencode-status /usr/local/bin/gardn-agent-tests-pi-omp-status /usr/local/bin/gardn-agent-tests-claude-status /usr/local/bin/gardn-agent-tests-codex-status /usr/local/bin/gardn-agent-tests-remaining-status /usr/local/bin/gardn-agent-tests-cursor-proxy-status /usr/local/bin/gardn-agent-tests-qoder-proxy-status /usr/local/bin/gardn-agent-tests-maki-status /usr/local/bin/gardn-agent-tests-qwen-status /usr/local/bin/gardn-agent-tests-kilo-status /usr/local/bin/gardn-agent-tests-target /usr/local/bin/gardn-agent-cursor-openrouter-proxy /usr/local/bin/gardn-agent-qoder-openrouter-proxy /usr/local/bin/gardn-agent-opencode-plugin-status-test /usr/local/bin/gardn-agent-pi-omp-plugin-status-test \
    && useradd --create-home --shell /bin/bash agenttest \
    && chmod 0644 /usr/local/share/gardn-agent-tests/cohort.json

USER agenttest
WORKDIR /work
ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["gardn-agent-tests-doctor"]
