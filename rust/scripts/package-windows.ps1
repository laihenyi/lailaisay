# Portable lailaisay folder for Windows (lailaisay-app.exe + README). Not an MSI.
#
#   cd rust
#   powershell -File scripts/package-windows.ps1
#   powershell -File scripts/package-windows.ps1 -Features mic,whisper -Zip
#   powershell -File scripts/package-windows.ps1 -SkipBuild -BinPath target\release\lailaisay-app.exe
#
# Layout-only (CI / docs check, no real exe required):
#   powershell -File scripts/package-windows.ps1 -LayoutOnly
[CmdletBinding()]
param(
    [string]$Features = "mic,whisper",
    [switch]$SkipBuild,
    [switch]$LayoutOnly,
    [switch]$Zip,
    [string]$BinPath = "",
    [string]$OutDir = ""
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
if (-not $OutDir) {
    $OutDir = Join-Path $Root "dist\windows\lailaisay"
}

function Write-AppReadme {
    param([string]$DestDir, [string]$BinName)
    $readme = @"
lailaisay (Windows)
=============

Hold-to-talk voice input. Notification-area tray: 開啟設定 / 結束 lailaisay.
Closing Settings hides to the tray when the icon is present.

Run:

  .\$BinName

Full local STT: automatic Vulkan GPU detection, then AVX2 CPU, then compatible CPU.
The GPU worker is isolated: missing drivers or native GPU failures retry the same audio on CPU.

  ./scripts/build-windows.ps1

Default hotkey: Win+Shift+Space (⌘⇧Space in settings). Speak-to-Edit:
Alt+Shift+Space. Change the chord in Settings if Win+Shift fights another app.

Permissions:
  Settings → Privacy & security → Microphone → allow lailaisay (or this terminal).
  WH_KEYBOARD_LL does not show a TCC prompt. If the hook fails, allow lailaisay
  in antivirus / ransomware protection and relaunch.

See WINDOWS.md in the repo for paste limits, CI, and packaging notes.
No API keys or Ollama required. GPU use requires a compatible Vulkan driver.
Users do not need the Vulkan SDK. If GPU fails, update the display driver and
restart lailaisay to retry. For slower CPUs, select base or tiny in Settings.
Keep all lailaisay-whisper-*.exe files beside lailaisay-app.exe.
"@
    Set-Content -Path (Join-Path $DestDir "README.txt") -Value $readme -Encoding UTF8
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
Get-ChildItem -Path $OutDir -Force | Remove-Item -Recurse -Force

if ($LayoutOnly) {
    $stub = Join-Path $OutDir "lailaisay-app.exe.txt"
    Set-Content -Path $stub -Value "lailaisay Windows layout stub. Build on Windows without -LayoutOnly." -Encoding UTF8
    Write-AppReadme -DestDir $OutDir -BinName "lailaisay-app.exe"
    $winMd = Join-Path $Root "WINDOWS.md"
    if (Test-Path $winMd) {
        Copy-Item $winMd (Join-Path $OutDir "WINDOWS.md")
    }
    Write-Host "Laid out $OutDir (layout-only stub)."
    if ($Zip) {
        $zipPath = Join-Path (Split-Path $OutDir) "lailaisay-windows.zip"
        if (Test-Path $zipPath) { Remove-Item $zipPath -Force }
        Compress-Archive -Path $OutDir -DestinationPath $zipPath
        Write-Host "Wrote $zipPath"
    }
    exit 0
}

$bin = $BinPath
if (-not $bin) {
    $rel = Join-Path $Root "target\release\lailaisay-app.exe"
    $debug = Join-Path $Root "target\debug\lailaisay-app.exe"
    if ($SkipBuild) {
        if (Test-Path $rel) { $bin = $rel }
        elseif (Test-Path $debug) { $bin = $debug }
        else {
            throw "lailaisay-app.exe not found. Pass -BinPath or build first."
        }
    } else {
        Push-Location $Root
        try {
            if ($Features -match 'whisper') {
                & (Join-Path $PSScriptRoot "build-windows.ps1")
            } else {
            $cargoArgs = @("build", "--release", "-p", "lailaisay-app")
            if ($Features) {
                $cargoArgs += @("--features", $Features)
            }
            Write-Host "cargo $($cargoArgs -join ' ')"
            & cargo @cargoArgs
            if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }
            }
        } finally {
            Pop-Location
        }
        $bin = $rel
    }
}

if (-not (Test-Path $bin)) {
    throw "lailaisay-app binary not found: $bin"
}

$exeName = "lailaisay-app.exe"
Copy-Item $bin (Join-Path $OutDir $exeName)
if ($Features -match 'whisper') {
    foreach ($worker in @('lailaisay-whisper-cpu.exe', 'lailaisay-whisper-avx2.exe', 'lailaisay-whisper-vulkan.exe')) {
        $source = Join-Path (Split-Path $bin) $worker
        if (-not (Test-Path $source)) { throw "Missing worker: $source. Run scripts/build-windows.ps1 first." }
        Copy-Item $source (Join-Path $OutDir $worker)
    }
}
$diagnostics = @'
@echo off
cd /d "%~dp0"
powershell.exe -NoProfile -Command "$ErrorActionPreference='Stop'; $app=Join-Path (Get-Location) 'lailaisay-app.exe'; if (Get-Process -Name lailaisay-app -ErrorAction SilentlyContinue) { Write-Host 'Close all lailaisay processes before diagnostics.'; exit 2 }; $log=Join-Path $env:TEMP 'lailaisay-diagnostic.log'; $prefix=Join-Path $env:TEMP ('lailaisay-'+[DateTime]::Now.ToString('yyyyMMdd-HHmmss')); $started=Get-Date -Format o; $p=Start-Process -FilePath $app -PassThru -RedirectStandardOutput ($prefix+'.out') -RedirectStandardError ($prefix+'.err'); Write-Host ('lailaisay PID: '+$p.Id+' - close lailaisay to finish'); Write-Host ('Live log: '+$prefix+'.err'); $p.WaitForExit(); $p.Refresh(); $code=$p.ExitCode; if ($null -eq $code) { throw 'Process exit code unavailable' }; $hex='{0:X8}' -f [BitConverter]::ToUInt32([BitConverter]::GetBytes([int]$code),0); @('Started: '+$started; 'Finished: '+(Get-Date -Format o); 'Exe: '+$app; 'PID: '+$p.Id; 'SHA256: '+(Get-FileHash $app -Algorithm SHA256).Hash; 'CPU: '+(Get-ItemProperty 'HKLM:\HARDWARE\DESCRIPTION\System\CentralProcessor\0').ProcessorNameString; '--- stderr ---'; Get-Content ($prefix+'.err'); '--- stdout ---'; Get-Content ($prefix+'.out'); 'Exit code: '+$code+' (0x'+$hex+')') | Set-Content $log -Encoding UTF8; Write-Host ('Exit code: '+$code+' (0x'+$hex+')'); Write-Host ('Log: '+$log)"
pause
'@
Set-Content -Path (Join-Path $OutDir "lailaisay-diagnostics.cmd") -Value $diagnostics -Encoding ASCII

Copy-Item (Join-Path $Root "crates\lailaisay-app\assets\fonts\OFL.txt") (Join-Path $OutDir "FONT-LICENSE.txt")
Write-AppReadme -DestDir $OutDir -BinName $exeName
$winMd = Join-Path $Root "WINDOWS.md"
if (Test-Path $winMd) {
    Copy-Item $winMd (Join-Path $OutDir "WINDOWS.md")
}
$license = Join-Path (Split-Path $Root) "LICENSE"
if (Test-Path $license) {
    Copy-Item $license (Join-Path $OutDir "LICENSE")
}

Write-Host "Copied $bin → $(Join-Path $OutDir $exeName)"
Write-Host "Portable folder: $OutDir"

if ($Zip) {
    $zipPath = Join-Path (Split-Path $OutDir) "lailaisay-windows.zip"
    if (Test-Path $zipPath) { Remove-Item $zipPath -Force }
    Compress-Archive -Path $OutDir -DestinationPath $zipPath
    Write-Host "Wrote $zipPath"
}

Write-Host ""
Write-Host "Run: $(Join-Path $OutDir $exeName)"
Write-Host "Tray: 開啟設定 / 結束 lailaisay. Close Settings hides to the tray."
Write-Host "Hold Win+Shift+Space to dictate (SendInput Ctrl+V)."
