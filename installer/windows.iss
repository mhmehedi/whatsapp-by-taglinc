; Inno Setup script for the Windows installer (built by .github/workflows/windows.yml).
; iscc /DAppVersion=0.1.4 /DExe=path\to\multi-whatsapp.exe installer\windows.iss

#define AppName "Whatsapp by TagLinc"
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef Exe
  #define Exe "..\target\x86_64-pc-windows-msvc\release\multi-whatsapp.exe"
#endif

[Setup]
AppId={{B737A171-4B55-4C43-88C7-839378F7DE66}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=TagLinc
AppPublisherURL=https://github.com/mhmehedi/whatsapp-by-taglinc
DefaultDirName={localappdata}\Programs\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
DisableDirPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir=..\dist
OutputBaseFilename=Whatsapp-by-TagLinc-Setup
SetupIconFile=..\assets\AppIcon.ico
UninstallDisplayIcon={app}\{#AppName}.exe
LicenseFile=..\LICENSE
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"

[Files]
Source: "{#Exe}"; DestDir: "{app}"; DestName: "{#AppName}.exe"; Flags: ignoreversion

[Icons]
; AppUserModelID must match APP_ID in src/win.rs, or notifications won't show.
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppName}.exe"; AppUserModelID: "TagLinc.WhatsappByTagLinc"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppName}.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppName}.exe"; Description: "Open {#AppName}"; Flags: nowait postinstall skipifsilent
