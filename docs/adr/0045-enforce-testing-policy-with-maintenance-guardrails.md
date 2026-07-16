---
status: accepted
---

# Enforce testing policy with maintenance guardrails

Oh My Herdr encodes selected test-design rules as maintenance tests, not only as reviewer guidance. The guardrail script rejects mechanically detectable regressions such as integration tests that connect to Unix sockets without the retrying readiness helper, render tests without visible assertions, test bodies that mutate environment variables directly, no-panic-only smoke tests, protocol error assertions that check only `is_err()`, and missing golden byte framing fixtures.

The guardrails are intentionally narrow. They catch high-signal test smells that are easy to detect, while human review still decides whether assertions are behavioral, public-seam oriented, and refactor-resistant.

This is separate from ADR 0001 and ADR 0005. Those ADRs make app state and orchestration testable; this ADR records the repository-level decision to enforce part of Oh My Herdr's testing policy mechanically through standard recipes and CI maintenance checks.

## Current rationale

`[INFERENCE]` Oh My Herdr uses maintenance guardrails because AI and human contributors both tend to drift toward weak tests under pressure. Encoding the clearest anti-patterns in scripts preserves test quality without requiring every reviewer to remember every policy detail.

## Consequences

New testing policy that is objective and high-signal can become a maintenance guardrail. Guardrails should stay narrow and should fail with actionable messages; subjective test quality remains a review responsibility, not a regex job.
