//! Sandbox-safe global hotkey via Carbon `RegisterEventHotKey`.
//!
//! The Mac App Store build cannot install a `CGEventTap` (App Sandbox), so
//! it registers the chord with the system instead. Carbon delivers
//! `kEventHotKeyPressed` / `kEventHotKeyReleased` to the application event
//! target on the **main thread** — the eframe / NSApplication run loop pumps
//! them. No Accessibility or Input Monitoring prompt is needed.
//!
//! Limits versus the tap:
//! - the chord must contain a key (modifier-only and fn-only chords are rejected);
//! - the system never reports other keys, so Escape does not cancel;
//! - matching events are consumed by the system, never seen by other apps.
//!
//! Register / unregister must run on the main thread (the host calls
//! [`MacOsCarbonHotkey::with_processor`] and `set_hotkey` from the egui update).

use std::ffi::c_void;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lailaisay_core::{HotKey, HotKeyProcessor, KeyEvent, Modifier, Modifiers};

use crate::cfptr::CfPtr;
use crate::mapping::key_to_keycode;
use crate::tap::{process_tap_event, TapMessage};
use crate::{HotkeyMonitor, InputError, Result};

type EventTargetRef = *mut c_void;
type EventHandlerRef = *mut c_void;
type EventHandlerCallRef = *mut c_void;
type EventRef = *mut c_void;
type EventHotKeyRef = *mut c_void;
type OsStatus = i32;

#[repr(C)]
struct EventTypeSpec {
    event_class: u32,
    event_kind: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct EventHotKeyId {
    signature: u32,
    id: u32,
}

const K_EVENT_CLASS_KEYBOARD: u32 = 0x6B65_7962; // 'keyb'
const K_EVENT_HOT_KEY_PRESSED: u32 = 5;
const K_EVENT_HOT_KEY_RELEASED: u32 = 6;
const K_EVENT_PARAM_DIRECT_OBJECT: u32 = 0x2D2D_2D2D; // '----'
const TYPE_EVENT_HOT_KEY_ID: u32 = 0x686B_6964; // 'hkid'
const HOTKEY_SIGNATURE: u32 = 0x4C4C_5341; // 'LLSA'
const ID_DICTATE: u32 = 1;
const ID_EDIT: u32 = 2;

const CMD_KEY: u32 = 1 << 8;
const SHIFT_KEY: u32 = 1 << 9;
const OPTION_KEY: u32 = 1 << 11;
const CONTROL_KEY: u32 = 1 << 12;

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn GetApplicationEventTarget() -> EventTargetRef;
    fn InstallEventHandler(
        target: EventTargetRef,
        handler: extern "C" fn(EventHandlerCallRef, EventRef, *mut c_void) -> OsStatus,
        num_types: u32,
        list: *const EventTypeSpec,
        user_data: *mut c_void,
        out_ref: *mut EventHandlerRef,
    ) -> OsStatus;
    fn RemoveEventHandler(handler: EventHandlerRef) -> OsStatus;
    fn RegisterEventHotKey(
        code: u32,
        modifiers: u32,
        id: EventHotKeyId,
        target: EventTargetRef,
        options: u32,
        out: *mut EventHotKeyRef,
    ) -> OsStatus;
    fn UnregisterEventHotKey(hotkey: EventHotKeyRef) -> OsStatus;
    fn GetEventKind(event: EventRef) -> u32;
    fn GetEventParameter(
        event: EventRef,
        name: u32,
        desired_type: u32,
        actual_type: *mut u32,
        size: u32,
        actual_size: *mut u32,
        out: *mut c_void,
    ) -> OsStatus;
}

/// Carbon keycode + modifier mask for a chord, or why it cannot be registered.
pub fn carbon_chord(hotkey: &HotKey) -> std::result::Result<(u32, u32), String> {
    let key = hotkey
        .key
        .as_ref()
        .ok_or_else(|| "App Store 版熱鍵需要包含一個按鍵（例如 ⌘⇧Space），不支援只有修飾鍵".to_string())?;
    let code = key_to_keycode(key)
        .ok_or_else(|| format!("按鍵 {} 沒有對應的 macOS 鍵碼", key.as_str()))?;
    let mut mask = 0u32;
    for m in &hotkey.modifiers.modifiers {
        mask |= match m {
            Modifier::Command => CMD_KEY,
            Modifier::Shift => SHIFT_KEY,
            Modifier::Option => OPTION_KEY,
            Modifier::Control => CONTROL_KEY,
            Modifier::Fn => {
                return Err("App Store 版熱鍵不支援 fn 鍵，請改用 ⌘ / ⌥ / ⇧ / ⌃ 組合".into())
            }
        };
    }
    if mask == 0 {
        return Err("App Store 版熱鍵至少需要一個修飾鍵（⌘ / ⌥ / ⇧ / ⌃）".into());
    }
    Ok((u32::from(code), mask))
}

/// True when [`carbon_chord`] can register this chord.
pub fn carbon_hotkey_supported(hotkey: &HotKey) -> bool {
    carbon_chord(hotkey).is_ok()
}

struct Slot {
    hotkey: HotKey,
    handle: CfPtr,
}

struct Inner {
    processor: Mutex<HotKeyProcessor>,
    edit_processor: Mutex<Option<HotKeyProcessor>>,
    tx: Mutex<Sender<TapMessage>>,
    dictate: Mutex<Option<Slot>>,
    edit: Mutex<Option<Slot>>,
}

pub struct MacOsCarbonHotkey {
    rx: Receiver<TapMessage>,
    inner: Arc<Inner>,
    handler: CfPtr,
    last_error: Option<String>,
}

impl MacOsCarbonHotkey {
    pub fn new() -> Result<Self> {
        Self::with_processor(HotKeyProcessor::new(HotKey::default(), false))
    }

    /// Install the Carbon handler. Registering the chords happens in
    /// [`set_hotkey`](Self::set_hotkey) / [`set_edit_hotkey`](Self::set_edit_hotkey).
    pub fn with_processor(processor: HotKeyProcessor) -> Result<Self> {
        let (tx, rx) = mpsc::channel();
        let inner = Arc::new(Inner {
            processor: Mutex::new(processor),
            edit_processor: Mutex::new(None),
            tx: Mutex::new(tx),
            dictate: Mutex::new(None),
            edit: Mutex::new(None),
        });
        let specs = [
            EventTypeSpec {
                event_class: K_EVENT_CLASS_KEYBOARD,
                event_kind: K_EVENT_HOT_KEY_PRESSED,
            },
            EventTypeSpec {
                event_class: K_EVENT_CLASS_KEYBOARD,
                event_kind: K_EVENT_HOT_KEY_RELEASED,
            },
        ];
        let user_data = Arc::into_raw(inner.clone()) as *mut c_void;
        let mut handler: EventHandlerRef = std::ptr::null_mut();
        let status = unsafe {
            InstallEventHandler(
                GetApplicationEventTarget(),
                hotkey_handler,
                specs.len() as u32,
                specs.as_ptr(),
                user_data,
                &mut handler,
            )
        };
        if status != 0 || handler.is_null() {
            // Reclaim the Arc handed to Carbon.
            let _ = unsafe { Arc::from_raw(user_data as *const Inner) };
            return Err(InputError::Message(format!(
                "InstallEventHandler failed (OSStatus {status})"
            )));
        }
        Ok(Self {
            rx,
            inner,
            handler: CfPtr::from_ptr(handler),
            last_error: None,
        })
    }

    /// Why the last `set_hotkey` / `set_edit_hotkey` could not register, if any.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn set_hotkey(&mut self, hotkey: HotKey, use_double_tap_only: bool) {
        if let Ok(mut p) = self.inner.processor.lock() {
            p.hotkey = hotkey.clone();
            p.use_double_tap_only = use_double_tap_only;
        }
        let result = register(&self.inner.dictate, hotkey, ID_DICTATE);
        self.last_error = result.err();
        if let Some(e) = &self.last_error {
            eprintln!("[lailaisay-input] Carbon hotkey not registered: {e}");
        }
    }

    pub fn set_edit_hotkey(&mut self, hotkey: HotKey) {
        if let Ok(mut slot) = self.inner.edit_processor.lock() {
            *slot = Some(HotKeyProcessor::new(hotkey.clone(), false));
        }
        if let Err(e) = register(&self.inner.edit, hotkey, ID_EDIT) {
            eprintln!("[lailaisay-input] Carbon edit hotkey not registered: {e}");
        }
    }

    pub fn recv_message(&mut self) -> Result<Option<TapMessage>> {
        match self.rx.try_recv() {
            Ok(m) => Ok(Some(m)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => {
                Err(InputError::Message("Carbon hotkey channel closed".into()))
            }
        }
    }

    pub fn recv_message_timeout(&mut self, timeout: Duration) -> Result<Option<TapMessage>> {
        match self.rx.recv_timeout(timeout) {
            Ok(m) => Ok(Some(m)),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err(InputError::Message("Carbon hotkey channel closed".into()))
            }
        }
    }
}

impl HotkeyMonitor for MacOsCarbonHotkey {
    fn next_event(&mut self) -> Result<Option<KeyEvent>> {
        Ok(self
            .recv_message_timeout(Duration::from_millis(250))?
            .map(|m| m.event))
    }
}

impl Drop for MacOsCarbonHotkey {
    fn drop(&mut self) {
        unregister(&self.inner.dictate);
        unregister(&self.inner.edit);
        if !self.handler.is_null() {
            unsafe {
                RemoveEventHandler(self.handler.as_ptr());
            }
            // Balance the Arc::into_raw in with_processor.
            let raw = Arc::as_ptr(&self.inner);
            let _ = unsafe { Arc::from_raw(raw) };
        }
    }
}

fn unregister(slot: &Mutex<Option<Slot>>) {
    if let Ok(mut guard) = slot.lock() {
        if let Some(old) = guard.take() {
            if !old.handle.is_null() {
                unsafe {
                    UnregisterEventHotKey(old.handle.as_ptr());
                }
            }
        }
    }
}

fn register(
    slot: &Mutex<Option<Slot>>,
    hotkey: HotKey,
    id: u32,
) -> std::result::Result<(), String> {
    unregister(slot);
    let (code, mask) = carbon_chord(&hotkey)?;
    let mut handle: EventHotKeyRef = std::ptr::null_mut();
    let status = unsafe {
        RegisterEventHotKey(
            code,
            mask,
            EventHotKeyId {
                signature: HOTKEY_SIGNATURE,
                id,
            },
            GetApplicationEventTarget(),
            0,
            &mut handle,
        )
    };
    if status != 0 || handle.is_null() {
        return Err(format!(
            "RegisterEventHotKey({hotkey}) failed (OSStatus {status}); 這組熱鍵可能已被其他 App 佔用"
        ));
    }
    if let Ok(mut guard) = slot.lock() {
        *guard = Some(Slot {
            hotkey,
            handle: CfPtr::from_ptr(handle),
        });
    }
    Ok(())
}

/// Key events the tap would have produced for one Carbon press / release.
///
/// Press: the full chord. Release: chord minus the key (what the tap sees on
/// `keyUp`), then everything released (`flagsChanged` to no modifiers) so the
/// processor's dirty / double-tap bookkeeping settles.
pub fn synthesize_events(hotkey: &HotKey, pressed: bool) -> Vec<KeyEvent> {
    if pressed {
        vec![KeyEvent {
            key: hotkey.key.clone(),
            modifiers: hotkey.modifiers.clone(),
        }]
    } else {
        vec![
            KeyEvent {
                key: None,
                modifiers: hotkey.modifiers.clone(),
            },
            KeyEvent {
                key: None,
                modifiers: Modifiers::default(),
            },
        ]
    }
}

extern "C" fn hotkey_handler(
    _call: EventHandlerCallRef,
    event: EventRef,
    user_data: *mut c_void,
) -> OsStatus {
    let inner = unsafe { &*(user_data as *const Inner) };
    let kind = unsafe { GetEventKind(event) };
    let pressed = match kind {
        K_EVENT_HOT_KEY_PRESSED => true,
        K_EVENT_HOT_KEY_RELEASED => false,
        _ => return 0,
    };
    let mut id = EventHotKeyId::default();
    let status = unsafe {
        GetEventParameter(
            event,
            K_EVENT_PARAM_DIRECT_OBJECT,
            TYPE_EVENT_HOT_KEY_ID,
            std::ptr::null_mut(),
            std::mem::size_of::<EventHotKeyId>() as u32,
            std::ptr::null_mut(),
            &mut id as *mut EventHotKeyId as *mut c_void,
        )
    };
    if status != 0 || id.signature != HOTKEY_SIGNATURE {
        return 0;
    }
    let slot = match id.id {
        ID_DICTATE => &inner.dictate,
        ID_EDIT => &inner.edit,
        _ => return 0,
    };
    let hotkey = match slot.lock() {
        Ok(guard) => match guard.as_ref() {
            Some(s) => s.hotkey.clone(),
            None => return 0,
        },
        Err(_) => return 0,
    };

    let (Ok(mut processor), Ok(mut edit)) =
        (inner.processor.lock(), inner.edit_processor.lock())
    else {
        return 0;
    };
    for key_event in synthesize_events(&hotkey, pressed) {
        processor.stamp_wall_clock();
        if let Some(p) = edit.as_mut() {
            p.stamp_wall_clock();
        }
        let msg = process_tap_event(&mut processor, &mut edit, key_event);
        if let Ok(tx) = inner.tx.lock() {
            let _ = tx.send(msg);
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use lailaisay_core::{HotKeyOutput, Key};

    fn cmd_shift_space() -> HotKey {
        HotKey {
            key: Some(Key::Space),
            modifiers: Modifiers::new([Modifier::Command, Modifier::Shift]),
        }
    }

    #[test]
    fn chord_maps_to_carbon_code_and_mask() {
        let (code, mask) = carbon_chord(&cmd_shift_space()).unwrap();
        assert_eq!(code, 49);
        assert_eq!(mask, CMD_KEY | SHIFT_KEY);
    }

    #[test]
    fn modifier_only_and_fn_chords_are_rejected() {
        let fn_only = HotKey {
            key: None,
            modifiers: Modifiers::new([Modifier::Fn]),
        };
        assert!(!carbon_hotkey_supported(&fn_only));
        let fn_space = HotKey {
            key: Some(Key::Space),
            modifiers: Modifiers::new([Modifier::Fn]),
        };
        assert!(!carbon_hotkey_supported(&fn_space));
        let bare_space = HotKey {
            key: Some(Key::Space),
            modifiers: Modifiers::default(),
        };
        assert!(!carbon_hotkey_supported(&bare_space));
        assert!(carbon_hotkey_supported(&HotKey::default()));
        assert!(carbon_hotkey_supported(&HotKey::default_edit()));
    }

    #[test]
    fn synthesized_press_release_drives_processor_like_the_tap() {
        let hk = cmd_shift_space();
        let mut processor = HotKeyProcessor::new(hk.clone(), false);
        let mut edit = None;
        processor.set_now(1.0);
        let mut outputs = Vec::new();
        for e in synthesize_events(&hk, true) {
            outputs.push(process_tap_event(&mut processor, &mut edit, e).output);
        }
        assert_eq!(outputs, vec![Some(HotKeyOutput::StartRecording)]);

        processor.set_now(1.6);
        outputs.clear();
        for e in synthesize_events(&hk, false) {
            outputs.push(process_tap_event(&mut processor, &mut edit, e).output);
        }
        assert_eq!(outputs, vec![Some(HotKeyOutput::StopRecording), None]);
        assert!(!processor.is_matched());
    }

    #[test]
    fn inner_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<Inner>();
        assert_sync::<Inner>();
    }
}
