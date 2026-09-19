//! Windows GPU faults must not terminate the tray process. Keep a resident decoder
//! in a child process and retry the same samples on CPU after a GPU failure.
use crate::{
    worker_protocol::{Reply, Request},
    Result, SttError, Transcriber, Transcript,
};
use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{mpsc, Mutex},
    thread,
    time::Duration,
};

fn error(e: impl std::fmt::Display) -> SttError {
    SttError::Whisper(e.to_string())
}

struct Session {
    child: Child,
    requests: mpsc::Sender<String>,
    replies: mpsc::Receiver<std::result::Result<Reply, String>>,
    device: String,
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Session {
    fn start(exe: &Path, model: &Path) -> Result<Self> {
        let mut command = Command::new(exe);
        command
            .arg(model)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        let mut child = command.spawn().map_err(error)?;
        let mut input = child
            .stdin
            .take()
            .ok_or_else(|| error("worker stdin unavailable"))?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| error("worker stdout unavailable"))?;
        let (requests, inbox) = mpsc::channel::<String>();
        let (outbox, replies) = mpsc::channel();
        thread::spawn(move || {
            for line in inbox {
                if writeln!(input, "{line}")
                    .and_then(|_| input.flush())
                    .is_err()
                {
                    break;
                }
            }
        });
        thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let reply = line
                    .map_err(|e| e.to_string())
                    .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()));
                if outbox.send(reply).is_err() {
                    break;
                }
            }
        });
        let mut session = Self {
            child,
            requests,
            replies,
            device: String::new(),
        };
        session.device = match session.receive(Duration::from_secs(120))? {
            Reply::Ready { device } => device,
            Reply::Error(e) => return Err(error(e)),
            _ => return Err(error("worker did not send ready message")),
        };
        Ok(session)
    }
    fn receive(&mut self, timeout: Duration) -> Result<Reply> {
        self.replies
            .recv_timeout(timeout)
            .map_err(|e| {
                let exit = self.child.try_wait().ok().flatten();
                error(format!("worker unavailable ({e}); exit={exit:?}"))
            })?
            .map_err(error)
    }
    fn transcribe(&mut self, request: &str, timeout: Duration) -> Result<Transcript> {
        self.requests.send(request.to_owned()).map_err(error)?;
        self.receive(timeout)?.transcript()
    }
}

struct State {
    session: Option<Session>,
    next: usize,
    active: usize,
    failures: Vec<String>,
}

pub struct ProcessTranscriber {
    model: PathBuf,
    candidates: Vec<(&'static str, PathBuf)>,
    state: Mutex<State>,
    note: Mutex<String>,
}

/// Runtime CPU checks include OS support for the vector register state.
pub fn cpu_supports_avx2() -> bool {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        std::is_x86_feature_detected!("avx2")
            && std::is_x86_feature_detected!("fma")
            && std::is_x86_feature_detected!("f16c")
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        false
    }
}

impl ProcessTranscriber {
    pub fn load(model: &Path) -> Result<Self> {
        if !model.is_file() {
            return Err(error(format!("model missing: {}", model.display())));
        }
        let exe = std::env::current_exe()?;
        let dir = exe
            .parent()
            .ok_or_else(|| error("application directory missing"))?;
        let mut candidates = vec![("GPU Vulkan", dir.join("lailaisay-whisper-vulkan.exe"))];
        if cpu_supports_avx2() {
            candidates.push(("CPU AVX2", dir.join("lailaisay-whisper-avx2.exe")));
        }
        candidates.push(("CPU 相容模式", dir.join("lailaisay-whisper-cpu.exe")));
        if !candidates.last().unwrap().1.is_file() {
            return Err(error(
                "缺少 lailaisay-whisper-cpu.exe，請解壓縮完整 lailaisay 套件",
            ));
        }
        eprintln!(
            "[lailaisay-stt] auto device selection; CPU AVX2/FMA/F16C={}",
            cpu_supports_avx2()
        );
        Ok(Self {
            model: model.to_owned(),
            candidates,
            state: Mutex::new(State {
                session: None,
                next: 0,
                active: 0,
                failures: Vec::new(),
            }),
            note: Mutex::new("自動：首次辨識偵測 GPU；不可用時改用 CPU".into()),
        })
    }
}

impl Transcriber for ProcessTranscriber {
    fn device_note(&self) -> Option<String> {
        self.note.lock().ok().map(|s| s.clone())
    }
    fn transcribe_pcm16k(
        &self,
        samples: &[f32],
        language: Option<&str>,
        initial_prompt: Option<&str>,
    ) -> Result<Transcript> {
        let request = serde_json::to_string(&Request {
            samples: samples.to_vec(),
            language: language.map(str::to_owned),
            prompt: initial_prompt.map(str::to_owned),
        })
        .map_err(error)?;
        let mut state = self.state.lock().map_err(error)?;
        loop {
            if state.session.is_none() {
                if state.next >= self.candidates.len() {
                    let note = format!(
                        "辨識引擎皆失敗：{}。請重新載入模型後重試。",
                        state.failures.join("；")
                    );
                    *self.note.lock().map_err(error)? = note.clone();
                    return Err(error(note));
                }
                state.active = state.next;
                state.next += 1;
                let (label, path) = &self.candidates[state.active];
                *self.note.lock().map_err(error)? = format!(
                    "正在嘗試 {label}{}",
                    if state.failures.is_empty() {
                        ""
                    } else {
                        "（前一引擎失敗，改用下一引擎）"
                    }
                );
                eprintln!("[lailaisay-stt] trying {label}");
                match Session::start(path, &self.model) {
                    Ok(session) => state.session = Some(session),
                    Err(e) => {
                        eprintln!("[lailaisay-stt] {label} unavailable: {e}; trying next backend");
                        state.failures.push(format!("{label}: {e}"));
                        continue;
                    }
                }
            }
            let gpu = self.candidates[state.active].0 == "GPU Vulkan";
            let timeout = Duration::from_secs(if gpu { 120 } else { 600 });
            let label = self.candidates[state.active].0;
            let session = state.session.as_mut().unwrap();
            match session.transcribe(&request, timeout) {
                Ok(t) => {
                    let mut note = format!("{label} · {}", session.device);
                    if !state.failures.is_empty() {
                        note.push_str(&format!("；已回退：{}。可更新顯示卡驅動後重啟重試 GPU；CPU 較慢可改用 base/tiny。", state.failures.join("；")));
                    }
                    eprintln!("[lailaisay-stt] {note}");
                    *self.note.lock().map_err(error)? = note;
                    return Ok(t);
                }
                Err(e) => {
                    let label = self.candidates[state.active].0;
                    eprintln!("[lailaisay-stt] {label} inference failed: {e}; retrying same audio on next backend");
                    state.failures.push(format!("{label}: {e}"));
                    state.session = None; // Kill and reap before allocating the next model.
                }
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path
    }
    fn transcriber(model: PathBuf, candidates: Vec<(&'static str, PathBuf)>) -> ProcessTranscriber {
        ProcessTranscriber {
            model,
            candidates,
            state: Mutex::new(State {
                session: None,
                next: 0,
                active: 0,
                failures: vec![],
            }),
            note: Mutex::new(String::new()),
        }
    }
    const CPU: &str = r#"echo '{"Ready":{"device":"CPU fixture"}}'
while IFS= read -r line; do
  echo '{"Transcript":{"text":"一二三","language":"zh","segments":[["一二三",0.0,1.5]]}}'
done"#;
    #[test]
    fn native_gpu_exit_retries_same_request_and_reuses_cpu() {
        let dir = tempfile::tempdir().unwrap();
        let gpu = script(
            dir.path(),
            "gpu",
            "echo '{\"Ready\":{\"device\":\"GPU fixture\"}}'\nread line\nexit 7",
        );
        let request_log = dir.path().join("request.json");
        let cpu_body = CPU.replace(
            "  echo",
            &format!(
                "  printf '%s' \"$line\" > '{}'\n  echo",
                request_log.display()
            ),
        );
        let cpu = script(dir.path(), "cpu", &cpu_body);
        let stt = transcriber(
            dir.path().join("model"),
            vec![("GPU Vulkan", gpu), ("CPU", cpu)],
        );
        for _ in 0..2 {
            let t = stt
                .transcribe_pcm16k(&[0.25, 0.5], Some("zh"), Some("數字"))
                .unwrap();
            assert_eq!(t.text, "一二三");
            assert_eq!(t.segments[0].end, 1.5);
        }
        let forwarded: Request =
            serde_json::from_str(&std::fs::read_to_string(request_log).unwrap()).unwrap();
        assert_eq!(forwarded.samples, vec![0.25, 0.5]);
        assert_eq!(forwarded.language.as_deref(), Some("zh"));
        assert_eq!(forwarded.prompt.as_deref(), Some("數字"));
        assert_eq!(stt.state.lock().unwrap().failures.len(), 1);
        assert!(stt.device_note().unwrap().contains("已回退"));
    }
    #[test]
    fn missing_gpu_loader_and_worker_do_not_prevent_cpu_transcription() {
        let dir = tempfile::tempdir().unwrap();
        let cpu = script(dir.path(), "cpu", CPU);
        let stt = transcriber(
            dir.path().join("model"),
            vec![("GPU Vulkan", dir.path().join("missing")), ("CPU", cpu)],
        );
        assert_eq!(
            stt.transcribe_pcm16k(&[0.5], None, None).unwrap().text,
            "一二三"
        );
    }
    #[test]
    fn worker_timeout_is_bounded_and_drop_reaps_child() {
        let dir = tempfile::tempdir().unwrap();
        let hung = script(
            dir.path(),
            "hung",
            "echo '{\"Ready\":{\"device\":\"GPU\"}}'\nwhile IFS= read -r line; do :; done",
        );
        let mut session = Session::start(&hung, Path::new("unused")).unwrap();
        assert!(session.transcribe("{}", Duration::from_millis(20)).is_err());
        session.child.kill().unwrap();
        assert!(session.child.wait().unwrap().code().is_none());
    }
    #[test]
    fn all_backends_failing_returns_error_instead_of_empty_success() {
        let dir = tempfile::tempdir().unwrap();
        let broken = script(dir.path(), "broken", "exit 9");
        let stt = transcriber(dir.path().join("model"), vec![("CPU", broken)]);
        assert!(stt.transcribe_pcm16k(&[0.1], None, None).is_err());
        assert!(stt.device_note().unwrap().contains("皆失敗"));
    }
}
