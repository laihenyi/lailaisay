//! Whether the CGEvent tap should swallow the event.

use lailaisay_core::{HotKey, HotKeyOutput, KeyEvent};

/// Port of `TranscriptionFeature.startHotKeyMonitoringEffect` swallow rules.
///
/// - `.startRecording`: intercept unless this is a modifier-only hotkey
///   (so Option-only still works as a normal modifier). Double-tap-only
///   always intercepts.
/// - `.stopRecording`: pass through
/// - `.cancel`: intercept
/// - no output: intercept repeated same-chord key downs so Space does not type
pub fn should_swallow(
    output: Option<HotKeyOutput>,
    event: &KeyEvent,
    hotkey: &HotKey,
    use_double_tap_only: bool,
) -> bool {
    match output {
        Some(HotKeyOutput::StartRecording) => use_double_tap_only || event.key.is_some(),
        Some(HotKeyOutput::StopRecording) => false,
        Some(HotKeyOutput::Cancel) => true,
        None => {
            event.key.is_some() && event.key == hotkey.key && event.modifiers == hotkey.modifiers
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lailaisay_core::{Key, Modifier, Modifiers};

    fn cmd_a() -> HotKey {
        HotKey {
            key: Some(Key::A),
            modifiers: Modifiers::new([Modifier::Command]),
        }
    }

    fn option_only() -> HotKey {
        HotKey {
            key: None,
            modifiers: Modifiers::new([Modifier::Option]),
        }
    }

    #[test]
    fn start_with_key_is_swallowed() {
        let event = KeyEvent::new(Some(Key::A), [Modifier::Command]);
        assert!(should_swallow(
            Some(HotKeyOutput::StartRecording),
            &event,
            &cmd_a(),
            false
        ));
    }

    #[test]
    fn start_modifier_only_is_not_swallowed() {
        let event = KeyEvent::new(None, [Modifier::Option]);
        assert!(!should_swallow(
            Some(HotKeyOutput::StartRecording),
            &event,
            &option_only(),
            false
        ));
    }

    #[test]
    fn double_tap_only_swallows_modifier_start() {
        let event = KeyEvent::new(None, [Modifier::Option]);
        assert!(should_swallow(
            Some(HotKeyOutput::StartRecording),
            &event,
            &option_only(),
            true
        ));
    }

    #[test]
    fn stop_is_not_swallowed() {
        let event = KeyEvent::new(None, [Modifier::Command]);
        assert!(!should_swallow(
            Some(HotKeyOutput::StopRecording),
            &event,
            &cmd_a(),
            false
        ));
    }

    #[test]
    fn cancel_is_swallowed() {
        let event = KeyEvent::new(Some(Key::Escape), []);
        assert!(should_swallow(
            Some(HotKeyOutput::Cancel),
            &event,
            &cmd_a(),
            false
        ));
    }

    #[test]
    fn repeated_matching_keydown_is_swallowed() {
        let event = KeyEvent::new(Some(Key::A), [Modifier::Command]);
        assert!(should_swallow(None, &event, &cmd_a(), false));
    }
}
