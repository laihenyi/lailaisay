use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::{Result, SttError};

/// Start/stop microphone capture (press-and-hold). `record_seconds` is the CLI helper.

/// Record from the default (or named) input device via cpal.
///
/// Works on Linux and macOS when a microphone is present. CI should use
/// `--file` instead of `--record`.
pub fn record_seconds(seconds: f64, device_name: Option<&str>) -> Result<Vec<f32>> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let host = cpal::default_host();
    let device = if let Some(name) = device_name {
        host.input_devices()
            .map_err(|e| SttError::Record(e.to_string()))?
            .find(|d| d.name().ok().as_deref() == Some(name))
            .ok_or_else(|| SttError::Record(format!("microphone not found: {name}")))?
    } else {
        host.default_input_device()
            .ok_or_else(|| SttError::Record("no default input device".into()))?
    };

    let config = device
        .default_input_config()
        .map_err(|e| SttError::Record(e.to_string()))?;
    let sample_rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    let collected: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let buf = collected.clone();

    let err_fn = |e| eprintln!("[lailaisay-stt] cpal error: {e}");

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            let buf = buf.clone();
            device
                .build_input_stream(
                    &config.into(),
                    move |data: &[f32], _| append_frames(data, channels, &buf),
                    err_fn,
                    None,
                )
                .map_err(|e| SttError::Record(e.to_string()))?
        }
        cpal::SampleFormat::I16 => {
            let buf = buf.clone();
            device
                .build_input_stream(
                    &config.into(),
                    move |data: &[i16], _| {
                        let f: Vec<f32> =
                            data.iter().map(|s| *s as f32 / i16::MAX as f32).collect();
                        append_frames(&f, channels, &buf);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| SttError::Record(e.to_string()))?
        }
        other => {
            return Err(SttError::Record(format!(
                "unsupported input sample format {other:?}"
            )))
        }
    };

    stream.play().map_err(|e| SttError::Record(e.to_string()))?;
    std::thread::sleep(Duration::from_secs_f64(seconds.max(0.05)));
    drop(stream);

    let samples = collected
        .lock()
        .map_err(|e| SttError::Record(e.to_string()))?
        .clone();
    Ok(resample_to_16k(&samples, sample_rate))
}

/// Hold-to-record: start the stream, later [`Self::stop`] to get 16 kHz mono f32.
///
/// `cpal::Stream` is `!Send` on macOS (Core Audio). Keep this value on the
/// thread that called [`Self::start`]; do not return it through `thread::spawn`.
pub struct LiveRecorder {
    _stream: cpal::Stream,
    collected: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
}

impl LiveRecorder {
    pub fn start(device_name: Option<&str>) -> Result<Self> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

        let host = cpal::default_host();
        let device = if let Some(name) = device_name {
            host.input_devices()
                .map_err(|e| SttError::Record(e.to_string()))?
                .find(|d| d.name().ok().as_deref() == Some(name))
                .ok_or_else(|| SttError::Record(format!("microphone not found: {name}")))?
        } else {
            host.default_input_device()
                .ok_or_else(|| SttError::Record("no default input device".into()))?
        };

        let config = device
            .default_input_config()
            .map_err(|e| SttError::Record(e.to_string()))?;
        let sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;
        let collected: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
        let buf = collected.clone();
        let err_fn = |e| eprintln!("[lailaisay-stt] cpal error: {e}");

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                let buf = buf.clone();
                device
                    .build_input_stream(
                        &config.into(),
                        move |data: &[f32], _| append_frames(data, channels, &buf),
                        err_fn,
                        None,
                    )
                    .map_err(|e| SttError::Record(e.to_string()))?
            }
            cpal::SampleFormat::I16 => {
                let buf = buf.clone();
                device
                    .build_input_stream(
                        &config.into(),
                        move |data: &[i16], _| {
                            let f: Vec<f32> =
                                data.iter().map(|s| *s as f32 / i16::MAX as f32).collect();
                            append_frames(&f, channels, &buf);
                        },
                        err_fn,
                        None,
                    )
                    .map_err(|e| SttError::Record(e.to_string()))?
            }
            other => {
                return Err(SttError::Record(format!(
                    "unsupported input sample format {other:?}"
                )))
            }
        };

        stream.play().map_err(|e| SttError::Record(e.to_string()))?;
        Ok(Self {
            _stream: stream,
            collected,
            sample_rate,
        })
    }

    pub fn stop(self) -> Result<Vec<f32>> {
        eprintln!("[lailaisay-stt] stopping microphone stream");
        drop(self._stream);
        eprintln!("[lailaisay-stt] microphone stream stopped; resampling");
        let samples = self
            .collected
            .lock()
            .map_err(|e| SttError::Record(e.to_string()))?
            .clone();
        Ok(resample_to_16k(&samples, self.sample_rate))
    }
}

fn append_frames(data: &[f32], channels: usize, buf: &Mutex<Vec<f32>>) {
    let mut guard = buf.lock().expect("record buffer");
    if channels <= 1 {
        guard.extend_from_slice(data);
        return;
    }
    for frame in data.chunks(channels) {
        let mono = frame.iter().sum::<f32>() / channels as f32;
        guard.push(mono);
    }
}

fn resample_to_16k(input: &[f32], from: u32) -> Vec<f32> {
    if from == 16_000 {
        return input.to_vec();
    }
    let ratio = from as f64 / 16_000.0;
    if input.is_empty() {
        return Vec::new();
    }
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
