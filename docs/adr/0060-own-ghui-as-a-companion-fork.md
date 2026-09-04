---
status: accepted
---

# Own ghui as a separately released companion fork

Gardn uses `masakirocorp/ghui` as its curated GitHub interface. The fork remains a separate MIT-licensed work. Gardn does not copy ghui source into the Gardn repository or link it into the AGPL binary.

Gardn pins each supported ghui integration to one immutable fork release and source commit. The curated launcher rejects a different ghui version instead of silently losing launch-scoped behavior. A user can configure a different `[commands].github` command to opt out of the curated integration.

The fork owns Gardn-specific launch inputs. These include terminal theme selection, visible scrollbars, and optional GitHub organization scope. Organization scope filters the ghui home repository, pull request, and issue collections. It does not change explicit repository views.

Masakiro publishes ghui from its own repository and release workflow. Release assets retain Kit Langton's copyright notice and the MIT License. Gardn documents the pinned release and exposes the same acknowledgment in **Settings > About**. A Masakiro Homebrew tap can package the immutable release assets without transferring update ownership to Gardn.

Upstream changes enter the fork through explicit merges from `kitlangton/ghui`. Gardn treats upstream as input, not authority. The fork can diverge when Gardn needs a product invariant that upstream does not accept or schedule.

## Current rationale

Gardn is mouse-first. ghui has the interaction model that fits that product direction, but upstream does not currently provide Group-level organization scope or all required launch-only presentation controls. An unpinned executable can ignore those inputs and expose unscoped data. A reviewed fork release makes the behavior and license boundary explicit.

Bundling ghui into Gardn would couple two release cadences and mix optional third-party source into the Gardn build. A separately released companion keeps the dependency replaceable while Masakiro still owns the exact behavior that the curated launcher requires.

## Consequences

A Gardn release that changes the ghui contract must first publish and verify a compatible fork release. The Gardn pin, installation guidance, tests, documentation, and acknowledgment must change together.

Fork releases require upstream synchronization, platform assets, checksums, and license retention. Gardn must fail closed when organization scope is configured but the required fork version is unavailable.

The default `ghui` command remains optional. Browser, review, editor, and custom GitHub commands keep their independent installation and update paths.
