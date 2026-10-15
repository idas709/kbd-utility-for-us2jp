; Inno Setup 6 Script for HID Keyboard Layout Remapper
; Professional Windows Installer with Japanese/English support

#define MyAppName "HID Keyboard Layout Remapper"
#define MyAppVersion "0.1.1"
#define MyAppPublisher "HID Remap Project"
#define MyAppURL "https://github.com/idas709/kbd-utility-for-us2jp"
#define MyAppExeName "hid-remap.exe"

[Setup]
; Unique GUID for this application
AppId={{8B321D50-0A16-417A-B962-E9C59B7F1D02}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} v{#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}

; Per-user non-admin installation by default (no UAC elevation required)
; If run as admin, dialog lets user choose per-user or all-users
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes

PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog

; Output file settings
OutputDir=..\dist
OutputBaseFilename=hid-remap-setup-v{#MyAppVersion}
SetupIconFile=..\crates\hid-remap-app\ui\assets\app_icon.ico
UninstallDisplayIcon={app}\{#MyAppExeName}

; Compression
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
LicenseFile=..\LICENSE

; Mutex check to prevent running/modifying while active
AppMutex=Local\HIDKeyboardLayoutRemapperSingleInstanceMutex,HIDKeyboardLayoutRemapperSingleInstanceMutex
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "japanese"; MessagesFile: "compiler:Languages\Japanese.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
japanese.StartWithWindows=Windows 起動時に自動起動する (推奨)
japanese.StartupGroup=起動設定:
japanese.CreateDesktopIcon=デスクトップにショートカットを作成する
english.StartWithWindows=Start automatically when Windows starts (Recommended)
english.StartupGroup=Startup settings:
english.CreateDesktopIcon=Create a desktop shortcut

[Tasks]
Name: "startwithwindows"; Description: "{cm:StartWithWindows}"; GroupDescription: "{cm:StartupGroup}"
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\config.toml"; DestDir: "{app}"; Flags: onlyifdoesntexist
Source: "..\crates\hid-remap-app\ui\assets\app_icon.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist; DestName: "README.md"
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist; DestName: "LICENSE"

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Registry]
; Start with Windows (Run key)
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "HIDKeyboardLayoutRemapper"; ValueData: """{app}\{#MyAppExeName}"""; Flags: uninsdeletevalue; Tasks: startwithwindows

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: files; Name: "{app}\.test_write_*.tmp"
