# Build isolated GPU / CPU workers. Different target dirs prevent CMake cache leakage.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
$previousInclude = $env:CMAKE_PROJECT_INCLUDE
try {
    $env:CMAKE_PROJECT_INCLUDE = Join-Path $root 'cmake/windows-portable.cmake'
    cargo build --locked --release -p lailaisay-app -p lailaisay-stt --features lailaisay-app/mic,lailaisay-app/whisper --bin lailaisay-app --bin lailaisay-whisper-worker
    if ($LASTEXITCODE -ne 0) { throw 'CPU app/worker build failed' }
    Copy-Item target/release/lailaisay-whisper-worker.exe target/release/lailaisay-whisper-cpu.exe -Force

    $env:CMAKE_PROJECT_INCLUDE = Join-Path $root 'cmake/windows-avx2.cmake'
    cargo build --locked --release --target-dir target/windows-avx2 -p lailaisay-stt --features whisper --bin lailaisay-whisper-worker
    if ($LASTEXITCODE -ne 0) { throw 'AVX2 worker build failed' }
    Copy-Item target/windows-avx2/release/lailaisay-whisper-worker.exe target/release/lailaisay-whisper-avx2.exe -Force

    $env:CMAKE_PROJECT_INCLUDE = Join-Path $root 'cmake/windows-portable.cmake'
    cargo build --locked --release --target-dir target/windows-vulkan -p lailaisay-stt --features vulkan --bin lailaisay-whisper-worker
    if ($LASTEXITCODE -ne 0) { throw 'Vulkan worker build failed' }
    Copy-Item target/windows-vulkan/release/lailaisay-whisper-worker.exe target/release/lailaisay-whisper-vulkan.exe -Force
} finally {
    $env:CMAKE_PROJECT_INCLUDE = $previousInclude
    Pop-Location
}
