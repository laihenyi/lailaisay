//! Experimental Qwen3-ASR backend: a resident Python (MLX) worker behind the
//! same line-delimited `worker_protocol` as the Windows whisper workers.
//!
//! Enabled by `TOK_STT_BACKEND=qwen3-mlx`; see `scripts/qwen3_asr_worker.py`.
use crate::{
    sidecar::{error, Session},
    worker_protocol::Request,
    Result, Transcriber, Transcript,
};
use std::{path::PathBuf, process::Command, sync::Mutex, time::Duration};

pub const BACKEND_ENV: &str = "TOK_STT_BACKEND";
pub const DEFAULT_MODEL: &str = "mlx-community/Qwen3-ASR-0.6B-8bit";

/// True when the environment asks for the Qwen3-ASR sidecar.
pub fn requested() -> bool {
    std::env::var(BACKEND_ENV)
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "qwen3-mlx" | "qwen3-asr" | "qwen"))
        .unwrap_or(false)
}

pub struct QwenSidecarTranscriber {
    session: Mutex<Session>,
}

impl QwenSidecarTranscriber {
    /// Spawn the worker and wait until the model is loaded and warmed up.
    pub fn load() -> Result<Self> {
        let python = std::env::var("TOK_QWEN_PYTHON").unwrap_or_else(|_| "python3".into());
        let worker = std::env::var("TOK_QWEN_WORKER")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/qwen3_asr_worker.py")
            });
        if !worker.is_file() {
            return Err(error(format!("qwen worker missing: {}", worker.display())));
        }
        let model = std::env::var("TOK_QWEN_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.into());
        eprintln!("[lailaisay-stt] qwen3-asr sidecar: {python} {} ({model})", worker.display());
        let session = Session::start(Command::new(python).arg(&worker).arg(&model))?;
        Ok(Self {
            session: Mutex::new(session),
        })
    }
}

impl Transcriber for QwenSidecarTranscriber {
    fn device_note(&self) -> Option<String> {
        self.session.lock().ok().map(|s| format!("Qwen3-ASR · {}", s.device))
    }

    fn transcribe_pcm16k(
        &self,
        samples: &[f32],
        language: Option<&str>,
        initial_prompt: Option<&str>,
    ) -> Result<Transcript> {
        crate::audio::log_pcm_stats(samples);
        let request = serde_json::to_string(&Request {
            samples: samples.to_vec(),
            language: language.map(str::to_owned),
            prompt: initial_prompt.map(str::to_owned),
        })
        .map_err(error)?;
        let mut session = self.session.lock().map_err(error)?;
        session.transcribe(&request, Duration::from_secs(60))
    }
}
