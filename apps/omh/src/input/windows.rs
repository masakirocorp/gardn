use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};

use super::{TerminalKey, WindowsKeyRecord};

#[cfg(any(windows, test))]
pub(crate) fn encode_windows_conpty_fallback(key: &TerminalKey) -> Option<Vec<u8>> {
    let (virtual_key_code, virtual_scan_code, unicode, control_key_state) =
        if let Some(record) = key.windows_record() {
            (
                record.virtual_key_code,
                record.virtual_scan_code,
                record.unicode,
                record.control_key_state,
            )
        } else if key.code == KeyCode::Esc
            && key.modifiers.is_empty()
            && key.kind == KeyEventKind::Press
            && key.vt_bytes().is_none()
        {
            return Some(b"\x1b[27;1;27;1;0;1_\x1b[27;1;27;0;0;1_".to_vec());
        } else if key.code == KeyCode::Enter && key.modifiers == KeyModifiers::SHIFT {
            (13, 28, 13, 16)
        } else {
            return None;
        };
    let key_down = key.kind != KeyEventKind::Release;
    let repeat_count = if key_down { key.repeat_count.max(1) } else { 1 };

    Some(
        format!(
            "\x1b[{virtual_key_code};{virtual_scan_code};{unicode};{};{control_key_state};{repeat_count}_",
            u8::from(key_down),
        )
        .into_bytes(),
    )
}

#[cfg(any(windows, test))]
pub(crate) fn encode_windows_conpty_shift_enter(key: &TerminalKey) -> Option<Vec<u8>> {
    if key.code != KeyCode::Enter || key.modifiers != KeyModifiers::SHIFT {
        return None;
    }

    let key_down = !matches!(key.kind, KeyEventKind::Release);
    Some(format!("\x1b[13;28;13;{};16;1_", u8::from(key_down)).into_bytes())
}

pub(crate) fn parse_windows_conpty_key_sequence(data: &str) -> Option<TerminalKey> {
    let body = data.strip_prefix("\x1b[")?.strip_suffix('_')?;
    let mut fields = body.split(';');
    let virtual_key_code = fields.next()?.parse::<u16>().ok()?;
    let virtual_scan_code = fields.next()?.parse::<u16>().ok()?;
    let unicode = fields.next()?.parse::<u16>().ok()?;
    let key_down = fields.next()?.parse::<u16>().ok()? != 0;
    let control_key_state = fields.next()?.parse::<u32>().ok()?;
    let repeat_count = fields.next()?.parse::<u16>().ok()?;
    if fields.next().is_some() {
        return None;
    }

    let record = WindowsKeyRecord {
        key_down,
        repeat_count,
        virtual_key_code,
        virtual_scan_code,
        unicode,
        control_key_state,
    };
    let modifiers = windows_control_key_modifiers(control_key_state);
    let code = windows_record_key_code(virtual_key_code, unicode, modifiers)?;
    let kind = if key_down {
        KeyEventKind::Press
    } else {
        KeyEventKind::Release
    };
    Some(
        TerminalKey::new(code, modifiers)
            .with_kind(kind)
            .with_windows_record(record),
    )
}

fn windows_control_key_modifiers(control_key_state: u32) -> KeyModifiers {
    const RIGHT_ALT_PRESSED: u32 = 0x0001;
    const LEFT_ALT_PRESSED: u32 = 0x0002;
    const RIGHT_CTRL_PRESSED: u32 = 0x0004;
    const LEFT_CTRL_PRESSED: u32 = 0x0008;
    const SHIFT_PRESSED: u32 = 0x0010;

    let mut modifiers = KeyModifiers::empty();
    let alt_gr = control_key_state & RIGHT_ALT_PRESSED != 0
        && control_key_state & LEFT_CTRL_PRESSED != 0
        && control_key_state & RIGHT_CTRL_PRESSED == 0;
    if control_key_state & SHIFT_PRESSED != 0 {
        modifiers |= KeyModifiers::SHIFT;
    }
    if !alt_gr && control_key_state & (LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED) != 0 {
        modifiers |= KeyModifiers::CONTROL;
    }
    if control_key_state & LEFT_ALT_PRESSED != 0
        || (control_key_state & RIGHT_ALT_PRESSED != 0 && !alt_gr)
    {
        modifiers |= KeyModifiers::ALT;
    }
    modifiers
}

fn windows_record_key_code(
    virtual_key_code: u16,
    unicode: u16,
    modifiers: KeyModifiers,
) -> Option<KeyCode> {
    Some(match virtual_key_code {
        0x08 => KeyCode::Backspace,
        0x09 if modifiers.contains(KeyModifiers::SHIFT) => KeyCode::BackTab,
        0x09 => KeyCode::Tab,
        0x0d => KeyCode::Enter,
        0x1b => KeyCode::Esc,
        0x21 => KeyCode::PageUp,
        0x22 => KeyCode::PageDown,
        0x23 => KeyCode::End,
        0x24 => KeyCode::Home,
        0x25 => KeyCode::Left,
        0x26 => KeyCode::Up,
        0x27 => KeyCode::Right,
        0x28 => KeyCode::Down,
        0x2d => KeyCode::Insert,
        0x2e => KeyCode::Delete,
        0x70..=0x87 => KeyCode::F((virtual_key_code - 0x6f) as u8),
        _ => {
            let ch = char::from_u32(u32::from(unicode)).filter(|ch| !ch.is_control())?;
            KeyCode::Char(ch)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_conpty_native_encoder_preserves_semantic_escape_fallback() {
        let escape = TerminalKey::new(KeyCode::Esc, KeyModifiers::empty());

        assert_eq!(
            encode_windows_conpty_fallback(&escape),
            Some(b"\x1b[27;1;27;1;0;1_\x1b[27;1;27;0;0;1_".to_vec())
        );
        assert_eq!(
            encode_windows_conpty_fallback(&escape.clone().with_kind(KeyEventKind::Repeat)),
            None
        );
        assert_eq!(
            encode_windows_conpty_fallback(&escape.clone().with_kind(KeyEventKind::Release)),
            None
        );
        assert_eq!(
            encode_windows_conpty_fallback(&escape.clone().with_vt_bytes(vec![27])),
            None
        );
        assert_eq!(
            encode_windows_conpty_fallback(&TerminalKey::new(KeyCode::Esc, KeyModifiers::ALT)),
            None
        );
    }

    #[test]
    fn windows_conpty_native_encoder_preserves_semantic_shift_enter_fallback() {
        let shift_enter = TerminalKey::new(KeyCode::Enter, KeyModifiers::SHIFT);

        assert_eq!(
            encode_windows_conpty_fallback(&shift_enter),
            Some(b"\x1b[13;28;13;1;16;1_".to_vec())
        );
        assert_eq!(
            encode_windows_conpty_shift_enter(&shift_enter),
            Some(b"\x1b[13;28;13;1;16;1_".to_vec())
        );
        assert_eq!(
            encode_windows_conpty_shift_enter(&shift_enter.with_kind(KeyEventKind::Release)),
            Some(b"\x1b[13;28;13;0;16;1_".to_vec())
        );
    }
}
