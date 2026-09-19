#![cfg(feature = "whisper")]

use std::path::Path;
use std::sync::Mutex;

use lailaisay_core::TranscriptionSegment;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::{whisper_centiseconds_to_secs, Result, SttError, Transcriber, Transcript};

/// whisper.cpp decoder. Load once, transcribe many files.
pub struct WhisperCppTranscriber {
    ctx: Mutex<WhisperContext>,
}

impl WhisperCppTranscriber {
    pub fn load(model: &Path) -> Result<Self> {
        Self::load_with_parameters(model, context_parameters())
    }

    pub fn load_with_parameters(
        model: &Path,
        params: WhisperContextParameters<'static>,
    ) -> Result<Self> {
        eprintln!(
            "[lailaisay-stt] whisper.cpp load {} use_gpu={} metal_feature={} (expect use gpu = 1 on Mac)",
            model.display(),
            params.use_gpu,
            cfg!(feature = "metal") || cfg!(target_os = "macos")
        );
        let ctx = WhisperContext::new_with_params(
            model.to_str().ok_or_else(|| {
                SttError::Whisper(format!("non-UTF8 model path: {}", model.display()))
            })?,
            params,
        )
        .map_err(|e| SttError::Whisper(e.to_string()))?;
        Ok(Self {
            ctx: Mutex::new(ctx),
        })
    }
}

/// whisper-rs 0.13.2 exposes `use_gpu` as a public field (no builder required).
/// Default is `cfg!(feature = "_gpu")`; Metal enables `_gpu`. We still set it
/// when this crate asked for GPU so a Mac `whisper` build cannot stay at 0.
pub fn context_parameters() -> WhisperContextParameters<'static> {
    let mut params = WhisperContextParameters::default();
    if crate::whisper_requests_gpu() {
        params.use_gpu = true;
    }
    params
}

impl Transcriber for WhisperCppTranscriber {
    fn transcribe_pcm16k(
        &self,
        samples: &[f32],
        language: Option<&str>,
        initial_prompt: Option<&str>,
    ) -> Result<Transcript> {
        crate::audio::log_pcm_stats(samples);
        if let Err(e) = crate::audio::maybe_save_last_wav(samples) {
            eprintln!("[lailaisay-stt] last.wav: {e}");
        }
        eprintln!(
            "[lailaisay-stt] whisper language={} prompt_chars={}",
            language.unwrap_or("auto"),
            initial_prompt.map(|s| s.chars().count()).unwrap_or(0)
        );

        let ctx = self
            .ctx
            .lock()
            .map_err(|e| SttError::Whisper(e.to_string()))?;
        let mut state = ctx
            .create_state()
            .map_err(|e| SttError::Whisper(e.to_string()))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_print_special(false);
        params.set_print_progress(true);
        params.set_print_realtime(false);
        // print_timestamps only affects C stderr when print_realtime is on.
        params.set_print_timestamps(false);
        // Segment t0/t1 come from whisper.cpp (centiseconds) without token-level
        // timestamps. Token timestamps leaked digits (`1` / `１`) into text.
        params.set_no_timestamps(false);
        params.set_token_timestamps(false);
        // Short digit / sequence clips: keep the first greedy decode instead of
        // escalating temperature into CC / 字幕製作 hallucinations.
        params.set_suppress_blank(crate::whisper_decode::SUPPRESS_BLANK);
        params.set_suppress_non_speech_tokens(crate::whisper_decode::SUPPRESS_NON_SPEECH_TOKENS);
        params.set_no_speech_thold(crate::whisper_decode::NO_SPEECH_THOLD);
        params.set_logprob_thold(crate::whisper_decode::LOGPROB_THOLD);
        params.set_temperature(crate::whisper_decode::TEMPERATURE);
        params.set_temperature_inc(crate::whisper_decode::TEMPERATURE_INC);
        if let Some(lang) = language {
            // whisper.cpp wants "zh" not "zh-tw"; Traditional is a post-step.
            let short = lang.split(['-', '_']).next().unwrap_or(lang);
            params.set_language(Some(short));
        }
        if let Some(prompt) = initial_prompt {
            if !prompt.is_empty() {
                params.set_initial_prompt(prompt);
            }
        }

        let started = std::time::Instant::now();
        eprintln!(
            "[lailaisay-stt] starting Whisper inference ({} samples)",
            samples.len()
        );
        state
            .full(params, samples)
            .map_err(|e| SttError::Whisper(e.to_string()))?;

        eprintln!(
            "[lailaisay-stt] Whisper inference completed in {:.2}s",
            started.elapsed().as_secs_f64()
        );
        let n = state
            .full_n_segments()
            .map_err(|e| SttError::Whisper(e.to_string()))?;
        let mut text = String::new();
        let mut segments = Vec::new();
        for i in 0..n {
            let seg = match state.full_get_segment_text(i) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let t0 = state
                .full_get_segment_t0(i)
                .map(whisper_centiseconds_to_secs)
                .unwrap_or(0.0);
            let t1 = state
                .full_get_segment_t1(i)
                .map(whisper_centiseconds_to_secs)
                .unwrap_or(t0);
            let trimmed = seg.trim();
            if !trimmed.is_empty() {
                segments.push(TranscriptionSegment::new(trimmed.to_string(), t0, t1));
                text.push_str(trimmed);
            }
        }
        let text = text.trim().to_string();
        if let Some(line) = lailaisay_core::format_unusable_whisper_log(
            &text,
            language,
            initial_prompt.map(|s| s.chars().count()).unwrap_or(0),
        ) {
            eprintln!("{line}");
        }
        Ok(Transcript {
            text,
            language: language.map(|s| s.to_string()),
            segments,
        })
    }
}
