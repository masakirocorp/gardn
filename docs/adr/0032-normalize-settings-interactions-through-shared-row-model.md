---
status: accepted
---

# Normalize settings interactions through a shared row model

Gardn models settings lists as typed `SettingsListRow` values so list-shaped sections share visual-row height, selection mapping, scroll extents, and hit-test mapping. Section-specific actions still live in input handling, and sections that need specialized presentation may keep it. A row can be a header, caption, spacer, toggle option, text input, choice, or status choice. `rows_for_section` builds that row model for global sections such as theme, layout, sound, toasts, behavior, experiments, agents, and integrations; group settings reuse the same model for group appearance/accent, group general, and group profiles.

The row model owns the translation between logical option indexes and visual rows. Headers, captions, and spacers are non-selectable one-row entries. Choices and status choices are selectable one-row entries. Toggle options occupy two selectable visual rows, while text inputs occupy two visual rows but map only the value row to the logical option. `visual_row_count` drives scroll extents, `selected_visual_row` keeps keyboard selection visible, and `option_index_for_visual_row` maps a viewport hit visual row back to the logical setting index. This keeps keyboard navigation, scrolling, and mouse selection aligned even when sections mix explanatory text, multi-line options, text fields, and status rows.

Rendering consumes the same row model for list-shaped sections. `render_settings_rows` converts rows into ratatui list items, applies selected styles, markers, descriptions, text-input cursor display, and status marker tones, and shares the same viewport calculation used by input handling. Most settings sections use `render_settings_sectioned_toggle_list`; integrations have specialized list and hint rendering but still reuse `rows_for_section` and `StatusChoice` rows for list content and input mapping.

This is separate from ADR 0017 — Treat config.toml as a public contract: the row model is an in-app interaction model for pending settings UI state, not the persistent config format. It is also separate from ADR 0005 — Split app orchestration by responsibility: that ADR records why app behavior is divided across state/actions/input/runtime helpers, while this ADR records the settings-specific list algebra that keeps rendering and input semantics in one shared shape.

## Current rationale

`[INFERENCE]` Gardn uses a shared row model because settings sections need richer structure than a flat toggle list, but duplicating visual-row math and hit-testing per section would make mouse-first settings brittle. Centralizing row semantics lets new sections add rows without re-deriving scroll, selected-row, and click mapping rules.

## Consequences

New settings sections that fit the modal list pattern should produce `SettingsListRow` values and reuse the shared visual-row helpers. They should not hand-roll independent selection or mouse-hit-test math unless the section is genuinely not list-shaped.

New row variants must define their visual height, selectable index behavior, render behavior, and mouse mapping together. Otherwise keyboard, mouse, and scroll behavior can silently disagree.
