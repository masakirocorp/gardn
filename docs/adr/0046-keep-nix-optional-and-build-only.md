---
status: accepted
---

# Keep Nix optional and build-only

Gardn provides Nix flake outputs for users who already use Nix, but Nix is not the authoritative test, update, or release channel. The flake exposes packages, apps, checks, dev shells, a formatter, and an overlay; the README directs Nix users to update through their own Nix workflow, while Gardn's direct updater and release process remain based on GitHub Release assets.

The Nix derivation builds Gardn from source with Rust checks disabled, and the dedicated Nix workflow runs a current-system `nix flake check` build plus an all-systems `nix flake check --all-systems --no-build` shape evaluation. That keeps Nix useful as a native packaging and development path without making the project maintain a second Rust test suite or multi-platform release pipeline.

This is separate from ADR 0012's install ownership decision. ADR 0012 records why managed installs do not self-update through Gardn; this ADR records the narrower flake topology and the choice to keep Nix optional rather than authoritative.

## Current rationale

`[INFERENCE]` Gardn supports Nix because some users and maintainers expect reproducible flake outputs and dev shells, but making Nix the primary release or test path would duplicate existing CI/release ownership and slow ordinary Rust iteration. A build-only flake path gives Nix users a native interface while preserving one main release line.

## Consequences

New release-critical checks should stay in the normal just/CI/release flow unless the project deliberately makes Nix authoritative. Nix changes should keep serving packaging, development shells, overlays, and flake checks without becoming a second source of release truth.
