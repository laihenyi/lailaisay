//! CLI proof path: file or mic → STT → local pipeline → optional LLM → print/copy.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use lailaisay_core::{
    apply_post_llm_terms, custom_words_path, phonetic_glossary_path, post_process, settings_path,
    whisper_decode_language, whisper_initial_prompt, AiEnhancementMode, CustomWordDictionary,
    LailaisaySettings, OutputStyle, PhoneticGlossary, PostProcessOptions, TranscriptionSegment,
};
use lailaisay_enhance::enhance_with_note_vocab;
use lailaisay_stt::{open_auto, open_backend, BackendKind, Transcriber};

#[derive(Parser, Debug)]
#[command(
    name = "lailaisay-cli",
    about = "lailaisay — record / file → local STT → optional AI enhance",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Transcribe a WAV (or the checked-in sample fixture).
    Transcribe {
        /// WAV path. Defaults to rust/fixtures/sample.wav when omitted.
        #[arg(short, long)]
        file: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = BackendArg::Auto)]
        backend: BackendArg,
        /// ggml/gguf Whisper model (or TOK_WHISPER_MODEL).
        #[arg(long)]
        model: Option<PathBuf>,
        /// Force LLM enhancement (overrides settings mode → full).
        #[arg(long)]
        enhance: bool,
        /// Copy result to the clipboard.
        #[arg(long)]
        copy: bool,
        /// Try paste (macOS AX / ⌘V; Windows and Linux copy-only).
        #[arg(long)]
        paste: bool,
        #[arg(long)]
        language: Option<String>,
    },
    /// Record from the microphone, then run the same pipeline.
    Record {
        #[arg(short, long, default_value_t = 3.0)]
        seconds: f64,
        /// Honour the 200 ms double-tap delay before capture starts.
        #[arg(long, default_value_t = true)]
        hold_delay: bool,
        #[arg(long, value_enum, default_value_t = BackendArg::Auto)]
        backend: BackendArg,
        #[arg(long)]
        model: Option<PathBuf>,
        #[arg(long)]
        enhance: bool,
        #[arg(long)]
        copy: bool,
        #[arg(long)]
        language: Option<String>,
        #[arg(long)]
        mic: Option<String>,
    },
    /// Run only the local post-STT pipeline (no audio).
    Process {
        #[arg(short, long)]
        text: String,
        #[arg(long)]
        enhance: bool,
        #[arg(long)]
        copy: bool,
    },
    /// Show or write settings JSON.
    Settings {
        #[command(subcommand)]
        action: SettingsCmd,
    },
}

#[derive(Subcommand, Debug)]
enum SettingsCmd {
    Show,
    /// Write defaults (and seed bundled dictionaries) if files are missing.
    Init,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum BackendArg {
    Auto,
    Dummy,
    Whisper,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Transcribe {
            file,
            backend,
            model,
            enhance,
            copy,
            paste,
            language,
        } => {
            let wav = resolve_wav(file)?;
            let (settings, dict, glossary) = load_context(enhance, language)?;
            let backend = open_stt(backend, model.as_deref(), wav.parent())?;
            run_pipeline(
                &*backend,
                Some(&wav),
                None,
                &settings,
                &dict,
                &glossary,
                copy,
                paste,
            )
            .await
        }
        Command::Record {
            seconds,
            hold_delay,
            backend,
            model,
            enhance,
            copy,
            language,
            mic,
        } => {
            if hold_delay {
                tokio::time::sleep(lailaisay_stt::record_start_delay()).await;
            }
            eprintln!(
                "recording {seconds:.1}s from {}…",
                mic.as_deref().unwrap_or("default mic")
            );
            let samples = record_from_mic(seconds, mic.as_deref())?;
            let (settings, dict, glossary) = load_context(enhance, language)?;
            let backend = open_stt(backend, model.as_deref(), None)?;
            run_pipeline(
                &*backend,
                None,
                Some(&samples),
                &settings,
                &dict,
                &glossary,
                copy,
                false,
            )
            .await
        }
        Command::Process {
            text,
            enhance,
            copy,
        } => {
            let (settings, dict, glossary) = load_context(enhance, None)?;
            finish_text(&text, None, &settings, &dict, &glossary, copy, false).await
        }
        Command::Settings { action } => match action {
            SettingsCmd::Show => {
                let path = settings_path();
                let settings = if path.exists() {
                    LailaisaySettings::load_path(&path)?
                } else {
                    LailaisaySettings::default()
                };
                println!("{}", serde_json::to_string_pretty(&settings)?);
                eprintln!("path: {}", path.display());
                Ok(())
            }
            SettingsCmd::Init => {
                init_files()?;
                Ok(())
            }
        },
    }
}

#[cfg(feature = "mic")]
fn record_from_mic(seconds: f64, mic: Option<&str>) -> Result<Vec<f32>> {
    lailaisay_stt::record::record_seconds(seconds, mic)
        .context("microphone recording failed (use `lailaisay-cli transcribe --file` on CI)")
}

#[cfg(not(feature = "mic"))]
fn record_from_mic(_seconds: f64, _mic: Option<&str>) -> Result<Vec<f32>> {
    bail!("rebuild with `--features mic` (and install libasound2-dev on Linux) to record from a microphone")
}

fn open_stt(
    backend: BackendArg,
    model: Option<&Path>,
    sidecar: Option<&Path>,
) -> Result<Box<dyn Transcriber>> {
    match backend {
        BackendArg::Auto => Ok(open_auto(model, sidecar)?),
        BackendArg::Dummy => Ok(open_backend(BackendKind::Dummy, model, sidecar)?),
        BackendArg::Whisper => Ok(open_backend(BackendKind::Whisper, model, sidecar)?),
    }
}

fn load_context(
    force_enhance: bool,
    language: Option<String>,
) -> Result<(LailaisaySettings, CustomWordDictionary, PhoneticGlossary)> {
    let mut settings = if settings_path().exists() {
        LailaisaySettings::load_path(&settings_path())?
    } else {
        LailaisaySettings::default()
    };
    if let Some((old, next)) = settings.apply_whisper_model_remap() {
        eprintln!("selectedWhisperModel remapped {old} → {next}");
        let path = settings_path();
        if path.exists() {
            if let Err(e) = settings.save_path(&path) {
                eprintln!("could not persist remapped selectedWhisperModel: {e}");
            }
        }
    }
    if force_enhance {
        settings.ai_enhancement_mode = AiEnhancementMode::Full;
    }
    if let Some(lang) = language {
        settings.output_language = Some(lang);
    }

    let dict = load_dictionary()?;
    let glossary = load_glossary()?;
    Ok((settings, dict, glossary))
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

async fn run_pipeline(
    backend: &dyn Transcriber,
    wav: Option<&Path>,
    pcm: Option<&[f32]>,
    settings: &LailaisaySettings,
    dict: &CustomWordDictionary,
    glossary: &PhoneticGlossary,
    copy: bool,
    paste: bool,
) -> Result<()> {
    let prompt = whisper_initial_prompt(settings, Some(&dict.prompt_text()));

    let started = Instant::now();
    let lang = whisper_decode_language(settings);
    let transcript = if let Some(path) = wav {
        eprintln!("transcribing {} …", path.display());
        backend.transcribe_wav(path, lang.as_deref(), prompt.as_deref())?
    } else if let Some(samples) = pcm {
        backend.transcribe_pcm16k(samples, lang.as_deref(), prompt.as_deref())?
    } else {
        bail!("no audio");
    };
    eprintln!(
        "stt {:.0}ms  raw={:?}  segments={}",
        started.elapsed().as_millis(),
        transcript.text,
        transcript.segments.len()
    );
    finish_text(
        &transcript.text,
        transcript.post_process_segments(),
        settings,
        dict,
        glossary,
        copy,
        paste,
    )
    .await
}

async fn finish_text(
    raw: &str,
    segments: Option<&[TranscriptionSegment]>,
    settings: &LailaisaySettings,
    dict: &CustomWordDictionary,
    glossary: &PhoneticGlossary,
    copy: bool,
    paste: bool,
) -> Result<()> {
    let local = post_process(
        raw,
        PostProcessOptions {
            settings,
            dictionary: dict,
            glossary,
            segments,
        },
    );
    eprintln!("local pipeline: {local:?}");

    let style = if settings.enable_context_aware_style {
        OutputStyle::General
    } else {
        OutputStyle::General
    };
    let vocab = dict.polish_vocabulary_block();
    let final_text = enhance_with_note_vocab(&local, settings, style, vocab.as_deref())
        .await
        .map(|o| o.text)
        .unwrap_or_else(|e| {
            eprintln!("enhancement skipped: {e}");
            local.clone()
        });
    let final_text = apply_post_llm_terms(&final_text, dict, glossary);

    if final_text.trim().is_empty() {
        let lang = whisper_decode_language(settings);
        let prompt = whisper_initial_prompt(settings, Some(&dict.prompt_text()));
        if let Some(line) = lailaisay_core::format_unusable_whisper_log(
            raw,
            lang.as_deref(),
            prompt.as_deref().map(|s| s.chars().count()).unwrap_or(0),
        ) {
            eprintln!("{line}");
        }
        eprintln!("empty transcript — skip clipboard/paste");
        println!();
        return Ok(());
    }

    if copy || paste {
        match lailaisay_paste::copy_text(&final_text) {
            Ok(()) => eprintln!("copied to clipboard"),
            Err(e) => eprintln!("clipboard unavailable: {e}"),
        }
    }
    if paste {
        match lailaisay_paste::paste_text(&final_text, settings) {
            Ok(()) => eprintln!("pasted"),
            Err(e) => eprintln!("paste: {e}"),
        }
    }

    println!("{final_text}");
    Ok(())
}

fn resolve_wav(file: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = file {
        return Ok(p);
    }
    find_workspace_file("fixtures/sample.wav")
        .context("no --file and could not find rust/fixtures/sample.wav")
}

fn init_files() -> Result<()> {
    let settings_p = settings_path();
    if !settings_p.exists() {
        LailaisaySettings::default().save_path(&settings_p)?;
        eprintln!("wrote {}", settings_p.display());
    } else {
        eprintln!("exists {}", settings_p.display());
    }
    if let Some(src) = bundled_data("DefaultCustomWords.json") {
        let dst = custom_words_path();
        if !dst.exists() {
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&src, &dst)?;
            eprintln!("wrote {}", dst.display());
        }
    }
    if let Some(src) = bundled_data("DefaultPhoneticGlossary.json") {
        let dst = phonetic_glossary_path();
        if !dst.exists() {
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&src, &dst)?;
            eprintln!("wrote {}", dst.display());
        }
    }
    Ok(())
}

fn bundled_data(name: &str) -> Option<PathBuf> {
    find_workspace_file(&format!("data/{name}"))
}

fn find_workspace_file(rel: &str) -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    for _ in 0..8 {
        let rust = dir.join("rust").join(rel);
        if rust.exists() {
            return Some(rust);
        }
        let direct = dir.join(rel);
        if direct.exists() && dir.join("Cargo.toml").exists() {
            return Some(direct);
        }
        if !dir.pop() {
            break;
        }
    }
    // When running the binary from rust/crates/lailaisay-cli
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let from_crate = manifest.join("../../").join(rel);
    if from_crate.exists() {
        return Some(from_crate);
    }
    None
}
