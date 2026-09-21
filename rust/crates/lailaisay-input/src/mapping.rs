//! ANSI/Carbon virtual-key mapping used by the macOS CGEvent tap.
//!
//! Layout-aware remapping (Sauce) is a follow-up; this table matches a US
//! keyboard and is enough for letters, Space, Escape, and digits — the keys
//! lailaisay actually uses as hotkeys. The conversion is pure and tested on Linux.

use lailaisay_core::{Key, KeyEvent, Modifier, Modifiers};

/// CGEventType values we care about (CoreGraphics).
pub const CG_EVENT_KEY_DOWN: u32 = 10;
pub const CG_EVENT_KEY_UP: u32 = 11;
pub const CG_EVENT_FLAGS_CHANGED: u32 = 12;
pub const CG_EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
pub const CG_EVENT_TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;

/// Carbon / Quartz modifier flag bits (`CGEventFlags`).
pub const FLAG_SHIFT: u64 = 0x0002_0000;
pub const FLAG_CONTROL: u64 = 0x0004_0000;
pub const FLAG_ALTERNATE: u64 = 0x0008_0000;
pub const FLAG_COMMAND: u64 = 0x0010_0000;
pub const FLAG_SECONDARY_FN: u64 = 0x0080_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CgEventKind {
    KeyDown,
    KeyUp,
    FlagsChanged,
    Other(u32),
}

impl CgEventKind {
    pub fn from_type(raw: u32) -> Self {
        match raw {
            CG_EVENT_KEY_DOWN => Self::KeyDown,
            CG_EVENT_KEY_UP => Self::KeyUp,
            CG_EVENT_FLAGS_CHANGED => Self::FlagsChanged,
            other => Self::Other(other),
        }
    }

    pub fn is_tap_disabled(raw: u32) -> bool {
        raw == CG_EVENT_TAP_DISABLED_BY_TIMEOUT || raw == CG_EVENT_TAP_DISABLED_BY_USER_INPUT
    }
}

/// Map a keyboard event using these rules:
/// key is present only on `keyDown`; `keyUp` / `flagsChanged` carry modifiers only.
pub fn key_event_from_cg(kind: CgEventKind, keycode: u16, flags: u64) -> KeyEvent {
    let modifiers = flags_to_modifiers(flags);
    let key = match kind {
        CgEventKind::KeyDown => keycode_to_key(keycode),
        CgEventKind::KeyUp | CgEventKind::FlagsChanged | CgEventKind::Other(_) => None,
    };
    KeyEvent { key, modifiers }
}

pub fn flags_to_modifiers(flags: u64) -> Modifiers {
    let mut mods = Vec::new();
    if flags & FLAG_SHIFT != 0 {
        mods.push(Modifier::Shift);
    }
    if flags & FLAG_CONTROL != 0 {
        mods.push(Modifier::Control);
    }
    if flags & FLAG_ALTERNATE != 0 {
        mods.push(Modifier::Option);
    }
    if flags & FLAG_COMMAND != 0 {
        mods.push(Modifier::Command);
    }
    if flags & FLAG_SECONDARY_FN != 0 {
        mods.push(Modifier::Fn);
    }
    Modifiers::new(mods)
}

/// ANSI Carbon virtual key codes → `lailaisay_core::Key`.
/// Modifier-only codes (55–59) map to `None` so `flagsChanged` stays key-less.
pub fn keycode_to_key(code: u16) -> Option<Key> {
    Some(match code {
        0 => Key::A,
        1 => Key::S,
        2 => Key::D,
        3 => Key::F,
        4 => Key::H,
        5 => Key::G,
        6 => Key::Z,
        7 => Key::X,
        8 => Key::C,
        9 => Key::V,
        11 => Key::B,
        12 => Key::Q,
        13 => Key::W,
        14 => Key::E,
        15 => Key::R,
        16 => Key::Y,
        17 => Key::T,
        18 => Key::One,
        19 => Key::Two,
        20 => Key::Three,
        21 => Key::Four,
        22 => Key::Six,
        23 => Key::Five,
        25 => Key::Nine,
        26 => Key::Seven,
        28 => Key::Eight,
        29 => Key::Zero,
        31 => Key::O,
        32 => Key::U,
        34 => Key::I,
        35 => Key::P,
        36 => Key::Return,
        37 => Key::L,
        38 => Key::J,
        40 => Key::K,
        45 => Key::N,
        46 => Key::M,
        48 => Key::Tab,
        49 => Key::Space,
        53 => Key::Escape,
        _ => return None,
    })
}

/// Inverse of [`keycode_to_key`]: the Carbon / Quartz virtual keycode for a
/// [`Key`]. Used by the sandbox-safe `RegisterEventHotKey` monitor.
pub fn key_to_keycode(key: &Key) -> Option<u16> {
    (0u16..=53).find(|code| keycode_to_key(*code).as_ref() == Some(key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_and_escape() {
        assert_eq!(keycode_to_key(49), Some(Key::Space));
        assert_eq!(keycode_to_key(53), Some(Key::Escape));
        assert_eq!(keycode_to_key(0), Some(Key::A));
        assert_eq!(keycode_to_key(55), None); // command key
    }

    #[test]
    fn key_to_keycode_round_trips() {
        assert_eq!(key_to_keycode(&Key::Space), Some(49));
        assert_eq!(key_to_keycode(&Key::Escape), Some(53));
        assert_eq!(key_to_keycode(&Key::A), Some(0));
        for code in 0u16..=53 {
            if let Some(key) = keycode_to_key(code) {
                assert_eq!(key_to_keycode(&key), Some(code), "{key:?}");
            }
        }
    }

    #[test]
    fn command_shift_flags() {
        let mods = flags_to_modifiers(FLAG_COMMAND | FLAG_SHIFT);
        assert!(mods.contains(Modifier::Command));
        assert!(mods.contains(Modifier::Shift));
        assert!(!mods.contains(Modifier::Option));
    }

    #[test]
    fn key_down_has_key_key_up_does_not() {
        let down = key_event_from_cg(CgEventKind::KeyDown, 0, FLAG_COMMAND);
        assert_eq!(down.key, Some(Key::A));
        assert!(down.modifiers.contains(Modifier::Command));

        let up = key_event_from_cg(CgEventKind::KeyUp, 0, FLAG_COMMAND);
        assert_eq!(up.key, None);
        assert!(up.modifiers.contains(Modifier::Command));
    }

    #[test]
    fn option_flags_changed_is_modifier_only_chord() {
        let e = key_event_from_cg(CgEventKind::FlagsChanged, 58, FLAG_ALTERNATE);
        assert_eq!(e.key, None);
        assert!(e.modifiers.contains(Modifier::Option));
    }
}
