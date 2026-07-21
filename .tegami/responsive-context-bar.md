---
packages:
  omh: patch
  omh-docs: patch
---

### Add a responsive workspace context bar

Desktop clients now show an independent, clickable, client-local group / workspace / tab path with focused pane context and live topology counts. Every path segment opens one tall, stable-height, grouped workspace navigator with its matching row visibly selected and configured group accents carried into the hierarchy. The navigator is also available from the command palette, shares the app's standard modal header, close action, dividers, and footer hints, lets `Space` toggle the visibly highlighted branch, and provides `E`/`C` controls to expand or collapse the full hierarchy; the bar supports per-client toggling and remains usable on narrow terminals.
