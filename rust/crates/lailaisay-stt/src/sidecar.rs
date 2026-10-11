//! Resident child process speaking the line-delimited `worker_protocol`.
use crate::{
    worker_protocol::Reply,
    Result, SttError, Transcript,
};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

pub(crate) fn error(e: impl std::fmt::Display) -> SttError {
    SttError::Whisper(e.to_string())
}

pub(crate) struct Session {
    pub(crate) child: Child,
    requests: mpsc::Sender<String>,
    replies: mpsc::Receiver<std::result::Result<Reply, String>>,
    pub(crate) device: String,
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Session {
    pub(crate) fn start(command: &mut Command) -> Result<Self> {
        command
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
    pub(crate) fn transcribe(&mut self, request: &str, timeout: Duration) -> Result<Transcript> {
        self.requests.send(request.to_owned()).map_err(error)?;
        self.receive(timeout)?.transcript()
    }
}

