# Release process

Gardn is pre-public. Old GitHub releases are internal artifacts; do not preserve their changelog shape.

## Pending changes

Use Tegami changelog files in `.tegami/`.

```md
---
packages:
  gardn: patch
---

### Fix sidebar focus

The sidebar keeps the selected workspace visible after filtering.
```

Rules:

- Add a `.tegami/*.md` file for user-facing app, docs, Nix, or website changes.
- Skip `.tegami/` for pure tests, refactors, or internal chores.
- Every release-worthy changefile must target `gardn`; `just release` rejects pending changefiles that do not.
- Also target `gardn-docs` for docs and `gardn-nix` for Nix packaging so their package changelogs stay surface-specific.
- Keep prose user-facing. No implementation notes.
- Do not edit package `CHANGELOG.md` files by hand.

## Product announcements

Product announcements are separate from changelog entries and should be reserved for a single
important message that users of a specific release need to see. Add at most one entry for the
release version to `apps/gardn/assets/product-announcements.json`:

```json
{
  "announcements": [
    {
      "version": "0.3.0",
      "id": "keymap-v2",
      "title": "Keybindings changed",
      "body": "### What changed\n\nExplain the user-visible change and required action."
    }
  ]
}
```

The matching binary shows the announcement once after onboarding and records
`<version>/<id>` as seen. Entries for other versions are inert, so old entries may remain for
release history. Use a stable, descriptive ID and do not reuse it for different content.

## Local workflow

```bash
pnpm install
pnpm tegami
cargo test --manifest-path apps/gardn/Cargo.toml --locked --bin gardn <focused-test>
git add .
git commit -m "short imperative summary"
```

Direct commits to `master` are OK while Gardn is solo/pre-public. Use PRs for upstream syncs, risky release changes, or when CI/review is useful. Never force-push `origin/master`.

## Manual QA

Use the [manual QA matrix](manual-qa.md) to select checks during development. Before tagging a release, run M01-M08 and the P1 cases affected by the release. After the release artifacts publish, run M09 before treating the release as cleared.

## Release

```bash
just release
```

`just release`:

1. requires a clean tree
2. verifies every pending Tegami changefile includes `gardn`
3. runs `CI=true pnpm tegami version`
4. runs `pnpm check`
5. commits Tegami's version/changelog changes
6. tags `v<version>`
7. pushes the branch and tag

The GitHub Release workflow builds binary assets from the pushed tag and uses the generated `apps/gardn/CHANGELOG.md` section as the release body.
