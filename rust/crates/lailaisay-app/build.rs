//! rpath for the transcribe.cpp shared libraries (`qwen` feature, macOS).
//!
//! Packaged app: `Contents/Frameworks` (`package-macos-app.sh` copies the dylibs there).
//! `cargo run`: the build output directory, so no environment variables are needed.
fn main() {
    println!("cargo:rerun-if-env-changed=DEP_LAILAISAY_STT_RUNTIME_DIR");
    if let Some(dir) = std::env::var_os("DEP_LAILAISAY_STT_RUNTIME_DIR") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", std::path::Path::new(&dir).display());
    }
}
