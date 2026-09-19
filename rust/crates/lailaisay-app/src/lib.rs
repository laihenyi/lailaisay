//! Shared session + pipeline for `lailaisay-app`.
//!
//! Linux CI tests delayed start, cancel, double-tap lock, the post-STT hook,
//! model listing, and resident-STT caching. The live tray + CGEvent tap lives
//! behind `#[cfg(target_os = "macos")]`; the settings window is eframe on every OS.

pub mod desktop;
pub mod dock_icon;
pub mod host;
pub mod mic_start;
pub mod models;
pub mod pipeline;
pub mod session;
pub mod settings_ui;
pub mod shared;
pub mod status_hud;
pub mod theme;
#[cfg(target_os = "windows")]
pub mod windows_install;

#[cfg(target_os = "macos")]
pub mod macos_runtime;

pub use host::{run, run_with, runtime_opts_from_settings, RuntimeOpts};
pub use pipeline::{
    disable_llm_for_once, find_sample_wav, load_app_context, open_backend_kind, run_edit_pipeline,
    run_text_pipeline, AppContext, TextPipelineOutcome,
};
pub use session::{armed_hold_status, Session, SessionAction};
