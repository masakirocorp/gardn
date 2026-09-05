---
packages:
  gardn: patch
---

# Make ghui the curated GitHub tool

Use the Masakiro ghui fork as the default GitHub project command for new configurations and command resets. Gardn pins the companion release and launches it with the active terminal theme, visible scrollbars, mouse controls, and optional Group-level GitHub organization scope. Running ghui panes now follow Gardn theme changes without a restart. Settings includes the retained MIT acknowledgment.

Each Space can use discovered GitHub repositories, an explicit repository list, or its Group organization. Gardn resolves scope when it launches ghui and keeps it fixed for that process. The companion adds scoped Overview and Actions views, isolated Worktrunk review Spaces, and explicit agent context handoffs.

The GitHub header keeps the active organization or repositories visible beside the Space name. Space Settings shows the configured Group organization and separates scope choices from the repository input.
