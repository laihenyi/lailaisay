use crate::hotkey::HotKey;
use serde::{Deserialize, Deserializer, Serialize};
use std::path::Path;

use crate::error::LailaisayError;
use crate::paths::remapped_selected_whisper_model;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ModelWarmStatus {
    #[default]
    Cold,
    Warming,
    Warm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum AiEnhancementMode {
    Off,
    #[default]
    Smart,
    Full,
}

/// Dictate polish aggressiveness. Optional JSON override (`aiPolishStyle`).
///
/// When omitted, lailaisay derives style from `aiEnhancementMode`:
/// Smart → [`AiPolishStyle::Clean`], Full → [`AiPolishStyle::Formal`].
/// Off never calls the LLM (local filters only — 關閉／不潤色).
///
/// `aiPolishStyle: minimal` is first-class when the LLM is on (Smart/Full):
/// punctuation + vocal noise only. Selectable in the AI pane as 最小.
///
/// 借鑑 typefree 開源「不優化」分流（kdsz001/typefree polish_provider == none），
/// 非官方 Typeless system prompt；亦非把 GPL 產品名稱寫進 lailaisay UI。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum AiPolishStyle {
    /// Punctuation + pure vocal noise only (最小). LLM still runs if mode ≠ Off.
    Minimal,
    /// Fillers + self-correction; do not change word choice (清理).
    #[default]
    Clean,
    /// Clean plus list/topic structure when the speaker signals it.
    Structured,
    /// May change register/word choice for email/work; no empty pleasantries.
    Formal,
}

impl AiPolishStyle {
    /// Settings 繁中 label. 最小／不潤色 must stay obvious.
    pub fn zh_label(self) -> &'static str {
        match self {
            Self::Minimal => "最小",
            Self::Clean => "清理",
            Self::Structured => "結構",
            Self::Formal => "正式",
        }
    }
}

impl AiEnhancementMode {
    pub fn from_legacy_bool(use_ai: bool) -> Self {
        if use_ai {
            Self::Full
        } else {
            Self::Off
        }
    }

    /// Default polish aggressiveness for this enablement mode when `aiPolishStyle` is unset.
    pub fn default_polish_style(self) -> AiPolishStyle {
        match self {
            Self::Off => AiPolishStyle::Minimal,
            Self::Smart => AiPolishStyle::Clean,
            Self::Full => AiPolishStyle::Formal,
        }
    }

    /// Settings 繁中 label. Off = 關閉／不潤色 (no LLM).
    pub fn zh_label(self) -> &'static str {
        match self {
            Self::Off => "關閉／不潤色",
            Self::Smart => "清理",
            Self::Full => "正式",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum AiProviderType {
    #[default]
    Ollama,
    Groq,
    Gemini,
}

impl AiProviderType {
    /// Groq and Gemini share `selectedRemoteModel`; Ollama uses `selectedAIModel`.
    pub fn uses_remote_model(self) -> bool {
        matches!(self, Self::Groq | Self::Gemini)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum OutputStyle {
    Formal,
    Casual,
    Technical,
    Notes,
    #[default]
    General,
}

/// Persisted settings. Field names use the existing camelCase JSON schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LailaisaySettings {
    pub sound_effects_enabled: bool,
    pub hotkey: HotKey,
    /// Speak-to-Edit hold chord. Optional in persisted JSON (`#[serde(default)]`).
    #[serde(default = "HotKey::default_edit")]
    pub edit_hotkey: HotKey,
    pub open_on_login: bool,
    pub show_dock_icon: bool,
    /// Stay in the menu bar / tray on launch. Product default is `true`.
    pub minimize_to_menu_bar_on_launch: bool,
    pub selected_model: String,
    /// ggml/gguf path for the Rust `lailaisay-app` whisper.cpp backend.
    /// Optional so existing settings files still load.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_whisper_model: Option<String>,
    pub use_clipboard_paste: bool,
    pub prevent_system_sleep: bool,
    pub pause_media_on_record: bool,
    pub minimum_key_time: f64,
    pub copy_to_clipboard: bool,
    pub use_double_tap_only: bool,
    pub output_language: Option<String>,
    #[serde(default, alias = "selectedMicrophoneID")]
    pub selected_microphone_id: Option<String>,
    pub disable_auto_capitalization: bool,
    pub enable_screen_capture: bool,
    /// First-run gate. Missing from older `hex_settings.json` files (serde
    /// `false`); [`Self::for_launch`] treats an existing file as complete.
    pub has_completed_onboarding: bool,
    pub prefer_traditional_chinese: bool,
    #[serde(deserialize_with = "deserialize_enhancement_mode")]
    pub ai_enhancement_mode: AiEnhancementMode,
    /// Optional aggressiveness override. Missing/`null` → derive from `aiEnhancementMode`.
    /// Omitted from serialized settings when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_polish_style: Option<AiPolishStyle>,
    pub selected_ai_model: String,
    pub ai_enhancement_prompt: String,
    pub ai_enhancement_temperature: f64,
    pub ai_provider_type: AiProviderType,
    /// Stored in the settings file. Prefer `TOK_GROQ_API_KEY` / `GROQ_API_KEY`.
    #[serde(default, alias = "groqAPIKey")]
    pub groq_api_key: String,
    /// Stored in the settings file. Prefer env over this field.
    /// `TOK_GEMINI_API_KEY`, then `GEMINI_API_KEY`, then `GOOGLE_API_KEY`.
    #[serde(default, alias = "geminiAPIKey")]
    pub gemini_api_key: String,
    pub selected_remote_model: String,
    /// Per-provider model choices; legacy selectedRemoteModel remains supported.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub remote_models: std::collections::BTreeMap<String, String>,
    pub voice_recognition_prompt: String,
    pub transcription_model_warm_status: ModelWarmStatus,
    pub selected_image_model: String,
    pub selected_remote_image_model: String,
    pub image_analysis_prompt: String,
    pub developer_mode_enabled: bool,
    pub enable_streaming_fallback: bool,
    pub minimum_fallback_length: i32,
    pub auto_learn_from_corrections: bool,
    pub remove_filler_words: bool,
    pub resolve_self_corrections: bool,
    /// Inject OutputStyle tone into the polish prompt. Default OFF (typefree:
    /// stacking style/preserve clauses makes the model timid). Existing
    /// `"enableContextAwareStyle": true` in a settings file is preserved.
    pub enable_context_aware_style: bool,
    pub enable_structured_output: bool,
}

impl Default for LailaisaySettings {
    fn default() -> Self {
        Self {
            sound_effects_enabled: true,
            hotkey: HotKey::default(),
            edit_hotkey: HotKey::default_edit(),
            open_on_login: false,
            show_dock_icon: true,
            minimize_to_menu_bar_on_launch: true,
            selected_model: "openai_whisper-large-v3-v20240930".into(),
            selected_whisper_model: None,
            use_clipboard_paste: true,
            prevent_system_sleep: true,
            pause_media_on_record: true,
            minimum_key_time: 0.2,
            copy_to_clipboard: true,
            use_double_tap_only: false,
            output_language: None,
            selected_microphone_id: None,
            disable_auto_capitalization: false,
            enable_screen_capture: false,
            has_completed_onboarding: false,
            prefer_traditional_chinese: true,
            ai_enhancement_mode: AiEnhancementMode::Smart,
            ai_polish_style: None,
            selected_ai_model: "gemma3".into(),
            ai_enhancement_prompt: DEFAULT_ENHANCEMENT_PROMPT.into(),
            ai_enhancement_temperature: 0.3,
            ai_provider_type: AiProviderType::Ollama,
            groq_api_key: String::new(),
            gemini_api_key: String::new(),
            selected_remote_model: "compound-beta-mini".into(),
            remote_models: Default::default(),
            voice_recognition_prompt: String::new(),
            transcription_model_warm_status: ModelWarmStatus::Cold,
            selected_image_model: "llava:latest".into(),
            selected_remote_image_model: "llava-v1.5-7b-4096-preview".into(),
            image_analysis_prompt: DEFAULT_IMAGE_ANALYSIS_PROMPT.into(),
            developer_mode_enabled: false,
            enable_streaming_fallback: false,
            minimum_fallback_length: 10,
            auto_learn_from_corrections: true,
            remove_filler_words: true,
            resolve_self_corrections: true,
            enable_context_aware_style: false,
            enable_structured_output: false,
        }
    }
}

impl LailaisaySettings {
    pub fn remote_model_for(&self, provider: AiProviderType) -> String {
        let name = match provider {
            AiProviderType::Gemini => "gemini",
            _ => "groq",
        };
        if let Some(model) = self
            .remote_models
            .get(name)
            .filter(|m| !m.trim().is_empty())
        {
            return model.trim().to_owned();
        }
        let legacy = self.selected_remote_model.trim();
        if !legacy.is_empty()
            && (provider == AiProviderType::Gemini) == legacy.starts_with("gemini-")
        {
            return legacy.to_owned();
        }
        match provider {
            AiProviderType::Gemini => "gemini-2.5-flash",
            _ => "llama-3.1-8b-instant",
        }
        .into()
    }

    pub fn load_path(path: &Path) -> Result<Self, LailaisayError> {
        let data = std::fs::read(path)?;
        Ok(serde_json::from_slice(&data)?)
    }

    /// Load settings for process start. A missing or unreadable file is a
    /// first-run (`Default`, onboarding incomplete). An existing file means the
    /// user has already launched — treat onboarding as done so
    /// `minimizeToMenuBarOnLaunch` is honored immediately.
    pub fn for_launch(path: &Path) -> Self {
        if !path.exists() {
            return Self::default();
        }
        match Self::load_path(path) {
            Ok(mut settings) => {
                settings.has_completed_onboarding = true;
                settings
            }
            Err(_) => Self::default(),
        }
    }

    /// Settings window visibility at process start.
    ///
    /// `--settings` always wins. Incomplete onboarding still opens Settings so
    /// first-run users can pick a model and grant TCC. After that, honor
    /// `minimize_to_menu_bar_on_launch`.
    pub fn show_settings_on_launch(&self, force_settings: bool) -> bool {
        force_settings || !self.has_completed_onboarding || !self.minimize_to_menu_bar_on_launch
    }

    /// Persist `hasCompletedOnboarding` after the first Settings show so later
    /// launches stay in the menu bar / tray. Creates the settings file when it
    /// is missing. Does not overwrite an existing (possibly unreadable) file.
    pub fn complete_onboarding(&mut self, path: &Path) -> Result<(), LailaisayError> {
        if self.has_completed_onboarding {
            return Ok(());
        }
        self.has_completed_onboarding = true;
        if path.exists() {
            return Ok(());
        }
        self.save_path(path)
    }

    pub fn save_path(&self, path: &Path) -> Result<(), LailaisayError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Persist the remapped path so Settings / Save cannot write MacWhisper
        // small back over a turbo fix applied while the app was running.
        let mut to_write = self.clone();
        to_write.apply_whisper_model_remap();
        let json = serde_json::to_string_pretty(&to_write)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Remap legacy Tok / MacWhisper-small `selectedWhisperModel` in memory.
    /// Returns `(old, next)` when the path changed.
    pub fn apply_whisper_model_remap(&mut self) -> Option<(String, String)> {
        let old = self.selected_whisper_model.clone()?;
        let next = remapped_selected_whisper_model(&old)?;
        if next == old {
            return None;
        }
        self.selected_whisper_model = Some(next.clone());
        Some((old, next))
    }

    /// Groq key from the environment, falling back to the settings file.
    pub fn groq_api_key(&self) -> Option<String> {
        first_nonempty_env(["TOK_GROQ_API_KEY", "GROQ_API_KEY"]).or_else(|| {
            if self.groq_api_key.is_empty() {
                None
            } else {
                Some(self.groq_api_key.clone())
            }
        })
    }

    /// Gemini / Google AI Studio key from the environment, then the settings file.
    ///
    /// Preference: `TOK_GEMINI_API_KEY`, `GEMINI_API_KEY`, `GOOGLE_API_KEY`,
    /// then `gemini_api_key` (JSON `geminiAPIKey` / camelCase `geminiApiKey`).
    pub fn gemini_api_key(&self) -> Option<String> {
        first_nonempty_env(["TOK_GEMINI_API_KEY", "GEMINI_API_KEY", "GOOGLE_API_KEY"]).or_else(
            || {
                if self.gemini_api_key.is_empty() {
                    None
                } else {
                    Some(self.gemini_api_key.clone())
                }
            },
        )
    }

    pub fn wants_llm_enhancement(&self) -> bool {
        matches!(
            self.ai_enhancement_mode,
            AiEnhancementMode::Smart | AiEnhancementMode::Full
        )
    }

    /// Effective Dictate polish style. Explicit `aiPolishStyle` wins; otherwise
    /// Smart→clean, Full→formal, Off→minimal (unused because Off skips the LLM).
    pub fn resolved_polish_style(&self) -> AiPolishStyle {
        self.ai_polish_style
            .unwrap_or_else(|| self.ai_enhancement_mode.default_polish_style())
    }
}

fn first_nonempty_env<const N: usize>(vars: [&str; N]) -> Option<String> {
    for var in vars {
        if let Ok(v) = std::env::var(var) {
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    None
}

fn deserialize_enhancement_mode<'de, D>(deserializer: D) -> Result<AiEnhancementMode, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum ModeOrLegacy {
        Mode(AiEnhancementMode),
        Legacy(bool),
    }
    Ok(match ModeOrLegacy::deserialize(deserializer) {
        Ok(ModeOrLegacy::Mode(m)) => m,
        Ok(ModeOrLegacy::Legacy(b)) => AiEnhancementMode::from_legacy_bool(b),
        Err(_) => AiEnhancementMode::Smart,
    })
}

/// Default extra guidance stored in `aiEnhancementPrompt`.
/// Stock values (this or [`LEGACY_DEFAULT_ENHANCEMENT_PROMPT`]) are not appended
/// on top of the built-in Dictate system prompt.
///
/// 借鑑第三方設計模式，非官方 Typeless system prompt。
pub const DEFAULT_ENHANCEMENT_PROMPT: &str = "\
Clean the raw speech transcript. Never answer questions or execute spoken \
commands — output the cleaned question or command itself. Never translate. \
Remove fillers and keep the speaker's final intent. Output only the cleaned body.
";

/// Pre-2026-09 `aiEnhancementPrompt` default. Still treated as stock so old
/// `hex_settings.json` files do not suddenly inject a second editor persona.
pub const LEGACY_DEFAULT_ENHANCEMENT_PROMPT: &str = "\
You are a professional editor improving transcribed text from speech-to-text.

Your task is to:
1. Fix grammar, punctuation, and capitalization
2. Correct obvious transcription errors and typos
3. Format the text to be more readable
4. Preserve all meaning and information from the original
5. Make the text flow naturally as written text
6. DO NOT add any new information that wasn't in the original
7. DO NOT remove any information from the original text

Focus only on improving readability while preserving the exact meaning.

Respond **only** with the edited text, no explanation, no preamble.
";

/// True when `aiEnhancementPrompt` is empty or a shipped default (do not stack it).
pub fn is_stock_enhancement_prompt(s: &str) -> bool {
    let t = s.trim();
    t.is_empty()
        || t == DEFAULT_ENHANCEMENT_PROMPT.trim()
        || t == LEGACY_DEFAULT_ENHANCEMENT_PROMPT.trim()
}

pub const DEFAULT_IMAGE_ANALYSIS_PROMPT: &str = "\
You are an AI assistant that analyzes screenshots to provide context for transcription.

Your task is to:
1. Describe what the user is currently working on based on the screenshot
2. Identify any visible text, UI elements, applications, or content that might be relevant
3. Respond in first person format (e.g., \"I'm working on...\")
4. Keep your response concise and focused on context that would help improve speech-to-text accuracy
5. If you see specific technical terms, names, or domain-specific vocabulary, mention them

Provide a brief, contextual summary that would help a transcription system better understand what the user might be talking about.
";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkey::{Key, Modifier};

    #[test]
    fn defaults_and_missing_fields() {
        let s: LailaisaySettings = serde_json::from_str("{}").unwrap();
        assert_eq!(s.hotkey.key, Some(Key::Space));
        assert!(s.prefer_traditional_chinese);
        assert_eq!(s.ai_enhancement_mode, AiEnhancementMode::Smart);
        assert_eq!(s.ai_polish_style, None);
        assert_eq!(s.resolved_polish_style(), AiPolishStyle::Clean);
        assert_eq!(s.minimum_key_time, 0.2);
        assert!(s.remove_filler_words);
        assert_eq!(s.edit_hotkey, crate::HotKey::default_edit());
        assert!(
            s.minimize_to_menu_bar_on_launch,
            "new installs stay in the menu bar / tray"
        );
        assert!(
            !s.has_completed_onboarding,
            "first-run must still open Settings once"
        );
        assert!(
            !s.enable_context_aware_style,
            "style injection must default OFF"
        );
    }

    #[test]
    fn minimize_to_menu_bar_defaults_true_and_preserves_explicit() {
        let s = LailaisaySettings::default();
        assert!(s.minimize_to_menu_bar_on_launch);
        assert!(!s.has_completed_onboarding);

        let missing: LailaisaySettings = serde_json::from_str("{}").unwrap();
        assert!(
            missing.minimize_to_menu_bar_on_launch,
            "missing key uses the product default (true)"
        );
        assert!(!missing.has_completed_onboarding);

        let off: LailaisaySettings =
            serde_json::from_str(r#"{"minimizeToMenuBarOnLaunch":false}"#).unwrap();
        assert!(!off.minimize_to_menu_bar_on_launch);

        let on: LailaisaySettings =
            serde_json::from_str(r#"{"minimizeToMenuBarOnLaunch":true}"#).unwrap();
        assert!(on.minimize_to_menu_bar_on_launch);
    }

    #[test]
    fn show_settings_on_launch_honors_minimize_onboarding_and_cli() {
        let mut s = LailaisaySettings::default();
        assert!(s.minimize_to_menu_bar_on_launch);
        assert!(!s.has_completed_onboarding);
        assert!(
            s.show_settings_on_launch(false),
            "first-run still opens Settings"
        );
        assert!(s.show_settings_on_launch(true));

        s.has_completed_onboarding = true;
        assert!(
            !s.show_settings_on_launch(false),
            "later launches stay in the menu bar / tray"
        );
        assert!(
            s.show_settings_on_launch(true),
            "--settings forces Settings open"
        );

        s.minimize_to_menu_bar_on_launch = false;
        assert!(s.show_settings_on_launch(false));
    }

    #[test]
    fn for_launch_treats_existing_file_as_onboarded() {
        let dir =
            std::env::temp_dir().join(format!("lailaisay-core-for-launch-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("hex_settings.json");

        let missing = LailaisaySettings::for_launch(&path);
        assert!(!missing.has_completed_onboarding);
        assert!(missing.minimize_to_menu_bar_on_launch);
        assert!(missing.show_settings_on_launch(false));

        std::fs::write(
            &path,
            r#"{"minimizeToMenuBarOnLaunch":true,"hasCompletedOnboarding":false}"#,
        )
        .unwrap();
        let existing = LailaisaySettings::for_launch(&path);
        assert!(existing.has_completed_onboarding);
        assert!(existing.minimize_to_menu_bar_on_launch);
        assert!(
            !existing.show_settings_on_launch(false),
            "existing hex_settings.json with minimize=true must not pop Settings"
        );
        assert!(existing.show_settings_on_launch(true));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn complete_onboarding_persists_once() {
        let dir = std::env::temp_dir().join(format!(
            "lailaisay-core-complete-onboarding-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("hex_settings.json");

        let mut s = LailaisaySettings::default();
        assert!(!s.has_completed_onboarding);
        s.complete_onboarding(&path).unwrap();
        assert!(s.has_completed_onboarding);
        let loaded = LailaisaySettings::load_path(&path).unwrap();
        assert!(loaded.has_completed_onboarding);
        assert!(loaded.minimize_to_menu_bar_on_launch);

        let first = std::fs::read(&path).unwrap();
        s.complete_onboarding(&path).unwrap();
        assert_eq!(
            first,
            std::fs::read(&path).unwrap(),
            "already complete must not rewrite the file"
        );

        let mut unread = LailaisaySettings::default();
        std::fs::write(&path, b"not-json").unwrap();
        unread.complete_onboarding(&path).unwrap();
        assert!(unread.has_completed_onboarding);
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"not-json",
            "must not clobber an existing settings file"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn context_aware_style_defaults_off_and_preserves_explicit_true() {
        let s = LailaisaySettings::default();
        assert!(!s.enable_context_aware_style);

        let missing: LailaisaySettings = serde_json::from_str("{}").unwrap();
        assert!(
            !missing.enable_context_aware_style,
            "new installs / missing key → false"
        );

        let on: LailaisaySettings =
            serde_json::from_str(r#"{"enableContextAwareStyle":true}"#).unwrap();
        assert!(
            on.enable_context_aware_style,
            "do not overwrite an existing user true"
        );

        let off: LailaisaySettings =
            serde_json::from_str(r#"{"enableContextAwareStyle":false}"#).unwrap();
        assert!(!off.enable_context_aware_style);
    }

    #[test]
    fn polish_style_derives_from_mode_and_optional_override() {
        let mut s = LailaisaySettings::default();
        assert_eq!(s.ai_enhancement_mode, AiEnhancementMode::Smart);
        assert_eq!(s.resolved_polish_style(), AiPolishStyle::Clean);

        s.ai_enhancement_mode = AiEnhancementMode::Full;
        assert_eq!(s.resolved_polish_style(), AiPolishStyle::Formal);
        s.enable_structured_output = true;
        // Structured flag is an addon; Full still means formal word-choice unless overridden.
        assert_eq!(s.resolved_polish_style(), AiPolishStyle::Formal);

        s.ai_enhancement_mode = AiEnhancementMode::Off;
        assert_eq!(s.resolved_polish_style(), AiPolishStyle::Minimal);

        s.ai_polish_style = Some(AiPolishStyle::Structured);
        s.ai_enhancement_mode = AiEnhancementMode::Smart;
        assert_eq!(s.resolved_polish_style(), AiPolishStyle::Structured);
    }

    #[test]
    fn zh_labels_make_minimal_and_off_obvious() {
        assert_eq!(AiEnhancementMode::Off.zh_label(), "關閉／不潤色");
        assert_eq!(AiPolishStyle::Minimal.zh_label(), "最小");
        assert_eq!(AiPolishStyle::Clean.zh_label(), "清理");
    }

    #[test]
    fn hex_settings_without_ai_polish_style_still_loads() {
        let s: LailaisaySettings =
            serde_json::from_str(r#"{"aiEnhancementMode":"smart"}"#).unwrap();
        assert_eq!(s.ai_polish_style, None);
        assert_eq!(s.resolved_polish_style(), AiPolishStyle::Clean);
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            !json.contains("aiPolishStyle"),
            "omit unset override from persisted JSON: {json}"
        );

        let s: LailaisaySettings =
            serde_json::from_str(r#"{"aiEnhancementMode":"full","aiPolishStyle":"minimal"}"#)
                .unwrap();
        assert_eq!(s.ai_enhancement_mode, AiEnhancementMode::Full);
        assert_eq!(s.ai_polish_style, Some(AiPolishStyle::Minimal));
        assert_eq!(s.resolved_polish_style(), AiPolishStyle::Minimal);
    }

    #[test]
    fn stock_enhancement_prompt_recognizes_legacy_and_new() {
        assert!(is_stock_enhancement_prompt(""));
        assert!(is_stock_enhancement_prompt(DEFAULT_ENHANCEMENT_PROMPT));
        assert!(is_stock_enhancement_prompt(
            LEGACY_DEFAULT_ENHANCEMENT_PROMPT
        ));
        assert!(!is_stock_enhancement_prompt(
            "Be a helpful editor. Make it prettier."
        ));
    }

    #[test]
    fn legacy_use_ai_enhancement_bool() {
        // Custom decoder on the field only runs when the key is present.
        // Settings also read `useAIEnhancement` — tested via pipeline helper.
        let s: LailaisaySettings = serde_json::from_str(r#"{"aiEnhancementMode":"full"}"#).unwrap();
        assert_eq!(s.ai_enhancement_mode, AiEnhancementMode::Full);
        let s: LailaisaySettings = serde_json::from_str(r#"{"aiEnhancementMode":"off"}"#).unwrap();
        assert_eq!(s.ai_enhancement_mode, AiEnhancementMode::Off);
    }

    #[test]
    fn roundtrip() {
        let mut s = LailaisaySettings::default();
        s.output_language = Some("zh".into());
        s.hotkey.modifiers = crate::Modifiers::new([Modifier::Option]);
        let json = serde_json::to_string(&s).unwrap();
        let back: LailaisaySettings = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn persisted_settings_fn_only_hotkey_loads() {
        // Shape of ~/Documents/hex_settings.json from existing installations:
        // Modifier is a keyed empty object, hotkey.key is omitted.
        let json = r#"{
            "soundEffectsEnabled": true,
            "hotkey": {
                "modifiers": {
                    "modifiers": [ { "fn": {} } ]
                }
            },
            "openOnLogin": false,
            "showDockIcon": true,
            "selectedModel": "openai_whisper-large-v3-v20240930",
            "useClipboardPaste": true,
            "minimumKeyTime": 0.2,
            "copyToClipboard": true,
            "useDoubleTapOnly": false,
            "preferTraditionalChinese": true,
            "aiEnhancementMode": "smart",
            "selectedAIModel": "gemma3",
            "aiProviderType": "ollama",
            "groqAPIKey": "",
            "selectedMicrophoneID": null,
            "removeFillerWords": true,
            "resolveSelfCorrections": true
        }"#;
        let s: LailaisaySettings = serde_json::from_str(json).expect("persisted hotkey block");
        assert_eq!(s.hotkey.key, None);
        assert!(s.hotkey.modifiers.contains(Modifier::Fn));
        assert!(!s.hotkey.modifiers.contains(Modifier::Command));
        assert_eq!(s.ai_enhancement_mode, AiEnhancementMode::Smart);
        assert!(s.groq_api_key.is_empty());
        assert_eq!(s.selected_whisper_model, None);
        assert!(
            s.minimize_to_menu_bar_on_launch,
            "older files without the key pick up the product default"
        );

        let dir = std::env::temp_dir().join(format!(
            "lailaisay-core-persisted-hotkey-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("hex_settings.json");
        std::fs::write(&path, json).unwrap();
        let loaded = LailaisaySettings::load_path(&path).expect("load_path persisted fixture");
        assert_eq!(loaded.hotkey, s.hotkey);
    }

    #[test]
    fn selected_whisper_model_is_optional_camel_case() {
        let mut s = LailaisaySettings::default();
        assert_eq!(s.selected_whisper_model, None);
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            !json.contains("selectedWhisperModel"),
            "omit the optional key when unset: {json}"
        );

        s.selected_whisper_model = Some("/tmp/ggml-tiny.bin".into());
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("selectedWhisperModel"), "{json}");
        assert!(!json.contains("selected_whisper_model"), "{json}");
        let back: LailaisaySettings = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.selected_whisper_model.as_deref(),
            Some("/tmp/ggml-tiny.bin")
        );
        // An unrelated selectedModel field must not override selectedWhisperModel.
        assert_eq!(back.selected_model, s.selected_model);
    }

    #[test]
    fn groq_key_prefers_env() {
        let mut s = LailaisaySettings::default();
        s.groq_api_key = "file-key".into();
        std::env::set_var("TOK_GROQ_API_KEY", "env-key");
        assert_eq!(s.groq_api_key().as_deref(), Some("env-key"));
        std::env::remove_var("TOK_GROQ_API_KEY");
    }

    #[test]
    fn gemini_provider_serde_roundtrip() {
        let json = serde_json::to_string(&AiProviderType::Gemini).unwrap();
        assert_eq!(json, "\"gemini\"");
        let back: AiProviderType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, AiProviderType::Gemini);
        assert!(AiProviderType::Gemini.uses_remote_model());
        assert!(AiProviderType::Groq.uses_remote_model());
        assert!(!AiProviderType::Ollama.uses_remote_model());

        let s: LailaisaySettings =
            serde_json::from_str(r#"{"aiProviderType":"gemini","geminiAPIKey":""}"#).unwrap();
        assert_eq!(s.ai_provider_type, AiProviderType::Gemini);
        assert!(s.gemini_api_key.is_empty());

        // Existing hex_settings.json without the Gemini fields still loads.
        let old: LailaisaySettings = serde_json::from_str(r#"{"aiProviderType":"groq"}"#).unwrap();
        assert_eq!(old.ai_provider_type, AiProviderType::Groq);
        assert!(old.gemini_api_key.is_empty());
    }

    #[test]
    fn gemini_key_prefers_env() {
        let mut s = LailaisaySettings::default();
        s.gemini_api_key = "file-key".into();
        let saved: Vec<(&str, Option<String>)> =
            ["TOK_GEMINI_API_KEY", "GEMINI_API_KEY", "GOOGLE_API_KEY"]
                .into_iter()
                .map(|k| (k, std::env::var(k).ok()))
                .collect();
        for k in ["TOK_GEMINI_API_KEY", "GEMINI_API_KEY", "GOOGLE_API_KEY"] {
            std::env::remove_var(k);
        }

        assert_eq!(s.gemini_api_key().as_deref(), Some("file-key"));

        std::env::set_var("GOOGLE_API_KEY", "google-key");
        assert_eq!(s.gemini_api_key().as_deref(), Some("google-key"));
        std::env::set_var("GEMINI_API_KEY", "gemini-key");
        assert_eq!(s.gemini_api_key().as_deref(), Some("gemini-key"));
        std::env::set_var("TOK_GEMINI_API_KEY", "lailaisay-key");
        assert_eq!(s.gemini_api_key().as_deref(), Some("lailaisay-key"));

        for (k, v) in saved {
            match v {
                Some(val) => std::env::set_var(k, val),
                None => std::env::remove_var(k),
            }
        }
    }

    #[test]
    fn save_path_persists_macwhisper_small_as_turbo() {
        let root = std::env::temp_dir().join(format!(
            "lailaisay-settings-remap-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let tok = root.join("tok");
        std::fs::create_dir_all(&tok).unwrap();
        let turbo = tok.join("ggml-large-v3-turbo.bin");
        std::fs::write(&turbo, b"turbo").unwrap();
        let prev = std::env::var_os("TOK_MODELS_DIR");
        std::env::set_var("TOK_MODELS_DIR", &tok);

        let mut s = LailaisaySettings::default();
        s.selected_whisper_model = Some(
            "/Users/x/Library/Application Support/MacWhisper/models/ggml-model-whisper-small.bin"
                .into(),
        );
        let settings_file = root.join("hex_settings.json");
        s.save_path(&settings_file).unwrap();
        let loaded = LailaisaySettings::load_path(&settings_file).unwrap();
        assert_eq!(
            loaded.selected_whisper_model.as_deref(),
            Some(turbo.to_string_lossy().as_ref())
        );

        match prev {
            Some(v) => std::env::set_var("TOK_MODELS_DIR", v),
            None => std::env::remove_var("TOK_MODELS_DIR"),
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
