---
packages:
  omh: patch
  omh-docs: patch
---

### Make the Git diff shortcut configurable

The Diff shortcut is now opt-in from Settings > Commands. Choose LazyGit (`lazygit`), Hunk watch mode (`hunk diff --watch`), or Plannotator (`plannotator review`), or enter any terminal command directly. The curated Hunk command inherits the target workspace's Oh My Herdr palette and group accent. A configured command shows Diff in the new-tab menu, command palette, and contextual Git actions; leaving it empty keeps those actions hidden.
