---
status: accepted
---

# Treat upstream as signal in the product fork

Gardn is a long-lived Masakiro product fork of `ogulcancelik/herdr`, not a downstream mirror. Repository policy expects `origin` to point to `masakirocorp/gardn`, `upstream` to point to `ogulcancelik/herdr`, and product trunk to be `origin/master`. Changes from upstream are candidate input: Gardn ports behavior by invariant after checking product context, tests, identity, release policy, docs, website, and sensitive plumbing.

This is accepted because Gardn has independent product identity, release history, docs/site direction, update channel/plumbing, workflows, and repository policy. Upstream can still contain valuable fixes and behavior changes, but those changes must be interpreted through Gardn's product model instead of trusted wholesale.

The checked-in automation reflects that posture: `just sync-upstream` creates an explicit `sync/upstream-YYYY-MM-DD` branch, fetches `origin` and `upstream`, merges `upstream/master` with a merge commit, runs the upstream-sync guard, writes the guard report and PR body, pushes the sync branch, and opens a PR in `masakirocorp/gardn` with base `master`. Review before merge remains repository policy, not something the script can prove by itself.

## Considered options

- Treat upstream as authority and keep Gardn mechanically aligned with it. Rejected because an automatic alignment can resurrect upstream identity, docs, website, release plumbing, or behavior that conflicts with Gardn's product direction.
- Ignore upstream after forking. Rejected because upstream remains useful signal for bug fixes, agent/session behavior, terminal handling, CI hardening, and maintenance ideas.
- Treat upstream as signal and port by invariant. Accepted because it preserves Gardn ownership while still harvesting upstream improvements when the same invariant applies.

## Consequences

Every upstream port must identify the invariant the upstream change protects, check whether Gardn has the same context, and add or adjust Gardn tests for that invariant before merging. Upstream syncs use explicit sync branches and merge commits; they do not rebase or squash upstream history into product trunk. Sync PRs target `masakirocorp/gardn:master`, not upstream, and must pass `pnpm check` plus PR CI before merge. If an upstream sync conflicts, resolve toward Gardn product identity first, rerun `python3 scripts/guard_upstream_sync.py --base origin/master --upstream upstream/master --head HEAD`, then run `pnpm check`.

Gardn-owned paths are protected by `.gitattributes` with `merge=keep-gardn`: `README.md`, `AGENTS.md`, `SKILL.md`, `apps/gardn/assets/logo.svg`, `/docs/**`, and `/website/**`. Protection does not mean ignore upstream changes; it means review them against Gardn's custom identity, documentation, site, and product direction. The upstream-sync guard also flags required identity tokens, forbidden upstream identity/plumbing resurrection, sensitive review-required paths, docs/site deletions, and upstream-port ledger gaps.

Historical rationale beyond the current repository instructions is `[INFERENCE]`: this policy likely exists because Gardn inherited useful code from upstream while becoming a separate product whose name, release line, update channel, docs, website, and operational workflow must not be silently overwritten by future upstream syncs.
