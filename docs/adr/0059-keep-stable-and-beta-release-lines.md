---
status: accepted
---

# Keep stable and beta as official release lines

Gardn publishes two official Direct Install release lines from the same GitHub repository.

Stable tags are `vX.Y.Z`. They are not GitHub prereleases. Direct Installs whose embedded version has no prerelease follow `/releases/latest`.

Beta tags are `vX.Y.Z-beta.N`. They are GitHub prereleases. Direct Installs whose embedded version is `-beta.N` follow the newest parseable beta tag. Install the beta binary as `gardn-beta` so rollback does not overwrite `~/.local/bin/gardn`.

Both lines embed `GARDN_BUILD_CHANNEL=release`, so they share `~/.config/gardn` and the same Shared Session State. A second process in that namespace attaches as a Thin Client. Attach requires the running server's Local API version and protocol to match this binary. Mixed stable/beta attach fails closed. Switch with `gardn server stop` or Live Handoff, then launch the other binary.

`just release` remains the stable cut. It refuses to tag if Cargo.toml still contains a prerelease. `just release-beta` runs Tegami with `GARDN_TEGAMI_PRERELEASE=beta` for that invocation only. Do not set `prerelease: "beta"` in the default Tegami paper.

This is not a third Build Channel, not a Session Namespace, and not upstream's preview update channel.

## Current rationale

Source builds already use `gardn-dev` and a separate config directory. Beta needs CI-signed binaries against the daily session, so it must stay an official release build. Overwriting the only `gardn` binary with a channel setting makes rollback depend on a download. Two Direct Install paths keep stable local.

Protocol Version is exact-match and is not app version. Without a Local API version check, `gardn-beta` would attach to a live stable server whenever protocol matched.

## Consequences

Cargo.toml, the git tag, and `GARDN_RELEASE_TAG` stay equal, including `-beta.N`. The updater Version parser accepts only stable triples or `-beta.N`. Unknown prerelease ids are not Gardn release lines. Managed installs still do not self-update from GitHub.
