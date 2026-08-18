mod encode;
mod model;
mod parse;
mod windows;

#[allow(unused_imports)]
pub use encode::{
    encode_cursor_key, encode_mouse_button, encode_mouse_scroll, encode_terminal_key,
};
pub use model::{
    host_modify_other_keys_mode, ime_compatible_keyboard_enhancement_flags, KeyIdentity,
    KeyboardProtocol, MouseProtocolEncoding, MouseProtocolMode, TerminalKey, TextCommit,
    WindowsKeyRecord,
};
pub use parse::parse_terminal_key_sequence;
pub(crate) use windows::parse_windows_conpty_key_sequence;
#[cfg(windows)]
pub(crate) use windows::{encode_windows_conpty_fallback, encode_windows_conpty_shift_enter};
