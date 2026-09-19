//! Shared tap message + processor dispatch used by macOS and Windows.

use lailaisay_core::{HotKeyOutput, HotKeyProcessor, HotKeyState, KeyEvent};

use crate::should_swallow;

/// One OS key event after it has been mapped and fed through the processors.
pub struct TapMessage {
    pub event: KeyEvent,
    pub output: Option<HotKeyOutput>,
    pub state: HotKeyState,
    pub edit_output: Option<HotKeyOutput>,
    pub edit_state: Option<HotKeyState>,
    pub swallowed: bool,
}

/// Feed one mapped key event through the dictation + optional edit processors.
///
/// Callers that need wall-clock time (live taps) should
/// [`HotKeyProcessor::stamp_wall_clock`] first. Tests can [`HotKeyProcessor::set_now`].
pub fn process_tap_event(
    processor: &mut HotKeyProcessor,
    edit: &mut Option<HotKeyProcessor>,
    key_event: KeyEvent,
) -> TapMessage {
    let output = processor.process(&key_event);
    let state = processor.state();
    let hotkey = processor.hotkey.clone();
    let double_tap = processor.use_double_tap_only;

    let (edit_output, edit_state, edit_hotkey) = if let Some(p) = edit.as_mut() {
        let output = p.process(&key_event);
        (output, Some(p.state()), Some(p.hotkey.clone()))
    } else {
        (None, None, None)
    };

    let mut swallowed = should_swallow(output, &key_event, &hotkey, double_tap);
    if let Some(hk) = edit_hotkey.as_ref() {
        swallowed |= should_swallow(edit_output, &key_event, hk, false);
    }

    TapMessage {
        event: key_event,
        output,
        state,
        edit_output,
        edit_state,
        swallowed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lailaisay_core::{HotKey, Key, Modifier, Modifiers};

    fn cmd_shift_space() -> HotKey {
        HotKey {
            key: Some(Key::Space),
            modifiers: Modifiers::new([Modifier::Command, Modifier::Shift]),
        }
    }

    #[test]
    fn hold_and_release_emits_start_then_stop() {
        let mut processor = HotKeyProcessor::new(cmd_shift_space(), false);
        let mut edit = None;
        processor.set_now(1.0);
        let down = process_tap_event(
            &mut processor,
            &mut edit,
            KeyEvent::new(Some(Key::Space), [Modifier::Command, Modifier::Shift]),
        );
        assert_eq!(down.output, Some(HotKeyOutput::StartRecording));
        assert!(down.swallowed);

        processor.set_now(1.4);
        let up = process_tap_event(
            &mut processor,
            &mut edit,
            KeyEvent::new(None, [Modifier::Command, Modifier::Shift]),
        );
        assert_eq!(up.output, Some(HotKeyOutput::StopRecording));
        assert!(!up.swallowed);
    }

    #[test]
    fn escape_cancels_and_is_swallowed() {
        let mut processor = HotKeyProcessor::new(cmd_shift_space(), false);
        let mut edit = None;
        processor.set_now(1.0);
        let _ = process_tap_event(
            &mut processor,
            &mut edit,
            KeyEvent::new(Some(Key::Space), [Modifier::Command, Modifier::Shift]),
        );
        processor.set_now(1.1);
        let cancel = process_tap_event(
            &mut processor,
            &mut edit,
            KeyEvent::new(Some(Key::Escape), [Modifier::Command, Modifier::Shift]),
        );
        assert_eq!(cancel.output, Some(HotKeyOutput::Cancel));
        assert!(cancel.swallowed);
    }
}
