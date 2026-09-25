//! Shared tray, status glyph, and mic-stop helpers for macOS and Windows.
//!
//! Linux compiles the hide-to-tray decision + glyph RGBA so CI can prove the
//! product-shape rules without linking `tray-icon`.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::settings_ui::TrayGlyph;
use crate::shared::{set_shared_status, SharedState, Work};

/// Tray → Settings (Traditional Chinese, same tone as the Settings form).
pub const TRAY_SETTINGS_LABEL: &str = "開啟設定";
/// Tray → Quit.
pub const TRAY_QUIT_LABEL: &str = "結束 lailaisay";
/// Disabled status row prefix.
pub const TRAY_STATUS_PREFIX: &str = "lailaisay — ";

/// Speak-to-Edit hint. Chord wording follows the host OS.
pub fn tray_edit_label() -> &'static str {
    if cfg!(target_os = "windows") {
        "選取文字後按住 Alt+Shift+Space 改寫"
    } else {
        "選取文字後按住 ⌥⇧Space 改寫"
    }
}

/// Close the Settings viewport but keep the process (tray / hook) alive.
pub fn should_hide_to_tray(should_quit: bool, tray_available: bool) -> bool {
    !should_quit && tray_available
}

/// Yellow traffic light / OS miniaturize must become hide-to-tray.
///
/// A miniaturized Settings window (`settings_visible` still true) is restored
/// when the Dock tile, Regular activation policy, or HUD `orderFront`
/// activates the app — including when the user merely switches to LINE.
pub fn should_convert_minimize_to_hide(
    settings_visible: bool,
    os_minimized: bool,
    should_quit: bool,
    tray_available: bool,
) -> bool {
    settings_visible && os_minimized && should_hide_to_tray(should_quit, tray_available)
}

/// After hide-to-tray / launch-minimize, only `open_settings` (tray 開啟設定)
/// or an explicit launch show may send `Visible(true)`.
pub fn may_show_settings_viewport(settings_visible: bool) -> bool {
    settings_visible
}

/// What AppKit should do to the Settings `NSWindow` when the app is no
/// longer frontmost. `orderBack` on a hidden or miniaturized window can
/// show it again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsDeactivateAction {
    None,
    OrderBack,
    OrderOut,
}

pub fn settings_deactivate_action(
    settings_visible: bool,
    should_yield: bool,
) -> SettingsDeactivateAction {
    if !should_yield {
        return SettingsDeactivateAction::None;
    }
    if settings_visible {
        SettingsDeactivateAction::OrderBack
    } else {
        SettingsDeactivateAction::OrderOut
    }
}

/// 16×16 RGBA waveform used by the menu extra / notification-area icon.
pub fn status_icon_rgba(kind: TrayGlyph) -> (Vec<u8>, u32, u32) {
    const N: u32 = 16;
    let mut rgba = vec![0u8; (N * N * 4) as usize];
    let (r, g, b) = match kind {
        TrayGlyph::Idle => (167, 171, 176),
        TrayGlyph::Active => (187, 136, 26),
        TrayGlyph::Error => (207, 66, 56),
    };
    let heights = match kind {
        TrayGlyph::Idle => [4i32, 8, 12, 8, 4],
        TrayGlyph::Active => [6, 12, 14, 10, 5],
        TrayGlyph::Error => [10, 4, 12, 4, 10],
    };
    for (i, h) in heights.iter().enumerate() {
        let cx = 3 + i as i32 * 3;
        let top = 8 - h / 2;
        for y in top..(top + h) {
            for dx in 0..2 {
                let x = cx + dx;
                if (0..16).contains(&x) && (0..16).contains(&y) {
                    let idx = ((y * N as i32 + x) * 4) as usize;
                    rgba[idx] = r;
                    rgba[idx + 1] = g;
                    rgba[idx + 2] = b;
                    rgba[idx + 3] = 255;
                }
            }
        }
    }
    (rgba, N, N)
}

pub fn stop_and_dispatch(
    work_tx: &mpsc::Sender<Work>,
    shared: &Arc<Mutex<SharedState>>,
    rec_started_at: &mut Option<Instant>,
    minimum: f64,
    edit_selection: Option<String>,
    #[cfg(feature = "mic")] live: &mut Option<lailaisay_stt::record::LiveRecorder>,
) {
    let elapsed = rec_started_at
        .take()
        .map(|t| t.elapsed().as_secs_f64())
        .unwrap_or(0.0);
    if elapsed > 0.0 && elapsed < minimum {
        tracing::info!("discarded hold shorter than minimum_key_time ({elapsed:.2}s)");
        #[cfg(feature = "mic")]
        {
            let _ = live.take().map(|r| r.stop());
        }
        set_shared_status(shared, "idle");
        return;
    }
    set_shared_status(shared, "transcribing");
    let pcm = {
        #[cfg(feature = "mic")]
        {
            if let Some(rec) = live.take() {
                match rec.stop() {
                    Ok(pcm) => pcm,
                    Err(e) => {
                        tracing::error!("mic stop: {e}");
                        Vec::new()
                    }
                }
            } else {
                Vec::new()
            }
        }
        #[cfg(not(feature = "mic"))]
        {
            Vec::new()
        }
    };
    if let Some(selected) = edit_selection {
        let _ = work_tx.send(Work::SpeakToEdit { pcm, selected });
    } else {
        let _ = work_tx.send(Work::Transcribe(pcm));
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod native {
    use anyhow::Result;
    use tray_icon::menu::{Menu, MenuId, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

    use super::{
        status_icon_rgba, tray_edit_label, TRAY_QUIT_LABEL, TRAY_SETTINGS_LABEL, TRAY_STATUS_PREFIX,
    };
    use crate::settings_ui::TrayGlyph;

    pub fn status_icon(kind: TrayGlyph) -> Icon {
        let (rgba, w, h) = status_icon_rgba(kind);
        let (r, g, b) = match kind {
            TrayGlyph::Idle => (167, 171, 176),
            TrayGlyph::Active => (187, 136, 26),
            TrayGlyph::Error => (207, 66, 56),
        };
        Icon::from_rgba(rgba, w, h)
            .unwrap_or_else(|_| Icon::from_rgba(vec![r, g, b, 255], 1, 1).expect("1x1 icon"))
    }

    pub fn build_tray() -> Result<(TrayIcon, MenuItem, MenuId, MenuId, MenuId)> {
        let menu = Menu::new();
        let status_item = MenuItem::with_id(
            "status",
            format!("{TRAY_STATUS_PREFIX}starting"),
            false,
            None,
        );
        let settings_item = MenuItem::with_id("settings", TRAY_SETTINGS_LABEL, true, None);
        let edit_item = MenuItem::with_id("speak_to_edit", tray_edit_label(), true, None);
        let quit_item = MenuItem::with_id("quit", TRAY_QUIT_LABEL, true, None);
        let settings_id = settings_item.id().clone();
        let edit_id = edit_item.id().clone();
        let quit_id = quit_item.id().clone();
        menu.append(&status_item)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&settings_item)?;
        if crate::settings_ui::speak_to_edit_supported() {
            menu.append(&edit_item)?;
        }
        menu.append(&quit_item)?;
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("lailaisay")
            .with_icon(status_icon(TrayGlyph::Idle))
            .build()?;
        Ok((tray, status_item, settings_id, edit_id, quit_id))
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use native::{build_tray, status_icon};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hide_to_tray_only_when_tray_is_up_and_not_quitting() {
        assert!(should_hide_to_tray(false, true));
        assert!(!should_hide_to_tray(true, true));
        assert!(!should_hide_to_tray(false, false));
        assert!(!should_hide_to_tray(true, false));
    }

    #[test]
    fn minimize_is_hide_to_tray_and_settings_stays_hidden() {
        assert!(
            should_convert_minimize_to_hide(true, true, false, true),
            "minimize ≡ hide-to-tray"
        );
        let settings_visible = false;
        assert!(
            !may_show_settings_viewport(settings_visible),
            "settings_visible must be false after minimize"
        );
        assert!(!should_convert_minimize_to_hide(false, true, false, true));
        assert!(!should_convert_minimize_to_hide(true, false, false, true));
        assert!(
            !should_convert_minimize_to_hide(true, true, false, false),
            "no tray: leave OS miniaturize so Dock can restore"
        );
        assert!(!should_convert_minimize_to_hide(true, true, true, true));
        assert_eq!(
            settings_deactivate_action(false, true),
            SettingsDeactivateAction::OrderOut,
            "switching to LINE must orderOut hidden Settings, not orderBack"
        );
        assert_eq!(
            settings_deactivate_action(true, true),
            SettingsDeactivateAction::OrderBack
        );
        assert_eq!(
            settings_deactivate_action(false, false),
            SettingsDeactivateAction::None
        );
        assert!(may_show_settings_viewport(true));
    }

    #[test]
    fn tray_labels_are_traditional_chinese() {
        assert_eq!(TRAY_SETTINGS_LABEL, "開啟設定");
        assert_eq!(TRAY_QUIT_LABEL, "結束 lailaisay");
        assert!(tray_edit_label().contains("選取文字"));
        if cfg!(target_os = "windows") {
            assert!(tray_edit_label().contains("Alt+Shift+Space"));
        } else {
            assert!(tray_edit_label().contains("⌥⇧Space"));
        }
    }

    #[test]
    fn status_icon_rgba_is_16px_opaque_waveform() {
        let (idle, w, h) = status_icon_rgba(TrayGlyph::Idle);
        assert_eq!((w, h), (16, 16));
        assert_eq!(idle.len(), 16 * 16 * 4);
        assert!(
            idle.chunks(4).any(|px| px == [167, 171, 176, 255]),
            "idle glyph should paint the grey waveform"
        );
        let (active, _, _) = status_icon_rgba(TrayGlyph::Active);
        assert!(active.chunks(4).any(|px| px == [187, 136, 26, 255]));
        let (err, _, _) = status_icon_rgba(TrayGlyph::Error);
        assert!(err.chunks(4).any(|px| px == [207, 66, 56, 255]));
    }
}
