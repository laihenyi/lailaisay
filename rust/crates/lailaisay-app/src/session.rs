//! Maps `HotKeyProcessor` output onto record start/stop, including the
//! A 200 ms delayed start (so a double-tap can cancel).

use lailaisay_core::{HotKey, HotKeyOutput, HotKeyProcessor, HotKeyState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionAction {
    None,
    /// Wait [`lailaisay_core::hotkey::RECORD_START_DELAY_SECS`] then start, unless cancelled.
    ArmDelayedStart,
    CancelDelayedStart,
    /// Double-tap lock: start immediately (no 200 ms window).
    StartNow,
    Stop,
    Cancel,
}

/// Owns the processor plus "are we recording / is a delayed start pending".
#[derive(Debug, Clone)]
pub struct Session {
    pub processor: HotKeyProcessor,
    pub edit_processor: HotKeyProcessor,
    pub recording: bool,
    pub pending_start: bool,
    pub recording_edit: bool,
}

impl Session {
    pub fn new(hotkey: HotKey, use_double_tap_only: bool) -> Self {
        Self::with_edit_hotkey(hotkey, use_double_tap_only, HotKey::default_edit())
    }

    pub fn with_edit_hotkey(
        hotkey: HotKey,
        use_double_tap_only: bool,
        edit_hotkey: HotKey,
    ) -> Self {
        Self {
            processor: HotKeyProcessor::new(hotkey, use_double_tap_only),
            edit_processor: HotKeyProcessor::new(edit_hotkey, false),
            recording: false,
            pending_start: false,
            recording_edit: false,
        }
    }

    pub fn handle_output(
        &mut self,
        output: Option<HotKeyOutput>,
        state: HotKeyState,
    ) -> SessionAction {
        match output {
            Some(HotKeyOutput::StartRecording) => {
                if self.recording_edit {
                    return SessionAction::None;
                }
                if matches!(state, HotKeyState::DoubleTapLock) {
                    self.pending_start = false;
                    self.recording = true;
                    SessionAction::StartNow
                } else {
                    self.pending_start = true;
                    SessionAction::ArmDelayedStart
                }
            }
            Some(HotKeyOutput::StopRecording) => {
                if self.recording {
                    self.recording = false;
                    self.pending_start = false;
                    SessionAction::Stop
                } else {
                    self.pending_start = false;
                    SessionAction::CancelDelayedStart
                }
            }
            Some(HotKeyOutput::Cancel) => {
                self.recording = false;
                self.pending_start = false;
                self.recording_edit = false;
                SessionAction::Cancel
            }
            None => SessionAction::None,
        }
    }

    /// Speak-to-Edit hold chord (same delayed start / stop as dictation).
    pub fn handle_edit_output(
        &mut self,
        output: Option<HotKeyOutput>,
        state: HotKeyState,
    ) -> SessionAction {
        if self.recording && !self.recording_edit {
            return SessionAction::None;
        }
        match output {
            Some(HotKeyOutput::StartRecording) => {
                if matches!(state, HotKeyState::DoubleTapLock) {
                    self.pending_start = false;
                    self.recording = true;
                    self.recording_edit = true;
                    SessionAction::StartNow
                } else {
                    self.pending_start = true;
                    self.recording_edit = true;
                    SessionAction::ArmDelayedStart
                }
            }
            Some(HotKeyOutput::StopRecording) => {
                if self.recording {
                    self.recording = false;
                    self.pending_start = false;
                    self.recording_edit = false;
                    SessionAction::Stop
                } else {
                    self.pending_start = false;
                    self.recording_edit = false;
                    SessionAction::CancelDelayedStart
                }
            }
            Some(HotKeyOutput::Cancel) => {
                self.recording = false;
                self.pending_start = false;
                self.recording_edit = false;
                SessionAction::Cancel
            }
            None => SessionAction::None,
        }
    }

    /// Feed a key event using wall-clock or injected time (tests call `set_now` first).
    pub fn handle_event(&mut self, event: &lailaisay_core::KeyEvent) -> SessionAction {
        let output = self.processor.process(event);
        let state = self.processor.state();
        self.handle_output(output, state)
    }

    /// Tray/status string while a hold is armed or recording.
    ///
    /// Set this as soon as [`SessionAction::ArmDelayedStart`] is returned — do
    /// not wait for [`lailaisay_core::hotkey::RECORD_START_DELAY_SECS`]. The tray
    /// treats these as Active (`record` substring).
    pub fn hold_feedback_status(&self) -> Option<&'static str> {
        if self.pending_start || self.recording {
            Some(armed_hold_status(self.recording_edit))
        } else {
            None
        }
    }
}

/// Shared status for an armed / live hold. Mapped to tray Active.
pub fn armed_hold_status(is_edit: bool) -> &'static str {
    if is_edit {
        "speak-to-edit recording"
    } else {
        "recording"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lailaisay_core::{HotKey, Key, KeyEvent, Modifier, Modifiers};

    fn cmd_a() -> HotKey {
        HotKey {
            key: Some(Key::A),
            modifiers: Modifiers::new([Modifier::Command]),
        }
    }

    #[test]
    fn press_arms_delayed_start_release_before_record_cancels() {
        let mut s = Session::new(cmd_a(), false);
        s.processor.set_now(0.0);
        let down = KeyEvent::new(Some(Key::A), [Modifier::Command]);
        assert_eq!(s.handle_event(&down), SessionAction::ArmDelayedStart);
        assert!(s.pending_start);
        assert!(!s.recording);
        assert_eq!(s.hold_feedback_status(), Some("recording"));
        assert_eq!(armed_hold_status(false), "recording");

        s.processor.set_now(0.1);
        let up = KeyEvent::new(None, [Modifier::Command]);
        assert_eq!(s.handle_event(&up), SessionAction::CancelDelayedStart);
        assert!(!s.pending_start);
        assert!(!s.recording);
        assert_eq!(s.hold_feedback_status(), None);
    }

    #[test]
    fn delayed_start_then_release_stops() {
        let mut s = Session::new(cmd_a(), false);
        s.processor.set_now(0.0);
        let down = KeyEvent::new(Some(Key::A), [Modifier::Command]);
        assert_eq!(s.handle_event(&down), SessionAction::ArmDelayedStart);
        // Runtime would fire the delayed start:
        s.recording = true;
        s.pending_start = false;

        s.processor.set_now(0.5);
        let up = KeyEvent::new(None, [Modifier::Command]);
        assert_eq!(s.handle_event(&up), SessionAction::Stop);
        assert!(!s.recording);
    }

    #[test]
    fn double_tap_lock_starts_immediately_on_second_release() {
        let mut s = Session::new(
            HotKey {
                key: None,
                modifiers: Modifiers::new([Modifier::Option]),
            },
            false,
        );
        s.processor.set_now(0.0);
        assert_eq!(
            s.handle_event(&KeyEvent::new(None, [Modifier::Option])),
            SessionAction::ArmDelayedStart
        );
        s.processor.set_now(0.1);
        assert_eq!(
            s.handle_event(&KeyEvent::new(None, [])),
            SessionAction::CancelDelayedStart
        );
        s.processor.set_now(0.2);
        assert_eq!(
            s.handle_event(&KeyEvent::new(None, [Modifier::Option])),
            SessionAction::ArmDelayedStart
        );
        s.processor.set_now(0.3);
        // Second release while still in the double-tap window → lock, no new output
        // from the processor (state becomes DoubleTapLock). Session stays pending
        // until the *next* start. The session stays recording without a new Start.
        let action = s.handle_event(&KeyEvent::new(None, []));
        assert_eq!(s.processor.state(), HotKeyState::DoubleTapLock);
        assert_eq!(action, SessionAction::None);
    }

    #[test]
    fn double_tap_only_second_release_starts_now() {
        let mut s = Session::new(
            HotKey {
                key: Some(Key::A),
                modifiers: Modifiers::new([Modifier::Command]),
            },
            true,
        );
        s.processor.set_now(0.0);
        assert_eq!(
            s.handle_event(&KeyEvent::new(Some(Key::A), [Modifier::Command])),
            SessionAction::None
        );
        s.processor.set_now(0.1);
        // Full release within 0.3s (`useDoubleTapOnly` + key).
        assert_eq!(
            s.handle_event(&KeyEvent::new(None, [])),
            SessionAction::StartNow
        );
        assert_eq!(s.processor.state(), HotKeyState::DoubleTapLock);
        assert!(s.recording);
    }

    #[test]
    fn escape_cancels() {
        let mut s = Session::new(cmd_a(), false);
        s.processor.set_now(0.0);
        s.handle_event(&KeyEvent::new(Some(Key::A), [Modifier::Command]));
        s.recording = true;
        s.processor.set_now(0.5);
        assert_eq!(
            s.handle_event(&KeyEvent::new(Some(Key::Escape), [])),
            SessionAction::Cancel
        );
        assert!(!s.recording);
    }

    #[test]
    fn edit_hotkey_arms_without_stealing_dictation() {
        let mut s = Session::with_edit_hotkey(
            cmd_a(),
            false,
            HotKey {
                key: Some(Key::E),
                modifiers: Modifiers::new([Modifier::Command]),
            },
        );
        s.edit_processor.set_now(0.0);
        let down = KeyEvent::new(Some(Key::E), [Modifier::Command]);
        let out = s.edit_processor.process(&down);
        let state = s.edit_processor.state();
        assert_eq!(
            s.handle_edit_output(out, state),
            SessionAction::ArmDelayedStart
        );
        assert!(s.recording_edit);
        assert!(s.pending_start);
        assert_eq!(s.hold_feedback_status(), Some("speak-to-edit recording"));
    }
}
