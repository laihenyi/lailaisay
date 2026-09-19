//! Microphone open helpers.
//!
//! `cpal::Stream` (and therefore [`lailaisay_stt::record::LiveRecorder`]) is `!Send`
//! on macOS Core Audio. Never `thread::spawn` a start that moves the recorder
//! back over a channel. Open and drop it on the eframe / UI thread.
//!
//! Tray latency is fixed by setting status + `set_icon` *before* this open,
//! so a slow device open happens behind an already-Active icon.

/// `Fire::DelayedStart` must not begin capture after cancel / `StartNow`.
pub fn delayed_start_still_wanted(cancelled: bool) -> bool {
    !cancelled
}

/// Open the default input on the **current** thread. Do not send the result
/// across threads — `LiveRecorder` is not `Send` on macOS.
#[cfg(feature = "mic")]
pub fn start_live_recorder_on_this_thread(
) -> std::result::Result<lailaisay_stt::record::LiveRecorder, String> {
    let rec = lailaisay_stt::record::LiveRecorder::start(None).map_err(|e| e.to_string())?;
    // Settings 權限 lamp: cpal opened, so TCC microphone is granted this session.
    lailaisay_input::note_microphone_opened();
    Ok(rec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delayed_start_skipped_after_cancel() {
        assert!(delayed_start_still_wanted(false));
        assert!(!delayed_start_still_wanted(true));
    }

    /// Documents the contract: the recorder type is stored locally, never
    /// required to be `Send` (a `thread::spawn` + `mpsc` return would fail
    /// `cargo build --features mic` on macOS).
    #[cfg(feature = "mic")]
    #[test]
    fn live_recorder_stays_on_calling_thread() {
        fn hold(
            rec: Option<lailaisay_stt::record::LiveRecorder>,
        ) -> Option<lailaisay_stt::record::LiveRecorder> {
            rec
        }
        assert!(hold(None).is_none());
    }
}
