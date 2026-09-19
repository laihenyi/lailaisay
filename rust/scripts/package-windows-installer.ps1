# Wrap the already-tested complete app folder in an Inno Setup per-user installer.
[CmdletBinding()]
param(
    [string]$PayloadDir = '',
    [string]$Version = '',
    [string]$IsccPath = '',
    [string]$VcRedistDir = '',
    [string]$OutDir = ''
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if (-not $PayloadDir) { $PayloadDir = Join-Path $root 'dist/windows/lailaisay' }
$PayloadDir = (Resolve-Path $PayloadDir).Path
if (-not $Version) {
    $manifest = Get-Content (Join-Path $root 'Cargo.toml') -Raw
    $match = [regex]::Match($manifest, '(?m)^version\s*=\s*"(\d+\.\d+\.\d+)"')
    if (-not $match.Success) { throw 'Workspace release version missing' }
    $build = if ($env:GITHUB_RUN_NUMBER) { [int]$env:GITHUB_RUN_NUMBER } else { 0 }
    $Version = "$($match.Groups[1].Value).$build"
}
if ($Version -notmatch '^\d+\.\d+\.\d+\.\d+$' -or
    @($Version.Split('.') | Where-Object { [int64]$_ -gt 65535 }).Count -gt 0) {
    throw 'Installer version must contain four integers in 0..65535'
}
foreach ($file in @('lailaisay-app.exe', 'lailaisay-whisper-cpu.exe', 'lailaisay-whisper-avx2.exe', 'lailaisay-whisper-vulkan.exe', 'lailaisay-diagnostics.cmd', 'FONT-LICENSE.txt', 'LICENSE')) {
    if (-not (Test-Path (Join-Path $PayloadDir $file))) { throw "Incomplete payload: $file" }
}
if (-not $IsccPath) {
    $IsccPath = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6/ISCC.exe'
}
if (-not (Test-Path $IsccPath)) { throw 'Install Inno Setup 6.5+ or pass -IsccPath' }
$chinese = Join-Path $root 'windows/Languages/ChineseTraditional.isl'
if (-not (Test-Path $chinese)) { throw 'Bundled Traditional Chinese language file missing' }

# Application-local MSVC runtime: no elevated prerequisite installer or SDK needed.
# Only take redistributable DLLs, never arbitrary files from System32.
if (-not $VcRedistDir) {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    $vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if ($LASTEXITCODE -ne 0 -or -not $vs) { throw 'Visual C++ redistributable directory not found' }
    $versions = Get-ChildItem (Join-Path $vs 'VC/Redist/MSVC') -Directory |
        Where-Object { $_.Name -match '^\d+\.\d+\.\d+$' } |
        Sort-Object { [version]$_.Name } -Descending
    foreach ($candidate in $versions) {
        $crt = Get-ChildItem (Join-Path $candidate.FullName 'x64') -Directory -Filter 'Microsoft.VC*.CRT' | Select-Object -First 1
        if ($crt) { $VcRedistDir = $crt.FullName; break }
    }
}
if (-not $VcRedistDir) { throw 'x64 VC runtime not found; pass -VcRedistDir from the matching build tools' }
foreach ($dll in @('msvcp140.dll', 'vcruntime140.dll', 'vcruntime140_1.dll')) {
    if (-not (Test-Path (Join-Path $VcRedistDir $dll))) { throw "Missing redistributable: $dll" }
}

if (-not $OutDir) { $OutDir = Join-Path $root 'dist/windows/installer' }
$output = $OutDir
$staging = Join-Path $output 'payload'
New-Item -ItemType Directory -Force $output | Out-Null
if (Test-Path $staging) { Remove-Item $staging -Recurse -Force }
New-Item -ItemType Directory $staging | Out-Null
Copy-Item (Join-Path $PayloadDir '*') $staging -Recurse
Copy-Item (Join-Path $VcRedistDir '*.dll') $staging
@'
Microsoft Visual C++ runtime files are redistributed from the Visual Studio
VC/Redist/MSVC/x64 CRT directory. Copyright Microsoft Corporation.
These files are subject to Microsoft's software license terms, not lailaisay's MIT license.
https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files
App-local runtime updates are delivered with subsequent lailaisay installer releases.
lailaisay's bundled Noto font license is in FONT-LICENSE.txt.
'@ | Set-Content (Join-Path $staging 'THIRD-PARTY-NOTICES.txt') -Encoding UTF8
@{
    version = $Version
    commit = $env:GITHUB_SHA
    runtimeSourceVersion = (Get-Item (Join-Path $VcRedistDir 'vcruntime140.dll')).VersionInfo.FileVersion
} | ConvertTo-Json | Set-Content (Join-Path $staging 'build-info.json') -Encoding UTF8
& $IsccPath "/DAppVersion=$Version" "/DPayloadDir=$staging" "/DInstallerOutputDir=$output" (Join-Path $root 'windows/lailaisay.iss')
if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed ($LASTEXITCODE)" }
$installer = Join-Path $output "lailaisay-Setup-$Version-x64.exe"
if (-not (Test-Path $installer)) { throw 'Installer output missing' }
$hash = (Get-FileHash $installer -Algorithm SHA256).Hash.ToLowerInvariant()
"$hash  $(Split-Path $installer -Leaf)" | Set-Content "$installer.sha256" -Encoding ASCII
Write-Host "Installer: $installer"
