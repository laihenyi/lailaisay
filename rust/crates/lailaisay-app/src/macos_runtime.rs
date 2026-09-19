//! macOS-only NSWorkspace / Dock policy helpers used by the eframe host.
#![cfg(target_os = "macos")]

pub use crate::desktop::{build_tray, status_icon, stop_and_dispatch};
use crate::desktop::{settings_deactivate_action, SettingsDeactivateAction};

/// Bundle id + localized name via NSWorkspace (no System Events Automation).
pub fn workspace_frontmost() -> Option<(String, String)> {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let ws: *mut Object = msg_send![class!(NSWorkspace), sharedWorkspace];
        if ws.is_null() {
            return None;
        }
        let app: *mut Object = msg_send![ws, frontmostApplication];
        if app.is_null() {
            return None;
        }
        let bid: *mut Object = msg_send![app, bundleIdentifier];
        let name: *mut Object = msg_send![app, localizedName];
        Some((
            nsstring_to_rust(bid)?,
            nsstring_to_rust(name).unwrap_or_default(),
        ))
    }
}

fn nsstring_to_rust(s: *mut objc::runtime::Object) -> Option<String> {
    use objc::{msg_send, sel, sel_impl};
    if s.is_null() {
        return None;
    }
    unsafe {
        let c: *const std::os::raw::c_char = msg_send![s, UTF8String];
        if c.is_null() {
            return None;
        }
        Some(std::ffi::CStr::from_ptr(c).to_string_lossy().into_owned())
    }
}

/// `NSApplicationActivationPolicyRegular` shows a Dock icon;
/// `NSApplicationActivationPolicyAccessory` is menu-bar only.
///
/// `LSUIElement` keeps launch accessory (no Dock flash when the setting is
/// off). Switching to Regular does not make Launch Services load
/// `AppIcon.icns` for an agent bundle, so the Dock tile must also receive
/// `applicationIconImage` from the packaged waveform icns.
pub fn apply_activation_policy(show_dock_icon: bool) {
    use crate::dock_icon::{activation_policy_code, should_apply_bundled_app_icon};
    use objc::{class, msg_send, sel, sel_impl};
    let policy = activation_policy_code(show_dock_icon);
    unsafe {
        let app: *mut objc::runtime::Object = msg_send![class!(NSApplication), sharedApplication];
        if should_apply_bundled_app_icon(show_dock_icon) {
            apply_bundled_app_icon(app);
        }
        let _: bool = msg_send![app, setActivationPolicy: policy];
        if should_apply_bundled_app_icon(show_dock_icon) {
            apply_bundled_app_icon(app);
        }
    }
}

/// Same bytes as `Contents/Resources/AppIcon.icns`. Used when `NSBundle` has
/// no resource (unbundled `cargo run`) or Launch Services will not name it.
const APP_ICON_ICNS: &[u8] = include_bytes!("../../../macos/AppIcon.icns");

unsafe fn apply_bundled_app_icon(app: *mut objc::runtime::Object) {
    use objc::{msg_send, sel, sel_impl};
    // `Option::or_else` needs a `FnOnce` closure. Passing an `unsafe fn` item
    // is E0277 (`expected a FnOnce() closure, found unsafe fn`).
    let icon = crate::dock_icon::first_present(
        nsimage_from_bundle_icns(),
        || unsafe { nsimage_named_app_icon() },
        || unsafe { nsimage_from_embedded_icns() },
    );
    let Some(icon) = icon else {
        eprintln!("[lailaisay-app] Dock icon: AppIcon.icns could not be loaded");
        return;
    };
    let _: () = msg_send![app, setApplicationIconImage: icon];
    let _: () = msg_send![icon, release];
}

unsafe fn nsstring(utf8: &str) -> *mut objc::runtime::Object {
    use objc::{class, msg_send, sel, sel_impl};
    let c = std::ffi::CString::new(utf8).expect("icon name is utf-8");
    msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()]
}

unsafe fn nsimage_from_bundle_icns() -> Option<*mut objc::runtime::Object> {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    let bundle: *mut Object = msg_send![class!(NSBundle), mainBundle];
    if bundle.is_null() {
        return None;
    }
    let name = nsstring("AppIcon");
    let ext = nsstring("icns");
    let path: *mut Object = msg_send![bundle, pathForResource: name ofType: ext];
    if path.is_null() {
        return None;
    }
    let alloc: *mut Object = msg_send![class!(NSImage), alloc];
    let icon: *mut Object = msg_send![alloc, initWithContentsOfFile: path];
    if icon.is_null() {
        None
    } else {
        Some(icon)
    }
}

unsafe fn nsimage_named_app_icon() -> Option<*mut objc::runtime::Object> {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    let name = nsstring("AppIcon");
    let icon: *mut Object = msg_send![class!(NSImage), imageNamed: name];
    if icon.is_null() {
        return None;
    }
    let _: *mut Object = msg_send![icon, retain];
    Some(icon)
}

unsafe fn nsimage_from_embedded_icns() -> Option<*mut objc::runtime::Object> {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    let data: *mut Object = msg_send![
        class!(NSData),
        dataWithBytes: APP_ICON_ICNS.as_ptr()
        length: APP_ICON_ICNS.len()
    ];
    if data.is_null() {
        return None;
    }
    let alloc: *mut Object = msg_send![class!(NSImage), alloc];
    let icon: *mut Object = msg_send![alloc, initWithData: data];
    if icon.is_null() {
        None
    } else {
        Some(icon)
    }
}

/// Accessory applications can leave their settings above another application's
/// windows even at NSNormalWindowLevel. Yield once on app deactivation rather
/// than repeatedly reordering on repaint. Use application activity (not key
/// window activity) so menus and file dialogs within lailaisay do not trigger this.
///
/// When Settings is already hide-to-tray (`settings_visible` false), `orderOut`
/// instead of `orderBack` — `orderBack` deminiaturizes / shows the window
/// when the user switches to LINE or Safari.
pub fn yield_inactive_settings_window(previous: &mut Option<bool>, settings_visible: bool) {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        // Query the actual foreground process. NSApplication::isActive can
        // lag behind workspace activation for a tray/resident app.
        let workspace: *mut Object = msg_send![class!(NSWorkspace), sharedWorkspace];
        let frontmost: *mut Object = msg_send![workspace, frontmostApplication];
        if frontmost.is_null() {
            return;
        }
        let pid: i32 = msg_send![frontmost, processIdentifier];
        let active = pid as u32 == std::process::id();
        let should_yield = should_yield_settings(*previous, active);
        *previous = Some(active);
        let action = settings_deactivate_action(settings_visible, should_yield);
        if action == SettingsDeactivateAction::None {
            return;
        }
        let windows: *mut Object = msg_send![app, windows];
        let count: usize = msg_send![windows, count];
        for index in 0..count {
            let window: *mut Object = msg_send![windows, objectAtIndex: index];
            let title: *mut Object = msg_send![window, title];
            if nsstring_to_rust(title).as_deref() != Some("lailaisay 設定") {
                continue;
            }
            match action {
                SettingsDeactivateAction::OrderOut => {
                    let _: () = msg_send![window, orderOut: std::ptr::null::<Object>()];
                }
                SettingsDeactivateAction::OrderBack => {
                    let _: () = msg_send![window, setLevel: 0isize];
                    let _: () = msg_send![window, orderBack: std::ptr::null::<Object>()];
                }
                SettingsDeactivateAction::None => {}
            }
        }
    }
}

fn should_yield_settings(previous: Option<bool>, active: bool) -> bool {
    !active && previous != Some(false)
}

#[cfg(test)]
mod window_order_tests {
    use super::should_yield_settings;
    use crate::desktop::{settings_deactivate_action, SettingsDeactivateAction};

    #[test]
    fn yields_only_on_initial_inactivity_or_deactivation() {
        assert!(should_yield_settings(None, false));
        assert!(should_yield_settings(Some(true), false));
        assert!(!should_yield_settings(Some(false), false));
        assert!(!should_yield_settings(None, true));
        assert!(!should_yield_settings(Some(false), true));
        assert!(!should_yield_settings(Some(true), true));
    }

    #[test]
    fn hidden_settings_order_out_on_deactivate_not_order_back() {
        assert_eq!(
            settings_deactivate_action(false, should_yield_settings(Some(true), false)),
            SettingsDeactivateAction::OrderOut
        );
        assert_eq!(
            settings_deactivate_action(true, should_yield_settings(Some(true), false)),
            SettingsDeactivateAction::OrderBack
        );
    }
}
