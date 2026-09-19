; Compile via scripts/package-windows-installer.ps1 after packaging the app.
#ifndef AppVersion
  #error AppVersion is required
#endif
#ifndef PayloadDir
  #error PayloadDir is required
#endif
#ifndef InstallerOutputDir
  #error InstallerOutputDir is required
#endif

; Product overrides are used only by CI to exercise the earlier Rust installation.
#ifndef ProductName
  #define ProductName "lailaisay"
#endif
#ifndef AppExecutable
  #define AppExecutable "lailaisay-app.exe"
#endif
#ifndef AppIconName
  #define AppIconName "lailaisay.ico"
#endif
#ifndef DiagnosticsFile
  #define DiagnosticsFile "lailaisay-diagnostics.cmd"
#endif

[Setup]
; Stable across releases: upgrades reuse one Windows uninstall entry.
AppId={{A15E83E4-9F3F-4FA4-9E91-92C42DE6F179}
AppName={#ProductName}
AppVersion={#AppVersion}
AppPublisher=lailaisay contributors
AppPublisherURL=https://github.com/laihenyi/lailaisay
AppSupportURL=https://github.com/laihenyi/lailaisay/issues
DefaultDirName={localappdata}\Programs\{#ProductName}
DefaultGroupName={#ProductName}
DisableProgramGroupPage=yes
UsePreviousAppDir=yes
UsePreviousTasks=yes
UsePreviousGroup=no
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
AppMutex=Local\Tok.Desktop.Running
CloseApplications=yes
RestartApplications=no
UninstallDisplayIcon={app}\{#AppIconName}
UninstallDisplayName={#ProductName}
SetupIconFile=lailaisay.ico
WizardStyle=modern
Compression=lzma2
SolidCompression=yes
OutputDir={#InstallerOutputDir}
OutputBaseFilename={#ProductName}-Setup-{#AppVersion}-x64
VersionInfoVersion={#AppVersion}
VersionInfoDescription=lailaisay Windows Installer
LicenseFile={#PayloadDir}\LICENSE

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "chinesetraditional"; MessagesFile: "Languages\ChineseTraditional.isl"

[Messages]
english.SetupAppRunningError=lailaisay is running. Quit lailaisay from the notification-area tray, then click OK to continue.
english.UninstallAppRunningError=lailaisay is running. Quit lailaisay from the notification-area tray, then click OK to continue.
chinesetraditional.SetupAppRunningError=lailaisay 正在執行。請從系統匣選擇「結束 lailaisay」，再按「確定」繼續。
chinesetraditional.UninstallAppRunningError=lailaisay 正在執行。請從系統匣選擇「結束 lailaisay」，再按「確定」繼續。

[CustomMessages]
english.DesktopShortcut=Create a desktop shortcut
chinesetraditional.DesktopShortcut=建立桌面捷徑
english.LaunchApp=Launch lailaisay
chinesetraditional.LaunchApp=啟動 lailaisay
english.Diagnostics={#ProductName} diagnostics
chinesetraditional.Diagnostics={#ProductName} 診斷工具

[Tasks]
Name: "desktopicon"; Description: "{cm:DesktopShortcut}"; Flags: unchecked

[Files]
; Only packaged program files. User settings/models are outside {app} and untouched.
Source: "{#PayloadDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "lailaisay.ico"; DestDir: "{app}"; DestName: "{#AppIconName}"; Flags: ignoreversion

#if ProductName == "lailaisay"
[InstallDelete]
; Remove only program files/shortcuts owned by the previous Rust product name.
Type: files; Name: "{app}\tok-app.exe"
Type: files; Name: "{app}\tok-whisper-cpu.exe"
Type: files; Name: "{app}\tok-whisper-avx2.exe"
Type: files; Name: "{app}\tok-whisper-vulkan.exe"
Type: files; Name: "{app}\Tok.ico"
Type: files; Name: "{app}\Tok-diagnostics.cmd"
Type: files; Name: "{userprograms}\Tok\Tok.lnk"
Type: files; Name: "{userprograms}\Tok\Tok diagnostics.lnk"
Type: files; Name: "{userprograms}\Tok\Tok 診斷工具.lnk"
Type: dirifempty; Name: "{userprograms}\Tok"
Type: files; Name: "{userdesktop}\Tok.lnk"

#endif

[Icons]
Name: "{group}\{#ProductName}"; Filename: "{app}\{#AppExecutable}"; WorkingDir: "{app}"; IconFilename: "{app}\{#AppIconName}"
Name: "{group}\{cm:Diagnostics}"; Filename: "{app}\{#DiagnosticsFile}"; WorkingDir: "{app}"; IconFilename: "{app}\{#AppIconName}"
Name: "{autodesktop}\{#ProductName}"; Filename: "{app}\{#AppExecutable}"; WorkingDir: "{app}"; IconFilename: "{app}\{#AppIconName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExecutable}"; Description: "{cm:LaunchApp}"; Flags: nowait postinstall skipifsilent
