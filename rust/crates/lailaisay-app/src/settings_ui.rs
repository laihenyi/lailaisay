//! lailaisay Settings: left rail + one pane. Tokens from [`crate::theme`]. JSON keys stay English.

use std::path::{Path, PathBuf};
use std::time::Duration;

use egui::{
    Align, Align2, Area, Color32, CornerRadius, CursorIcon, FontFamily, FontId, Frame, Id, Layout,
    Order, Popup, Pos2, Rect, Sense, Stroke, TextStyle, TextWrapMode, Ui, Vec2, ViewportCommand,
    WidgetText,
};
use lailaisay_core::{
    is_unsafe_digit_list_to_punctuation, remapped_selected_whisper_model, AiEnhancementMode,
    AiPolishStyle, AiProviderType, CustomWordDictionary, CustomWordEntry, HotKey, Key,
    LailaisaySettings, Modifier,
};
use lailaisay_input::GrantStatus;

use crate::models::{
    catalog_by_id, catalog_dest, catalog_is_downloaded, friendly_model_label, list_usable_models,
    resolve_whisper_model, CatalogEntry, FoundModel, CATALOG,
};
use crate::theme::{
    self, BORDER, DANGER, FG, GLASS_EDGE, GLASS_FILL, GLASS_FILL_DEEP, GLASS_HIGHLIGHT,
    GLASS_HOVER, GLASS_TINT, MUTED, RAIL_W, ROUND_CTL, ROW_H, SUCCESS, WARN,
};

/// Default settings size; both axes can grow.
///
/// Dense panes scroll independently; the window may resize in both directions.
pub(crate) const SETTINGS_INNER_SIZE: [f32; 2] = [760.0, 760.0];
/// Slightly smaller than the design size on cramped displays.
pub(crate) const SETTINGS_MIN_SIZE: [f32; 2] = [640.0, 600.0];
/// Allow users to widen dense panes without clipping.
pub(crate) const SETTINGS_MAX_SIZE: [f32; 2] = [1400.0, 1400.0];
/// In-chrome titlebar (`lailaisay 設定` at 13pt) above the status strip.
const SETTINGS_TITLEBAR_H: f32 = 32.0;
/// Left safe inset: no drag/title widgets over macOS traffic lights (~12pt
/// discs, 8pt gaps, 8pt leading pad → occupied width 60, plus slack).
const TRAFFIC_LIGHT_INSET: f32 = 78.0;
const TRAFFIC_LIGHT_DIAMETER: f32 = 12.0;
const TRAFFIC_LIGHT_GAP: f32 = 8.0;
const TRAFFIC_LIGHT_LEFT_PAD: f32 = 8.0;
/// Hit target is slightly larger than the 12pt disc.
const TRAFFIC_LIGHT_HIT: f32 = 16.0;
/// Status strip `exact_height` (chip 28 + vertical inset 8+8).
const SETTINGS_STATUS_H: f32 = 44.0;
/// Footer `exact_height` when `save_message` is empty.
const SETTINGS_FOOTER_H: f32 = 82.0;

pub struct SettingsView<'a> {
    pub live_status: &'a str,
    pub stt_note: &'a str,
    pub last_llm: Option<&'a str>,
    /// Pre-LLM local pipeline text (STT + filters). Session-only.
    pub last_raw: &'a str,
    /// Last pasted / polished body. Session-only.
    pub last_final: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    General,
    Whisper,
    Ai,
    Dict,
    Perms,
}

impl SettingsSection {
    const ALL: [Self; 5] = [
        Self::General,
        Self::Whisper,
        Self::Ai,
        Self::Dict,
        Self::Perms,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::General => "一般",
            Self::Whisper => "語音模型",
            Self::Ai => "AI 潤稿",
            Self::Dict => "自訂辭典",
            Self::Perms => "權限",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayGlyph {
    Idle,
    Active,
    Error,
}

pub struct SettingsForm {
    pub language: String,
    pub prefer_traditional_chinese: bool,
    pub ai_mode: AiEnhancementMode,
    pub ai_polish_style: AiPolishStyle,
    pub ai_provider: AiProviderType,
    pub selected_ai_model: String,
    pub selected_remote_model: String,
    /// File-backed Groq key (`groqApiKey`). Env `TOK_GROQ_API_KEY` still wins at runtime.
    pub groq_api_key: String,
    /// File-backed Gemini key (`geminiApiKey`). Env still wins at runtime.
    pub gemini_api_key: String,
    pub ollama_models: Vec<String>,
    pub ollama_reachable: Option<bool>,
    pub ollama_refresh_message: String,
    pub use_double_tap_only: bool,
    pub show_dock_icon: bool,
    pub minimize_to_menu_bar_on_launch: bool,
    pub minimum_key_time: f64,
    pub selected_model: String,
    pub local_models: Vec<FoundModel>,
    pub selected_catalog_id: String,
    pub save_message: String,
    pub download_message: String,
    pub downloading: bool,
    pub pane: SettingsSection,
    hotkey: HotKey,
    remote_models: std::collections::BTreeMap<String, String>,
    dictionary_search: String,
    pub(crate) dictionary_undo: Option<CustomWordDictionary>,
    editing_entry: Option<usize>,
    edit_hotkey: HotKey,
    pub enable_structured_output: bool,
    pub auto_learn_from_corrections: bool,
    pub dictionary: CustomWordDictionary,
    pub new_original: String,
    pub new_replacement: String,
    /// Linux fallback when zenity/kdialog is missing. Not persistable.
    pub dict_file_path: String,
    /// Last successfully loaded/saved persistable fields. Dictionary edits are excluded.
    saved: PersistableSnapshot,
    permissions_open_message: String,
}

/// Fields `apply_to` / Save write to `hex_settings.json`. Catalog + dictionary are not here.
#[derive(Debug, Clone, PartialEq, Default)]
struct PersistableSnapshot {
    hotkey: HotKey,
    edit_hotkey: HotKey,
    remote_models: std::collections::BTreeMap<String, String>,
    language: String,
    prefer_traditional_chinese: bool,
    ai_mode: AiEnhancementMode,
    ai_polish_style: AiPolishStyle,
    ai_provider: AiProviderType,
    selected_ai_model: String,
    selected_remote_model: String,
    groq_api_key: String,
    gemini_api_key: String,
    use_double_tap_only: bool,
    show_dock_icon: bool,
    minimize_to_menu_bar_on_launch: bool,
    minimum_key_time: f64,
    selected_model: String,
    enable_structured_output: bool,
    auto_learn_from_corrections: bool,
}

/// macOS Privacy & Security panes opened from the 權限 list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyPane {
    Accessibility,
    Microphone,
    InputMonitoring,
    Automation,
}

impl PrivacyPane {
    /// Ventura+ / Sequoia / Tahoe Privacy & Security extension URL.
    pub fn modern_url(self) -> &'static str {
        match self {
            Self::Accessibility => {
                "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Accessibility"
            }
            Self::Microphone => {
                "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Microphone"
            }
            Self::InputMonitoring => {
                "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_ListenEvent"
            }
            Self::Automation => {
                "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Automation"
            }
        }
    }

    /// Pre-Ventura System Preferences fallback (`open` still accepts these on many builds).
    pub fn legacy_url(self) -> &'static str {
        match self {
            Self::Accessibility => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
            }
            Self::Microphone => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
            }
            Self::InputMonitoring => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"
            }
            Self::Automation => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Automation"
            }
        }
    }
}

/// Modern URL first, then the legacy Security pane. Input Monitoring uses `Privacy_ListenEvent`.
pub fn privacy_settings_urls(pane: PrivacyPane) -> [&'static str; 2] {
    [pane.modern_url(), pane.legacy_url()]
}

/// `open` the matching Privacy pane. macOS only; Linux returns an error without spawning.
pub fn open_macos_privacy_settings(pane: PrivacyPane) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let mut last_err = String::from("open failed");
        for url in privacy_settings_urls(pane) {
            match std::process::Command::new("open").arg(url).status() {
                Ok(status) if status.success() => return Ok(()),
                Ok(status) => last_err = format!("open exited {status}"),
                Err(e) => last_err = e.to_string(),
            }
        }
        Err(last_err)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = pane;
        Err("僅 macOS 可開啟系統設定".into())
    }
}

impl SettingsForm {
    pub fn from_settings(settings: &LailaisaySettings) -> Self {
        let resolved = resolve_whisper_model(settings);
        let selected_model = settings
            .selected_whisper_model
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| resolved.map(|p| p.to_string_lossy().into_owned()))
            .unwrap_or_default();
        // Show the remapped production path so Settings does not still list
        // MacWhisper small after load remapped it.
        let selected_model =
            remapped_selected_whisper_model(&selected_model).unwrap_or(selected_model);
        let mut form = Self {
            language: settings.output_language.clone().unwrap_or_default(),
            prefer_traditional_chinese: settings.prefer_traditional_chinese,
            ai_mode: settings.ai_enhancement_mode,
            ai_polish_style: settings.resolved_polish_style(),
            ai_provider: settings.ai_provider_type,
            selected_ai_model: settings.selected_ai_model.clone(),
            selected_remote_model: settings.remote_model_for(settings.ai_provider_type),
            groq_api_key: settings.groq_api_key.clone(),
            gemini_api_key: settings.gemini_api_key.clone(),
            ollama_models: Vec::new(),
            ollama_reachable: None,
            ollama_refresh_message: String::new(),
            use_double_tap_only: settings.use_double_tap_only,
            show_dock_icon: settings.show_dock_icon,
            minimize_to_menu_bar_on_launch: settings.minimize_to_menu_bar_on_launch,
            minimum_key_time: settings.minimum_key_time,
            selected_model,
            local_models: Vec::new(),
            selected_catalog_id: "small".into(),
            save_message: String::new(),
            download_message: String::new(),
            downloading: false,
            pane: SettingsSection::General,
            hotkey: settings.hotkey.clone(),
            remote_models: [
                (
                    "groq".into(),
                    settings.remote_model_for(AiProviderType::Groq),
                ),
                (
                    "gemini".into(),
                    settings.remote_model_for(AiProviderType::Gemini),
                ),
            ]
            .into(),
            dictionary_search: String::new(),
            dictionary_undo: None,
            editing_entry: None,
            edit_hotkey: settings.edit_hotkey.clone(),
            enable_structured_output: settings.enable_structured_output,
            auto_learn_from_corrections: settings.auto_learn_from_corrections,
            dictionary: CustomWordDictionary::default(),
            new_original: String::new(),
            new_replacement: String::new(),
            dict_file_path: String::new(),
            saved: PersistableSnapshot::default(),
            permissions_open_message: String::new(),
        };
        form.refresh_models();
        form.mark_clean();
        form
    }

    fn persistable_snapshot(&self) -> PersistableSnapshot {
        let language = self.language.trim().to_string();
        let selected_ai_model = {
            let ai = self.selected_ai_model.trim();
            if ai.is_empty() {
                "gemma3".into()
            } else {
                ai.to_string()
            }
        };
        let selected_remote_model = self.selected_remote_model.trim().to_string();
        PersistableSnapshot {
            hotkey: self.hotkey.clone(),
            edit_hotkey: self.edit_hotkey.clone(),
            remote_models: self.current_remote_models(),
            language,
            prefer_traditional_chinese: self.prefer_traditional_chinese,
            ai_mode: self.ai_mode,
            ai_polish_style: self.ai_polish_style,
            ai_provider: self.ai_provider,
            selected_ai_model,
            selected_remote_model,
            groq_api_key: self.groq_api_key.trim().to_string(),
            gemini_api_key: self.gemini_api_key.trim().to_string(),
            use_double_tap_only: self.use_double_tap_only,
            show_dock_icon: self.show_dock_icon,
            minimize_to_menu_bar_on_launch: self.minimize_to_menu_bar_on_launch,
            minimum_key_time: self.minimum_key_time,
            selected_model: self.selected_model.trim().to_string(),
            enable_structured_output: self.enable_structured_output,
            auto_learn_from_corrections: self.auto_learn_from_corrections,
        }
    }

    /// Call after a successful settings Save (not after SaveDictionary).
    pub fn mark_clean(&mut self) {
        self.saved = self.persistable_snapshot();
    }

    pub fn is_dirty(&self) -> bool {
        self.persistable_snapshot() != self.saved
    }

    fn current_remote_models(&self) -> std::collections::BTreeMap<String, String> {
        let mut models = self.remote_models.clone();
        if self.ai_provider.uses_remote_model() {
            models.insert(
                provider_key(self.ai_provider).into(),
                self.selected_remote_model.trim().into(),
            );
        }
        models
    }

    fn switch_provider(&mut self, previous: AiProviderType) {
        if previous.uses_remote_model() {
            self.remote_models.insert(
                provider_key(previous).into(),
                self.selected_remote_model.clone(),
            );
        }
        if self.ai_provider.uses_remote_model() {
            self.selected_remote_model = self
                .remote_models
                .get(provider_key(self.ai_provider))
                .cloned()
                .unwrap_or_default();
        }
    }

    pub fn validation_error(&self) -> Option<&'static str> {
        if self.hotkey == self.edit_hotkey {
            return Some("兩組快捷鍵不可相同");
        }
        if self.hotkey.modifiers.is_empty() || self.edit_hotkey.modifiers.is_empty() {
            return Some("快捷鍵至少需要一個修飾鍵");
        }
        if self.ai_mode != AiEnhancementMode::Off && self.ai_provider.uses_remote_model() {
            let model = self.selected_remote_model.trim();
            if model.is_empty() {
                return Some("請輸入模型名稱");
            }
            if (self.ai_provider == AiProviderType::Gemini) != model.starts_with("gemini-") {
                return Some("模型與服務不相容，請確認模型名稱");
            }
        }
        None
    }

    pub fn apply_to(&self, settings: &mut LailaisaySettings) {
        settings.hotkey = self.hotkey.clone();
        settings.edit_hotkey = self.edit_hotkey.clone();
        settings.remote_models = self.current_remote_models();
        let lang = self.language.trim();
        settings.output_language = if lang.is_empty() {
            None
        } else {
            Some(lang.to_string())
        };
        settings.prefer_traditional_chinese = self.prefer_traditional_chinese;
        settings.ai_enhancement_mode = self.ai_mode;
        // Off = no LLM (不潤色). Persist `aiPolishStyle` only when it overrides
        // the mode default so hex_settings camelCase stays familiar.
        settings.ai_polish_style = if self.ai_mode == AiEnhancementMode::Off
            || self.ai_polish_style == self.ai_mode.default_polish_style()
        {
            None
        } else {
            Some(self.ai_polish_style)
        };
        settings.ai_provider_type = self.ai_provider;
        // Ollama persists `selectedAIModel`. Groq and Gemini persist
        // `selectedRemoteModel` so switching providers does not clobber the
        // local model name (the Groq UI previously wrote into the Ollama field).
        let ai = self.selected_ai_model.trim();
        settings.selected_ai_model = if ai.is_empty() {
            "gemma3".into()
        } else {
            ai.to_string()
        };
        let remote = self.selected_remote_model.trim();
        if self.ai_provider.uses_remote_model() && !remote.is_empty() {
            settings.selected_remote_model = remote.to_string();
        }
        settings.groq_api_key = self.groq_api_key.trim().to_string();
        settings.gemini_api_key = self.gemini_api_key.trim().to_string();
        settings.use_double_tap_only = self.use_double_tap_only;
        settings.show_dock_icon = self.show_dock_icon;
        settings.minimize_to_menu_bar_on_launch = self.minimize_to_menu_bar_on_launch;
        settings.has_completed_onboarding = true;
        settings.minimum_key_time = self.minimum_key_time;
        settings.enable_structured_output = self.enable_structured_output;
        settings.auto_learn_from_corrections = self.auto_learn_from_corrections;
        settings.selected_whisper_model = if self.selected_model.trim().is_empty() {
            None
        } else {
            Some(self.selected_model.trim().to_string())
        };
        // Save must not persist MacWhisper small over a turbo fix.
        settings.apply_whisper_model_remap();
    }

    pub fn refresh_models(&mut self) {
        self.local_models = list_usable_models();
        if !self.selected_model.is_empty()
            && !self
                .local_models
                .iter()
                .any(|m| m.path.to_string_lossy() == self.selected_model)
        {
            let extra = PathBuf::from(&self.selected_model);
            if extra.is_file() {
                self.local_models.push(FoundModel {
                    path: extra,
                    source: crate::models::ModelSource::Local,
                });
            }
        }
    }

    pub fn catalog_entry(&self) -> Option<&'static CatalogEntry> {
        catalog_by_id(&self.selected_catalog_id)
    }

    pub fn load_dictionary(&mut self, dict: &CustomWordDictionary) {
        self.dictionary = dict.clone();
        self.editing_entry = None;
        self.new_original.clear();
        self.new_replacement.clear();
    }

    pub fn hotkey_label(&self) -> String {
        format_hotkey(&self.hotkey)
    }

    pub fn edit_hotkey_label(&self) -> String {
        format_hotkey(&self.edit_hotkey)
    }

    /// Full window chrome: in-chrome title, status chip, rail, one pane, footer.
    pub fn show_window(&mut self, ctx: &egui::Context, view: &SettingsView<'_>) -> FormAction {
        let mut action = FormAction::None;
        let hold = self.hotkey_label();
        let whisper_name = self.whisper_short_name();
        let ai_name = active_ai_model_label(
            self.ai_provider,
            &self.selected_ai_model,
            &self.selected_remote_model,
        );
        let detail = status_detail(view.live_status, &hold, &whisper_name, &ai_name);

        paint_settings_titlebar(ctx);

        let status = egui::TopBottomPanel::top("status_bar")
            .exact_height(SETTINGS_STATUS_H)
            .frame(
                // Fill only: the 1px titlebar/status divider is drawn on the
                // titlebar. A chrome_panel_frame stroke here would make it 2px.
                Frame::new()
                    .fill(GLASS_FILL)
                    .inner_margin(egui::Margin::symmetric(14, 8)),
            )
            .show(ctx, |ui| {
                paint_status_strip(ui, view.live_status, &detail);
            });
        paint_chrome_highlight(ctx, status.response.layer_id, status.response.rect);

        if self.is_dirty() && self.save_message.starts_with("已儲存") {
            self.save_message.clear();
        }

        let foot = egui::TopBottomPanel::bottom("settings_foot")
            .exact_height(SETTINGS_FOOTER_H)
            .frame(theme::chrome_panel_frame().inner_margin(egui::Margin::symmetric(14, 10)))
            .show(ctx, |ui| {
                action = merge_action(action, self.show_footer(ui));
            });
        paint_chrome_highlight(ctx, foot.response.layer_id, foot.response.rect);

        let rail = egui::SidePanel::left("settings_rail")
            .exact_width(RAIL_W)
            .resizable(false)
            .frame(theme::chrome_panel_frame().inner_margin(egui::Margin::symmetric(8, 10)))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.add_space(4.0);
                for section in SettingsSection::ALL {
                    if rail_item(ui, section.label(), self.pane == section).clicked() {
                        self.pane = section;
                    }
                }
            });
        paint_chrome_highlight(ctx, rail.response.layer_id, rail.response.rect);

        egui::CentralPanel::default()
            .frame(Frame::new().fill(theme::BG).inner_margin(0))
            .show(ctx, |ui| {
                theme::content_pane_frame().show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt(("lailaisay_settings_pane", self.pane.label()))
                        .auto_shrink([false, false])
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                        )
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            action = merge_action(action, self.show_pane(ui, view));
                        });
                });
            });

        crate::status_hud::paint_recording_hud(ctx, view.live_status);
        action
    }

    fn show_pane(&mut self, ui: &mut Ui, view: &SettingsView<'_>) -> FormAction {
        let action = match self.pane {
            SettingsSection::General => self.show_general(ui),
            SettingsSection::Whisper => self.show_whisper(ui, view.stt_note),
            SettingsSection::Ai => self.show_ai(ui, view),
            SettingsSection::Dict => self.show_dictionary(ui),
            SettingsSection::Perms => {
                self.show_permissions(ui);
                FormAction::None
            }
        };
        mark_pane_end(ui, self.pane);
        action
    }

    fn show_general(&mut self, ui: &mut Ui) -> FormAction {
        pane_heading(
            ui,
            "一般",
            "設定說話方式與輸出偏好；修改後按「儲存設定」生效。",
        );

        ui.label(egui::RichText::new("按住說話").size(14.0).color(FG));
        note(ui, "對準輸入框按住，放開後貼上。");
        hotkey_editor(ui, "dictation_hotkey", &mut self.hotkey);
        ui.add_space(6.0);

        ui.label(egui::RichText::new("選取改寫").size(14.0).color(FG));
        note(ui, "先選文字，再說「精簡一點」。");
        hotkey_editor(ui, "edit_hotkey", &mut self.edit_hotkey);
        if let Some(error) = self.validation_error() {
            danger_note(ui, error);
        }
        ui.add_space(12.0);

        ui.label(egui::RichText::new("輸出語言").size(14.0).color(FG));
        show_pane_combo(
            ui,
            "output_language",
            language_label(&self.language),
            220.0,
            |ui| {
                for (code, label) in LANGUAGE_CHOICES {
                    if ui
                        .selectable_label(self.language == *code, *label)
                        .clicked()
                    {
                        self.language = (*code).to_string();
                    }
                }
                if !self.language.is_empty()
                    && !LANGUAGE_CHOICES.iter().any(|(c, _)| *c == self.language)
                {
                    let _ = ui.selectable_label(true, format!("自訂：{}", self.language));
                }
            },
        );
        ui.add_space(4.0);
        switch_row(ui, &mut self.prefer_traditional_chinese, "優先繁體中文");
        note(ui, "輸出偏向 zh-TW，不預設譯成英文。");
        ui.add_space(4.0);
        switch_row(ui, &mut self.use_double_tap_only, "僅雙擊鎖定");
        note(ui, "避免誤觸按住說話。");
        ui.add_space(4.0);
        switch_row(ui, &mut self.show_dock_icon, "顯示 Dock 圖示");
        note(ui, "關閉後只留選單列圖示。");
        ui.add_space(4.0);
        switch_row(
            ui,
            &mut self.minimize_to_menu_bar_on_launch,
            MINIMIZE_ON_LAUNCH_LABEL,
        );
        note(ui, "關閉後每次啟動都會打開設定視窗。");
        ui.add_space(4.0);
        min_hold_row(ui, &mut self.minimum_key_time);
        FormAction::None
    }

    fn show_whisper(&mut self, ui: &mut Ui, stt_note: &str) -> FormAction {
        let mut action = FormAction::None;
        pane_heading(
            ui,
            "語音模型",
            "語音在本機辨識。選擇模型後儲存，背景載入完成即可使用。",
        );

        ui.label(egui::RichText::new("使用模型").size(14.0).color(FG));
        let selected_text = if self.selected_model.is_empty() {
            "（無 / dummy）".to_string()
        } else {
            self.local_models
                .iter()
                .find(|m| m.path.to_string_lossy() == self.selected_model)
                .map(friendly_model_label)
                .unwrap_or_else(|| {
                    PathBuf::from(&self.selected_model)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| self.selected_model.clone())
                })
        };
        show_pane_combo(
            ui,
            "whisper_model",
            egui::RichText::new(selected_text),
            460.0,
            |ui| {
                if ui
                    .selectable_label(self.selected_model.is_empty(), "（無 / dummy）")
                    .clicked()
                {
                    self.selected_model.clear();
                }
                for model in &self.local_models {
                    let label = friendly_model_label(model);
                    let value = model.path.to_string_lossy();
                    if ui
                        .selectable_label(self.selected_model == value, egui::RichText::new(label))
                        .on_hover_text(tilde_home_path(&model.path))
                        .clicked()
                    {
                        self.selected_model = value.into_owned();
                    }
                }
            },
        );
        if let Some(loaded) = resident_load_note(stt_note, &self.local_models, &self.selected_model)
        {
            note(ui, &loaded);
        }
        if let Some(device) = stt_note.strip_prefix("device: ") {
            note(ui, device);
        }

        ui.add_space(10.0);
        ui.label(egui::RichText::new("目錄下載").size(14.0).color(FG));
        let catalog_text = self
            .catalog_entry()
            .map(catalog_row_label)
            .unwrap_or_else(|| self.selected_catalog_id.clone());
        show_pane_combo(ui, "catalog_model", catalog_text, 300.0, |ui| {
            for entry in CATALOG {
                if ui
                    .selectable_label(
                        self.selected_catalog_id == entry.id,
                        catalog_row_label(entry),
                    )
                    .clicked()
                {
                    self.selected_catalog_id = entry.id.to_string();
                }
            }
        });
        ui.horizontal(|ui| {
            if secondary_button(ui, "重新整理列表").clicked() {
                self.refresh_models();
                self.download_message =
                    format!("已重新整理，找到 {} 個可用模型", self.local_models.len());
            }
            let download_enabled =
                !self.downloading && catalog_by_id(&self.selected_catalog_id).is_some();
            let dest_hint = self
                .catalog_entry()
                .map(|e| tilde_home_path(&catalog_dest(e)))
                .unwrap_or_default();
            ui.add_enabled_ui(download_enabled, |ui| {
                if secondary_button(ui, "下載所選")
                    .on_hover_text(dest_hint)
                    .clicked()
                {
                    action = FormAction::DownloadCatalog;
                }
            });
        });
        success_note(ui, &model_inventory_note(self.local_models.len()));
        if !self.download_message.is_empty() {
            note(ui, &self.download_message);
        }

        action
    }

    fn show_ai(&mut self, ui: &mut Ui, view: &SettingsView<'_>) -> FormAction {
        let mut action = FormAction::None;
        pane_heading(ui, "AI 潤稿", "選擇文字整理程度，並設定使用的 AI 服務。");
        let mut choice = if self.ai_mode == AiEnhancementMode::Off {
            None
        } else {
            Some(self.ai_polish_style)
        };
        let before = choice;
        ui.scope(|ui| {
            ui.spacing_mut().interact_size.y = 26.0;
            ui.spacing_mut().item_spacing.y = 4.0;
            for (value, title, detail) in [
                (None, "關閉", "只做本機文字清理，不呼叫 AI"),
                (
                    Some(AiPolishStyle::Minimal),
                    "修正標點",
                    "保留原句，只修正標點與雜訊",
                ),
                (
                    Some(AiPolishStyle::Clean),
                    "輕度清理",
                    "移除口頭禪與改口，保留原本用詞",
                ),
                (
                    Some(AiPolishStyle::Structured),
                    "整理結構",
                    "整理句子與段落，保留原意",
                ),
                (
                    Some(AiPolishStyle::Formal),
                    "正式改寫",
                    "調整措辭，適合信件與正式文字",
                ),
            ] {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut choice, value, title);
                    note(ui, detail);
                });
            }
        });
        if choice != before {
            self.ai_mode = match choice {
                None => AiEnhancementMode::Off,
                Some(AiPolishStyle::Formal) => AiEnhancementMode::Full,
                _ => AiEnhancementMode::Smart,
            };
            if let Some(style) = choice {
                self.ai_polish_style = style;
            }
        }
        ui.add_space(12.0);
        ui.separator();
        ui.label(egui::RichText::new("服務設定").strong().size(15.0));
        let provider_before = self.ai_provider;
        ui.label(egui::RichText::new("來源").size(14.0).color(FG));
        segmented(
            ui,
            "ai_provider",
            &mut self.ai_provider,
            &[
                (AiProviderType::Ollama, "Ollama"),
                (AiProviderType::Groq, "Groq"),
                (AiProviderType::Gemini, "Gemini"),
            ],
        );
        if self.ai_provider != provider_before {
            self.switch_provider(provider_before);
        }
        ui.add_space(10.0);

        if self.ai_provider == AiProviderType::Ollama {
            ui.label(egui::RichText::new("模型").size(14.0).color(FG));
            ui.horizontal(|ui| {
                let combo_text = if self.selected_ai_model.trim().is_empty() {
                    "gemma3".to_string()
                } else {
                    self.selected_ai_model.clone()
                };
                if self.ollama_models.is_empty() {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.selected_ai_model)
                            .desired_width(200.0)
                            .hint_text("gemma3")
                            .font(FontId::proportional(13.0)),
                    );
                } else {
                    show_pane_combo(
                        ui,
                        "ollama_model",
                        egui::RichText::new(combo_text),
                        220.0,
                        |ui| {
                            for name in &self.ollama_models {
                                if ui
                                    .selectable_label(self.selected_ai_model == *name, name)
                                    .clicked()
                                {
                                    self.selected_ai_model = name.clone();
                                }
                            }
                            if !self
                                .ollama_models
                                .iter()
                                .any(|n| n == &self.selected_ai_model)
                                && !self.selected_ai_model.trim().is_empty()
                            {
                                let _ = ui.selectable_label(
                                    true,
                                    format!("（自訂）{}", self.selected_ai_model),
                                );
                            }
                        },
                    );
                }
                if secondary_button(ui, "重新整理").clicked() {
                    action = FormAction::RefreshOllama;
                }
            });
            if !self.ollama_refresh_message.is_empty() {
                note(ui, &self.ollama_refresh_message);
            }
            let wants_llm = matches!(
                self.ai_mode,
                AiEnhancementMode::Smart | AiEnhancementMode::Full
            );
            if wants_llm && self.ollama_reachable == Some(false) {
                danger_note(ui, "Ollama 未連線 — 只走本機濾波。");
            } else {
                note(ui, "使用本機 Ollama 服務。重新整理可取得已安裝模型。");
            }
        } else {
            let hint = match self.ai_provider {
                AiProviderType::Gemini => "gemini-2.5-flash",
                _ => "compound-beta-mini",
            };
            ui.label("模型");
            ui.add(
                egui::TextEdit::singleline(&mut self.selected_remote_model)
                    .desired_width(260.0)
                    .hint_text(hint)
                    .font(FontId::proportional(13.0)),
            );
            ui.add_space(8.0);
            let key_label = ui.label(egui::RichText::new("API 金鑰").size(14.0).color(FG));
            let _ = ui.interact(key_label.rect, api_key_label_id(), Sense::hover());
            match self.ai_provider {
                AiProviderType::Gemini => {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.gemini_api_key)
                            .id(api_key_field_id())
                            .password(true)
                            .desired_width(260.0)
                            .font(FontId::proportional(13.0)),
                    );
                    note(ui, "金鑰已遮蔽；如有設定環境變數，會優先使用。");
                }
                _ => {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.groq_api_key)
                            .id(api_key_field_id())
                            .password(true)
                            .desired_width(260.0)
                            .font(FontId::proportional(13.0)),
                    );
                    note(ui, "金鑰已遮蔽；如有設定環境變數，會優先使用。");
                }
            }
        }

        if let Some(error) = self.validation_error() {
            danger_note(ui, error);
        }
        ui.add_space(8.0);
        switch_row(ui, &mut self.enable_structured_output, "整理清單／步驟");
        if let Some(llm) = view.last_llm.filter(|s| !s.is_empty()) {
            note(ui, llm);
        }
        action = merge_action(
            action,
            self.show_last_utterance(ui, view.last_raw, view.last_final),
        );
        action
    }

    fn show_last_utterance(&mut self, ui: &mut Ui, raw: &str, final_text: &str) -> FormAction {
        let raw = raw.trim();
        let final_text = final_text.trim();
        if raw.is_empty() && final_text.is_empty() {
            return FormAction::None;
        }
        ui.add_space(12.0);
        ui.label(egui::RichText::new("辨識生稿").size(14.0).color(FG));
        let mut action = FormAction::None;
        if raw.is_empty() {
            note(ui, "尚無本次辨識");
        } else {
            note(ui, "本機 STT＋濾波，未貼上。用來對照 Whisper 與潤稿。");
            show_readonly_block(ui, raw, "尚無本次辨識");
            ui.add_space(4.0);
            if secondary_button(ui, "複製生稿").clicked() {
                action = FormAction::CopyLastRaw;
            }
        }
        ui.add_space(8.0);
        ui.label(egui::RichText::new("潤稿定稿").size(14.0).color(FG));
        if final_text.is_empty() {
            note(ui, "尚無本次定稿");
        } else {
            note(ui, "實際貼上的文字（潤稿或本機後援）。");
            show_readonly_block(ui, final_text, "尚無本次定稿");
        }
        action
    }

    fn show_dictionary(&mut self, ui: &mut Ui) -> FormAction {
        let mut action = FormAction::None;
        pane_heading(
            ui,
            "自訂辭典",
            "管理專有名詞與常見辨識修正。搜尋規則，或新增「原文 → 取代」。",
        );
        switch_row(ui, &mut self.auto_learn_from_corrections, "自動學習");

        let file_tools = egui::CollapsingHeader::new("匯入、匯出與檔案位置")
            .id_salt("dictionary_files")
            .show(ui, |ui| {
                ui.add_space(8.0);
                show_dictionary_file_card(
                    ui,
                    &lailaisay_core::custom_words_path(),
                    self.dictionary.entries.len(),
                );

                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.spacing_mut().item_spacing.y = 8.0;
                    if dictionary_reveal_supported()
                        && tagged_secondary_button(ui, "lailaisay_dict_reveal", "在 Finder 顯示")
                            .clicked()
                    {
                        action = FormAction::RevealDictionary;
                    }
                    if tagged_secondary_button(ui, "lailaisay_dict_reload", "重新載入").clicked()
                    {
                        action = FormAction::ReloadDictionary;
                    }
                    if tagged_secondary_button(ui, "lailaisay_dict_import", "匯入…").clicked() {
                        action = FormAction::ImportDictionary;
                    }
                    if tagged_secondary_button(ui, "lailaisay_dict_export", "匯出…").clicked() {
                        action = FormAction::ExportDictionary;
                    }
                });
                note(ui, "匯入會合併同名原文。");
                if cfg!(target_os = "linux") {
                    ui.add_space(4.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.dict_file_path)
                            .desired_width(ui.available_width())
                            .hint_text("或貼上 JSON 路徑"),
                    );
                }
            });
        let _ = ui.interact(
            file_tools.header_response.rect,
            Id::new("lailaisay_dict_file_tools"),
            Sense::hover(),
        );
        note(ui, "規則操作會立即儲存；可撤銷上次變更。");
        ui.add_space(12.0);
        let duplicate =
            self.dictionary.entries.iter().enumerate().any(|(i, e)| {
                Some(i) != self.editing_entry && e.original == self.new_original.trim()
            });
        let unsafe_digit_punct = is_unsafe_digit_list_to_punctuation(
            self.new_original.trim(),
            self.new_replacement.trim(),
        );
        ui.label(egui::RichText::new("新增規則").size(14.0).color(FG));
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.add(
                egui::TextEdit::singleline(&mut self.new_original)
                    .desired_width(96.0)
                    .hint_text("原文"),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.new_replacement)
                    .desired_width(96.0)
                    .hint_text("取代"),
            );
            if ui
                .add_enabled(
                    !duplicate
                        && !unsafe_digit_punct
                        && !self.new_original.trim().is_empty()
                        && !self.new_replacement.trim().is_empty(),
                    egui::Button::new(if self.editing_entry.is_some() {
                        "更新規則"
                    } else {
                        "新增"
                    }),
                )
                .clicked()
            {
                let o = self.new_original.trim();
                let r = self.new_replacement.trim();
                if !o.is_empty() && !r.is_empty() && !is_unsafe_digit_list_to_punctuation(o, r) {
                    self.dictionary_undo = Some(self.dictionary.clone());
                    if let Some(index) = self.editing_entry.take() {
                        if let Some(entry) = self.dictionary.entries.get_mut(index) {
                            entry.original = o.into();
                            entry.replacement = r.into();
                        }
                    } else {
                        self.dictionary
                            .add_entry(CustomWordEntry::replacement(o, r));
                    }
                    self.new_original.clear();
                    self.new_replacement.clear();
                    action = FormAction::SaveDictionary;
                }
            }
        });
        if duplicate {
            danger_note(ui, "此原文已有規則，請搜尋並編輯原規則。");
        }
        if unsafe_digit_punct {
            danger_note(ui, "數字列表不能被取代成標點，以免把 B12345 這類編號改壞。");
        }
        if self.editing_entry.is_some() && ui.button("取消編輯").clicked() {
            self.editing_entry = None;
            self.new_original.clear();
            self.new_replacement.clear();
        }
        ui.add_space(6.0);
        if secondary_button(ui, "加入上次 STT→定稿").clicked() {
            action = FormAction::PromoteLast;
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(format!("{} 條規則", self.dictionary.entries.len()));
            ui.add(
                egui::TextEdit::singleline(&mut self.dictionary_search)
                    .hint_text("搜尋原文或取代文字")
                    .desired_width(220.0),
            );
            if ui
                .add_enabled(
                    self.dictionary_undo.is_some(),
                    egui::Button::new("撤銷上次變更"),
                )
                .clicked()
            {
                self.dictionary = self.dictionary_undo.take().unwrap();
                self.editing_entry = None;
                action = FormAction::SaveDictionary;
            }
        });
        let query = self.dictionary_search.to_lowercase();
        let mut remove = None;
        let mut edit = None;
        let mut visible = 0;
        for (index, entry) in self.dictionary.entries.iter().enumerate() {
            if !query.is_empty()
                && !entry.original.to_lowercase().contains(&query)
                && !entry.replacement.to_lowercase().contains(&query)
            {
                continue;
            }
            visible += 1;
            ui.push_id(entry.id, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width((ui.available_width() - 130.0).max(140.0));
                        ui.label(egui::RichText::new(&entry.original).strong());
                        ui.label(format!(
                            "→ {}{}",
                            entry.replacement,
                            if entry.is_enabled {
                                ""
                            } else {
                                "（已停用）"
                            }
                        ));
                    });
                    if ui.button("編輯").clicked() {
                        edit = Some(index);
                    }
                    if ui.button("刪除").clicked() {
                        remove = Some(index);
                    }
                });
                ui.separator();
            });
        }
        if visible == 0 {
            note(ui, "沒有符合的規則");
        }
        if let Some(index) = edit {
            let entry = &self.dictionary.entries[index];
            self.new_original = entry.original.clone();
            self.new_replacement = entry.replacement.clone();
            self.editing_entry = Some(index);
            ui.scroll_to_rect(ui.min_rect(), Some(Align::Min));
        }
        if let Some(index) = remove {
            self.dictionary_undo = Some(self.dictionary.clone());
            self.dictionary.entries.remove(index);
            self.editing_entry = None;
            action = FormAction::SaveDictionary;
        }
        action
    }

    fn show_permissions(&mut self, ui: &mut Ui) {
        // Re-probe TCC while this pane is on screen so System Settings toggles show up.
        ui.ctx().request_repaint_after(Duration::from_millis(500));
        pane_heading(ui, "系統權限", "查看錄音、快捷鍵與貼上功能需要的系統權限。");
        if cfg!(target_os = "macos") {
            note(ui, PERMISSIONS_NOTE_MACOS);
        } else {
            note(ui, PERMISSIONS_NOTE_OTHER);
        }
        ui.add_space(6.0);
        for (pane, title, detail) in PRIVACY_ROWS {
            // The sandboxed App Store build never asks for Accessibility,
            // Input Monitoring or Automation; only the microphone applies.
            if lailaisay_paste::is_app_store_build() && *pane != PrivacyPane::Microphone {
                continue;
            }
            if let Some(err) = perm_row(ui, title, detail, *pane) {
                self.permissions_open_message = err;
            }
        }
        if !self.permissions_open_message.is_empty() {
            danger_note(ui, &self.permissions_open_message);
        }
        ui.add_space(8.0);
        warn_note(
            ui,
            "若仍用 cargo run，系統記的是 Terminal／Cursor。打包後請改授權 lailaisay.app。",
        );
    }

    fn show_footer(&mut self, ui: &mut Ui) -> FormAction {
        let mut action = FormAction::None;
        ui.horizontal(|ui| {
            if ghost_button(ui, "結束 lailaisay").clicked() {
                action = FormAction::Quit;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(
                        self.validation_error().is_none(),
                        egui::Button::new("儲存設定")
                            .fill(GLASS_TINT)
                            .min_size(Vec2::new(108.0, ROW_H)),
                    )
                    .clicked()
                {
                    action = FormAction::Save;
                }
                if self.is_dirty() {
                    ui.label(
                        egui::RichText::new("尚未儲存")
                            .color(WARN)
                            .size(12.5)
                            .strong(),
                    );
                }
            });
        });
        if !self.save_message.is_empty() {
            let failed = save_message_is_error(&self.save_message);
            let color = if failed {
                DANGER
            } else if self.save_message.starts_with("已") || self.save_message.contains("已儲存")
            {
                SUCCESS
            } else {
                MUTED
            };
            ui.label(
                egui::RichText::new(&self.save_message)
                    .color(color)
                    .size(12.0),
            );
        }
        action
    }

    fn whisper_short_name(&self) -> String {
        if self.selected_model.is_empty() {
            return "dummy".into();
        }
        PathBuf::from(&self.selected_model)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.selected_model.clone())
    }
}

fn merge_action(current: FormAction, next: FormAction) -> FormAction {
    if next == FormAction::None {
        current
    } else {
        next
    }
}

/// Settings `ViewportBuilder`: chrome paints from the top edge of the window.
///
/// egui 0.32 has no `with_titlebar_transparent`. `with_titlebar_shown(false)`
/// is what egui-winit maps to winit's `with_titlebar_transparent(true)`.
/// `with_title_shown(false)` hides the native title so only the in-chrome
/// 「lailaisay 設定」 is drawn. `with_titlebar_buttons_shown(true)` keeps the macOS
/// traffic lights visible; clicks are emulated in
/// [`paint_settings_titlebar`] because the fullsize content view covers their
/// AppKit hit area.
pub fn settings_viewport_builder() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title("lailaisay 設定")
        .with_window_level(egui::WindowLevel::Normal)
        .with_inner_size(SETTINGS_INNER_SIZE)
        .with_min_inner_size(SETTINGS_MIN_SIZE)
        .with_max_inner_size(SETTINGS_MAX_SIZE)
        .with_titlebar_shown(false)
        .with_titlebar_buttons_shown(true)
        .with_fullsize_content_view(true)
        .with_title_shown(false)
}

fn titlebar_heading_id() -> Id {
    Id::new("lailaisay_settings_titlebar")
}

fn titlebar_drag_id() -> Id {
    Id::new("lailaisay_settings_titlebar_drag")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrafficLight {
    Close,
    Minimize,
    Zoom,
}

impl TrafficLight {
    const ALL: [Self; 3] = [Self::Close, Self::Minimize, Self::Zoom];

    fn index(self) -> usize {
        match self {
            Self::Close => 0,
            Self::Minimize => 1,
            Self::Zoom => 2,
        }
    }
}

fn traffic_light_id(light: TrafficLight) -> Id {
    Id::new(("lailaisay_settings_traffic_light", light.index()))
}

fn traffic_light_center(titlebar: Rect, light: TrafficLight) -> Pos2 {
    let i = light.index() as f32;
    let x = titlebar.left()
        + TRAFFIC_LIGHT_LEFT_PAD
        + TRAFFIC_LIGHT_DIAMETER * 0.5
        + i * (TRAFFIC_LIGHT_DIAMETER + TRAFFIC_LIGHT_GAP);
    Pos2::new(x, titlebar.center().y)
}

fn traffic_light_hit_rect(titlebar: Rect, light: TrafficLight) -> Rect {
    Rect::from_center_size(
        traffic_light_center(titlebar, light),
        Vec2::splat(TRAFFIC_LIGHT_HIT),
    )
}

fn traffic_light_command(light: TrafficLight, maximized: bool) -> ViewportCommand {
    match light {
        TrafficLight::Close | TrafficLight::Minimize => ViewportCommand::Close,
        TrafficLight::Zoom => ViewportCommand::Maximized(!maximized),
    }
}

/// Native traffic lights stay visible, but the glow/fullsize content view
/// sits on their AppKit hit area. Emulate the three clicks in-process.
/// Live windows do this on macOS only; unit tests exercise the same path.
fn emulate_traffic_light_clicks() -> bool {
    cfg!(target_os = "macos") || cfg!(test)
}

/// In-chrome title above the status strip. Divider is 1px `GLASS_EDGE`, not
/// egui's default gray `(186,187,188)` separator.
///
/// Left `TRAFFIC_LIGHT_INSET` is reserved: no heading/drag widgets. On macOS
/// (and in tests) invisible click targets there send `ViewportCommand`s so
/// close / minimize / zoom work even when AppKit hit-testing does not.
/// Close **and** Minimize go through `close_requested` → hide-to-tray
/// (`settings_visible = false` + `Visible(false)`). OS miniaturize would be
/// restored when switching to LINE or when the HUD shows. Tray Quit is the
/// only quit.
fn paint_settings_titlebar(ctx: &egui::Context) {
    let titlebar = egui::TopBottomPanel::top("settings_titlebar")
        .exact_height(SETTINGS_TITLEBAR_H)
        .frame(
            Frame::new()
                .fill(GLASS_FILL)
                .inner_margin(egui::Margin::ZERO),
        )
        .show(ctx, |ui| {
            let full = ui.max_rect();
            paint_titlebar_heading(ui, full);
            handle_titlebar_traffic_lights(ui, full);
            handle_titlebar_drag(ui, full);
        });
    // 1px glass-edge between titlebar and status — not ui.separator().
    let rect = titlebar.response.rect;
    if rect.height() >= 1.0 && rect.width() >= 1.0 {
        ctx.layer_painter(titlebar.response.layer_id).rect_filled(
            Rect::from_min_max(
                Pos2::new(rect.left(), rect.bottom() - 1.0),
                Pos2::new(rect.right(), rect.bottom()),
            ),
            0.0,
            GLASS_EDGE,
        );
    }
}

fn paint_titlebar_heading(ui: &mut Ui, full: Rect) {
    // Centered in the window, but the interact rect must stay clear of the
    // traffic-light inset so it cannot eat those clicks.
    let heading_rect = Rect::from_center_size(full.center(), Vec2::new(96.0, 20.0));
    let resp = ui.put(
        heading_rect,
        egui::Label::new(
            egui::RichText::new("lailaisay 設定")
                .size(13.0)
                .strong()
                .color(FG),
        ),
    );
    let _ = ui.interact(resp.rect, titlebar_heading_id(), Sense::hover());
}

fn handle_titlebar_traffic_lights(ui: &mut Ui, titlebar: Rect) {
    let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
    for light in TrafficLight::ALL {
        let rect = traffic_light_hit_rect(titlebar, light);
        let resp = ui.interact(rect, traffic_light_id(light), Sense::click());
        if resp.clicked() && emulate_traffic_light_clicks() {
            ui.ctx()
                .send_viewport_cmd(traffic_light_command(light, maximized));
        }
    }
}

fn handle_titlebar_drag(ui: &mut Ui, titlebar: Rect) {
    let drag_rect = Rect::from_min_max(
        Pos2::new(titlebar.left() + TRAFFIC_LIGHT_INSET, titlebar.top()),
        titlebar.max,
    );
    if drag_rect.width() < 1.0 || drag_rect.height() < 1.0 {
        return;
    }
    let drag = ui.interact(drag_rect, titlebar_drag_id(), Sense::click_and_drag());
    if drag.drag_started_by(egui::PointerButton::Primary) {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }
}

/// 「最短按住」: label + helper on the left, `DragValue` + 「秒」 on the right.
fn min_hold_row(ui: &mut Ui, value: &mut f64) {
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.scope(|ui| {
                ui.style_mut().override_font_id = Some(FontId::monospace(13.0));
                ui.add(
                    egui::DragValue::new(value)
                        .speed(0.05)
                        .range(0.0..=5.0)
                        .suffix("秒")
                        .custom_formatter(|n, _| format!("{n:.2}")),
                );
            });
            ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(egui::RichText::new("最短按住").size(14.0).color(FG));
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(MIN_HOLD_HELPER).size(12.5).color(MUTED),
                        )
                        .wrap(),
                    );
                });
            });
        });
    });
}

/// Axis-aligned inner rect that stays inside the rounded window chrome.
pub(crate) fn popup_safe_rect(screen: Rect) -> Rect {
    screen.shrink(theme::ROUND_WIN)
}

/// Where a pane combo popup may grow without leaving `safe` or the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ComboPopupPlacement {
    pub rect: Rect,
    pub width: f32,
    pub max_height: f32,
    pub open_above: bool,
    pub pivot: Align2,
    pub pos: Pos2,
}

/// Flip above when the usable space below is too short; shift left so a wide
/// menu does not cross the right window edge.
pub(crate) fn combo_popup_placement(
    button: Rect,
    safe: Rect,
    desired_width: f32,
    gap: f32,
) -> ComboPopupPlacement {
    let width = desired_width.min(safe.width()).max(0.0);
    let space_below = (safe.bottom() - button.bottom() - gap).max(0.0);
    let space_above = (button.top() - safe.top() - gap).max(0.0);
    let open_above = space_below < COMBO_FLIP_BELOW && space_above > space_below;
    let max_height = if open_above { space_above } else { space_below };

    let mut left = button.left();
    if left + width > safe.right() {
        left = safe.right() - width;
    }
    if left < safe.left() {
        left = safe.left();
    }

    let (pivot, pos, rect) = if open_above {
        let pos = Pos2::new(left, button.top() - gap);
        (
            Align2::LEFT_BOTTOM,
            pos,
            Rect::from_min_max(
                Pos2::new(left, pos.y - max_height),
                Pos2::new(left + width, pos.y),
            ),
        )
    } else {
        let pos = Pos2::new(left, button.bottom() + gap);
        (
            Align2::LEFT_TOP,
            pos,
            Rect::from_min_size(pos, Vec2::new(width, max_height)),
        )
    };

    ComboPopupPlacement {
        rect,
        width,
        max_height,
        open_above,
        pivot,
        pos,
    }
}

fn api_key_label_id() -> Id {
    Id::new("lailaisay_ai_api_key_label")
}

fn api_key_field_id() -> Id {
    Id::new("lailaisay_ai_api_key")
}

fn pane_combo_button_id(id_salt: &str) -> Id {
    Id::new(("lailaisay_pane_combo", id_salt))
}

fn pane_combo_popup_id(id_salt: &str) -> Id {
    Id::new(("lailaisay_pane_combo_popup", id_salt))
}

/// Combo whose menu is an `Order::Foreground` area, clipped to the window
/// inset rather than the parent `ScrollArea` / panel.
fn show_pane_combo(
    ui: &mut Ui,
    id_salt: &'static str,
    selected_text: impl Into<WidgetText>,
    desired_width: f32,
    add_contents: impl FnOnce(&mut Ui),
) -> egui::Response {
    let button_id = pane_combo_button_id(id_salt);
    let popup_id = pane_combo_popup_id(id_salt);
    let width = desired_width.min(ui.available_width()).max(64.0);
    let is_open = Popup::is_id_open(ui.ctx(), popup_id);

    let old_clip = ui.clip_rect();
    ui.set_clip_rect(old_clip.union(ui.ctx().screen_rect()));
    let response = paint_pane_combo_button(ui, button_id, selected_text.into(), width, is_open);
    ui.set_clip_rect(old_clip);

    if response.clicked() {
        Popup::toggle_id(ui.ctx(), popup_id);
    } else if Popup::is_id_open(ui.ctx(), popup_id) {
        // Area (not Popup::show) so we can constrain_to the rounded-chrome inset
        // and size the first frame. Memory still requires an explicit keep.
        #[allow(deprecated)]
        ui.ctx().memory_mut(|mem| mem.keep_popup_open(popup_id));
    }

    if Popup::is_id_open(ui.ctx(), popup_id) {
        let safe = popup_safe_rect(ui.ctx().screen_rect());
        let placement = combo_popup_placement(response.rect, safe, width, COMBO_POPUP_GAP);
        Area::new(popup_id)
            .order(Order::Foreground)
            .pivot(placement.pivot)
            .fixed_pos(placement.pos)
            .constrain_to(safe)
            .default_size(Vec2::new(placement.width, placement.max_height.max(1.0)))
            .sense(Sense::click())
            .show(ui.ctx(), |ui| {
                ui.set_clip_rect(safe);
                ui.set_min_width(placement.width);
                ui.set_max_width(placement.width);
                Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_max_width(placement.width);
                    ui.style_mut().wrap_mode = Some(TextWrapMode::Truncate);
                    egui::ScrollArea::vertical()
                        .max_height(placement.max_height)
                        .show(ui, add_contents);
                });
            });

        let escape = ui.ctx().input(|i| i.key_pressed(egui::Key::Escape));
        if response.clicked_elsewhere() || escape {
            Popup::close_id(ui.ctx(), popup_id);
        }
    }

    response
}

fn paint_pane_combo_button(
    ui: &mut Ui,
    id: Id,
    selected_text: WidgetText,
    width: f32,
    is_open: bool,
) -> egui::Response {
    let (_, rect) = ui.allocate_space(Vec2::new(width, ROW_H));
    let response = ui.interact(rect, id, Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::ComboBox,
            ui.is_enabled(),
            selected_text.text(),
        )
    });
    if !ui.is_rect_visible(rect) {
        return response;
    }

    let visuals = if is_open {
        &ui.visuals().widgets.open
    } else {
        ui.style().interact(&response)
    };
    ui.painter().rect(
        rect.expand(visuals.expansion),
        visuals.corner_radius,
        visuals.weak_bg_fill,
        visuals.bg_stroke,
        egui::StrokeKind::Inside,
    );

    let pad = ui.spacing().button_padding;
    let icon_size = Vec2::splat(ui.spacing().icon_width);
    let inner = rect.shrink2(pad);
    let icon_rect = Align2::RIGHT_CENTER.align_size_within_rect(icon_size, inner);
    let icon = Rect::from_center_size(
        icon_rect.center(),
        Vec2::new(icon_rect.width() * 0.7, icon_rect.height() * 0.45),
    );
    ui.painter().add(egui::Shape::convex_polygon(
        vec![icon.left_top(), icon.right_top(), icon.center_bottom()],
        visuals.fg_stroke.color,
        Stroke::NONE,
    ));

    let text_max = (icon_rect.left() - ui.spacing().icon_spacing - inner.left()).max(8.0);
    let galley = selected_text.into_galley(
        ui,
        Some(TextWrapMode::Truncate),
        text_max,
        TextStyle::Button,
    );
    let text_rect = Align2::LEFT_CENTER.align_size_within_rect(galley.size(), inner);
    ui.painter()
        .galley(text_rect.min, galley, visuals.text_color());
    response
}

fn pane_heading_id(title: &str) -> Id {
    Id::new(("lailaisay_pane_heading", title))
}

fn pane_end_id(pane: SettingsSection) -> Id {
    Id::new(("lailaisay_pane_end", pane.label()))
}

fn mark_pane_end(ui: &mut Ui, pane: SettingsSection) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, 1.0), Sense::hover());
    let _ = ui.interact(rect, pane_end_id(pane), Sense::hover());
}

fn pane_heading(ui: &mut Ui, title: &str, blurb: &str) {
    let resp = ui.label(egui::RichText::new(title).strong().size(17.0).color(FG));
    let _ = ui.interact(resp.rect, pane_heading_id(title), Sense::hover());
    ui.add_space(4.0);
    note(ui, blurb);
    ui.add_space(10.0);
}

fn note(ui: &mut Ui, text: &str) {
    ui.add(egui::Label::new(egui::RichText::new(text).size(12.5).color(MUTED)).wrap());
}

fn show_readonly_block(ui: &mut Ui, text: &str, hint: &str) {
    let mut buf = text.to_string();
    ui.add(
        egui::TextEdit::multiline(&mut buf)
            .desired_rows(3)
            .desired_width(ui.available_width())
            .interactive(false)
            .hint_text(hint)
            .font(FontId::proportional(13.0)),
    );
}

fn warn_note(ui: &mut Ui, text: &str) {
    ui.add(egui::Label::new(egui::RichText::new(text).size(12.5).color(WARN)).wrap());
}

fn success_note(ui: &mut Ui, text: &str) {
    ui.add(egui::Label::new(egui::RichText::new(text).size(12.5).color(SUCCESS)).wrap());
}

fn danger_note(ui: &mut Ui, text: &str) {
    ui.add(egui::Label::new(egui::RichText::new(text).size(12.5).color(DANGER)).wrap());
}

#[derive(Clone, Copy)]
enum ChromeButton {
    Secondary,
    Ghost,
}

fn paint_chrome_highlight(ctx: &egui::Context, layer_id: egui::LayerId, rect: Rect) {
    theme::paint_top_highlight(&ctx.layer_painter(layer_id), rect, GLASS_HIGHLIGHT);
}

fn chrome_button(ui: &mut Ui, label: &str, kind: ChromeButton, height: f32) -> egui::Response {
    let button = egui::Button::new(label).min_size(Vec2::new(0.0, height));
    ui.add(match kind {
        ChromeButton::Ghost => button.frame(false),
        ChromeButton::Secondary => button,
    })
}

fn secondary_button(ui: &mut Ui, label: &str) -> egui::Response {
    chrome_button(ui, label, ChromeButton::Secondary, ROW_H)
}

fn ghost_button(ui: &mut Ui, label: &str) -> egui::Response {
    chrome_button(ui, label, ChromeButton::Ghost, ROW_H)
}

fn tagged_secondary_button(ui: &mut Ui, id: &'static str, label: &str) -> egui::Response {
    let resp = secondary_button(ui, label);
    let _ = ui.interact(resp.rect, Id::new(id), Sense::hover());
    resp
}

fn show_dictionary_file_card(ui: &mut Ui, path: &Path, entry_count: usize) {
    let width = ui.available_width();
    Frame::new()
        .fill(GLASS_FILL_DEEP)
        .stroke(Stroke::new(1.0_f32, GLASS_EDGE))
        .corner_radius(CornerRadius::same(ROUND_CTL as u8))
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_min_width((width - 30.0).max(1.0));
            let title = ui.label(
                egui::RichText::new("目前字典檔")
                    .size(14.0)
                    .strong()
                    .color(FG),
            );
            let _ = ui.interact(
                title.rect,
                Id::new("lailaisay_dict_file_card"),
                Sense::hover(),
            );
            ui.add_space(2.0);
            note(ui, &tilde_home_path(path));
            ui.add_space(4.0);
            let exists = path.is_file();
            let color = if exists { SUCCESS } else { WARN };
            ui.label(
                egui::RichText::new(dictionary_status_line(path, entry_count))
                    .size(12.5)
                    .color(color),
            );
        });
}

fn rail_item(ui: &mut Ui, label: &str, selected: bool) -> egui::Response {
    ui.add_sized(
        [ui.available_width(), ROW_H],
        egui::Button::selectable(selected, label),
    )
}

fn switch_row(ui: &mut Ui, on: &mut bool, label: &str) {
    ui.checkbox(on, label);
}

fn segmented<T: PartialEq + Copy>(ui: &mut Ui, id: &str, value: &mut T, options: &[(T, &str)]) {
    ui.push_id(id, |ui| {
        ui.horizontal_wrapped(|ui| {
            for (v, label) in options {
                ui.selectable_value(value, *v, *label);
            }
        });
    });
}

fn provider_key(provider: AiProviderType) -> &'static str {
    match provider {
        AiProviderType::Gemini => "gemini",
        _ => "groq",
    }
}

fn hotkey_editor(ui: &mut Ui, id: &str, hotkey: &mut HotKey) {
    ui.push_id(id, |ui| {
        egui::CollapsingHeader::new(format!("{}  ·  更改快捷鍵", hotkey_parts(hotkey).join(" ")))
            .id_salt("hotkey_editor")
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (modifier, label) in [
                        (
                            Modifier::Command,
                            if cfg!(target_os = "windows") {
                                "Win"
                            } else {
                                "⌘ Command"
                            },
                        ),
                        (
                            Modifier::Option,
                            if cfg!(target_os = "windows") {
                                "Alt"
                            } else {
                                "⌥ Option"
                            },
                        ),
                        (
                            Modifier::Control,
                            if cfg!(target_os = "windows") {
                                "Ctrl"
                            } else {
                                "⌃ Control"
                            },
                        ),
                        (
                            Modifier::Shift,
                            if cfg!(target_os = "windows") {
                                "Shift"
                            } else {
                                "⇧ Shift"
                            },
                        ),
                        (Modifier::Fn, "fn"),
                    ] {
                        let mut enabled = hotkey.modifiers.contains(modifier);
                        if ui.checkbox(&mut enabled, label).changed() {
                            if enabled {
                                hotkey.modifiers.modifiers.insert(modifier);
                            } else {
                                hotkey.modifiers.modifiers.remove(&modifier);
                            }
                        }
                    }
                });
                egui::ComboBox::from_id_salt("key")
                    .selected_text(
                        hotkey
                            .key
                            .as_ref()
                            .map(|k| k.as_str())
                            .unwrap_or("僅修飾鍵"),
                    )
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut hotkey.key, None, "僅修飾鍵");
                        for key in [
                            "space", "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l",
                            "m", "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "y", "z",
                            "0", "1", "2", "3", "4", "5", "6", "7", "8", "9",
                        ] {
                            ui.selectable_value(&mut hotkey.key, Some(Key::parse(key)), key);
                        }
                    });
                note(
                    ui,
                    "至少選一個修飾鍵；兩組快捷鍵不可相同，請避開系統及其他 App 快捷鍵。",
                );
            });
    });
}

fn hotkey_parts(hotkey: &HotKey) -> Vec<String> {
    let mut parts = Vec::new();
    for m in &hotkey.modifiers.modifiers {
        parts.push(match m {
            Modifier::Command => if cfg!(target_os = "windows") {
                "Win"
            } else {
                "⌘"
            }
            .into(),
            Modifier::Option => if cfg!(target_os = "windows") {
                "Alt"
            } else {
                "⌥"
            }
            .into(),
            Modifier::Shift => if cfg!(target_os = "windows") {
                "Shift"
            } else {
                "⇧"
            }
            .into(),
            Modifier::Control => if cfg!(target_os = "windows") {
                "Ctrl"
            } else {
                "⌃"
            }
            .into(),
            Modifier::Fn => "fn".into(),
        });
    }
    if let Some(key) = &hotkey.key {
        let s = key.as_str();
        parts.push(if *key == Key::Space {
            "Space".into()
        } else {
            s.to_uppercase()
        });
    }
    if parts.is_empty() {
        parts.push("—".into());
    }
    parts
}

fn privacy_grant(pane: PrivacyPane) -> GrantStatus {
    match pane {
        PrivacyPane::Accessibility => lailaisay_input::accessibility_grant(),
        PrivacyPane::Microphone => lailaisay_input::microphone_grant(),
        PrivacyPane::InputMonitoring => lailaisay_input::input_monitoring_grant(),
        PrivacyPane::Automation => lailaisay_input::automation_grant(),
    }
}

fn perm_status_label(status: GrantStatus, on_macos: bool) -> &'static str {
    if on_macos {
        status.zh_label()
    } else {
        "僅 macOS"
    }
}

fn perm_lamp_color(status: GrantStatus, on_macos: bool) -> Color32 {
    if !on_macos {
        return MUTED;
    }
    match status {
        GrantStatus::Granted => SUCCESS,
        GrantStatus::Denied => DANGER,
        GrantStatus::Unknown | GrantStatus::NotDetermined | GrantStatus::TargetNotRunning => WARN,
    }
}

fn permission_hint(pane: PrivacyPane, status: GrantStatus) -> Option<&'static str> {
    match (pane, status) {
        (PrivacyPane::Microphone, GrantStatus::NotDetermined) => {
            Some("macOS 尚未取得錄音授權決定；首次按住說話時會詢問。")
        }
        (PrivacyPane::Automation, GrantStatus::NotDetermined) => {
            Some("macOS 尚未允許控制 System Events；首次需要自動貼上時會詢問。")
        }
        (PrivacyPane::Automation, GrantStatus::TargetNotRunning) => {
            Some("System Events 尚未執行，暫時無法查詢；這不代表權限被關閉。")
        }
        (_, GrantStatus::Unknown) => {
            Some("系統查詢未成功，請在系統設定確認。此狀態不代表已授權或已拒絕。")
        }
        _ => None,
    }
}

fn paint_perm_lamp(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.5, color);
}

/// Returns `Some(error)` when the user asked to open Settings and `open` failed.
fn perm_row(ui: &mut Ui, title: &str, detail: &str, pane: PrivacyPane) -> Option<String> {
    let can_open = cfg!(target_os = "macos");
    let on_macos = cfg!(target_os = "macos");
    let grant = privacy_grant(pane);
    let status_text = perm_status_label(grant, on_macos);
    let lamp = perm_lamp_color(grant, on_macos);
    let id = ui.make_persistent_id(("lailaisay_perm", title));
    let hovered = can_open && ui.ctx().read_response(id).is_some_and(|r| r.hovered());
    let mut open_clicked = false;

    let inner = Frame::new()
        .fill(if hovered {
            GLASS_HOVER
        } else {
            Color32::TRANSPARENT
        })
        .stroke(Stroke::new(
            1.0_f32,
            if hovered { GLASS_EDGE } else { BORDER },
        ))
        .corner_radius(CornerRadius::same(ROUND_CTL as u8))
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                paint_perm_lamp(ui, lamp);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(title).size(14.0).color(FG));
                    ui.label(egui::RichText::new(detail).size(12.0).color(MUTED));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if can_open && secondary_button(ui, "開啟").clicked() {
                        open_clicked = true;
                    }
                    ui.label(egui::RichText::new(status_text).size(12.0).color(lamp));
                });
            });
            if let Some(hint) = permission_hint(pane, grant) {
                note(ui, hint);
            }
        });

    let sense = if can_open {
        Sense::click()
    } else {
        Sense::hover()
    };
    let resp = ui.interact(inner.response.rect, id, sense);
    ui.add_space(6.0);

    if can_open {
        let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
        if resp.clicked() || open_clicked {
            return match open_macos_privacy_settings(pane) {
                Ok(()) => Some(String::new()),
                Err(e) => Some(e),
            };
        }
    }
    None
}

pub fn paint_status_strip(ui: &mut Ui, status: &str, detail: &str) {
    let (label, color) = status_appearance(status);
    ui.horizontal(|ui| {
        paint_chip(ui, &label, color);
        if !detail.is_empty() {
            ui.add_space(8.0);
            ui.add(
                egui::Label::new(egui::RichText::new(detail).size(12.5).color(MUTED)).truncate(),
            );
        }
    });
}

fn paint_chip(ui: &mut Ui, label: &str, color: Color32) {
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        FontId::new(12.0, FontFamily::Proportional),
        color,
    );
    let size = Vec2::new(galley.size().x + 28.0, 28.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let rounding = theme::pill(rect.height());
    theme::paint_glass_control(
        ui.painter(),
        rect,
        rounding,
        GLASS_FILL_DEEP,
        Some(GLASS_EDGE),
        true,
        true,
    );
    ui.painter()
        .circle_filled(Pos2::new(rect.left() + 10.0, rect.center().y), 3.5, color);
    ui.painter().galley(
        Pos2::new(rect.left() + 18.0, rect.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
}

fn needs_permission(status: &str) -> bool {
    let s = status.to_ascii_lowercase();
    s.contains("needs") || s.contains("permission") || s.contains("accessibility")
}

pub fn status_appearance(status: &str) -> (String, Color32) {
    let s = status.to_ascii_lowercase();
    if s.contains("record") {
        ("錄音中".into(), WARN)
    } else if s.contains("transcrib") {
        ("辨識中".into(), WARN)
    } else if s.contains("enhanc") {
        ("潤稿中".into(), WARN)
    } else if s.contains("pasted") || s.contains("replaced") {
        ("已貼上".into(), SUCCESS)
    } else if s.contains("copied") {
        ("已複製".into(), SUCCESS)
    } else if s.contains("hotkey unavailable") {
        ("熱鍵未生效".into(), DANGER)
    } else if s.contains("hold longer") {
        ("請按久一點".into(), WARN)
    } else if s.contains("no speech") {
        ("待命".into(), MUTED)
    } else if needs_permission(status) {
        ("需要權限".into(), DANGER)
    } else if s.contains("fail")
        || s.contains("error")
        || s.contains("unavailable")
        || s.contains("denied")
        || s.contains("timeout")
    {
        ("錯誤".into(), DANGER)
    } else if s.contains("idle") || s.contains("待命") || status.is_empty() {
        ("待命".into(), MUTED)
    } else {
        ("待命".into(), MUTED)
    }
}

pub fn status_detail(status: &str, hold_label: &str, whisper_name: &str, ai_model: &str) -> String {
    let s = status.to_ascii_lowercase();
    if s.contains("record") {
        "放開後開始辨識".into()
    } else if s.contains("transcrib") {
        format!("whisper.cpp · {whisper_name}")
    } else if s.contains("enhanc") {
        format!("Ollama · {ai_model}")
    } else if s.contains("pasted") || s.contains("replaced") {
        "已寫入目前輸入框".into()
    } else if s.contains("copied") {
        "已複製到剪貼簿，請在目標 App 按 ⌘V 貼上".into()
    } else if s.contains("hotkey unavailable") {
        "App Store 版熱鍵需包含修飾鍵與一個按鍵（例如 ⌘⇧Space），且未被其他 App 佔用".into()
    } else if s.contains("hold longer") {
        "按住熱鍵久一點再說話".into()
    } else if s.contains("no speech") {
        "上次未辨識到語音，請再試一次".into()
    } else if needs_permission(status) {
        "系統設定 › 隱私權與安全性 › 輔助使用，授權 lailaisay.app".into()
    } else if s.contains("fail")
        || s.contains("error")
        || s.contains("unavailable")
        || s.contains("denied")
        || s.contains("timeout")
    {
        if is_path_like(status) {
            "發生錯誤".into()
        } else {
            status.to_string()
        }
    } else {
        format!("按住 {hold_label} 說話")
    }
}

pub fn tray_glyph_from_status(status: &str) -> TrayGlyph {
    let s = status.to_ascii_lowercase();
    if s.contains("fail")
        || s.contains("error")
        || s.contains("unavailable")
        || s.contains("denied")
        || s.contains("timeout")
        || needs_permission(status)
    {
        TrayGlyph::Error
    } else if s.contains("record") || s.contains("transcrib") || s.contains("enhanc") {
        TrayGlyph::Active
    } else {
        TrayGlyph::Idle
    }
}

fn is_path_like(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    l.contains("resident:")
        || l.contains("/users/")
        || l.contains("/home/")
        || l.contains("/library/")
        || l.contains("/application support/")
        || (l.contains('/')
            && (l.ends_with(".bin") || l.ends_with(".gguf") || l.ends_with(".json")))
}

/// macOS `open -R` only. Hidden on other platforms.
pub(crate) fn dictionary_reveal_supported() -> bool {
    cfg!(target_os = "macos")
}

pub(crate) fn dictionary_status_line(path: &Path, entry_count: usize) -> String {
    let file = if path.is_file() {
        "檔案存在"
    } else {
        "找不到檔案"
    };
    format!("{file} · 目前 {entry_count} 條規則")
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DictionaryReload {
    Loaded(CustomWordDictionary),
    Missing,
}

pub(crate) fn reload_dictionary_from_disk() -> Result<DictionaryReload, String> {
    reload_dictionary_from_path(&lailaisay_core::custom_words_path())
}

pub(crate) fn reload_dictionary_from_path(path: &Path) -> Result<DictionaryReload, String> {
    if !path.exists() {
        return Ok(DictionaryReload::Missing);
    }
    CustomWordDictionary::load_path(path)
        .map(DictionaryReload::Loaded)
        .map_err(|e| format!("辭典重新載入失敗：JSON 無效（{e}）"))
}

pub(crate) fn load_imported_dictionary(path: &Path) -> Result<CustomWordDictionary, String> {
    CustomWordDictionary::load_path(path).map_err(|e| format!("匯入失敗：JSON 無效（{e}）"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // constructed on macOS only
pub(crate) enum RevealOutcome {
    Revealed,
    CreatedAndRevealed,
}

/// Write the in-memory dictionary only when the file is missing, then reveal it.
/// Never overwrites an existing file. No-op writer on non-macOS (button is hidden).
pub(crate) fn reveal_dictionary_in_file_manager(
    path: &Path,
    current: &CustomWordDictionary,
) -> Result<RevealOutcome, String> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (path, current);
        return Err("僅 macOS 可在 Finder 顯示".into());
    }
    #[cfg(target_os = "macos")]
    {
        let created = if path.exists() {
            false
        } else {
            current
                .save_path(path)
                .map_err(|e| format!("無法建立字典檔：{e}"))?;
            true
        };
        match std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .status()
        {
            Ok(status) if status.success() => Ok(if created {
                RevealOutcome::CreatedAndRevealed
            } else {
                RevealOutcome::Revealed
            }),
            Ok(status) => Err(format!("Finder 顯示失敗：open exited {status}")),
            Err(e) => Err(format!("Finder 顯示失敗：{e}")),
        }
    }
}

pub(crate) fn fallback_json_path(s: &str, must_exist: bool) -> Option<PathBuf> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = PathBuf::from(trimmed);
    if must_exist && !path.is_file() {
        return None;
    }
    Some(path)
}

pub(crate) fn pick_json_file(title: &str, save: bool) -> Option<PathBuf> {
    #[cfg(not(target_os = "linux"))]
    {
        let dialog = rfd::FileDialog::new()
            .add_filter("JSON", &["json"])
            .set_title(title);
        if save {
            let name = lailaisay_core::custom_words_path()
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("hex_custom_words.json")
                .to_string();
            dialog.set_file_name(name).save_file()
        } else {
            dialog.pick_file()
        }
    }
    #[cfg(target_os = "linux")]
    {
        pick_json_file_linux(title, save)
    }
}

#[cfg(target_os = "linux")]
fn pick_json_file_linux(title: &str, save: bool) -> Option<PathBuf> {
    let zenity: Vec<String> = {
        let mut args = vec![
            format!("--title={title}"),
            "--file-selection".into(),
            "--file-filter=JSON | *.json".into(),
        ];
        if save {
            args.push("--save".into());
            args.push("--confirm-overwrite".into());
        }
        args
    };
    if let Some(path) = run_stdout_path("zenity", &zenity) {
        return Some(path);
    }
    let kdialog = if save {
        vec![
            "--title".into(),
            title.into(),
            "--getsavefilename".into(),
            ".".into(),
            "*.json".into(),
        ]
    } else {
        vec![
            "--title".into(),
            title.into(),
            "--getopenfilename".into(),
            ".".into(),
            "*.json".into(),
        ]
    };
    run_stdout_path("kdialog", &kdialog)
}

#[cfg(target_os = "linux")]
fn run_stdout_path(bin: &str, args: &[String]) -> Option<PathBuf> {
    let output = std::process::Command::new(bin).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(PathBuf::from(text))
    }
}

pub fn tilde_home_path(path: &Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if let Some(home) = home {
        if let Ok(rel) = path.strip_prefix(&home) {
            return format!("~/{}", rel.display());
        }
    }
    let s = path.display().to_string();
    for prefix in ["/Users/", "/home/"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            if let Some((_, rest)) = rest.split_once('/') {
                return format!("~/{rest}");
            }
        }
    }
    s
}

pub fn model_inventory_note(local_ggml: usize) -> String {
    format!("{local_ggml} 個本機 ggml 模型")
}

pub fn resident_load_note(
    stt_note: &str,
    local_models: &[crate::models::FoundModel],
    selected: &str,
) -> Option<String> {
    if stt_note.starts_with("dummy") || (stt_note.is_empty() && selected.is_empty()) {
        return if stt_note.starts_with("dummy") {
            Some("目前載入：dummy（無模型）".into())
        } else {
            None
        };
    }
    let path = if let Some(rest) = stt_note.strip_prefix("resident: ") {
        PathBuf::from(rest.trim())
    } else if !selected.is_empty() {
        PathBuf::from(selected)
    } else {
        return None;
    };
    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    if is_path_like(&filename) {
        return None;
    }
    let source = local_models
        .iter()
        .find(|m| m.path == path || m.path.file_name() == path.file_name())
        .map(|m| m.source.tag())
        .unwrap_or("lailaisay");
    let label = if stt_note.starts_with("device: ") {
        "目前選擇"
    } else {
        "目前載入"
    };
    Some(format!("{label}：{filename}（{source}）"))
}

fn save_message_is_error(msg: &str) -> bool {
    let l = msg.to_ascii_lowercase();
    l.contains("fail") || l.contains("失敗") || l.contains("error") || msg.contains("無效")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormAction {
    None,
    Save,
    DownloadCatalog,
    Quit,
    SaveDictionary,
    PromoteLast,
    RefreshOllama,
    CopyLastRaw,
    RevealDictionary,
    ReloadDictionary,
    ImportDictionary,
    ExportDictionary,
}
const COMBO_POPUP_GAP: f32 = 2.0;
/// Open the menu above the button when less than this many px remain below.
const COMBO_FLIP_BELOW: f32 = 96.0;

const PRIVACY_ROWS: &[(PrivacyPane, &str, &str)] = &[
    (PrivacyPane::Accessibility, "輔助使用", "全域熱鍵與貼上"),
    (PrivacyPane::Microphone, "麥克風", "按住說話時錄音"),
    (
        PrivacyPane::InputMonitoring,
        "輸入監控",
        "部分系統的 HID tap",
    ),
    (PrivacyPane::Automation, "自動化", "把文字貼進目標 App"),
];

const PERMISSIONS_NOTE_MACOS: &str =
    "狀態為即時讀取系統授權。點一列或「開啟」仍會打開對應的系統設定。";
const PERMISSIONS_NOTE_OTHER: &str =
    "開啟系統設定僅適用於 macOS。各列顯示「僅 macOS」，不會假裝已授權。";
const MIN_HOLD_HELPER: &str = "低於此秒數不啟動錄音。";
const MINIMIZE_ON_LAUNCH_LABEL: &str = "啟動時縮到選單列";

const LANGUAGE_CHOICES: &[(&str, &str)] = &[
    ("", "自動（保持來源）"),
    ("zh", "中文"),
    ("en", "English"),
    ("ja", "日本語"),
];

fn active_ai_model_label(
    provider: AiProviderType,
    selected_ai_model: &str,
    selected_remote_model: &str,
) -> String {
    if provider.uses_remote_model() {
        let remote = selected_remote_model.trim();
        if remote.is_empty() {
            match provider {
                AiProviderType::Gemini => "gemini-2.5-flash".into(),
                _ => "compound-beta-mini".into(),
            }
        } else {
            remote.to_string()
        }
    } else if selected_ai_model.trim().is_empty() {
        "gemma3".into()
    } else {
        selected_ai_model.trim().to_string()
    }
}

fn language_label(code: &str) -> String {
    LANGUAGE_CHOICES
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, l)| (*l).to_string())
        .unwrap_or_else(|| format!("自訂：{code}"))
}

fn catalog_row_label(entry: &CatalogEntry) -> String {
    let state = if catalog_is_downloaded(entry) {
        "已下載"
    } else {
        "未下載"
    };
    format!("{}  —  {}  —  {state}", entry.label, entry.size_hint)
}

pub fn format_hotkey(hotkey: &HotKey) -> String {
    let shown = if cfg!(target_os = "windows") && !hotkey.modifiers.is_empty() {
        hotkey_parts(hotkey).join("+")
    } else {
        hotkey.to_string()
    };
    if hotkey.key.is_none() {
        if shown.is_empty() {
            "(empty)".into()
        } else {
            format!("{shown}  (modifier-only, no key)")
        }
    } else if shown.is_empty() {
        "(empty)".into()
    } else {
        shown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lailaisay_core::Modifiers;

    #[test]
    fn provider_switch_preserves_each_model_across_save_reload() {
        let mut settings = LailaisaySettings::default();
        settings.ai_provider_type = AiProviderType::Gemini;
        settings.selected_remote_model = "gemini-2.5-flash".into();
        let mut form = SettingsForm::from_settings(&settings);
        form.ai_provider = AiProviderType::Groq;
        form.switch_provider(AiProviderType::Gemini);
        assert!(!form.selected_remote_model.starts_with("gemini-"));
        form.selected_remote_model = "llama-3.3-70b-versatile".into();
        form.ai_provider = AiProviderType::Gemini;
        form.switch_provider(AiProviderType::Groq);
        assert_eq!(form.selected_remote_model, "gemini-2.5-flash");
        form.apply_to(&mut settings);
        let mut reloaded = SettingsForm::from_settings(&settings);
        reloaded.ai_provider = AiProviderType::Groq;
        reloaded.switch_provider(AiProviderType::Gemini);
        assert_eq!(reloaded.selected_remote_model, "llama-3.3-70b-versatile");
    }

    #[test]
    fn invalid_provider_model_and_hotkey_conflicts_cannot_save() {
        let mut form = SettingsForm::from_settings(&LailaisaySettings::default());
        form.ai_provider = AiProviderType::Groq;
        form.selected_remote_model = "gemini-2.5-flash".into();
        assert!(form.validation_error().unwrap().contains("不相容"));
        form.ai_provider = AiProviderType::Ollama;
        form.edit_hotkey = form.hotkey.clone();
        assert!(form.validation_error().unwrap().contains("不可相同"));
        form.edit_hotkey = HotKey::default_edit();
        form.hotkey.modifiers.modifiers.clear();
        assert!(form.validation_error().unwrap().contains("修飾鍵"));
    }

    #[test]
    fn edited_hotkeys_are_dirty_and_persisted() {
        let mut settings = LailaisaySettings::default();
        let mut form = SettingsForm::from_settings(&settings);
        form.hotkey.key = Some(Key::K);
        assert!(form.is_dirty());
        form.apply_to(&mut settings);
        assert_eq!(settings.hotkey.key, Some(Key::K));
        form.mark_clean();
        assert!(!form.is_dirty());
    }

    #[test]
    fn minimize_on_launch_loads_and_saves() {
        let mut s = LailaisaySettings::default();
        assert!(s.minimize_to_menu_bar_on_launch);
        let form = SettingsForm::from_settings(&s);
        assert!(form.minimize_to_menu_bar_on_launch);
        assert_eq!(MINIMIZE_ON_LAUNCH_LABEL, "啟動時縮到選單列");

        s.minimize_to_menu_bar_on_launch = false;
        let mut form = SettingsForm::from_settings(&s);
        assert!(!form.minimize_to_menu_bar_on_launch);
        form.minimize_to_menu_bar_on_launch = true;
        form.apply_to(&mut s);
        assert!(s.minimize_to_menu_bar_on_launch);
        assert!(s.has_completed_onboarding);
    }

    #[test]
    fn apply_roundtrip_keeps_unrelated_fields() {
        let mut s = LailaisaySettings::default();
        s.selected_model = "openai_whisper-large-v3-v20240930".into();
        s.sound_effects_enabled = true;
        let mut form = SettingsForm::from_settings(&s);
        form.language = "zh".into();
        form.prefer_traditional_chinese = false;
        form.ai_mode = AiEnhancementMode::Off;
        form.ai_provider = AiProviderType::Groq;
        form.selected_ai_model = "gemma3:12b".into();
        form.selected_remote_model = "llama-3.1-8b-instant".into();
        form.use_double_tap_only = true;
        form.show_dock_icon = false;
        form.minimize_to_menu_bar_on_launch = false;
        form.minimum_key_time = 0.4;
        form.selected_model = "/tmp/ggml-tiny.bin".into();
        form.apply_to(&mut s);
        assert_eq!(s.output_language.as_deref(), Some("zh"));
        assert!(!s.prefer_traditional_chinese);
        assert_eq!(s.ai_enhancement_mode, AiEnhancementMode::Off);
        assert_eq!(s.ai_provider_type, AiProviderType::Groq);
        assert_eq!(s.selected_ai_model, "gemma3:12b");
        assert_eq!(s.selected_remote_model, "llama-3.1-8b-instant");
        assert!(s.use_double_tap_only);
        assert!(!s.show_dock_icon);
        assert!(!s.minimize_to_menu_bar_on_launch);
        assert!(s.has_completed_onboarding);
        assert!((s.minimum_key_time - 0.4).abs() < f64::EPSILON);
        assert_eq!(
            s.selected_whisper_model.as_deref(),
            Some("/tmp/ggml-tiny.bin")
        );
        assert_eq!(s.selected_model, "openai_whisper-large-v3-v20240930");
        assert!(s.sound_effects_enabled);
        assert_eq!(s.ai_polish_style, None);
    }

    #[test]
    fn apply_persists_minimal_style_without_breaking_mode_keys() {
        let mut s = LailaisaySettings::default();
        assert_eq!(s.ai_enhancement_mode, AiEnhancementMode::Smart);
        assert_eq!(s.ai_polish_style, None);
        let mut form = SettingsForm::from_settings(&s);
        assert_eq!(form.ai_polish_style, AiPolishStyle::Clean);
        form.ai_polish_style = AiPolishStyle::Minimal;
        form.apply_to(&mut s);
        assert_eq!(s.ai_enhancement_mode, AiEnhancementMode::Smart);
        assert_eq!(s.ai_polish_style, Some(AiPolishStyle::Minimal));
        assert_eq!(s.resolved_polish_style(), AiPolishStyle::Minimal);

        form.ai_polish_style = AiPolishStyle::Clean;
        form.apply_to(&mut s);
        assert_eq!(
            s.ai_polish_style, None,
            "matching Smart default must omit aiPolishStyle"
        );

        form.ai_mode = AiEnhancementMode::Off;
        form.ai_polish_style = AiPolishStyle::Minimal;
        form.apply_to(&mut s);
        assert_eq!(s.ai_enhancement_mode, AiEnhancementMode::Off);
        assert_eq!(s.ai_polish_style, None);
    }

    #[test]
    fn apply_gemini_writes_remote_model_not_ollama_field() {
        let mut s = LailaisaySettings::default();
        s.selected_ai_model = "gemma3".into();
        s.selected_remote_model = "compound-beta-mini".into();
        let mut form = SettingsForm::from_settings(&s);
        form.ai_provider = AiProviderType::Gemini;
        form.selected_remote_model = "gemini-2.5-flash".into();
        form.apply_to(&mut s);
        assert_eq!(s.ai_provider_type, AiProviderType::Gemini);
        assert_eq!(s.selected_remote_model, "gemini-2.5-flash");
        assert_eq!(s.selected_ai_model, "gemma3");
        assert!(s.gemini_api_key.is_empty());
    }

    #[test]
    fn apply_remote_api_keys_roundtrip() {
        let mut s = LailaisaySettings::default();
        assert!(s.groq_api_key.is_empty());
        assert!(s.gemini_api_key.is_empty());
        let mut form = SettingsForm::from_settings(&s);
        form.groq_api_key = "  file-groq-key  ".into();
        form.gemini_api_key = "file-gemini-key".into();
        form.apply_to(&mut s);
        assert_eq!(s.groq_api_key, "file-groq-key");
        assert_eq!(s.gemini_api_key, "file-gemini-key");

        let loaded = SettingsForm::from_settings(&s);
        assert_eq!(loaded.groq_api_key, "file-groq-key");
        assert_eq!(loaded.gemini_api_key, "file-gemini-key");
        assert!(!loaded.is_dirty());
    }

    #[test]
    fn form_loads_file_keys_not_env_resolved() {
        let mut s = LailaisaySettings::default();
        s.groq_api_key = "file-groq".into();
        s.gemini_api_key = "file-gemini".into();
        let saved: Vec<(&str, Option<String>)> = [
            "TOK_GROQ_API_KEY",
            "GROQ_API_KEY",
            "TOK_GEMINI_API_KEY",
            "GEMINI_API_KEY",
            "GOOGLE_API_KEY",
        ]
        .into_iter()
        .map(|k| (k, std::env::var(k).ok()))
        .collect();
        std::env::set_var("TOK_GROQ_API_KEY", "env-groq");
        std::env::set_var("TOK_GEMINI_API_KEY", "env-gemini");

        let form = SettingsForm::from_settings(&s);
        assert_eq!(form.groq_api_key, "file-groq");
        assert_eq!(form.gemini_api_key, "file-gemini");
        assert_eq!(s.groq_api_key().as_deref(), Some("env-groq"));
        assert_eq!(s.gemini_api_key().as_deref(), Some("env-gemini"));

        for (k, v) in saved {
            match v {
                Some(val) => std::env::set_var(k, val),
                None => std::env::remove_var(k),
            }
        }
    }

    #[test]
    fn empty_ai_model_falls_back_to_gemma3() {
        let mut s = LailaisaySettings::default();
        s.selected_ai_model = "keep-me".into();
        let mut form = SettingsForm::from_settings(&s);
        form.selected_ai_model = "   ".into();
        form.apply_to(&mut s);
        assert_eq!(s.selected_ai_model, "gemma3");
    }

    #[test]
    fn fn_only_hotkey_label() {
        let hk = HotKey {
            key: None,
            modifiers: Modifiers::new([Modifier::Fn]),
        };
        let label = format_hotkey(&hk);
        assert!(label.contains("fn"), "{label}");
        assert!(label.contains("modifier-only"), "{label}");
    }

    #[test]
    fn catalog_row_mentions_size_and_state() {
        let label = super::catalog_row_label(crate::models::catalog_by_id("medium").unwrap());
        assert!(label.contains("medium"), "{label}");
        assert!(label.contains("GB") || label.contains("MB"), "{label}");
        assert!(
            label.contains("已下載") || label.contains("未下載"),
            "{label}"
        );
    }

    #[test]
    fn default_hotkey_label_has_chord() {
        let hk = HotKey {
            key: Some(Key::Space),
            modifiers: Modifiers::new([Modifier::Command, Modifier::Shift]),
        };
        let label = format_hotkey(&hk);
        assert!(
            label.contains('⌘') || label.to_ascii_lowercase().contains("space"),
            "{label}"
        );
    }

    #[test]
    fn speak_to_edit_default_label_is_option_shift_space() {
        let label = format_hotkey(&HotKey::default_edit());
        assert!(
            label.contains(if cfg!(target_os = "windows") {
                "Alt"
            } else {
                "⌥"
            }),
            "{label}"
        );
        assert!(
            label.contains(if cfg!(target_os = "windows") {
                "Shift"
            } else {
                "⇧"
            }),
            "{label}"
        );
        assert!(label.to_ascii_lowercase().contains("space"), "{label}");
    }

    #[test]
    fn status_strip_maps_pipeline_states() {
        assert!(status_appearance("recording").0.contains("錄音"));
        assert!(status_appearance("transcribing").0.contains("辨識"));
        assert!(status_appearance("enhancing").0.contains("潤稿"));
        assert!(status_appearance("pasted").0.contains("已貼上"));
        assert!(status_appearance("idle").0.contains("待命"));
        let (err, color) = status_appearance("Ollama unavailable — local filters only");
        assert!(err.contains("錯誤"), "{err}");
        assert_eq!(color, DANGER);
        assert_eq!(status_appearance("recording").1, WARN);
        assert_eq!(status_appearance("pasted").1, SUCCESS);
        assert_eq!(status_appearance("no speech").0, "待命");
        assert_eq!(status_appearance("hold longer").0, "請按久一點");
        assert_eq!(status_appearance("no speech").1, MUTED);

        let (need, need_color) = status_appearance("needs Accessibility");
        assert_eq!(need, "需要權限");
        assert_eq!(need_color, DANGER);
        assert_eq!(status_appearance("needs permission").0, "需要權限");
        assert_eq!(status_appearance("Accessibility").0, "需要權限");
        assert_eq!(
            status_detail("needs Accessibility", "fn", "tiny.bin", "gemma3"),
            "系統設定 › 隱私權與安全性 › 輔助使用，授權 lailaisay.app"
        );
        assert_eq!(
            status_detail("permission", "fn", "tiny.bin", "gemma3"),
            "系統設定 › 隱私權與安全性 › 輔助使用，授權 lailaisay.app"
        );

        let idle_detail = status_detail(
            "idle",
            "fn",
            "ggml-model-whisper-small.bin",
            "gemma4:12b-mlx",
        );
        assert_eq!(idle_detail, "按住 fn 說話");
        assert!(!idle_detail.contains("resident"));
        assert!(!idle_detail.contains("/Users/"));
        assert!(!idle_detail.contains("/Library/"));

        let transcribe = status_detail(
            "transcribing",
            "fn",
            "ggml-model-whisper-small.bin",
            "gemma4:12b-mlx",
        );
        assert_eq!(transcribe, "whisper.cpp · ggml-model-whisper-small.bin");
        assert!(!transcribe.contains('/'));

        let err_detail = status_detail(
            "Ollama unavailable — local filters only",
            "fn",
            "tiny.bin",
            "gemma3",
        );
        assert_eq!(err_detail, "Ollama unavailable — local filters only");
        assert!(!is_path_like(&err_detail));

        // Chip never leaks the internal English status string.
        assert_eq!(status_appearance("needs Accessibility").0, "需要權限");
        assert_ne!(status_appearance("no-tap").0, "no-tap");
    }

    #[test]
    fn resident_note_uses_filename_not_full_path() {
        let path = PathBuf::from(
            "/Users/laihenyi/Library/Application Support/com.kitlangton.Hex/models/ggml-model-whisper-small.bin",
        );
        let models = vec![crate::models::FoundModel {
            path: path.clone(),
            source: crate::models::ModelSource::MacWhisper,
        }];
        let note = resident_load_note(
            &format!("resident: {}", path.display()),
            &models,
            path.to_str().unwrap(),
        )
        .expect("note");
        assert_eq!(note, "目前載入：ggml-model-whisper-small.bin（MacWhisper）");
        assert_eq!(
            resident_load_note(
                "device: 自動：首次辨識偵測 GPU",
                &[],
                "/models/ggml-small.bin"
            ),
            Some("目前選擇：ggml-small.bin（lailaisay）".into())
        );

        assert!(!note.contains("/Users/"));
        assert!(!note.contains("resident:"));
        assert_eq!(
            resident_load_note("dummy (no model file)", &[], ""),
            Some("目前載入：dummy（無模型）".into())
        );
        assert_eq!(model_inventory_note(2), "2 個本機 ggml 模型");
    }

    #[test]
    fn tilde_path_abbreviates_home() {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/demo".into());
        let path = PathBuf::from(&home).join("Documents/hex_custom_words.json");
        let shown = tilde_home_path(&path);
        assert!(shown.starts_with("~/"), "{shown}");
        assert!(shown.contains("hex_custom_words.json"), "{shown}");
        assert!(!shown.contains("/Users/"), "{shown}");
        assert!(!shown.contains("/home/"), "{shown}");
    }

    #[test]
    fn tray_glyph_three_states() {
        assert_eq!(tray_glyph_from_status("idle"), TrayGlyph::Idle);
        assert_eq!(tray_glyph_from_status("recording"), TrayGlyph::Active);
        assert_eq!(
            tray_glyph_from_status("speak-to-edit recording"),
            TrayGlyph::Active
        );
        assert_eq!(tray_glyph_from_status("transcribing"), TrayGlyph::Active);
        assert_eq!(
            tray_glyph_from_status("needs Accessibility"),
            TrayGlyph::Error
        );
    }

    #[test]
    fn form_starts_clean_and_tracks_persistable_dirty() {
        let s = LailaisaySettings::default();
        let mut form = SettingsForm::from_settings(&s);
        assert!(!form.is_dirty(), "load must start clean");

        form.language = "zh".into();
        assert!(form.is_dirty());
        form.mark_clean();
        assert!(!form.is_dirty());

        form.prefer_traditional_chinese = !form.prefer_traditional_chinese;
        assert!(form.is_dirty());
        form.mark_clean();

        form.use_double_tap_only = !form.use_double_tap_only;
        assert!(form.is_dirty());
        form.mark_clean();

        form.show_dock_icon = !form.show_dock_icon;
        assert!(form.is_dirty());
        form.mark_clean();

        form.minimize_to_menu_bar_on_launch = !form.minimize_to_menu_bar_on_launch;
        assert!(form.is_dirty());
        form.mark_clean();

        form.minimum_key_time = 1.25;
        assert!(form.is_dirty());
        form.mark_clean();

        form.selected_model = "/tmp/ggml-tiny.bin".into();
        assert!(form.is_dirty());
        form.mark_clean();

        form.ai_mode = AiEnhancementMode::Off;
        assert!(form.is_dirty());
        form.mark_clean();

        form.ai_polish_style = AiPolishStyle::Minimal;
        assert!(form.is_dirty());
        form.mark_clean();

        form.ai_provider = AiProviderType::Groq;
        assert!(form.is_dirty());
        form.mark_clean();

        form.selected_ai_model = "gemma3:12b".into();
        assert!(form.is_dirty());
        form.mark_clean();

        form.selected_remote_model = "gemini-2.5-flash".into();
        assert!(form.is_dirty());
        form.mark_clean();

        form.groq_api_key = "file-groq-key".into();
        assert!(form.is_dirty());
        form.mark_clean();

        form.gemini_api_key = "file-gemini-key".into();
        assert!(form.is_dirty());
        form.mark_clean();

        form.groq_api_key = "  file-groq-key  ".into();
        assert!(
            !form.is_dirty(),
            "trimmed API key must match the saved snapshot"
        );

        form.enable_structured_output = !form.enable_structured_output;
        assert!(form.is_dirty());
        form.mark_clean();

        form.auto_learn_from_corrections = !form.auto_learn_from_corrections;
        assert!(form.is_dirty());
        form.mark_clean();
        assert!(!form.is_dirty());
    }

    #[test]
    fn dictionary_and_catalog_do_not_mark_settings_dirty() {
        let s = LailaisaySettings::default();
        let mut form = SettingsForm::from_settings(&s);
        form.selected_catalog_id = "medium".into();
        form.new_original = "原文".into();
        form.new_replacement = "取代".into();
        form.dict_file_path = "/tmp/import.json".into();
        form.dictionary
            .add_entry(CustomWordEntry::replacement("foo", "bar"));
        form.pane = SettingsSection::Dict;
        form.download_message = "Downloading".into();
        form.save_message = "辭典已儲存".into();
        assert!(
            !form.is_dirty(),
            "SaveDictionary / catalog pick must not look like unsaved settings"
        );

        form.prefer_traditional_chinese = !form.prefer_traditional_chinese;
        assert!(form.is_dirty());

        form.mark_clean();
        let _ = FormAction::RevealDictionary;
        let _ = FormAction::ReloadDictionary;
        let _ = FormAction::ImportDictionary;
        let _ = FormAction::ExportDictionary;
        form.dictionary
            .merge_by_original(CustomWordDictionary::default());
        assert!(
            !form.is_dirty(),
            "dictionary file actions must not fake 尚未儲存"
        );
    }

    #[test]
    fn dictionary_status_line_and_reload_helpers() {
        let missing = std::env::temp_dir().join(format!(
            "lailaisay-dict-missing-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_file(&missing);
        assert_eq!(
            dictionary_status_line(&missing, 3),
            "找不到檔案 · 目前 3 條規則"
        );
        assert!(matches!(
            reload_dictionary_from_path(&missing),
            Ok(DictionaryReload::Missing)
        ));

        let mut dict = CustomWordDictionary::default();
        dict.add_entry(CustomWordEntry::replacement("原文", "取代"));
        dict.save_path(&missing).unwrap();
        assert_eq!(
            dictionary_status_line(&missing, 1),
            "檔案存在 · 目前 1 條規則"
        );
        match reload_dictionary_from_path(&missing) {
            Ok(DictionaryReload::Loaded(loaded)) => {
                assert_eq!(loaded.entries.len(), 1);
                assert_eq!(loaded.entries[0].replacement, "取代");
            }
            other => panic!("expected loaded dict, got {other:?}"),
        }

        std::fs::write(&missing, "{not json").unwrap();
        let err = reload_dictionary_from_path(&missing).unwrap_err();
        assert!(err.contains("JSON 無效"), "{err}");
        assert!(save_message_is_error(&err), "{err}");

        let import_err = load_imported_dictionary(&missing).unwrap_err();
        assert!(import_err.contains("匯入失敗"), "{import_err}");
        assert!(import_err.contains("JSON 無效"), "{import_err}");
        let _ = std::fs::remove_file(&missing);

        assert_eq!(dictionary_reveal_supported(), cfg!(target_os = "macos"));
        if !cfg!(target_os = "macos") {
            let err = reveal_dictionary_in_file_manager(&missing, &dict).unwrap_err();
            assert!(err.contains("僅 macOS"), "{err}");
            assert!(!missing.exists(), "reveal must not create a file off macOS");
        }

        assert!(fallback_json_path("  ", true).is_none());
        assert!(fallback_json_path("/no/such/lailaisay-import.json", true).is_none());
        assert_eq!(
            fallback_json_path("/tmp/export-new.json", false).as_deref(),
            Some(std::path::Path::new("/tmp/export-new.json"))
        );
    }

    #[test]
    fn dictionary_pane_exposes_file_tools_and_rule_search() {
        let size = design_size();
        let ctx = egui::Context::default();
        theme::apply_visuals(&ctx);
        let mut form = SettingsForm::from_settings(&LailaisaySettings::default());
        form.pane = SettingsSection::Dict;
        form.dictionary
            .add_entry(CustomWordEntry::replacement("SECRET_DICT_TOKEN", "取代"));
        let _ = ctx.run(settings_harness_input(size, vec![]), |ctx| {
            paint_settings(ctx, &mut form);
        });
        let _ = ctx.run(settings_harness_input(size, vec![]), |ctx| {
            paint_settings(ctx, &mut form);
        });
        assert!(ctx
            .read_response(Id::new("lailaisay_dict_file_card"))
            .is_none());
        let toggle = ctx
            .read_response(Id::new("lailaisay_dict_file_tools"))
            .unwrap();
        run_settings(&ctx, &mut form, size, click_events(toggle.rect.center()));
        run_settings(&ctx, &mut form, size, vec![]);
        assert!(
            ctx.read_response(Id::new("lailaisay_dict_file_card"))
                .is_some(),
            "file card should paint"
        );
        assert!(
            ctx.read_response(Id::new("lailaisay_dict_reload"))
                .is_some(),
            "reload button should paint"
        );
        assert!(
            ctx.read_response(Id::new("lailaisay_dict_import"))
                .is_some(),
            "import button should paint"
        );
        assert!(
            ctx.read_response(Id::new("lailaisay_dict_export"))
                .is_some(),
            "export button should paint"
        );
        let finder = ctx.read_response(Id::new("lailaisay_dict_reveal"));
        if cfg!(target_os = "macos") {
            assert!(finder.is_some(), "Finder reveal is macOS-only");
        } else {
            assert!(finder.is_none(), "Finder button must be hidden off macOS");
        }
        assert!(!form.is_dirty());
    }

    #[test]
    fn active_model_label_prefers_remote_for_gemini() {
        assert_eq!(
            active_ai_model_label(AiProviderType::Ollama, "gemma4:12b", "ignored"),
            "gemma4:12b"
        );
        assert_eq!(
            active_ai_model_label(AiProviderType::Gemini, "gemma3", "gemini-2.5-flash"),
            "gemini-2.5-flash"
        );
        assert_eq!(
            active_ai_model_label(AiProviderType::Gemini, "gemma3", "  "),
            "gemini-2.5-flash"
        );
        assert_eq!(
            active_ai_model_label(AiProviderType::Groq, "gemma3", ""),
            "compound-beta-mini"
        );
    }

    #[test]
    fn empty_ai_model_matches_saved_gemma3_default() {
        let mut s = LailaisaySettings::default();
        s.selected_ai_model = "gemma3".into();
        let mut form = SettingsForm::from_settings(&s);
        assert!(!form.is_dirty());
        form.selected_ai_model = "   ".into();
        assert!(
            !form.is_dirty(),
            "apply_to writes gemma3 for a blank AI model, so that is not unsaved"
        );
    }

    #[test]
    fn privacy_settings_urls_use_current_macos_anchors() {
        let acc = privacy_settings_urls(PrivacyPane::Accessibility);
        assert!(acc[0].contains("PrivacySecurity.extension"));
        assert!(acc[0].contains("Privacy_Accessibility"));
        assert!(acc[1].contains("com.apple.preference.security"));
        assert!(acc[1].contains("Privacy_Accessibility"));

        assert!(PrivacyPane::Microphone
            .modern_url()
            .contains("Privacy_Microphone"));
        assert!(
            PrivacyPane::InputMonitoring
                .modern_url()
                .contains("Privacy_ListenEvent"),
            "{}",
            PrivacyPane::InputMonitoring.modern_url()
        );
        assert!(PrivacyPane::Automation
            .modern_url()
            .contains("Privacy_Automation"));

        let titles: Vec<&str> = PRIVACY_ROWS.iter().map(|r| r.1).collect();
        assert_eq!(titles, ["輔助使用", "麥克風", "輸入監控", "自動化"]);
        assert!(!PERMISSIONS_NOTE_MACOS.contains("未開啟／視系統"));
        assert!(!PERMISSIONS_NOTE_MACOS.contains("不會即時讀取"));
        assert!(PERMISSIONS_NOTE_MACOS.contains("即時讀取"));
        assert!(PERMISSIONS_NOTE_OTHER.contains("僅 macOS"));
    }

    #[test]
    fn permission_hints_explain_consent_and_target_lifecycle() {
        assert!(
            permission_hint(PrivacyPane::Microphone, GrantStatus::NotDetermined)
                .unwrap()
                .contains("首次按住說話")
        );
        assert!(
            permission_hint(PrivacyPane::Automation, GrantStatus::TargetNotRunning)
                .unwrap()
                .contains("System Events")
        );
        assert!(
            permission_hint(PrivacyPane::Automation, GrantStatus::Unknown)
                .unwrap()
                .contains("查詢未成功")
        );
        assert_eq!(
            permission_hint(PrivacyPane::Microphone, GrantStatus::Granted),
            None
        );
        assert_eq!(
            perm_status_label(GrantStatus::NotDetermined, true),
            "尚未詢問"
        );
        assert_eq!(
            perm_status_label(GrantStatus::TargetNotRunning, true),
            "待目標啟動"
        );
    }

    #[test]
    fn perm_lamp_and_label_mapping() {
        assert_eq!(perm_status_label(GrantStatus::Granted, true), "已開啟");
        assert_eq!(perm_status_label(GrantStatus::Denied, true), "未開啟");
        assert_eq!(perm_status_label(GrantStatus::Unknown, true), "偵測失敗");
        assert_eq!(perm_status_label(GrantStatus::Granted, false), "僅 macOS");
        assert_eq!(perm_status_label(GrantStatus::Denied, false), "僅 macOS");
        assert_eq!(perm_lamp_color(GrantStatus::Granted, true), SUCCESS);
        assert_eq!(perm_lamp_color(GrantStatus::Denied, true), DANGER);
        assert_eq!(perm_lamp_color(GrantStatus::Unknown, true), WARN);
        assert_eq!(perm_lamp_color(GrantStatus::Granted, false), MUTED);
        assert_eq!(perm_lamp_color(GrantStatus::Denied, false), MUTED);

        // Consent pending is not a failed probe; an explicit denial wins over old recording evidence.
        assert_eq!(
            lailaisay_input::map_av_authorization_status(0),
            GrantStatus::NotDetermined
        );
        assert_ne!(
            lailaisay_input::map_av_authorization_status(0),
            GrantStatus::Denied
        );
        assert_eq!(
            lailaisay_input::resolve_microphone_grant(true, GrantStatus::Denied),
            GrantStatus::Denied
        );
        assert_eq!(
            lailaisay_input::resolve_accessibility_grant(true, false),
            GrantStatus::Granted
        );
        assert_eq!(
            lailaisay_input::resolve_input_monitoring_grant(true, GrantStatus::Denied, Some(false)),
            GrantStatus::Granted
        );

        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(
                privacy_grant(PrivacyPane::Accessibility),
                GrantStatus::Denied
            );
            assert_eq!(privacy_grant(PrivacyPane::Microphone), GrantStatus::Unknown);
            assert_eq!(
                privacy_grant(PrivacyPane::InputMonitoring),
                GrantStatus::Unknown
            );
            assert_eq!(privacy_grant(PrivacyPane::Automation), GrantStatus::Unknown);
        }
    }

    #[test]
    fn open_privacy_pane_is_macos_only_on_this_host() {
        #[cfg(not(target_os = "macos"))]
        {
            let err = open_macos_privacy_settings(PrivacyPane::Microphone).unwrap_err();
            assert!(err.contains("macOS"), "{err}");
        }
        #[cfg(target_os = "macos")]
        {
            // `open` is present; do not assert success (CI may lack a GUI session).
            let _ = PrivacyPane::Microphone.modern_url();
        }
    }

    fn assert_rect_inside(inner: Rect, outer: Rect) {
        const EPS: f32 = 0.6;
        assert!(
            inner.left() >= outer.left() - EPS
                && inner.right() <= outer.right() + EPS
                && inner.top() >= outer.top() - EPS
                && inner.bottom() <= outer.bottom() + EPS,
            "popup {inner:?} escaped safe {outer:?}"
        );
    }

    fn design_size() -> Vec2 {
        Vec2::new(SETTINGS_INNER_SIZE[0], SETTINGS_INNER_SIZE[1])
    }

    #[test]
    fn popup_safe_rect_stays_inside_rounded_chrome() {
        let size = design_size();
        let screen = Rect::from_min_size(Pos2::ZERO, size);
        let safe = popup_safe_rect(screen);
        assert_rect_inside(safe, screen);
        assert!((safe.width() - (size.x - 2.0 * theme::ROUND_WIN)).abs() < 0.1);
        assert!((safe.height() - (size.y - 2.0 * theme::ROUND_WIN)).abs() < 0.1);
    }

    #[test]
    fn combo_popup_opens_below_when_there_is_room() {
        let screen = Rect::from_min_size(Pos2::ZERO, design_size());
        let safe = popup_safe_rect(screen);
        let button = Rect::from_min_size(Pos2::new(148.0, 180.0), Vec2::new(300.0, ROW_H));
        let place = combo_popup_placement(button, safe, 300.0, COMBO_POPUP_GAP);
        assert!(!place.open_above, "{place:?}");
        assert_eq!(place.pivot, Align2::LEFT_TOP);
        assert_rect_inside(place.rect, safe);
        assert!(place.rect.top() >= button.bottom());
        assert!(place.max_height > COMBO_FLIP_BELOW);
    }

    #[test]
    fn combo_popup_opens_above_when_near_window_bottom() {
        let size = design_size();
        let screen = Rect::from_min_size(Pos2::ZERO, size);
        let safe = popup_safe_rect(screen);
        let button = Rect::from_min_size(Pos2::new(148.0, size.y - 80.0), Vec2::new(300.0, ROW_H));
        let place = combo_popup_placement(button, safe, 300.0, COMBO_POPUP_GAP);
        assert!(place.open_above, "{place:?}");
        assert_eq!(place.pivot, Align2::LEFT_BOTTOM);
        assert_rect_inside(place.rect, safe);
        assert!(place.rect.bottom() <= button.top() + 0.1);
        assert!(
            place.max_height > 80.0,
            "should use the large space above, got {}",
            place.max_height
        );
    }

    #[test]
    fn combo_popup_shifts_left_instead_of_crossing_window_edge() {
        let size = design_size();
        let screen = Rect::from_min_size(Pos2::ZERO, size);
        let safe = popup_safe_rect(screen);
        let button = Rect::from_min_size(Pos2::new(size.x - 270.0, 200.0), Vec2::new(300.0, ROW_H));
        let place = combo_popup_placement(button, safe, 300.0, COMBO_POPUP_GAP);
        assert_rect_inside(place.rect, safe);
        assert!(
            place.rect.right() <= safe.right() + 0.1,
            "right {} vs safe {}",
            place.rect.right(),
            safe.right()
        );
        assert!(place.rect.left() < button.left());
    }

    fn settings_harness_input(size: Vec2, events: Vec<egui::Event>) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            events,
            ..Default::default()
        }
    }

    fn paint_settings(ctx: &egui::Context, form: &mut SettingsForm) {
        paint_settings_with_status(ctx, form, "idle");
    }

    fn paint_settings_with_status(ctx: &egui::Context, form: &mut SettingsForm, live_status: &str) {
        let view = SettingsView {
            live_status,
            stt_note: "",
            last_llm: None,
            last_raw: "",
            last_final: "",
        };
        form.show_window(ctx, &view);
    }

    fn run_settings(
        ctx: &egui::Context,
        form: &mut SettingsForm,
        size: Vec2,
        events: Vec<egui::Event>,
    ) {
        let _ = ctx.run(settings_harness_input(size, events), |ctx| {
            paint_settings(ctx, form);
        });
    }

    fn click_events(pos: Pos2) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            },
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            },
        ]
    }

    #[test]
    fn whisper_combo_popup_stays_inside_settings_window() {
        let size = design_size();
        let ctx = egui::Context::default();
        theme::apply_visuals(&ctx);

        let mut form = SettingsForm::from_settings(&LailaisaySettings::default());
        form.pane = SettingsSection::Whisper;
        form.local_models = (0..12)
            .map(|i| crate::models::FoundModel {
                path: PathBuf::from(format!(
                    "/tmp/ggml-model-whisper-small-extra-long-name-{i}.bin"
                )),
                source: crate::models::ModelSource::MacWhisper,
            })
            .collect();
        form.selected_model = form.local_models[0].path.to_string_lossy().into_owned();

        run_settings(&ctx, &mut form, size, vec![]);

        let button_id = pane_combo_button_id("whisper_model");
        let popup_id = pane_combo_popup_id("whisper_model");
        let button = ctx
            .read_response(button_id)
            .unwrap_or_else(|| panic!("missing whisper combo {button_id:?}"));
        assert!(
            button.rect.width() <= 461.0,
            "button should not grow to the pane edge, got {}",
            button.rect.width()
        );

        run_settings(&ctx, &mut form, size, click_events(button.rect.center()));
        run_settings(&ctx, &mut form, size, vec![]);
        run_settings(&ctx, &mut form, size, vec![]);

        assert!(
            Popup::is_id_open(&ctx, popup_id),
            "whisper combo popup should open"
        );
        let popup = ctx
            .memory(|m| m.area_rect(popup_id))
            .expect("popup area rect");
        let safe = popup_safe_rect(Rect::from_min_size(Pos2::ZERO, size));
        assert_rect_inside(popup, Rect::from_min_size(Pos2::ZERO, size));
        assert_rect_inside(popup, safe);
        assert!(
            popup.height() > 20.0 && popup.width() > 80.0,
            "popup too small: {popup:?}"
        );
    }

    #[test]
    fn permissions_pane_lays_out_at_design_size() {
        let size = design_size();
        let ctx = egui::Context::default();
        theme::apply_visuals(&ctx);
        let mut form = SettingsForm::from_settings(&LailaisaySettings::default());
        form.pane = SettingsSection::Perms;
        run_settings(&ctx, &mut form, size, vec![]);
        assert_eq!(ctx.screen_rect().size(), size);
        assert_eq!(form.pane, SettingsSection::Perms);
    }

    #[test]
    fn gemini_ai_pane_shows_api_key_field() {
        let size = design_size();
        let ctx = egui::Context::default();
        theme::apply_visuals(&ctx);
        let mut form = SettingsForm::from_settings(&LailaisaySettings::default());
        form.pane = SettingsSection::Ai;
        form.ai_provider = AiProviderType::Gemini;
        run_settings(&ctx, &mut form, size, vec![]);
        let label = ctx
            .read_response(api_key_label_id())
            .expect("Gemini pane should paint API 金鑰");
        assert!(label.rect.width() > 8.0 && label.rect.height() > 8.0);
        assert_rect_inside(label.rect, Rect::from_min_size(Pos2::ZERO, size));
        let field = ctx
            .read_response(api_key_field_id())
            .expect("Gemini pane should paint the API key TextEdit");
        assert!(field.rect.width() > 80.0 && field.rect.height() > 10.0);
        assert_rect_inside(field.rect, Rect::from_min_size(Pos2::ZERO, size));
    }

    #[test]
    fn groq_ai_pane_shows_api_key_field() {
        let size = design_size();
        let ctx = egui::Context::default();
        theme::apply_visuals(&ctx);
        let mut form = SettingsForm::from_settings(&LailaisaySettings::default());
        form.pane = SettingsSection::Ai;
        form.ai_provider = AiProviderType::Groq;
        run_settings(&ctx, &mut form, size, vec![]);
        assert!(
            ctx.read_response(api_key_field_id()).is_some(),
            "Groq pane should paint the API key field"
        );
        assert!(ctx.read_response(api_key_label_id()).is_some());
    }

    #[test]
    fn ollama_ai_pane_hides_api_key_field() {
        let size = design_size();
        let ctx = egui::Context::default();
        theme::apply_visuals(&ctx);
        let mut form = SettingsForm::from_settings(&LailaisaySettings::default());
        form.pane = SettingsSection::Ai;
        form.ai_provider = AiProviderType::Ollama;
        run_settings(&ctx, &mut form, size, vec![]);
        assert!(
            ctx.read_response(api_key_field_id()).is_none(),
            "Ollama is local — no API key row"
        );
        assert!(ctx.read_response(api_key_label_id()).is_none());
    }

    #[test]
    fn settings_chrome_still_lays_out_at_design_size() {
        let size = design_size();
        let ctx = egui::Context::default();
        theme::apply_visuals(&ctx);
        let mut form = SettingsForm::from_settings(&LailaisaySettings::default());
        form.pane = SettingsSection::General;
        run_settings(&ctx, &mut form, size, vec![]);
        assert_eq!(ctx.screen_rect().size(), size);
        let safe = popup_safe_rect(ctx.screen_rect());
        assert!((safe.width() - (size.x - 2.0 * theme::ROUND_WIN)).abs() < 0.1);
        let combo = ctx
            .read_response(pane_combo_button_id("output_language"))
            .expect("language combo should paint inside the inset pane");
        assert!(combo.rect.width() > 0.0 && combo.rect.height() > 0.0);
        assert_rect_inside(combo.rect, Rect::from_min_size(Pos2::ZERO, size));
    }

    #[test]
    fn catalog_and_language_combos_open_inside_window() {
        let size = design_size();
        let ctx = egui::Context::default();
        theme::apply_visuals(&ctx);
        let mut form = SettingsForm::from_settings(&LailaisaySettings::default());

        for (pane, salt) in [
            (SettingsSection::Whisper, "catalog_model"),
            (SettingsSection::General, "output_language"),
        ] {
            form.pane = pane;
            run_settings(&ctx, &mut form, size, vec![]);
            let button_id = pane_combo_button_id(salt);
            let popup_id = pane_combo_popup_id(salt);
            let button = ctx
                .read_response(button_id)
                .unwrap_or_else(|| panic!("missing combo {salt}"));
            run_settings(&ctx, &mut form, size, click_events(button.rect.center()));
            run_settings(&ctx, &mut form, size, vec![]);
            assert!(Popup::is_id_open(&ctx, popup_id), "{salt} should open");
            let popup = ctx
                .memory(|m| m.area_rect(popup_id))
                .unwrap_or_else(|| panic!("{salt} area"));
            assert_rect_inside(
                popup,
                popup_safe_rect(Rect::from_min_size(Pos2::ZERO, size)),
            );
            Popup::close_id(&ctx, popup_id);
        }
    }

    fn measure_isolated_pane_height(pane: SettingsSection, width: f32) -> f32 {
        let ctx = egui::Context::default();
        theme::apply_visuals(&ctx);
        let mut form = SettingsForm::from_settings(&LailaisaySettings::default());
        form.pane = pane;
        let view = SettingsView {
            live_status: "needs Accessibility",
            stt_note: "",
            last_llm: None,
            last_raw: "",
            last_final: "",
        };
        let size = Vec2::new(width + 24.0, 2400.0);
        let mut height = 0.0;
        let _ = ctx.run(settings_harness_input(size, vec![]), |ctx| {
            egui::CentralPanel::default()
                .frame(Frame::new().inner_margin(0))
                .show(ctx, |ui| {
                    ui.set_width(width);
                    ui.set_max_width(width);
                    let top = ui.cursor().top();
                    let _ = form.show_pane(ui, &view);
                    height = ui.cursor().top() - top;
                });
        });
        height
    }

    #[test]
    fn every_default_pane_heading_clears_titlebar_and_status() {
        let size = design_size();
        let pane_frame = theme::content_pane_frame();
        let content_width = size.x
            - RAIL_W
            - pane_frame.outer_margin.rightf()
            - pane_frame.inner_margin.leftf()
            - pane_frame.inner_margin.rightf();
        let chrome_bottom = SETTINGS_TITLEBAR_H + SETTINGS_STATUS_H;
        let footer_top = size.y - SETTINGS_FOOTER_H;
        let pane_top = chrome_bottom + pane_frame.outer_margin.topf();
        let heading_min_y = pane_top + pane_frame.inner_margin.topf();
        let content_bottom = footer_top - pane_frame.inner_margin.bottomf();
        let ctx = egui::Context::default();
        theme::apply_visuals(&ctx);

        let mut form = SettingsForm::from_settings(&LailaisaySettings::default());
        let panes = [
            (SettingsSection::General, "一般"),
            (SettingsSection::Whisper, "語音模型"),
            (SettingsSection::Ai, "AI 潤稿"),
            (SettingsSection::Dict, "自訂辭典"),
            (SettingsSection::Perms, "系統權限"),
        ];

        let mut overflow = Vec::new();
        for (pane, heading) in panes {
            form.pane = pane;
            let isolated = measure_isolated_pane_height(pane, content_width);

            // Two passes so wrap/clip settle the same way the live window does.
            let _ = ctx.run(settings_harness_input(size, vec![]), |ctx| {
                paint_settings_with_status(ctx, &mut form, "needs Accessibility");
            });
            let _ = ctx.run(settings_harness_input(size, vec![]), |ctx| {
                paint_settings_with_status(ctx, &mut form, "needs Accessibility");
            });

            let title = ctx
                .read_response(pane_heading_id(heading))
                .unwrap_or_else(|| panic!("{} heading `{heading}` should paint", pane.label()));
            let end = ctx
                .read_response(pane_end_id(pane))
                .unwrap_or_else(|| panic!("{} end marker should paint", pane.label()));

            eprintln!(
                "[lailaisay-app] pane {} isolated_h={:.1} heading={:?} end_bottom={:.1} content_bottom={:.1} window={:.0}x{:.0}",
                pane.label(),
                isolated,
                title.rect,
                end.rect.bottom(),
                content_bottom,
                size.x,
                size.y
            );

            assert!(
                title.rect.height() > 10.0 && title.rect.width() > 8.0,
                "{} heading collapsed: {:?}",
                pane.label(),
                title.rect
            );
            assert!(
                title.rect.top() >= heading_min_y - 1.0,
                "{} heading top {:.1} sits under the titlebar/status inset (min {:.1})",
                pane.label(),
                title.rect.top(),
                heading_min_y
            );
            assert!(
                title.rect.top() >= chrome_bottom + 8.0,
                "{} heading {:?} overlaps the {}px titlebar+status chrome",
                pane.label(),
                title.rect,
                chrome_bottom
            );
            assert!(
                title.rect.bottom() <= end.rect.top() + 0.5,
                "{} heading {:?} must sit above pane end {:?}",
                pane.label(),
                title.rect,
                end.rect
            );
            if end.rect.bottom() > content_bottom + 1.0 {
                overflow.push(format!(
                    "{} end.bottom={:.1} content_bottom={:.1} isolated_h={:.1} heading={:?}",
                    pane.label(),
                    end.rect.bottom(),
                    content_bottom,
                    isolated,
                    title.rect
                ));
            }
            assert_rect_inside(title.rect, Rect::from_min_size(Pos2::ZERO, size));
        }
        // 520×680 is shorter than some default panes (AI 潤稿 / Linux CJK).
        // ScrollArea VisibleWhenNeeded is the required overflow policy.
        if !overflow.is_empty() {
            eprintln!(
                "[lailaisay-app] panes using vertical scroll at 520×680:\n{}",
                overflow.join("\n")
            );
        }
        let titlebar = ctx
            .read_response(titlebar_heading_id())
            .expect("in-chrome title lailaisay 設定");
        assert!(
            titlebar.rect.bottom() <= SETTINGS_TITLEBAR_H + 1.0,
            "title should sit in the titlebar, got {:?}",
            titlebar.rect
        );
        assert!(titlebar.rect.top() < SETTINGS_TITLEBAR_H);
        assert!(
            titlebar.rect.left() >= TRAFFIC_LIGHT_INSET,
            "heading {:?} must sit right of the traffic-light inset",
            titlebar.rect
        );
        let drag = ctx
            .read_response(titlebar_drag_id())
            .expect("titlebar drag region");
        assert!(
            drag.rect.left() >= TRAFFIC_LIGHT_INSET - 0.1,
            "drag {:?} must not cover traffic lights",
            drag.rect
        );
    }

    #[test]
    fn viewport_builder_uses_transparent_titlebar_and_r3_size() {
        let vp = settings_viewport_builder();
        assert_eq!(vp.title.as_deref(), Some("lailaisay 設定"));
        assert_eq!(vp.titlebar_shown, Some(false));
        assert_eq!(vp.titlebar_buttons_shown, Some(true));
        assert_eq!(vp.fullsize_content_view, Some(true));
        assert_eq!(vp.title_shown, Some(false));
        assert_eq!(SETTINGS_INNER_SIZE, [760.0, 760.0]);
        assert_eq!(SETTINGS_MIN_SIZE, [640.0, 600.0]);
        assert_eq!(vp.inner_size, Some(Vec2::new(760.0, 760.0)));
        assert_eq!(vp.min_inner_size, Some(Vec2::new(640.0, 600.0)));
        assert_eq!(
            GLASS_EDGE,
            Color32::from_rgba_unmultiplied(255, 255, 255, 46)
        );
        assert_ne!(GLASS_EDGE, Color32::from_rgb(186, 187, 188));
    }

    #[test]
    fn traffic_lights_are_ltr_close_minimize_zoom_inside_inset() {
        let titlebar = Rect::from_min_size(Pos2::ZERO, Vec2::new(520.0, SETTINGS_TITLEBAR_H));
        let close = traffic_light_hit_rect(titlebar, TrafficLight::Close);
        let minimize = traffic_light_hit_rect(titlebar, TrafficLight::Minimize);
        let zoom = traffic_light_hit_rect(titlebar, TrafficLight::Zoom);
        assert!(close.center().x < minimize.center().x);
        assert!(minimize.center().x < zoom.center().x);
        assert!((close.center().x - 14.0).abs() < 0.1);
        assert!((minimize.center().x - 34.0).abs() < 0.1);
        assert!((zoom.center().x - 54.0).abs() < 0.1);
        assert!(zoom.right() <= TRAFFIC_LIGHT_INSET);
        assert!(close.left() >= 0.0);
        assert!(close.top() >= titlebar.top());
        assert!(close.bottom() <= titlebar.bottom());
    }

    #[test]
    fn traffic_light_commands_match_macos_actions() {
        assert_eq!(
            traffic_light_command(TrafficLight::Close, false),
            ViewportCommand::Close
        );
        assert_eq!(
            traffic_light_command(TrafficLight::Minimize, false),
            ViewportCommand::Close,
            "minimize ≡ hide-to-tray (same Close → settings_visible=false path)"
        );
        assert_eq!(
            traffic_light_command(TrafficLight::Zoom, false),
            ViewportCommand::Maximized(true)
        );
        assert_eq!(
            traffic_light_command(TrafficLight::Zoom, true),
            ViewportCommand::Maximized(false)
        );
    }

    fn viewport_commands_from(output: &egui::FullOutput) -> Vec<ViewportCommand> {
        output
            .viewport_output
            .values()
            .flat_map(|vp| vp.commands.iter().cloned())
            .collect()
    }

    #[test]
    fn clicking_traffic_lights_sends_viewport_commands() {
        let size = design_size();
        let ctx = egui::Context::default();
        theme::apply_visuals(&ctx);
        let mut form = SettingsForm::from_settings(&LailaisaySettings::default());
        run_settings(&ctx, &mut form, size, vec![]);

        let titlebar = Rect::from_min_size(Pos2::ZERO, Vec2::new(size.x, SETTINGS_TITLEBAR_H));
        let cases = [
            (TrafficLight::Close, ViewportCommand::Close),
            (TrafficLight::Minimize, ViewportCommand::Close),
            (TrafficLight::Zoom, ViewportCommand::Maximized(true)),
        ];
        for (light, expected) in cases {
            let pos = traffic_light_hit_rect(titlebar, light).center();
            let output = ctx.run(settings_harness_input(size, click_events(pos)), |ctx| {
                paint_settings(ctx, &mut form);
            });
            let cmds = viewport_commands_from(&output);
            assert!(
                cmds.contains(&expected),
                "{light:?} click should send {expected:?}, got {cmds:?}"
            );
            assert!(
                !cmds.contains(&ViewportCommand::StartDrag),
                "{light:?} click must not start a window drag"
            );
        }
    }

    #[test]
    fn min_hold_row_uses_specified_helper() {
        assert_eq!(MIN_HOLD_HELPER, "低於此秒數不啟動錄音。");
        assert_eq!(MINIMIZE_ON_LAUNCH_LABEL, "啟動時縮到選單列");
    }

    #[test]
    fn settings_form_remaps_macwhisper_small_on_load_and_save() {
        let root = std::env::temp_dir().join(format!(
            "lailaisay-form-remap-{}-{}",
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

        let small =
            "/Users/x/Library/Application Support/MacWhisper/models/ggml-model-whisper-small.bin";
        let mut s = LailaisaySettings::default();
        s.selected_whisper_model = Some(small.into());
        let mut form = SettingsForm::from_settings(&s);
        assert!(
            form.selected_model.ends_with("ggml-large-v3-turbo.bin"),
            "Settings must show turbo, not MacWhisper small: {}",
            form.selected_model
        );

        form.selected_model = small.into();
        form.apply_to(&mut s);
        assert_eq!(
            s.selected_whisper_model.as_deref(),
            Some(turbo.to_string_lossy().as_ref())
        );

        match prev {
            Some(v) => std::env::set_var("TOK_MODELS_DIR", v),
            None => std::env::remove_var("TOK_MODELS_DIR"),
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
