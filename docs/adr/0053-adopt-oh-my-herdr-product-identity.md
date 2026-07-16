---
status: accepted
---

# Adopt Oh My Herdr product identity

The product name is **Oh My Herdr**. Its canonical machine namespace is `omh`: the executable, Rust package, application workspace directory, Tegami package scope, environment-variable prefix, config/cache/runtime directories, socket names, integration source prefix, release assets, and local development binary names use `omh` or `OMH` as appropriate. The canonical repository and current product homepage are `https://github.com/masakirocorp/oh-my-herdr`; a dedicated product domain is not established yet.

This is a clean product-identity cutover. Production code does not retain a `hako` executable alias, accept `HAKO_*` environment variables, search Hako-named runtime or persistence paths, or publish Hako-named release assets. Existing pre-public development state may be copied once outside the product into the new namespace, but compatibility shims do not remain in the application.

Herdr remains the upstream project and historical attribution. References that identify `ogulcancelik/herdr`, upstream behavior, licensing provenance, or intentional Herdr compatibility remain Herdr-branded; they are not part of the former Hako product namespace.

## Consequences

All machine-visible identity surfaces must change atomically enough that the `omh` client, server, integrations, remote bootstrap, updater, release workflow, and documentation agree. A wire-protocol version bump is required only if framed client/server messages change; renaming process paths, environment variables, sockets, or Local API integration-source labels does not by itself change the wire format.

Users launch `omh`, store configuration under `~/.config/omh`, and receive `omh-*` release assets. Supported managed agent profiles use `OMH_AGENT=<agent>`. Direct API callers continue to use the existing JSON method names unless a method itself contains the former product identity.
