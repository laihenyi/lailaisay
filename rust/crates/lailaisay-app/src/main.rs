//! lailaisay host.
//!
//! macOS: CGEvent tap + menu extra + record → transcribe → enhance → paste.
//! Windows: WH_KEYBOARD_LL + notification-area tray + SendInput paste.
//! `--once` is the TCC-free first smoke (file/text → filters → print).
//! Linux default mode (no `--once`) prints setup notes and exits 0.

#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::{Parser, ValueEnum};
use lailaisay_app::{
    disable_llm_for_once, find_sample_wav, load_app_context, open_backend_kind, run_text_pipeline,
};
use lailaisay_core::{settings_path, whisper_decode_language, LailaisaySettings};
use lailaisay_stt::{open_auto, BackendKind, Transcript};

#[derive(Parser, Debug)]
#[command(
    name = "lailaisay-app",
    about = "lailaisay menu-bar: hold hotkey → record → transcribe → paste"
)]
struct Args {
    /// Run the pipeline once (file or --text) and exit. No tap, mic, or TCC.
    #[arg(long)]
    once: bool,
    /// WAV for `--once` (defaults to rust/fixtures/sample.wav).
    #[arg(long)]
    file: Option<PathBuf>,
    /// Skip STT and process this text (`--once`).
    #[arg(long)]
    text: Option<String>,
    /// STT backend for `--once`. `dummy` is the guaranteed first smoke.
    #[arg(long, value_enum, default_value_t = BackendArg::Dummy)]
    backend: BackendArg,
    /// Opt in to paste/copy after `--once` (needs Accessibility on macOS).
    #[arg(long)]
    paste: bool,
    /// Legacy: `--once` already skips paste unless `--paste`.
    #[arg(long, hide = true)]
    no_paste: bool,
    /// Call Ollama/Groq after `--once` (off by default so a broken LLM cannot fail the smoke).
    #[arg(long)]
    enhance: bool,
    /// ggml/gguf model for `--once` / the menu-bar worker.
    #[arg(long)]
    model: Option<PathBuf>,
    /// Tray without a global hotkey hook (debug / no Accessibility or hook yet).
    #[arg(long)]
    no_tap: bool,
    /// Force the Settings window open (overrides minimize-to-menu-bar).
    #[arg(long)]
    settings: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum BackendArg {
    Dummy,
    Auto,
    Whisper,
}

fn main() -> Result<()> {
    let result = run();
    eprintln!("[lailaisay-app] main returned: {result:?}");
    result
}

fn run() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    eprintln!(
        "[lailaisay-app] pid={} build={}",
        std::process::id(),
        option_env!("GITHUB_SHA").unwrap_or("local")
    );
    let args = Args::parse();
    #[cfg(target_os = "windows")]
    lailaisay_app::windows_install::register_running_app()?;
    if args.once {
        return run_once(args);
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let settings = LailaisaySettings::for_launch(&settings_path());
        return lailaisay_app::run_with(lailaisay_app::runtime_opts_from_settings(
            &settings,
            !args.no_tap,
            args.settings,
        ));
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        use lailaisay_input::{ACCESSIBILITY_HELP, MICROPHONE_HELP};
        use lailaisay_paste::PASTE_TCC_HELP;

        if args.settings {
            let settings = LailaisaySettings::for_launch(&settings_path());
            return lailaisay_app::run_with(lailaisay_app::runtime_opts_from_settings(
                &settings, false, true,
            ));
        }

        eprintln!(
            "lailaisay-app tray / global hotkey loop is macOS and Windows (this host is {}).",
            std::env::consts::OS
        );
        eprintln!();
        eprintln!("First smoke (no TCC, any OS):");
        eprintln!("  cargo run -p lailaisay-app -- --once");
        eprintln!("  # expected stdout: 去台南。");
        eprintln!();
        eprintln!("Settings UI on this machine (needs a display):");
        eprintln!("  cargo run -p lailaisay-app -- --settings");
        eprintln!();
        eprintln!("On a Mac, after --once works, see rust/MAC_SMOKE.md for tray + Settings.");
        eprintln!(
            "  ./scripts/package-macos-app.sh          # → dist/lailaisay.app (com.yikai.lailaisay, Apple Silicon)"
        );
        eprintln!("  open dist/lailaisay.app                      # grant TCC to lailaisay.app, not Terminal");
        eprintln!("  # or still: cargo run -p lailaisay-app --release --features mic,whisper");
        eprintln!("  First launch opens Settings; later launches stay in the menu bar (tray 「開啟設定」).");
        eprintln!();
        eprintln!("On Windows, see rust/WINDOWS.md:");
        eprintln!("  cargo run -p lailaisay-app --release --features mic,whisper");
        eprintln!(
            "  # notification-area tray: 開啟設定 / 結束 lailaisay; close Settings hides to tray."
        );
        eprintln!();
        eprintln!("{ACCESSIBILITY_HELP}");
        eprintln!("{MICROPHONE_HELP}");
        eprintln!("{PASTE_TCC_HELP}");
        Ok(())
    }
}

fn run_once(args: Args) -> Result<()> {
    let do_paste = args.paste && !args.no_paste;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let mut ctx = load_app_context()?;
        if !args.enhance {
            disable_llm_for_once(&mut ctx);
        }

        let transcript = if let Some(text) = args.text {
            eprintln!(
                "[lailaisay-app --once] backend=text paste={do_paste} settings={}",
                settings_path().display()
            );
            Transcript::plain(text, None)
        } else {
            let wav = args
                .file
                .or_else(find_sample_wav)
                .ok_or_else(|| anyhow::anyhow!("pass --file or --text (no fixtures/sample.wav found)"))?;
            if !wav.exists() {
                bail!("WAV not found: {}", wav.display());
            }
            let sidecar = wav.parent();
            let kind = match args.backend {
                BackendArg::Dummy => BackendKind::Dummy,
                BackendArg::Auto => {
                    eprintln!(
                        "[lailaisay-app --once] backend=auto file={} paste={do_paste} settings={}",
                        wav.display(),
                        settings_path().display()
                    );
                    let stt = open_auto(args.model.as_deref(), sidecar)?;
                    let lang = whisper_decode_language(&ctx.settings);
                    let transcript = stt.transcribe_wav(&wav, lang.as_deref(), None)?;
                    let out = run_text_pipeline(&transcript, &ctx, do_paste).await?;
                    print_once_result(&out.polished, do_paste);
                    return Ok(());
                }
                BackendArg::Whisper => BackendKind::Whisper,
            };
            eprintln!(
                "[lailaisay-app --once] backend={:?} file={} paste={do_paste} settings={}",
                kind,
                wav.display(),
                settings_path().display()
            );
            let stt = open_backend_kind(kind, args.model.as_deref(), sidecar).map_err(|e| {
                anyhow::anyhow!(
                    "{e}\nHint: first smoke uses --backend dummy (default). For real STT: \
                     rust/scripts/download-whisper-tiny.sh then \
                     cargo run -p lailaisay-app --features whisper -- --once --backend whisper --model \"$TOK_WHISPER_MODEL\""
                )
            })?;
            let lang = whisper_decode_language(&ctx.settings);
            stt.transcribe_wav(&wav, lang.as_deref(), None)?
        };

        let out = run_text_pipeline(&transcript, &ctx, do_paste).await?;
        print_once_result(&out.polished, do_paste);
        Ok(())
    })
}

fn print_once_result(out: &str, do_paste: bool) {
    println!("{out}");
    if out.contains("去台南") {
        eprintln!("[lailaisay-app --once] pipeline OK (dummy fixture path).");
    } else {
        eprintln!("[lailaisay-app --once] pipeline finished.");
    }
    if do_paste {
        eprintln!(
            "[lailaisay-app --once] paste was requested (Accessibility / Automation may have prompted)."
        );
    } else {
        eprintln!(
            "[lailaisay-app --once] no TCC used (no tap, mic, or paste). Next: rust/MAC_SMOKE.md"
        );
    }
}
