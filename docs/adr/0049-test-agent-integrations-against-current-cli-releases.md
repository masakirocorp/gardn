---
status: accepted
---

# Test agent integrations against current CLI releases

Gardn's optional real-agent smoke image installs the current release of each supported coding-agent CLI by default. The image keeps build tooling pinned where needed, such as pnpm, but does not pin the agent CLIs themselves. Push builds verify that the image still builds, that provider configuration wiring is sane, and publish the image to GHCR for scheduled/manual compatibility probes. Scheduled and manually dispatched runs exercise the real agent CLIs through that image and configured provider secrets.

Real-agent smokes use OpenRouter-backed model configuration with an ordered free-model fallback list where the CLI supports a usable BYOK/provider path. A single unavailable, removed, rate-limited, or timed-out model is provider volatility, not a Gardn result. Smoke scripts may retry whole smoke scenarios with the next candidate model before they reach Gardn assertions. Once a CLI run produces a valid provider response, missing status reports, wrong state ordering, bad hook metadata, or missing proxy routing remain hard test failures. Cursor and Qoder are explicit exceptions: their CI coverage uses a deterministic local inference proxy because their current CLIs do not expose the same direct OpenRouter path Gardn can drive for the other agents. Devin is a different explicit exception: its current CLI auth/model contract is Devin-account based, and Gardn's Devin integration hook reports session identity only, so CI covers the hook seam instead of spending a credentialed real Devin session.

This is separate from ADR 0016's agent profile and integration authority boundary and ADR 0048's state-evidence precedence. Those ADRs record how Gardn interprets agent reports once they arrive. This ADR records which upstream CLI versions and provider behavior the smoke workflow treats as the compatibility target.

## Current coverage contract

The smoke workflow must be explicit about coverage level per agent. A row is not "covered" unless the workflow exercises that exact behavior against the real CLI or the documented proxy seam.

| Agent | status/lifecycle smoke | Notes |
|---|---|---|
| OpenCode | real CLI + plugin | OpenRouter model syntax uses OpenCode's `openrouter/...` form. |
| Pi | real CLI + extension | Uses the Pi extension seam. |
| OMP | real CLI + extension | Uses the OMP extension seam. |
| Claude | real CLI + hooks | Routed through OpenRouter-compatible Anthropic env. |
| Codex | real CLI transport + hook seam | Codex `exec` still limits hook assertions. |
| Copilot | real CLI + hooks | BYOK/OpenRouter provider path. |
| Droid | real CLI + hooks | BYOK/OpenRouter provider path. |
| Kimi | real CLI + hooks | BYOK/OpenRouter provider path. |
| Hermes | real CLI + plugin | BYOK/OpenRouter provider path. |
| Cursor | proxy/auth contract + hook proxy | Requires Cursor-specific proxy handling; the smoke verifies real Cursor CLI launch, Gardn hooks, and deterministic proxied response delivery. |
| Qoder | proxy/auth contract + hook proxy | Requires Qoder token/proxy handling; the smoke verifies real Qoder CLI launch, Gardn hooks, and deterministic proxied response delivery. |
| Devin | hook seam only | Devin CLI auth/model selection is tied to Devin account access, not Gardn's OpenRouter smoke path. The current Gardn hook only reports `pane.report_agent_session`, so the smoke asserts session identity and stale-list suppression rather than lifecycle states. |

## Current rationale

The supported coding agents are fast-moving developer tools. Users normally run recently updated CLIs, not the exact versions Gardn tested when a commit landed. Pinning every smoke image agent version would make CI stable but would prove compatibility with stale surfaces and delay detection when an agent changes hook shape, environment requirements, model configuration, or status behavior.

Provider availability is a separate volatile dependency. Free OpenRouter models can disappear, timeout, or reject a route independently of Gardn. Retrying complete smoke scenarios against an ordered fallback list preserves the compatibility signal while keeping the assertion boundary honest: retry before Gardn receives a provider-backed answer, fail after Gardn-observable behavior is wrong.

The smoke workflow intentionally accepts some external volatility. A current-agent smoke failure may be caused by Gardn, by an upstream CLI change, or by provider availability. That is acceptable because these checks are optional scheduled/manual compatibility probes, not the core deterministic `pnpm check` gate.

## Considered options

- Pin every agent CLI version in the smoke image. Rejected because it would make real-agent parity tests less representative of users who update frequently, and would hide upstream compatibility drift until users report it.
- Float all build tooling and agent CLIs. Rejected because installer/tooling churn would make the image itself noisy for reasons unrelated to agent behavior.
- Pin image tooling but install current agent CLIs by default. Accepted because it keeps the container construction mostly reproducible while making the tested agent surfaces match the current user-facing ecosystem.

- Use one hardcoded free model and fail on any provider issue. Rejected because removed or unavailable free models would make the smoke harness noisy without proving anything about Gardn.
- Retry complete smoke scenarios through an ordered free-model fallback list, but do not retry assertion failures. Accepted because it separates provider volatility from Gardn behavior while avoiding partial-run mixed-model state.

## Consequences

Smoke failures must be triaged as compatibility signals, not automatically treated as Gardn regressions. When an upstream CLI change breaks Gardn integration behavior, prefer updating the integration or smoke setup over pinning the old CLI unless the new upstream release is clearly broken and temporary quarantine is needed.

Fallback model lists should be explicit, printed in logs, and routed through the same CLI configuration path the smoke is validating. A script may transform model identifiers only when a CLI requires its own provider/model syntax, such as OpenCode's `openrouter/...` model ids.

Smokes that proxy a CLI to OpenRouter must assert that the intended network route was actually exercised when that is the feature under test. It is acceptable to preserve the real vendor catalog/auth path if the proxy bypasses local host overrides safely and still verifies the intercepted inference route.

If a specific agent temporarily needs a version override for diagnosis, the Dockerfile build args may still be used deliberately. Such overrides should not become the default without recording why current releases are no longer the desired compatibility target.
