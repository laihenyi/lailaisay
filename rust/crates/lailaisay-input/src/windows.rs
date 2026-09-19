//! Windows press-and-hold hotkey via `WH_KEYBOARD_LL`.
//!
//! Installs [`SetWindowsHookExW`](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setwindowshookexw)
//! on a dedicated thread that pumps `GetMessageW`. Mapped vkeys feed
//! [`lailaisay_core::HotKeyProcessor`] and emit [`crate::TapMessage`] — the same
//! channel shape as [`crate::MacOsEventTap`].
//!
//! Returning a non-zero hook result swallows the hotkey (when
//! [`crate::should_swallow`] says so). Injected `SendInput` events are ignored
//! so paste does not re-enter the processor.

use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use lailaisay_core::{HotKey, HotKeyProcessor, KeyEvent};

use crate::tap::{process_tap_event, TapMessage};
use crate::vk::{
    apply_vk_to_modifiers, is_injected_ll_flags, key_event_from_win, ModifierDown, WinEventKind,
    LLKHF_UP, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL,
    VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
};
use crate::{HotkeyMonitor, InputError, Result};

const WH_KEYBOARD_LL: i32 = 13;
const WM_QUIT: u32 = 0x0012;
const HC_ACTION: i32 = 0;

#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
struct Msg {
    hwnd: isize,
    message: u32,
    w_param: usize,
    l_param: isize,
    time: u32,
    pt: Point,
}

/// `KBDLLHOOKSTRUCT`
#[repr(C)]
struct KbdLlHook {
    vk_code: u32,
    scan_code: u32,
    flags: u32,
    time: u32,
    extra: usize,
}

#[link(name = "user32")]
extern "system" {
    fn SetWindowsHookExW(
        id_hook: i32,
        lpfn: Option<unsafe extern "system" fn(i32, usize, isize) -> isize>,
        hmod: isize,
        dw_thread_id: u32,
    ) -> isize;
    fn UnhookWindowsHookEx(hhk: isize) -> i32;
    fn CallNextHookEx(hhk: isize, n_code: i32, w_param: usize, l_param: isize) -> isize;
    fn GetMessageW(lp_msg: *mut Msg, hwnd: isize, min: u32, max: u32) -> i32;
    fn TranslateMessage(lp_msg: *const Msg) -> i32;
    fn DispatchMessageW(lp_msg: *const Msg) -> isize;
    fn PostThreadMessageW(id_thread: u32, msg: u32, w_param: usize, l_param: isize) -> i32;
    fn GetAsyncKeyState(v_key: i32) -> i16;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentThreadId() -> u32;
    fn GetLastError() -> u32;
}

struct TapInner {
    processor: Mutex<HotKeyProcessor>,
    edit_processor: Mutex<Option<HotKeyProcessor>>,
    tx: Mutex<Sender<TapMessage>>,
}

/// Process-wide callback target. `WH_KEYBOARD_LL` has no user-info pointer.
static HOOK_INNER: Mutex<Option<Arc<TapInner>>> = Mutex::new(None);

/// Live `WH_KEYBOARD_LL` tap. Same host-facing API as [`crate::MacOsEventTap`].
pub struct WindowsEventTap {
    rx: Receiver<TapMessage>,
    inner: Arc<TapInner>,
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

/// Documented alias used by the original Windows host notes.
pub type WindowsHotkeyMonitor = WindowsEventTap;

impl WindowsEventTap {
    pub fn new() -> Result<Self> {
        Self::with_processor(HotKeyProcessor::new(HotKey::default(), false))
    }

    pub fn with_processor(processor: HotKeyProcessor) -> Result<Self> {
        let (tx, rx) = mpsc::channel();
        let inner = Arc::new(TapInner {
            processor: Mutex::new(processor),
            edit_processor: Mutex::new(None),
            tx: Mutex::new(tx),
        });

        {
            let mut slot = HOOK_INNER
                .lock()
                .map_err(|e| InputError::Message(format!("hook state lock: {e}")))?;
            if slot.is_some() {
                return Err(InputError::Message(
                    "a Windows keyboard hook is already installed in this process".into(),
                ));
            }
            *slot = Some(inner.clone());
        }

        let (ready_tx, ready_rx) = mpsc::channel::<std::result::Result<u32, u32>>();
        let thread = thread::Builder::new()
            .name("lailaisay-wh-keyboard-ll".into())
            .spawn(move || unsafe { run_hook_thread(ready_tx) })
            .map_err(|e| {
                let _ = HOOK_INNER.lock().map(|mut g| *g = None);
                InputError::Message(e.to_string())
            })?;

        let thread_id = match ready_rx.recv_timeout(Duration::from_secs(3)) {
            Ok(Ok(tid)) => tid,
            Ok(Err(code)) => {
                let _ = HOOK_INNER.lock().map(|mut g| *g = None);
                let _ = thread.join();
                return Err(InputError::Message(hook_install_error(code)));
            }
            Err(_) => {
                let _ = HOOK_INNER.lock().map(|mut g| *g = None);
                return Err(InputError::Message(
                    "WH_KEYBOARD_LL thread did not report ready — antivirus or policy may have blocked the hook"
                        .into(),
                ));
            }
        };

        if thread.is_finished() {
            let _ = HOOK_INNER.lock().map(|mut g| *g = None);
            return Err(InputError::Message(hook_install_error(0)));
        }

        Ok(Self {
            rx,
            inner,
            thread_id,
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
                Err(InputError::Message("WH_KEYBOARD_LL channel closed".into()))
            }
        }
    }

    pub fn recv_message_timeout(&mut self, timeout: Duration) -> Result<Option<TapMessage>> {
        match self.rx.recv_timeout(timeout) {
            Ok(m) => Ok(Some(m)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                Err(InputError::Message("WH_KEYBOARD_LL channel closed".into()))
            }
        }
    }
}

impl HotkeyMonitor for WindowsEventTap {
    fn next_event(&mut self) -> Result<Option<KeyEvent>> {
        Ok(self
            .recv_message_timeout(Duration::from_millis(250))?
            .map(|m| m.event))
    }
}

impl Drop for WindowsEventTap {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
        }
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
        if let Ok(mut slot) = HOOK_INNER.lock() {
            if let Some(current) = slot.as_ref() {
                if Arc::ptr_eq(current, &self.inner) {
                    *slot = None;
                }
            }
        }
    }
}

fn hook_install_error(code: u32) -> String {
    format!(
        "{}\nSetWindowsHookExW(WH_KEYBOARD_LL) failed (Win32 error {code}). \
         Low-level hooks do not need a macOS-style Accessibility grant, but \
         antivirus, ransomware protection, or enterprise software can block them. \
         Try allowing lailaisay / the terminal, then relaunch.",
        crate::permissions::WINDOWS_HOTKEY_HELP.trim()
    )
}

fn async_key_down(vk: u32) -> bool {
    unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

fn current_modifier_snapshot() -> ModifierDown {
    ModifierDown {
        control: async_key_down(VK_CONTROL)
            || async_key_down(VK_LCONTROL)
            || async_key_down(VK_RCONTROL),
        alt: async_key_down(VK_MENU) || async_key_down(VK_LMENU) || async_key_down(VK_RMENU),
        shift: async_key_down(VK_SHIFT) || async_key_down(VK_LSHIFT) || async_key_down(VK_RSHIFT),
        win: async_key_down(VK_LWIN) || async_key_down(VK_RWIN),
    }
}

unsafe extern "system" fn low_level_keyboard_proc(
    n_code: i32,
    w_param: usize,
    l_param: isize,
) -> isize {
    if n_code < 0 || n_code != HC_ACTION {
        return CallNextHookEx(0, n_code, w_param, l_param);
    }
    let Some(kb) = (l_param as *const KbdLlHook).as_ref() else {
        return CallNextHookEx(0, n_code, w_param, l_param);
    };
    if is_injected_ll_flags(kb.flags) {
        return CallNextHookEx(0, n_code, w_param, l_param);
    }

    let kind = {
        let from_msg = WinEventKind::from_wparam(w_param);
        let from_flags = WinEventKind::from_ll_flags(kb.flags);
        if from_flags.is_up() || from_msg.is_up() || (kb.flags & LLKHF_UP) != 0 {
            WinEventKind::KeyUp
        } else {
            WinEventKind::KeyDown
        }
    };

    let snap = apply_vk_to_modifiers(current_modifier_snapshot(), kb.vk_code, kind.is_up());
    let key_event = key_event_from_win(kind, kb.vk_code, snap.to_modifiers());

    let inner = {
        let Ok(guard) = HOOK_INNER.lock() else {
            return CallNextHookEx(0, n_code, w_param, l_param);
        };
        guard.clone()
    };
    let Some(inner) = inner else {
        return CallNextHookEx(0, n_code, w_param, l_param);
    };

    let (swallowed, msg) = {
        let Ok(mut processor) = inner.processor.lock() else {
            return CallNextHookEx(0, n_code, w_param, l_param);
        };
        let Ok(mut edit) = inner.edit_processor.lock() else {
            return CallNextHookEx(0, n_code, w_param, l_param);
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
        1
    } else {
        CallNextHookEx(0, n_code, w_param, l_param)
    }
}

unsafe fn run_hook_thread(ready: Sender<std::result::Result<u32, u32>>) {
    let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_keyboard_proc), 0, 0);
    if hook == 0 {
        let _ = ready.send(Err(GetLastError()));
        return;
    }
    let tid = GetCurrentThreadId();
    if ready.send(Ok(tid)).is_err() {
        UnhookWindowsHookEx(hook);
        return;
    }

    let mut msg = Msg {
        hwnd: 0,
        message: 0,
        w_param: 0,
        l_param: 0,
        time: 0,
        pt: Point { x: 0, y: 0 },
    };
    loop {
        let ret = GetMessageW(&mut msg, 0, 0, 0);
        if ret == 0 || ret == -1 {
            break;
        }
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    UnhookWindowsHookEx(hook);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vk::{key_event_from_win, ModifierDown, WinEventKind, VK_ESCAPE, VK_SPACE};
    use lailaisay_core::{HotKey, HotKeyOutput, Key, Modifier, Modifiers};

    #[test]
    fn mapped_hold_matches_hotkey_processor() {
        let mut processor = HotKeyProcessor::new(
            HotKey {
                key: Some(Key::Space),
                modifiers: Modifiers::new([Modifier::Command, Modifier::Shift]),
            },
            false,
        );
        let mut edit = None;
        processor.set_now(2.0);
        let mods = ModifierDown {
            win: true,
            shift: true,
            ..ModifierDown::default()
        }
        .to_modifiers();
        let down = process_tap_event(
            &mut processor,
            &mut edit,
            key_event_from_win(WinEventKind::KeyDown, VK_SPACE, mods.clone()),
        );
        assert_eq!(down.output, Some(HotKeyOutput::StartRecording));
        assert!(down.swallowed);

        processor.set_now(2.5);
        let up = process_tap_event(
            &mut processor,
            &mut edit,
            key_event_from_win(WinEventKind::KeyUp, VK_SPACE, mods),
        );
        assert_eq!(up.output, Some(HotKeyOutput::StopRecording));
    }

    #[test]
    fn escape_vk_cancels() {
        let mut processor = HotKeyProcessor::new(HotKey::default(), false);
        let mut edit = None;
        processor.set_now(1.0);
        let mods = ModifierDown {
            win: true,
            shift: true,
            ..ModifierDown::default()
        }
        .to_modifiers();
        let _ = process_tap_event(
            &mut processor,
            &mut edit,
            key_event_from_win(WinEventKind::KeyDown, VK_SPACE, mods.clone()),
        );
        processor.set_now(1.1);
        let cancel = process_tap_event(
            &mut processor,
            &mut edit,
            key_event_from_win(WinEventKind::KeyDown, VK_ESCAPE, mods),
        );
        assert_eq!(cancel.output, Some(HotKeyOutput::Cancel));
        assert!(cancel.swallowed);
    }

    #[test]
    fn tap_inner_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<TapInner>();
        assert_sync::<TapInner>();
        assert_send::<Arc<TapInner>>();
    }
}
