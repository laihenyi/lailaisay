//! Windows clipboard paste via `SendInput` Ctrl+V (Shift+Insert fallback).
//!
//! Text is already on the clipboard (`arboard`) when the caller used
//! `copy_to_clipboard` / `use_clipboard_paste`. This module:
//!
//! 1. Restores the HWND captured at hotkey-down (best-effort — Windows
//!    foreground-lock can refuse `SetForegroundWindow` after a long LLM).
//! 2. Injects Ctrl+V, then Shift+Insert if that fails.
//! 3. Returns [`PasteError::WindowsCopyOnly`] if injection fails so the
//!    transcript stays on the clipboard.
//!
//! Success is “`SendInput` accepted every event”. We do **not** use UI
//! Automation to prove the field changed (v1). Antivirus or a UIPI mismatch
//! can still drop injected keys.

use std::thread;
use std::time::Duration;

use lailaisay_core::LailaisaySettings;

use crate::windows_keys::{chord_keys, windows_paste_chords, SyntheticKey, WindowsPasteChord};
use crate::{PasteError, PasteTarget, Result};

const INPUT_KEYBOARD: u32 = 1;
const KEYEVENTF_EXTENDEDKEY: u32 = 0x0001;
const KEYEVENTF_KEYUP: u32 = 0x0002;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

#[repr(C)]
#[derive(Clone, Copy)]
struct KeybdInput {
    w_vk: u16,
    w_scan: u16,
    dw_flags: u32,
    time: u32,
    extra: usize,
}

/// Win32 `INPUT` on x64/ARM64: DWORD type + 4-byte pad + 32-byte union
/// (`MOUSEINPUT` is the largest member). `KEYBDINPUT` itself is 24 bytes;
/// padding the union to 32 keeps `sizeof(INPUT) == 40` (MSVC CI measured
/// 48 when this pad was wrong, and `SendInput` then rejected `cbSize`).
#[repr(C)]
union InputUnion {
    ki: KeybdInput,
    _largest: [u8; 32],
}

#[repr(C)]
struct Input {
    type_: u32,
    _pad: u32,
    u: InputUnion,
}

#[link(name = "user32")]
extern "system" {
    fn SendInput(n_inputs: u32, inputs: *const Input, cb_size: i32) -> u32;
    fn GetForegroundWindow() -> isize;
    fn SetForegroundWindow(hwnd: isize) -> i32;
    fn GetWindowTextW(hwnd: isize, lp_string: *mut u16, n_max: i32) -> i32;
    fn GetWindowThreadProcessId(hwnd: isize, lpdw_process_id: *mut u32) -> u32;
    fn AttachThreadInput(id_attach: u32, id_attach_to: u32, attach: i32) -> i32;
    fn IsWindow(hwnd: isize) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentThreadId() -> u32;
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
    fn QueryFullProcessImageNameW(
        process: isize,
        flags: u32,
        name: *mut u16,
        size: *mut u32,
    ) -> i32;
    fn CloseHandle(handle: isize) -> i32;
}

fn input_from_key(k: SyntheticKey) -> Input {
    let mut flags = 0u32;
    if k.up {
        flags |= KEYEVENTF_KEYUP;
    }
    if k.extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    Input {
        type_: INPUT_KEYBOARD,
        _pad: 0,
        u: InputUnion {
            ki: KeybdInput {
                w_vk: k.vk,
                w_scan: 0,
                dw_flags: flags,
                time: 0,
                extra: 0,
            },
        },
    }
}

fn send_chord(chord: WindowsPasteChord) -> bool {
    let keys = chord_keys(chord);
    let inputs: Vec<Input> = keys.iter().copied().map(input_from_key).collect();
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<Input>() as i32,
        )
    };
    sent == inputs.len() as u32
}

/// Best-effort restore of the window that was focused at hotkey-down.
///
/// Windows can ignore `SetForegroundWindow` when lailaisay is not the foreground
/// process (especially after a long enhance). We `AttachThreadInput` to the
/// target and current foreground threads; this can still fail. Documented
/// limit — user can click the field; text stays on the clipboard.
pub fn restore_foreground(hwnd: isize) -> bool {
    if hwnd == 0 {
        return false;
    }
    unsafe {
        if IsWindow(hwnd) == 0 {
            return false;
        }
        let current_fg = GetForegroundWindow();
        if current_fg == hwnd {
            return true;
        }
        let our_tid = GetCurrentThreadId();
        let target_tid = GetWindowThreadProcessId(hwnd, std::ptr::null_mut());
        let fg_tid = if current_fg != 0 {
            GetWindowThreadProcessId(current_fg, std::ptr::null_mut())
        } else {
            0
        };
        if target_tid != 0 && target_tid != our_tid {
            AttachThreadInput(our_tid, target_tid, 1);
        }
        if fg_tid != 0 && fg_tid != our_tid && fg_tid != target_tid {
            AttachThreadInput(our_tid, fg_tid, 1);
        }
        let ok = SetForegroundWindow(hwnd) != 0;
        if target_tid != 0 && target_tid != our_tid {
            AttachThreadInput(our_tid, target_tid, 0);
        }
        if fg_tid != 0 && fg_tid != our_tid && fg_tid != target_tid {
            AttachThreadInput(our_tid, fg_tid, 0);
        }
        ok
    }
}

fn wide_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

fn window_title(hwnd: isize) -> Option<String> {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if n <= 0 {
        return None;
    }
    let s = wide_to_string(&buf);
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn process_image_name(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle == 0 {
            return None;
        }
        let mut buf = [0u16; 512];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size);
        CloseHandle(handle);
        if ok == 0 {
            return None;
        }
        let path = wide_to_string(&buf);
        std::path::Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .or(Some(path))
    }
}

/// Frontmost HWND + exe name. Call at hotkey-down, not after STT/LLM.
pub fn capture_frontmost_target() -> Option<PasteTarget> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd == 0 {
        return None;
    }
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
    }
    let exe = if pid != 0 {
        process_image_name(pid)
    } else {
        None
    };
    Some(PasteTarget {
        bundle_id: exe.clone().unwrap_or_default(),
        app_name: window_title(hwnd).or(exe),
        ax_role: None,
        ax_value_len: None,
        hwnd: Some(hwnd),
    })
}

/// Copy is already done by [`crate::paste_text_to`]. Inject Ctrl+V / Shift+Insert.
pub fn paste_into_target(
    text: &str,
    settings: &LailaisaySettings,
    target: Option<&PasteTarget>,
) -> Result<()> {
    let _ = (text, settings);
    if let Some(hwnd) = target.and_then(|t| t.hwnd).filter(|&h| h != 0) {
        let restored = restore_foreground(hwnd);
        eprintln!(
            "[lailaisay-paste] Windows restore HWND={hwnd:#x} ok={restored} target={}",
            target
                .map(ToString::to_string)
                .unwrap_or_else(|| "?".into())
        );
        if restored {
            thread::sleep(Duration::from_millis(30));
        }
    } else {
        eprintln!(
            "[lailaisay-paste] Windows paste into current foreground (no captured HWND). \
             Focus restore is best-effort; Windows may refuse SetForegroundWindow."
        );
    }

    for chord in windows_paste_chords() {
        if send_chord(chord) {
            eprintln!(
                "[lailaisay-paste] SendInput {} ok (not UIA-verified; field change is best-effort)",
                match chord {
                    WindowsPasteChord::CtrlV => "Ctrl+V",
                    WindowsPasteChord::ShiftInsert => "Shift+Insert",
                }
            );
            return Ok(());
        }
        eprintln!(
            "[lailaisay-paste] SendInput {} failed, trying next chord",
            match chord {
                WindowsPasteChord::CtrlV => "Ctrl+V",
                WindowsPasteChord::ShiftInsert => "Shift+Insert",
            }
        );
    }

    eprintln!("[lailaisay-paste] SendInput failed — text is on the clipboard. Paste with Ctrl+V.");
    Err(PasteError::WindowsCopyOnly)
}

/// `std::mem::size_of::<Input>()` must be 40 on 64-bit Windows.
#[cfg(test)]
fn input_struct_size() -> usize {
    std::mem::size_of::<Input>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windows_keys::VK_INSERT;

    #[test]
    fn input_layout_is_40_on_64bit() {
        if cfg!(target_pointer_width = "64") {
            assert_eq!(std::mem::size_of::<KeybdInput>(), 24);
            assert_eq!(std::mem::size_of::<InputUnion>(), 32);
            assert_eq!(input_struct_size(), 40, "x64 INPUT is 40 bytes");
        }
    }

    #[test]
    fn insert_is_extended_in_builder() {
        let input = input_from_key(SyntheticKey::down_ext(VK_INSERT));
        assert_eq!(input.type_, INPUT_KEYBOARD);
        let ki = unsafe { input.u.ki };
        assert_eq!(ki.w_vk, VK_INSERT);
        assert_eq!(ki.dw_flags & KEYEVENTF_EXTENDEDKEY, KEYEVENTF_EXTENDEDKEY);
        assert_eq!(ki.dw_flags & KEYEVENTF_KEYUP, 0);
    }

    #[test]
    fn paste_returns_ok_or_copy_only() {
        let settings = LailaisaySettings::default();
        match paste_into_target("hello", &settings, None) {
            Ok(()) => {}
            Err(PasteError::WindowsCopyOnly) => {}
            other => panic!("expected Ok or WindowsCopyOnly, got {other:?}"),
        }
    }
}
