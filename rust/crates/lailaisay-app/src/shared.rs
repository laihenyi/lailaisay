//! Live settings + a **resident** STT backend (load on start / model change only).

use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use lailaisay_core::{
    clean_whisper_tokens, should_skip_llm, whisper_decode_language, CorrectionStore,
    CustomWordDictionary, LailaisaySettings, PhoneticGlossary,
};
use lailaisay_stt::{open_backend, BackendKind, Transcriber};

use crate::models::resolve_whisper_model;
use crate::pipeline::{open_stt, run_edit_pipeline, run_text_pipeline, whisper_prompt, AppContext};

pub enum Work {
    Transcribe(Vec<f32>),
    SpeakToEdit {
        pcm: Vec<f32>,
        selected: String,
    },
    /// Drop the cached decoder and load [`LailaisaySettings::selected_whisper_model`] again.
    ReloadModel,
}

pub struct SharedState {
    pub settings: LailaisaySettings,
    pub dictionary: CustomWordDictionary,
    pub glossary: PhoneticGlossary,
    pub status: String,
    stt: Option<Arc<dyn Transcriber>>,
    stt_path: Option<PathBuf>,
    pub stt_note: String,
    stt_load_count: u32,
    pub last_stt: String,
    pub last_final: String,
    pub last_llm_note: Option<String>,
    /// Captured at hotkey-down so paste can restore focus after a long LLM.
    pub paste_target: Option<lailaisay_paste::PasteTarget>,
}

impl SharedState {
    pub fn from_context(ctx: AppContext) -> Self {
        Self {
            settings: ctx.settings,
            dictionary: ctx.dictionary,
            glossary: ctx.glossary,
            status: String::from("starting"),
            stt: None,
            stt_path: None,
            stt_note: String::from("not loaded"),
            stt_load_count: 0,
            last_stt: String::new(),
            last_final: String::new(),
            last_llm_note: None,
            paste_target: None,
        }
    }

    pub fn current_stt_note(&self) -> String {
        self.stt
            .as_ref()
            .and_then(|s| s.device_note())
            .map(|note| format!("device: {note}"))
            .unwrap_or_else(|| self.stt_note.clone())
    }

    pub fn app_context(&self) -> AppContext {
        AppContext {
            settings: self.settings.clone(),
            dictionary: self.dictionary.clone(),
            glossary: self.glossary.clone(),
            paste_target: self.paste_target.clone(),
        }
    }

    pub fn set_status(&mut self, s: impl Into<String>) {
        self.status = s.into();
    }

    /// Last utterance only (session memory). `raw` is pre-LLM local text.
    pub fn set_last_utterance(
        &mut self,
        raw: impl Into<String>,
        final_text: impl Into<String>,
        note: Option<String>,
    ) {
        self.last_stt = raw.into();
        self.last_final = final_text.into();
        self.last_llm_note = note;
    }

    pub fn stt_load_count(&self) -> u32 {
        self.stt_load_count
    }

    pub fn cached_model_path(&self) -> Option<&PathBuf> {
        self.stt_path.as_ref()
    }

    /// Load whisper.cpp (or dummy) once. No-ops when the resolved path is unchanged.
    pub fn ensure_stt(&mut self) {
        let want = resolve_whisper_model(&self.settings);
        if self.stt.is_some() && self.stt_path == want {
            return;
        }
        self.load_stt(want);
    }

    pub fn force_reload_stt(&mut self) {
        self.stt = None;
        self.stt_path = None;
        self.ensure_stt();
    }

    fn load_stt(&mut self, want: Option<PathBuf>) {
        self.stt_load_count += 1;
        let (backend, note) = match want.as_deref() {
            Some(path) => match open_stt(Some(path)) {
                Ok(boxed) => {
                    tracing::info!(
                        path = %path.display(),
                        "STT resident — reused for every utterance until the selected model changes"
                    );
                    (Arc::from(boxed), format!("resident: {}", path.display()))
                }
                Err(e) => {
                    tracing::warn!(
                        path = %path.display(),
                        "could not load whisper model ({e}); dummy STT until a `whisper` build + ggml/gguf"
                    );
                    (
                        dummy_backend(),
                        format!("dummy (failed to load {}: {e})", path.display()),
                    )
                }
            },
            None => {
                tracing::info!(
                    "STT dummy (no ggml/gguf selected); stays loaded until you pick a model"
                );
                (dummy_backend(), "dummy (no model file)".into())
            }
        };
        self.stt = Some(backend);
        self.stt_path = want;
        self.stt_note = note;
    }
}

fn dummy_backend() -> Arc<dyn Transcriber> {
    Arc::from(
        open_backend(BackendKind::Dummy, None, None)
            .expect("dummy STT backend is always available"),
    )
}

pub fn set_shared_status(shared: &Arc<Mutex<SharedState>>, s: &str) {
    if let Ok(mut g) = shared.lock() {
        g.set_status(s);
    }
    eprintln!("[lailaisay-app] {s}");
}

pub fn spawn_worker(shared: Arc<Mutex<SharedState>>, rx: mpsc::Receiver<Work>) {
    thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio");
        rt.block_on(async move {
            while let Ok(work) = rx.recv() {
                let kind = match &work {
                    Work::ReloadModel => "reload model",
                    Work::SpeakToEdit { .. } => "speak to edit",
                    Work::Transcribe(_) => "transcribe",
                };
                eprintln!("[lailaisay-app] worker starting: {kind}");
                match work {
                    Work::ReloadModel => {
                        if let Ok(mut g) = shared.lock() {
                            g.force_reload_stt();
                        }
                    }
                    Work::SpeakToEdit { pcm, selected } => {
                        let (stt, ctx) = {
                            let mut g = shared.lock().unwrap_or_else(|e| e.into_inner());
                            g.ensure_stt();
                            (g.stt.clone(), g.app_context())
                        };
                        let prompt = whisper_prompt(&ctx);
                        let lang = whisper_decode_language(&ctx.settings);
                        let instruction = if let Some(stt) = stt {
                            match stt.transcribe_pcm16k(&pcm, lang.as_deref(), prompt.as_deref()) {
                                Ok(t) => t.text,
                                Err(e) => {
                                    tracing::error!("stt: {e}");
                                    set_shared_status(&shared, &format!("辨識失敗：{e}"));
                                    continue;
                                }
                            }
                        } else {
                            std::env::var("TOK_DUMMY_TRANSCRIPT").unwrap_or_default()
                        };
                        set_shared_status(&shared, "enhancing");
                        match run_edit_pipeline(&selected, &instruction, &ctx).await {
                            Ok((text, note)) => {
                                if let Ok(mut g) = shared.lock() {
                                    g.set_last_utterance(
                                        instruction.clone(),
                                        text.clone(),
                                        note.clone(),
                                    );
                                }
                                if let Some(n) = note {
                                    set_shared_status(&shared, &n);
                                } else {
                                    set_shared_status(&shared, "replaced");
                                }
                                tracing::info!("speak-to-edit result: {text}");
                            }
                            Err(e) => {
                                tracing::error!("speak-to-edit: {e}");
                                set_shared_status(&shared, &format!("error: {e}"));
                            }
                        }
                    }
                    Work::Transcribe(pcm) => {
                        let (stt, ctx) = {
                            let mut g = shared.lock().unwrap_or_else(|e| e.into_inner());
                            g.ensure_stt();
                            (g.stt.clone(), g.app_context())
                        };
                        let prompt = whisper_prompt(&ctx);
                        let lang = whisper_decode_language(&ctx.settings);
                        let stats =
                            lailaisay_stt::pcm_stats(&pcm, lailaisay_stt::WHISPER_SAMPLE_RATE);
                        let transcript = if let Some(stt) = stt {
                            match stt.transcribe_pcm16k(&pcm, lang.as_deref(), prompt.as_deref()) {
                                Ok(t) => t,
                                Err(e) => {
                                    tracing::error!("stt: {e}");
                                    set_shared_status(&shared, &format!("辨識失敗：{e}"));
                                    continue;
                                }
                            }
                        } else {
                            lailaisay_stt::Transcript::with_single_segment(
                                std::env::var("TOK_DUMMY_TRANSCRIPT").unwrap_or_default(),
                                None,
                            )
                        };
                        if transcript.segments.len() >= 2 {
                            tracing::info!(
                                n = transcript.segments.len(),
                                "pause punctuation from STT segments"
                            );
                            for (i, seg) in transcript.segments.iter().enumerate() {
                                let cleaned = clean_whisper_tokens(&seg.text);
                                let gap = transcript
                                    .segments
                                    .get(i + 1)
                                    .map(|next| (next.start - seg.end).max(0.0));
                                tracing::info!(
                                    i,
                                    start = seg.start,
                                    end = seg.end,
                                    ?gap,
                                    raw = %seg.text,
                                    cleaned = %cleaned,
                                    "STT segment"
                                );
                            }
                        }
                        if !should_skip_llm(&transcript.text, ctx.settings.ai_enhancement_mode) {
                            set_shared_status(&shared, "enhancing");
                        }
                        match run_text_pipeline(&transcript, &ctx, true).await {
                            Ok(out) => {
                                let text = out.polished.clone();
                                let note = out.note.clone();
                                if let Ok(mut g) = shared.lock() {
                                    g.set_last_utterance(
                                        out.local.clone(),
                                        out.polished.clone(),
                                        out.note.clone(),
                                    );
                                    if ctx.settings.auto_learn_from_corrections {
                                        let mut store = CorrectionStore::load_default();
                                        store.record_pair(&transcript.text, &text);
                                        if store.promote_ready(&mut g.dictionary) > 0 {
                                            let _ = g
                                                .dictionary
                                                .save_path(&lailaisay_core::custom_words_path());
                                        }
                                        let _ = store.save_default();
                                    }
                                }
                                tracing::info!("result: {text}");
                                if text.trim().is_empty() {
                                    set_shared_status(
                                        &shared,
                                        lailaisay_stt::empty_speech_status(&stats),
                                    );
                                } else if let Some(n) = &note {
                                    set_shared_status(&shared, n);
                                } else {
                                    set_shared_status(&shared, "idle");
                                }
                            }
                            Err(e) => {
                                tracing::error!("pipeline: {e}");
                                set_shared_status(&shared, &format!("error: {e}"));
                            }
                        }
                    }
                }
                eprintln!("[lailaisay-app] worker completed: {kind}");
            }
            eprintln!("[lailaisay-app] worker stopped: command channel closed");
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use lailaisay_core::LailaisaySettings;

    fn empty_ctx() -> AppContext {
        AppContext {
            settings: LailaisaySettings::default(),
            dictionary: CustomWordDictionary::default(),
            glossary: PhoneticGlossary::default(),
            paste_target: None,
        }
    }

    #[test]
    fn ensure_stt_is_cached_until_resolved_path_changes() {
        let dir = std::env::temp_dir().join(format!("lailaisay-stt-cache-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let a = dir.join("a.bin");
        let b = dir.join("b.bin");
        std::fs::write(&a, b"x").unwrap();
        std::fs::write(&b, b"x").unwrap();

        let mut st = SharedState::from_context(empty_ctx());
        st.settings.selected_whisper_model = Some(a.to_string_lossy().into());
        st.ensure_stt();
        assert_eq!(st.stt_load_count(), 1);
        assert_eq!(st.cached_model_path(), Some(&a));
        st.ensure_stt();
        assert_eq!(st.stt_load_count(), 1, "must not re-init on every call");

        st.settings.selected_whisper_model = Some(b.to_string_lossy().into());
        st.ensure_stt();
        assert_eq!(st.stt_load_count(), 2);
        assert_eq!(st.cached_model_path(), Some(&b));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn last_utterance_starts_empty_and_stores_raw_and_final() {
        let mut st = SharedState::from_context(empty_ctx());
        assert!(st.last_stt.is_empty());
        assert!(st.last_final.is_empty());
        assert!(st.last_llm_note.is_none());
        st.set_last_utterance(
            "我舉個例子，比方說，一看電影，二爬山。",
            "我舉個例子，比方說：\n一、看電影\n二、爬山。",
            Some("pasted".into()),
        );
        assert_eq!(st.last_stt, "我舉個例子，比方說，一看電影，二爬山。");
        assert_eq!(
            st.last_final,
            "我舉個例子，比方說：\n一、看電影\n二、爬山。"
        );
        assert_eq!(st.last_llm_note.as_deref(), Some("pasted"));
        assert_ne!(
            st.last_stt, st.last_final,
            "生稿 and 定稿 must stay distinct for side-by-side"
        );
    }
}
