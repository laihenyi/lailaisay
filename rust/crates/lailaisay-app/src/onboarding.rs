//! First-run onboarding logic: which steps remain before dictation works.
//! Pure functions only (no egui, no OS calls except `Probe::read`), so Linux
//! CI covers the step order and the Developer ID / App Store / Windows split.

use lailaisay_input::GrantStatus;

use crate::settings_ui::PrivacyPane;

pub const SMALL_MODEL_ID: &str = "small";

/// Shown in the tray tooltip and status item while the wizard is owed.
pub const TRAY_REMINDER: &str = "需要設定，請開啟設定繼續";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacDeveloperId,
    MacAppStore,
    Windows,
    Other,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            if lailaisay_paste::is_app_store_build() {
                Self::MacAppStore
            } else {
                Self::MacDeveloperId
            }
        } else {
            Self::Other
        }
    }

    /// Permissions the user must grant before dictation can work. Windows and
    /// other platforms have no probe, so a broken microphone shows up in the
    /// 試用 step instead.
    pub fn required_permissions(self) -> &'static [PrivacyPane] {
        match self {
            Self::MacDeveloperId => &[
                PrivacyPane::Microphone,
                PrivacyPane::Accessibility,
                PrivacyPane::InputMonitoring,
            ],
            Self::MacAppStore => &[PrivacyPane::Microphone],
            Self::Windows | Self::Other => &[],
        }
    }
}

/// One snapshot of everything the wizard decides on.
#[derive(Debug, Clone, Copy)]
pub struct Probe {
    pub model_ready: bool,
    pub microphone: GrantStatus,
    pub accessibility: GrantStatus,
    pub input_monitoring: GrantStatus,
}

impl Probe {
    pub fn read(model_ready: bool) -> Self {
        Self {
            model_ready,
            microphone: lailaisay_input::microphone_grant(),
            accessibility: lailaisay_input::accessibility_grant(),
            input_monitoring: lailaisay_input::input_monitoring_grant(),
        }
    }

    pub fn grant(&self, pane: PrivacyPane) -> GrantStatus {
        match pane {
            PrivacyPane::Microphone => self.microphone,
            PrivacyPane::Accessibility => self.accessibility,
            PrivacyPane::InputMonitoring => self.input_monitoring,
            PrivacyPane::Automation => GrantStatus::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Welcome,
    Model,
    Permissions,
    Trial,
}

/// Only an explicit `Granted` counts. Denied, undecided and failed probes are
/// all "missing" so the wizard never claims a permission it cannot see.
pub fn missing_permissions(platform: Platform, probe: &Probe) -> Vec<PrivacyPane> {
    platform
        .required_permissions()
        .iter()
        .copied()
        .filter(|pane| probe.grant(*pane) != GrantStatus::Granted)
        .collect()
}

/// Steps still to do, in order. `trial_pending` is `onboardingPending`
/// (a fresh install); existing users never see Welcome or Trial.
pub fn remaining_steps(
    platform: Platform,
    probe: &Probe,
    trial_pending: bool,
    welcome_seen: bool,
) -> Vec<Step> {
    let mut steps = Vec::new();
    if trial_pending && !welcome_seen {
        steps.push(Step::Welcome);
    }
    if !probe.model_ready {
        steps.push(Step::Model);
    }
    if !missing_permissions(platform, probe).is_empty() {
        steps.push(Step::Permissions);
    }
    if trial_pending {
        steps.push(Step::Trial);
    }
    steps
}

pub fn needs_wizard(platform: Platform, probe: &Probe, trial_pending: bool) -> bool {
    !remaining_steps(platform, probe, trial_pending, true).is_empty()
}

pub fn recommended_model_id() -> &'static str {
    if lailaisay_stt::qwen_supported() {
        "qwen3-asr-1.7b"
    } else {
        "large-v3-turbo"
    }
}

/// The resident-STT note when a model file failed to load (`shared::load_stt`).
/// A plain "no model file" placeholder is not an error.
pub fn model_load_error(stt_note: &str) -> Option<&str> {
    stt_note.starts_with("dummy (failed").then_some(stt_note)
}

/// 稍後再說 hides the wizard for the rest of the run, so tray 開啟設定 still
/// reaches the normal panes (hotkey, microphone, models) when the user needs them.
pub fn wizard_should_open(needed: bool, dismissed: bool) -> bool {
    needed && !dismissed
}

/// The trial passes on a new utterance whose final text is not blank.
pub fn trial_passed(baseline: u64, count: u64, last_final: &str) -> bool {
    count > baseline && !last_final.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(model: bool, mic: GrantStatus, ax: GrantStatus, im: GrantStatus) -> Probe {
        Probe {
            model_ready: model,
            microphone: mic,
            accessibility: ax,
            input_monitoring: im,
        }
    }
    const G: GrantStatus = GrantStatus::Granted;

    #[test]
    fn developer_id_needs_three_permissions_app_store_only_the_microphone() {
        use PrivacyPane::*;
        assert_eq!(
            Platform::MacDeveloperId.required_permissions(),
            &[Microphone, Accessibility, InputMonitoring]
        );
        assert_eq!(Platform::MacAppStore.required_permissions(), &[Microphone]);
        assert!(Platform::Windows.required_permissions().is_empty());
        assert!(Platform::Other.required_permissions().is_empty());
    }

    #[test]
    fn denied_unknown_and_undecided_all_count_as_missing() {
        for bad in [
            GrantStatus::Denied,
            GrantStatus::Unknown,
            GrantStatus::NotDetermined,
            GrantStatus::TargetNotRunning,
        ] {
            let p = probe(true, bad, G, G);
            assert_eq!(
                missing_permissions(Platform::MacDeveloperId, &p),
                vec![PrivacyPane::Microphone],
                "{bad:?} must not be treated as granted"
            );
        }
        let all = probe(true, G, G, G);
        assert!(missing_permissions(Platform::MacDeveloperId, &all).is_empty());
    }

    #[test]
    fn steps_follow_welcome_model_permissions_trial_order() {
        let p = probe(false, GrantStatus::Denied, G, G);
        assert_eq!(
            remaining_steps(Platform::MacDeveloperId, &p, true, false),
            vec![Step::Welcome, Step::Model, Step::Permissions, Step::Trial]
        );
        assert_eq!(
            remaining_steps(Platform::MacDeveloperId, &p, true, true),
            vec![Step::Model, Step::Permissions, Step::Trial]
        );
    }

    #[test]
    fn existing_user_only_sees_what_is_missing() {
        let ok = probe(true, G, G, G);
        assert!(remaining_steps(Platform::MacDeveloperId, &ok, false, false).is_empty());
        assert!(!needs_wizard(Platform::MacDeveloperId, &ok, false));

        let no_model = probe(false, G, G, G);
        assert_eq!(
            remaining_steps(Platform::MacDeveloperId, &no_model, false, false),
            vec![Step::Model],
            "no Welcome and no Trial for a user who already finished onboarding"
        );
        assert!(needs_wizard(Platform::MacDeveloperId, &no_model, false));
    }

    #[test]
    fn windows_skips_permissions_but_still_runs_the_trial() {
        let p = probe(
            true,
            GrantStatus::Unknown,
            GrantStatus::Unknown,
            GrantStatus::Unknown,
        );
        assert_eq!(
            remaining_steps(Platform::Windows, &p, true, true),
            vec![Step::Trial]
        );
    }

    #[test]
    fn trial_needs_a_new_nonblank_utterance() {
        assert!(!trial_passed(3, 3, "舊的結果"), "no new utterance yet");
        assert!(!trial_passed(3, 4, ""), "silence is not a pass");
        assert!(!trial_passed(3, 4, "  \n "), "whitespace is not a pass");
        assert!(trial_passed(3, 4, "去台南。"));
    }

    #[test]
    fn recommended_model_is_in_the_visible_catalog() {
        let id = recommended_model_id();
        assert!(crate::models::catalog_by_id(id).is_some());
        assert!(crate::models::visible_catalog().any(|e| e.id == id));
        assert!(crate::models::visible_catalog().any(|e| e.id == SMALL_MODEL_ID));
        if lailaisay_stt::qwen_supported() {
            assert_eq!(id, "qwen3-asr-1.7b");
        } else {
            assert_eq!(id, "large-v3-turbo");
        }
    }

    #[test]
    fn load_failure_is_surfaced_but_a_plain_missing_model_is_not() {
        let failed = "dummy (failed to load /m/x.bin: bad magic)";
        assert_eq!(model_load_error(failed), Some(failed));
        assert_eq!(model_load_error("dummy (no model file)"), None);
        assert_eq!(model_load_error("resident: /m/x.bin"), None);
    }

    #[test]
    fn dismissing_the_wizard_keeps_it_closed_until_relaunch() {
        assert!(wizard_should_open(true, false));
        assert!(!wizard_should_open(true, true), "稍後再說 must reach normal settings");
        assert!(!wizard_should_open(false, false));
    }
}
