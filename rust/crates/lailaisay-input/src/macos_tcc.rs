//! Non-prompting macOS permission probes for the Settings 權限 pane.
//!
//! Compiled only on `target_os = "macos"`. Never call request/prompt APIs here.

use std::ffi::c_void;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use objc::runtime::{Class, Object};
use objc::{msg_send, sel, sel_impl};

use crate::permissions::{
    map_automation_status, map_av_authorization_status, map_avaudio_record_permission,
    microphone_session_opened, resolve_input_monitoring_grant, resolve_microphone_grant,
    GrantStatus,
};

/// `AVMediaTypeAudio` value. Do not link the `AVMediaTypeAudio` extern static
/// — it has been null / wrong in this process and mapped as NotDetermined.
const AV_MEDIA_TYPE_AUDIO: &std::ffi::CStr = c"soun";

const K_IOHID_REQUEST_TYPE_LISTEN_EVENT: u32 = 1;
const K_IOHID_ACCESS_GRANTED: u32 = 0;
const K_IOHID_ACCESS_DENIED: u32 = 1;
const K_IOHID_ACCESS_UNKNOWN: u32 = 2;

const TYPE_APPLICATION_BUNDLE_ID: u32 = u32::from_be_bytes(*b"bund");
const TYPE_WILDCARD: u32 = u32::from_be_bytes(*b"****");
const AUTOMATION_CACHE_TTL: Duration = Duration::from_millis(400);

// AEDataModel.h uses #pragma pack(push, 2), including on 64-bit macOS.
#[repr(C, packed(2))]
struct AEDesc {
    descriptor_type: u32,
    data_handle: *mut c_void,
}

#[link(name = "Foundation", kind = "framework")]
#[link(name = "AVFoundation", kind = "framework")]
#[link(name = "IOKit", kind = "framework")]
#[link(name = "CoreGraphics", kind = "framework")]
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn IOHIDCheckAccess(request_type: u32) -> u32;
    fn CGPreflightListenEventAccess() -> bool;
    fn AECreateDesc(
        type_code: u32,
        data_ptr: *const c_void,
        data_size: isize,
        result: *mut AEDesc,
    ) -> i16;
    fn AEDisposeDesc(desc: *mut AEDesc) -> i16;
    fn AEDeterminePermissionToAutomateTarget(
        target: *const AEDesc,
        event_class: u32,
        event_id: u32,
        ask_user_if_needed: u8,
    ) -> i32;
}

/// Status only — never `requestAccess` / `requestRecordPermission`.
///
/// Order: successful cpal open this session, then `AVAudioApplication`
/// (macOS 14+), then `AVCaptureDevice` with an NSString `"soun"` media type.
pub fn microphone_grant() -> GrantStatus {
    resolve_microphone_grant(microphone_session_opened(), probe_microphone_tcc())
}

fn probe_microphone_tcc() -> GrantStatus {
    let primary = avaudio_record_permission();
    if let Some(status @ (GrantStatus::Granted | GrantStatus::Denied)) = primary {
        return status;
    }
    let fallback = avcapture_audio_status();
    if fallback == GrantStatus::Unknown {
        primary.unwrap_or(GrantStatus::Unknown)
    } else {
        fallback
    }
}

/// `AVAudioApplication.sharedInstance.recordPermission` — no prompt.
fn avaudio_record_permission() -> Option<GrantStatus> {
    unsafe {
        let cls = Class::get("AVAudioApplication")?;
        let app: *mut Object = msg_send![cls, sharedInstance];
        if app.is_null() {
            return None;
        }
        let perm: i64 = msg_send![app, recordPermission];
        Some(map_avaudio_record_permission(perm))
    }
}

/// `AVCaptureDevice authorizationStatusForMediaType:` with NSString `"soun"`.
fn avcapture_audio_status() -> GrantStatus {
    unsafe {
        let Some(ns) = Class::get("NSString") else {
            return GrantStatus::Unknown;
        };
        let media: *mut Object = msg_send![ns, stringWithUTF8String: AV_MEDIA_TYPE_AUDIO.as_ptr()];
        if media.is_null() {
            return GrantStatus::Unknown;
        }
        let Some(cls) = Class::get("AVCaptureDevice") else {
            return GrantStatus::Unknown;
        };
        let status: i64 = msg_send![cls, authorizationStatusForMediaType: media];
        map_av_authorization_status(status)
    }
}

/// Input Monitoring: live tap, then IOHID, then `CGPreflightListenEventAccess`.
/// There is no AX-style public API; if none of these resolve, stay Unknown.
pub fn input_monitoring_grant() -> GrantStatus {
    resolve_input_monitoring_grant(
        crate::macos::event_tap_is_live(),
        iohid_listen_access(),
        cg_preflight_listen(),
    )
}

fn iohid_listen_access() -> GrantStatus {
    let code = unsafe { IOHIDCheckAccess(K_IOHID_REQUEST_TYPE_LISTEN_EVENT) };
    match code {
        K_IOHID_ACCESS_GRANTED => GrantStatus::Granted,
        K_IOHID_ACCESS_DENIED => GrantStatus::Denied,
        K_IOHID_ACCESS_UNKNOWN => GrantStatus::Unknown,
        _ => GrantStatus::Unknown,
    }
}

fn cg_preflight_listen() -> Option<bool> {
    Some(unsafe { CGPreflightListenEventAccess() })
}

static AUTOMATION_CACHE: Mutex<Option<(Instant, GrantStatus)>> = Mutex::new(None);

/// `AEDeterminePermissionToAutomateTarget` for System Events, `askUserIfNeeded=false`.
/// No TCC-db fallback. Distinguishes consent pending, target absent, and failure.
pub fn automation_grant() -> GrantStatus {
    if let Ok(guard) = AUTOMATION_CACHE.lock() {
        if let Some((at, status)) = *guard {
            if at.elapsed() < AUTOMATION_CACHE_TTL {
                return status;
            }
        }
    }
    let status = automation_grant_uncached();
    if let Ok(mut guard) = AUTOMATION_CACHE.lock() {
        *guard = Some((Instant::now(), status));
    }
    status
}

fn automation_grant_uncached() -> GrantStatus {
    let bundle = b"com.apple.systemevents";
    let mut desc = AEDesc {
        descriptor_type: 0,
        data_handle: std::ptr::null_mut(),
    };
    let created = unsafe {
        AECreateDesc(
            TYPE_APPLICATION_BUNDLE_ID,
            bundle.as_ptr() as *const c_void,
            bundle.len() as isize,
            &mut desc,
        )
    };
    if created != 0 {
        return GrantStatus::Unknown;
    }
    let status =
        unsafe { AEDeterminePermissionToAutomateTarget(&desc, TYPE_WILDCARD, TYPE_WILDCARD, 0) };
    unsafe {
        let _ = AEDisposeDesc(&mut desc);
    }
    map_automation_status(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apple_event_descriptor_matches_sdk_abi() {
        assert_eq!(std::mem::align_of::<AEDesc>(), 2);
        assert_eq!(std::mem::offset_of!(AEDesc, data_handle), 4);
        assert_eq!(
            std::mem::size_of::<AEDesc>(),
            4 + std::mem::size_of::<*mut c_void>()
        );
    }
}
