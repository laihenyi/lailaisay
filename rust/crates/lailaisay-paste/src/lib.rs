//! Copy / paste into the frontmost app.
//!
//! - **Every OS:** clipboard write via `arboard` ([`copy_text`]).
//! - **macOS:** clipboard + System Events / CGEvent ⌘V / AX insert ([`macos`]).
//!   Needs Accessibility + Automation TCC — see [`strategy::PASTE_TCC_HELP`].
//! - **Windows:** clipboard + [`SendInput`](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-sendinput)
//!   Ctrl+V (Shift+Insert fallback). HWND restore is best-effort. See
//!   [`strategy::WINDOWS_PASTE_HELP`]. [`PasteError::WindowsCopyOnly`] if
//!   injection fails.
//! - **Linux CI:** copy-only.

use lailaisay_core::LailaisaySettings;
use thiserror::Error;

pub mod strategy;
pub mod windows_keys;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

pub use strategy::{
    ax_value_shows_paste, choose_strategy, escape_applescript, is_ax_flaky_terminal_bundle,
    is_electron_or_chromium, should_attempt_second_clipboard_paste, PasteStrategy, PasteTarget,
    PASTE_TCC_HELP, WINDOWS_PASTE_HELP,
};
pub use windows_keys::{
    chord_keys, ctrl_v_sequence, shift_insert_sequence, windows_paste_chords, SyntheticKey,
    WindowsPasteChord,
};

#[derive(Debug, Error)]
pub enum PasteError {
    #[error("clipboard error: {0}")]
    Clipboard(String),
    #[error(
        "paste is only fully implemented on macOS (this host is {0}); text was copied instead"
    )]
    CopyOnlyFallback(&'static str),
    #[error(
        "Windows SendInput paste failed; text was copied to the clipboard — paste with Ctrl+V"
    )]
    WindowsCopyOnly,
    #[error("{0}")]
    Message(String),
}

pub type Result<T> = std::result::Result<T, PasteError>;

/// Write UTF-8 text to the system clipboard.
pub fn copy_text(text: &str) -> Result<()> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| PasteError::Clipboard(e.to_string()))?;
    clipboard
        .set_text(text.to_string())
        .map_err(|e| PasteError::Clipboard(e.to_string()))
}

/// Read the focused field's selected text (macOS AX). `Ok(None)` if empty / unavailable.
pub fn read_selected_text() -> Result<Option<String>> {
    #[cfg(target_os = "macos")]
    {
        return macos::read_selected_text();
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(None)
    }
}

/// Frontmost bundle + AX field snapshot (macOS) or HWND + exe (Windows).
/// Call at hotkey-down, not after STT/LLM.
pub fn capture_frontmost_target() -> Option<PasteTarget> {
    #[cfg(target_os = "macos")]
    {
        return macos::capture_frontmost_target();
    }
    #[cfg(target_os = "windows")]
    {
        return windows::capture_frontmost_target();
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

/// Replace the current AX selection, or fall back to clipboard paste.
pub fn replace_selected_text(text: &str, settings: &LailaisaySettings) -> Result<()> {
    replace_selected_text_to(text, settings, None)
}

pub fn replace_selected_text_to(
    text: &str,
    settings: &LailaisaySettings,
    target: Option<&PasteTarget>,
) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        if let Some(t) = target {
            macos::activate_target(t);
        }
        if macos::ax_replace_selected(text) {
            eprintln!(
                "[lailaisay-paste] AxInsert replace ok target={} clipboard_written=false",
                target
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "current".into())
            );
            return Ok(());
        }
    }
    paste_text_to(text, settings, target)
}

/// Best-effort paste: macOS uses menu/keyboard/accessibility strategies; Windows uses SendInput.
pub fn paste_text(text: &str, settings: &LailaisaySettings) -> Result<()> {
    paste_text_to(text, settings, None)
}

pub fn paste_text_to(
    text: &str,
    settings: &LailaisaySettings,
    target: Option<&PasteTarget>,
) -> Result<()> {
    if settings.copy_to_clipboard || settings.use_clipboard_paste {
        copy_text(text)?;
    }

    #[cfg(target_os = "macos")]
    {
        return macos::paste_into_target(text, settings, target);
    }

    #[cfg(target_os = "windows")]
    {
        if !settings.copy_to_clipboard {
            copy_text(text)?;
        }
        return windows::paste_into_target(text, settings, target);
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = target;
        if !settings.copy_to_clipboard {
            copy_text(text)?;
        }
        Err(PasteError::CopyOnlyFallback(std::env::consts::OS))
    }
}

pub fn paste_is_native() -> bool {
    cfg!(any(target_os = "macos", target_os = "windows"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_does_not_panic_without_display() {
        let _ = copy_text("lailaisay-paste test");
    }

    #[test]
    fn paste_native_flag_matches_os() {
        assert_eq!(
            paste_is_native(),
            cfg!(any(target_os = "macos", target_os = "windows"))
        );
    }

    #[test]
    fn windows_copy_only_message_names_sendinput() {
        let msg = PasteError::WindowsCopyOnly.to_string();
        assert!(msg.contains("SendInput"), "{msg}");
        assert!(msg.contains("clipboard"), "{msg}");
        assert!(WINDOWS_PASTE_HELP.contains("SendInput"));
        assert!(WINDOWS_PASTE_HELP.contains("UI Automation"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_copy_is_the_safe_fallback() {
        // Full paste needs Accessibility + Automation + a frontmost app.
        // CI / headless Macs only prove clipboard write.
        let _ = copy_text("lailaisay-paste macos fallback");
    }
}
