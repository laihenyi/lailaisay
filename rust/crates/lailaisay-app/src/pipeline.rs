//! Same post-STT path as `lailaisay-cli`: local filters → optional LLM → paste/copy.

use std::path::{Path, PathBuf};

use anyhow::Result;
use lailaisay_core::{
    apply_post_llm_terms, custom_words_path, phonetic_glossary_path, post_process, settings_path,
    AiEnhancementMode, CustomWordDictionary, LailaisaySettings, OutputStyle, PhoneticGlossary,
    PostProcessOptions,
};
use lailaisay_enhance::{enhance_edit, enhance_with_note_vocab};
use lailaisay_paste::PasteTarget;
use lailaisay_stt::{open_auto, open_backend, BackendKind, Transcriber, Transcript};

pub struct AppContext {
    pub settings: LailaisaySettings,
    pub dictionary: CustomWordDictionary,
    pub glossary: PhoneticGlossary,
    /// Frontmost app at hotkey-down (restored before paste after a long LLM).
    pub paste_target: Option<PasteTarget>,
}

/// Load settings + bundled dictionaries. A missing file uses defaults.
/// A corrupt settings file is warned and replaced with defaults so `--once`
/// still works on a first-time machine.
pub fn load_app_context() -> Result<AppContext> {
    let migrated = lailaisay_core::migrate_legacy_macos_models();
    if migrated > 0 {
        eprintln!(
            "[lailaisay-app] copied {migrated} ggml/gguf file(s) from ~/Library/Application Support/{} to {}/",
            lailaisay_core::LEGACY_MACOS_SUPPORT_NAME,
            lailaisay_core::MACOS_BUNDLE_ID
        );
    }
    let path = settings_path();
    if path.exists() {
        if let Err(e) = LailaisaySettings::load_path(&path) {
            tracing::warn!(
                path = %path.display(),
                "settings unreadable ({e}); using defaults"
            );
        }
    }
    // Existing file → onboarding complete so minimize-to-tray is honored.
    // Missing / unreadable file stays first-run (Settings opens once).
    let mut settings = LailaisaySettings::for_launch(&path);
    if let Some((old, next)) = settings.apply_whisper_model_remap() {
        let reason = if old.contains(lailaisay_core::LEGACY_MACOS_SUPPORT_NAME) {
            lailaisay_core::LEGACY_MACOS_SUPPORT_NAME
        } else {
            "MacWhisper small/tiny"
        };
        eprintln!(
            "[lailaisay-app] selectedWhisperModel still pointed at {reason} ({old}) — using {next}"
        );
        if path.exists() {
            if let Err(e) = settings.save_path(&path) {
                tracing::warn!(
                    path = %path.display(),
                    "could not persist remapped selectedWhisperModel ({e})"
                );
            }
        }
    }
    let dictionary = load_dictionary()?;
    let glossary = load_glossary()?;
    Ok(AppContext {
        settings,
        dictionary,
        glossary,
        paste_target: None,
    })
}

/// `--once` first smoke: never call Ollama/Groq unless the user passed `--enhance`.
pub fn disable_llm_for_once(ctx: &mut AppContext) {
    ctx.settings.ai_enhancement_mode = AiEnhancementMode::Off;
}

/// Resolve the checked-in fixture from the crate, `rust/`, or any parent cwd.
pub fn find_sample_wav() -> Option<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let from_crate = manifest.join("../../fixtures/sample.wav");
    if from_crate.exists() {
        return Some(from_crate);
    }
    let mut dir = std::env::current_dir().ok()?;
    for _ in 0..10 {
        for rel in ["fixtures/sample.wav", "rust/fixtures/sample.wav"] {
            let p = dir.join(rel);
            if p.exists() {
                return Some(p);
            }
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

pub fn open_backend_kind(
    kind: BackendKind,
    model: Option<&Path>,
    sidecar: Option<&Path>,
) -> Result<Box<dyn Transcriber>> {
    Ok(open_backend(kind, model, sidecar)?)
}

fn load_dictionary() -> Result<CustomWordDictionary> {
    let path = custom_words_path();
    if path.exists() {
        return Ok(CustomWordDictionary::load_path(&path)?);
    }
    if let Some(bundled) = bundled_data("DefaultCustomWords.json") {
        return Ok(CustomWordDictionary::load_path(&bundled)?);
    }
    Ok(CustomWordDictionary::default())
}

fn load_glossary() -> Result<PhoneticGlossary> {
    let path = phonetic_glossary_path();
    if path.exists() {
        return Ok(PhoneticGlossary::load_path(&path)?);
    }
    if let Some(bundled) = bundled_data("DefaultPhoneticGlossary.json") {
        return Ok(PhoneticGlossary::load_path(&bundled)?);
    }
    Ok(PhoneticGlossary::default())
}

fn bundled_data(name: &str) -> Option<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let from_crate = manifest.join("../../data").join(name);
    if from_crate.exists() {
        return Some(from_crate);
    }
    let mut dir = std::env::current_dir().ok()?;
    for _ in 0..8 {
        let p = dir.join("rust/data").join(name);
        if p.exists() {
            return Some(p);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

pub fn whisper_prompt(ctx: &AppContext) -> Option<String> {
    lailaisay_core::whisper_initial_prompt(&ctx.settings, Some(&ctx.dictionary.prompt_text()))
}

pub fn open_stt(model: Option<&std::path::Path>) -> Result<Box<dyn Transcriber>> {
    Ok(open_auto(model, None)?)
}

/// Dictate pipeline result. `local` is the pre-LLM body shown as 辨識生稿.
#[derive(Debug, Clone)]
pub struct TextPipelineOutcome {
    /// STT + local filters (fillers, glossary, pause punctuation). Never pasted.
    pub local: String,
    /// Polished (or local fallback) body that is pasted / copied.
    pub polished: String,
    pub note: Option<String>,
}

pub async fn run_text_pipeline(
    transcript: &Transcript,
    ctx: &AppContext,
    paste: bool,
) -> Result<TextPipelineOutcome> {
    let local = post_process(
        &transcript.text,
        PostProcessOptions {
            settings: &ctx.settings,
            dictionary: &ctx.dictionary,
            glossary: &ctx.glossary,
            segments: transcript.post_process_segments(),
        },
    );
    tracing::info!(?local, "local pipeline");

    let vocab = ctx.dictionary.polish_vocabulary_block();
    let outcome = enhance_with_note_vocab(
        &local,
        &ctx.settings,
        OutputStyle::General,
        vocab.as_deref(),
    )
    .await
    .unwrap_or_else(|e| {
        tracing::warn!("enhancement skipped: {e}");
        lailaisay_enhance::EnhanceOutcome {
            text: local.clone(),
            note: Some(e.to_string()),
        }
    });
    if let Some(note) = &outcome.note {
        tracing::warn!("{note}");
    }

    // Post-LLM local term correction (dictionary + glossary), then numeral norms.
    // Proper nouns the model rewrote are restored; 國字 numerals become Arabic.
    let polished = apply_post_llm_terms(&outcome.text, &ctx.dictionary, &ctx.glossary);
    let mut note = outcome.note;
    if polished.trim().is_empty() {
        let lang = lailaisay_core::whisper_decode_language(&ctx.settings);
        let prompt = whisper_prompt(ctx);
        if let Some(line) = lailaisay_core::format_unusable_whisper_log(
            &transcript.text,
            lang.as_deref(),
            prompt.as_deref().map(|s| s.chars().count()).unwrap_or(0),
        ) {
            eprintln!("{line}");
            tracing::warn!("{line}");
        }
        if paste {
            eprintln!("[lailaisay-app] empty transcript — skip paste/clipboard");
        }
        if paste && note.is_none() {
            note = Some("no speech".into());
        }
    }
    if paste && !polished.trim().is_empty() {
        match lailaisay_paste::paste_text_to(&polished, &ctx.settings, ctx.paste_target.as_ref()) {
            Ok(()) => {
                tracing::info!(
                    target = %ctx
                        .paste_target
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "frontmost".into()),
                    "pasted"
                );
                if note.is_none() {
                    note = Some("pasted".into());
                }
            }
            Err(e) => {
                tracing::warn!("paste failed (clipboard still holds text): {e}");
                eprintln!(
                    "[lailaisay-app] PASTE FAILED — transcription is on the clipboard. Cmd+V in the target app."
                );
                let fail = format!("paste failed — clipboard has text ({e})");
                note = Some(match note {
                    Some(n) => format!("{n}; {fail}"),
                    None => fail,
                });
            }
        }
    }
    Ok(TextPipelineOutcome {
        local,
        polished,
        note,
    })
}

/// Speak-to-Edit: STT instruction + selected text → LLM rewrite → replace selection.
pub async fn run_edit_pipeline(
    selected: &str,
    instruction: &str,
    ctx: &AppContext,
) -> Result<(String, Option<String>)> {
    let instruction = post_process(
        instruction,
        PostProcessOptions {
            settings: &ctx.settings,
            dictionary: &ctx.dictionary,
            glossary: &ctx.glossary,
            segments: None,
        },
    );
    if selected.trim().is_empty() {
        return Ok((String::new(), Some("Speak to Edit: no selection".into())));
    }
    if instruction.trim().is_empty() {
        return Ok((String::new(), Some("no speech".into())));
    }
    let outcome = enhance_edit(selected, &instruction, &ctx.settings)
        .await
        .unwrap_or_else(|e| lailaisay_enhance::EnhanceOutcome {
            text: selected.to_string(),
            note: Some(e.to_string()),
        });
    match lailaisay_paste::replace_selected_text_to(
        &outcome.text,
        &ctx.settings,
        ctx.paste_target.as_ref(),
    ) {
        Ok(()) => tracing::info!("replaced selection"),
        Err(e) => tracing::warn!("replace selection: {e}"),
    }
    Ok((outcome.text, outcome.note))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lailaisay_core::TranscriptionSegment;

    #[test]
    fn finds_checked_in_sample_wav() {
        let wav = find_sample_wav().expect("fixtures/sample.wav in the repo");
        assert!(wav.ends_with("sample.wav"), "{}", wav.display());
        let side = wav.with_file_name("sample.transcript.txt");
        assert!(side.exists(), "dummy sidecar missing: {}", side.display());
    }

    fn test_ctx() -> AppContext {
        let mut settings = LailaisaySettings::default();
        settings.ai_enhancement_mode = AiEnhancementMode::Off;
        AppContext {
            settings,
            dictionary: CustomWordDictionary::default(),
            glossary: PhoneticGlossary::default(),
            paste_target: None,
        }
    }

    #[tokio::test]
    async fn once_pipeline_filters_sample() {
        let ctx = test_ctx();
        let out = run_text_pipeline(
            &Transcript::plain("嗯，那個，我們去台北 不是 去台南。Thank you.", None),
            &ctx,
            false,
        )
        .await
        .unwrap()
        .polished;
        assert!(!out.contains("嗯"), "{out}");
        assert!(!out.to_lowercase().contains("thank you"), "{out}");
    }

    #[tokio::test]
    async fn multi_segment_inserts_pause_punctuation() {
        let ctx = test_ctx();
        let transcript = Transcript {
            text: "今天天氣很好我們出去玩吧".into(),
            language: Some("zh".into()),
            segments: vec![
                TranscriptionSegment::new("今天天氣很好", 0.0, 1.5),
                TranscriptionSegment::new("我們出去玩吧", 2.5, 4.0),
            ],
        };
        let out = run_text_pipeline(&transcript, &ctx, false)
            .await
            .unwrap()
            .polished;
        assert!(
            out.contains('。') || out.contains('，'),
            "expected pause punctuation from 2 segments: {out}"
        );
        assert!(out.contains("今天天氣很好"), "{out}");
    }

    #[tokio::test]
    async fn timestamp_junk_in_segments_does_not_survive() {
        let ctx = test_ctx();
        let transcript = Transcript {
            text: "今天天氣很好<|1.00|><|0.00|>很適合出去走一走".into(),
            language: Some("zh".into()),
            segments: vec![
                TranscriptionSegment::new("今天天氣很好<|1.00|>", 0.0, 1.5),
                TranscriptionSegment::new("<|0.00|>", 1.55, 1.6),
                TranscriptionSegment::new("很適合出去走一走", 2.5, 4.0),
            ],
        };
        let out = run_text_pipeline(&transcript, &ctx, false)
            .await
            .unwrap()
            .polished;
        assert!(!out.contains('１'), "{out}");
        assert!(
            out.contains('，') || out.contains('。'),
            "expected pause punctuation: {out}"
        );
    }

    #[tokio::test]
    async fn numeric_variants_survive_complete_local_pipeline() {
        let ctx = test_ctx();
        for (number, expected_digits) in [
            ("１", "1"),
            ("1.28", "128"),
            ("0.00", "000"),
            ("123", "123"),
            ("１、２、３、４", "1234"),
        ] {
            let transcript = Transcript {
                text: number.into(),
                language: Some("zh".into()),
                segments: vec![TranscriptionSegment::new(number, 0.0, 1.0)],
            };
            let out = run_text_pipeline(&transcript, &ctx, false).await.unwrap();
            assert!(!out.polished.is_empty(), "number was removed: {number}");
            let actual: String = out
                .polished
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect();
            assert_eq!(actual, expected_digits, "{number}: {}", out.polished);
        }
    }

    #[tokio::test]
    async fn spoken_digit_segments_reach_clipboard() {
        let ctx = test_ctx();
        let transcript = Transcript {
            text: "1 2 3 4 5 6".into(),
            language: Some("zh".into()),
            segments: vec![
                TranscriptionSegment::new("1", 0.0, 0.15),
                TranscriptionSegment::new("2", 0.2, 0.35),
                TranscriptionSegment::new("3", 0.4, 0.55),
                TranscriptionSegment::new("4", 0.6, 0.75),
                TranscriptionSegment::new("5", 0.8, 0.95),
                TranscriptionSegment::new("6", 1.0, 1.15),
            ],
        };
        let out = run_text_pipeline(&transcript, &ctx, false)
            .await
            .unwrap()
            .polished;
        assert!(
            !out.is_empty() && out.contains('1') && out.contains('6'),
            "spoken digit list must not be dropped as timestamp junk: {out:?}"
        );
    }

    #[tokio::test]
    async fn standalone_number_sequences_reach_local_draft() {
        let ctx = test_ctx();
        for (raw, expected) in [
            ("1,2,3,4,5,6,7", "1,2,3,4,5,6,7"),
            ("一二三四五六七", "1234567"),
            ("一、二、三", "1、2、3"),
            ("一、三、五、七、九", "1、3、5、7、9"),
            ("one, two, three", "one, two, three"),
        ] {
            let out = run_text_pipeline(&Transcript::plain(raw, Some("zh".into())), &ctx, false)
                .await
                .unwrap();
            assert_eq!(
                out.local, expected,
                "local 辨識生稿 for {raw}: {:?}",
                out.local
            );
            assert_eq!(
                out.polished, expected,
                "paste body for {raw}: {:?}",
                out.polished
            );
            assert!(!out.polished.is_empty(), "{raw}");
        }
        let mixed = run_text_pipeline(
            &Transcript::plain("今天開會討論1,2,3,4,5,6,7", Some("zh".into())),
            &ctx,
            false,
        )
        .await
        .unwrap();
        assert!(
            mixed.local.contains("今天開會") && mixed.local.contains("7"),
            "{}",
            mixed.local
        );
        let prose = run_text_pipeline(
            &Transcript::plain("今天天氣一五八", Some("zh".into())),
            &ctx,
            false,
        )
        .await
        .unwrap();
        assert!(prose.local.contains("今天天氣一五八"), "{}", prose.local);
        for idiom in ["一模一樣", "三心二意", "萬一"] {
            let out = run_text_pipeline(&Transcript::plain(idiom, Some("zh".into())), &ctx, false)
                .await
                .unwrap();
            assert!(
                out.local.contains(idiom),
                "idiom {idiom} was rewritten: {}",
                out.local
            );
        }
        let spoken = run_text_pipeline(
            &Transcript::plain("預算大概三千六塊", Some("zh".into())),
            &ctx,
            false,
        )
        .await
        .unwrap();
        assert_eq!(spoken.local, "預算大概3600塊");
        assert_eq!(spoken.polished, "預算大概3600塊");
        let range = run_text_pipeline(
            &Transcript::plain("兩到三次", Some("zh".into())),
            &ctx,
            false,
        )
        .await
        .unwrap();
        assert_eq!(range.local, "2到3次");
        let cc = run_text_pipeline(&Transcript::plain("(CC)", Some("zh".into())), &ctx, false)
            .await
            .unwrap();
        assert!(cc.local.is_empty(), "{}", cc.local);
    }

    #[tokio::test]
    async fn dummy_sidecar_single_segment_still_filters_sample() {
        let wav = find_sample_wav().expect("fixtures/sample.wav in the repo");
        let stt = open_backend(BackendKind::Dummy, None, wav.parent()).expect("dummy backend");
        let transcript = stt.transcribe_wav(&wav, Some("zh"), None).expect("sidecar");
        assert_eq!(transcript.segments.len(), 1);
        let ctx = test_ctx();
        let out = run_text_pipeline(&transcript, &ctx, false)
            .await
            .unwrap()
            .polished;
        assert!(
            out == "去台南。" || out == "去臺南。",
            "dummy sidecar + single segment must match --once: {out:?}"
        );
    }

    #[tokio::test]
    async fn speak_to_edit_empty_selection_is_noop() {
        let ctx = test_ctx();
        let (text, note) = run_edit_pipeline("", "make shorter", &ctx).await.unwrap();
        assert!(text.is_empty(), "{text}");
        assert!(note.unwrap().contains("no selection"));
    }

    #[tokio::test]
    async fn empty_transcript_does_not_paste() {
        let ctx = test_ctx();
        let out = run_text_pipeline(&Transcript::plain("", None), &ctx, true)
            .await
            .unwrap();
        let text = out.polished;
        let note = out.note;
        assert!(text.is_empty(), "{text:?}");
        let note = note.expect("empty STT should set a status, not a successful paste");
        assert!(
            note.contains("no speech"),
            "must not report pasted/clipboard for 0 chars: {note}"
        );
        assert!(!note.to_ascii_lowercase().contains("pasted"), "{note}");
        assert!(!note.to_ascii_lowercase().contains("clipboard"), "{note}");
    }

    #[test]
    fn default_settings_pin_whisper_language_and_dummy_records_it() {
        let ctx = test_ctx();
        let lang = lailaisay_core::whisper_decode_language(&ctx.settings);
        assert_eq!(lang.as_deref(), Some("zh"));
        let prompt = whisper_prompt(&ctx).expect("default zh sends a digit-biasing prompt");
        assert!(
            prompt.contains("一二三") && prompt.contains('1'),
            "{prompt}"
        );
        let arabic_at = prompt.find("1 2 3").expect("arabic in whisper prompt");
        let chinese_at = prompt.find("一二三").expect("chinese in whisper prompt");
        assert!(arabic_at < chinese_at, "{prompt}");
        let stt = open_backend(BackendKind::Dummy, None, None).expect("dummy");
        let t = stt
            .transcribe_pcm16k(&[0.0; 800], lang.as_deref(), Some(prompt.as_str()))
            .unwrap();
        assert_eq!(t.language.as_deref(), Some("zh"));
    }

    #[tokio::test]
    async fn off_pipeline_strips_spoken_translate_command() {
        let ctx = test_ctx();
        let out = run_text_pipeline(
            &Transcript::plain("今天天氣很好，用英文", None),
            &ctx,
            false,
        )
        .await
        .unwrap()
        .polished;
        assert!(!out.contains("用英文"), "{out}");
        assert!(out.contains("天氣很好"), "{out}");
    }

    #[tokio::test]
    async fn off_pipeline_leaves_negated_translate_command() {
        let ctx = test_ctx();
        let out = run_text_pipeline(
            &Transcript::plain("這段話不要翻譯成英文", None),
            &ctx,
            false,
        )
        .await
        .unwrap()
        .polished;
        assert!(out.contains("不要翻譯成英文"), "{out}");
    }

    #[tokio::test]
    async fn speak_to_edit_empty_instruction_skips_replace() {
        let ctx = test_ctx();
        let (text, note) = run_edit_pipeline("selected text", "", &ctx).await.unwrap();
        assert!(text.is_empty(), "{text}");
        assert_eq!(note.as_deref(), Some("no speech"));
    }

    #[tokio::test]
    async fn pipeline_exposes_local_pre_llm_and_final() {
        let ctx = test_ctx();
        let out = run_text_pipeline(
            &Transcript::plain("嗯，那個，我們去台北 不是 去台南。Thank you.", None),
            &ctx,
            false,
        )
        .await
        .unwrap();
        assert!(
            !out.local.contains("嗯") && !out.local.to_lowercase().contains("thank you"),
            "local must be STT + filters, not raw Whisper: {}",
            out.local
        );
        assert_eq!(
            out.polished, out.local,
            "LLM Off: 定稿 equals 生稿 (local fallback)"
        );
        assert!(
            out.local.contains("台南") || out.local.contains("臺南"),
            "{}",
            out.local
        );
    }
}
