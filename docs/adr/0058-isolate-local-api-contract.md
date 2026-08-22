---
status: accepted
---

# Isolate the Local API contract in a leaf crate

Gardn keeps the pure Local API compatibility contract in the `gardn-local-api` workspace crate. The crate owns request, response, event, subscription, integration, and plugin DTOs, their Serde and JSON Schema implementations, schema generation, and explicit serialization fixtures. It depends only on contract libraries and does not depend on the `gardn` application crate.

The `gardn` crate owns transport, dispatch, runtime state, configuration, terminal behavior, and presentation. Its `api::schema` facade re-exports the contract and contains explicit adapters for runtime-owned types. Product and client protocol versions are injected when `gardn` generates the public schema, so the contract crate does not depend on build or runtime modules.

## Current rationale

Compile profiling identified Local API derives as the largest macro-expansion surface in the application crate. A leaf crate lets Cargo cache that stable surface independently when ordinary runtime or UI source changes. Matched local measurements found no material clean-build regression and reduced repeated touched-source development builds by approximately 10% on the measured M4 workstation.

## Consequences

Local API compatibility changes must update `gardn-local-api` and its explicit fixtures. Runtime-owned types must cross the facade through explicit adapters; adding an `gardn` dependency or UI/runtime dependency to `gardn-local-api` would collapse the cache boundary and is not allowed. Local API transport and handlers remain in `gardn`; this decision does not merge the Local API with the binary client wire protocol defined by ADR 0013.

## Rejected alternatives

The thin-client wire protocol offered a cleaner seam but isolated substantially fewer derive expansions. Config and persistence types had wider runtime dependencies and required larger domain changes. Compiler backends, linkers, and compiler caches were measured separately and were neutral or slower for the current development workload, so they do not replace this structural boundary.
