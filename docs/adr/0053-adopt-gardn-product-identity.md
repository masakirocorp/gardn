---
status: accepted
---

# Adopt Gardn product identity

The product name is **Gardn**. Its canonical machine namespace is `gardn`: the executable, Rust package, application workspace directory, Tegami package scope, environment-variable prefix, config/cache/runtime directories, socket names, integration source prefix, release assets, and local development binary names use `gardn` or `GARDN` as appropriate. The canonical repository and current product homepage are `https://github.com/masakirocorp/gardn`; a dedicated product domain is not established yet.

This is a clean product-identity cutover. Production code does not retain a `gardn` executable alias, accept `GARDN_*` environment variables, search Gardn-named runtime or persistence paths, or publish Gardn-named release assets. Existing pre-public development state may be copied once outside the product into the new namespace, but compatibility shims do not remain in the application.

Gardn remains the upstream project and historical attribution. References that identify `ogulcancelik/herdr`, upstream behavior, licensing provenance, or intentional Gardn compatibility remain Gardn-branded; they are not part of the former Gardn product namespace.

## Consequences

All machine-visible identity surfaces must change atomically enough that the `gardn` client, server, integrations, remote bootstrap, updater, release workflow, and documentation agree. A wire-protocol version bump is required only if framed client/server messages change; renaming process paths, environment variables, sockets, or Local API integration-source labels does not by itself change the wire format.

Users launch `gardn`, store configuration under `~/.config/gardn`, and receive `gardn-*` release assets. Supported managed agent profiles use `GARDN_AGENT=<agent>`. Direct API callers continue to use the existing JSON method names unless a method itself contains the former product identity.
