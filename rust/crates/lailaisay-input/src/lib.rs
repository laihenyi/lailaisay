//! Global hotkey capture.
//!
//! The **state machine** lives in `lailaisay_core::HotKeyProcessor` (unit-tested on
//! every OS). This crate is the OS event source:
//!
//! - **macOS:** HID `CGEventTap`.
//!   Requires Accessibility (and sometimes Input Monitoring). See
//!   [`permissions::ACCESSIBILITY_HELP`].
//! - **macOS App Store (sandbox):** [`MacOsCarbonHotkey`] — Carbon
//!   `RegisterEventHotKey`, no TCC prompt, chord must include a key.
//! - **Windows:** [`WindowsEventTap`] — `WH_KEYBOARD_LL` on a message-pump
//!   thread, same [`TapMessage`] channel as macOS. See
//!   [`permissions::WINDOWS_HOTKEY_HELP`].
//! - **Linux / CI:** [`UnsupportedMonitor`]. Use `lailaisay-cli` CLI `--file` or
//!   `lailaisay-app --once`.
//!
//! Menu-bar / settings host (`lailaisay-app`):
//! 1. Tap emits [`TapMessage`] (event + processor output)
//! 2. `lailaisay_app::Session` maps start/stop/cancel, including the 200 ms delay
//! 3. record → STT → enhance → paste

use lailaisay_core::KeyEvent;
use thiserror::Error;

pub mod cfptr;
pub mod mapping;
pub mod permissions;
pub mod swallow;
pub mod tap;
pub mod vk;

#[cfg(target_os = "macos")]
pub mod carbon;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod macos_tcc;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod stub;
#[cfg(target_os = "windows")]
mod windows;

pub use mapping::{flags_to_modifiers, key_event_from_cg, keycode_to_key, CgEventKind};
pub use permissions::{
    accessibility_grant, accessibility_trusted, automation_grant, input_monitoring_grant,
    map_automation_status, map_av_authorization_status, map_avaudio_record_permission,
    microphone_grant, note_microphone_opened, resolve_accessibility_grant,
    resolve_input_monitoring_grant, resolve_microphone_grant, GrantStatus, ACCESSIBILITY_HELP,
    MICROPHONE_HELP, WINDOWS_HOTKEY_HELP, WINDOWS_MICROPHONE_HELP,
};
pub use swallow::should_swallow;
pub use tap::{process_tap_event, TapMessage};
pub use vk::{
    apply_vk_to_modifiers, is_modifier_vk, key_event_from_win, vk_to_key, ModifierDown,
    WinEventKind,
};

#[cfg(target_os = "macos")]
pub use carbon::{carbon_hotkey_supported, MacOsCarbonHotkey};
#[cfg(target_os = "macos")]
pub use macos::MacOsEventTap;
#[cfg(target_os = "windows")]
pub use windows::{WindowsEventTap, WindowsHotkeyMonitor};

#[derive(Debug, Error)]
pub enum InputError {
    #[error("global hotkeys are only implemented on macOS and Windows (this host is {0})")]
    UnsupportedOs(&'static str),
    #[error("{0}")]
    Message(String),
}

pub type Result<T> = std::result::Result<T, InputError>;

/// A source of global key events. The GUI / daemon owns the processor.
pub trait HotkeyMonitor {
    /// Block until the next event, or return `None` on timeout / stop.
    fn next_event(&mut self) -> Result<Option<KeyEvent>>;
}

/// Construct the platform monitor (starts the tap / hook).
pub fn native_monitor() -> Result<Box<dyn HotkeyMonitor>> {
    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(macos::MacOsEventTap::new()?))
    }
    #[cfg(target_os = "windows")]
    {
        Ok(Box::new(windows::WindowsEventTap::new()?))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Ok(Box::new(stub::UnsupportedMonitor))
    }
}

pub fn is_macos_hotkey_available() -> bool {
    cfg!(target_os = "macos")
}

pub fn is_windows_hotkey_available() -> bool {
    cfg!(target_os = "windows")
}

pub fn is_native_hotkey_available() -> bool {
    is_macos_hotkey_available() || is_windows_hotkey_available()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_hotkey_availability_matches_os() {
        assert_eq!(is_macos_hotkey_available(), cfg!(target_os = "macos"));
        assert_eq!(is_windows_hotkey_available(), cfg!(target_os = "windows"));
        assert_eq!(
            is_native_hotkey_available(),
            cfg!(any(target_os = "macos", target_os = "windows"))
        );
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let mut mon = native_monitor().expect("stub constructs");
            match mon.next_event() {
                Err(InputError::UnsupportedOs(_)) => {}
                other => panic!("expected UnsupportedOs, got {other:?}"),
            }
            assert!(!accessibility_trusted(false));
            assert_eq!(accessibility_grant(), GrantStatus::Denied);
            assert_eq!(microphone_grant(), GrantStatus::Unknown);
            assert_eq!(input_monitoring_grant(), GrantStatus::Unknown);
            assert_eq!(automation_grant(), GrantStatus::Unknown);
        }
        assert!(WINDOWS_HOTKEY_HELP.contains("WH_KEYBOARD_LL"));
        assert!(WINDOWS_MICROPHONE_HELP.contains("Microphone"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_accessibility_check_does_not_panic() {
        // Must not prompt in tests (`false`). Result depends on TCC.
        let _ = accessibility_trusted(false);
        assert!(is_macos_hotkey_available());
    }
}
