---
packages:
  omh: patch
  omh-docs: patch
---

### Make the Git diff shortcut configurable

The Diff shortcut is now opt-in from Settings > Commands. Choose LazyGit (`lazygit`) or Hunk watch mode (`hunk diff --watch`), or enter any terminal command directly. Curated LazyGit follows the terminal palette for Terminal and System themes and receives a generated palette overlay for named Oh My Herdr themes. Curated Hunk inherits the target workspace's active palette and group accent. A configured command shows Diff in the new-tab menu, command palette, and contextual Git actions; leaving it empty keeps those actions hidden.
