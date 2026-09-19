# Destructive only to the disposable CI user's test installation. Never run on a user's PC.
[CmdletBinding()]
param([string]$Installer = '')
$ErrorActionPreference = 'Stop'
if ($env:CI -ne 'true') { throw 'Installer smoke test is restricted to disposable CI runners' }
$root = Split-Path -Parent $PSScriptRoot
$output = Join-Path $root 'dist/windows/installer'
if (-not $Installer) {
    $Installer = (Get-ChildItem $output -Filter 'lailaisay-Setup-*-x64.exe' | Select-Object -First 1).FullName
}
if (-not $Installer) { throw 'Installer not found' }
$testRoot = Join-Path $env:RUNNER_TEMP 'lailaisay Installer Smoke'
$installDir = Join-Path $testRoot '安裝目錄'
$registryPath = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{A15E83E4-9F3F-4FA4-9E91-92C42DE6F179}_is1'
$dataDir = Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'Tok'
$programs = Join-Path ([Environment]::GetFolderPath('Programs')) 'lailaisay'
$oldPrograms = Join-Path ([Environment]::GetFolderPath('Programs')) 'Tok'
$oldDesktopLink = Join-Path ([Environment]::GetFolderPath('Desktop')) 'Tok.lnk'
$desktopLink = Join-Path ([Environment]::GetFolderPath('Desktop')) 'lailaisay.lnk'
if ((Test-Path $registryPath) -or (Test-Path $dataDir) -or (Test-Path $programs) -or (Test-Path $desktopLink) -or (Test-Path $oldPrograms) -or (Test-Path $oldDesktopLink)) {
    throw 'Refusing to test over an existing lailaisay installation or user data'
}
New-Item -ItemType Directory -Force $testRoot | Out-Null
New-Item -ItemType Directory -Force (Join-Path $dataDir 'models') | Out-Null
'{"outputLanguage":"en","preferTraditionalChinese":false}' | Set-Content (Join-Path $dataDir 'settings.json') -Encoding UTF8
'preserve user model' | Set-Content (Join-Path $dataDir 'models/test-model.bin')
$before = Get-ChildItem $dataDir -File -Recurse | Get-FileHash -Algorithm SHA256

function Invoke-TestProcess {
    param([string]$Path, [string[]]$Arguments, [string]$LogName, [int]$TimeoutSeconds = 180)
    if ($LogName -notlike '*inference') {
        $Arguments += "/LOG=`"$(Join-Path $testRoot "$LogName.setup.log")`""
    }
    $p = Start-Process -FilePath $Path -ArgumentList $Arguments -PassThru -RedirectStandardOutput (Join-Path $testRoot "$LogName.out") -RedirectStandardError (Join-Path $testRoot "$LogName.err")
    if (-not $p.WaitForExit($TimeoutSeconds * 1000)) {
        $p.Kill(); $p.WaitForExit()
        throw "Process timed out: $LogName"
    }
    $p.Refresh()
    if ($null -eq $p.ExitCode) { throw "Missing exit code: $LogName" }
    return $p.ExitCode
}
function Get-InstalledUninstaller {
    # Inno may choose unins001.exe after an immediate reinstall while its prior
    # self-deleting uninstaller is still exiting. The registry is authoritative.
    $command = [string](Get-ItemProperty $registryPath).UninstallString
    $path = $command.Trim('"')
    if (-not $path.EndsWith('.exe') -or -not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Registered uninstaller is missing or invalid: $command"
    }
    Write-Host "Registered uninstaller: $path"
    return $path
}
function Assert-UserDataPreserved {
    foreach ($entry in $before) {
        if (-not (Test-Path $entry.Path) -or (Get-FileHash $entry.Path).Hash -ne $entry.Hash) {
            throw "User data changed: $($entry.Path)"
        }
    }
}
$commonArgs = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-', '/LANG=chinesetraditional', "/DIR=`"$installDir`"")
# Compile an older-branded Rust installer with the same AppId to exercise renaming.
$oldPayload = Join-Path $testRoot 'old-payload'
Copy-Item (Join-Path $output 'payload') $oldPayload -Recurse
Rename-Item (Join-Path $oldPayload 'lailaisay-app.exe') 'tok-app.exe'
foreach ($backend in @('cpu', 'avx2', 'vulkan')) {
    Rename-Item (Join-Path $oldPayload "lailaisay-whisper-$backend.exe") "tok-whisper-$backend.exe"
}
Rename-Item (Join-Path $oldPayload 'lailaisay-diagnostics.cmd') 'Tok-diagnostics.cmd'

$iscc = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6/ISCC.exe'
& $iscc '/DAppVersion=0.0.0.1' '/DProductName=Tok' '/DAppExecutable=tok-app.exe' '/DAppIconName=Tok.ico' '/DDiagnosticsFile=Tok-diagnostics.cmd' "/DPayloadDir=$oldPayload" "/DInstallerOutputDir=$testRoot" (Join-Path $root 'windows/lailaisay.iss')
if ($LASTEXITCODE -ne 0) { throw 'Old-version fixture compilation failed' }
$oldInstaller = Join-Path $testRoot 'Tok-Setup-0.0.0.1-x64.exe'
if ((Invoke-TestProcess $oldInstaller ($commonArgs + '/TASKS=desktopicon') 'install') -ne 0) { throw 'Fresh installation failed' }
if ((Get-ItemProperty $registryPath).DisplayVersion -ne '0.0.0.1') { throw 'Old version was not registered' }
if (-not (Test-Path (Join-Path $oldPrograms 'Tok.lnk'))) { throw 'Old Start menu shortcut missing' }
if (-not (Test-Path $oldDesktopLink)) { throw 'Old desktop shortcut missing' }
Assert-UserDataPreserved

# Running-app guard must block both upgrade and uninstall, without killing lailaisay.
$mutex = [Threading.Mutex]::new($false, 'Local\Tok.Desktop.Running')
try {
    if ((Invoke-TestProcess $Installer $commonArgs 'blocked-upgrade' 30) -eq 0) { throw 'Upgrade ignored running-app guard' }
    $uninstaller = Get-InstalledUninstaller
    if ((Invoke-TestProcess $uninstaller @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART') 'blocked-uninstall' 30) -eq 0) { throw 'Uninstall ignored running-app guard' }
    if (-not (Test-Path (Join-Path $installDir 'tok-app.exe'))) { throw 'Running app files were removed' }
} finally { $mutex.Dispose() }

# Omit /DIR on upgrade: Inno must reuse the registered path, including spaces/Unicode.
$upgradeArgs = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-', '/LANG=chinesetraditional', '/TASKS=desktopicon')
if ((Invoke-TestProcess $Installer $upgradeArgs 'upgrade') -ne 0) { throw 'Upgrade failed' }
$info = Get-Content (Join-Path $installDir 'build-info.json') -Raw | ConvertFrom-Json
$registered = Get-ItemProperty $registryPath
if ($registered.DisplayName -ne 'lailaisay') { throw 'Product display name was not upgraded' }
if ($registered.DisplayVersion -ne $info.version) { throw 'Installed version mismatch' }
if ($registered.InstallLocation.TrimEnd('\') -ne $installDir.TrimEnd('\')) { throw 'Upgrade changed install directory' }
if (-not (Test-Path $desktopLink)) { throw 'Selected desktop shortcut missing' }
foreach ($file in @('lailaisay-app.exe', 'lailaisay-whisper-cpu.exe', 'lailaisay-whisper-avx2.exe', 'lailaisay-whisper-vulkan.exe', 'msvcp140.dll', 'vcruntime140.dll', 'vcruntime140_1.dll', 'lailaisay.ico', 'FONT-LICENSE.txt')) {
    if (-not (Test-Path (Join-Path $installDir $file))) { throw "Installed file missing: $file" }
}
if (-not (Test-Path (Join-Path $programs 'lailaisay.lnk'))) { throw 'Renamed Start menu shortcut missing' }
if ((Test-Path $oldDesktopLink) -or (Test-Path (Join-Path $oldPrograms 'Tok.lnk'))) { throw 'Old shortcuts left behind' }
foreach ($file in @('tok-app.exe', 'tok-whisper-cpu.exe', 'tok-whisper-avx2.exe', 'tok-whisper-vulkan.exe', 'Tok.ico', 'Tok-diagnostics.cmd')) {
    if (Test-Path (Join-Path $installDir $file)) { throw "Old program file left behind: $file" }
}
Assert-UserDataPreserved

# Verify the installed app can find workers and transcribe from its final directory.
$env:TOK_CONFIG = Join-Path $dataDir 'settings.json'
$inferenceArgs = @('--once', '--backend', 'whisper', '--model', "`"$env:RUNNER_TEMP/lailaisay-small.bin`"", '--file', "`"$env:RUNNER_TEMP/lailaisay-jfk.wav`"")
$started = Get-Date
if ((Invoke-TestProcess (Join-Path $installDir 'lailaisay-app.exe') $inferenceArgs 'installed-inference' 600) -ne 0) { throw 'Installed inference failed' }
Get-Content (Join-Path $testRoot 'installed-inference.err')
$text = Get-Content (Join-Path $testRoot 'installed-inference.out') -Raw
if ($text -notmatch 'country') { throw 'Installed app did not transcribe expected speech' }
Write-Host "Installed inference passed in $(((Get-Date) - $started).TotalSeconds) seconds"

$uninstaller = Get-InstalledUninstaller
if ((Invoke-TestProcess $uninstaller @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART') 'uninstall') -ne 0) { throw 'Uninstall failed' }
if ((Test-Path $registryPath) -or (Test-Path (Join-Path $installDir 'lailaisay-app.exe')) -or
    (Test-Path (Join-Path $programs 'lailaisay.lnk')) -or (Test-Path $desktopLink)) {
    throw 'Uninstall left program files, registration or shortcuts behind'
}
Assert-UserDataPreserved
# Also cover a fresh renamed install with no desktop task selected.
if ((Invoke-TestProcess $Installer ($commonArgs + '/TASKS=""') 'fresh-renamed-install') -ne 0) { throw 'Fresh renamed installation failed' }
if (Test-Path $desktopLink) { throw 'Unselected desktop shortcut was created' }
if (-not (Test-Path (Join-Path $programs 'lailaisay.lnk'))) { throw 'Fresh renamed Start menu shortcut missing' }
$uninstaller = Get-InstalledUninstaller
if ((Invoke-TestProcess $uninstaller @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART') 'fresh-renamed-uninstall') -ne 0) { throw 'Fresh renamed uninstall failed' }
Assert-UserDataPreserved
Write-Host 'PASS: fresh install, Tok-to-lailaisay upgrade and shortcut cleanup, running-app guards, installed Whisper inference, uninstall, data preservation'
