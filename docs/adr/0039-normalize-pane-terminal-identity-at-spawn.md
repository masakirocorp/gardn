---
status: accepted
---

# Normalize pane terminal identity at spawn

Hako panes advertise the terminal identity Hako's inner terminal layer supports, not the outer terminal that launched Hako. Every shell, profile command, shell command, and argv command spawn gets `TERM=xterm-256color` and `COLORTERM=truecolor` before integration-specific pane environment is applied; explicit profile/command environment can still override those values afterward.

This avoids leaking host-terminal identity into shells and remote sessions that are rendered by Hako rather than by the outer terminal directly. Inheriting values such as a host-specific `TERM` would make child processes and SSH targets assume terminfo capabilities that Hako may not implement or that remote machines may not have installed.

This is separate from ADR 0014's terminal core choice. ADR 0014 records the emulation engine boundary; this ADR records the spawn-time environment contract exposed to processes running inside panes.

## Current rationale

`[INFERENCE]` Hako normalizes pane terminal identity so commands get a stable, widely available terminal contract that matches Hako's rendering layer closely enough for redraw, color, and cursor behavior. Allowing later explicit environment overrides preserves escape hatches for commands that intentionally need a different identity.

## Consequences

New pane-spawn paths should call the same terminal-identity setup before applying user/profile-specific environment. Code should not pass through the launching terminal's `TERM` or `COLORTERM` by default unless Hako deliberately changes the inner terminal contract.
