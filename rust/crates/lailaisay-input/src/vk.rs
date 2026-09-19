//! Win32 virtual-key mapping used by the Windows `WH_KEYBOARD_LL` hook.
//!
//! Pure functions so Linux CI can test the table. Live hook install lives in
//! [`crate::windows`].

use lailaisay_core::{Key, KeyEvent, Modifier, Modifiers};

/// `WM_KEYDOWN`
pub const WM_KEYDOWN: u32 = 0x0100;
/// `WM_KEYUP`
pub const WM_KEYUP: u32 = 0x0101;
/// `WM_SYSKEYDOWN` (Alt combinations)
pub const WM_SYSKEYDOWN: u32 = 0x0104;
/// `WM_SYSKEYUP`
pub const WM_SYSKEYUP: u32 = 0x0105;

pub const VK_TAB: u32 = 0x09;
pub const VK_RETURN: u32 = 0x0D;
pub const VK_SHIFT: u32 = 0x10;
pub const VK_CONTROL: u32 = 0x11;
pub const VK_MENU: u32 = 0x12;
pub const VK_ESCAPE: u32 = 0x1B;
pub const VK_SPACE: u32 = 0x20;
pub const VK_0: u32 = 0x30;
pub const VK_1: u32 = 0x31;
pub const VK_2: u32 = 0x32;
pub const VK_3: u32 = 0x33;
pub const VK_4: u32 = 0x34;
pub const VK_5: u32 = 0x35;
pub const VK_6: u32 = 0x36;
pub const VK_7: u32 = 0x37;
pub const VK_8: u32 = 0x38;
pub const VK_9: u32 = 0x39;
pub const VK_A: u32 = 0x41;
pub const VK_V: u32 = 0x56;
pub const VK_Z: u32 = 0x5A;
pub const VK_LWIN: u32 = 0x5B;
pub const VK_RWIN: u32 = 0x5C;
pub const VK_NUMPAD0: u32 = 0x60;
pub const VK_NUMPAD9: u32 = 0x69;
pub const VK_LSHIFT: u32 = 0xA0;
pub const VK_RSHIFT: u32 = 0xA1;
pub const VK_LCONTROL: u32 = 0xA2;
pub const VK_RCONTROL: u32 = 0xA3;
pub const VK_LMENU: u32 = 0xA4;
pub const VK_RMENU: u32 = 0xA5;

/// `LLKHF_UP` — the key is being released.
pub const LLKHF_UP: u32 = 0x80;
/// `LLKHF_INJECTED` — synthesized (`SendInput`); skip so paste does not re-enter the hook.
pub const LLKHF_INJECTED: u32 = 0x10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinEventKind {
    KeyDown,
    KeyUp,
}

impl WinEventKind {
    pub fn from_wparam(w_param: usize) -> Self {
        match w_param as u32 {
            WM_KEYUP | WM_SYSKEYUP => Self::KeyUp,
            _ => Self::KeyDown,
        }
    }

    pub fn from_ll_flags(flags: u32) -> Self {
        if flags & LLKHF_UP != 0 {
            Self::KeyUp
        } else {
            Self::KeyDown
        }
    }

    pub fn is_up(self) -> bool {
        matches!(self, Self::KeyUp)
    }
}

/// Snapshot of the four lailaisay modifiers as Windows key-down flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModifierDown {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
}

impl ModifierDown {
    pub fn to_modifiers(self) -> Modifiers {
        let mut mods = Vec::new();
        if self.control {
            mods.push(Modifier::Control);
        }
        if self.alt {
            mods.push(Modifier::Option);
        }
        if self.shift {
            mods.push(Modifier::Shift);
        }
        if self.win {
            mods.push(Modifier::Command);
        }
        Modifiers::new(mods)
    }
}

pub fn is_modifier_vk(vk: u32) -> bool {
    matches!(
        vk,
        VK_SHIFT
            | VK_CONTROL
            | VK_MENU
            | VK_LWIN
            | VK_RWIN
            | VK_LSHIFT
            | VK_RSHIFT
            | VK_LCONTROL
            | VK_RCONTROL
            | VK_LMENU
            | VK_RMENU
    )
}

/// Apply the current event to a modifier snapshot.
///
/// `GetAsyncKeyState` can lag the `WH_KEYBOARD_LL` callback by one event;
/// force the vk that generated this message to the post-event state.
pub fn apply_vk_to_modifiers(mut snap: ModifierDown, vk: u32, is_up: bool) -> ModifierDown {
    let down = !is_up;
    match vk {
        VK_SHIFT | VK_LSHIFT | VK_RSHIFT => snap.shift = down,
        VK_CONTROL | VK_LCONTROL | VK_RCONTROL => snap.control = down,
        VK_MENU | VK_LMENU | VK_RMENU => snap.alt = down,
        VK_LWIN | VK_RWIN => snap.win = down,
        _ => {}
    }
    snap
}

/// Same rule as macOS [`crate::key_event_from_cg`]: the key is present only
/// on key-down of a non-modifier. Key-up / modifier changes are key-less so
/// [`lailaisay_core::HotKeyProcessor`] treats them as chord release / flagsChanged.
pub fn key_event_from_win(kind: WinEventKind, vk: u32, modifiers: Modifiers) -> KeyEvent {
    let key = match kind {
        WinEventKind::KeyDown if !is_modifier_vk(vk) => vk_to_key(vk),
        WinEventKind::KeyDown | WinEventKind::KeyUp => None,
    };
    KeyEvent { key, modifiers }
}

/// Map a Win32 virtual-key code to [`Key`].
///
/// Letters, digits (top row + numpad), Space, Escape, Return, Tab. Modifier
/// vkeys are `None`. Anything else becomes [`Key::Other`] so extra keys still
/// dirty an active chord.
pub fn vk_to_key(vk: u32) -> Option<Key> {
    if is_modifier_vk(vk) {
        return None;
    }
    if (VK_A..=VK_Z).contains(&vk) {
        let letter = (b'a' + (vk - VK_A) as u8) as char;
        return Some(Key::parse(&letter.to_string()));
    }
    if (VK_0..=VK_9).contains(&vk) {
        return Some(digit_key((vk - VK_0) as u8));
    }
    if (VK_NUMPAD0..=VK_NUMPAD9).contains(&vk) {
        return Some(digit_key((vk - VK_NUMPAD0) as u8));
    }
    Some(match vk {
        VK_SPACE => Key::Space,
        VK_ESCAPE => Key::Escape,
        VK_RETURN => Key::Return,
        VK_TAB => Key::Tab,
        other => Key::Other(format!("vk_{other:02x}")),
    })
}

fn digit_key(n: u8) -> Key {
    match n {
        0 => Key::Zero,
        1 => Key::One,
        2 => Key::Two,
        3 => Key::Three,
        4 => Key::Four,
        5 => Key::Five,
        6 => Key::Six,
        7 => Key::Seven,
        8 => Key::Eight,
        9 => Key::Nine,
        _ => Key::Other(n.to_string()),
    }
}

pub fn is_injected_ll_flags(flags: u32) -> bool {
    flags & LLKHF_INJECTED != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_digits_space_escape() {
        assert_eq!(vk_to_key(VK_A), Some(Key::A));
        assert_eq!(vk_to_key(VK_V), Some(Key::V));
        assert_eq!(vk_to_key(VK_Z), Some(Key::Z));
        assert_eq!(vk_to_key(VK_0), Some(Key::Zero));
        assert_eq!(vk_to_key(VK_1), Some(Key::One));
        assert_eq!(vk_to_key(VK_9), Some(Key::Nine));
        assert_eq!(vk_to_key(VK_NUMPAD0), Some(Key::Zero));
        assert_eq!(vk_to_key(VK_NUMPAD9), Some(Key::Nine));
        assert_eq!(vk_to_key(VK_SPACE), Some(Key::Space));
        assert_eq!(vk_to_key(VK_ESCAPE), Some(Key::Escape));
        assert_eq!(vk_to_key(VK_RETURN), Some(Key::Return));
        assert_eq!(vk_to_key(VK_TAB), Some(Key::Tab));
    }

    #[test]
    fn modifiers_map_to_none() {
        assert_eq!(vk_to_key(VK_SHIFT), None);
        assert_eq!(vk_to_key(VK_LWIN), None);
        assert_eq!(vk_to_key(VK_RWIN), None);
        assert_eq!(vk_to_key(VK_LMENU), None);
        assert_eq!(vk_to_key(VK_RCONTROL), None);
        assert!(is_modifier_vk(VK_LCONTROL));
        assert!(!is_modifier_vk(VK_SPACE));
    }

    #[test]
    fn unknown_vk_is_other() {
        match vk_to_key(0x70) {
            Some(Key::Other(s)) => assert_eq!(s, "vk_70"),
            other => panic!("expected Other, got {other:?}"),
        }
    }

    #[test]
    fn win_shift_is_command_shift() {
        let mods = ModifierDown {
            win: true,
            shift: true,
            ..ModifierDown::default()
        }
        .to_modifiers();
        assert!(mods.contains(Modifier::Command));
        assert!(mods.contains(Modifier::Shift));
        assert!(!mods.contains(Modifier::Control));
        assert!(!mods.contains(Modifier::Option));
    }

    #[test]
    fn alt_is_option() {
        let mods = ModifierDown {
            alt: true,
            ..ModifierDown::default()
        }
        .to_modifiers();
        assert!(mods.contains(Modifier::Option));
    }

    #[test]
    fn apply_vk_forces_post_event_state() {
        let snap = ModifierDown {
            win: true,
            shift: true,
            ..ModifierDown::default()
        };
        let after_space_up = apply_vk_to_modifiers(snap, VK_SPACE, true);
        assert!(after_space_up.win && after_space_up.shift);
        let after_win_up = apply_vk_to_modifiers(after_space_up, VK_LWIN, true);
        assert!(!after_win_up.win);
        assert!(after_win_up.shift);
        let after_ctrl_down = apply_vk_to_modifiers(ModifierDown::default(), VK_CONTROL, false);
        assert!(after_ctrl_down.control);
    }

    #[test]
    fn key_down_has_key_key_up_does_not() {
        let mods = ModifierDown {
            win: true,
            shift: true,
            ..ModifierDown::default()
        }
        .to_modifiers();
        let down = key_event_from_win(WinEventKind::KeyDown, VK_SPACE, mods.clone());
        assert_eq!(down.key, Some(Key::Space));
        assert!(down.modifiers.contains(Modifier::Command));
        assert!(down.modifiers.contains(Modifier::Shift));

        let up = key_event_from_win(WinEventKind::KeyUp, VK_SPACE, mods);
        assert_eq!(up.key, None);
        assert!(up.modifiers.contains(Modifier::Command));
    }

    #[test]
    fn modifier_keydown_is_keyless_like_flagschanged() {
        let mods = ModifierDown {
            win: true,
            ..ModifierDown::default()
        }
        .to_modifiers();
        let e = key_event_from_win(WinEventKind::KeyDown, VK_LWIN, mods);
        assert_eq!(e.key, None);
        assert!(e.modifiers.contains(Modifier::Command));
    }

    #[test]
    fn wparam_and_ll_flags_agree_on_keyup() {
        assert_eq!(
            WinEventKind::from_wparam(WM_KEYUP as usize),
            WinEventKind::KeyUp
        );
        assert_eq!(
            WinEventKind::from_wparam(WM_SYSKEYUP as usize),
            WinEventKind::KeyUp
        );
        assert_eq!(
            WinEventKind::from_wparam(WM_KEYDOWN as usize),
            WinEventKind::KeyDown
        );
        assert_eq!(WinEventKind::from_ll_flags(LLKHF_UP), WinEventKind::KeyUp);
        assert_eq!(WinEventKind::from_ll_flags(0), WinEventKind::KeyDown);
        assert!(is_injected_ll_flags(LLKHF_INJECTED));
        assert!(!is_injected_ll_flags(0));
    }

    #[test]
    fn vk_space_win_shift_drives_press_and_hold() {
        use crate::process_tap_event;
        use lailaisay_core::{HotKey, HotKeyOutput, HotKeyProcessor, Key, Modifier, Modifiers};

        let mut processor = HotKeyProcessor::new(
            HotKey {
                key: Some(Key::Space),
                modifiers: Modifiers::new([Modifier::Command, Modifier::Shift]),
            },
            false,
        );
        let mut edit = None;
        processor.set_now(4.0);
        let mods = ModifierDown {
            win: true,
            shift: true,
            ..ModifierDown::default()
        }
        .to_modifiers();
        let start = process_tap_event(
            &mut processor,
            &mut edit,
            key_event_from_win(WinEventKind::KeyDown, VK_SPACE, mods.clone()),
        );
        assert_eq!(start.output, Some(HotKeyOutput::StartRecording));
        assert!(start.swallowed);
        processor.set_now(4.6);
        let stop = process_tap_event(
            &mut processor,
            &mut edit,
            key_event_from_win(WinEventKind::KeyUp, VK_SPACE, mods),
        );
        assert_eq!(stop.output, Some(HotKeyOutput::StopRecording));
    }
}
