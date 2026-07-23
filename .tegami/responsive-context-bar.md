---
packages:
  omh: patch
  omh-docs: patch
---

### Add a responsive workspace context bar

Desktop clients now show an independent, clickable, client-local group / workspace / tab path with focused pane context. Optional topology and section counters can be enabled globally with `ui.show_counters`; they are hidden by default across desktop and mobile views. Every path segment opens one tall, stable-height, grouped workspace navigator with its matching row visibly selected, configured group accents carried into the hierarchy, and conditional tab and pane rows already visible for the active group. The navigator is also available from the command palette, shares the app's standard modal header, close action, dividers, and footer hints, lets `Space` toggle the visibly highlighted branch, and provides `E`/`C` controls to expand or collapse the full hierarchy; the bar supports per-client toggling and remains usable on narrow terminals.
