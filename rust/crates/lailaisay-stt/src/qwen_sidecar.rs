//! Qwen3-ASR backend: a resident Python (MLX) worker behind the same
//! line-delimited `worker_protocol` as the Windows whisper workers.
//!
//! Selected in settings as `selectedWhisperModel = "qwen3-asr:<HF repo>"`
//! (developer override: `TOK_STT_BACKEND=qwen3-mlx`). The Python environment
//! lives in `<app support>/qwen-venv`; create it with `scripts/setup-qwen3-asr.sh`.
use crate::{
    sidecar::{error, Session},
    worker_protocol::Request,
    Result, Transcriber, Transcript,
};
use std::{path::PathBuf, process::Command, sync::Mutex, time::Duration};

pub const BACKEND_ENV: &str = "TOK_STT_BACKEND";
pub const SELECTION_PREFIX: &str = "qwen3-asr:";
pub const DEFAULT_MODEL: &str = "mlx-community/Qwen3-ASR-0.6B-8bit";

/// MLX conversions offered in settings: (Hugging Face repo, label).
pub const CHOICES: &[(&str, &str)] = &[
    (DEFAULT_MODEL, "Qwen3-ASR 0.6B 8bit · 最快、已實測"),
    ("mlx-community/Qwen3-ASR-0.6B-4bit", "Qwen3-ASR 0.6B 4bit · 更小"),
    ("mlx-community/Qwen3-ASR-1.7B-8bit", "Qwen3-ASR 1.7B 8bit · 較準、較慢"),
    ("mlx-community/Qwen3-ASR-1.7B-4bit", "Qwen3-ASR 1.7B 4bit · 較準、較小"),
];

/// Value stored in `selectedWhisperModel` for a repo.
pub fn selection_value(repo: &str) -> String {
    format!("{SELECTION_PREFIX}{repo}")
}

/// Repo id when `selected` is a Qwen3-ASR selection.
pub fn selection_repo(selected: &str) -> Option<&str> {
    selected
        .trim()
        .strip_prefix(SELECTION_PREFIX)
        .map(str::trim)
        .filter(|r| !r.is_empty())
}

/// Short label for a repo (known choice, else the repo name).
pub fn choice_label(repo: &str) -> String {
    CHOICES
        .iter()
        .find(|(id, _)| *id == repo)
        .map(|(_, label)| (*label).to_string())
        .unwrap_or_else(|| repo.rsplit('/').next().unwrap_or(repo).to_string())
}

/// Developer override: `TOK_STT_BACKEND=qwen3-mlx` forces the default model.
pub fn env_override_repo() -> Option<String> {
    std::env::var(BACKEND_ENV)
        .ok()
        .filter(|v| matches!(v.to_ascii_lowercase().as_str(), "qwen3-mlx" | "qwen3-asr" | "qwen"))
        .map(|_| DEFAULT_MODEL.to_string())
}

/// Where `scripts/setup-qwen3-asr.sh` creates the virtualenv.
pub fn default_python() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        Some(lailaisay_core::paths::macos_app_support_dir().join("qwen-venv/bin/python"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

fn worker_script() -> PathBuf {
    if let Ok(p) = std::env::var("TOK_QWEN_WORKER") {
        return PathBuf::from(p);
    }
    // Packaged app: <bundle>/Contents/MacOS/exe → ../Resources/.
    if let Some(p) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|d| d.join("../Resources/qwen3_asr_worker.py")))
        .filter(|p| p.is_file())
    {
        return p;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/qwen3_asr_worker.py")
}

pub struct QwenSidecarTranscriber {
    session: Mutex<Session>,
}

impl QwenSidecarTranscriber {
    /// Spawn the worker and wait until the model is loaded and warmed up.
    pub fn load(repo: &str) -> Result<Self> {
        let python = std::env::var("TOK_QWEN_PYTHON")
            .map(PathBuf::from)
            .ok()
            .or_else(default_python)
            .ok_or_else(|| error("Qwen3-ASR 只支援 macOS"))?;
        if !python.is_file() {
            return Err(error(format!(
                "尚未安裝 Qwen 環境（找不到 {}）。請執行 scripts/setup-qwen3-asr.sh",
                python.display()
            )));
        }
        let worker = worker_script();
        if !worker.is_file() {
            return Err(error(format!("找不到 Qwen worker：{}", worker.display())));
        }
        eprintln!(
            "[lailaisay-stt] qwen3-asr sidecar: {} {} ({repo})",
            python.display(),
            worker.display()
        );
        let session = Session::start(Command::new(python).arg(&worker).arg(repo))?;
        Ok(Self {
            session: Mutex::new(session),
        })
    }
}

impl Transcriber for QwenSidecarTranscriber {
    fn device_note(&self) -> Option<String> {
        self.session
            .lock()
            .ok()
            .map(|s| format!("Qwen3-ASR · {}", s.device))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_round_trips() {
        let v = selection_value(DEFAULT_MODEL);
        assert_eq!(selection_repo(&v), Some(DEFAULT_MODEL));
        assert_eq!(selection_repo("/tmp/ggml-tiny.bin"), None);
        assert_eq!(selection_repo("qwen3-asr:"), None);
    }

    #[test]
    fn every_choice_has_a_label() {
        for (id, label) in CHOICES {
            assert!(id.starts_with("mlx-community/"));
            assert_eq!(&choice_label(id), label);
        }
        assert_eq!(choice_label("someone/Other-Model"), "Other-Model");
    }
}
