#define AppName "Future Academy Link"
#define AppPublisher "Windify"
#define AppURL "https://stem.windify.edu.vn/"
#ifndef AppVersion
  #define AppVersion "2.1.3"
#endif
#ifndef OutputBaseFilename
  #define OutputBaseFilename "Future Academy Link-2.1.3-x64-setup"
#endif

[Setup]
AppId={{A7B3C4D5-E6F7-4890-ABCD-123456789ABC}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppURL}
AppSupportURL={#AppURL}
AppUpdatesURL={#AppURL}
DefaultDirName={autopf64}\Future Academy Link
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
OutputDir=..\dist
OutputBaseFilename={#OutputBaseFilename}
SetupIconFile=..\assets\FutureAcademy.ico
#ifdef GuiBuild
UninstallDisplayIcon={app}\FutureAcademy.ico
#else
UninstallDisplayIcon={app}\FutureAcademyTray.exe
#endif
Compression=lzma2/ultra64
SolidCompression=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
WizardStyle=modern
MinVersion=10.0

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
#ifdef GuiBuild
Source: "..\dist\installer-payload\app\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
#else
Source: "..\dist\installer-payload\Future Academy Link.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\dist\installer-payload\FutureAcademyTray.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\dist\installer-payload\7zr.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\dist\installer-payload\7za.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\dist\installer-payload\firmwares\*"; DestDir: "{app}\firmwares"; Flags: ignoreversion recursesubdirs createallsubdirs
#endif
Source: "..\dist\installer-payload\7za.exe"; DestDir: "{tmp}"; DestName: "7za.exe"; Flags: deleteafterinstall
Source: "..\dist\installer-payload\tools.7z"; DestDir: "{tmp}"; Flags: deleteafterinstall
#ifndef GuiBuild
Source: "..\dist\installer-payload\node-v18.20.8-x64.msi"; DestDir: "{tmp}"; DestName: "node.msi"; Flags: deleteafterinstall
#endif

[Icons]
#ifdef GuiBuild
Name: "{group}\{#AppName}"; Filename: "{app}\Future Academy Link.exe"; IconFilename: "{app}\FutureAcademy.ico"; Comment: "Start Future Academy local hardware server"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\Future Academy Link.exe"; IconFilename: "{app}\FutureAcademy.ico"; Tasks: desktopicon
#else
Name: "{group}\{#AppName}"; Filename: "{app}\FutureAcademyTray.exe"; Comment: "Start Future Academy local hardware server"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\FutureAcademyTray.exe"; Tasks: desktopicon
#endif

[Registry]
Root: HKLM; Subkey: "Software\Windify\Future Academy"; ValueType: string; ValueName: "InstallPath"; ValueData: "{app}"; Flags: uninsdeletekey
Root: HKLM; Subkey: "Software\Windify\Future Academy"; ValueType: string; ValueName: "ToolsPath"; ValueData: "C:\futureacademy\tools"

[Code]
{ --------------------------------------------------------------------------
  VC++ Redistributable 2015-2022 x64 detection and silent installation.
  esptool.exe (PyInstaller binary) requires VCRUNTIME140.dll and the
  Universal CRT (api-ms-win-crt-*.dll / ucrtbase.dll).
  Both are shipped by the Microsoft Visual C++ 2015-2022 Redistributable.
  -------------------------------------------------------------------------- }

function IsVCRedist2022x64Installed: Boolean;
var
  Installed: Cardinal;
begin
  Result := False;
  { Primary key written by VC Redist 2015–2022 x64 }
  if RegQueryDWordValue(HKLM,
      'SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64',
      'Installed', Installed) and (Installed = 1) then
  begin
    Result := True;
    Exit;
  end;
  { WOW6432Node view (seen on some 64-bit Windows builds) }
  if RegQueryDWordValue(HKLM,
      'SOFTWARE\WOW6432Node\Microsoft\VisualStudio\14.0\VC\Runtimes\x64',
      'Installed', Installed) and (Installed = 1) then
    Result := True;
end;

function DownloadVCRedist(const DestFile: String): Boolean;
var
  PsCmd: String;
  ResultCode: Integer;
begin
  { Use PowerShell (available on all Windows 10+) to download.
    The aka.ms/vs/17/release/vc_redist.x64.exe URL is Microsoft's
    permanent redirect to the latest VC 2015-2022 x64 installer. }
  PsCmd := '-NoProfile -NonInteractive -Command ' +
    '"try { ' +
    '[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; ' +
    'Invoke-WebRequest -Uri ''https://aka.ms/vs/17/release/vc_redist.x64.exe'' ' +
    '-OutFile ''' + DestFile + ''' -UseBasicParsing -TimeoutSec 120; ' +
    'exit 0 } catch { exit 1 }"';
  Result := Exec('powershell.exe', PsCmd, '', SW_HIDE, ewWaitUntilTerminated, ResultCode)
            and (ResultCode = 0)
            and FileExists(DestFile);
end;

function EnsureVCRedist: Boolean;
var
  ResultCode: Integer;
  Installer: String;
begin
  if IsVCRedist2022x64Installed then
  begin
    Log('VC++ 2015-2022 x64 Redistributable already installed — skipping.');
    Result := True;
    Exit;
  end;

  Log('VC++ 2015-2022 x64 Redistributable not found — downloading and installing...');
  WizardForm.StatusLabel.Caption :=
    'Installing Visual C++ Runtime (required for hardware tools)...';

  Installer := ExpandConstant('{tmp}\vc_redist.x64.exe');

  if not DownloadVCRedist(Installer) then
  begin
    MsgBox(
      'Could not download the Visual C++ 2015-2022 Redistributable.' + #13#10 +
      'Please ensure you have an internet connection and try again,' + #13#10 +
      'or install it manually from:' + #13#10 +
      'https://aka.ms/vs/17/release/vc_redist.x64.exe',
      mbError, MB_OK);
    Result := False;
    Exit;
  end;

  if not Exec(Installer, '/install /quiet /norestart', '', SW_HIDE,
              ewWaitUntilTerminated, ResultCode) then
  begin
    MsgBox('Failed to launch the Visual C++ Redistributable installer.', mbError, MB_OK);
    Result := False;
    Exit;
  end;

  { Exit codes: 0=success, 1638=newer version exists (also OK), 3010=reboot needed (OK) }
  if (ResultCode = 0) or (ResultCode = 1638) or (ResultCode = 3010) then
  begin
    Log('VC++ 2015-2022 x64 installed successfully (exit code ' + IntToStr(ResultCode) + ').');
    Result := True;
  end
  else
  begin
    MsgBox(
      'Visual C++ Redistributable installer returned error code ' + IntToStr(ResultCode) + '.' + #13#10 +
      'Hardware upload (esptool) may not work correctly.' + #13#10 +
      'You can install it manually from: https://aka.ms/vs/17/release/vc_redist.x64.exe',
      mbError, MB_OK);
    { Non-fatal: allow installation to continue. esptool may still work
      if ucrtbase.dll is already present from a Windows Update. }
    Result := True;
  end;
end;

function GetNodeVersion: String;
var
  Version: String;
begin
  Result := '';
  if RegQueryStringValue(HKLM, 'SOFTWARE\Node.js', 'Version', Version) then
    Result := Version
  else if RegQueryStringValue(HKLM, 'SOFTWARE\WOW6432Node\Node.js', 'Version', Version) then
    Result := Version;
end;

function NodeVersionAtLeast(MinMajor: Integer): Boolean;
var
  Version: String;
  MajorStr: String;
  DotPos: Integer;
  Major: Integer;
begin
  Result := False;
  Version := GetNodeVersion;
  if Version = '' then
    Exit;

  if (Length(Version) > 0) and (Version[1] = 'v') then
    Delete(Version, 1, 1);

  DotPos := Pos('.', Version);
  if DotPos > 1 then
    MajorStr := Copy(Version, 1, DotPos - 1)
  else
    MajorStr := Version;

  Major := StrToIntDef(MajorStr, 0);
  Result := Major >= MinMajor;
end;

#ifdef GuiBuild
function EnsureNodeJs: Boolean;
begin
  { Electron GUI bundles its own Node runtime; no system Node.js MSI. }
  Result := True;
end;
#else
function EnsureNodeJs: Boolean;
var
  ResultCode: Integer;
  NodeMsi: String;
begin
  if NodeVersionAtLeast(18) then
  begin
    Result := True;
    Exit;
  end;

  NodeMsi := ExpandConstant('{tmp}\node.msi');
  if not FileExists(NodeMsi) then
  begin
    MsgBox('Missing Node.js installer payload.', mbError, MB_OK);
    Result := False;
    Exit;
  end;

  if not Exec('msiexec.exe', ExpandConstant('/i "' + NodeMsi + '" /qn /norestart ADDLOCAL=ALL'), '', SW_HIDE, ewWaitUntilTerminated, ResultCode) then
  begin
    MsgBox('Failed to launch Node.js installer.', mbError, MB_OK);
    Result := False;
    Exit;
  end;

  Result := (ResultCode = 0) or (ResultCode = 3010);
  if not Result then
    MsgBox(ExpandConstant('Node.js installer failed with exit code ' + IntToStr(ResultCode) + '.'), mbError, MB_OK);
end;
#endif

function ShouldExtractToolsInternal: Boolean;
var
  ToolsRoot: String;
  LibrariesRoot: String;
  RequiredLibs: TArrayOfString;
  I: Integer;
begin
  ToolsRoot := 'C:\futureacademy\tools';
  LibrariesRoot := ToolsRoot + '\Arduino\libraries';

  { First install or broken tools folder: always extract. }
  if not FileExists(ToolsRoot + '\Arduino\arduino-cli.exe') then
  begin
    Log('Tools extract required: missing arduino-cli.exe');
    Result := True;
    Exit;
  end;

  { Required libraries from script/libraries.json (dirName or resolved folder name). }
  SetArrayLength(RequiredLibs, 25);
  RequiredLibs[0] := 'Adafruit_AHTX0';
  RequiredLibs[1] := 'Adafruit_BusIO';
  RequiredLibs[2] := 'Adafruit_GFX_Library';
  RequiredLibs[3] := 'Adafruit_Sensor';
  RequiredLibs[4] := 'Adafruit_SSD1306';
  RequiredLibs[5] := 'Adafruit_TCS34725';
  RequiredLibs[6] := 'Adafruit_VL53L0X';
  RequiredLibs[7] := 'ArduinoGraphics';
  RequiredLibs[8] := 'AsyncTCP';
  RequiredLibs[9] := 'ESP32Servo';
  RequiredLibs[10] := 'ESPAsyncWebServer';
  RequiredLibs[11] := 'ESP8266Audio';
  RequiredLibs[12] := 'Servo';
  RequiredLibs[13] := 'avr-stl';
  RequiredLibs[14] := 'ServoK210';
  RequiredLibs[15] := 'SimpleList';
  RequiredLibs[16] := 'Button';
  RequiredLibs[17] := 'DS18B20';
  RequiredLibs[18] := 'ESP_Scan';
  RequiredLibs[19] := 'Led_Control';
  RequiredLibs[20] := 'Motor';
  RequiredLibs[21] := 'pgmspace';
  RequiredLibs[22] := 'PIR';
  RequiredLibs[23] := 'WS2812B';
  RequiredLibs[24] := 'Windify';

  for I := 0 to GetArrayLength(RequiredLibs) - 1 do
  begin
    if not DirExists(LibrariesRoot + '\' + RequiredLibs[I]) then
    begin
      Log(Format('Tools extract required: missing library "%s"', [RequiredLibs[I]]));
      Result := True;
      Exit;
    end;
  end;

  Log('Tools extract skipped: required libraries already present.');
  Result := False;
end;

function ShouldExtractTools: Boolean;
begin
  Result := ShouldExtractToolsInternal;
end;

function ExtractToolsArchive: Boolean;
var
  ResultCode: Integer;
  SevenZip: String;
  Archive: String;
  DestRoot: String;
  Params: String;
begin
  SevenZip := ExpandConstant('{tmp}\7za.exe');
  Archive := ExpandConstant('{tmp}\tools.7z');
  DestRoot := 'C:\futureacademy';

  if not FileExists(SevenZip) then
  begin
    MsgBox('Missing 7-Zip helper in installer temp folder. Antivirus may have removed it.', mbError, MB_OK);
    Result := False;
    Exit;
  end;
  if not FileExists(Archive) then
  begin
    MsgBox('Missing tools.7z in installer temp folder.', mbError, MB_OK);
    Result := False;
    Exit;
  end;

  ForceDirectories(DestRoot);
  Params := 'x "' + Archive + '" -o"' + DestRoot + '" -y';

  WizardForm.StatusLabel.Caption := 'Extracting build tools (this may take a few minutes)...';
  WizardForm.ProgressGauge.Style := npbstMarquee;
  try
    if not Exec(SevenZip, Params, ExpandConstant('{tmp}'), SW_HIDE, ewWaitUntilTerminated, ResultCode) then
    begin
      MsgBox(
        'Could not run 7-Zip to extract Arduino tools.' + #13#10 +
        'Windows Defender or SmartScreen may have blocked the installer.' + #13#10#13#10 +
        'Try: unblock the setup file (Properties → Unblock), allow the installer in antivirus, then run again.',
        mbError, MB_OK);
      Result := False;
      Exit;
    end;

    if (ResultCode <> 0) and (ResultCode <> 1) then
    begin
      MsgBox(
        ExpandConstant('Extracting build tools failed (7-Zip exit code ') + IntToStr(ResultCode) + ').' + #13#10 +
        'Check free disk space on the install drive and antivirus logs.',
        mbError, MB_OK);
      Result := False;
      Exit;
    end;

    if not FileExists(DestRoot + '\tools\Arduino\arduino-cli.exe') then
    begin
      MsgBox(
        'Build tools were not extracted correctly (arduino-cli.exe missing).' + #13#10 +
        'Allow Future Academy in Windows Security, then reinstall.',
        mbError, MB_OK);
      Result := False;
      Exit;
    end;

    Result := True;
  finally
    WizardForm.ProgressGauge.Style := npbstNormal;
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
  begin
    { Step 1: Ensure Visual C++ Redistributable 2015-2022 x64 is present.
      Required by esptool.exe (VCRUNTIME140.dll, api-ms-win-crt-*.dll). }
    if not EnsureVCRedist then
      Abort;
    { Step 2: Install Node.js runtime if needed (CLI build only). }
    if not EnsureNodeJs then
      Abort;
    { Step 3: Extract Arduino tools archive if missing or outdated. }
    if ShouldExtractToolsInternal then
    begin
      if not ExtractToolsArchive then
        Abort;
    end;
  end;
end;
