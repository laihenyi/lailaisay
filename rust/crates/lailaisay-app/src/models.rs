//! Discover, catalog, and download ggml/gguf files for the settings model menu.

use std::path::{Path, PathBuf};
use std::process::Command;

use lailaisay_core::{models_dir, remapped_selected_whisper_model, LailaisaySettings};

const DEFAULT_HF_BASE: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogEntry {
    pub id: &'static str,
    pub filename: &'static str,
    pub label: &'static str,
    pub size_hint: &'static str,
    pub min_bytes: u64,
}

/// Official ggerganov/whisper.cpp ggml files (multilingual, plus large-v3-turbo).
pub const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        id: "tiny",
        filename: "ggml-tiny.bin",
        label: "tiny",
        size_hint: "~75 MB",
        min_bytes: 10_000_000,
    },
    CatalogEntry {
        id: "base",
        filename: "ggml-base.bin",
        label: "base",
        size_hint: "~142 MB",
        min_bytes: 50_000_000,
    },
    CatalogEntry {
        id: "small",
        filename: "ggml-small.bin",
        label: "small",
        size_hint: "~466 MB",
        min_bytes: 100_000_000,
    },
    CatalogEntry {
        id: "medium",
        filename: "ggml-medium.bin",
        label: "medium",
        size_hint: "~1.5 GB",
        min_bytes: 500_000_000,
    },
    CatalogEntry {
        id: "large-v3",
        filename: "ggml-large-v3.bin",
        label: "large-v3",
        size_hint: "~2.9 GB",
        min_bytes: 1_000_000_000,
    },
    CatalogEntry {
        id: "large-v3-turbo",
        filename: "ggml-large-v3-turbo.bin",
        label: "large-v3-turbo",
        size_hint: "~1.6 GB",
        min_bytes: 500_000_000,
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelSource {
    Local,
    Env,
    MacWhisper,
}

impl ModelSource {
    pub fn tag(&self) -> &'static str {
        match self {
            Self::Local => "lailaisay",
            Self::Env => "TOK_WHISPER_MODEL",
            Self::MacWhisper => "MacWhisper",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundModel {
    pub path: PathBuf,
    pub source: ModelSource,
}

pub fn catalog_by_id(id: &str) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|e| e.id == id)
}

pub fn catalog_dest(entry: &CatalogEntry) -> PathBuf {
    models_dir().join(entry.filename)
}

pub fn catalog_url(entry: &CatalogEntry) -> String {
    let key = format!(
        "TOK_WHISPER_{}_URL",
        entry.id.replace('-', "_").to_ascii_uppercase()
    );
    if let Ok(u) = std::env::var(&key) {
        if !u.is_empty() {
            return u;
        }
    }
    if entry.id == "tiny" {
        if let Ok(u) = std::env::var("TOK_WHISPER_TINY_URL") {
            if !u.is_empty() {
                return u;
            }
        }
    }
    let base = std::env::var("TOK_WHISPER_MODEL_BASE_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_HF_BASE.into());
    format!("{}/{}", base.trim_end_matches('/'), entry.filename)
}

pub fn catalog_is_downloaded(entry: &CatalogEntry) -> bool {
    file_looks_complete(&catalog_dest(entry), entry.min_bytes)
}

fn file_looks_complete(path: &Path, min_bytes: u64) -> bool {
    path.is_file()
        && std::fs::metadata(path)
            .map(|m| m.len() >= min_bytes)
            .unwrap_or(false)
}

pub fn is_whisper_weight(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(ext.as_str(), "bin" | "gguf" | "ggml")
}

/// ggml / gguf (and legacy `.bin`) files in `dir`, sorted by file name.
pub fn list_whisper_models(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for ent in entries.flatten() {
        let path = ent.path();
        if path.is_file() && is_whisper_weight(&path) {
            out.push(path);
        }
    }
    out.sort();
    out
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn library_support_dir() -> PathBuf {
    home_dir().join("Library").join("Application Support")
}

pub fn macwhisper_models_dir() -> PathBuf {
    if let Ok(p) = std::env::var("TOK_MACWHISPER_MODELS_DIR") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    library_support_dir().join("MacWhisper").join("models")
}

fn push_unique(out: &mut Vec<FoundModel>, path: PathBuf, source: ModelSource) {
    if out.iter().any(|e| e.path == path) {
        return;
    }
    out.push(FoundModel { path, source });
}

fn push_dir(out: &mut Vec<FoundModel>, dir: &Path, source: ModelSource) {
    for path in list_whisper_models(dir) {
        push_unique(out, path, source.clone());
    }
}

/// Local ggml/gguf files: lailaisay cache, `TOK_WHISPER_MODEL`, MacWhisper.
pub fn list_usable_models() -> Vec<FoundModel> {
    let mut out = Vec::new();
    push_dir(&mut out, &models_dir(), ModelSource::Local);

    if let Ok(env) = std::env::var("TOK_WHISPER_MODEL") {
        let p = PathBuf::from(env);
        if p.is_file() && is_whisper_weight(&p) {
            push_unique(&mut out, p, ModelSource::Env);
        } else if p.is_dir() {
            push_dir(&mut out, &p, ModelSource::Env);
        }
    }

    push_dir(&mut out, &macwhisper_models_dir(), ModelSource::MacWhisper);

    out.sort_by(|a, b| {
        a.source
            .tag()
            .cmp(b.source.tag())
            .then_with(|| a.path.file_name().cmp(&b.path.file_name()))
    });
    out
}

/// lailaisay cache plus env file (kept for older callers / tests).
pub fn list_available_models() -> Vec<PathBuf> {
    list_usable_models().into_iter().map(|m| m.path).collect()
}

pub fn friendly_model_label(model: &FoundModel) -> String {
    let name = model
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| model.path.display().to_string());
    let size = std::fs::metadata(&model.path)
        .map(|m| format!(" · {}", human_bytes(m.len())))
        .unwrap_or_default();
    let friendly = match name.as_str() {
        "ggml-tiny.bin" => "Tiny · 快速測試",
        "ggml-base.bin" => "Base · 輕量",
        "ggml-small.bin" | "ggml-model-whisper-small.bin" => "Small · 速度與品質平衡",
        "ggml-medium.bin" => "Medium · 較高品質",
        "ggml-large-v3.bin" => "Large v3 · 高品質",
        "ggml-large-v3-turbo.bin" => "Large v3 Turbo · 高品質、較快速",
        _ => &name,
    };
    format!("{} ({}){size}", friendly, model.source.tag())
}

pub fn human_bytes(n: u64) -> String {
    const GB: f64 = 1_000_000_000.0;
    const MB: f64 = 1_000_000.0;
    if n as f64 >= GB {
        format!("{:.1} GB", n as f64 / GB)
    } else if n as f64 >= MB {
        format!("{:.0} MB", n as f64 / MB)
    } else {
        format!("{n} B")
    }
}

/// Active whisper.cpp path: settings file, then `TOK_WHISPER_MODEL`, then first cache file.
///
/// MacWhisper small/tiny is remapped to a production weight in [`models_dir`]
/// when one exists, so a leftover `ggml-model-whisper-small.bin` path cannot
/// silently load even if that file is still on disk.
pub fn resolve_whisper_model(settings: &LailaisaySettings) -> Option<PathBuf> {
    if let Some(raw) = settings.selected_whisper_model.as_deref() {
        if !raw.is_empty() {
            let remapped = remapped_selected_whisper_model(raw);
            let candidate = remapped.as_deref().unwrap_or(raw);
            let as_is = PathBuf::from(candidate);
            if as_is.is_file() {
                return Some(as_is);
            }
            let in_dir = models_dir().join(candidate);
            if in_dir.is_file() {
                return Some(in_dir);
            }
        }
    }
    if let Ok(env) = std::env::var("TOK_WHISPER_MODEL") {
        let p = PathBuf::from(env);
        if p.is_file() {
            return Some(p);
        }
    }
    list_usable_models().into_iter().next().map(|m| m.path)
}

pub fn tiny_model_path() -> PathBuf {
    models_dir().join("ggml-tiny.bin")
}

pub fn download_whisper_tiny() -> Result<PathBuf, String> {
    download_catalog_model("tiny")
}

/// Download a catalog ggml into [`models_dir`]. Does not commit the file.
pub fn download_catalog_model(id: &str) -> Result<PathBuf, String> {
    let entry = catalog_by_id(id).ok_or_else(|| format!("unknown catalog model {id:?}"))?;
    let dest = catalog_dest(entry);
    if file_looks_complete(&dest, entry.min_bytes) {
        return Ok(dest);
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    if entry.id == "tiny" {
        if let Some(script) = find_download_script() {
            let status = Command::new("bash")
                .arg(&script)
                .env("TOK_MODELS_DIR", models_dir())
                .status()
                .map_err(|e| e.to_string())?;
            if status.success() && dest.exists() {
                return Ok(dest);
            }
        }
    }
    let url = catalog_url(entry);
    let tmp = dest.with_extension("bin.partial");
    let _ = std::fs::remove_file(&tmp);
    let status = Command::new("curl")
        .args(["-fL", "--retry", "3", "--retry-delay", "2", "-o"])
        .arg(&tmp)
        .arg(&url)
        .status()
        .map_err(|e| {
            format!("curl failed ({e}); install curl or set TOK_WHISPER_MODEL_BASE_URL")
        })?;
    if !status.success() {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("curl exited {status} for {}", entry.filename));
    }
    let len = std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
    if len < entry.min_bytes {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!(
            "download of {} looks truncated ({} bytes, expected at least {})",
            entry.filename, len, entry.min_bytes
        ));
    }
    std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}

fn find_download_script() -> Option<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let from_crate = manifest.join("../../scripts/download-whisper-tiny.sh");
    if from_crate.exists() {
        return Some(from_crate);
    }
    let mut dir = std::env::current_dir().ok()?;
    for _ in 0..8 {
        for rel in [
            "scripts/download-whisper-tiny.sh",
            "rust/scripts/download-whisper-tiny.sh",
        ] {
            let p = dir.join(rel);
            if p.exists() {
                return Some(p);
            }
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn restore_env(key: &str, prev: Option<std::ffi::OsString>) {
        match prev {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }

    #[test]
    fn catalog_covers_real_sizes_including_turbo() {
        let _g = ENV_LOCK.lock().unwrap();
        let previous = std::env::var_os("TOK_WHISPER_MODEL_BASE_URL");
        std::env::remove_var("TOK_WHISPER_MODEL_BASE_URL");
        let ids: Vec<_> = CATALOG.iter().map(|e| e.id).collect();
        assert!(ids.contains(&"tiny"));
        assert!(ids.contains(&"base"));
        assert!(ids.contains(&"small"));
        assert!(ids.contains(&"medium"));
        assert!(ids.contains(&"large-v3"));
        assert!(ids.contains(&"large-v3-turbo"));
        let large = catalog_by_id("large-v3").unwrap();
        assert!(catalog_url(large).contains("ggml-large-v3.bin"));
        let url = catalog_url(large);
        restore_env("TOK_WHISPER_MODEL_BASE_URL", previous);
        assert!(url.contains("huggingface.co"));
    }

    #[test]
    fn unknown_catalog_id_errors() {
        let err = download_catalog_model("not-a-model").unwrap_err();
        assert!(err.contains("unknown"), "{err}");
    }

    #[test]
    fn lists_ggml_and_gguf_only() {
        let dir = std::env::temp_dir().join(format!("lailaisay-models-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("ggml-tiny.bin"), b"x").unwrap();
        std::fs::write(dir.join("model.gguf"), b"x").unwrap();
        std::fs::write(dir.join("notes.txt"), b"x").unwrap();
        let found = list_whisper_models(&dir);
        assert_eq!(found.len(), 2);
        assert!(found.iter().any(|p| p.ends_with("ggml-tiny.bin")));
        assert!(found.iter().any(|p| p.ends_with("model.gguf")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_prefers_settings_path() {
        let dir = std::env::temp_dir().join(format!("lailaisay-resolve-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let model = dir.join("picked.bin");
        std::fs::write(&model, b"x").unwrap();
        let mut s = LailaisaySettings::default();
        s.selected_whisper_model = Some(model.to_string_lossy().into());
        assert_eq!(resolve_whisper_model(&s), Some(model));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_remaps_macwhisper_small_to_turbo() {
        let _g = ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "lailaisay-resolve-mw-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let tok = root.join("tok");
        let mw = root.join("MacWhisper").join("models");
        let _ = std::fs::create_dir_all(&tok);
        let _ = std::fs::create_dir_all(&mw);
        let turbo = tok.join("ggml-large-v3-turbo.bin");
        let tiny = tok.join("ggml-tiny.bin");
        let small = mw.join("ggml-model-whisper-small.bin");
        std::fs::write(&turbo, b"turbo").unwrap();
        std::fs::write(&tiny, b"tiny").unwrap();
        std::fs::write(&small, b"small").unwrap();

        let prev_tok = std::env::var_os("TOK_MODELS_DIR");
        std::env::set_var("TOK_MODELS_DIR", &tok);

        let mut s = LailaisaySettings::default();
        s.selected_whisper_model = Some(small.to_string_lossy().into());
        assert_eq!(resolve_whisper_model(&s), Some(turbo.clone()));

        s.selected_whisper_model = Some(tiny.to_string_lossy().into());
        assert_eq!(
            resolve_whisper_model(&s),
            Some(tiny),
            "explicit lailaisay tiny must not be remapped"
        );

        restore_env("TOK_MODELS_DIR", prev_tok);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn available_models_includes_env_file() {
        let _g = ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("lailaisay-avail-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let extra = dir.join("env-model.gguf");
        std::fs::write(&extra, b"x").unwrap();
        let prev = std::env::var_os("TOK_WHISPER_MODEL");
        std::env::set_var("TOK_WHISPER_MODEL", &extra);
        let listed = list_available_models();
        assert!(
            listed.iter().any(|p| p == &extra),
            "expected {extra:?} in {listed:?}"
        );
        restore_env("TOK_WHISPER_MODEL", prev);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scans_macwhisper_and_dedupes() {
        let _g = ENV_LOCK.lock().unwrap();
        let root = std::env::temp_dir().join(format!("lailaisay-scan-{}", std::process::id()));
        let mw = root.join("macwhisper");
        let tok = root.join("tok");
        let _ = std::fs::create_dir_all(&mw);
        let _ = std::fs::create_dir_all(&tok);
        let small = mw.join("ggml-model-whisper-small.bin");
        std::fs::write(&small, vec![0u8; 64]).unwrap();
        let prev_mw = std::env::var_os("TOK_MACWHISPER_MODELS_DIR");
        let prev_tok = std::env::var_os("TOK_MODELS_DIR");
        std::env::set_var("TOK_MACWHISPER_MODELS_DIR", &mw);
        std::env::set_var("TOK_MODELS_DIR", &tok);

        let found = list_usable_models();
        assert!(
            found
                .iter()
                .any(|m| m.path == small && m.source == ModelSource::MacWhisper),
            "{found:?}"
        );
        let label = friendly_model_label(
            found
                .iter()
                .find(|m| m.path == small)
                .expect("macwhisper small"),
        );
        assert!(label.contains("MacWhisper"), "{label}");
        assert!(label.contains("Small · 速度與品質平衡"), "{label}");

        restore_env("TOK_MACWHISPER_MODELS_DIR", prev_mw);
        restore_env("TOK_MODELS_DIR", prev_tok);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn catalog_url_honors_base_override() {
        let _g = ENV_LOCK.lock().unwrap();
        let prev = std::env::var_os("TOK_WHISPER_MODEL_BASE_URL");
        std::env::set_var("TOK_WHISPER_MODEL_BASE_URL", "https://example.test/models");
        let url = catalog_url(catalog_by_id("small").unwrap());
        assert_eq!(url, "https://example.test/models/ggml-small.bin");
        restore_env("TOK_WHISPER_MODEL_BASE_URL", prev);
    }
}
