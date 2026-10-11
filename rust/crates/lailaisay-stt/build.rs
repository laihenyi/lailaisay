//! Forward the transcribe.cpp shared-library directory to dependent binaries.
//!
//! Cargo only hands `DEP_*` metadata to the direct dependent of a crate with
//! `links`, so `lailaisay-app` / `lailaisay-cli` read `DEP_LAILAISAY_STT_RUNTIME_DIR`
//! from here (see their build.rs) to add the rpath.
fn main() {
    println!("cargo:rerun-if-env-changed=DEP_TRANSCRIBE_CPP_RUNTIME_DIR");
    if let Some(dir) = std::env::var_os("DEP_TRANSCRIBE_CPP_RUNTIME_DIR") {
        println!("cargo:runtime_dir={}", std::path::Path::new(&dir).display());
    }
}
