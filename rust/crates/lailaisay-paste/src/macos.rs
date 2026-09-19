//! macOS paste: clipboard + System Events / CGEvent ⌘V / AX insert.
//!
//! Runtime-only on a Mac with Accessibility + Automation. Linux compiles the
//! strategy module instead.

use std::process::Command;
use std::thread;
use std::time::Duration;

use lailaisay_core::LailaisaySettings;

use crate::strategy::{
    ax_value_shows_paste, choose_strategy, cmd_v_script, fallback_after, frontmost_bundle_script,
    is_vscode, menu_paste_script, should_attempt_second_clipboard_paste, type_text_script,
    PasteStrategy, PasteTarget,
};
use crate::{copy_text, PasteError, Result};

type CfTypeRef = *const std::ffi::c_void;
type AxUiElementRef = *mut std::ffi::c_void;
type CfStringRef = *const std::ffi::c_void;
type CgEventRef = *mut std::ffi::c_void;
type CgEventSourceRef = *mut std::ffi::c_void;

const K_AX_ERROR_SUCCESS: i32 = 0;
const K_CG_EVENT_SOURCE_STATE_COMBINED: i32 = 1;
const CG_KEY_COMMAND: u16 = 55;
const CG_KEY_V: u16 = 9;
const CG_FLAG_COMMAND: u64 = 0x0010_0000;

#[link(name = "ApplicationServices", kind = "framework")]
#[link(name = "CoreGraphics", kind = "framework")]
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn AXUIElementCreateSystemWide() -> AxUiElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AxUiElementRef,
        attribute: CfStringRef,
        value: *mut CfTypeRef,
    ) -> i32;
    fn AXUIElementSetAttributeValue(
        element: AxUiElementRef,
        attribute: CfStringRef,
        value: CfTypeRef,
    ) -> i32;
    fn CFStringCreateWithCString(
        alloc: *const std::ffi::c_void,
        c_str: *const i8,
        encoding: u32,
    ) -> CfStringRef;
    fn CFStringGetLength(s: CfStringRef) -> isize;
    fn CFStringGetMaximumSizeForEncoding(len: isize, encoding: u32) -> isize;
    fn CFStringGetCString(s: CfStringRef, buf: *mut i8, max: isize, encoding: u32) -> bool;
    fn CFRelease(cf: CfTypeRef);

    fn CGEventSourceCreate(state_id: i32) -> CgEventSourceRef;
    fn CGEventCreateKeyboardEvent(
        source: CgEventSourceRef,
        virtual_key: u16,
        key_down: bool,
    ) -> CgEventRef;
    fn CGEventSetFlags(event: CgEventRef, flags: u64);
    fn CGEventPost(tap: u32, event: CgEventRef);
}

const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
const K_CG_SESSION_EVENT_TAP: u32 = 1;

pub fn paste_into_target(
    text: &str,
    settings: &LailaisaySettings,
    target: Option<&PasteTarget>,
) -> Result<()> {
    copy_text(text)?;
    eprintln!(
        "[lailaisay-paste] clipboard written=true ({} chars)",
        text.chars().count()
    );
    thread::sleep(Duration::from_millis(50));

    if let Some(t) = target.filter(|t| !t.is_empty()) {
        eprintln!("[lailaisay-paste] restoring focus to {t}");
        if !activate_target(t) {
            eprintln!(
                "[lailaisay-paste] activate failed for {t} — will still try current frontmost"
            );
        }
        thread::sleep(Duration::from_millis(150));
    }

    let current = frontmost_bundle_id();
    let bundle = target
        .map(|t| t.bundle_id.clone())
        .filter(|s| !s.is_empty())
        .or_else(|| current.clone())
        .unwrap_or_default();
    eprintln!(
        "[lailaisay-paste] target_bundle={bundle} frontmost_now={}",
        current.as_deref().unwrap_or("?")
    );

    let primary = choose_strategy(&bundle, settings.use_clipboard_paste);
    let mut attempts = vec![primary];
    attempts.extend_from_slice(fallback_after(primary));

    let before = snapshot_focused_field();
    let mut succeeded = false;
    for strategy in attempts {
        match apply_strategy(strategy, text, &bundle, &before) {
            Ok(()) => {
                eprintln!("[lailaisay-paste] {strategy} ok bundle={bundle}");
                succeeded = true;
                break;
            }
            Err(why) => {
                eprintln!("[lailaisay-paste] {strategy} failed bundle={bundle}: {why}");
            }
        }
    }

    if !succeeded {
        eprintln!("============================================================");
        eprintln!(
            "[lailaisay-paste] PASTE FAILED — text is on the clipboard. Cmd+V in the target app."
        );
        eprintln!("[lailaisay-paste] target={bundle} clipboard_written=true");
        eprintln!("============================================================");
        return Err(PasteError::Message(
            crate::strategy::PASTE_TCC_HELP.trim().into(),
        ));
    }

    if !settings.copy_to_clipboard {
        eprintln!("[lailaisay-paste] copyToClipboard=false: previous pasteboard restore is not yet ported; text stays on clipboard");
    }
    Ok(())
}

pub fn capture_frontmost_target() -> Option<PasteTarget> {
    let bundle_id = frontmost_bundle_id().unwrap_or_default();
    let app_name = osascript_string(
        r#"tell application "System Events" to get name of first application process whose frontmost is true"#,
    );
    let snap = snapshot_focused_field();
    let t = PasteTarget {
        bundle_id,
        app_name,
        ax_role: snap.role.clone(),
        ax_value_len: snap.value.as_ref().map(|s| s.chars().count()),
        hwnd: None,
    };
    if t.is_empty() && snap.role.is_none() {
        None
    } else {
        Some(t)
    }
}

pub fn activate_target(target: &PasteTarget) -> bool {
    if !target.bundle_id.is_empty() {
        let out = Command::new("/usr/bin/open")
            .args(["-b", &target.bundle_id])
            .output();
        match out {
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                eprintln!(
                    "[lailaisay-paste] activate open -b {} exit={} stderr={}",
                    target.bundle_id,
                    o.status.code().unwrap_or(-1),
                    trim_log(&stderr)
                );
                if o.status.success() {
                    return true;
                }
            }
            Err(e) => eprintln!("[lailaisay-paste] activate open -b: {e}"),
        }
        let script = format!(
            r#"tell application "System Events" to set frontmost of first process whose bundle identifier is "{}" to true"#,
            target.bundle_id.replace('\\', "\\\\").replace('"', "\\\"")
        );
        return run_osascript(&script).success;
    }
    if let Some(name) = target.app_name.as_deref().filter(|s| !s.is_empty()) {
        let script = format!(
            r#"tell application "System Events" to set frontmost of process "{}" to true"#,
            name.replace('\\', "\\\\").replace('"', "\\\"")
        );
        return run_osascript(&script).success;
    }
    false
}

struct ScriptResult {
    success: bool,
    exit: i32,
    stdout: String,
    stderr: String,
}

impl ScriptResult {
    fn log_line(&self) -> String {
        format!(
            "osascript exit={} stderr={} stdout={}",
            self.exit,
            trim_log(&self.stderr),
            trim_log(&self.stdout)
        )
    }
}

struct FieldSnap {
    role: Option<String>,
    value: Option<String>,
    selected: Option<String>,
}

fn apply_strategy(
    strategy: PasteStrategy,
    text: &str,
    bundle: &str,
    before: &FieldSnap,
) -> std::result::Result<(), String> {
    match strategy {
        PasteStrategy::MenuPaste => {
            let r = run_osascript(menu_paste_script());
            if !r.success {
                return Err(r.log_line());
            }
            let t = r.stdout.trim().to_ascii_lowercase();
            if t != "true" && t != "yes" {
                return Err(format!("menu Paste not enabled/clicked ({})", r.log_line()));
            }
            confirm_clipboard_script(text, bundle, before, &r.log_line())
        }
        PasteStrategy::AppleScriptCmdV => {
            if is_vscode(bundle) {
                thread::sleep(Duration::from_millis(200));
            } else {
                thread::sleep(Duration::from_millis(100));
            }
            let r = run_osascript(cmd_v_script());
            if !r.success {
                return Err(r.log_line());
            }
            confirm_clipboard_script(text, bundle, before, &r.log_line())
        }
        PasteStrategy::TypeText => {
            let r = run_osascript(&type_text_script(text));
            if !r.success {
                return Err(r.log_line());
            }
            confirm_if_ax_available(text, before, &r.log_line())
        }
        PasteStrategy::AxInsert => {
            if ax_insert(text) {
                Ok(())
            } else {
                Err("AXSetAttribute AXSelectedText failed".into())
            }
        }
        PasteStrategy::CgEventCmdV => {
            cgevent_cmd_v()?;
            thread::sleep(Duration::from_millis(80));
            let after = snapshot_focused_field();
            if paste_landed(before, &after, text) {
                Ok(())
            } else {
                Err("CGEventPost returned; AX field unchanged — not treating as success".into())
            }
        }
    }
}

fn confirm_if_ax_available(
    text: &str,
    before: &FieldSnap,
    script_ok: &str,
) -> std::result::Result<(), String> {
    thread::sleep(Duration::from_millis(60));
    let after = snapshot_focused_field();
    if before.value.is_none()
        && after.value.is_none()
        && before.selected.is_none()
        && after.selected.is_none()
    {
        // No AX on this field (common in some Electron apps). Trust osascript.
        return Ok(());
    }
    if paste_landed(before, &after, text) {
        Ok(())
    } else {
        Err(format!(
            "script reported success but AX field unchanged ({script_ok})"
        ))
    }
}

/// MenuPaste / AppleScript ⌘V already reported a successful click or keystroke.
/// Do not fall through to another clipboard paste unless AX can prove the
/// field still lacks `text`. Terminals often look unchanged after a real paste.
fn confirm_clipboard_script(
    text: &str,
    bundle: &str,
    before: &FieldSnap,
    script_ok: &str,
) -> std::result::Result<(), String> {
    thread::sleep(Duration::from_millis(60));
    let mut after = snapshot_focused_field();
    if !paste_landed(before, &after, text) {
        thread::sleep(Duration::from_millis(80));
        after = snapshot_focused_field();
    }
    if paste_landed(before, &after, text) {
        return Ok(());
    }
    let (before_ax, after_ax) = if before.value.is_some() || after.value.is_some() {
        (before.value.as_deref(), after.value.as_deref())
    } else {
        (before.selected.as_deref(), after.selected.as_deref())
    };
    if should_attempt_second_clipboard_paste(bundle, before_ax, after_ax, text) {
        Err(format!(
            "script reported success but AX field still lacks text ({script_ok})"
        ))
    } else {
        eprintln!("[lailaisay-paste] accepting clipboard script without AX growth bundle={bundle}");
        Ok(())
    }
}

fn paste_landed(before: &FieldSnap, after: &FieldSnap, text: &str) -> bool {
    ax_value_shows_paste(before.value.as_deref(), after.value.as_deref(), text)
        || ax_value_shows_paste(before.selected.as_deref(), after.selected.as_deref(), text)
}

fn frontmost_bundle_id() -> Option<String> {
    osascript_string(frontmost_bundle_script())
}

fn run_osascript(script: &str) -> ScriptResult {
    match Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .output()
    {
        Ok(out) => ScriptResult {
            success: out.status.success(),
            exit: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        },
        Err(e) => ScriptResult {
            success: false,
            exit: -1,
            stdout: String::new(),
            stderr: e.to_string(),
        },
    }
}

fn osascript_string(script: &str) -> Option<String> {
    let r = run_osascript(script);
    if !r.success || r.stdout.is_empty() {
        if !r.stderr.is_empty() {
            eprintln!("[lailaisay-paste] {}", r.log_line());
        }
        None
    } else {
        Some(r.stdout)
    }
}

fn trim_log(s: &str) -> String {
    let t = s.replace('\n', " ").trim().to_string();
    if t.chars().count() > 240 {
        format!("{}…", t.chars().take(240).collect::<String>())
    } else if t.is_empty() {
        "∅".into()
    } else {
        t
    }
}

/// Post ⌘V. Success of *posting* is not paste success — caller must verify AX.
fn cgevent_cmd_v() -> std::result::Result<(), String> {
    unsafe {
        let source = CGEventSourceCreate(K_CG_EVENT_SOURCE_STATE_COMBINED);
        if source.is_null() {
            return Err("CGEventSourceCreate failed".into());
        }
        let cmd_down = CGEventCreateKeyboardEvent(source, CG_KEY_COMMAND, true);
        let v_down = CGEventCreateKeyboardEvent(source, CG_KEY_V, true);
        let v_up = CGEventCreateKeyboardEvent(source, CG_KEY_V, false);
        let cmd_up = CGEventCreateKeyboardEvent(source, CG_KEY_COMMAND, false);
        if cmd_down.is_null() || v_down.is_null() || v_up.is_null() || cmd_up.is_null() {
            return Err("CGEventCreateKeyboardEvent failed".into());
        }
        CGEventSetFlags(v_down, CG_FLAG_COMMAND);
        CGEventSetFlags(v_up, CG_FLAG_COMMAND);
        CGEventPost(K_CG_SESSION_EVENT_TAP, cmd_down);
        thread::sleep(Duration::from_millis(10));
        CGEventPost(K_CG_SESSION_EVENT_TAP, v_down);
        thread::sleep(Duration::from_millis(10));
        CGEventPost(K_CG_SESSION_EVENT_TAP, v_up);
        thread::sleep(Duration::from_millis(10));
        CGEventPost(K_CG_SESSION_EVENT_TAP, cmd_up);
        CFRelease(cmd_down as CfTypeRef);
        CFRelease(v_down as CfTypeRef);
        CFRelease(v_up as CfTypeRef);
        CFRelease(cmd_up as CfTypeRef);
        CFRelease(source as CfTypeRef);
        Ok(())
    }
}

fn snapshot_focused_field() -> FieldSnap {
    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return FieldSnap {
                role: None,
                value: None,
                selected: None,
            };
        }
        let focused_attr = cfstr("AXFocusedUIElement");
        if focused_attr.is_null() {
            CFRelease(system as CfTypeRef);
            return FieldSnap {
                role: None,
                value: None,
                selected: None,
            };
        }
        let mut focused: CfTypeRef = std::ptr::null();
        let err = AXUIElementCopyAttributeValue(system, focused_attr, &mut focused);
        CFRelease(system as CfTypeRef);
        CFRelease(focused_attr as CfTypeRef);
        if err != K_AX_ERROR_SUCCESS || focused.is_null() {
            return FieldSnap {
                role: None,
                value: None,
                selected: None,
            };
        }
        let el = focused as AxUiElementRef;
        let role = ax_attr_string(el, "AXRole");
        let value = ax_attr_string(el, "AXValue");
        let selected = ax_attr_string(el, "AXSelectedText");
        CFRelease(focused);
        FieldSnap {
            role,
            value,
            selected,
        }
    }
}

fn ax_attr_string(el: AxUiElementRef, name: &str) -> Option<String> {
    unsafe {
        let attr = cfstr(name);
        if attr.is_null() {
            return None;
        }
        let mut value: CfTypeRef = std::ptr::null();
        let err = AXUIElementCopyAttributeValue(el, attr, &mut value);
        CFRelease(attr as CfTypeRef);
        if err != K_AX_ERROR_SUCCESS || value.is_null() {
            return None;
        }
        let text = cfstring_to_rust(value);
        CFRelease(value);
        text
    }
}

pub fn read_selected_text() -> crate::Result<Option<String>> {
    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return Ok(None);
        }
        let focused_attr = cfstr("AXFocusedUIElement");
        let selected_attr = cfstr("AXSelectedText");
        if focused_attr.is_null() || selected_attr.is_null() {
            CFRelease(system as CfTypeRef);
            return Ok(None);
        }
        let mut focused: CfTypeRef = std::ptr::null();
        let err = AXUIElementCopyAttributeValue(system, focused_attr, &mut focused);
        CFRelease(system as CfTypeRef);
        CFRelease(focused_attr as CfTypeRef);
        if err != K_AX_ERROR_SUCCESS || focused.is_null() {
            CFRelease(selected_attr as CfTypeRef);
            return Ok(None);
        }
        let mut value: CfTypeRef = std::ptr::null();
        let get =
            AXUIElementCopyAttributeValue(focused as AxUiElementRef, selected_attr, &mut value);
        CFRelease(focused);
        CFRelease(selected_attr as CfTypeRef);
        if get != K_AX_ERROR_SUCCESS || value.is_null() {
            return Ok(None);
        }
        let text = cfstring_to_rust(value);
        CFRelease(value);
        Ok(text.filter(|s| !s.is_empty()))
    }
}

pub fn ax_replace_selected(text: &str) -> bool {
    ax_insert(text)
}

fn cfstring_to_rust(s: CfTypeRef) -> Option<String> {
    if s.is_null() {
        return None;
    }
    unsafe {
        let len = CFStringGetLength(s as CfStringRef);
        let max = CFStringGetMaximumSizeForEncoding(len, K_CF_STRING_ENCODING_UTF8) + 1;
        if max <= 1 || max > 1_000_000 {
            return None;
        }
        let mut buf = vec![0u8; max as usize];
        let ok = CFStringGetCString(
            s as CfStringRef,
            buf.as_mut_ptr() as *mut i8,
            max,
            K_CF_STRING_ENCODING_UTF8,
        );
        if !ok {
            return None;
        }
        let nul = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        String::from_utf8(buf[..nul].to_vec()).ok()
    }
}

fn ax_insert(text: &str) -> bool {
    unsafe {
        let system = AXUIElementCreateSystemWide();
        if system.is_null() {
            return false;
        }
        let focused_attr = cfstr("AXFocusedUIElement");
        let selected_attr = cfstr("AXSelectedText");
        if focused_attr.is_null() || selected_attr.is_null() {
            CFRelease(system as CfTypeRef);
            return false;
        }
        let mut focused: CfTypeRef = std::ptr::null();
        let err = AXUIElementCopyAttributeValue(system, focused_attr, &mut focused);
        CFRelease(system as CfTypeRef);
        CFRelease(focused_attr as CfTypeRef);
        if err != K_AX_ERROR_SUCCESS || focused.is_null() {
            CFRelease(selected_attr as CfTypeRef);
            return false;
        }
        let value = cfstr(text);
        if value.is_null() {
            CFRelease(focused);
            CFRelease(selected_attr as CfTypeRef);
            return false;
        }
        let set = AXUIElementSetAttributeValue(focused as AxUiElementRef, selected_attr, value);
        CFRelease(focused);
        CFRelease(selected_attr as CfTypeRef);
        CFRelease(value as CfTypeRef);
        set == K_AX_ERROR_SUCCESS
    }
}

fn cfstr(s: &str) -> CfStringRef {
    let c = std::ffi::CString::new(s).ok();
    let Some(c) = c else {
        return std::ptr::null();
    };
    unsafe { CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), K_CF_STRING_ENCODING_UTF8) }
}
