//! Private line-delimited protocol. stdout is reserved for replies; diagnostics use stderr.
use crate::Transcript;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Request {
    pub samples: Vec<f32>,
    pub language: Option<String>,
    pub prompt: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Reply {
    Ready {
        device: String,
    },
    Transcript {
        text: String,
        language: Option<String>,
        segments: Vec<(String, f64, f64)>,
    },
    Error(String),
}

impl From<Transcript> for Reply {
    fn from(t: Transcript) -> Self {
        Self::Transcript {
            text: t.text,
            language: t.language,
            segments: t
                .segments
                .into_iter()
                .map(|s| (s.text, s.start, s.end))
                .collect(),
        }
    }
}

impl Reply {
    pub fn transcript(self) -> crate::Result<Transcript> {
        match self {
            Self::Transcript {
                text,
                language,
                segments,
            } => Ok(Transcript {
                text,
                language,
                segments: segments
                    .into_iter()
                    .map(|(text, start, end)| {
                        lailaisay_core::TranscriptionSegment::new(text, start, end)
                    })
                    .collect(),
            }),
            Self::Error(e) => Err(crate::SttError::Whisper(e)),
            Self::Ready { .. } => Err(crate::SttError::Whisper(
                "unexpected worker ready message".into(),
            )),
        }
    }
}
