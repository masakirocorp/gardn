# syntax=docker/dockerfile:1.7
# Small runtime image for optional real-agent smoke tests.
# Secrets are never baked in; pass OPENROUTER_API_KEY at docker run time.
FROM node:24-bookworm-slim

ARG CLAUDE_CODE_VERSION=2.1.175
ARG CODEX_VERSION=0.139.0
ARG OPENCODE_VERSION=1.17.4
ARG COPILOT_VERSION=1.0.61
ARG HERMES_VERSION=0.16.0
ARG DROID_VERSION=0.144.2
ARG PNPM_VERSION=11.6.0
ARG OMP_REF=v15.12.3

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
        tini \
    && rm -rf /var/lib/apt/lists/*

RUN --mount=type=cache,id=hako-agent-smoke-pnpm,target=/pnpm/store \
    corepack enable pnpm \
    && corepack prepare "pnpm@${PNPM_VERSION}" --activate \
    && mkdir -p /opt/agent-smoke-node \
    && cd /opt/agent-smoke-node \
    && printf '%s\n' \
        'minimumReleaseAge: 0' \
        'allowBuilds:' \
        '  "@anthropic-ai/claude-code": true' \
        '  "droid": true' \
        '  "hermes-agent": true' \
        '  "opencode-ai": true' \
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
        "    \"droid\": \"${DROID_VERSION}\"" \
        '  }' \
        '}' \
        > package.json \
    && pnpm install --prod \
    && for bin in claude codex opencode copilot hermes droid; do \
        printf '%s\n' '#!/bin/sh' "exec /opt/agent-smoke-node/node_modules/.bin/${bin} \"\$@\"" > "/usr/local/bin/${bin}"; \
        chmod +x "/usr/local/bin/${bin}"; \
    done \
    && pnpm store prune

RUN KIMI_INSTALL_DIR=/opt/kimi-code KIMI_NO_MODIFY_PATH=1 \
    bash -c 'curl -fsSL https://code.kimi.com/kimi-code/install.sh | bash' \
    && ln -s /opt/kimi-code/bin/kimi /usr/local/bin/kimi

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

# OMP is distributed as a compact release binary. Hako also supports `pi`; keep a
# `pi` entrypoint so launch/profile plumbing can be exercised without installing
# a second copy of the same agent family.
RUN set -eux; \
    arch="$(dpkg --print-architecture)"; \
    case "$arch" in \
      amd64) omp_arch=x64 ;; \
      arm64) omp_arch=arm64 ;; \
      *) echo "unsupported OMP arch: $arch" >&2; exit 1 ;; \
    esac; \
    if [ -n "$OMP_REF" ]; then \
      tag="$OMP_REF"; \
    else \
      tag="$(curl -fsSL https://api.github.com/repos/can1357/oh-my-pi/releases/latest | jq -r .tag_name)"; \
    fi; \
    curl -fsSL "https://github.com/can1357/oh-my-pi/releases/download/${tag}/omp-linux-${omp_arch}" -o /usr/local/bin/omp; \
    chmod +x /usr/local/bin/omp; \
    ln -s /usr/local/bin/omp /usr/local/bin/pi

COPY smoke-env.sh /usr/local/bin/hako-agent-smoke-env
COPY doctor.sh /usr/local/bin/hako-agent-smoke-doctor
COPY verify-env.sh /usr/local/bin/hako-agent-smoke-verify-env
COPY opencode-smoke.sh /usr/local/bin/hako-agent-smoke-opencode
COPY opencode-status-smoke.sh /usr/local/bin/hako-agent-smoke-opencode-status
COPY opencode-plugin-status-test.mjs /usr/local/bin/hako-agent-opencode-plugin-status-test
COPY pi-omp-plugin-status-test.mjs /usr/local/bin/hako-agent-pi-omp-plugin-status-test
COPY pi-omp-status-smoke.sh /usr/local/bin/hako-agent-smoke-pi-omp-status
COPY claude-status-smoke.sh /usr/local/bin/hako-agent-smoke-claude-status
COPY codex-status-smoke.sh /usr/local/bin/hako-agent-smoke-codex-status
RUN chmod +x /usr/local/bin/hako-agent-smoke-env /usr/local/bin/hako-agent-smoke-doctor /usr/local/bin/hako-agent-smoke-verify-env /usr/local/bin/hako-agent-smoke-opencode /usr/local/bin/hako-agent-smoke-opencode-status /usr/local/bin/hako-agent-smoke-pi-omp-status /usr/local/bin/hako-agent-smoke-claude-status /usr/local/bin/hako-agent-smoke-codex-status /usr/local/bin/hako-agent-opencode-plugin-status-test /usr/local/bin/hako-agent-pi-omp-plugin-status-test \
    && useradd --create-home --shell /bin/bash smoke

USER smoke
WORKDIR /work
ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["hako-agent-smoke-doctor"]
