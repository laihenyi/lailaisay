//! Portable Win32 key sequences for clipboard paste.
//!
//! Pure data so Linux CI can test the chords. Live [`SendInput`] lives in
//! [`crate::windows`].

/// Virtual-key constants used by the paste path.
pub const VK_SHIFT: u16 = 0x10;
pub const VK_CONTROL: u16 = 0x11;
pub const VK_V: u16 = 0x56;
pub const VK_INSERT: u16 = 0x2D;

/// One synthesized key transition (down or up).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntheticKey {
    pub vk: u16,
    pub up: bool,
    /// `KEYEVENTF_EXTENDEDKEY` — needed for Insert (and other grey keys).
    pub extended: bool,
}

impl SyntheticKey {
    pub const fn down(vk: u16) -> Self {
        Self {
            vk,
            up: false,
            extended: false,
        }
    }

    pub const fn up(vk: u16) -> Self {
        Self {
            vk,
            up: true,
            extended: false,
        }
    }

    pub const fn down_ext(vk: u16) -> Self {
        Self {
            vk,
            up: false,
            extended: true,
        }
    }

    pub const fn up_ext(vk: u16) -> Self {
        Self {
            vk,
            up: true,
            extended: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsPasteChord {
    CtrlV,
    ShiftInsert,
}

/// Ctrl+V, then Shift+Insert if the first injection fails.
pub fn windows_paste_chords() -> [WindowsPasteChord; 2] {
    [WindowsPasteChord::CtrlV, WindowsPasteChord::ShiftInsert]
}

pub fn chord_keys(chord: WindowsPasteChord) -> [SyntheticKey; 4] {
    match chord {
        WindowsPasteChord::CtrlV => ctrl_v_sequence(),
        WindowsPasteChord::ShiftInsert => shift_insert_sequence(),
    }
}

pub fn ctrl_v_sequence() -> [SyntheticKey; 4] {
    [
        SyntheticKey::down(VK_CONTROL),
        SyntheticKey::down(VK_V),
        SyntheticKey::up(VK_V),
        SyntheticKey::up(VK_CONTROL),
    ]
}

pub fn shift_insert_sequence() -> [SyntheticKey; 4] {
    [
        SyntheticKey::down(VK_SHIFT),
        SyntheticKey::down_ext(VK_INSERT),
        SyntheticKey::up_ext(VK_INSERT),
        SyntheticKey::up(VK_SHIFT),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_v_is_control_then_v() {
        let keys = ctrl_v_sequence();
        assert_eq!(keys[0], SyntheticKey::down(VK_CONTROL));
        assert_eq!(keys[1], SyntheticKey::down(VK_V));
        assert_eq!(keys[2], SyntheticKey::up(VK_V));
        assert_eq!(keys[3], SyntheticKey::up(VK_CONTROL));
        assert!(keys.iter().all(|k| !k.extended));
    }

    #[test]
    fn shift_insert_marks_insert_extended() {
        let keys = shift_insert_sequence();
        assert_eq!(keys[0].vk, VK_SHIFT);
        assert!(!keys[0].extended);
        assert_eq!(keys[1].vk, VK_INSERT);
        assert!(keys[1].extended && !keys[1].up);
        assert!(keys[2].extended && keys[2].up);
        assert_eq!(
            windows_paste_chords(),
            [WindowsPasteChord::CtrlV, WindowsPasteChord::ShiftInsert]
        );
        assert_eq!(chord_keys(WindowsPasteChord::CtrlV), ctrl_v_sequence());
    }
}
