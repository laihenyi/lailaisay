//! eframe host: settings window + tray / global hotkey / resident STT
//! (macOS CGEvent tap, Windows WH_KEYBOARD_LL).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use lailaisay_core::hotkey::RECORD_START_DELAY_SECS;
use lailaisay_core::settings_path;

#[allow(dead_code)] // used from end_recording (live tap on macOS / Windows)
use crate::desktop::stop_and_dispatch;
use crate::desktop::{should_convert_minimize_to_hide, should_hide_to_tray};
use crate::mic_start::delayed_start_still_wanted;
#[cfg(feature = "mic")]
use crate::mic_start::start_live_recorder_on_this_thread;
use crate::models::download_catalog_model;
use crate::pipeline::load_app_context;
use crate::session::{Session, SessionAction};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use crate::settings_ui::tray_glyph_from_status;
use crate::settings_ui::{
    fallback_json_path, load_imported_dictionary, pick_json_file, reload_dictionary_from_disk,
    reveal_dictionary_in_file_manager, DictionaryReload, FormAction, RevealOutcome, SettingsForm,
    SettingsView,
};
use crate::shared::{set_shared_status, spawn_worker, SharedState, Work};

#[cfg(any(target_os = "macos", target_os = "windows"))]
use crate::desktop::{build_tray, status_icon};
#[cfg(target_os = "macos")]
use crate::macos_runtime::{apply_activation_policy, workspace_frontmost};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use lailaisay_core::HotKeyProcessor;
#[cfg(all(target_os = "macos", feature = "appstore"))]
use lailaisay_input::{MacOsCarbonHotkey, MICROPHONE_HELP};
#[cfg(all(target_os = "macos", not(feature = "appstore")))]
use lailaisay_input::{MacOsEventTap, ACCESSIBILITY_HELP, MICROPHONE_HELP};
/// Global hotkey source on macOS: CGEvent tap, or Carbon hot keys in the sandboxed App Store build.
#[cfg(all(target_os = "macos", not(feature = "appstore")))]
type MacHotkeySource = MacOsEventTap;
#[cfg(all(target_os = "macos", feature = "appstore"))]
type MacHotkeySource = MacOsCarbonHotkey;
#[cfg(target_os = "windows")]
use lailaisay_input::{WindowsEventTap, WINDOWS_HOTKEY_HELP, WINDOWS_MICROPHONE_HELP};
#[cfg(all(target_os = "macos", not(feature = "appstore")))]
use lailaisay_paste::PASTE_TCC_HELP;
#[cfg(target_os = "windows")]
use lailaisay_paste::WINDOWS_PASTE_HELP;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use tray_icon::menu::{MenuEvent, MenuId, MenuItem};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use tray_icon::TrayIcon;

#[allow(dead_code)] // constructed from the macOS / Windows tap path
enum Fire {
    DelayedStart,
}

enum DownloadMsg {
    Done(Result<std::path::PathBuf, String>),
}

pub struct RuntimeOpts {
    pub enable_tap: bool,
    pub show_settings: bool,
}

/// Resolve tray / Settings launch from persisted settings plus `--settings`.
pub fn runtime_opts_from_settings(
    settings: &lailaisay_core::LailaisaySettings,
    enable_tap: bool,
    force_settings: bool,
) -> RuntimeOpts {
    RuntimeOpts {
        enable_tap,
        show_settings: settings.show_settings_on_launch(force_settings),
    }
}

pub fn run() -> Result<()> {
    let settings = lailaisay_core::LailaisaySettings::for_launch(&settings_path());
    run_with(runtime_opts_from_settings(&settings, true, false))
}

pub fn run_with(opts: RuntimeOpts) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        eprintln!(
            "lailaisay menu-bar (Rust). Bundle id: {}",
            lailaisay_core::MACOS_BUNDLE_ID
        );
        #[cfg(not(feature = "appstore"))]
        {
            eprintln!("{ACCESSIBILITY_HELP}");
            eprintln!("{PASTE_TCC_HELP}");
        }
        eprintln!("{MICROPHONE_HELP}");
        #[cfg(feature = "appstore")]
        eprintln!(
            "[lailaisay-app] App Store build: Carbon hotkey, clipboard output (Cmd+V to paste)."
        );
    }
    #[cfg(target_os = "windows")]
    {
        eprintln!("lailaisay (Windows). Settings + notification-area tray + WH_KEYBOARD_LL.");
        eprintln!("{WINDOWS_HOTKEY_HELP}");
        eprintln!("{WINDOWS_MICROPHONE_HELP}");
        eprintln!("{WINDOWS_PASTE_HELP}");
    }

    let mut ctx = load_app_context()?;
    // First GUI launch: persist onboarding so later starts stay in the tray.
    if let Err(e) = ctx.settings.complete_onboarding(&settings_path()) {
        tracing::warn!("could not persist hasCompletedOnboarding ({e})");
    }
    let form = SettingsForm::from_settings(&ctx.settings);
    let shared = Arc::new(Mutex::new(SharedState::from_context(ctx)));
    {
        let mut g = shared.lock().map_err(|e| anyhow!("shared lock: {e}"))?;
        g.ensure_stt();
        eprintln!("[lailaisay-app] STT: {}", g.stt_note);
    }

    let (work_tx, work_rx) = mpsc::channel::<Work>();
    spawn_worker(shared.clone(), work_rx);

    let start_visible = opts.show_settings;
    let native = eframe::NativeOptions {
        viewport: crate::settings_ui::settings_viewport_builder().with_visible(start_visible),
        persist_window: false,
        ..Default::default()
    };

    let shared_for_app = shared;
    let result = eframe::run_native(
        "lailaisay",
        native,
        Box::new(move |cc| {
            install_lailaisay_visuals(&cc.egui_ctx);
            Ok(Box::new(LailaisayHost::new(
                opts,
                shared_for_app,
                work_tx,
                form,
            )))
        }),
    )
    .map_err(|e| anyhow!("eframe: {e}"));
    eprintln!("[lailaisay-app] GUI event loop returned: {result:?}");
    result
}

#[allow(dead_code)] // tap / delayed-start fields are live on macOS
struct LailaisayHost {
    opts: RuntimeOpts,
    shared: Arc<Mutex<SharedState>>,
    work_tx: mpsc::Sender<Work>,
    form: SettingsForm,
    settings_visible: bool,
    should_quit: bool,
    did_init: bool,
    #[cfg(target_os = "macos")]
    was_app_active: Option<bool>,
    session: Session,
    cancel_start: Arc<AtomicBool>,
    fire_tx: mpsc::Sender<Fire>,
    fire_rx: mpsc::Receiver<Fire>,
    rec_started_at: Option<Instant>,
    download_tx: mpsc::Sender<DownloadMsg>,
    download_rx: mpsc::Receiver<DownloadMsg>,
    #[cfg(feature = "mic")]
    live: Option<lailaisay_stt::record::LiveRecorder>,
    #[cfg(target_os = "macos")]
    tap: Option<MacHotkeySource>,
    #[cfg(target_os = "windows")]
    tap: Option<WindowsEventTap>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    tray: Option<TrayIcon>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    status_item: Option<MenuItem>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    settings_id: Option<MenuId>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    edit_help_id: Option<MenuId>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    quit_id: Option<MenuId>,
    pending_is_edit: bool,
    edit_selection: Option<String>,
}

impl LailaisayHost {
    fn new(
        opts: RuntimeOpts,
        shared: Arc<Mutex<SharedState>>,
        work_tx: mpsc::Sender<Work>,
        form: SettingsForm,
    ) -> Self {
        let (hotkey, edit_hotkey, double_tap) = {
            let g = shared.lock().unwrap_or_else(|e| e.into_inner());
            (
                g.settings.hotkey.clone(),
                g.settings.edit_hotkey.clone(),
                g.settings.use_double_tap_only,
            )
        };
        let (fire_tx, fire_rx) = mpsc::channel();
        let (download_tx, download_rx) = mpsc::channel();
        let mut form = form;
        if let Ok(g) = shared.lock() {
            form.load_dictionary(&g.dictionary);
        }
        Self {
            settings_visible: opts.show_settings,
            should_quit: false,
            did_init: false,
            #[cfg(target_os = "macos")]
            was_app_active: None,
            session: Session::with_edit_hotkey(hotkey, double_tap, edit_hotkey),
            cancel_start: Arc::new(AtomicBool::new(false)),
            fire_tx,
            fire_rx,
            rec_started_at: None,
            download_tx,
            download_rx,
            opts,
            shared,
            work_tx,
            form,
            #[cfg(feature = "mic")]
            live: None,
            #[cfg(target_os = "macos")]
            tap: None,
            #[cfg(target_os = "windows")]
            tap: None,
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            tray: None,
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            status_item: None,
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            settings_id: None,
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            edit_help_id: None,
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            quit_id: None,
            pending_is_edit: false,
            edit_selection: None,
        }
    }

    #[allow(dead_code)] // tray 「開啟設定」 on macOS / Windows; Linux starts visible
    fn open_settings(&mut self, ctx: &egui::Context) {
        if let Ok(g) = self.shared.lock() {
            self.form = SettingsForm::from_settings(&g.settings);
            self.form.load_dictionary(&g.dictionary);
        }
        self.settings_visible = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    /// Hide-to-tray: `orderOut` / `Visible(false)`, not OS miniaturize.
    /// Dock reactivation and HUD `orderFront` restore miniaturized windows.
    fn hide_settings_to_tray(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        self.settings_visible = false;
    }

    fn handle_form_action(&mut self, action: FormAction) {
        match action {
            FormAction::None => {}
            FormAction::Save => self.save_settings(),
            FormAction::DownloadCatalog => self.start_catalog_download(),
            FormAction::Quit => {
                eprintln!("[lailaisay-app] exit requested: settings Quit button");
                self.should_quit = true;
            }
            FormAction::SaveDictionary => {
                let _ = self.save_dictionary();
            }
            FormAction::PromoteLast => self.promote_last_correction(),
            FormAction::RefreshOllama => self.refresh_ollama_models(),
            FormAction::CopyLastRaw => self.copy_last_raw(),
            FormAction::RevealDictionary => self.reveal_dictionary(),
            FormAction::ReloadDictionary => self.reload_dictionary(),
            FormAction::ImportDictionary => self.import_dictionary(),
            FormAction::ExportDictionary => self.export_dictionary(),
        }
    }

    fn copy_last_raw(&mut self) {
        let raw = self
            .shared
            .lock()
            .map(|g| g.last_stt.clone())
            .unwrap_or_default();
        if raw.trim().is_empty() {
            self.form.save_message = "尚無生稿。請先說一次。".into();
            return;
        }
        match lailaisay_paste::copy_text(&raw) {
            Ok(()) => self.form.save_message = "已複製生稿".into(),
            Err(e) => self.form.save_message = format!("複製失敗：{e}"),
        }
    }

    fn refresh_ollama_models(&mut self) {
        self.form.ollama_refresh_message = "正在查詢 Ollama /api/tags…".into();
        match lailaisay_enhance::list_ollama_models_blocking(None) {
            Ok(models) => {
                self.form.ollama_reachable = Some(true);
                if models.is_empty() {
                    self.form.ollama_models.clear();
                    self.form.ollama_refresh_message =
                        "Ollama 已連線，但 /api/tags 沒有模型。可手動輸入 selectedAiModel。".into();
                } else {
                    self.form.ollama_models = models;
                    self.form.ollama_refresh_message =
                        format!("已載入 {} 個 Ollama 模型", self.form.ollama_models.len());
                }
            }
            Err(e) => {
                self.form.ollama_reachable = Some(false);
                self.form.ollama_models.clear();
                self.form.ollama_refresh_message = format!("Ollama 未連線（維持文字欄）：{e}");
            }
        }
    }

    fn save_dictionary(&mut self) -> bool {
        let dict = self.form.dictionary.clone();
        match dict.save_path(&lailaisay_core::custom_words_path()) {
            Ok(()) => {
                if let Ok(mut g) = self.shared.lock() {
                    g.dictionary = dict;
                }
                self.form.save_message = format!(
                    "已儲存辭典，共 {} 條規則",
                    self.form.dictionary.entries.len()
                );
                true
            }
            Err(e) => {
                if let Ok(g) = self.shared.lock() {
                    self.form.load_dictionary(&g.dictionary);
                }
                self.form.save_message = format!("辭典儲存失敗：{e}（已保留原規則）");
                false
            }
        }
    }

    fn apply_dictionary(&mut self, dict: lailaisay_core::CustomWordDictionary, message: String) {
        self.form.load_dictionary(&dict);
        self.form.dictionary_undo = None;
        if let Ok(mut g) = self.shared.lock() {
            g.dictionary = dict;
        }
        self.form.save_message = message;
    }

    fn reload_dictionary(&mut self) {
        match reload_dictionary_from_disk() {
            Ok(DictionaryReload::Loaded(dict)) => {
                let n = dict.entries.len();
                self.apply_dictionary(dict, format!("已重新載入，目前 {n} 條規則"));
            }
            Ok(DictionaryReload::Missing) => {
                self.apply_dictionary(
                    lailaisay_core::CustomWordDictionary::default(),
                    "找不到字典檔，已載入 0 條規則".into(),
                );
            }
            Err(e) => self.form.save_message = e,
        }
    }

    fn reveal_dictionary(&mut self) {
        match reveal_dictionary_in_file_manager(
            &lailaisay_core::custom_words_path(),
            &self.form.dictionary,
        ) {
            Ok(RevealOutcome::Revealed) => {
                self.form.save_message = "已在 Finder 顯示".into();
            }
            Ok(RevealOutcome::CreatedAndRevealed) => {
                self.form.save_message = "已建立字典檔並在 Finder 顯示".into();
            }
            Err(e) => self.form.save_message = e,
        }
    }

    fn import_dictionary(&mut self) {
        let Some(path) = pick_json_file("匯入自訂辭典", false)
            .or_else(|| fallback_json_path(&self.form.dict_file_path, true))
        else {
            if cfg!(target_os = "linux") {
                self.form.save_message = "未選擇檔案。可貼上 JSON 路徑後再按匯入。".into();
            }
            return;
        };
        match load_imported_dictionary(&path) {
            Ok(imported) => {
                self.form.dictionary_undo = Some(self.form.dictionary.clone());
                let stats = self.form.dictionary.merge_by_original(imported);
                if self.save_dictionary() {
                    self.form.save_message = format!(
                        "已匯入並合併，新增 {} 條、更新 {} 條",
                        stats.added, stats.updated
                    );
                }
            }
            Err(e) => self.form.save_message = e,
        }
    }

    fn export_dictionary(&mut self) {
        let Some(path) = pick_json_file("匯出自訂辭典", true)
            .or_else(|| fallback_json_path(&self.form.dict_file_path, false))
        else {
            if cfg!(target_os = "linux") {
                self.form.save_message = "未選擇檔案。可貼上儲存路徑後再按匯出。".into();
            }
            return;
        };
        match self.form.dictionary.save_path(&path) {
            Ok(()) => {
                self.form.save_message = format!("已匯出 {}", path.display());
            }
            Err(e) => self.form.save_message = format!("匯出失敗：{e}"),
        }
    }

    fn promote_last_correction(&mut self) {
        let (stt, final_text) = {
            let g = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            (g.last_stt.clone(), g.last_final.clone())
        };
        if stt.is_empty() || final_text.is_empty() || stt == final_text {
            self.form.save_message = "尚無 STT→定稿可加入。請先說一次。".into();
            return;
        }
        if let Some((orig, corr)) = lailaisay_core::infer_substitution(&stt, &final_text) {
            let mut e = lailaisay_core::CustomWordEntry::replacement(orig, corr);
            e.source = lailaisay_core::EntrySource::Learned;
            self.form.dictionary_undo = Some(self.form.dictionary.clone());
            self.form.dictionary.add_entry(e);
            self.save_dictionary();
        } else {
            self.form.save_message = "上次變更不是明確的單一代換。".into();
        }
    }

    fn save_settings(&mut self) {
        if let Some(error) = self.form.validation_error() {
            self.form.save_message = format!("儲存失敗：{error}");
            return;
        }
        let mut settings = {
            let g = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            g.settings.clone()
        };
        let old_model = settings.selected_whisper_model.clone();
        let old_hotkey = settings.hotkey.clone();
        let old_edit = settings.edit_hotkey.clone();
        let old_double = settings.use_double_tap_only;
        self.form.apply_to(&mut settings);
        if let Some(m) = settings.selected_whisper_model.as_deref() {
            if self.form.selected_model.trim() != m {
                self.form.selected_model = m.to_string();
            }
        }
        match settings.save_path(&settings_path()) {
            Ok(()) => {
                let model_changed = settings.selected_whisper_model != old_model;
                let hotkey_changed = settings.hotkey != old_hotkey
                    || settings.edit_hotkey != old_edit
                    || settings.use_double_tap_only != old_double;
                {
                    let mut g = self.shared.lock().unwrap_or_else(|e| e.into_inner());
                    g.settings = settings.clone();
                    g.dictionary = self.form.dictionary.clone();
                }

                if hotkey_changed && !self.session.recording {
                    self.session = Session::with_edit_hotkey(
                        settings.hotkey.clone(),
                        settings.use_double_tap_only,
                        settings.edit_hotkey.clone(),
                    );
                } else if hotkey_changed {
                    self.session.processor.hotkey = settings.hotkey.clone();
                    self.session.processor.use_double_tap_only = settings.use_double_tap_only;
                    self.session.edit_processor.hotkey = settings.edit_hotkey.clone();
                }
                #[cfg(target_os = "macos")]
                {
                    apply_activation_policy(settings.show_dock_icon);
                }
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                {
                    if let Some(tap) = self.tap.as_mut() {
                        tap.set_hotkey(settings.hotkey.clone(), settings.use_double_tap_only);
                        tap.set_edit_hotkey(settings.edit_hotkey.clone());
                    }
                }
                #[cfg(all(target_os = "macos", feature = "appstore"))]
                {
                    if let Some(err) = self.tap.as_ref().and_then(|t| t.last_error()) {
                        self.form.save_message = format!("已儲存設定，但熱鍵未生效：{err}");
                        set_shared_status(&self.shared, "hotkey unavailable");
                        self.form.mark_clean();
                        return;
                    }
                }
                if model_changed {
                    let _ = self.work_tx.send(Work::ReloadModel);
                    self.form.download_message =
                        "Model will load in the background (resident after first load).".into();
                }
                // Dictionary SaveDictionary / PromoteLast do not call this.
                self.form.mark_clean();
                self.form.save_message = "已儲存設定".into();
            }
            Err(e) => {
                self.form.save_message = format!("儲存失敗：{e}");
            }
        }
    }

    fn start_catalog_download(&mut self) {
        if self.form.downloading {
            return;
        }
        let id = self.form.selected_catalog_id.clone();
        self.form.downloading = true;
        self.form.download_message = format!("Downloading {id}…");
        let tx = self.download_tx.clone();
        thread::spawn(move || {
            let _ = tx.send(DownloadMsg::Done(download_catalog_model(&id)));
        });
    }

    fn poll_download(&mut self) {
        if let Ok(DownloadMsg::Done(result)) = self.download_rx.try_recv() {
            self.form.downloading = false;
            match result {
                Ok(path) => {
                    // With no usable model loaded, activate the download right
                    // away so the next hold-to-talk works without a separate
                    // 儲存設定 step. Never auto-save other pending edits.
                    let activate = !self.form.is_dirty() && {
                        let g = self.shared.lock().unwrap_or_else(|e| e.into_inner());
                        g.stt_is_placeholder()
                    };
                    self.form.selected_model = path.to_string_lossy().into_owned();
                    self.form.refresh_models();
                    if activate {
                        self.save_settings();
                        self.form.download_message = format!(
                            "已下載並啟用 {}，背景載入完成後即可按住說話。",
                            path.display()
                        );
                    } else {
                        self.form.download_message =
                            format!("Downloaded {}. Save to make it active.", path.display());
                    }
                }
                Err(e) => {
                    self.form.download_message = format!("Download failed: {e}");
                }
            }
        }
    }

    fn poll_session(&mut self) {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            // Drain the tap first so `self.tap` is not borrowed while we
            // mutate `self.session` / call `apply_session_action`.
            let pending = match self.tap.as_mut() {
                Some(tap) => drain_tap_messages(|| tap.recv_message()),
                None => Vec::new(),
            };
            for msg in pending {
                let action = if msg.edit_output.is_some() && !self.session.recording {
                    self.session
                        .handle_edit_output(msg.edit_output, msg.edit_state.unwrap_or(msg.state))
                } else {
                    self.session.handle_output(msg.output, msg.state)
                };
                self.apply_session_action(action);
            }
        }
        while let Ok(Fire::DelayedStart) = self.fire_rx.try_recv() {
            if !delayed_start_still_wanted(self.cancel_start.load(Ordering::SeqCst)) {
                continue;
            }
            self.session.recording = true;
            self.session.pending_start = false;
            self.rec_started_at = Some(Instant::now());
            self.begin_recording();
        }
    }

    #[allow(dead_code)] // CGEvent tap → session on macOS
    fn apply_session_action(&mut self, action: SessionAction) {
        match action {
            SessionAction::ArmDelayedStart => {
                self.pending_is_edit = self.session.recording_edit;
                // Tray/status Active on key-down. The 0.2s timer still gates
                // begin_recording so a double-tap can cancel.
                self.show_hold_feedback();
                if self.pending_is_edit && !self.capture_edit_selection() {
                    self.session.recording = false;
                    self.session.pending_start = false;
                    self.session.recording_edit = false;
                    self.sync_status_feedback();
                    return;
                }
                self.capture_paste_target();
                self.cancel_start.store(false, Ordering::SeqCst);
                let cancel = self.cancel_start.clone();
                let fire = self.fire_tx.clone();
                thread::spawn(move || {
                    thread::sleep(Duration::from_secs_f64(RECORD_START_DELAY_SECS));
                    if !cancel.load(Ordering::SeqCst) {
                        let _ = fire.send(Fire::DelayedStart);
                    }
                });
            }
            SessionAction::CancelDelayedStart => {
                self.cancel_start.store(true, Ordering::SeqCst);
                set_shared_status(&self.shared, "idle");
                self.sync_status_feedback();
            }
            SessionAction::StartNow => {
                self.pending_is_edit = self.session.recording_edit;
                self.show_hold_feedback();
                if self.pending_is_edit && !self.capture_edit_selection() {
                    self.session.recording = false;
                    self.session.recording_edit = false;
                    self.sync_status_feedback();
                    return;
                }
                self.capture_paste_target();
                self.cancel_start.store(true, Ordering::SeqCst);
                self.rec_started_at = Some(Instant::now());
                self.begin_recording();
            }
            SessionAction::Stop => {
                self.cancel_start.store(true, Ordering::SeqCst);
                let minimum = self
                    .shared
                    .lock()
                    .map(|g| g.settings.minimum_key_time)
                    .unwrap_or(0.2);
                self.end_recording(minimum);
                self.sync_status_feedback();
            }
            SessionAction::Cancel => {
                self.cancel_start.store(true, Ordering::SeqCst);
                self.rec_started_at = None;
                #[cfg(feature = "mic")]
                {
                    let _ = self.live.take().map(|r| r.stop());
                }
                set_shared_status(&self.shared, "idle");
                self.sync_status_feedback();
            }
            SessionAction::None => {}
        }
    }

    fn capture_paste_target(&mut self) {
        #[allow(unused_mut)] // NSWorkspace fill is macOS-only
        let mut target = lailaisay_paste::capture_frontmost_target().unwrap_or_default();
        #[cfg(target_os = "macos")]
        if target.bundle_id.is_empty() {
            if let Some((bid, name)) = workspace_frontmost() {
                target.bundle_id = bid;
                if target.app_name.is_none() && !name.is_empty() {
                    target.app_name = Some(name);
                }
            }
        }
        if target.is_empty() {
            eprintln!("[lailaisay-app] paste target at hotkey-down: (none)");
            if let Ok(mut g) = self.shared.lock() {
                g.paste_target = None;
            }
            return;
        }
        eprintln!("[lailaisay-app] paste target at hotkey-down: {target}");
        if let Ok(mut g) = self.shared.lock() {
            g.paste_target = Some(target);
        }
    }

    fn capture_edit_selection(&mut self) -> bool {
        match lailaisay_paste::read_selected_text() {
            Ok(Some(s)) if !s.trim().is_empty() => {
                self.edit_selection = Some(s);
                set_shared_status(&self.shared, "speak-to-edit recording");
                true
            }
            _ => {
                set_shared_status(&self.shared, "Speak to Edit: no selection");
                self.edit_selection = None;
                false
            }
        }
    }

    fn show_hold_feedback(&mut self) {
        if let Some(status) = self.session.hold_feedback_status() {
            set_shared_status(&self.shared, status);
        } else {
            set_shared_status(
                &self.shared,
                crate::session::armed_hold_status(self.pending_is_edit),
            );
        }
        self.sync_status_feedback();
    }

    fn sync_status_feedback(&mut self) {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        self.sync_tray_status();
    }

    fn tray_is_up(&self) -> bool {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            self.tray.is_some()
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            false
        }
    }

    fn begin_recording(&mut self) {
        let missing = self
            .shared
            .lock()
            .map(|g| g.paste_target.is_none())
            .unwrap_or(true);
        if missing {
            self.capture_paste_target();
        }
        // Status + tray first so set_icon is not stuck behind cpal device open.
        // LiveRecorder / cpal Stream is !Send on macOS — open on this thread.
        self.show_hold_feedback();
        #[cfg(feature = "mic")]
        self.open_mic_on_ui_thread();
    }

    /// After tray/status is already Active. Blocks this thread on device open;
    /// does not move the recorder across threads.
    #[cfg(feature = "mic")]
    fn open_mic_on_ui_thread(&mut self) {
        if let Some(old) = self.live.take() {
            let _ = old.stop();
        }
        match start_live_recorder_on_this_thread() {
            Ok(rec) => self.live = Some(rec),
            Err(e) => {
                tracing::error!("mic start: {e}");
                set_shared_status(&self.shared, "mic error");
                self.sync_status_feedback();
            }
        }
    }

    #[allow(dead_code)] // hotkey release on macOS / Windows
    fn end_recording(&mut self, minimum: f64) {
        let edit_sel = if self.pending_is_edit {
            self.edit_selection.take()
        } else {
            None
        };
        self.pending_is_edit = false;
        stop_and_dispatch(
            &self.work_tx,
            &self.shared,
            &mut self.rec_started_at,
            minimum,
            edit_sel,
            #[cfg(feature = "mic")]
            &mut self.live,
        );
    }

    #[cfg(target_os = "macos")]
    fn init_macos(&mut self, ctx: &egui::Context) {
        let show_dock = {
            let g = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            g.settings.show_dock_icon
        };
        apply_activation_policy(show_dock);
        self.attach_tray();
        if self.settings_visible {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        }
        // Ask for microphone consent up front so the app is listed under
        // Privacy & Security → Microphone before the first recording. macOS
        // only shows the dialog while the decision is NotDetermined.
        if lailaisay_input::should_request_microphone(lailaisay_input::microphone_grant()) {
            lailaisay_input::request_microphone_access();
        }

        if self.opts.enable_tap {
            let (hotkey, edit_hotkey, double_tap) = {
                let g = self.shared.lock().unwrap_or_else(|e| e.into_inner());
                (
                    g.settings.hotkey.clone(),
                    g.settings.edit_hotkey.clone(),
                    g.settings.use_double_tap_only,
                )
            };
            #[cfg(not(feature = "appstore"))]
            match MacOsEventTap::with_processor(HotKeyProcessor::new(hotkey.clone(), double_tap)) {
                Ok(t) => {
                    t.set_hotkey(hotkey, double_tap);
                    t.set_edit_hotkey(edit_hotkey);
                    self.tap = Some(t);
                    set_shared_status(&self.shared, "idle");
                    eprintln!(
                        "[lailaisay-app] CGEvent tap running. Default/settings hotkey active."
                    );
                }
                Err(e) => {
                    eprintln!("[lailaisay-app] hotkey tap disabled: {e}");
                    eprintln!(
                        "[lailaisay-app] tray stays up. Grant Accessibility + Input Monitoring, then Quit and relaunch."
                    );
                    set_shared_status(&self.shared, "needs Accessibility");
                }
            }
            #[cfg(feature = "appstore")]
            match MacOsCarbonHotkey::with_processor(HotKeyProcessor::new(
                hotkey.clone(),
                double_tap,
            )) {
                Ok(mut t) => {
                    t.set_hotkey(hotkey.clone(), double_tap);
                    t.set_edit_hotkey(edit_hotkey);
                    match t.last_error() {
                        None => {
                            set_shared_status(&self.shared, "idle");
                            eprintln!(
                                "[lailaisay-app] Carbon hotkey registered ({hotkey}); sandbox build, clipboard output."
                            );
                        }
                        Some(err) => {
                            eprintln!("[lailaisay-app] Carbon hotkey not registered: {err}");
                            set_shared_status(&self.shared, "hotkey unavailable");
                        }
                    }
                    self.tap = Some(t);
                }
                Err(e) => {
                    eprintln!("[lailaisay-app] Carbon hotkey handler failed: {e}");
                    set_shared_status(&self.shared, "hotkey unavailable");
                }
            }
        } else {
            eprintln!("[lailaisay-app] --no-tap: tray only, no CGEvent tap.");
            set_shared_status(&self.shared, "no-tap");
        }
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn attach_tray(&mut self) {
        match build_tray() {
            Ok((t, status_i, sid, eid, qid)) => {
                self.tray = Some(t);
                self.status_item = Some(status_i);
                self.settings_id = Some(sid);
                self.edit_help_id = Some(eid);
                self.quit_id = Some(qid);
            }
            Err(e) => {
                eprintln!("============================================================");
                eprintln!("[lailaisay-app] MENU EXTRA / TRAY FAILED: {e}");
                eprintln!("[lailaisay-app] There is no status-item menu. Settings is opening");
                eprintln!("[lailaisay-app] so you are not left with a blank window.");
                eprintln!("[lailaisay-app] Close Settings quits when the tray is missing.");
                eprintln!("============================================================");
                self.settings_visible = true;
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn init_windows(&mut self, ctx: &egui::Context) {
        self.attach_tray();
        if self.settings_visible {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        if self.opts.enable_tap {
            let (hotkey, edit_hotkey, double_tap) = {
                let g = self.shared.lock().unwrap_or_else(|e| e.into_inner());
                (
                    g.settings.hotkey.clone(),
                    g.settings.edit_hotkey.clone(),
                    g.settings.use_double_tap_only,
                )
            };
            match WindowsEventTap::with_processor(HotKeyProcessor::new(hotkey.clone(), double_tap))
            {
                Ok(t) => {
                    t.set_hotkey(hotkey, double_tap);
                    t.set_edit_hotkey(edit_hotkey);
                    self.tap = Some(t);
                    set_shared_status(&self.shared, "idle");
                    eprintln!(
                        "[lailaisay-app] WH_KEYBOARD_LL running. Hold the settings hotkey (Win+Shift+Space by default)."
                    );
                }
                Err(e) => {
                    eprintln!("[lailaisay-app] keyboard hook disabled: {e}");
                    eprintln!(
                        "[lailaisay-app] tray stays up. Allow lailaisay in antivirus / ransomware protection, then 結束 lailaisay and relaunch."
                    );
                    set_shared_status(&self.shared, "hook failed");
                }
            }
        } else {
            eprintln!("[lailaisay-app] --no-tap: tray + Settings only, no WH_KEYBOARD_LL.");
            set_shared_status(&self.shared, "no-tap");
        }
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn poll_tray_menu(&mut self, ctx: &egui::Context) {
        let rx = MenuEvent::receiver();
        while let Ok(ev) = rx.try_recv() {
            if self.settings_id.as_ref() == Some(&ev.id) {
                self.open_settings(ctx);
            } else if self.edit_help_id.as_ref() == Some(&ev.id) {
                set_shared_status(&self.shared, crate::desktop::tray_edit_label());
                self.open_settings(ctx);
            } else if self.quit_id.as_ref() == Some(&ev.id) {
                eprintln!("[lailaisay-app] exit requested: tray Quit menu");
                self.should_quit = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn sync_tray_status(&mut self) {
        let status = self
            .shared
            .lock()
            .map(|g| g.status.clone())
            .unwrap_or_else(|_| "…".into());
        if let Some(t) = self.tray.as_mut() {
            let _ = t.set_tooltip(Some(format!("lailaisay — {status}")));
            let _ = t.set_icon(Some(status_icon(tray_glyph_from_status(&status))));
        }
        if let Some(item) = self.status_item.as_ref() {
            let _ = item.set_text(format!("lailaisay — {status}"));
        }
    }

    /// Floating lamp follows the same shared status string as the tray.
    /// Independent of Settings visibility so hold-to-talk is visible over Safari.
    fn sync_status_hud(&self, ctx: &egui::Context) {
        let status = self
            .shared
            .lock()
            .map(|g| g.status.clone())
            .unwrap_or_else(|_| "…".into());
        crate::status_hud::sync_floating_hud(ctx, &status);
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn drain_tap_messages<E>(
    mut recv: impl FnMut() -> std::result::Result<Option<lailaisay_input::TapMessage>, E>,
) -> Vec<lailaisay_input::TapMessage> {
    let mut pending = Vec::new();
    while let Ok(Some(msg)) = recv() {
        pending.push(msg);
    }
    pending
}

fn install_lailaisay_visuals(ctx: &egui::Context) {
    // Always Dark — no system auto-detect. Content pane sets SURFACE itself.
    crate::theme::apply_visuals(ctx);
}

/// Paint whenever we intend to show Settings, or the OS left a window on screen.
pub(crate) fn should_paint_settings(settings_visible: bool, os_window_visible: bool) -> bool {
    settings_visible || os_window_visible
}

pub(crate) fn should_force_hide_viewport(settings_visible: bool, should_quit: bool) -> bool {
    !settings_visible && !should_quit
}

fn os_window_looks_visible(ctx: &egui::Context) -> bool {
    ctx.input(|i| {
        let v = i.viewport();
        if v.minimized.unwrap_or(false) {
            return false;
        }
        let has_size = v
            .inner_rect
            .map(|r| r.width() > 1.0 && r.height() > 1.0)
            .unwrap_or(false);
        has_size || v.focused.unwrap_or(false)
    })
}

impl eframe::App for LailaisayHost {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        crate::theme::clear_color()
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        install_lailaisay_visuals(ctx);
        ctx.request_repaint_after(Duration::from_millis(20));

        if !self.did_init {
            self.did_init = true;
            #[cfg(target_os = "macos")]
            self.init_macos(ctx);
            #[cfg(target_os = "windows")]
            self.init_windows(ctx);
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            set_shared_status(&self.shared, "idle");
        }

        #[cfg(any(target_os = "macos", target_os = "windows"))]
        self.poll_tray_menu(ctx);

        self.poll_download();
        self.poll_session();

        #[cfg(any(target_os = "macos", target_os = "windows"))]
        self.sync_tray_status();

        if ctx.input(|i| i.viewport().close_requested()) {
            eprintln!(
                "[lailaisay-app] viewport close requested: explicit_quit={} tray_available={}",
                self.should_quit,
                self.tray_is_up()
            );
            if should_hide_to_tray(self.should_quit, self.tray_is_up()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.hide_settings_to_tray(ctx);
            }
        }

        let os_minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));
        if should_convert_minimize_to_hide(
            self.settings_visible,
            os_minimized,
            self.should_quit,
            self.tray_is_up(),
        ) {
            eprintln!("[lailaisay-app] minimize → hide-to-tray (settings_visible=false)");
            self.hide_settings_to_tray(ctx);
        }

        if self.should_quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        if should_force_hide_viewport(self.settings_visible, self.should_quit) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        }

        // After hide-to-tray so a miniaturized window is not orderBack'd.
        #[cfg(target_os = "macos")]
        crate::macos_runtime::yield_inactive_settings_window(
            &mut self.was_app_active,
            self.settings_visible,
        );

        // After hide-to-tray so HUD show cannot activate a miniaturized Settings.
        self.sync_status_hud(ctx);

        // Never leave a visible viewport unpainted (macOS used to show a black frame).
        if should_paint_settings(self.settings_visible, os_window_looks_visible(ctx)) {
            self.paint_settings(ctx);
        }
    }
}

impl LailaisayHost {
    fn paint_settings(&mut self, ctx: &egui::Context) {
        crate::theme::paint_shell_frame(ctx);
        let (stt_note, status, last_llm, last_raw, last_final) = match self.shared.lock() {
            Ok(g) => (
                g.current_stt_note(),
                g.status.clone(),
                g.last_llm_note.clone().unwrap_or_default(),
                g.last_stt.clone(),
                g.last_final.clone(),
            ),
            Err(_) => Default::default(),
        };
        let action = self.form.show_window(
            ctx,
            &SettingsView {
                live_status: &status,
                stt_note: &stt_note,
                last_llm: if last_llm.is_empty() {
                    None
                } else {
                    Some(last_llm.as_str())
                },
                last_raw: &last_raw,
                last_final: &last_final,
            },
        );
        self.handle_form_action(action);
    }
}

/// Split-borrow pattern used by the macOS tap poll (collect, then apply).
/// Kept here so Linux CI type-checks the same two-step apply the Mac build uses.
#[cfg(test)]
fn apply_collected_tap_events(
    session: &mut Session,
    pending: Vec<(
        Option<lailaisay_core::HotKeyOutput>,
        lailaisay_core::HotKeyState,
    )>,
) -> Vec<SessionAction> {
    let mut actions = Vec::new();
    for (output, state) in pending {
        let action = session.handle_output(output, state);
        actions.push(action);
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings_ui::{SETTINGS_INNER_SIZE, SETTINGS_MAX_SIZE, SETTINGS_MIN_SIZE};
    use lailaisay_core::{
        HotKey, HotKeyOutput, HotKeyState, Key, LailaisaySettings, Modifier, Modifiers,
    };

    #[test]
    fn collected_tap_events_apply_without_overlapping_borrows() {
        let mut session = Session::new(
            HotKey {
                key: Some(Key::A),
                modifiers: Modifiers::new([Modifier::Command]),
            },
            false,
        );
        session.processor.set_now(0.0);
        let pending = vec![(
            Some(HotKeyOutput::StartRecording),
            HotKeyState::PressAndHold { start_time: 0.0 },
        )];
        let actions = apply_collected_tap_events(&mut session, pending);
        assert_eq!(actions, vec![SessionAction::ArmDelayedStart]);
        assert!(session.pending_start);
        assert!(!session.recording);
        let status = session.hold_feedback_status().expect("armed hold status");
        assert_eq!(status, "recording");
        assert_eq!(
            crate::settings_ui::tray_glyph_from_status(status),
            crate::settings_ui::TrayGlyph::Active
        );
        assert!(delayed_start_still_wanted(false));
        assert!(
            !delayed_start_still_wanted(true),
            "CancelDelayedStart / StartNow must skip a queued DelayedStart"
        );
    }

    #[test]
    fn runtime_opts_follow_settings_minimize_onboarding_and_cli() {
        let mut settings = LailaisaySettings::default();
        assert!(settings.minimize_to_menu_bar_on_launch);
        assert!(!settings.has_completed_onboarding);

        let first = runtime_opts_from_settings(&settings, true, false);
        assert!(first.enable_tap);
        assert!(
            first.show_settings,
            "first-run still opens Settings so TCC / model pick is not locked out"
        );

        settings.has_completed_onboarding = true;
        let hidden = runtime_opts_from_settings(&settings, true, false);
        assert!(
            !hidden.show_settings,
            "later launches with minimize=true stay in the tray"
        );

        let forced = runtime_opts_from_settings(&settings, false, true);
        assert!(!forced.enable_tap);
        assert!(forced.show_settings, "--settings forces Settings open");

        settings.minimize_to_menu_bar_on_launch = false;
        let shown = runtime_opts_from_settings(&settings, true, false);
        assert!(shown.show_settings);

        let dir =
            std::env::temp_dir().join(format!("lailaisay-app-launch-opts-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("hex_settings.json");
        std::fs::write(
            &path,
            r#"{"minimizeToMenuBarOnLaunch":true,"hasCompletedOnboarding":false}"#,
        )
        .unwrap();
        let loaded = LailaisaySettings::for_launch(&path);
        let from_file = runtime_opts_from_settings(&loaded, true, false);
        assert!(
            !from_file.show_settings,
            "existing hex_settings.json with minimize=true must not pop Settings"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_viewport_matches_design_size() {
        assert_eq!(SETTINGS_INNER_SIZE, [760.0, 760.0]);
        assert_eq!(SETTINGS_MIN_SIZE, [640.0, 600.0]);
        assert!(SETTINGS_MAX_SIZE[0] > SETTINGS_INNER_SIZE[0]);
        assert!(SETTINGS_MAX_SIZE[1] > SETTINGS_INNER_SIZE[1]);
        let vp = crate::settings_ui::settings_viewport_builder();
        // egui 0.32: titlebar_shown=false → winit titlebar_transparent=true
        assert_eq!(vp.titlebar_shown, Some(false));
        assert_eq!(vp.titlebar_buttons_shown, Some(true));
        assert_eq!(vp.fullsize_content_view, Some(true));
        assert_eq!(vp.title_shown, Some(false));
        assert_eq!(vp.inner_size, Some(egui::vec2(760.0, 760.0)));
        assert_eq!(vp.min_inner_size, Some(egui::vec2(640.0, 600.0)));
    }

    #[test]
    fn visible_window_always_paints_and_hidden_forces_hide() {
        assert!(should_paint_settings(true, false));
        assert!(should_paint_settings(false, true));
        assert!(should_paint_settings(true, true));
        assert!(!should_paint_settings(false, false));
        assert!(should_force_hide_viewport(false, false));
        assert!(!should_force_hide_viewport(true, false));
        assert!(!should_force_hide_viewport(false, true));
        assert!(
            should_hide_to_tray(false, true),
            "Windows/macOS close hides Settings when the tray is up"
        );
        assert!(
            !should_hide_to_tray(false, false),
            "close must quit when there is no tray (Linux --settings, or tray-create failed)"
        );
    }

    #[test]
    fn minimize_is_hide_to_tray_settings_visible_false() {
        use crate::desktop::{
            may_show_settings_viewport, settings_deactivate_action, SettingsDeactivateAction,
        };

        assert!(
            should_convert_minimize_to_hide(true, true, false, true),
            "yellow button / OS miniaturize ≡ hide-to-tray"
        );
        // After conversion the host stores settings_visible = false and
        // force-hides every frame. Visible(true) is only open_settings.
        let settings_visible = false;
        assert!(should_force_hide_viewport(settings_visible, false));
        assert!(
            !may_show_settings_viewport(settings_visible),
            "Visible(true) is blocked until open_settings"
        );
        assert_eq!(
            settings_deactivate_action(false, true),
            SettingsDeactivateAction::OrderOut,
            "LINE / Safari switch must not orderBack a hidden Settings window"
        );
        assert!(
            !should_convert_minimize_to_hide(false, true, false, true),
            "already hidden: do not treat leftover miniaturize as a new show"
        );
        assert!(
            runtime_opts_from_settings(
                &{
                    let mut s = LailaisaySettings::default();
                    s.has_completed_onboarding = true;
                    s
                },
                true,
                false
            )
            .show_settings
                == false,
            "launch minimize (#37) starts with the same hidden intent"
        );
    }

    #[test]
    fn floating_hud_follows_status_when_settings_is_hidden() {
        assert!(!should_paint_settings(false, false));
        assert!(crate::status_hud::hud_visible("recording"));
        assert!(crate::status_hud::hud_visible("transcribing"));
        assert!(crate::status_hud::hud_visible("enhancing"));
        assert!(!crate::status_hud::hud_visible("idle"));
        assert!(!crate::status_hud::hud_visible("pasted"));
    }
}
