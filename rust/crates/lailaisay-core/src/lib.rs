//! Core lailaisay pipeline: settings, hotkey state machine, and post-STT text work.
//!
//! This crate is pure Rust and is the part that `cargo test` must always cover.

pub mod chinese;
pub mod corrections;
pub mod custom_words;
pub mod error;
pub mod hallucination;
pub mod hotkey;
pub mod mishearing;
pub mod output_language_command;
pub mod paths;
pub mod phonetic;
pub mod pipeline;
pub mod punctuation;
pub mod settings;
pub mod text;
pub mod tokens;

pub use chinese::{contains_chinese, is_chinese_character, to_traditional};
pub use corrections::{
    infer_substitution, learnable_substitution, CorrectionRecord, CorrectionStore,
    LEARNING_THRESHOLD,
};
pub use custom_words::{
    is_unsafe_digit_list_to_punctuation, CustomWordDictionary, CustomWordEntry,
    DictionaryMergeStats, EntrySource, EntryType,
};
pub use error::LailaisayError;
pub use hallucination::{
    format_unusable_whisper_log, is_likely_hallucination, unusable_whisper_reason,
};
pub use hotkey::{
    HotKey, HotKeyOutput, HotKeyProcessor, HotKeyState, Key, KeyEvent, Modifier, Modifiers,
};
pub use mishearing::is_likely_mishearing;
pub use output_language_command::{
    apply_spoken_translate_command, detect_output_language_command, CommandPosition, LanguageCue,
    SpokenTranslateCommand, LANGUAGE_CUES,
};
pub use paths::{
    copy_ggml_if_dest_empty, correction_history_path, custom_words_path,
    migrate_legacy_macos_models, models_dir, phonetic_glossary_path, remapped_legacy_whisper_model,
    remapped_macwhisper_weak_model, remapped_selected_whisper_model, settings_path,
    windows_app_data_dir_from, LEGACY_MACOS_SUPPORT_NAME, MACOS_BUNDLE_ID, WINDOWS_APP_DIR,
};
pub use phonetic::PhoneticGlossary;
pub use pipeline::{
    apply_post_llm_terms, assemble_dictate_user_message, build_ask_prompt, build_edit_prompt,
    build_enhancement_prompt, build_enhancement_prompt_ex, effective_output_language,
    llm_rejected_as_translation, needs_mixed_language_preserve, polish_few_shot_enabled,
    post_process, prepare_dictate_text, should_skip_llm, should_translate_output,
    structure_spoken_lists, whisper_decode_language, whisper_initial_prompt,
    EnhancementPromptOptions, PostProcessOptions, LANGUAGE_LOCK_SUFFIX, LLM_MIN_CHARS,
    MIXED_LANGUAGE_PRESERVE, POLISH_FEWSHOT_ZH, RAW_TRANSCRIPT_CLOSE, RAW_TRANSCRIPT_OPEN,
    RAW_TRANSCRIPT_PREAMBLE, WHISPER_EN_DIGIT_PROMPT, WHISPER_ZH_DIGIT_PROMPT,
};
pub use punctuation::{punctuated_text, sanitize_transcription_segments, TranscriptionSegment};
pub use settings::{
    is_stock_enhancement_prompt, AiEnhancementMode, AiPolishStyle, AiProviderType,
    LailaisaySettings, ModelWarmStatus, OutputStyle, DEFAULT_ENHANCEMENT_PROMPT,
    LEGACY_DEFAULT_ENHANCEMENT_PROMPT,
};
pub use text::{detect_language, TextProcessingOptions, TextProcessor};
pub use tokens::{
    arabicize_spoken_number_text, clean_whisper_tokens, is_spoken_number_text,
    is_timestamp_junk_segment,
};
