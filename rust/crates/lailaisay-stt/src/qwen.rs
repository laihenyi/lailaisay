//! Qwen3-ASR GGUF models through transcribe.cpp (ggml, Metal on Apple Silicon).
//!
//! Compiled with the `qwen` feature on macOS. transcribe.cpp is linked as a
//! shared library so its ggml does not collide with whisper.cpp's copy.
use std::path::Path;
use std::sync::Mutex;

use lailaisay_core::TranscriptionSegment;
use transcribe_cpp::{Feature, Model, RunOptions, Session};

use crate::{Result, SttError, Transcriber, Transcript, WHISPER_SAMPLE_RATE};

fn error(e: impl std::fmt::Display) -> SttError {
    SttError::Whisper(e.to_string())
}

pub struct QwenTranscriber {
    // `Session` holds its own reference to the model; keep `Model` for feature probes.
    model: Model,
    session: Mutex<Session>,
    backend: String,
}

impl QwenTranscriber {
    pub fn load(path: &Path) -> Result<Self> {
        let started = std::time::Instant::now();
        let model = Model::load(path).map_err(error)?;
        let session = model.session().map_err(error)?;
        let backend = format!("{:?}", model.backend());
        // First run compiles the Metal pipelines; pay that now, not on the first dictation.
        let silence = vec![0.0f32; WHISPER_SAMPLE_RATE as usize];
        let mut session = session;
        let _ = session.run(&silence, &RunOptions::default());
        eprintln!(
            "[lailaisay-stt] qwen3-asr load {} backend={backend} in {:.2}s",
            path.display(),
            started.elapsed().as_secs_f64()
        );
        Ok(Self {
            model,
            session: Mutex::new(session),
            backend,
        })
    }
}

impl Transcriber for QwenTranscriber {
    fn device_note(&self) -> Option<String> {
        Some(format!("Qwen3-ASR · {}", self.backend))
    }

    fn transcribe_pcm16k(
        &self,
        samples: &[f32],
        language: Option<&str>,
        initial_prompt: Option<&str>,
    ) -> Result<Transcript> {
        crate::audio::log_pcm_stats(samples);
        let mut options = RunOptions::default();
        // "zh-TW" → "zh"; Traditional output is a post-step, as with Whisper.
        options.language = language
            .map(|l| l.split(['-', '_']).next().unwrap_or(l).to_string())
            .filter(|l| !l.is_empty());
        if let Some(prompt) = initial_prompt.filter(|p| !p.is_empty()) {
            if self.model.supports(Feature::ContextPrompt) {
                options.prompt = Some(prompt.to_string());
            }
        }
        let started = std::time::Instant::now();
        let result = self
            .session
            .lock()
            .map_err(error)?
            .run(samples, &options)
            .map_err(error)?;
        eprintln!(
            "[lailaisay-stt] qwen3-asr inference {:.3}s",
            started.elapsed().as_secs_f64()
        );
        let text = result.text.trim().to_string();
        let duration = samples.len() as f64 / WHISPER_SAMPLE_RATE as f64;
        // One segment over the whole clip so clause-boundary punctuation still runs.
        let segments = if text.is_empty() {
            Vec::new()
        } else {
            vec![TranscriptionSegment::new(text.clone(), 0.0, duration)]
        };
        Ok(Transcript {
            text,
            language: language.map(str::to_string),
            segments,
        })
    }
}
