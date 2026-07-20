; ==========================================================================
;  Instant Replay - Installer
;
;  Dependency handling design:
;   - FFmpeg and the Virtual Audio Capturer are only downloaded if missing.
;   - Downloads run as fully detached processes (a temp .cmd launcher +
;     Exec(..., ewNoWait)) instead of blocking child processes, and the
;     wizard pumps its own Windows message queue while it waits. This is
;     what keeps the window movable/responsive instead of "Not Responding".
;   - The FFmpeg archive's internal folder name is detected at runtime
;     (never hardcoded), so an upstream naming/versioning change can't
;     silently break the install.
; ==========================================================================

[Setup]
AppId={{B3A7F2E1-9C4D-4E8B-A6F1-2D8E9C5B3A7F}
AppName=Instant Replay
AppVersion=1.0.0
AppPublisher=InstantReplay
DefaultDirName={autopf}\Instant Replay
DefaultGroupName=Instant Replay
OutputDir=..
OutputBaseFilename=InstantReplaySetup
SetupIconFile=..\icon.ico
UninstallDisplayIcon={app}\InstantReplay.exe
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
DisableProgramGroupPage=yes
LicenseFile=terms.txt
CloseApplications=no
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional icons:"; Flags: unchecked

[Files]
Source: "..\publish\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\Instant Replay"; Filename: "{app}\InstantReplay.exe"
Name: "{group}\Uninstall Instant Replay"; Filename: "{uninstallexe}"
Name: "{autodesktop}\Instant Replay"; Filename: "{app}\InstantReplay.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\InstantReplay.exe"; Description: "Launch Instant Replay now"; Flags: nowait postinstall skipifsilent unchecked

[UninstallDelete]
Type: filesandordirs; Name: "{app}"

[Code]
const
  FFmpegDownloadUrl = 'https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip';
  FFmpegBinDir = 'C:\ffmpeg';
  VACDownloadUrl = 'https://github.com/rdp/screen-capture-recorder-to-video-windows-free/releases/download/v0.13.3/Setup.Screen.Capturer.Recorder.v0.13.3.exe';
  VACUninstallKey = 'SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Screen Capturer Recorder_is1';
  VACUninstallKeyWow64 = 'SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Screen Capturer Recorder_is1';
  DownloadConnectTimeoutSec = '30';
  DownloadMaxTimeSec = '300';
  DownloadTimeoutMs = 330000;   // slightly above curl's own --max-time, as a safety net
  ExtractTimeoutMs = 60000;
  InstallTimeoutMs = 120000;
  PM_REMOVE = 1;

type
  TPoint2 = record
    x, y: LongInt;
  end;
  TMsg2 = record
    hwnd: LongWord;
    message: LongWord;
    wParam: LongWord;
    lParam: LongWord;
    time: LongWord;
    pt: TPoint2;
  end;

var
  LogStrings: TStringList;
  UserCancelled: Boolean;

{ ---------------------------------------------------------------------- }
{  Win32 message pump (this is what keeps the window responsive/movable  }
{  while a background process runs)                                     }
{ ---------------------------------------------------------------------- }

function PeekMessageA(var lpMsg: TMsg2; hWnd, wMsgFilterMin, wMsgFilterMax, wRemoveMsg: LongWord): LongWord;
  external 'PeekMessageA@user32.dll stdcall';
function TranslateMessage(const lpMsg: TMsg2): LongWord;
  external 'TranslateMessage@user32.dll stdcall';
function DispatchMessageA(const lpMsg: TMsg2): LongWord;
  external 'DispatchMessageA@user32.dll stdcall';
function GetTickCount: LongWord;
  external 'GetTickCount@kernel32.dll stdcall';

function CreateFileW(lpFileName: String; dwDesiredAccess, dwShareMode, lpSecurityAttributes: LongWord;
  dwCreationDisposition, dwFlagsAndAttributes, hTemplateFile: LongWord): LongWord;
  external 'CreateFileW@kernel32.dll stdcall';
function GetFileSizeEx(hFile: LongWord; var lpFileSize: Int64): LongWord;
  external 'GetFileSizeEx@kernel32.dll stdcall';
function CloseHandle(hObject: LongWord): LongWord;
  external 'CloseHandle@kernel32.dll stdcall';

function GetFileSizeByPath(const FilePath: String; var FileSize: Int64): Boolean;
var
  hFile: LongWord;
begin
  Result := False;
  FileSize := 0;
  hFile := CreateFileW(FilePath, $80000000, 1, 0, 3, $80, 0);
  if hFile = $FFFFFFFF then Exit;
  try
    Result := GetFileSizeEx(hFile, FileSize) <> 0;
  finally
    CloseHandle(hFile);
  end;
end;

procedure PumpMessages;
var
  Msg: TMsg2;
begin
  while PeekMessageA(Msg, 0, 0, 0, PM_REMOVE) <> 0 do
  begin
    TranslateMessage(Msg);
    DispatchMessageA(Msg);
  end;
end;

{ ---------------------------------------------------------------------- }
{  Logging helpers                                                       }
{ ---------------------------------------------------------------------- }

procedure WriteLog(const Msg: String);
begin
  if LogStrings <> nil then
    LogStrings.Add(Msg);
end;

procedure UpdateStatus(const Msg: String);
begin
  WriteLog(Msg);
  if WizardForm <> nil then
  begin
    WizardForm.StatusLabel.Caption := Msg;
    WizardForm.Refresh;
  end;
  PumpMessages;
end;

procedure SetBusyProgress(Busy: Boolean);
begin
  if WizardForm = nil then Exit;
  if Busy then
    WizardForm.ProgressGauge.Style := npbstMarquee
  else
  begin
    WizardForm.ProgressGauge.Style := npbstNormal;
    WizardForm.ProgressGauge.Position := 0;
  end;
  PumpMessages;
end;

{ ---------------------------------------------------------------------- }
{  Detection helpers                                                     }
{ ---------------------------------------------------------------------- }

function IsCurlAvailable: Boolean;
var
  ResultCode: Integer;
begin
  Result := Exec('cmd.exe', '/c curl.exe --version >nul 2>nul', '', SW_HIDE, ewWaitUntilTerminated, ResultCode)
            and (ResultCode = 0);
end;

function IsFFmpegInstalled: Boolean;
begin
  Result := FileExists(FFmpegBinDir + '\ffmpeg.exe');
end;

function IsFFmpegInPath: Boolean;
var
  CurrentPath: String;
begin
  Result := False;
  if RegQueryStringValue(HKLM, 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment', 'Path', CurrentPath) then
    Result := Pos(LowerCase(FFmpegBinDir), LowerCase(CurrentPath)) > 0;
end;

{ Checks the registry (source of truth, same key the uninstaller looks
  for) as well as the known install paths, so neither side can produce a
  false negative that triggers a pointless reinstall. }
function IsVACInstalled: Boolean;
var
  Dummy: String;
begin
  Result := RegQueryStringValue(HKLM, VACUninstallKey, 'UninstallString', Dummy) or
            RegQueryStringValue(HKLM, VACUninstallKeyWow64, 'UninstallString', Dummy) or
            FileExists('C:\Program Files\Screen Capturer Recorder\ScreenCapturerRecorder.exe') or
            FileExists('C:\Program Files (x86)\Screen Capturer Recorder\ScreenCapturerRecorder.exe');
end;

function IsDefenderExclusionAdded(const ExclusionPath: String): Boolean;
var
  ResultCode: Integer;
begin
  Result := False;
  if Exec('powershell.exe',
    '-NoProfile -ExecutionPolicy Bypass -Command "if ((Get-MpPreference).ExclusionPath -contains ''' + ExclusionPath + ''') { exit 0 } else { exit 1 }"',
    '', SW_HIDE, ewWaitUntilTerminated, ResultCode) then
    Result := (ResultCode = 0);
end;

function IsCFAExclusionAdded(const ExePath: String): Boolean;
var
  ResultCode: Integer;
begin
  Result := False;
  if Exec('powershell.exe',
    '-NoProfile -ExecutionPolicy Bypass -Command "if ((Get-MpPreference).EnableControlledFolderAccessAllowedApplications -contains ''' + ExePath + ''') { exit 0 } else { exit 1 }"',
    '', SW_HIDE, ewWaitUntilTerminated, ResultCode) then
    Result := (ResultCode = 0);
end;

{ ---------------------------------------------------------------------- }
{  Detached process runner                                               }
{  Writes a tiny .cmd launcher, starts it with ewNoWait (so it is NOT a  }
{  child process Setup blocks on), then polls for a done-marker file     }
{  while pumping messages so the window stays responsive and movable.    }
{ ---------------------------------------------------------------------- }

function RunHiddenAsync(const Exe, Params, WhatLabel: String; TimeoutMs: Integer): Integer;
var
  DoneFile, ScriptFile, ScriptContent: String;
  Lines: TStringList;
  StartTick, Elapsed: LongWord;
  DummyCode: Integer;
begin
  Result := -1;
  DoneFile := ExpandConstant('{tmp}\') + WhatLabel + '_done.txt';
  ScriptFile := ExpandConstant('{tmp}\') + WhatLabel + '_run.cmd';
  if FileExists(DoneFile) then DeleteFile(DoneFile);
  if FileExists(ScriptFile) then DeleteFile(ScriptFile);

  ScriptContent := '@echo off' + #13#10 +
    '"' + Exe + '" ' + Params + #13#10 +
    'echo %errorlevel%>"' + DoneFile + '"' + #13#10;
  SaveStringToFile(ScriptFile, ScriptContent, False);

  WriteLog('[' + WhatLabel + '] Launching detached: ' + Exe + ' ' + Params);

  if not Exec(ScriptFile, '', '', SW_HIDE, ewNoWait, DummyCode) then
  begin
    WriteLog('[' + WhatLabel + '] ERROR: failed to launch background process.');
    Exit;
  end;

  StartTick := GetTickCount;
  while not FileExists(DoneFile) do
  begin
    PumpMessages;
    Sleep(80);

    if UserCancelled then
    begin
      WriteLog('[' + WhatLabel + '] Cancelled by user.');
      Result := -1;
      Exit;
    end;

    if WizardForm <> nil then
      WizardForm.Refresh;

    Elapsed := GetTickCount - StartTick;
    if (TimeoutMs > 0) and (Elapsed > LongWord(TimeoutMs)) then
    begin
      WriteLog('[' + WhatLabel + '] ERROR: timed out after ' + IntToStr(TimeoutMs div 1000) + 's.');
      Exit;
    end;
  end;

  Lines := TStringList.Create;
  try
    try
      Lines.LoadFromFile(DoneFile);
      if Lines.Count > 0 then
        Result := StrToIntDef(Trim(Lines[0]), -1);
    except
      Result := -1;
    end;
  finally
    Lines.Free;
  end;

  DeleteFile(DoneFile);
  DeleteFile(ScriptFile);
  WriteLog('[' + WhatLabel + '] Finished with exit code ' + IntToStr(Result) + '.');
end;

{ ---------------------------------------------------------------------- }
{  System configuration helpers                                          }
{ ---------------------------------------------------------------------- }

procedure AddToSystemPath(const Dir: String);
var
  CurrentPath, NewPath: String;
begin
  if RegQueryStringValue(HKLM, 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment', 'Path', CurrentPath) then
  begin
    if Pos(LowerCase(Dir), LowerCase(CurrentPath)) = 0 then
    begin
      NewPath := CurrentPath + ';' + Dir;
      RegWriteStringValue(HKLM, 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment', 'Path', NewPath);
      WriteLog('  Added to system PATH: ' + Dir);
    end
    else
      WriteLog('  Already in system PATH: ' + Dir);
  end
  else
    WriteLog('  WARNING: Could not read system PATH');
end;

procedure RemoveFromSystemPath(const Dir: String);
var
  CurrentPath: String;
begin
  if RegQueryStringValue(HKLM, 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment', 'Path', CurrentPath) then
  begin
    StringChangeEx(CurrentPath, ';' + Dir, '', False);
    StringChangeEx(CurrentPath, Dir + ';', '', False);
    StringChangeEx(CurrentPath, Dir, '', False);
    RegWriteStringValue(HKLM, 'SYSTEM\CurrentControlSet\Control\Session Manager\Environment', 'Path', CurrentPath);
    WriteLog('  Removed from system PATH: ' + Dir);
  end;
end;

procedure BroadcastEnvironmentChange;
var
  ResultCode: Integer;
begin
  Exec('powershell.exe',
    '-NoProfile -ExecutionPolicy Bypass -Command "$sig = [System.Runtime.InteropServices.Marshal]; $HWND_BROADCAST = [IntPtr]0xffff; $WM_SETTINGCHANGE = 0x001A; $sig::SendMessage($HWND_BROADCAST, $WM_SETTINGCHANGE, [IntPtr]::Zero, [IntPtr]::Zero)"',
    '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

procedure AddDefenderExclusion(const ExclusionPath: String);
var
  ResultCode: Integer;
begin
  Exec('powershell.exe',
    '-NoProfile -ExecutionPolicy Bypass -Command "try { Add-MpPreference -ExclusionPath ''' + ExclusionPath + ''' -ErrorAction Stop } catch { }"',
    '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  WriteLog('  Defender exclusion added.');
end;

procedure RemoveDefenderExclusion(const ExclusionPath: String);
var
  ResultCode: Integer;
begin
  Exec('powershell.exe',
    '-NoProfile -ExecutionPolicy Bypass -Command "try { Remove-MpPreference -ExclusionPath ''' + ExclusionPath + ''' -ErrorAction Stop } catch { }"',
    '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  WriteLog('  Defender exclusion removed.');
end;

procedure AddCFAExclusion(const ExePath: String);
var
  ResultCode: Integer;
begin
  Exec('powershell.exe',
    '-NoProfile -ExecutionPolicy Bypass -Command "try { Add-MpPreference -EnableControlledFolderAccessAllowedApplications @(''' + ExePath + ''') -ErrorAction Stop } catch { }"',
    '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  WriteLog('  Controlled Folder Access exclusion added.');
end;

procedure RemoveCFAExclusion(const ExePath: String);
var
  ResultCode: Integer;
begin
  Exec('powershell.exe',
    '-NoProfile -ExecutionPolicy Bypass -Command "try { Remove-MpPreference -EnableControlledFolderAccessAllowedApplications @(''' + ExePath + ''') -ErrorAction Stop } catch { }"',
    '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  WriteLog('  Controlled Folder Access exclusion removed.');
end;

procedure KillVACProcesses;
var
  ResultCode: Integer;
begin
  Exec('taskkill.exe', '/f /im runcapture.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec('taskkill.exe', '/f /im ScreenCapturerRecorder.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec('taskkill.exe', '/f /im audiorepeater.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec('taskkill.exe', '/f /im audiorepeater_kc.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

{ ---------------------------------------------------------------------- }
{  Download (detached, non-blocking, validated)                          }
{ ---------------------------------------------------------------------- }

function DownloadFile(const Url, DestPath, WhatLabel: String): Boolean;
var
  Params: String;
  ExitCode: Integer;
  FileSize: Int64;
begin
  Result := False;

  if UserCancelled then
    Exit;

  if not IsCurlAvailable then
  begin
    WriteLog('[' + WhatLabel + '] ERROR: curl.exe not found on this system.');
    Exit;
  end;

  if FileExists(DestPath) then
    DeleteFile(DestPath);

  { -f  : fail (no output written) on HTTP 4xx/5xx instead of saving the
          error page as if it were the file - this was the root cause of
          "download succeeds, then fails weirdly later"
    -L  : follow redirects
    --connect-timeout / --max-time : never hang forever
    --retry : transient network retries }
  Params := '-f -L --connect-timeout ' + DownloadConnectTimeoutSec + ' --max-time ' + DownloadMaxTimeSec +
    ' --retry 3 --retry-delay 5 --retry-connrefused -s -S -o "' + DestPath + '" "' + Url + '"';

  ExitCode := RunHiddenAsync('curl.exe', Params, WhatLabel + '_dl', DownloadTimeoutMs);

  if ExitCode <> 0 then
  begin
    WriteLog('[' + WhatLabel + '] ERROR: Download failed (curl exit code ' + IntToStr(ExitCode) + ').');
    Exit;
  end;

  if not FileExists(DestPath) then
  begin
    WriteLog('[' + WhatLabel + '] ERROR: Download reported success but file is missing.');
    Exit;
  end;

  FileSize := 0;
  if not GetFileSizeByPath(DestPath, FileSize) then
    FileSize := 0;

  if FileSize <= 0 then
  begin
    WriteLog('[' + WhatLabel + '] ERROR: Downloaded file is empty.');
    DeleteFile(DestPath);
    Exit;
  end;

  WriteLog('[' + WhatLabel + '] Download complete (' + IntToStr(FileSize) + ' bytes).');
  Result := True;
end;

function PromptContinueWithoutComponent(const ComponentName: String): Boolean;
begin
  Result := (MsgBox(ComponentName + ' could not be installed automatically (see install_log.txt for details).' + #13#10 + #13#10 +
    'You can continue installing Instant Replay and set up ' + ComponentName + ' manually later, or cancel setup and try again.' + #13#10 + #13#10 +
    'Continue without ' + ComponentName + '?',
    mbConfirmation, MB_YESNO) = IDYES);
end;

{ Finds the single top-level directory produced by extracting the FFmpeg
  archive at runtime, instead of assuming a hardcoded folder name. This
  is what survives BtbN renaming/re-versioning their build output. }
function FindExtractedFFmpegDir(const ExtractRoot: String): String;
var
  ListFile: String;
  Lines: TStringList;
begin
  Result := '';
  ListFile := ExpandConstant('{tmp}\ffmpeg_dirlist.txt');
  if FileExists(ListFile) then DeleteFile(ListFile);

  RunHiddenAsync('powershell.exe',
    '-NoProfile -ExecutionPolicy Bypass -Command "Get-ChildItem -Path \"' + ExtractRoot + '\" -Directory | Select-Object -First 1 -ExpandProperty FullName | Out-File -FilePath \"' + ListFile + '\" -Encoding ASCII"',
    'ffmpeg_finddir', ExtractTimeoutMs);

  if FileExists(ListFile) then
  begin
    Lines := TStringList.Create;
    try
      Lines.LoadFromFile(ListFile);
      if Lines.Count > 0 then
        Result := Trim(Lines[0]);
    finally
      Lines.Free;
    end;
    DeleteFile(ListFile);
  end;
end;

{ ---------------------------------------------------------------------- }
{  Dependency installers                                                 }
{ ---------------------------------------------------------------------- }

function InstallFFmpeg: Boolean;
var
  TempZip, TempExtract, SourceDir: String;
  ExtractExitCode: Integer;
begin
  Result := False;
  if UserCancelled then
    Exit;
  TempZip := ExpandConstant('{tmp}\ffmpeg.zip');
  TempExtract := ExpandConstant('{tmp}\ffmpeg_extract');

  WriteLog('[FFmpeg] Not found. Downloading...');
  UpdateStatus('Downloading FFmpeg...');
  SetBusyProgress(True);

  if not DownloadFile(FFmpegDownloadUrl, TempZip, 'FFmpeg') then
  begin
    SetBusyProgress(False);
    Exit;
  end;

  UpdateStatus('Extracting FFmpeg...');
  ExtractExitCode := RunHiddenAsync('powershell.exe',
    '-NoProfile -ExecutionPolicy Bypass -Command "Expand-Archive -Path \"' + TempZip + '\" -DestinationPath \"' + TempExtract + '\" -Force"',
    'ffmpeg_extract', ExtractTimeoutMs);

  if ExtractExitCode <> 0 then
  begin
    WriteLog('[FFmpeg] ERROR: Extraction failed (exit code ' + IntToStr(ExtractExitCode) + ').');
    SetBusyProgress(False);
    Exit;
  end;
  WriteLog('[FFmpeg] Extraction complete.');

  SourceDir := FindExtractedFFmpegDir(TempExtract);
  if SourceDir = '' then
  begin
    WriteLog('[FFmpeg] ERROR: Could not locate extracted FFmpeg folder.');
    SetBusyProgress(False);
    Exit;
  end;
  WriteLog('[FFmpeg] Found extracted folder: ' + SourceDir);

  if not FileExists(SourceDir + '\bin\ffmpeg.exe') then
  begin
    WriteLog('[FFmpeg] ERROR: ffmpeg.exe not found under ' + SourceDir + '\bin');
    SetBusyProgress(False);
    Exit;
  end;

  UpdateStatus('Installing FFmpeg...');
  if not DirExists(FFmpegBinDir) then
    if not CreateDir(FFmpegBinDir) then
    begin
      WriteLog('[FFmpeg] ERROR: Could not create ' + FFmpegBinDir);
      SetBusyProgress(False);
      Exit;
    end;

  WriteLog('[FFmpeg] Copying ffmpeg.exe -> ' + FFmpegBinDir + '\');
  if not FileCopy(SourceDir + '\bin\ffmpeg.exe', FFmpegBinDir + '\ffmpeg.exe', False) then
  begin
    WriteLog('[FFmpeg] ERROR: Copy to ' + FFmpegBinDir + ' failed.');
    SetBusyProgress(False);
    Exit;
  end;

  UpdateStatus('Adding FFmpeg to system PATH...');
  AddToSystemPath(FFmpegBinDir);
  BroadcastEnvironmentChange;

  WriteLog('[FFmpeg] Cleaning up temp files...');
  DeleteFile(TempZip);
  RunHiddenAsync('powershell.exe',
    '-NoProfile -ExecutionPolicy Bypass -Command "Remove-Item -Path \"' + TempExtract + '\" -Recurse -Force -ErrorAction SilentlyContinue"',
    'ffmpeg_cleanup', ExtractTimeoutMs);

  SetBusyProgress(False);

  if not IsFFmpegInstalled then
  begin
    WriteLog('[FFmpeg] ERROR: Post-install verification failed - ffmpeg.exe/ffprobe.exe missing from ' + FFmpegBinDir);
    Exit;
  end;

  WriteLog('[FFmpeg] Installation complete and verified.');
  Result := True;
end;

function InstallVAC: Boolean;
var
  TempExe: String;
  InstallExitCode: Integer;
begin
  Result := False;
  if UserCancelled then
    Exit;
  TempExe := ExpandConstant('{tmp}\vac-setup.exe');

  WriteLog('[VAC] Not found. Downloading...');
  UpdateStatus('Downloading Virtual Audio Capturer...');
  SetBusyProgress(True);

  if not DownloadFile(VACDownloadUrl, TempExe, 'VAC') then
  begin
    SetBusyProgress(False);
    Exit;
  end;

  UpdateStatus('Installing Virtual Audio Capturer (silent)...');
  InstallExitCode := RunHiddenAsync(TempExe, '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-', 'vac_install', InstallTimeoutMs);

  if InstallExitCode <> 0 then
  begin
    WriteLog('[VAC] ERROR: Install failed (exit code ' + IntToStr(InstallExitCode) + ').');
    SetBusyProgress(False);
    Exit;
  end;

  WriteLog('[VAC] Cleaning up installer windows...');
  KillVACProcesses;
  DeleteFile(TempExe);
  SetBusyProgress(False);

  if not IsVACInstalled then
  begin
    WriteLog('[VAC] ERROR: Post-install verification failed - VAC does not appear to be registered.');
    Exit;
  end;

  WriteLog('[VAC] Installation complete and verified.');
  Result := True;
end;

{ Ensures FFmpeg is present, downloading/installing only if missing.
  Returns False only if the user chose to abort after a failure. }
function EnsureFFmpeg: Boolean;
begin
  Result := True;
  if UserCancelled then
  begin
    Result := False;
    Exit;
  end;
  UpdateStatus('Checking for FFmpeg...');

  if IsFFmpegInstalled then
  begin
    WriteLog('[FFmpeg] Already installed. Skipping download.');
    if not IsFFmpegInPath then
    begin
      WriteLog('[FFmpeg] Not in PATH. Adding...');
      AddToSystemPath(FFmpegBinDir);
      BroadcastEnvironmentChange;
    end
    else
      WriteLog('[FFmpeg] Already in PATH. Skipping.');
    Exit;
  end;

  if not InstallFFmpeg then
  begin
    if not PromptContinueWithoutComponent('FFmpeg') then
      Result := False;
  end;
end;

{ Ensures the Virtual Audio Capturer is present, downloading/installing
  only if missing. Returns False only if the user chose to abort. }
function EnsureVAC: Boolean;
begin
  Result := True;
  if UserCancelled then
  begin
    Result := False;
    Exit;
  end;
  UpdateStatus('Checking for Virtual Audio Capturer...');

  if IsVACInstalled then
  begin
    WriteLog('[VAC] Already installed. Skipping download.');
    Exit;
  end;

  if not InstallVAC then
  begin
    if not PromptContinueWithoutComponent('Virtual Audio Capturer') then
      Result := False;
  end;
end;

{ ---------------------------------------------------------------------- }
{  Uninstall helpers                                                     }
{ ---------------------------------------------------------------------- }

procedure DeinstallFFmpeg;
var
  ResultCode: Integer;
begin
  if not IsFFmpegInstalled then
  begin
    WriteLog('[Uninstall] FFmpeg not present under ' + FFmpegBinDir + '. Skipping.');
    Exit;
  end;

  WriteLog('[Uninstall] Removing FFmpeg from PATH...');
  RemoveFromSystemPath(FFmpegBinDir);
  BroadcastEnvironmentChange;

  WriteLog('[Uninstall] Deleting ' + FFmpegBinDir + '...');
  Exec('cmd.exe', '/c rmdir /s /q "' + FFmpegBinDir + '"', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  WriteLog('[Uninstall] FFmpeg removed.');
end;

procedure DeinstallVAC;
var
  UninstallString: String;
  ResultCode: Integer;
begin
  if not IsVACInstalled then
  begin
    WriteLog('[Uninstall] Virtual Audio Capturer not present. Skipping.');
    Exit;
  end;

  WriteLog('[Uninstall] Uninstalling Virtual Audio Capturer...');
  UninstallString := '';

  if RegQueryStringValue(HKLM, VACUninstallKey, 'UninstallString', UninstallString) or
     RegQueryStringValue(HKLM, VACUninstallKeyWow64, 'UninstallString', UninstallString) then
  begin
    Exec(UninstallString, '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
    WriteLog('[Uninstall] VAC uninstalled.');
  end
  else
    WriteLog('[Uninstall] VAC uninstaller not found in registry.');
end;

{ ---------------------------------------------------------------------- }
{  Setup event hooks                                                     }
{ ---------------------------------------------------------------------- }

procedure WizardFormCloseQuery(Sender: TObject; var CanClose: Boolean);
begin
  UserCancelled := True;
  CanClose := True;
end;

procedure InitializeWizard;
begin
  LogStrings := TStringList.Create;
  UserCancelled := False;
  WizardForm.OnCloseQuery := @WizardFormCloseQuery;
end;

procedure CurStepChanged(Step: TSetupStep);
var
  InstallPath, ExePath: String;
begin
  if Step = ssInstall then
  begin
    InstallPath := ExpandConstant('{app}');
    ExePath := InstallPath + '\InstantReplay.exe';

    WriteLog('');
    WriteLog('========================================');
    WriteLog('  Instant Replay - Installation Log');
    WriteLog('========================================');
    WriteLog('Install destination: ' + InstallPath);
    WriteLog('');

    { Step 1: dependencies - each checked first, downloaded/installed only
      if missing (non-blocking), then verified. }
    if not EnsureFFmpeg then
    begin
      WriteLog('[Setup] Installation cancelled by user after FFmpeg failure.');
      Abort;
    end;

    if not EnsureVAC then
    begin
      WriteLog('[Setup] Installation cancelled by user after VAC failure.');
      Abort;
    end;

    { Step 2: Windows security exclusions }
    UpdateStatus('Configuring Windows security...');
    if not IsDefenderExclusionAdded(InstallPath) then
    begin
      WriteLog('[Security] Adding Windows Defender exclusion for ' + InstallPath);
      AddDefenderExclusion(InstallPath);
    end
    else
      WriteLog('[Security] Windows Defender exclusion already exists. Skipping.');

    if not IsCFAExclusionAdded(ExePath) then
    begin
      WriteLog('[Security] Adding Controlled Folder Access exclusion for ' + ExePath);
      AddCFAExclusion(ExePath);
    end
    else
      WriteLog('[Security] Controlled Folder Access exclusion already exists. Skipping.');

    WriteLog('');
    WriteLog('========================================');
    WriteLog('  Installation Complete');
    WriteLog('========================================');

    UpdateStatus('Installation complete!');

    SaveStringToFile(InstallPath + '\install_log.txt', LogStrings.Text, False);
    WriteLog('Log saved to: ' + InstallPath + '\install_log.txt');
  end;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  InstallPath, ExePath: String;
begin
  if CurUninstallStep = usUninstall then
  begin
    if LogStrings = nil then
      LogStrings := TStringList.Create;

    InstallPath := ExpandConstant('{app}');
    ExePath := InstallPath + '\InstantReplay.exe';

    WriteLog('');
    WriteLog('========================================');
    WriteLog('  Instant Replay - Uninstall Log');
    WriteLog('========================================');

    DeinstallFFmpeg;
    DeinstallVAC;

    WriteLog('[Uninstall] Removing Windows Defender exclusion...');
    RemoveDefenderExclusion(InstallPath);

    WriteLog('[Uninstall] Removing Controlled Folder Access exclusion...');
    RemoveCFAExclusion(ExePath);

    WriteLog('');
    WriteLog('========================================');
    WriteLog('  Uninstall Complete');
    WriteLog('========================================');
  end;
end;

procedure DeinitializeSetup;
begin
  if LogStrings <> nil then
    LogStrings.Free;
end;
