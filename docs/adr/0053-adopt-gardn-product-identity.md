---
status: accepted
---

# Adopt Gardn product identity

The product name is **Gardn**. The canonical machine namespace is `gardn`. The executable, Rust package, application workspace directory, Tegami package scopes, environment-variable prefix, config/cache/state/runtime directories, socket names, integration source prefix, plugin manifest and fields, release assets, and local development binary names use `gardn` or `GARDN` as appropriate. The canonical repository is `https://github.com/masakirocorp/gardn`. The product website is `https://gardn.dev`.

This decision is a clean cutover from the product's prior pre-public identity. Production code does not retain an earlier executable alias, search earlier config/cache/state/runtime or persistence paths, bind earlier socket names, or publish earlier release assets. The application does not read old product state or carry storage migration shims. Herdr plugin compatibility is a separate, bounded exception for manifest fields and process environment names.

Historical provenance is separate from product identity. `ogulcancelik/herdr` and `herdrdev/herdr` are factual external repository identifiers and may appear when identifying upstream commits, source links, licensing provenance, or inherited history. Product-facing references use Gardn.

## Consequences

All machine-visible identity surfaces change as one contract so the `gardn` client, server, integrations, remote bootstrap, updater, release workflow, and documentation agree.

The wire protocol version is 13. The identity cutover breaks public client/server magic and environment contracts even where message framing is otherwise unchanged. Mixed binaries must fail the exact-version handshake instead of entering a compatibility path.

The snapshot version does not change. Gardn writes state under a new namespace and never reads files from the prior product identity, so no snapshot migration contract is exposed.

Users launch `gardn`, store configuration under `~/.config/gardn`, and receive `gardn-*` release assets. Supported managed agent profiles use `GARDN_AGENT=<agent>`. Gardn also accepts `herdr-plugin.toml` and `min_herdr_version` through the independently audited Herdr plugin API ceiling. Plugin processes receive protected `HERDR_*` aliases pointing to Gardn's executable, socket, and per-plugin paths. On macOS and Linux, `HERDR_AGENT` is a process-detection fallback only when `GARDN_AGENT` is absent. An empty or invalid canonical value still takes precedence. These exceptions permit upstream plugins without importing upstream storage or executable identity. Direct API callers keep brand-neutral JSON method names.
