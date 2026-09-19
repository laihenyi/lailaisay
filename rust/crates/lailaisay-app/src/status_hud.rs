//! Always-on-top recording / STT lamp, independent of the Settings window.
//!
//! Visibility follows the same status substrings as the Settings overlay HUD
//! (`record` / `transcrib` / `enhanc`). The host syncs this from the same
//! shared-status read as the tray (`sync_tray_status`), on the UI thread.

use egui::{Align2, Area, CornerRadius, Frame, Order, Pos2, Rect, Sense, Vec2};

use crate::theme::{self, GLASS_FILL, WARN};

/// Same title used by the Windows/Linux egui viewport (must not match Settings).
pub const HUD_WINDOW_TITLE: &str = "lailaisay-hud";
pub const HUD_INNER_SIZE: [f32; 2] = [168.0, 44.0];

/// Show the lamp only while hold-to-talk or Whisper/polish is in flight.
pub fn hud_visible(status: &str) -> bool {
    let s = status.to_ascii_lowercase();
    s.contains("record") || s.contains("transcrib") || s.contains("enhanc")
}

/// Compact pill copy. Recording is distinct from Whisper / LLM work.
pub fn hud_label(status: &str) -> &'static str {
    if hud_is_recording(status) {
        "錄音中"
    } else {
        "處理中"
    }
}

pub fn hud_is_recording(status: &str) -> bool {
    status.to_ascii_lowercase().contains("record")
}

/// Waveform bar height used by the Settings overlay, the Windows viewport,
/// and the macOS NSPanel so the motion stays in sync.
pub fn hud_bar_height(recording: bool, reduced_motion: bool, time: f64, index: usize) -> f32 {
    if recording && !reduced_motion {
        4.0 + 10.0 * (time * 6.0 + index as f64 * 0.7).sin().abs() as f32
    } else {
        7.0
    }
}

/// Settings-window overlay. Invisible when Settings is closed — the floating
/// lamp in [`sync_floating_hud`] is the always-on-top copy.
pub fn paint_recording_hud(ctx: &egui::Context, status: &str) {
    if !hud_visible(status) {
        return;
    }
    let recording = hud_is_recording(status);
    let t = ctx.input(|i| i.time);
    let reduced = ctx.style().animation_time <= 0.0;

    Area::new(egui::Id::new("lailaisay_rec_hud"))
        .order(Order::Foreground)
        .anchor(Align2::CENTER_BOTTOM, [0.0, -20.0])
        .interactable(false)
        .show(ctx, |ui| {
            Frame::new()
                .fill(GLASS_FILL)
                .shadow(theme::CONTROL_SHADOW)
                .corner_radius(theme::pill(44.0))
                .inner_margin(egui::Margin::symmetric(16, 8))
                .show(ui, |ui| {
                    paint_hud_row(ui, recording, t, reduced);
                });
        });
}

fn paint_hud_row(ui: &mut egui::Ui, recording: bool, time: f64, reduced: bool) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(if recording { "錄音中" } else { "處理中" })
                .size(12.0)
                .color(WARN),
        );
        let bars = 5;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(48.0, 16.0), Sense::hover());
        for i in 0..bars {
            let h = hud_bar_height(recording, reduced, time, i);
            let x = rect.left() + 4.0 + i as f32 * 9.0;
            let bar = Rect::from_center_size(Pos2::new(x, rect.center().y), Vec2::new(3.0, h));
            ui.painter().rect_filled(bar, CornerRadius::same(2), WARN);
        }
    });
}

/// UI-thread sync. Does not create AppKit / winit objects from the worker.
pub fn sync_floating_hud(ctx: &egui::Context, status: &str) {
    #[cfg(target_os = "macos")]
    {
        let _ = ctx;
        macos_panel::sync(status);
    }
    #[cfg(not(target_os = "macos"))]
    {
        sync_egui_hud(ctx, status);
    }
}

/// Borderless always-on-top viewport for Windows (and Linux `--settings`).
pub fn floating_hud_viewport_builder() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title(HUD_WINDOW_TITLE)
        .with_inner_size(HUD_INNER_SIZE)
        .with_min_inner_size(HUD_INNER_SIZE)
        .with_max_inner_size(HUD_INNER_SIZE)
        .with_decorations(false)
        .with_resizable(false)
        .with_taskbar(false)
        .with_active(false)
        .with_mouse_passthrough(true)
        .with_always_on_top()
        .with_visible(true)
}

#[cfg(not(target_os = "macos"))]
fn hud_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("lailaisay_status_hud")
}

#[cfg(not(target_os = "macos"))]
fn sync_egui_hud(ctx: &egui::Context, status: &str) {
    if !hud_visible(status) {
        return;
    }
    let mut builder = floating_hud_viewport_builder();
    if let Some(pos) = hud_anchor(ctx) {
        builder = builder.with_position(pos);
    }
    let recording = hud_is_recording(status);
    ctx.show_viewport_immediate(hud_viewport_id(), builder, move |ctx, _class| {
        crate::theme::apply_visuals(ctx);
        ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
            egui::WindowLevel::AlwaysOnTop,
        ));
        paint_floating_viewport(ctx, recording);
    });
}

#[cfg(not(target_os = "macos"))]
fn hud_anchor(ctx: &egui::Context) -> Option<egui::Pos2> {
    ctx.input(|i| {
        let size = i.viewport().monitor_size?;
        Some(egui::pos2(
            (size.x - HUD_INNER_SIZE[0]) * 0.5,
            size.y - HUD_INNER_SIZE[1] - 28.0,
        ))
    })
}

#[cfg(not(target_os = "macos"))]
fn paint_floating_viewport(ctx: &egui::Context, recording: bool) {
    let t = ctx.input(|i| i.time);
    let reduced = ctx.style().animation_time <= 0.0;
    egui::CentralPanel::default()
        .frame(
            Frame::new()
                .fill(GLASS_FILL)
                .shadow(theme::CONTROL_SHADOW)
                .corner_radius(theme::pill(44.0))
                .inner_margin(egui::Margin::symmetric(16, 8)),
        )
        .show(ctx, |ui| {
            paint_hud_row(ui, recording, t, reduced);
        });
}

/// macOS NSPanel: non-activating, click-through, status-window level.
/// Kept off the worker thread (AppKit is main-thread only).
#[cfg(target_os = "macos")]
mod macos_panel {
    use std::cell::RefCell;
    use std::ffi::CString;
    use std::sync::OnceLock;
    use std::time::Instant;

    use objc::declare::ClassDecl;
    use objc::runtime::{Class, Object, Sel, BOOL, NO, YES};
    use objc::{class, msg_send, sel, sel_impl};

    use super::{hud_bar_height, hud_is_recording, hud_label, hud_visible};

    const HUD_W: f64 = 152.0;
    const HUD_H: f64 = 36.0;
    /// `NSWindowStyleMaskBorderless | NSWindowStyleMaskNonactivatingPanel`
    pub const HUD_STYLE_MASK: u64 = 1 << 7;
    /// `NSStatusWindowLevel` — above other apps, below screensaver.
    pub const HUD_WINDOW_LEVEL: isize = 25;
    /// Join all spaces + stay put + skip cycle + survive fullscreen.
    pub const HUD_COLLECTION_BEHAVIOR: u64 = (1 << 0) | (1 << 3) | (1 << 4) | (1 << 6) | (1 << 8);
    /// Subclass so `canBecomeKeyWindow` / `canBecomeMainWindow` stay false.
    pub const HUD_PANEL_CLASS_NAME: &str = "LailaisayHudPanel";

    fn hud_panel_class() -> &'static Class {
        static CLASS: OnceLock<&'static Class> = OnceLock::new();
        CLASS.get_or_init(|| unsafe { register_hud_panel_class() })
    }

    unsafe fn register_hud_panel_class() -> &'static Class {
        if let Some(existing) = Class::get(HUD_PANEL_CLASS_NAME) {
            return existing;
        }
        let mut decl = ClassDecl::new(HUD_PANEL_CLASS_NAME, class!(NSPanel))
            .expect("LailaisayHudPanel ClassDecl");
        decl.add_method(
            sel!(canBecomeKeyWindow),
            hud_can_become_key as extern "C" fn(&Object, Sel) -> BOOL,
        );
        decl.add_method(
            sel!(canBecomeMainWindow),
            hud_can_become_main as extern "C" fn(&Object, Sel) -> BOOL,
        );
        decl.register()
    }

    extern "C" fn hud_can_become_key(_this: &Object, _cmd: Sel) -> BOOL {
        NO
    }

    extern "C" fn hud_can_become_main(_this: &Object, _cmd: Sel) -> BOOL {
        NO
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSPoint {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSSize {
        width: f64,
        height: f64,
    }

    struct StatusHud {
        panel: *mut Object,
        label: *mut Object,
        bars: [*mut Object; 5],
        shown: bool,
        last_label: &'static str,
        started: Instant,
    }

    thread_local! {
        static HUD: RefCell<Option<StatusHud>> = const { RefCell::new(None) };
    }

    pub fn sync(status: &str) {
        HUD.with(|cell| {
            let mut slot = cell.borrow_mut();
            if !hud_visible(status) {
                if let Some(hud) = slot.as_mut() {
                    hide(hud);
                }
                return;
            }
            if slot.is_none() {
                *slot = unsafe { create() };
            }
            let Some(hud) = slot.as_mut() else {
                return;
            };
            unsafe {
                apply_label(hud, hud_label(status));
                apply_bars(hud, hud_is_recording(status));
                if !hud.shown {
                    position(hud.panel);
                    // `orderFront:` can activate NSApp and restore Settings.
                    let _: () = msg_send![hud.panel, orderFrontRegardless];
                    hud.shown = true;
                }
            }
        });
    }

    fn hide(hud: &mut StatusHud) {
        if !hud.shown {
            return;
        }
        unsafe {
            let _: () = msg_send![hud.panel, orderOut: std::ptr::null::<Object>()];
        }
        hud.shown = false;
    }

    unsafe fn create() -> Option<StatusHud> {
        // `new` + `setContentSize` / `setFrameOrigin` avoids passing NSRect
        // through `objc_msgSend` (HFA ABI is easy to get wrong with objc 0.2).
        let panel: *mut Object = msg_send![hud_panel_class(), new];
        if panel.is_null() {
            eprintln!("[lailaisay-app] status HUD: NSPanel alloc failed");
            return None;
        }
        let _: () = msg_send![panel, setStyleMask: HUD_STYLE_MASK];
        let _: () = msg_send![
            panel,
            setContentSize: NSSize {
                width: HUD_W,
                height: HUD_H,
            }
        ];

        let _: () = msg_send![panel, setTitle: nsstring(super::HUD_WINDOW_TITLE)];
        let _: () = msg_send![panel, setLevel: HUD_WINDOW_LEVEL];
        let _: () = msg_send![panel, setOpaque: NO];
        let clear: *mut Object = msg_send![class!(NSColor), clearColor];
        let _: () = msg_send![panel, setBackgroundColor: clear];
        let _: () = msg_send![panel, setIgnoresMouseEvents: YES];
        let _: () = msg_send![panel, setHidesOnDeactivate: NO];
        let _: () = msg_send![panel, setFloatingPanel: YES];
        let _: () = msg_send![panel, setBecomesKeyOnlyIfNeeded: YES];
        let _: () = msg_send![panel, setReleasedWhenClosed: NO];
        let _: () = msg_send![panel, setExcludedFromWindowsMenu: YES];
        let _: () = msg_send![panel, setCollectionBehavior: HUD_COLLECTION_BEHAVIOR];
        // NSWindowAnimationBehaviorNone
        let _: () = msg_send![panel, setAnimationBehavior: 2isize];
        let _: () = msg_send![panel, setHasShadow: YES];

        let fx: *mut Object = msg_send![class!(NSVisualEffectView), new];
        if fx.is_null() {
            let _: () = msg_send![panel, release];
            eprintln!("[lailaisay-app] status HUD: NSVisualEffectView failed");
            return None;
        }
        let _: () = msg_send![
            fx,
            setFrameSize: NSSize {
                width: HUD_W,
                height: HUD_H,
            }
        ];
        // HUDWindow / BehindWindow / Active
        let _: () = msg_send![fx, setMaterial: 13isize];
        let _: () = msg_send![fx, setBlendingMode: 0isize];
        let _: () = msg_send![fx, setState: 1isize];
        let _: () = msg_send![fx, setWantsLayer: YES];
        let layer: *mut Object = msg_send![fx, layer];
        if !layer.is_null() {
            let _: () = msg_send![layer, setCornerRadius: 18.0f64];
            let _: () = msg_send![layer, setMasksToBounds: YES];
        }
        let _: () = msg_send![panel, setContentView: fx];
        let _: () = msg_send![fx, release];

        let label: *mut Object = msg_send![class!(NSTextField), new];
        if label.is_null() {
            let _: () = msg_send![panel, release];
            return None;
        }
        let _: () = msg_send![label, setFrameOrigin: NSPoint { x: 14.0, y: 8.0 }];
        let _: () = msg_send![
            label,
            setFrameSize: NSSize {
                width: 56.0,
                height: 20.0,
            }
        ];
        let font: *mut Object = msg_send![class!(NSFont), systemFontOfSize: 12.0f64];
        let _: () = msg_send![label, setFont: font];
        let _: () = msg_send![label, setTextColor: warn_color()];
        let _: () = msg_send![label, setBezeled: NO];
        let _: () = msg_send![label, setDrawsBackground: NO];
        let _: () = msg_send![label, setEditable: NO];
        let _: () = msg_send![label, setSelectable: NO];
        let _: () = msg_send![fx, addSubview: label];
        let _: () = msg_send![label, release];

        let mut bars = [std::ptr::null_mut(); 5];
        for (i, slot) in bars.iter_mut().enumerate() {
            let bar: *mut Object = msg_send![class!(NSView), new];
            if bar.is_null() {
                let _: () = msg_send![panel, release];
                return None;
            }
            place_bar(bar, i, 7.0);
            let _: () = msg_send![bar, setWantsLayer: YES];
            let bar_layer: *mut Object = msg_send![bar, layer];
            if !bar_layer.is_null() {
                if let Some(cg) = warn_cgcolor() {
                    let _: () = msg_send![bar_layer, setBackgroundColor: cg];
                }
                let _: () = msg_send![bar_layer, setCornerRadius: 1.5f64];
            }
            let _: () = msg_send![fx, addSubview: bar];
            let _: () = msg_send![bar, release];
            *slot = bar;
        }

        Some(StatusHud {
            panel,
            label,
            bars,
            shown: false,
            last_label: "",
            started: Instant::now(),
        })
    }

    unsafe fn apply_label(hud: &mut StatusHud, text: &'static str) {
        if hud.last_label == text {
            return;
        }
        let _: () = msg_send![hud.label, setStringValue: nsstring(text)];
        hud.last_label = text;
    }

    unsafe fn apply_bars(hud: &StatusHud, recording: bool) {
        let t = hud.started.elapsed().as_secs_f64();
        for (i, bar) in hud.bars.iter().enumerate() {
            if bar.is_null() {
                continue;
            }
            let h = hud_bar_height(recording, false, t, i) as f64;
            place_bar(*bar, i, h);
        }
    }

    unsafe fn place_bar(bar: *mut Object, index: usize, height: f64) {
        let x = 78.0 + index as f64 * 9.0;
        let y = (HUD_H - height) * 0.5;
        let _: () = msg_send![bar, setFrameOrigin: NSPoint { x, y }];
        let _: () = msg_send![
            bar,
            setFrameSize: NSSize {
                width: 3.0,
                height,
            }
        ];
    }

    unsafe fn position(panel: *mut Object) {
        let screen: *mut Object = msg_send![class!(NSScreen), mainScreen];
        if screen.is_null() {
            return;
        }
        // `NSDeviceSize` + `sizeValue` returns NSSize (2 doubles), not NSRect.
        let desc: *mut Object = msg_send![screen, deviceDescription];
        if desc.is_null() {
            return;
        }
        let key = nsstring("NSDeviceSize");
        let val: *mut Object = msg_send![desc, objectForKey: key];
        if val.is_null() {
            return;
        }
        let px: NSSize = msg_send![val, sizeValue];
        let scale: f64 = msg_send![screen, backingScaleFactor];
        let scale = if scale > 0.0 { scale } else { 1.0 };
        let width = px.width / scale;
        let x = (width - HUD_W) * 0.5;
        let y = 48.0;
        let _: () = msg_send![panel, setFrameOrigin: NSPoint { x, y }];
    }

    unsafe fn warn_color() -> *mut Object {
        msg_send![
            class!(NSColor),
            colorWithCalibratedRed: 187.0 / 255.0
            green: 136.0 / 255.0
            blue: 26.0 / 255.0
            alpha: 1.0
        ]
    }

    unsafe fn warn_cgcolor() -> Option<*const std::ffi::c_void> {
        let color = warn_color();
        if color.is_null() {
            return None;
        }
        let cg: *const std::ffi::c_void = msg_send![color, CGColor];
        if cg.is_null() {
            None
        } else {
            Some(cg)
        }
    }

    unsafe fn nsstring(utf8: &str) -> *mut Object {
        let c = CString::new(utf8).expect("hud string is utf-8");
        msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings_ui::tray_glyph_from_status;
    use crate::theme::WARN;

    #[test]
    fn shows_only_for_record_transcribe_enhance() {
        for status in [
            "recording",
            "speak-to-edit recording",
            "transcribing",
            "enhancing",
            "RECORDING",
            "Transcribing…",
        ] {
            assert!(hud_visible(status), "{status}");
            assert_eq!(
                tray_glyph_from_status(status),
                crate::settings_ui::TrayGlyph::Active,
                "{status} must stay tray-Active"
            );
        }
        for status in [
            "idle",
            "pasted",
            "replaced",
            "done",
            "",
            "待命",
            "辨識失敗：boom",
            "error: nope",
            "needs Accessibility",
            "mic error",
        ] {
            assert!(!hud_visible(status), "{status}");
        }
    }

    #[test]
    fn labels_recording_versus_processing() {
        assert_eq!(hud_label("recording"), "錄音中");
        assert_eq!(hud_label("speak-to-edit recording"), "錄音中");
        assert_eq!(hud_label("transcribing"), "處理中");
        assert_eq!(hud_label("enhancing"), "處理中");
        assert!(hud_is_recording("recording"));
        assert!(!hud_is_recording("transcribing"));
        assert!(!hud_is_recording("enhancing"));
    }

    #[test]
    fn bar_heights_animate_only_while_recording() {
        let a = hud_bar_height(true, false, 0.2, 0);
        let b = hud_bar_height(true, false, 0.4, 0);
        assert_ne!(a, b, "recording bars must move");
        assert_eq!(hud_bar_height(false, false, 0.2, 0), 7.0);
        assert_eq!(hud_bar_height(true, true, 0.2, 0), 7.0);
        assert!((4.0..=14.0).contains(&a));
    }

    #[test]
    fn floating_viewport_is_click_through_and_not_settings() {
        let vp = floating_hud_viewport_builder();
        assert_eq!(vp.title.as_deref(), Some(HUD_WINDOW_TITLE));
        assert_ne!(vp.title.as_deref(), Some("lailaisay 設定"));
        assert_eq!(vp.decorations, Some(false));
        assert_eq!(vp.mouse_passthrough, Some(true));
        assert_eq!(vp.window_level, Some(egui::WindowLevel::AlwaysOnTop));
        assert_eq!(vp.taskbar, Some(false));
        assert_eq!(vp.active, Some(false));
        assert_eq!(
            vp.inner_size,
            Some(Vec2::new(HUD_INNER_SIZE[0], HUD_INNER_SIZE[1]))
        );
    }

    #[test]
    fn overlay_hud_paints_when_recording_and_skips_idle() {
        let rec = run_overlay("recording");
        let idle = run_overlay("idle");
        let pasted = run_overlay("pasted");
        assert!(
            rec.shapes.len() > idle.shapes.len(),
            "recording shapes={} idle={}",
            rec.shapes.len(),
            idle.shapes.len()
        );
        assert_eq!(idle.shapes.len(), pasted.shapes.len());
    }

    fn run_overlay(status: &str) -> egui::FullOutput {
        let ctx = egui::Context::default();
        crate::theme::apply_visuals(&ctx);
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 200.0))),
                ..Default::default()
            },
            |ctx| {
                paint_recording_hud(ctx, status);
            },
        )
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_panel_is_nonactivating_status_level() {
        assert_eq!(macos_panel::HUD_STYLE_MASK, 1 << 7);
        assert_eq!(macos_panel::HUD_WINDOW_LEVEL, 25);
        assert_eq!(
            macos_panel::HUD_COLLECTION_BEHAVIOR,
            (1 << 0) | (1 << 3) | (1 << 4) | (1 << 6) | (1 << 8)
        );
        assert_eq!(macos_panel::HUD_PANEL_CLASS_NAME, "LailaisayHudPanel");
    }

    #[test]
    fn hud_warn_matches_theme() {
        assert_eq!(WARN, egui::Color32::from_rgb(187, 136, 26));
    }
}
