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

var
  LogStrings: TStringList;

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

function IsVACInstalled: Boolean;
begin
  Result := FileExists('C:\Program Files\ScreenCapturerRecorder\ScreenCapturerRecorder.exe') or
            FileExists('C:\Program Files (x86)\ScreenCapturerRecorder\ScreenCapturerRecorder.exe');
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

function InstallFFmpeg: Boolean;
var
  TempZip, TempExtract: String;
  ResultCode: Integer;
begin
  Result := False;
  TempZip := ExpandConstant('{tmp}\ffmpeg.zip');
  TempExtract := ExpandConstant('{tmp}\ffmpeg_extract');

  WriteLog('[FFmpeg] Not found. Downloading...');
  UpdateStatus('Downloading FFmpeg...');

  if not Exec('curl.exe',
    '--connect-timeout 30 --retry 3 -L -s -o "' + TempZip + '" "' + FFmpegDownloadUrl + '"',
    '', SW_HIDE, ewWaitUntilTerminated, ResultCode) or (ResultCode <> 0) then
  begin
    WriteLog('[FFmpeg] ERROR: Download failed (exit code ' + IntToStr(ResultCode) + ')');
    Exit;
  end;
  WriteLog('[FFmpeg] Download complete.');

  UpdateStatus('Extracting FFmpeg...');
  if not Exec('powershell.exe',
    '-NoProfile -ExecutionPolicy Bypass -Command "Expand-Archive -Path ''' + TempZip + ''' -DestinationPath ''' + TempExtract + ''' -Force"',
    '', SW_HIDE, ewWaitUntilTerminated, ResultCode) or (ResultCode <> 0) then
  begin
    WriteLog('[FFmpeg] ERROR: Extraction failed');
    Exit;
  end;
  WriteLog('[FFmpeg] Extraction complete.');

  UpdateStatus('Installing FFmpeg...');
  Exec('cmd.exe', '/c if not exist "C:\ffmpeg" mkdir "C:\ffmpeg"', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);

  WriteLog('[FFmpeg] Copying ffmpeg.exe, ffprobe.exe -> C:\ffmpeg\');
  if not Exec('powershell.exe',
    '-NoProfile -ExecutionPolicy Bypass -Command "Copy-Item -Path ''' + TempExtract + '\ffmpeg-master-latest-win64-gpl\bin\ffmpeg.exe'' -Destination ''' + FFmpegBinDir + '\ffmpeg.exe'' -Force; Copy-Item -Path ''' + TempExtract + '\ffmpeg-master-latest-win64-gpl\bin\ffprobe.exe'' -Destination ''' + FFmpegBinDir + '\ffprobe.exe'' -Force"',
    '', SW_HIDE, ewWaitUntilTerminated, ResultCode) or (ResultCode <> 0) then
  begin
    WriteLog('[FFmpeg] ERROR: Copy failed');
    Exit;
  end;

  UpdateStatus('Adding FFmpeg to system PATH...');
  AddToSystemPath(FFmpegBinDir);

  WriteLog('[FFmpeg] Broadcasting environment change...');
  Exec('powershell.exe',
    '-NoProfile -ExecutionPolicy Bypass -Command "$sig = [System.Runtime.InteropServices.Marshal]; $HWND_BROADCAST = [IntPtr]0xffff; $WM_SETTINGCHANGE = 0x001A; $sig::SendMessage($HWND_BROADCAST, $WM_SETTINGCHANGE, [IntPtr]::Zero, [IntPtr]::Zero)"',
    '', SW_HIDE, ewWaitUntilTerminated, ResultCode);

  WriteLog('[FFmpeg] Cleaning up temp files...');
  Exec('cmd.exe', '/c del /f /q "' + TempZip + '"', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Exec('powershell.exe', '-NoProfile -ExecutionPolicy Bypass -Command "Remove-Item -Path ''' + TempExtract + ''' -Recurse -Force -ErrorAction SilentlyContinue"', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);

  WriteLog('[FFmpeg] Installation complete.');
  Result := True;
end;

function InstallVAC: Boolean;
var
  TempExe: String;
  ResultCode: Integer;
begin
  Result := False;
  TempExe := ExpandConstant('{tmp}\vac-setup.exe');

  WriteLog('[VAC] Not found. Downloading...');
  UpdateStatus('Downloading Virtual Audio Capturer...');

  if not Exec('curl.exe',
    '--connect-timeout 30 --retry 3 -L -s -o "' + TempExe + '" "' + VACDownloadUrl + '"',
    '', SW_HIDE, ewWaitUntilTerminated, ResultCode) or (ResultCode <> 0) then
  begin
    WriteLog('[VAC] ERROR: Download failed (exit code ' + IntToStr(ResultCode) + ')');
    Exit;
  end;
  WriteLog('[VAC] Download complete.');

  UpdateStatus('Installing Virtual Audio Capturer (silent)...');
  if not Exec(TempExe, '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP-', '', SW_HIDE, ewWaitUntilTerminated, ResultCode) or (ResultCode <> 0) then
  begin
    WriteLog('[VAC] ERROR: Install failed (exit code ' + IntToStr(ResultCode) + ')');
    Exit;
  end;

  WriteLog('[VAC] Cleaning up installer windows...');
  KillVACProcesses;

  WriteLog('[VAC] Installation complete.');
  Result := True;
end;

procedure DeinstallFFmpeg;
var
  ResultCode: Integer;
begin
  WriteLog('[Uninstall] Removing FFmpeg from PATH...');
  RemoveFromSystemPath(FFmpegBinDir);

  WriteLog('[Uninstall] Broadcasting environment change...');
  Exec('powershell.exe',
    '-NoProfile -ExecutionPolicy Bypass -Command "$sig = [System.Runtime.InteropServices.Marshal]; $HWND_BROADCAST = [IntPtr]0xffff; $WM_SETTINGCHANGE = 0x001A; $sig::SendMessage($HWND_BROADCAST, $WM_SETTINGCHANGE, [IntPtr]::Zero, [IntPtr]::Zero)"',
    '', SW_HIDE, ewWaitUntilTerminated, ResultCode);

  WriteLog('[Uninstall] Deleting C:\ffmpeg...');
  Exec('cmd.exe', '/c rmdir /s /q "C:\ffmpeg"', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  WriteLog('[Uninstall] FFmpeg removed.');
end;

procedure DeinstallVAC;
var
  UninstallString: String;
  ResultCode: Integer;
begin
  WriteLog('[Uninstall] Uninstalling Virtual Audio Capturer...');
  UninstallString := '';

  if RegQueryStringValue(HKLM, 'SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\ScreenCapturerRecorder_is1', 'UninstallString', UninstallString) then
  begin
    Exec(UninstallString, '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
    WriteLog('[Uninstall] VAC uninstalled.');
  end
  else if RegQueryStringValue(HKLM, 'SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\ScreenCapturerRecorder_is1', 'UninstallString', UninstallString) then
  begin
    Exec(UninstallString, '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
    WriteLog('[Uninstall] VAC uninstalled.');
  end
  else
    WriteLog('[Uninstall] VAC uninstaller not found.');
end;

procedure InitializeWizard;
begin
  LogStrings := TStringList.Create;
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

    UpdateStatus('Checking for FFmpeg...');
    if IsFFmpegInstalled then
    begin
      WriteLog('[FFmpeg] Already installed.');
      if not IsFFmpegInPath then
      begin
        WriteLog('[FFmpeg] Not in PATH. Adding...');
        AddToSystemPath(FFmpegBinDir);
      end
      else
        WriteLog('[FFmpeg] Already in PATH. Skipping.');
    end
    else
    begin
      if not InstallFFmpeg then
        RaiseException('Failed to install FFmpeg. Installation aborted.');
    end;

    UpdateStatus('Checking for Virtual Audio Capturer...');
    if IsVACInstalled then
    begin
      WriteLog('[VAC] Already installed. Skipping.');
    end
    else
    begin
      if not InstallVAC then
        RaiseException('Failed to install Virtual Audio Capturer. Installation aborted.');
    end;

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

    WizardForm.StatusLabel.Caption := 'Installation complete!';
    WizardForm.Refresh;

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
