#define AppName "Future Academy Link"
#define AppPublisher "Windify"
#define AppURL "https://stem.windify.edu.vn/"
#ifndef AppVersion
  #define AppVersion "2.1.0"
#endif
#ifndef OutputBaseFilename
  #define OutputBaseFilename "FutureAcademyLinkWizard-{#AppVersion}-x64-setup"
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
OutputDir=..\dist
OutputBaseFilename={#OutputBaseFilename}
SetupIconFile=..\assets\FutureAcademy.ico
UninstallDisplayIcon={app}\FutureAcademyTray.exe
Compression=lzma2/ultra64
SolidCompression=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
WizardStyle=modern
MinVersion=10.0

; Enable wizard pages
DisableWelcomePage=no
DisableDirPage=no
DisableProgramGroupPage=no
DisableReadyPage=no
DisableReadyToInstallPage=no
DisableFinishedPage=no
ShowLanguageDialog=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
english.LaunchAfterInstall=Launch Future Academy Link after installation
english.CreateDesktopIcon=Create a &desktop shortcut
english.QuickLaunchIcon=Create a &Quick Launch shortcut

[Types]
Name: "full"; Description: "Full installation (recommended)"
Name: "custom"; Description: "Custom installation"; Flags: iscustom

[Components]
Name: "main"; Description: "Future Academy Link"; Types: full custom; Flags: fixed
Name: "tools"; Description: "Arduino build tools (required for hardware programming)"; Types: full; Flags: fixed
Name: "firmwares"; Description: "Device firmwares"; Types: full

[Dirs]
Name: "{app}"; Permissions: everyone-modify
Name: "{commonstartup}"; Permissions: users-modify

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "quicklaunchicon"; Description: "{cm:QuickLaunchIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked; OnlyBelowVersion: 6.1; Check: not IsAdminInstallMode
Name: "launchapp"; Description: "{cm:LaunchAfterInstall}"; GroupDescription: "Additional options:"; Flags: checkedonce

[Files]
; Main executable
Source: "..\dist\installer-payload\FutureAcademyTray.exe"; DestDir: "{app}"; Flags: ignoreversion; Components: main

; 7-Zip executables
Source: "..\dist\installer-payload\7zr.exe"; DestDir: "{app}"; Flags: ignoreversion; Components: main
Source: "..\dist\installer-payload\7za.exe"; DestDir: "{app}"; Flags: ignoreversion; Components: main

; Tools archive (for post-install extraction)
Source: "..\dist\installer-payload\tools.7z"; DestDir: "{tmp}"; Flags: deleteafterinstall; Components: tools

; 7za.exe helper for tools extraction
Source: "..\dist\installer-payload\7za.exe"; DestDir: "{tmp}"; DestName: "7za.exe"; Flags: deleteafterinstall; Components: tools

; Firmwares
Source: "..\dist\installer-payload\firmwares\*"; DestDir: "{app}\firmwares"; Flags: ignoreversion recursesubdirs createallsubdirs; Components: firmwares

[Icons]
; Start Menu shortcuts
Name: "{group}\{#AppName}"; Filename: "{app}\FutureAcademyTray.exe"; Comment: "Start Future Academy local hardware server"; Components: main
Name: "{group}\Uninstall {#AppName}"; Filename: "{uninstallexe}"; Components: main

; Desktop shortcut
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\FutureAcademyTray.exe"; Tasks: desktopicon; Components: main

; Quick Launch shortcut
Name: "{userappdata}\Microsoft\Internet Explorer\Quick Launch\{#AppName}"; Filename: "{app}\FutureAcademyTray.exe"; Tasks: quicklaunchicon; Components: main

[Registry]
; Installation path registry entries
Root: HKLM; Subkey: "Software\Windify\Future Academy"; ValueType: string; ValueName: "InstallPath"; ValueData: "{app}"; Flags: uninsdeletekey; Components: main
Root: HKLM; Subkey: "Software\Windify\Future Academy"; ValueType: string; ValueName: "Version"; ValueData: "{#AppVersion}"; Flags: uninsdeletekey; Components: main
Root: HKLM; Subkey: "Software\Windify\Future Academy"; ValueType: string; ValueName: "ToolsPath"; ValueData: "C:\futureacademy\tools"; Flags: uninsdeletekey; Components: tools

; App paths for command-line access
Root: HKLM; Subkey: "Software\Microsoft\Windows\CurrentVersion\App Paths\FutureAcademyTray.exe"; ValueType: string; ValueData: "{app}\FutureAcademyTray.exe"; Flags: uninsdeletekey; Components: main
Root: HKLM; Subkey: "Software\Microsoft\Windows\CurrentVersion\App Paths\FutureAcademyTray.exe"; ValueType: string; ValueName: "Path"; ValueData: "{app}"; Components: main

[Run]
; Launch application after installation if selected
Filename: "{app}\FutureAcademyTray.exe"; Description: "{cm:LaunchAfterInstall}"; Tasks: launchapp; Flags: nowait postinstall skipifsilent

[Code]
// Check if tools need extraction
function ShouldExtractToolsInternal: Boolean;
var
  ToolsRoot: String;
  LibrariesRoot: String;
  RequiredLibs: TArrayOfString;
  I: Integer;
begin
  ToolsRoot := 'C:\futureacademy\tools';
  LibrariesRoot := ToolsRoot + '\Arduino\libraries';
  Result := False;

  // First install or broken tools folder: always extract
  if not FileExists(ToolsRoot + '\Arduino\arduino-cli.exe') then
  begin
    Log('Tools extract required: missing arduino-cli.exe');
    Result := True;
    Exit;
  end;

  // Required libraries
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

// Extract tools archive using 7za.exe
function ExtractToolsArchive: Boolean;
var
  ResultCode: Integer;
  SevenZip: String;
  Archive: String;
  DestRoot: String;
  Params: String;
begin
  Result := False;
  SevenZip := ExpandConstant('{tmp}\7za.exe');
  Archive := ExpandConstant('{tmp}\tools.7z');
  DestRoot := 'C:\futureacademy';

  if not FileExists(SevenZip) then
  begin
    MsgBox('Missing 7-Zip helper in installer temp folder. Antivirus may have removed it.', mbError, MB_OK);
    Exit;
  end;

  if not FileExists(Archive) then
  begin
    MsgBox('Missing tools.7z in installer temp folder.', mbError, MB_OK);
    Exit;
  end;

  ForceDirectories(DestRoot);
  Params := 'x "' + Archive + '" -o"' + DestRoot + '" -y';

  WizardForm.StatusLabel.Caption := 'Extracting Arduino build tools (this may take a few minutes)...';
  WizardForm.ProgressGauge.Style := npbstMarquee;
  try
    if not Exec(SevenZip, Params, ExpandConstant('{tmp}'), SW_HIDE, ewWaitUntilTerminated, ResultCode) then
    begin
      MsgBox(
        'Could not run 7-Zip to extract Arduino tools.' + #13#10 +
        'Windows Defender or SmartScreen may have blocked the installer.' + #13#10#13#10 +
        'Try: unblock the setup file (Properties -> Unblock), allow the installer in antivirus, then run again.',
        mbError, MB_OK);
      Exit;
    end;

    if (ResultCode <> 0) and (ResultCode <> 1) then
    begin
      MsgBox(
        'Extracting build tools failed (7-Zip exit code ' + IntToStr(ResultCode) + ').' + #13#10 +
        'Check free disk space on the install drive and antivirus logs.',
        mbError, MB_OK);
      Exit;
    end;

    if not FileExists(DestRoot + '\tools\Arduino\arduino-cli.exe') then
    begin
      MsgBox(
        'Build tools were not extracted correctly (arduino-cli.exe missing).' + #13#10 +
        'Allow Future Academy in Windows Security, then reinstall.',
        mbError, MB_OK);
      Exit;
    end;

    Result := True;
  finally
    WizardForm.ProgressGauge.Style := npbstNormal;
  end;
end;

// Initialize wizard
procedure InitializeWizard;
begin
  // Set default components based on install type
  WizardForm.StatusLabel.Caption := 'Click Next to continue...';
end;

// Handle installation steps
procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
  begin
    // Extract tools if component selected
    if ShouldExtractToolsInternal then
    begin
      if not ExtractToolsArchive then
      begin
        if MsgBox(
          'Failed to extract Arduino build tools. You can try extracting them manually later.' + #13#10 +
          'Continue with installation anyway?',
          mbConfirmation, MB_YESNO) = IDNO then
        begin
          Abort;
        end;
      end;
    end;
  end;
end;

// Custom Next button behavior
function NextButtonClick(CurPageID: Integer): Boolean;
begin
  Result := True;
end;

// Handle completion
procedure CurPageChanged(CurPageID: Integer);
begin
  if CurPageID = wpFinished then
  begin
    WizardForm.FinishedLabel.Caption :=
      'Future Academy Link has been successfully installed.' + #13#10 + #13#10 +
      'Click "Finish" to close the wizard and launch the application.' + #13#10 + #13#10 +
      'Note: The first run may take a moment to start the local hardware server.';
  end;
end;
