//! Live CGEvent tap. Compiled only on `target_os = "macos"`.
//!
//! Creates a HID-level tap, maps
//! events through [`crate::mapping`], feeds [`lailaisay_core::HotKeyProcessor`], and
//! swallows chords according to [`crate::should_swallow`].

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use lailaisay_core::{HotKey, HotKeyProcessor, KeyEvent};

use crate::cfptr::CfPtr;
use crate::mapping::{
    key_event_from_cg, CgEventKind, CG_EVENT_FLAGS_CHANGED, CG_EVENT_KEY_DOWN, CG_EVENT_KEY_UP,
};
use crate::tap::{process_tap_event, TapMessage};
use crate::{HotkeyMonitor, InputError, Result};

type CfTypeRef = *const c_void;
type CfMachPortRef = *mut c_void;
type CfRunLoopRef = *mut c_void;
type CfRunLoopSourceRef = *mut c_void;
type CfAllocatorRef = *const c_void;
type CfStringRef = *const c_void;
type CfDictionaryRef = *const c_void;
type CgEventRef = *mut c_void;
type CgEventTapProxy = *mut c_void;

const K_CF_ALLOCATOR_DEFAULT: CfAllocatorRef = std::ptr::null();
const K_CG_HID_EVENT_TAP: u32 = 0;
const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
const K_CG_EVENT_TAP_OPTION_DEFAULT: u32 = 0;
const K_CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;

#[link(name = "CoreGraphics", kind = "framework")]
#[link(name = "CoreFoundation", kind = "framework")]
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: extern "C" fn(CgEventTapProxy, u32, CgEventRef, *mut c_void) -> CgEventRef,
        user_info: *mut c_void,
    ) -> CfMachPortRef;
    fn CGEventTapEnable(tap: CfMachPortRef, enable: bool);
    fn CGEventGetIntegerValueField(event: CgEventRef, field: u32) -> i64;
    fn CGEventGetFlags(event: CgEventRef) -> u64;

    fn CFMachPortCreateRunLoopSource(
        allocator: CfAllocatorRef,
        port: CfMachPortRef,
        order: isize,
    ) -> CfRunLoopSourceRef;
    fn CFRunLoopGetCurrent() -> CfRunLoopRef;
    fn CFRunLoopAddSource(rl: CfRunLoopRef, source: CfRunLoopSourceRef, mode: CfStringRef);
    fn CFRunLoopRemoveSource(rl: CfRunLoopRef, source: CfRunLoopSourceRef, mode: CfStringRef);
    fn CFRunLoopRun();
    fn CFRunLoopStop(rl: CfRunLoopRef);
    static kCFRunLoopCommonModes: CfStringRef;

    fn CFRelease(cf: CfTypeRef);

    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CfDictionaryRef) -> bool;
}

static EVENT_TAP_LIVE: AtomicBool = AtomicBool::new(false);

/// True while this process has a running HID `CGEventTap` (Input Monitoring granted).
pub(crate) fn event_tap_is_live() -> bool {
    EVENT_TAP_LIVE.load(Ordering::Relaxed)
}

/// Check TCC without building a prompt dictionary (null-callback CFDictionary
/// + AX prompt has segfaulted on first launch).
pub fn accessibility_trusted(_prompt: bool) -> bool {
    unsafe {
        let simple = AXIsProcessTrusted();
        if simple {
            return true;
        }
        AXIsProcessTrustedWithOptions(std::ptr::null())
    }
}

struct TapInner {
    processor: Mutex<HotKeyProcessor>,
    edit_processor: Mutex<Option<HotKeyProcessor>>,
    tx: Mutex<Sender<TapMessage>>,
    /// `CGEventTap` port as [`CfPtr`] so `Arc<TapInner>` is `Send`+`Sync`.
    tap: Mutex<CfPtr>,
}

pub struct MacOsEventTap {
    rx: Receiver<TapMessage>,
    inner: Arc<TapInner>,
    run_loop: Arc<Mutex<Option<CfPtr>>>,
    thread: Option<JoinHandle<()>>,
}

impl MacOsEventTap {
    pub fn new() -> Result<Self> {
        Self::with_processor(HotKeyProcessor::new(HotKey::default(), false))
    }

    pub fn with_processor(processor: HotKeyProcessor) -> Result<Self> {
        if !accessibility_trusted(false) {
            return Err(InputError::Message(
                crate::permissions::ACCESSIBILITY_HELP.trim().into(),
            ));
        }

        let (tx, rx) = mpsc::channel();
        let inner = Arc::new(TapInner {
            processor: Mutex::new(processor),
            edit_processor: Mutex::new(None),
            tx: Mutex::new(tx),
            tap: Mutex::new(CfPtr::NULL),
        });
        let run_loop = Arc::new(Mutex::new(None));
        let thread_inner = inner.clone();
        let thread_rl = run_loop.clone();

        let thread = thread::Builder::new()
            .name("lailaisay-cgevent-tap".into())
            .spawn(move || unsafe { run_tap_thread(thread_inner, thread_rl) })
            .map_err(|e| InputError::Message(e.to_string()))?;

        // Give the tap a moment to fail fast if Accessibility was a lie.
        thread::sleep(Duration::from_millis(50));
        if thread.is_finished() {
            return Err(InputError::Message(
                "CGEvent tap thread exited immediately — check Accessibility / Input Monitoring"
                    .into(),
            ));
        }

        Ok(Self {
            rx,
            inner,
            run_loop,
            thread: Some(thread),
        })
    }

    pub fn set_hotkey(&self, hotkey: HotKey, use_double_tap_only: bool) {
        if let Ok(mut p) = self.inner.processor.lock() {
            p.hotkey = hotkey;
            p.use_double_tap_only = use_double_tap_only;
        }
    }

    pub fn set_edit_hotkey(&self, hotkey: HotKey) {
        if let Ok(mut slot) = self.inner.edit_processor.lock() {
            *slot = Some(HotKeyProcessor::new(hotkey, false));
        }
    }

    pub fn recv_message(&mut self) -> Result<Option<TapMessage>> {
        match self.rx.try_recv() {
            Ok(m) => Ok(Some(m)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => {
                Err(InputError::Message("CGEvent tap channel closed".into()))
            }
        }
    }

    pub fn recv_message_timeout(&mut self, timeout: Duration) -> Result<Option<TapMessage>> {
        match self.rx.recv_timeout(timeout) {
            Ok(m) => Ok(Some(m)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                Err(InputError::Message("CGEvent tap channel closed".into()))
            }
        }
    }
}

impl HotkeyMonitor for MacOsEventTap {
    fn next_event(&mut self) -> Result<Option<KeyEvent>> {
        Ok(self
            .recv_message_timeout(Duration::from_millis(250))?
            .map(|m| m.event))
    }
}

impl Drop for MacOsEventTap {
    fn drop(&mut self) {
        if let Ok(guard) = self.run_loop.lock() {
            if let Some(rl) = *guard {
                if !rl.is_null() {
                    unsafe { CFRunLoopStop(rl.as_ptr()) };
                }
            }
        }
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

extern "C" fn tap_callback(
    _proxy: CgEventTapProxy,
    etype: u32,
    event: CgEventRef,
    user_info: *mut c_void,
) -> CgEventRef {
    if CgEventKind::is_tap_disabled(etype) {
        // Timeout / user-input disable: re-enable so hold-to-talk keeps working.
        let inner = unsafe { &*(user_info as *const TapInner) };
        if let Ok(tap) = inner.tap.lock() {
            if !tap.is_null() {
                unsafe { CGEventTapEnable(tap.as_ptr(), true) };
            }
        }
        return event;
    }

    let inner = unsafe { &*(user_info as *const TapInner) };
    let kind = CgEventKind::from_type(etype);
    let keycode = unsafe { CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) as u16 };
    let flags = unsafe { CGEventGetFlags(event) };
    let key_event = key_event_from_cg(kind, keycode, flags);

    let (swallowed, msg) = {
        let Ok(mut processor) = inner.processor.lock() else {
            return event;
        };
        let Ok(mut edit) = inner.edit_processor.lock() else {
            return event;
        };
        processor.stamp_wall_clock();
        if let Some(p) = edit.as_mut() {
            p.stamp_wall_clock();
        }
        let msg = process_tap_event(&mut processor, &mut edit, key_event);
        (msg.swallowed, msg)
    };

    if let Ok(tx) = inner.tx.lock() {
        let _ = tx.send(msg);
    }

    if swallowed {
        std::ptr::null_mut()
    } else {
        event
    }
}

unsafe fn run_tap_thread(inner: Arc<TapInner>, run_loop_slot: Arc<Mutex<Option<CfPtr>>>) {
    let mask: u64 =
        (1u64 << CG_EVENT_KEY_DOWN) | (1u64 << CG_EVENT_KEY_UP) | (1u64 << CG_EVENT_FLAGS_CHANGED);

    let user_info = Arc::into_raw(inner) as *mut c_void;
    let tap = CGEventTapCreate(
        K_CG_HID_EVENT_TAP,
        K_CG_HEAD_INSERT_EVENT_TAP,
        K_CG_EVENT_TAP_OPTION_DEFAULT,
        mask,
        tap_callback,
        user_info,
    );
    if tap.is_null() {
        let _ = unsafe { Arc::from_raw(user_info as *const TapInner) };
        eprintln!("[lailaisay-input] CGEventTapCreate failed (Accessibility / Input Monitoring?)");
        return;
    }
    {
        let inner = unsafe { &*(user_info as *const TapInner) };
        if let Ok(mut slot) = inner.tap.lock() {
            *slot = CfPtr::from_ptr(tap);
        }
    }

    let source = CFMachPortCreateRunLoopSource(K_CF_ALLOCATOR_DEFAULT, tap, 0);
    if source.is_null() {
        CFRelease(tap as CfTypeRef);
        let _ = unsafe { Arc::from_raw(user_info as *const TapInner) };
        eprintln!("[lailaisay-input] CFMachPortCreateRunLoopSource failed");
        return;
    }

    let rl = CFRunLoopGetCurrent();
    if rl.is_null() || kCFRunLoopCommonModes.is_null() {
        CFRelease(source as CfTypeRef);
        CFRelease(tap as CfTypeRef);
        let _ = unsafe { Arc::from_raw(user_info as *const TapInner) };
        eprintln!("[lailaisay-input] CFRunLoop is not available");
        return;
    }
    if let Ok(mut slot) = run_loop_slot.lock() {
        *slot = Some(CfPtr::from_ptr(rl));
    }
    CFRunLoopAddSource(rl, source, kCFRunLoopCommonModes);
    CGEventTapEnable(tap, true);
    EVENT_TAP_LIVE.store(true, Ordering::Relaxed);
    CFRunLoopRun();
    EVENT_TAP_LIVE.store(false, Ordering::Relaxed);

    CFRunLoopRemoveSource(rl, source, kCFRunLoopCommonModes);
    CGEventTapEnable(tap, false);
    CFRelease(source as CfTypeRef);
    CFRelease(tap as CfTypeRef);
    let _ = unsafe { Arc::from_raw(user_info as *const TapInner) };
}

#[cfg(test)]
mod tests {
    // Runtime tap tests need a Mac + Accessibility. Mapping/swallow live in
    // sibling modules and run on Linux CI.

    #[test]
    fn tap_inner_and_arcs_are_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<super::TapInner>();
        assert_sync::<super::TapInner>();
        assert_send::<std::sync::Arc<super::TapInner>>();
        assert_sync::<std::sync::Arc<super::TapInner>>();
        assert_send::<std::sync::Arc<std::sync::Mutex<Option<crate::cfptr::CfPtr>>>>();
    }
}
