//! Local speech-to-text.
//!
//! Rust adapters for whisper.cpp, ggml model files and fixture transcription.
//!
//! The `whisper` cargo feature compiles whisper.cpp. On **macOS** that also
//! enables the Metal backend (`whisper-rs/metal`) so `use gpu = 1`. Default
//! builds use the **sidecar / dummy** backend so `cargo test` and Linux CI
//! stay light.

use std::path::{Path, PathBuf};
use std::time::Duration;

use lailaisay_core::TranscriptionSegment;
use thiserror::Error;

pub mod audio;
pub use audio::{
    empty_speech_status, last_wav_destination_from, log_pcm_stats, maybe_save_last_wav, pcm_stats,
    PcmStats, SHORT_HOLD_SECS, SILENT_PEAK, WHISPER_SAMPLE_RATE,
};
pub mod dummy;
#[cfg(feature = "gpu-probe")]
pub mod gpu_probe;
#[cfg(feature = "process-whisper")]
pub mod process_whisper;
#[cfg(feature = "process-whisper")]
pub(crate) mod sidecar;
#[cfg(feature = "mic")]
pub mod record;
pub mod worker_protocol;

#[cfg(feature = "whisper")]
pub mod whisper;

#[cfg(all(feature = "qwen", target_os = "macos"))]
pub mod qwen;

/// Qwen3-ASR GGUF files (transcribe.cpp), as opposed to whisper.cpp ggml/gguf weights.
/// Detected by file name (`Qwen3-ASR-*.gguf`), the same names the catalog downloads.
pub fn is_qwen_asr_model(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| {
            let n = n.to_ascii_lowercase();
            n.starts_with("qwen3-asr") && n.ends_with(".gguf")
        })
        .unwrap_or(false)
}

#[derive(Debug, Error)]
pub enum SttError {
    #[error("audio error: {0}")]
    Audio(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("whisper error: {0}")]
    Whisper(String),
    #[error("no speech-to-text backend available (set --backend dummy, or build with --features whisper and pass --model)")]
    NoBackend,
    #[error("recording error: {0}")]
    Record(String),
}

pub type Result<T> = std::result::Result<T, SttError>;

#[derive(Debug, Clone)]
pub struct Transcript {
    pub text: String,
    pub language: Option<String>,
    /// Whisper (or dummy) segments with start/end seconds for pause punctuation.
    pub segments: Vec<TranscriptionSegment>,
}

impl Transcript {
    /// Raw text only — no STT segments (`post_process` skips VAD punctuation).
    pub fn plain(text: impl Into<String>, language: Option<String>) -> Self {
        Self {
            text: text.into(),
            language,
            segments: Vec::new(),
        }
    }

    /// Dummy / sidecar: one segment so clause-boundary fallback still runs.
    pub fn with_single_segment(text: impl Into<String>, language: Option<String>) -> Self {
        let text = text.into();
        let segments = if text.is_empty() {
            Vec::new()
        } else {
            vec![TranscriptionSegment::new(text.clone(), 0.0, 1.0)]
        };
        Self {
            text,
            language,
            segments,
        }
    }

    /// Non-empty STT segments for `post_process` (pause punctuation).
    pub fn post_process_segments(&self) -> Option<&[TranscriptionSegment]> {
        if self.segments.is_empty() {
            None
        } else {
            Some(self.segments.as_slice())
        }
    }
}

pub trait Transcriber: Send + Sync {
    fn device_note(&self) -> Option<String> {
        None
    }

    fn transcribe_pcm16k(
        &self,
        samples: &[f32],
        language: Option<&str>,
        initial_prompt: Option<&str>,
    ) -> Result<Transcript>;

    fn transcribe_wav(
        &self,
        path: &Path,
        language: Option<&str>,
        initial_prompt: Option<&str>,
    ) -> Result<Transcript> {
        let samples = audio::load_wav_mono16k(path)?;
        self.transcribe_pcm16k(&samples, language, initial_prompt)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Dummy,
    Whisper,
    /// Qwen3-ASR GGUF via transcribe.cpp (`qwen` feature, macOS).
    Qwen,
}

impl BackendKind {
    pub fn parse(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "dummy" | "fixture" => Some(Self::Dummy),
            "whisper" | "whisper.cpp" | "whisper-cpp" => Some(Self::Whisper),
            "qwen" | "qwen3-asr" => Some(Self::Qwen),
            _ => None,
        }
    }
}

/// Choose a backend. `dummy` always works. `whisper` requires the feature + a model file.
pub fn open_backend(
    kind: BackendKind,
    model: Option<&Path>,
    sidecar_root: Option<&Path>,
) -> Result<Box<dyn Transcriber>> {
    match kind {
        BackendKind::Dummy => Ok(Box::new(dummy::DummyTranscriber::new(sidecar_root))),
        BackendKind::Qwen => {
            let path = model.ok_or_else(|| {
                SttError::Whisper("pass --model / path to a Qwen3-ASR .gguf file".into())
            })?;
            open_qwen(path)
        }
        BackendKind::Whisper => {
            #[cfg(feature = "whisper")]
            {
                let path = model.ok_or_else(|| {
                    SttError::Whisper("pass --model / path to a ggml/gguf Whisper file".into())
                })?;
                #[cfg(all(target_os = "windows", feature = "process-whisper"))]
                {
                    Ok(Box::new(process_whisper::ProcessTranscriber::load(path)?))
                }
                #[cfg(not(all(target_os = "windows", feature = "process-whisper")))]
                {
                    Ok(Box::new(whisper::WhisperCppTranscriber::load(path)?))
                }
            }
            #[cfg(not(feature = "whisper"))]
            {
                let _ = model;
                Err(SttError::Whisper(
                    "lailaisay-stt was built without the `whisper` feature. Rebuild with `--features whisper`."
                        .into(),
                ))
            }
        }
    }
}

#[cfg(all(feature = "qwen", target_os = "macos"))]
fn open_qwen(path: &Path) -> Result<Box<dyn Transcriber>> {
    Ok(Box::new(qwen::QwenTranscriber::load(path)?))
}

#[cfg(not(all(feature = "qwen", target_os = "macos")))]
fn open_qwen(_path: &Path) -> Result<Box<dyn Transcriber>> {
    Err(SttError::Whisper(
        "Qwen3-ASR 需要 macOS 並以 `--features qwen` 編譯（此版本不含 Qwen3-ASR）".into(),
    ))
}

/// The backend a model file needs: Qwen3-ASR GGUF vs. whisper.cpp.
pub fn backend_for_model(path: &Path) -> BackendKind {
    if is_qwen_asr_model(path) {
        BackendKind::Qwen
    } else {
        BackendKind::Whisper
    }
}

/// Auto: whisper (or Qwen3-ASR, by file name) if a model path exists, otherwise dummy.
pub fn open_auto(
    model: Option<&Path>,
    sidecar_root: Option<&Path>,
) -> Result<Box<dyn Transcriber>> {
    if let Some(path) = model {
        if path.exists() {
            return open_backend(backend_for_model(path), Some(path), sidecar_root);
        }
    }
    if let Ok(env) = std::env::var("TOK_WHISPER_MODEL") {
        let path = PathBuf::from(env);
        if path.exists() {
            return open_backend(backend_for_model(&path), Some(&path), sidecar_root);
        }
    }
    open_backend(BackendKind::Dummy, None, sidecar_root)
}

/// Delay before the mic actually starts, allowing a 200 ms double-tap window.
pub fn record_start_delay() -> Duration {
    Duration::from_millis(200)
}

/// Whether the whisper.cpp context should request a GPU.
///
/// True for every macOS `whisper` build (Metal is compiled in automatically)
/// and when the explicit `metal` feature is on.
pub fn whisper_requests_gpu() -> bool {
    cfg!(all(feature = "whisper", target_os = "macos")) || cfg!(feature = "metal")
}

/// whisper.cpp segment timestamps are centiseconds (`t0`/`t1`).
pub fn whisper_centiseconds_to_secs(cs: i64) -> f64 {
    cs as f64 / 100.0
}

/// Decode knobs for short numeric utterances. Applied in `whisper` when that
/// feature is on. Exposed so tests can lock the values without compiling
/// whisper.cpp.
///
/// Short digit clips often fail `no_speech` / logprob gates, then temperature
/// fallbacks hallucinate `(CC)` / 字幕製作. Keep the first greedy pass.
pub mod whisper_decode {
    pub const NO_SPEECH_THOLD: f32 = 0.4;
    pub const LOGPROB_THOLD: f32 = -1.5;
    pub const TEMPERATURE: f32 = 0.0;
    pub const TEMPERATURE_INC: f32 = 0.0;
    pub const SUPPRESS_NON_SPEECH_TOKENS: bool = true;
    pub const SUPPRESS_BLANK: bool = true;
}

#[cfg(test)]
mod timestamp_tests {
    use super::whisper_centiseconds_to_secs;

    #[test]
    fn qwen_models_are_routed_by_file_name() {
        use std::path::Path;
        assert!(crate::is_qwen_asr_model(Path::new("/m/Qwen3-ASR-1.7B-Q4_K_M.gguf")));
        assert!(crate::is_qwen_asr_model(Path::new("qwen3-asr-0.6b-q8_0.GGUF")));
        assert!(!crate::is_qwen_asr_model(Path::new("/m/ggml-large-v3-turbo.bin")));
        assert!(!crate::is_qwen_asr_model(Path::new("/m/Qwen3-ASR-1.7B.safetensors")));
        assert_eq!(
            crate::backend_for_model(Path::new("Qwen3-ASR-0.6B-Q8_0.gguf")),
            crate::BackendKind::Qwen
        );
        assert_eq!(
            crate::backend_for_model(Path::new("ggml-tiny.bin")),
            crate::BackendKind::Whisper
        );
    }

    #[test]
    fn centiseconds_convert_like_whisper_rs_docs() {
        assert!((whisper_centiseconds_to_secs(0) - 0.0).abs() < f64::EPSILON);
        assert!((whisper_centiseconds_to_secs(150) - 1.5).abs() < f64::EPSILON);
        assert!((whisper_centiseconds_to_secs(250) - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn gpu_request_matches_macos_whisper_or_metal_feature() {
        assert_eq!(
            crate::whisper_requests_gpu(),
            cfg!(all(feature = "whisper", target_os = "macos")) || cfg!(feature = "metal")
        );
        #[cfg(not(any(feature = "whisper", feature = "metal")))]
        assert!(
            !crate::whisper_requests_gpu(),
            "Linux CI default features must not request a GPU"
        );
    }

    #[test]
    fn post_process_segments_skips_empty() {
        let empty = super::Transcript::plain("hi", None);
        assert!(empty.post_process_segments().is_none());
        let one = super::Transcript::with_single_segment("你好", Some("zh".into()));
        assert_eq!(one.post_process_segments().map(|s| s.len()), Some(1));
    }

    #[test]
    fn digit_friendly_decode_knobs_prefer_first_greedy_pass() {
        use crate::whisper_decode;
        assert!(whisper_decode::NO_SPEECH_THOLD < 0.6);
        assert!(whisper_decode::LOGPROB_THOLD < -1.0);
        assert_eq!(whisper_decode::TEMPERATURE, 0.0);
        assert_eq!(whisper_decode::TEMPERATURE_INC, 0.0);
        assert!(whisper_decode::SUPPRESS_NON_SPEECH_TOKENS);
        assert!(whisper_decode::SUPPRESS_BLANK);
    }
}
