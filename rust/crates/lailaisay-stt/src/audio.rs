use std::path::{Path, PathBuf};

use crate::{Result, SttError};

/// Whisper.cpp / lailaisay pipeline sample rate (mono f32).
pub const WHISPER_SAMPLE_RATE: u32 = 16_000;

/// Holds shorter than this often produce empty STT (delay + release).
pub const SHORT_HOLD_SECS: f32 = 0.4;

/// Peak below this is effectively silence (uninitialized / muted / wrong device).
pub const SILENT_PEAK: f32 = 0.01;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PcmStats {
    pub samples: usize,
    pub duration_secs: f32,
    pub peak: f32,
    pub rms: f32,
}

impl PcmStats {
    pub fn is_short(&self) -> bool {
        self.duration_secs < SHORT_HOLD_SECS
    }

    pub fn is_silent(&self) -> bool {
        self.peak < SILENT_PEAK
    }

    pub fn looks_unusable(&self) -> bool {
        self.is_short() || self.is_silent()
    }
}

/// Duration / peak / RMS for a mono buffer at `sample_rate`.
pub fn pcm_stats(samples: &[f32], sample_rate: u32) -> PcmStats {
    let n = samples.len();
    let duration_secs = if sample_rate == 0 {
        0.0
    } else {
        n as f32 / sample_rate as f32
    };
    let peak = samples.iter().fold(0.0f32, |acc, s| acc.max(s.abs()));
    let rms = if n == 0 {
        0.0
    } else {
        (samples.iter().map(|s| s * s).sum::<f32>() / n as f32).sqrt()
    };
    PcmStats {
        samples: n,
        duration_secs,
        peak,
        rms,
    }
}

/// Tray / pipeline status when STT produced no text.
pub fn empty_speech_status(stats: &PcmStats) -> &'static str {
    if stats.looks_unusable() {
        "hold longer"
    } else {
        "no speech"
    }
}

/// Log `[lailaisay-stt] pcm …` plus a hold-longer / check-mic hint when the buffer
/// is too short or silent. Call this immediately before Whisper.
pub fn log_pcm_stats(samples: &[f32]) -> PcmStats {
    let stats = pcm_stats(samples, WHISPER_SAMPLE_RATE);
    eprintln!(
        "[lailaisay-stt] pcm samples={} duration_secs={:.3} peak={:.4} rms={:.4}",
        stats.samples, stats.duration_secs, stats.peak, stats.rms
    );
    if stats.is_short() {
        eprintln!(
            "[lailaisay-stt] pcm too short ({:.3}s < {:.1}s) — hold the hotkey longer",
            stats.duration_secs, SHORT_HOLD_SECS
        );
    }
    if stats.is_silent() {
        eprintln!(
            "[lailaisay-stt] pcm peak near 0 ({:.4}) — hold longer / check mic",
            stats.peak
        );
    }
    stats
}

/// `TOK_SAVE_LAST_WAV=1`/`true` → default last.wav; a `*.wav` path → that file.
/// Off by default (unset / `0` / `false`).
pub fn last_wav_destination_from(env_val: Option<&str>, home: Option<&Path>) -> Option<PathBuf> {
    let v = env_val?.trim();
    if v.is_empty() || v == "0" || v.eq_ignore_ascii_case("false") {
        return None;
    }
    if v == "1" || v.eq_ignore_ascii_case("true") {
        return Some(default_last_wav_path(home));
    }
    if v.ends_with(".wav") {
        return Some(PathBuf::from(v));
    }
    None
}

fn default_last_wav_path(home: Option<&Path>) -> PathBuf {
    if cfg!(target_os = "macos") {
        if let Some(home) = home {
            return home.join("Library/Logs/lailaisay/last.wav");
        }
    }
    if let Some(home) = home {
        return home.join(".local/state/tok/last.wav");
    }
    std::env::temp_dir().join("lailaisay-last.wav")
}

/// Persist the last PCM when `TOK_SAVE_LAST_WAV` is set. No-op by default.
pub fn maybe_save_last_wav(samples: &[f32]) -> Result<Option<PathBuf>> {
    let env = std::env::var("TOK_SAVE_LAST_WAV").ok();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let Some(path) = last_wav_destination_from(env.as_deref(), home.as_deref()) else {
        return Ok(None);
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    write_wav_mono16k(&path, samples)?;
    eprintln!("[lailaisay-stt] wrote last.wav {}", path.display());
    Ok(Some(path))
}

/// Load a WAV and convert to 16 kHz mono f32 in `-1.0..=1.0` (whisper.cpp input).
pub fn load_wav_mono16k(path: &Path) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)
        .map_err(|e| SttError::Audio(format!("open {}: {e}", path.display())))?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let rate = spec.sample_rate;

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => match spec.bits_per_sample {
            16 => reader
                .samples::<i16>()
                .map(|s| s.map(|v| v as f32 / i16::MAX as f32))
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| SttError::Audio(e.to_string()))?,
            32 => reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / i32::MAX as f32))
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| SttError::Audio(e.to_string()))?,
            bits => {
                return Err(SttError::Audio(format!(
                    "unsupported integer WAV bit depth {bits}"
                )))
            }
        },
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| SttError::Audio(e.to_string()))?,
    };

    let mono = if channels == 1 {
        samples
    } else {
        samples
            .chunks(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    };

    Ok(resample_linear(&mono, rate, 16_000))
}

pub fn write_wav_mono16k(path: &Path, samples: &[f32]) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer =
        hound::WavWriter::create(path, spec).map_err(|e| SttError::Audio(e.to_string()))?;
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        writer
            .write_sample(v)
            .map_err(|e| SttError::Audio(e.to_string()))?;
    }
    writer
        .finalize()
        .map_err(|e| SttError::Audio(e.to_string()))?;
    Ok(())
}

fn resample_linear(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let out_len = ((input.len() as f64) / ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src = i as f64 * ratio;
        let i0 = src.floor() as usize;
        let i1 = (i0 + 1).min(input.len() - 1);
        let t = (src - i0 as f64) as f32;
        out.push(input[i0] * (1.0 - t) + input[i1] * t);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn roundtrip_silence() {
        let dir = std::env::temp_dir().join("lailaisay-stt-audio-test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("silence.wav");
        write_wav_mono16k(&path, &[0.0; 1600]).unwrap();
        let samples = load_wav_mono16k(&path).unwrap();
        assert!(samples.len() >= 1500);
        assert!(samples.iter().all(|s| s.abs() < 0.01));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn pcm_stats_silence_vs_tone() {
        let silent = pcm_stats(&[0.0; 1600], WHISPER_SAMPLE_RATE);
        assert_eq!(silent.samples, 1600);
        assert!((silent.duration_secs - 0.1).abs() < 1e-4);
        assert_eq!(silent.peak, 0.0);
        assert_eq!(silent.rms, 0.0);
        assert!(silent.is_short());
        assert!(silent.is_silent());
        assert_eq!(empty_speech_status(&silent), "hold longer");

        let long_silence = pcm_stats(&[0.0; 16_000], WHISPER_SAMPLE_RATE);
        assert!((long_silence.duration_secs - 1.0).abs() < 1e-4);
        assert!(long_silence.is_silent());
        assert!(!long_silence.is_short());
        assert_eq!(empty_speech_status(&long_silence), "hold longer");

        let mut tone = vec![0.0f32; 16_000];
        for (i, s) in tone.iter_mut().enumerate() {
            *s = 0.25 * ((i as f32) * 0.1).sin();
        }
        let voiced = pcm_stats(&tone, WHISPER_SAMPLE_RATE);
        assert!(voiced.peak > 0.2, "peak={}", voiced.peak);
        assert!(voiced.rms > 0.1, "rms={}", voiced.rms);
        assert!(!voiced.looks_unusable());
        assert_eq!(empty_speech_status(&voiced), "no speech");
    }

    #[test]
    fn last_wav_env_off_by_default() {
        assert!(last_wav_destination_from(None, Some(Path::new("/Users/tok"))).is_none());
        assert!(last_wav_destination_from(Some("0"), Some(Path::new("/Users/tok"))).is_none());
        assert!(last_wav_destination_from(Some("false"), Some(Path::new("/Users/tok"))).is_none());
        let on = last_wav_destination_from(Some("1"), Some(Path::new("/Users/tok"))).unwrap();
        assert!(on.ends_with("last.wav"), "{}", on.display());
        let custom =
            last_wav_destination_from(Some("/tmp/debug.wav"), Some(Path::new("/Users/tok")))
                .unwrap();
        assert_eq!(custom, PathBuf::from("/tmp/debug.wav"));
    }

    #[test]
    fn rejects_garbage() {
        let dir = std::env::temp_dir().join("lailaisay-stt-audio-test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("not-a-wav.bin");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"hello").unwrap();
        assert!(load_wav_mono16k(&path).is_err());
    }
}
