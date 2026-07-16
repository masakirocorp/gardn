---
status: accepted
---

# Preserve compatibility while translating terminal keys

Oh My Herdr treats terminal keys as semantic key events internally, not as one escape-sequence dialect. `TerminalKey` carries a `KeyCode`, modifiers, press/repeat/release kind, and an optional shifted codepoint. Parsers normalize Kitty CSI u, modifyOtherKeys, and legacy terminal sequences into that shape, in that order, so downstream app logic can reason about keys without keeping the original wire encoding.

The parser preserves information that normal key events lose. Kitty CSI u event suffixes become `KeyEventKind::Press`, `Repeat`, or `Release`; Ghostty/xterm-style modified special-key suffixes can preserve event kind too. Alternate shifted codepoints are stored separately from the base key. Legacy input still maps common control bytes, Alt-prefixed character input, and xterm modified special-key sequences into the same `TerminalKey` model. A bare line feed intentionally falls through to legacy control-byte parsing instead of being treated as Enter, because raw-mode Enter is carriage return and LF is commonly used for Ctrl+J or Shift+Enter workarounds.

Encoding is deliberately conservative. `encode_terminal_key` sends plain text input directly when possible, then tries Kitty CSI u only when a Kitty protocol is active, and finally falls back to legacy/xterm encoding. Unmodified keys keep legacy encoding. Modified arrows, navigation keys, insert/delete/page keys, and function keys also keep xterm legacy formats even in Kitty mode; `[INFERENCE]` those sequences are more widely compatible and match what Ghostty sends with Kitty mode enabled. CSI u is reserved for character keys and other keys that need the richer representation.

The terminal-core bridge keeps both sides of a shifted character when Oh My Herdr builds a Ghostty key event. `ghostty_key_event_from_terminal_key` sets Ghostty action from the semantic key kind, text from the shifted codepoint when available, and the unshifted codepoint from the base character. Character-key delivery can also prefer Oh My Herdr text encoding before Ghostty key-event encoding, so shifted text may be delivered as text rather than as a round-trip key identity; shifted-text release events can intentionally emit no bytes.

This is separate from ADR 0010's client input framing. ADR 0010 says thin clients frame input bytes and the server owns semantic input. This ADR records the semantic key model and outgoing terminal-compatibility policy once bytes have already been parsed into terminal key events.

## Current rationale

`[INFERENCE]` Oh My Herdr normalizes incoming key dialects because UI commands, keybindings, normal pane input, and terminal-core events need one internal key model. Direct terminal attach remains ADR 0010's raw-byte forwarding exception. Carrying the original escape dialect through app logic would make every semantic-input feature care which host terminal encoding produced the key.

`[INFERENCE]` Oh My Herdr keeps legacy/xterm output for special keys because terminal applications have decades of compatibility with those sequences, while CSI u support is uneven. Using CSI u only where it buys information avoids breaking applications that already understand modified arrows, function keys, and navigation keys.

`[INFERENCE]` Preserving shifted and unshifted character information is needed for terminal cores and IME-friendly enhanced keyboard reporting: the app needs to know both what the user typed and what physical/base key the terminal should receive.

## Consequences

New input parsers should normalize into `TerminalKey` and preserve event kind and alternate codepoint data when the source protocol provides them. They should not leak protocol-specific escape-sequence details into app keybinding logic.

New output protocols should be conservative about replacing legacy sequences. If a key already has a broadly compatible xterm/legacy encoding, Oh My Herdr should keep that encoding unless there is a concrete reason to require a richer protocol.

Changes to key encoding should check both directions: app semantic handling and pane/terminal-core delivery. A key can be correctly parsed but still wrong if its shifted text, unshifted codepoint, or repeat/release action is lost on delivery.
