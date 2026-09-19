use std::path::{Path, PathBuf};

use crate::{Result, Transcriber, Transcript};

/// Fixture / CI backend.
///
/// Resolution order for a WAV path:
/// 1. `{stem}.transcript.txt` next to the file
/// 2. `TOK_DUMMY_TRANSCRIPT` env var
/// 3. empty string (silence / unknown)
///
/// Direct PCM calls use (2) then empty — used when recording without a sidecar.
#[derive(Debug, Default)]
pub struct DummyTranscriber {
    sidecar_root: Option<PathBuf>,
    last_wav: std::sync::Mutex<Option<PathBuf>>,
}

impl DummyTranscriber {
    pub fn new(sidecar_root: Option<&Path>) -> Self {
        Self {
            sidecar_root: sidecar_root.map(|p| p.to_path_buf()),
            last_wav: std::sync::Mutex::new(None),
        }
    }

    fn sidecar_for(&self, wav: &Path) -> Option<PathBuf> {
        let name = format!("{}.transcript.txt", wav.file_stem()?.to_string_lossy());
        let next_to = wav.with_file_name(&name);
        if next_to.exists() {
            return Some(next_to);
        }
        if let Some(root) = &self.sidecar_root {
            let candidate = root.join(&name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
        None
    }
}

impl Transcriber for DummyTranscriber {
    fn transcribe_pcm16k(
        &self,
        _samples: &[f32],
        language: Option<&str>,
        _initial_prompt: Option<&str>,
    ) -> Result<Transcript> {
        if let Ok(guard) = self.last_wav.lock() {
            if let Some(wav) = guard.as_ref() {
                if let Some(side) = self.sidecar_for(wav) {
                    let text = std::fs::read_to_string(side)?;
                    return Ok(Transcript::with_single_segment(
                        text.trim_end(),
                        language.map(|s| s.to_string()),
                    ));
                }
            }
        }
        if let Ok(text) = std::env::var("TOK_DUMMY_TRANSCRIPT") {
            return Ok(Transcript::with_single_segment(
                text,
                language.map(|s| s.to_string()),
            ));
        }
        Ok(Transcript::plain(
            String::new(),
            language.map(|s| s.to_string()),
        ))
    }

    fn transcribe_wav(
        &self,
        path: &Path,
        language: Option<&str>,
        initial_prompt: Option<&str>,
    ) -> Result<Transcript> {
        if let Ok(mut guard) = self.last_wav.lock() {
            *guard = Some(path.to_path_buf());
        }
        // Prefer sidecar even if PCM path would miss it.
        if let Some(side) = self.sidecar_for(path) {
            let text = std::fs::read_to_string(side)?;
            return Ok(Transcript::with_single_segment(
                text.trim_end(),
                language.map(|s| s.to_string()),
            ));
        }
        self.transcribe_pcm16k(&[], language, initial_prompt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sidecar() {
        let dir = std::env::temp_dir().join("lailaisay-dummy-stt");
        let _ = std::fs::create_dir_all(&dir);
        let wav = dir.join("hello.wav");
        let side = dir.join("hello.transcript.txt");
        std::fs::write(&wav, b"fake").unwrap();
        std::fs::write(&side, "你好世界\n").unwrap();
        let t = DummyTranscriber::new(None);
        let out = t.transcribe_wav(&wav, Some("zh"), None).unwrap();
        assert_eq!(out.text, "你好世界");
        assert_eq!(out.segments.len(), 1);
        assert_eq!(out.segments[0].text, "你好世界");
    }
}
