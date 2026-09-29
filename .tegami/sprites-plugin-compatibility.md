---
packages:
  gardn: patch
  gardn-docs: patch
---

### Support the Sprites plugin

Gardn accepts upstream plugin API requirements through 0.9.0. The new `gardn plugin config-dir <plugin_id>` command creates and prints the plugin configuration directory. On macOS and Linux, wrapped agents can use the upstream agent hint when `GARDN_AGENT` is absent. Plugin actions use the selected pane's context. Compatibility keeps Gardn's executable, sockets, configuration, and state separate from the upstream application.

The `gardn plugin disable <plugin_id>` command remains available and persists the disabled state. Plugin manifests and commands reject the reserved IDs `.` and `..` instead of treating shared directories as plugin storage.
