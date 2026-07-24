---
status: accepted
---

# Use GitHub Releases for direct updates

Oh My Herdr direct installs update from `masakirocorp/oh-my-herdr` GitHub Releases. `just release` is the release entry point: it requires a clean tree, verifies every pending Tegami changefile includes `omh`, runs `CI=true pnpm tegami version`, runs `pnpm check`, commits Tegami's version/changelog changes, tags `v<version>`, and pushes the branch and tag. The `Release` GitHub Actions workflow runs on `v*` tags, verifies the tag matches `apps/omh/Cargo.toml`, builds the supported binary release assets through Turbo, and publishes them on the GitHub Release.

The updater treats those releases as the source of truth for direct binary installs. `apps/omh/src/update.rs` fetches `/repos/masakirocorp/oh-my-herdr/releases/latest`, parses the release tag after stripping an optional `v` prefix, requires a platform asset named `omh-{os}-{arch}` or `omh-windows-x86_64.exe`, stores the trimmed release body as pending release notes or falls back to `Oh My Herdr v<version>`, downloads the selected asset, and swaps the current executable during `omh update` where the platform supports in-place replacement. Windows builds are release assets, but Windows self-update remains guarded by platform support.

Managed installs keep their package manager as the installer. mise and Nix installs are detected from their install paths; `omh update` refuses to replace them and update-ready UI routes installation to `mise upgrade omh` or Nix guidance, while their availability notification currently comes from the GitHub latest-release check. Background checks only notify and save release notes; installation remains an explicit user action.

## Current rationale

`[INFERENCE]` Do not let package-managed installs self-update from GitHub Release binaries because package managers own their installed files and may pin, verify, cache, or roll back versions independently of Oh My Herdr. Do not use package-manager metadata as the release source for direct installs because direct installs need one stable source with matching release notes and binary assets; package-manager lag should not block users who installed the standalone binary. Keep GitHub Releases for direct installs and defer managed installs to their manager because it keeps the standalone update path simple while respecting package-manager ownership.

## Consequences

Release tags, `apps/omh/Cargo.toml` versions, and asset names are part of Oh My Herdr's update contract. If a release misses `omh-linux-x86_64`, `omh-linux-aarch64`, `omh-macos-x86_64`, `omh-macos-aarch64`, or `omh-windows-x86_64.exe`, supported direct installs on that platform cannot update or be distributed through the release asset path. If the latest GitHub Release tag does not parse as an Oh My Herdr version, the direct update check fails instead of guessing.

Oh My Herdr does not currently ship a Homebrew formula. Do not add Homebrew-managed update checks or prompts until a real formula or tap exists.
