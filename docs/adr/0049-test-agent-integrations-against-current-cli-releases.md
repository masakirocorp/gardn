---
status: accepted
---

# Test agent integrations against current CLI releases

Hako's optional real-agent smoke image installs the current release of each supported coding-agent CLI by default. The image keeps build tooling pinned where needed, such as pnpm, but does not pin the agent CLIs themselves. Push builds verify that the image still builds and that provider configuration wiring is sane. Scheduled and manually dispatched runs exercise the real agent CLIs through the published image and configured provider secrets.

This is separate from ADR 0016's agent profile and integration authority boundary, and ADR 0048's state-evidence precedence. Those ADRs record how Hako interprets agent reports once they arrive. This ADR records which upstream CLI versions the smoke workflow treats as the compatibility target.

## Current rationale

The supported coding agents are fast-moving developer tools. Users normally run recently updated CLIs, not the exact versions Hako tested when a commit landed. Pinning every smoke image agent version would make CI stable but would prove compatibility with stale surfaces and delay detection when an agent changes hook shape, environment requirements, model configuration, or status behavior.

The smoke workflow intentionally accepts some external volatility. A current-agent smoke failure may be caused by Hako, by an upstream CLI change, or by provider availability. That is acceptable because these checks are optional scheduled/manual compatibility probes, not the core deterministic `just check` gate.

## Considered options

- Pin every agent CLI version in the smoke image. Rejected because it would make real-agent parity tests less representative of users who update frequently, and would hide upstream compatibility drift until users report it.
- Float all build tooling and agent CLIs. Rejected because installer/tooling churn would make the image itself noisy for reasons unrelated to agent behavior.
- Pin image tooling but install current agent CLIs by default. Accepted because it keeps the container construction mostly reproducible while making the tested agent surfaces match the current user-facing ecosystem.

## Consequences

Smoke failures must be triaged as compatibility signals, not automatically treated as Hako regressions. When an upstream CLI change breaks Hako integration behavior, prefer updating the integration or smoke setup over pinning the old CLI unless the new upstream release is clearly broken and temporary quarantine is needed.

If a specific agent temporarily needs a version override for diagnosis, the Dockerfile build args may still be used deliberately. Such overrides should not become the default without recording why current releases are no longer the desired compatibility target.
