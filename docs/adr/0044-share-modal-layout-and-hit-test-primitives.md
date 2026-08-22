---
status: accepted
---

# Share modal layout and hit-test primitives

Gardn modal overlays share geometry primitives for shell stacking, scrollable list viewports, and responsive tab strips. `modal_stack_areas` partitions headers, content, footers, and action rows; `ModalListViewport` owns visible row ranges, scroll metrics, selection visibility, and hit-testing; `modal_tabs` owns width-driven visible tab ranges, hit areas, and chevron targets.

This keeps rendering and input aligned across settings, command palette, agent profile picker, onboarding, release notes, and keybind help. Letting each overlay define its own geometry would make mouse hit targets, scrollbars, clipped tab rows, and close/action areas drift apart even when the UI appears visually similar.

This is separate from ADR 0032's settings row model. ADR 0032 records how settings content rows are normalized; this ADR records the shared modal geometry used by multiple overlay surfaces and their input handlers.

## Current rationale

`[INFERENCE]` Gardn shares modal primitives because it is a mouse-first TUI: the same rectangle math must drive painting, hover, click, scroll, and selection behavior. Centralizing that geometry makes new overlays feel native and prevents one-off modal code from reintroducing inconsistent affordances.

## Consequences

New modal overlays should use the shared stack, viewport, and tab-strip helpers when they match these interaction patterns. If a modal needs different geometry, the new primitive should be named and shared intentionally rather than copying layout and hit-test math into a single surface.
