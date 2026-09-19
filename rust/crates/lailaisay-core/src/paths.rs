use std::path::{Path, PathBuf};

/// Rust `lailaisay.app` / Application Support folder (`~/Library/Application Support/com.yikai.lailaisay`).
pub const MACOS_BUNDLE_ID: &str = "com.yikai.lailaisay";

/// Previous Rust support folder. First launch copies ggml/gguf out of `…/models` when the new cache is empty.
pub const LEGACY_MACOS_SUPPORT_NAME: &str = "xyz.2qs.Tok";

/// Windows Roaming AppData folder (`%APPDATA%\Tok`). Product-name equivalent of `com.yikai.lailaisay`.
pub const WINDOWS_APP_DIR: &str = "Tok";

/// Settings JSON. Preserve the existing macOS path (`~/Documents/hex_settings.json`).
pub fn settings_path() -> PathBuf {
    if let Ok(p) = std::env::var("TOK_CONFIG") {
        return PathBuf::from(p);
    }
    #[cfg(target_os = "macos")]
    {
        documents_dir().join("hex_settings.json")
    }
    #[cfg(target_os = "windows")]
    {
        windows_app_data_dir().join("settings.json")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        config_dir().join("settings.json")
    }
}

pub fn custom_words_path() -> PathBuf {
    if let Ok(p) = std::env::var("TOK_CUSTOM_WORDS") {
        return PathBuf::from(p);
    }
    #[cfg(target_os = "macos")]
    {
        documents_dir().join("hex_custom_words.json")
    }
    #[cfg(target_os = "windows")]
    {
        windows_app_data_dir().join("custom_words.json")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        config_dir().join("custom_words.json")
    }
}

pub fn correction_history_path() -> PathBuf {
    if let Ok(p) = std::env::var("TOK_CORRECTION_HISTORY") {
        return PathBuf::from(p);
    }
    #[cfg(target_os = "macos")]
    {
        documents_dir().join("correction_history.json")
    }
    #[cfg(target_os = "windows")]
    {
        windows_app_data_dir().join("correction_history.json")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        config_dir().join("correction_history.json")
    }
}

pub fn phonetic_glossary_path() -> PathBuf {
    if let Ok(p) = std::env::var("TOK_GLOSSARY") {
        return PathBuf::from(p);
    }
    #[cfg(target_os = "macos")]
    {
        documents_dir().join("hex_phonetic_glossary.json")
    }
    #[cfg(target_os = "windows")]
    {
        windows_app_data_dir().join("phonetic_glossary.json")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        config_dir().join("phonetic_glossary.json")
    }
}

pub fn models_dir() -> PathBuf {
    if let Ok(p) = std::env::var("TOK_MODELS_DIR") {
        return PathBuf::from(p);
    }
    let dest = default_models_dir();
    let _ = migrate_legacy_macos_models_into(&dest);
    dest
}

fn default_models_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        macos_app_support_dir().join("models")
    }
    #[cfg(target_os = "windows")]
    {
        windows_app_data_dir().join("models")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("tok")
            .join("models")
    }
}

/// `%APPDATA%\Tok` on Windows (`dirs::data_dir()` is Roaming AppData).
#[cfg(target_os = "windows")]
pub fn windows_app_data_dir() -> PathBuf {
    windows_app_data_dir_from(dirs::data_dir())
}

/// Build the Windows support dir from an optional `dirs::data_dir()` (testable on every OS).
pub fn windows_app_data_dir_from(data_dir: Option<PathBuf>) -> PathBuf {
    data_dir
        .or_else(|| dirs::home_dir().map(|h| h.join("AppData").join("Roaming")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join(WINDOWS_APP_DIR)
}

/// `~/Library/Application Support/com.yikai.lailaisay` on macOS.
#[cfg(target_os = "macos")]
pub fn macos_app_support_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join(MACOS_BUNDLE_ID)
}

#[cfg(target_os = "macos")]
fn macos_legacy_models_dir(identity: &str) -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join(identity)
        .join("models")
}

/// Copy ggml/gguf from the legacy cache when the new dir has none.
/// No-op when `TOK_MODELS_DIR` is set (caller chose a cache).
pub fn migrate_legacy_macos_models() -> u32 {
    if std::env::var("TOK_MODELS_DIR").is_ok() {
        return 0;
    }
    migrate_legacy_macos_models_into(&default_models_dir())
}

fn migrate_legacy_macos_models_into(dest: &Path) -> u32 {
    #[cfg(target_os = "macos")]
    {
        copy_ggml_if_dest_empty(&macos_legacy_models_dir(LEGACY_MACOS_SUPPORT_NAME), dest)
            .unwrap_or(0)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = dest;
        0
    }
}

fn looks_like_whisper_weight(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with(".bin") || n.ends_with(".gguf") || n.ends_with(".ggml")
}

fn dir_has_whisper_weight(dir: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    rd.flatten().any(|e| {
        e.path().is_file()
            && e.file_name()
                .to_str()
                .is_some_and(looks_like_whisper_weight)
    })
}

/// If `selected` is under the legacy cache and the same file exists
/// in the current [`models_dir`], return the new path (in-memory remap).
pub fn remapped_legacy_whisper_model(selected: &str) -> Option<String> {
    if !Path::new(selected)
        .components()
        .any(|part| part.as_os_str() == LEGACY_MACOS_SUPPORT_NAME)
    {
        return None;
    }
    let name = Path::new(selected).file_name()?;
    let dest = models_dir().join(name);
    dest.is_file().then(|| dest.to_string_lossy().into_owned())
}

/// Remap a persisted `selectedWhisperModel` at load/save/resolve.
///
/// 1. Legacy cache → same filename in [`models_dir`] when present.
/// 2. MacWhisper **small** (and MacWhisper **tiny**) → a production model in
///    [`models_dir`]: `ggml-large-v3-turbo.bin`, else `ggml-large-v3.bin`,
///    else the first usable non-tiny/non-small weight.
///
/// MacWhisper small is uniquely named `ggml-model-whisper-small.bin` and is
/// terrible on short digit clips (blank / CC hallucinations). We never silently
/// run it for production STT when a better local model exists.
///
/// Explicit lailaisay-cache tiny (`…/com.yikai.lailaisay/models/ggml-tiny.bin`, or
/// any non-MacWhisper `ggml-tiny.bin`) is left alone so tests can keep using it.
pub fn remapped_selected_whisper_model(selected: &str) -> Option<String> {
    let after_legacy = remapped_legacy_whisper_model(selected);
    let candidate = after_legacy.as_deref().unwrap_or(selected);
    remapped_macwhisper_weak_model(candidate).or(after_legacy)
}

/// Sibling of [`remapped_legacy_whisper_model`]: only MacWhisper weak weights.
///
/// Matches MacWhisper's `ggml-model-whisper-small.bin` / `…-tiny.bin` by
/// filename (those names are unique to MacWhisper), plus any `tiny`/`small`
/// weight whose path sits under a `MacWhisper` directory. Official catalog
/// files in the lailaisay cache (`ggml-tiny.bin`, `ggml-small.bin`) are not
/// remapped unless they were selected via a MacWhisper path.
pub fn remapped_macwhisper_weak_model(selected: &str) -> Option<String> {
    if !is_macwhisper_weak_selected(selected) {
        return None;
    }
    preferred_production_whisper_model()
}

fn is_macwhisper_path(selected: &str) -> bool {
    selected.to_ascii_lowercase().contains("macwhisper")
}

fn whisper_filename_is_tiny_or_small(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("tiny") || n.contains("small")
}

fn is_macwhisper_named_weak_file(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("ggml-model-whisper-small") || n.contains("ggml-model-whisper-tiny")
}

fn is_macwhisper_weak_selected(selected: &str) -> bool {
    let Some(name) = Path::new(selected).file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if is_macwhisper_named_weak_file(name) {
        return true;
    }
    is_macwhisper_path(selected) && whisper_filename_is_tiny_or_small(name)
}

/// Prefer turbo, then large-v3, then any other non-tiny/non-small ggml in
/// [`models_dir`]. `None` when nothing better than MacWhisper small exists.
fn preferred_production_whisper_model() -> Option<String> {
    let dir = models_dir();
    for name in ["ggml-large-v3-turbo.bin", "ggml-large-v3.bin"] {
        let dest = dir.join(name);
        if dest.is_file() {
            return Some(dest.to_string_lossy().into_owned());
        }
    }
    first_usable_non_weak_local_model(&dir)
}

fn first_usable_non_weak_local_model(dir: &Path) -> Option<String> {
    let mut files: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect(),
        Err(_) => return None,
    };
    files.sort();
    files.into_iter().find_map(|p| {
        let name = p.file_name()?.to_str()?;
        if !looks_like_whisper_weight(name) || whisper_filename_is_tiny_or_small(name) {
            return None;
        }
        Some(p.to_string_lossy().into_owned())
    })
}

/// If `dest` has no ggml/gguf, copy weight files from `src` (same filenames).
pub fn copy_ggml_if_dest_empty(src: &Path, dest: &Path) -> std::io::Result<u32> {
    if dir_has_whisper_weight(dest) {
        return Ok(0);
    }
    if !src.is_dir() {
        return Ok(0);
    }
    std::fs::create_dir_all(dest)?;
    let mut copied = 0u32;
    for entry in std::fs::read_dir(src)? {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name() else {
            continue;
        };
        if !name.to_str().is_some_and(looks_like_whisper_weight) {
            continue;
        }
        let target = dest.join(name);
        if target.exists() {
            continue;
        }
        std::fs::copy(&path, &target)?;
        copied += 1;
    }
    Ok(copied)
}

#[cfg(target_os = "macos")]
fn documents_dir() -> PathBuf {
    dirs::document_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Documents")
    })
}

#[cfg(not(target_os = "macos"))]
fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("tok")
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

    fn unique_temp(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "lailaisay-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ))
    }

    #[test]
    fn rust_macos_identity_is_yikai_lailaisay() {
        assert_eq!(MACOS_BUNDLE_ID, "com.yikai.lailaisay");
        assert_eq!(LEGACY_MACOS_SUPPORT_NAME, "xyz.2qs.Tok");
    }

    #[test]
    fn windows_app_dir_preserves_existing_data() {
        assert_eq!(WINDOWS_APP_DIR, "Tok");
        let dir = windows_app_data_dir_from(Some(PathBuf::from(r"C:\Users\x\AppData\Roaming")));
        assert_eq!(
            dir,
            PathBuf::from(r"C:\Users\x\AppData\Roaming").join("Tok")
        );
        assert_eq!(
            dir.join("settings.json").file_name().unwrap(),
            "settings.json"
        );
        assert_eq!(dir.join("models").file_name().unwrap(), "models");
    }

    #[test]
    fn copy_ggml_when_dest_empty_skips_when_dest_already_has_weights() {
        let root = std::env::temp_dir().join(format!(
            "lailaisay-migrate-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let src = root.join("old").join("models");
        let dest = root.join("new").join("models");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("ggml-large-v3-turbo.bin"), b"turbo").unwrap();
        std::fs::write(src.join("readme.txt"), b"ignore").unwrap();

        let n = copy_ggml_if_dest_empty(&src, &dest).unwrap();
        assert_eq!(n, 1);
        assert_eq!(
            std::fs::read(dest.join("ggml-large-v3-turbo.bin")).unwrap(),
            b"turbo"
        );
        assert!(!dest.join("readme.txt").exists());

        std::fs::write(src.join("ggml-tiny.bin"), b"tiny").unwrap();
        let n2 = copy_ggml_if_dest_empty(&src, &dest).unwrap();
        assert_eq!(n2, 0, "must not copy again once dest has a ggml");
        assert!(!dest.join("ggml-tiny.bin").exists());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn remaps_legacy_whisper_path_when_new_cache_has_file() {
        let _g = ENV_LOCK.lock().unwrap();
        let dest = unique_temp("remap-legacy");
        std::fs::create_dir_all(&dest).unwrap();
        let file = dest.join("ggml-large-v3-turbo.bin");
        std::fs::write(&file, b"x").unwrap();
        let prev = std::env::var_os("TOK_MODELS_DIR");
        std::env::set_var("TOK_MODELS_DIR", &dest);
        for identity in [LEGACY_MACOS_SUPPORT_NAME] {
            let old = format!(
                "/Users/x/Library/Application Support/{identity}/models/ggml-large-v3-turbo.bin"
            );
            let next = remapped_legacy_whisper_model(&old).expect("remap");
            assert_eq!(Path::new(&next), file.as_path());
            let missing =
                format!("/Users/x/Library/Application Support/{identity}/models/missing.bin");
            assert!(remapped_legacy_whisper_model(&missing).is_none());
        }
        assert!(remapped_legacy_whisper_model("/tmp/other.bin").is_none());
        let unrelated = format!("/tmp/{LEGACY_MACOS_SUPPORT_NAME}-other/ggml-large-v3-turbo.bin");
        assert!(remapped_legacy_whisper_model(&unrelated).is_none());
        restore_env("TOK_MODELS_DIR", prev);
        let _ = std::fs::remove_dir_all(&dest);
    }

    #[test]
    fn remaps_macwhisper_small_to_turbo_when_turbo_exists() {
        let _g = ENV_LOCK.lock().unwrap();
        let root = unique_temp("remap-mw-small");
        let tok = root.join("tok");
        let mw = root
            .join("Library")
            .join("Application Support")
            .join("MacWhisper")
            .join("models");
        std::fs::create_dir_all(&tok).unwrap();
        std::fs::create_dir_all(&mw).unwrap();
        std::fs::write(tok.join("ggml-large-v3-turbo.bin"), b"turbo").unwrap();
        std::fs::write(tok.join("ggml-tiny.bin"), b"tiny").unwrap();
        let small = mw.join("ggml-model-whisper-small.bin");
        std::fs::write(&small, b"small").unwrap();

        let prev = std::env::var_os("TOK_MODELS_DIR");
        std::env::set_var("TOK_MODELS_DIR", &tok);

        let next = remapped_macwhisper_weak_model(&small.to_string_lossy()).expect("remap");
        assert!(
            next.ends_with("ggml-large-v3-turbo.bin"),
            "expected turbo, got {next}"
        );
        assert!(!next.to_ascii_lowercase().contains("macwhisper"), "{next}");

        let via_selected =
            remapped_selected_whisper_model(&small.to_string_lossy()).expect("selected remap");
        assert_eq!(via_selected, next);

        restore_env("TOK_MODELS_DIR", prev);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn remaps_macwhisper_tiny_path_only() {
        let _g = ENV_LOCK.lock().unwrap();
        let root = unique_temp("remap-mw-tiny");
        let tok = root.join("tok");
        let mw = root.join("MacWhisper").join("models");
        std::fs::create_dir_all(&tok).unwrap();
        std::fs::create_dir_all(&mw).unwrap();
        std::fs::write(tok.join("ggml-large-v3-turbo.bin"), b"turbo").unwrap();
        let mw_tiny = mw.join("ggml-model-whisper-tiny.bin");
        std::fs::write(&mw_tiny, b"mw-tiny").unwrap();
        let local_tiny = tok.join("ggml-tiny.bin");
        std::fs::write(&local_tiny, b"local-tiny").unwrap();

        let prev = std::env::var_os("TOK_MODELS_DIR");
        std::env::set_var("TOK_MODELS_DIR", &tok);

        let remapped = remapped_macwhisper_weak_model(&mw_tiny.to_string_lossy()).expect("mw tiny");
        assert!(remapped.ends_with("ggml-large-v3-turbo.bin"), "{remapped}");

        assert!(
            remapped_macwhisper_weak_model(&local_tiny.to_string_lossy()).is_none(),
            "explicit lailaisay ggml-tiny.bin must stay for tests"
        );
        assert!(
            remapped_selected_whisper_model(&local_tiny.to_string_lossy()).is_none(),
            "unified remap must not steal com.yikai.lailaisay tiny"
        );

        restore_env("TOK_MODELS_DIR", prev);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn macwhisper_small_falls_back_to_large_v3_then_other_non_weak() {
        let _g = ENV_LOCK.lock().unwrap();
        let root = unique_temp("remap-mw-fallback");
        let tok = root.join("tok");
        std::fs::create_dir_all(&tok).unwrap();
        let small = format!(
            "/Users/x/Library/Application Support/MacWhisper/models/ggml-model-whisper-small.bin"
        );

        let prev = std::env::var_os("TOK_MODELS_DIR");
        std::env::set_var("TOK_MODELS_DIR", &tok);

        std::fs::write(tok.join("ggml-tiny.bin"), b"tiny").unwrap();
        std::fs::write(tok.join("ggml-small.bin"), b"small").unwrap();
        assert!(
            remapped_macwhisper_weak_model(&small).is_none(),
            "must not remap onto tiny/small"
        );

        std::fs::write(tok.join("ggml-medium.bin"), b"medium").unwrap();
        let medium = remapped_macwhisper_weak_model(&small).expect("medium fallback");
        assert!(medium.ends_with("ggml-medium.bin"), "{medium}");

        std::fs::write(tok.join("ggml-large-v3.bin"), b"large").unwrap();
        let large = remapped_macwhisper_weak_model(&small).expect("large-v3");
        assert!(large.ends_with("ggml-large-v3.bin"), "{large}");

        std::fs::write(tok.join("ggml-large-v3-turbo.bin"), b"turbo").unwrap();
        let turbo = remapped_macwhisper_weak_model(&small).expect("turbo wins");
        assert!(turbo.ends_with("ggml-large-v3-turbo.bin"), "{turbo}");

        restore_env("TOK_MODELS_DIR", prev);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn does_not_remap_unrelated_or_catalog_small_outside_macwhisper() {
        let _g = ENV_LOCK.lock().unwrap();
        let dest = unique_temp("remap-leave");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("ggml-large-v3-turbo.bin"), b"turbo").unwrap();
        let catalog_small = dest.join("ggml-small.bin");
        std::fs::write(&catalog_small, b"small").unwrap();

        let prev = std::env::var_os("TOK_MODELS_DIR");
        std::env::set_var("TOK_MODELS_DIR", &dest);

        assert!(remapped_macwhisper_weak_model("/tmp/other.bin").is_none());
        assert!(
            remapped_macwhisper_weak_model(&catalog_small.to_string_lossy()).is_none(),
            "intentional ggml-small.bin in the lailaisay cache stays"
        );

        restore_env("TOK_MODELS_DIR", prev);
        let _ = std::fs::remove_dir_all(&dest);
    }
}
