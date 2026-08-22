---
status: accepted
---

# Adopt Gardn product identity

The product name is **Gardn**. The canonical machine namespace is `gardn`. The executable, Rust package, application workspace directory, Tegami package scopes, environment-variable prefix, config/cache/state/runtime directories, socket names, integration source prefix, plugin manifest and fields, release assets, and local development binary names use `gardn` or `GARDN` as appropriate. The canonical repository is `https://github.com/masakirocorp/gardn`. The product website is `https://gardn.dev`.

This decision is a clean cutover from the product's prior pre-public identity. Production code does not retain an earlier executable alias, accept earlier product-prefixed environment variables, search earlier config/cache/state/runtime or persistence paths, bind earlier socket names, accept earlier plugin manifest names or fields, or publish earlier release assets. The application does not read old product state or carry migration shims.

Historical provenance is separate from product identity. `ogulcancelik/herdr` and `herdrdev/herdr` are factual external repository identifiers and may appear when identifying upstream commits, source links, licensing provenance, or inherited history. Product-facing references use Gardn.

## Consequences

All machine-visible identity surfaces change as one contract so the `gardn` client, server, integrations, remote bootstrap, updater, release workflow, and documentation agree.

The wire protocol version is 13. The identity cutover breaks public client/server magic and environment contracts even where message framing is otherwise unchanged. Mixed binaries must fail the exact-version handshake instead of entering a compatibility path.

The snapshot version does not change. Gardn writes state under a new namespace and never reads files from the prior product identity, so no snapshot migration contract is exposed.

Users launch `gardn`, store configuration under `~/.config/gardn`, and receive `gardn-*` release assets. Supported managed agent profiles use `GARDN_AGENT=<agent>`. Plugins use `gardn-plugin.toml`, `min_gardn_version`, and `GARDN_*` context variables without earlier-name aliases. Direct API callers keep brand-neutral JSON method names.
