---
status: accepted
---

# Stage clipboard-image paste through thin clients

Gardn treats clipboard image paste as a client-local paste bridge, not as normal terminal text input and not as server-owned clipboard state. The thin client loop inspects local input before forwarding `ClientMessage::Input`: a Ctrl+V key event or an empty bracketed paste can trigger `read_clipboard_image` on that client host. If the local clipboard has no image, Gardn forwards the original input normally. If the clipboard image is larger than `MAX_CLIPBOARD_IMAGE_PAYLOAD` (16 MiB), Gardn logs and drops the paste trigger. If the image fits, the client sends `ClientMessage::ClipboardImage { extension, data }` instead of forwarding the original paste trigger.

The server does not stream raw image bytes into the terminal. `server::clipboard_image::stage` writes the bytes to a temp file, sanitizes the extension to a known image extension, creates the file with exclusive creation and `0600` permissions on Unix, and returns the staged path as paste text. The staging directory is per user on Unix and per process elsewhere; staging ensures Unix directory permissions are `0700` and opportunistically removes files older than 24 hours when staging a new image.

Staged files are tracked per client connection. When a client disconnects, when the server completes shutdown, and when the server is dropped, Gardn removes that client's staged files. The transport layer independently rejects oversized clipboard-image messages from clients, so both client and server boundaries enforce the 16 MiB payload ceiling.

The final terminal input is the staged file path. In direct terminal attach mode, Gardn sends the path directly to the attached terminal runtime, wrapped as bracketed paste when the runtime has bracketed paste enabled. In normal app-client mode, Gardn promotes the client to foreground if needed, requests semantic redraw after input, and routes a paste event containing the path through normal app input semantics.

This is separate from ADR 0010's byte-framed input boundary and ADR 0021's notification policy and client-local delivery effects. ADR 0010 says clients forward input bytes and the server decodes semantic input. Clipboard image paste is a deliberate exception at the client edge: the client must read host clipboard image data because the server may not share the user's desktop clipboard. ADR 0021 records notification delivery; this ADR records clipboard image data transfer and temp-file staging.

## Current rationale

`[INFERENCE]` Gardn stages clipboard images as files because many terminal programs and coding agents can consume local file paths, while raw image bytes pasted into a terminal would corrupt text input and require every shell/app to understand an Gardn-specific binary protocol.

`[INFERENCE]` The thin-client boundary keeps clipboard authority on the machine where the user pressed paste. That lets remote/headless servers receive useful paste behavior without gaining ambient access to the user's clipboard outside explicit paste gestures.

`[INFERENCE]` The size limit, extension sanitization, private staging directory, stale cleanup, and per-client cleanup keep the bridge bounded. The staged file is intentionally temporary transfer state, not durable project input or session snapshot data.

## Consequences

New clipboard-image formats should pass through the same bounded staging model or explicitly replace it with a later ADR. They should not bypass extension sanitization, payload limits, or staged-file cleanup.

Client input paths should keep the paste-trigger check at the thin-client edge. Server-side code should treat clipboard image bytes as an explicit client message, never as implicit access to host clipboard state.

Terminal delivery should remain path-based unless Gardn adopts a separate terminal graphics or file-transfer contract. Direct attach may send the path directly; app-client mode should continue routing the path through semantic paste so normal app focus and redraw behavior remains intact.
