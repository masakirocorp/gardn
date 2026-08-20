# Add terminal UI controls

You can now filter keybinding help by shortcut, action, or section. Settings > Appearance > Panes exposes pane borders, pane scrollbars, pane gaps, and the single-tab bar. Settings > Appearance > Window edits the outer terminal window-title template. Settings > Behavior exposes workspace naming, copy-on-select, agent-session resume, and every accepted right-click passthrough modifier combination. Settings > Advanced > Server configures the headless terminal size. You can also choose dot or symbol agent status indicators and route right-clicks to individual panes. Disabling automatic copy now keeps mouse selections visible until you copy or clear them. Context bar, sidebar, and status indicator choices now save without corrupting `config.toml`.

Settings > Commands now identifies each launcher by purpose instead of exposing its internal working-directory rules. Each command can be reset to its built-in value, and one action resets all four.

Settings now covers the remaining stable config values that belong in the modal. Behavior > Terminal edits the default shell and shell startup mode. Notifications expands popups with background-alert delay and in-app toast position, and adds clipboard copy confirmation. Advanced > Updates toggles version and manifest checks. Custom agent profiles can be disabled without changing their identity.

Arbitrary theme token overrides and `ui.accent` compatibility were removed. Appearance now uses built-in themes plus the six terminal accent choices.
