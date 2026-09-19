use lailaisay_stt::{
    worker_protocol::{Reply, Request},
    Transcriber,
};
use std::io::{BufRead, Write};

fn emit(reply: Reply) -> Result<(), Box<dyn std::error::Error>> {
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, &reply)?;
    writeln!(out)?;
    out.flush()?;
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("[lailaisay-worker] {e}");
        let _ = emit(Reply::Error(e.to_string()));
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            if tx.send(line).is_err() {
                return;
            }
        }
        // The UI owns this pipe. If it exits during native inference, stop this
        // worker too instead of leaving a model/GPU allocation orphaned.
        std::process::exit(0);
    });
    let model = std::env::args_os().nth(1).ok_or("model path missing")?;
    let mut params = whisper_rs::WhisperContextParameters::default();
    params.use_gpu = false;
    #[cfg(feature = "vulkan")]
    let device = {
        let (index, name) = lailaisay_stt::gpu_probe::detect_gpu()?;
        // Pin GGML's visible list to the physical device actually inspected.
        // Set before initializing whisper.cpp, before the decoder starts any native threads.
        std::env::set_var("GGML_VK_VISIBLE_DEVICES", index.to_string());
        params.use_gpu = true;
        params.gpu_device = 0;
        format!("GPU · Vulkan · {name}")
    };
    #[cfg(not(feature = "vulkan"))]
    let device = "CPU".to_owned();
    eprintln!("[lailaisay-worker] selected {device}");
    let stt = lailaisay_stt::whisper::WhisperCppTranscriber::load_with_parameters(
        std::path::Path::new(&model),
        params,
    )?;
    emit(Reply::Ready { device })?;
    for line in rx {
        let request: Request = serde_json::from_str(&line?)?;
        let reply = match stt.transcribe_pcm16k(
            &request.samples,
            request.language.as_deref(),
            request.prompt.as_deref(),
        ) {
            Ok(t) => t.into(),
            Err(e) => Reply::Error(e.to_string()),
        };
        emit(reply)?;
    }
    Ok(())
}
