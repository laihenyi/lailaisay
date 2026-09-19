//! Accessibility / Input Monitoring / Microphone / Automation probes.
//!
//! Help strings are shared with the app and README. Grant checks never prompt.
//! Recording is cpal `LiveRecorder`, not `AVCaptureSession`.

/// User-facing text shown when the CGEvent tap cannot be created.
pub const ACCESSIBILITY_HELP: &str = "\
lailaisay needs Accessibility (and on some macOS versions, Input Monitoring) \
to see a global press-and-hold hotkey.

System Settings → Privacy & Security → Accessibility → enable **lailaisay.app** \
(bundle id com.yikai.lailaisay) when you launched the packaged app. \
If you still `cargo run`, enable the terminal (Terminal / iTerm / Cursor) \
and lailaisay-app if it appears.

Also check: Privacy & Security → Input Monitoring.

macOS requires this permission for the global CGEvent tap. \
No API keys are involved.
";

/// Microphone TCC (used by lailaisay-app when the `mic` feature is on).
pub const MICROPHONE_HELP: &str = "\
Recording needs Microphone permission: \
System Settings → Privacy & Security → Microphone → enable **lailaisay.app** \
(or the terminal you launched `cargo run` from).
";

/// Shown when `SetWindowsHookExW(WH_KEYBOARD_LL)` fails, and in WINDOWS.md.
pub const WINDOWS_HOTKEY_HELP: &str = "\
lailaisay uses a low-level keyboard hook (WH_KEYBOARD_LL / SetWindowsHookExW) \
for press-and-hold. Hold the configured hotkey to start; release to stop.

Default chord is ⌘⇧Space — on Windows that is **Win+Shift+Space** \
(⌘ is the Windows key). Change it in Settings to Ctrl+Shift+Space if you \
prefer. Speak-to-Edit is Alt+Shift+Space (⌥⇧Space).

Windows does not prompt like macOS Accessibility. If the hook fails to \
install, allow lailaisay / the terminal in antivirus or ransomware protection, \
then relaunch. Some enterprise policies block WH_KEYBOARD_LL.

Use `lailaisay-app --no-tap` to open Settings without a hook, or \
`lailaisay-app --once` / `lailaisay-cli process` for a file/text smoke. No API keys.
";

/// Microphone privacy (Windows Settings).
pub const WINDOWS_MICROPHONE_HELP: &str = "\
Recording needs Microphone access: \
Settings → Privacy & security → Microphone → allow **lailaisay** \
(or the terminal you launched `cargo run` from).
";

/// Live TCC / privacy grant as shown on the Settings 權限 pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantStatus {
    Granted,
    Denied,
    /// The OS has not obtained a consent decision yet.
    NotDetermined,
    /// AppleEvents requires a running target to probe its permission.
    TargetNotRunning,
    Unknown,
}

impl GrantStatus {
    pub fn zh_label(self) -> &'static str {
        match self {
            Self::Granted => "已開啟",
            Self::Denied => "未開啟",
            Self::NotDetermined => "尚未詢問",
            Self::TargetNotRunning => "待目標啟動",
            Self::Unknown => "偵測失敗",
        }
    }

    pub fn from_trusted(trusted: bool) -> Self {
        if trusted {
            Self::Granted
        } else {
            Self::Denied
        }
    }
}

/// `AVAuthorizationStatus` (NotDetermined=0 … Authorized=3).
/// Keep an undecided consent state distinct from a failed probe.
pub fn map_av_authorization_status(status: i64) -> GrantStatus {
    match status {
        3 => GrantStatus::Granted,
        2 | 1 => GrantStatus::Denied,
        0 => GrantStatus::NotDetermined,
        _ => GrantStatus::Unknown,
    }
}

/// `AVAudioApplicationRecordPermission` FourCC (`undt` / `deny` / `grnt`).
pub fn map_avaudio_record_permission(status: i64) -> GrantStatus {
    const UNDT: i64 = u32::from_be_bytes(*b"undt") as i64;
    const DENY: i64 = u32::from_be_bytes(*b"deny") as i64;
    const GRNT: i64 = u32::from_be_bytes(*b"grnt") as i64;
    match status {
        GRNT => GrantStatus::Granted,
        DENY => GrantStatus::Denied,
        UNDT => GrantStatus::NotDetermined,
        other => map_av_authorization_status(other),
    }
}

/// Map non-prompting AppleEvents results without treating lifecycle/consent as failure.
pub fn map_automation_status(status: i32) -> GrantStatus {
    match status {
        0 => GrantStatus::Granted,
        -1743 => GrantStatus::Denied,
        -1744 => GrantStatus::NotDetermined,
        -600 => GrantStatus::TargetNotRunning,
        _ => GrantStatus::Unknown,
    }
}

/// Recording evidence can resolve an unavailable probe, but cannot override revocation.
pub fn resolve_microphone_grant(session_opened: bool, probed: GrantStatus) -> GrantStatus {
    if session_opened && matches!(probed, GrantStatus::Unknown | GrantStatus::NotDetermined) {
        GrantStatus::Granted
    } else {
        probed
    }
}

/// Live HID tap wins over a flaky `AXIsProcessTrusted` false-negative.
pub fn resolve_accessibility_grant(tap_live: bool, ax_trusted: bool) -> GrantStatus {
    if tap_live {
        GrantStatus::Granted
    } else {
        GrantStatus::from_trusted(ax_trusted)
    }
}

/// Live tap, then IOHID, then `CGPreflightListenEventAccess`.
pub fn resolve_input_monitoring_grant(
    tap_live: bool,
    iohid: GrantStatus,
    cg_listen: Option<bool>,
) -> GrantStatus {
    if tap_live {
        return GrantStatus::Granted;
    }
    match iohid {
        GrantStatus::Granted | GrantStatus::Denied => return iohid,
        GrantStatus::Unknown | GrantStatus::NotDetermined | GrantStatus::TargetNotRunning => {}
    }
    match cg_listen {
        Some(true) => GrantStatus::Granted,
        Some(false) => GrantStatus::Denied,
        None => GrantStatus::Unknown,
    }
}

static MIC_OPENED_THIS_SESSION: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Call after `LiveRecorder::start` succeeds. Sticky for the process.
pub fn note_microphone_opened() {
    MIC_OPENED_THIS_SESSION.store(true, std::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn microphone_session_opened() -> bool {
    MIC_OPENED_THIS_SESSION.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(target_os = "macos")]
pub fn accessibility_trusted(prompt: bool) -> bool {
    crate::macos::accessibility_trusted(prompt)
}

#[cfg(not(target_os = "macos"))]
pub fn accessibility_trusted(_prompt: bool) -> bool {
    false
}

pub fn accessibility_grant() -> GrantStatus {
    #[cfg(target_os = "macos")]
    {
        resolve_accessibility_grant(
            crate::macos::event_tap_is_live(),
            crate::macos::accessibility_trusted(false),
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        GrantStatus::from_trusted(false)
    }
}

pub fn microphone_grant() -> GrantStatus {
    #[cfg(target_os = "macos")]
    {
        crate::macos_tcc::microphone_grant()
    }
    #[cfg(not(target_os = "macos"))]
    {
        GrantStatus::Unknown
    }
}

pub fn input_monitoring_grant() -> GrantStatus {
    #[cfg(target_os = "macos")]
    {
        crate::macos_tcc::input_monitoring_grant()
    }
    #[cfg(not(target_os = "macos"))]
    {
        GrantStatus::Unknown
    }
}

pub fn automation_grant() -> GrantStatus {
    #[cfg(target_os = "macos")]
    {
        crate::macos_tcc::automation_grant()
    }
    #[cfg(not(target_os = "macos"))]
    {
        GrantStatus::Unknown
    }
}

pub fn is_macos() -> bool {
    cfg!(target_os = "macos")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automation_consent_lifecycle_and_errors_are_distinct() {
        assert_eq!(map_automation_status(0), GrantStatus::Granted);
        assert_eq!(map_automation_status(-1743), GrantStatus::Denied);
        assert_eq!(map_automation_status(-1744), GrantStatus::NotDetermined);
        assert_eq!(map_automation_status(-600), GrantStatus::TargetNotRunning);
        assert_eq!(map_automation_status(-50), GrantStatus::Unknown);
        assert_eq!(GrantStatus::NotDetermined.zh_label(), "尚未詢問");
        assert_eq!(GrantStatus::TargetNotRunning.zh_label(), "待目標啟動");
    }

    #[test]
    fn grant_status_zh_labels() {
        assert_eq!(GrantStatus::Granted.zh_label(), "已開啟");
        assert_eq!(GrantStatus::Denied.zh_label(), "未開啟");
        assert_eq!(GrantStatus::Unknown.zh_label(), "偵測失敗");
        assert_eq!(GrantStatus::from_trusted(true), GrantStatus::Granted);
        assert_eq!(GrantStatus::from_trusted(false), GrantStatus::Denied);
    }

    #[test]
    fn av_not_determined_is_unknown_not_denied() {
        assert_eq!(map_av_authorization_status(0), GrantStatus::NotDetermined);
        assert_ne!(map_av_authorization_status(0), GrantStatus::Denied);
        assert_eq!(map_av_authorization_status(1), GrantStatus::Denied);
        assert_eq!(map_av_authorization_status(2), GrantStatus::Denied);
        assert_eq!(map_av_authorization_status(3), GrantStatus::Granted);
        assert_eq!(map_av_authorization_status(99), GrantStatus::Unknown);
    }

    #[test]
    fn avaudio_record_permission_fourcc() {
        let undt = u32::from_be_bytes(*b"undt") as i64;
        let deny = u32::from_be_bytes(*b"deny") as i64;
        let grnt = u32::from_be_bytes(*b"grnt") as i64;
        assert_eq!(
            map_avaudio_record_permission(undt),
            GrantStatus::NotDetermined
        );
        assert_ne!(map_avaudio_record_permission(undt), GrantStatus::Denied);
        assert_eq!(map_avaudio_record_permission(deny), GrantStatus::Denied);
        assert_eq!(map_avaudio_record_permission(grnt), GrantStatus::Granted);
    }

    #[test]
    fn cpal_session_open_makes_mic_granted() {
        assert_eq!(
            resolve_microphone_grant(true, GrantStatus::Unknown),
            GrantStatus::Granted
        );
        assert_eq!(
            resolve_microphone_grant(true, GrantStatus::Denied),
            GrantStatus::Denied
        );
        assert_eq!(
            resolve_microphone_grant(false, GrantStatus::Denied),
            GrantStatus::Denied
        );
        assert_eq!(
            resolve_microphone_grant(false, GrantStatus::Unknown),
            GrantStatus::Unknown
        );
    }

    #[test]
    fn tap_live_prefers_accessibility_granted() {
        assert_eq!(
            resolve_accessibility_grant(true, false),
            GrantStatus::Granted
        );
        assert_eq!(
            resolve_accessibility_grant(false, true),
            GrantStatus::Granted
        );
        assert_eq!(
            resolve_accessibility_grant(false, false),
            GrantStatus::Denied
        );
    }

    #[test]
    fn tap_live_prefers_input_monitoring_granted() {
        assert_eq!(
            resolve_input_monitoring_grant(true, GrantStatus::Denied, Some(false)),
            GrantStatus::Granted
        );
        assert_eq!(
            resolve_input_monitoring_grant(false, GrantStatus::Granted, Some(false)),
            GrantStatus::Granted
        );
        assert_eq!(
            resolve_input_monitoring_grant(false, GrantStatus::Unknown, Some(false)),
            GrantStatus::Denied
        );
        assert_eq!(
            resolve_input_monitoring_grant(false, GrantStatus::Unknown, None),
            GrantStatus::Unknown
        );
    }

    #[test]
    fn non_macos_stubs_are_not_granted() {
        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(accessibility_grant(), GrantStatus::Denied);
            assert_eq!(microphone_grant(), GrantStatus::Unknown);
            assert_eq!(input_monitoring_grant(), GrantStatus::Unknown);
            assert_eq!(automation_grant(), GrantStatus::Unknown);
            assert!(!accessibility_trusted(false));
        }
        #[cfg(target_os = "macos")]
        {
            let _ = accessibility_grant();
            let _ = microphone_grant();
            let _ = input_monitoring_grant();
            let _ = automation_grant();
        }
    }
}
