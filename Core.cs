using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;

namespace InstantReplay;

[ComImport, Guid("BCDE0395-E52F-467C-8E3D-C4579291692E")]
internal class MMDeviceEnumeratorComObject
{
}

[ComImport, Guid("A95664D2-9614-4F35-A746-DE8DB63617E6"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
internal interface IMMDeviceEnumerator
{
    int EnumAudioEndpoints(int dataFlow, int stateMask, out IntPtr devices);
    int GetDefaultAudioEndpoint(int dataFlow, int role, out IntPtr endpoint);
    int GetDevice([MarshalAs(UnmanagedType.LPWStr)] string id, out IntPtr device);
    int RegisterEndpointNotificationCallback(IMMNotificationClient client);
    int UnregisterEndpointNotificationCallback(IMMNotificationClient client);
}

[StructLayout(LayoutKind.Sequential)]
internal struct PropertyKey
{
    public Guid FormatId;
    public int PropertyId;
}

[ComImport, Guid("7991EEC9-7E89-4D85-8390-6C703CEC60C0"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
internal interface IMMNotificationClient
{
    void OnDeviceStateChanged([MarshalAs(UnmanagedType.LPWStr)] string deviceId, int newState);
    void OnDeviceAdded([MarshalAs(UnmanagedType.LPWStr)] string deviceId);
    void OnDeviceRemoved([MarshalAs(UnmanagedType.LPWStr)] string deviceId);
    void OnDefaultDeviceChanged(int flow, int role, [MarshalAs(UnmanagedType.LPWStr)] string deviceId);
    void OnPropertyValueChanged([MarshalAs(UnmanagedType.LPWStr)] string deviceId, PropertyKey key);
}

public sealed class AudioDeviceWatcher : IMMNotificationClient, IDisposable
{
    public event Action? DevicesChanged;

    private IMMDeviceEnumerator? _enumerator;
    private bool _registered;

    public AudioDeviceWatcher()
    {
        try
        {
            _enumerator = (IMMDeviceEnumerator)new MMDeviceEnumeratorComObject();
            _enumerator.RegisterEndpointNotificationCallback(this);
            _registered = true;
        }
        catch (Exception ex)
        {
            Logger.Warn($"AudioDeviceWatcher: could not register for device-change notifications, falling back to periodic polling only: {ex.Message}");
        }
    }

    void IMMNotificationClient.OnDeviceStateChanged(string deviceId, int newState) => DevicesChanged?.Invoke();
    void IMMNotificationClient.OnDeviceAdded(string deviceId) => DevicesChanged?.Invoke();
    void IMMNotificationClient.OnDeviceRemoved(string deviceId) => DevicesChanged?.Invoke();
    void IMMNotificationClient.OnDefaultDeviceChanged(int flow, int role, string deviceId) { }
    void IMMNotificationClient.OnPropertyValueChanged(string deviceId, PropertyKey key) { }

    public void Dispose()
    {
        try
        {
            if (_registered && _enumerator != null)
                _enumerator.UnregisterEndpointNotificationCallback(this);
        }
        catch (Exception ex)
        {
            Logger.Warn($"AudioDeviceWatcher: could not unregister cleanly: {ex.Message}");
        }
        if (_enumerator != null && Marshal.IsComObject(_enumerator))
            Marshal.ReleaseComObject(_enumerator);
        _enumerator = null;
    }
}

public static class WindowsTheme
{
    public static (byte R, byte G, byte B) GetAccentColor()
    {
        try
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.OpenSubKey(@"Software\Microsoft\Windows\DWM");
            if (key?.GetValue("AccentColor") is int argb)
            {
                uint v = unchecked((uint)argb);
                byte r = (byte)(v & 0xFF);
                byte g = (byte)((v >> 8) & 0xFF);
                byte b = (byte)((v >> 16) & 0xFF);
                return (r, g, b);
            }
        }
        catch (Exception ex)
        {
            Logger.Warn($"WindowsTheme: could not read system accent color, using default: {ex.Message}");
        }
        return (0x8B, 0x5F, 0xBF);
    }

    public static bool IsSystemDarkMode()
    {
        try
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.OpenSubKey(
                @"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
            if (key?.GetValue("AppsUseLightTheme") is int lightMode)
                return lightMode == 0;
        }
        catch (Exception ex)
        {
            Logger.Warn($"WindowsTheme: could not read system light/dark mode, defaulting to dark: {ex.Message}");
        }
        return true;
    }

    public static bool IsTransparencyEnabled()
    {
        try
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.OpenSubKey(
                @"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
            if (key?.GetValue("EnableTransparency") is int value)
                return value == 1;
        }
        catch (Exception ex)
        {
            Logger.Warn($"WindowsTheme: could not read transparency setting, defaulting to enabled: {ex.Message}");
        }
        return true;
    }
}

public sealed class Settings
{
    public string FFmpegPath { get; set; } = "ffmpeg.exe";
    public string Microphone { get; set; } = "";
    public int ReplayDurationSeconds { get; set; } = 60;
    public int FrameRate { get; set; } = 30;
    public int SystemAudioOffsetMs { get; set; } = 0;
    public int MicOffsetMs { get; set; } = 0;
    public bool DebugLogging { get; set; } = false;

    public bool WindowTransparency { get; set; } = true;

    public bool DarkMode { get; set; } = true;

    public uint HotkeyModifiers { get; set; } = 0x0002;
    public uint HotkeyVk { get; set; } = 0x6A;

    public string QualityPreset { get; set; } = "Fast";

    public string SaveDestination { get; set; } = "";

    public string GetEffectiveSaveDestination() =>
        string.IsNullOrWhiteSpace(SaveDestination)
            ? Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.MyPictures), "Instant Replay")
            : SaveDestination;

    private static string PathOnDisk => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
        "InstantReplay", "settings.json");

    public static Settings Load()
    {
        try
        {
            string path = PathOnDisk;
            if (File.Exists(path))
            {
                var loaded = JsonSerializer.Deserialize<Settings>(File.ReadAllText(path));
                if (loaded != null) return loaded;
            }
        }
        catch
        {
        }
        return new Settings { DarkMode = WindowsTheme.IsSystemDarkMode() };
    }

    public void Save()
    {
        try
        {
            string path = PathOnDisk;
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            File.WriteAllText(path, JsonSerializer.Serialize(this, new JsonSerializerOptions { WriteIndented = true }));
        }
        catch (Exception ex)
        {
            Logger.Error("Settings.Save failed", ex);
        }
    }
}

public static class Logger
{
    private static readonly object s_lock = new();
    private static StreamWriter? s_writer;
    private static bool s_debugMode;

    public static string LogPath { get; private set; } = "";

    public static void Init(bool debugMode)
    {
        lock (s_lock)
        {
            s_debugMode = debugMode;
            string fileName = $"debug_{DateTime.Now:yyyyMMdd_HHmmss}.txt";
            LogPath = Path.Combine(Path.GetTempPath(), fileName);
            s_writer = new StreamWriter(LogPath, append: true, Encoding.UTF8) { AutoFlush = true };
        }
        Info("Application Started");
        Info("Version: 1.0.0");
    }

    public static void SetDebugMode(bool enabled) => s_debugMode = enabled;

    public static void Info(string message) => Write("INFO", message);
    public static void Warn(string message) => Write("WARN", message);

    public static void Error(string message, Exception? ex = null) =>
        Write("ERROR", ex is null ? message : $"{message}\n{ex}");

    public static void Debug(string message)
    {
        if (s_debugMode) Write("DEBUG", message);
    }

    private static void Write(string level, string message)
    {
        lock (s_lock)
        {
            s_writer?.WriteLine($"[{DateTime.Now:HH:mm:ss.fff}] [{level}] {message}");
        }
    }

    public static void Shutdown()
    {
        lock (s_lock)
        {
            s_writer?.Flush();
            s_writer?.Dispose();
            s_writer = null;
        }
    }
}

public enum SaveReplayOutcome
{
    Success,
    NotRecording,
    NoSegments,
    WriteBlocked,
    MergeFailed,
}

public readonly struct SaveReplayResult
{
    public SaveReplayOutcome Outcome { get; }
    public string? Path { get; }
    public bool Success => Outcome == SaveReplayOutcome.Success;

    public SaveReplayResult(SaveReplayOutcome outcome, string? path = null)
    {
        Outcome = outcome;
        Path = path;
    }

    public string UserMessage() => Outcome switch
    {
        SaveReplayOutcome.Success => $"Replay saved to {Path}",
        SaveReplayOutcome.NotRecording => "Recorder isn't running — try starting capture first.",
        SaveReplayOutcome.NoSegments => "Still buffering — nothing to save yet. Try again in a few seconds.",
        SaveReplayOutcome.WriteBlocked =>
            "Save blocked by Windows security. Open Windows Security → Virus & threat protection → " +
            "Manage ransomware protection → Controlled folder access, and allow ffmpeg.exe (and Instant " +
            "Replay.exe) through it, then try again.",
        SaveReplayOutcome.MergeFailed => "Failed to save replay (see log for details).",
        _ => "Failed to save replay (see log for details).",
    };
}

public sealed class RecorderEngine : IDisposable
{
    private const int SegmentSeconds = 1;
    private const string SegmentBaseName = "seg";
    private const string SegmentExt = ".ts";

    private const int WatchdogRestartMinutes = 60;
    private const int WatchdogPollSeconds = 5;

    private readonly object _lock = new();
    private readonly object _opLock = new();
    private readonly Settings _settings;

    private Process? _process;
    private DateTime _startTimeUtc;
    private string _segmentDir = "";
    private int _segmentWrapCount;
    private CancellationTokenSource? _watchdogCts;

    public RecorderEngine(Settings settings) => _settings = settings;

    public bool IsRunning
    {
        get { lock (_lock) return _process is { HasExited: false }; }
    }

    private const int StaleSegmentThresholdSeconds = 5;

    public bool IsBufferHealthy()
    {
        string dir;
        DateTime startTimeUtc;
        lock (_lock)
        {
            if (_process is not { HasExited: false }) return false;
            dir = _segmentDir;
            startTimeUtc = _startTimeUtc;
        }

        if (string.IsNullOrEmpty(dir)) return false;

        try
        {
            DateTime newest = DateTime.MinValue;
            foreach (var path in Directory.EnumerateFiles(dir, SegmentBaseName + "*" + SegmentExt))
            {
                var written = File.GetLastWriteTimeUtc(path);
                if (written > newest) newest = written;
            }

            DateTime reference = newest == DateTime.MinValue ? startTimeUtc : newest;
            return (DateTime.UtcNow - reference).TotalSeconds < StaleSegmentThresholdSeconds;
        }
        catch
        {
            return true;
        }
    }

    public List<string> EnumerateMicrophones()
    {
        var result = new List<string>();
        if (string.IsNullOrEmpty(_settings.FFmpegPath))
            return result;

        var psi = new ProcessStartInfo
        {
            FileName = _settings.FFmpegPath,
            Arguments = "-list_devices true -f dshow -i dummy",
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardError = true,
        };

        string output = "";
        try
        {
            using var proc = Process.Start(psi);
            if (proc is null) return result;

            Task<string> readTask = proc.StandardError.ReadToEndAsync();
            if (!proc.WaitForExit(5000))
                proc.Kill(entireProcessTree: true);
            output = readTask.Wait(2000) ? readTask.Result : "";
        }
        catch (Exception ex)
        {
            Logger.Warn($"EnumerateMicrophones failed: {ex.Message}");
            return result;
        }

        bool inAudioSection = false;
        foreach (string rawLine in output.Split('\n'))
        {
            string line = rawLine.TrimEnd('\r');

            if (line.Contains("DirectShow audio devices")) { inAudioSection = true; continue; }
            if (line.Contains("DirectShow video devices")) { inAudioSection = false; continue; }

            bool taggedAudio = line.Contains("(audio)");
            bool taggedVideo = line.Contains("(video)");
            if (!taggedAudio && !(inAudioSection && !taggedVideo)) continue;

            int q1 = line.IndexOf('"');
            if (q1 < 0) continue;
            int q2 = line.IndexOf('"', q1 + 1);
            if (q2 < 0) continue;

            string name = line.Substring(q1 + 1, q2 - q1 - 1);
            if (name == "virtual-audio-capturer") continue;

            result.Add(name);
        }

        return result;
    }

    private static (string Preset, int Crf) ResolveQuality(string qualityPreset) => qualityPreset switch
    {
        "Quality" => ("medium", 18),
        "Balanced" => ("faster", 20),
        _ => ("ultrafast", 23),
    };

    public bool Start(string tempDir)
    {
        lock (_opLock)
        {
            lock (_lock)
            {
                if (_process is { HasExited: false })
                {
                    Logger.Warn("Start() called while already running; ignoring.");
                    return true;
                }
            }

            string segmentDir = tempDir;
            Directory.CreateDirectory(segmentDir);
            DeleteOldSegments(segmentDir);

            int segmentWrapCount = Math.Max(_settings.ReplayDurationSeconds / SegmentSeconds, 1);

            string mic = _settings.Microphone;
            if (!string.IsNullOrEmpty(mic))
            {
                var mics = EnumerateMicrophones();
                if (!mics.Contains(mic))
                {
                    Logger.Warn($"Configured microphone '{mic}' not found; falling back to desktop audio only.");
                    mic = "";
                }
            }

            string outputPattern = Path.Combine(segmentDir, SegmentBaseName + "%d" + SegmentExt);

            string filterComplex = $"ddagrab=framerate={_settings.FrameRate},hwdownload,format=bgra[v]";
            string audioMap = "0:a";
            if (!string.IsNullOrEmpty(mic))
            {
                filterComplex += ";[0:a][1:a]amix=inputs=2:duration=longest[a]";
                audioMap = "[a]";
            }

            var (encPreset, crf) = ResolveQuality(_settings.QualityPreset);

            var args = new StringBuilder();
            args.Append("-y -nostats -loglevel warning ");
            args.Append($"-filter_complex \"{filterComplex}\" ");
            args.Append($"-thread_queue_size 1024 -itsoffset {MsToOffset(_settings.SystemAudioOffsetMs)} -audio_buffer_size 50 -f dshow -i audio=\"virtual-audio-capturer\" ");
            if (!string.IsNullOrEmpty(mic))
                args.Append($"-thread_queue_size 1024 -itsoffset {MsToOffset(_settings.MicOffsetMs)} -audio_buffer_size 50 -f dshow -i audio=\"{mic}\" ");
            args.Append($"-map \"[v]\" -map \"{audioMap}\" ");
            args.Append($"-c:v libx264 -preset {encPreset} -crf {crf} -pix_fmt yuv420p ");
            args.Append($"-force_key_frames \"expr:gte(t,n_forced*{SegmentSeconds})\" -sc_threshold 0 ");
            args.Append("-c:a aac -b:a 160k -ar 44100 -ac 2 ");
            args.Append($"-f segment -segment_time {SegmentSeconds} -segment_wrap {segmentWrapCount} " +
                        $"-segment_format mpegts -reset_timestamps 1 \"{outputPattern}\"");

            var psi = new ProcessStartInfo
            {
                FileName = _settings.FFmpegPath,
                Arguments = args.ToString(),
                UseShellExecute = false,
                CreateNoWindow = true,
                RedirectStandardInput = true,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
            };

            var proc = new Process { StartInfo = psi, EnableRaisingEvents = true };
            const int StderrTailMaxLines = 40;
            var stderrTail = new Queue<string>(StderrTailMaxLines);
            var stderrLock = new object();
            proc.OutputDataReceived += (_, e) => { if (e.Data != null) Logger.Debug($"ffmpeg stdout: {e.Data}"); };
            proc.ErrorDataReceived += (_, e) =>
            {
                if (e.Data == null) return;
                Logger.Debug($"ffmpeg stderr: {e.Data}");
                lock (stderrLock)
                {
                    if (stderrTail.Count == StderrTailMaxLines) stderrTail.Dequeue();
                    stderrTail.Enqueue(e.Data);
                }
            };

            try
            {
                if (!proc.Start())
                {
                    Logger.Error("Could not start ffmpeg for video+audio capture.");
                    return false;
                }
            }
            catch (Exception ex)
            {
                Logger.Error("Exception starting ffmpeg", ex);
                return false;
            }

            proc.BeginOutputReadLine();
            proc.BeginErrorReadLine();

            Thread.Sleep(750);
            if (proc.HasExited)
            {
                string tail;
                lock (stderrLock) tail = string.Join("\n", stderrTail);
                Logger.Error($"ffmpeg exited immediately (code {proc.ExitCode}). Check the FFmpeg path and microphone setting.\n" +
                             $"--- ffmpeg stderr (last {StderrTailMaxLines} lines) ---\n{tail}");
                return false;
            }

            lock (_lock)
            {
                _process = proc;
                _startTimeUtc = DateTime.UtcNow;
                _segmentDir = segmentDir;
                _segmentWrapCount = segmentWrapCount;
            }

            Logger.Info($"Video Capture Started - Source: ddagrab, FPS: {_settings.FrameRate}");
            Logger.Info($"Audio Capture Started - Device: virtual-audio-capturer" + (string.IsNullOrEmpty(mic) ? "" : $" + {mic}"));

            StartWatchdog(tempDir);
            return true;
        }
    }

    public void Stop()
    {
        lock (_opLock)
        {
            _watchdogCts?.Cancel();
            _watchdogCts = null;

            Process? proc;
            lock (_lock)
            {
                proc = _process;
                _process = null;
            }
            if (proc is null) return;

            try
            {
                if (!proc.HasExited)
                {
                    proc.StandardInput.Write("q\r\n");
                    proc.StandardInput.Flush();
                }
            }
            catch { }

            try
            {
                if (!proc.WaitForExit(3000))
                    proc.Kill(entireProcessTree: true);
            }
            catch { }

            proc.Dispose();
            Logger.Info("Recording stopped.");
        }
    }

    public bool Restart(string tempDir)
    {
        Stop();
        return Start(tempDir);
    }

    private void StartWatchdog(string tempDir)
    {
        _watchdogCts = new CancellationTokenSource();
        var token = _watchdogCts.Token;

        _ = Task.Run(async () =>
        {
            while (!token.IsCancellationRequested)
            {
                try { await Task.Delay(TimeSpan.FromSeconds(WatchdogPollSeconds), token); }
                catch (TaskCanceledException) { break; }

                try
                {
                    bool needsRestart;
                    lock (_lock)
                    {
                        if (_process is null)
                        {
                            needsRestart = false;
                        }
                        else if (_process.HasExited)
                        {
                            Logger.Warn($"ffmpeg exited unexpectedly (code {_process.ExitCode}); restarting capture.");
                            needsRestart = true;
                        }
                        else if (DateTime.UtcNow - _startTimeUtc > TimeSpan.FromMinutes(WatchdogRestartMinutes))
                        {
                            Logger.Info("Watchdog: periodic restart interval reached; recycling ffmpeg process.");
                            needsRestart = true;
                        }
                        else
                        {
                            needsRestart = false;
                        }
                    }

                    if (needsRestart && !token.IsCancellationRequested)
                        Restart(tempDir);
                }
                catch (Exception ex)
                {
                    Logger.Error("Watchdog iteration failed unexpectedly; continuing to poll.", ex);
                }
            }
        }, token);
    }

    public SaveReplayResult SaveReplay(string tempDir, string destDir)
    {
        lock (_opLock)
        {
            return SaveReplayCore(tempDir, destDir);
        }
    }

    private SaveReplayResult SaveReplayCore(string tempDir, string destDir)
    {
        Process? proc;
        lock (_lock)
        {
            proc = _process;
        }

        if (proc is null || proc.HasExited)
        {
            Logger.Warn("SaveReplay requested but recorder is not running.");
            return new SaveReplayResult(SaveReplayOutcome.NotRecording);
        }

        const int ActiveMargin = 2;

        List<(string Path, DateTime Written, long Length)> allSegments;
        try
        {
            allSegments = Directory.EnumerateFiles(tempDir, SegmentBaseName + "*" + SegmentExt)
                .Select(p => (Path: p, Written: File.GetLastWriteTimeUtc(p), Length: new FileInfo(p).Length))
                .ToList();
        }
        catch (Exception ex)
        {
            Logger.Error("SaveReplay: failed enumerating segments", ex);
            return new SaveReplayResult(SaveReplayOutcome.MergeFailed);
        }

        if (allSegments.Count == 0)
        {
            Logger.Warn("SaveReplay: no closed segments found yet (nothing recorded this session?).");
            return new SaveReplayResult(SaveReplayOutcome.NoSegments);
        }

        allSegments.Sort((a, b) => a.Written.CompareTo(b.Written));

        var segments = allSegments
            .Take(Math.Max(allSegments.Count - ActiveMargin, 0))
            .Where(s => s.Length > 0)
            .Select(s => (s.Path, s.Written))
            .ToList();

        if (segments.Count == 0)
        {
            Logger.Warn("SaveReplay: no closed segments found yet (nothing recorded this session?).");
            return new SaveReplayResult(SaveReplayOutcome.NoSegments);
        }

        string listPath = Path.Combine(tempDir, "concat_list.txt");
        try
        {
            var sb = new StringBuilder();
            foreach (var seg in segments)
                sb.Append($"file '{seg.Path.Replace("'", "'\\''")}'\n");

            File.WriteAllText(listPath, sb.ToString(), new UTF8Encoding(encoderShouldEmitUTF8Identifier: false));
        }
        catch (Exception ex)
        {
            Logger.Error("SaveReplay: failed writing concat list", ex);
            return new SaveReplayResult(SaveReplayOutcome.MergeFailed);
        }

        string saveFolder = Path.Combine(destDir, $"replay_{DateTime.Now:yyyyMMdd_HHmmss}");

        try
        {
            Directory.CreateDirectory(saveFolder);
        }
        catch (Exception ex)
        {
            Logger.Error("SaveReplay: failed creating save folder", ex);
            try { File.Delete(listPath); } catch { }
            return ProbeWriteBlocked(destDir, ex);
        }

        string destMp4 = Path.Combine(saveFolder, "replay.mp4");
        string args = $"-y -f concat -safe 0 -i \"{listPath}\" -c copy \"{destMp4}\"";

        try
        {
            var psi = new ProcessStartInfo
            {
                FileName = _settings.FFmpegPath,
                Arguments = args,
                UseShellExecute = false,
                CreateNoWindow = true,
                RedirectStandardError = true,
            };

            using var merge = Process.Start(psi);
            if (merge is null)
            {
                CleanupFailedSaveFolder(saveFolder);
                return new SaveReplayResult(SaveReplayOutcome.MergeFailed);
            }

            merge.ErrorDataReceived += (_, e) => { if (e.Data != null) Logger.Debug($"ffmpeg (merge) stderr: {e.Data}"); };
            merge.BeginErrorReadLine();

            if (!merge.WaitForExit(15000))
            {
                merge.Kill(entireProcessTree: true);
                Logger.Error("SaveReplay: merge process timed out.");
                CleanupFailedSaveFolder(saveFolder);
                return new SaveReplayResult(SaveReplayOutcome.MergeFailed);
            }

            bool ok = merge.ExitCode == 0 && File.Exists(destMp4);
            if (ok)
            {
                Logger.Info($"Replay Saved - Path: {destMp4}");
                return new SaveReplayResult(SaveReplayOutcome.Success, destMp4);
            }

            Logger.Info("SaveReplay: merge failed.");

            var probeResult = ProbeWriteBlocked(saveFolder, null);
            CleanupFailedSaveFolder(saveFolder);
            return probeResult;
        }
        catch (Exception ex)
        {
            Logger.Error("SaveReplay: exception launching merge ffmpeg", ex);
            var probeResult = ProbeWriteBlocked(destDir, ex);
            CleanupFailedSaveFolder(saveFolder);
            return probeResult;
        }
        finally
        {
            try { File.Delete(listPath); } catch { }
        }
    }

    private static void CleanupFailedSaveFolder(string saveFolder)
    {
        try
        {
            if (Directory.Exists(saveFolder))
                Directory.Delete(saveFolder, recursive: true);
        }
        catch (Exception ex)
        {
            Logger.Warn($"SaveReplay: could not clean up failed save folder '{saveFolder}': {ex.Message}");
        }
    }

    private static SaveReplayResult ProbeWriteBlocked(string dir, Exception? originalError)
    {
        try
        {
            string probePath = Path.Combine(dir, ".ir_write_test");
            File.WriteAllText(probePath, "instant replay write test");
            File.Delete(probePath);
        }
        catch (UnauthorizedAccessException)
        {
            Logger.Warn($"SaveReplay: write to '{dir}' blocked (UnauthorizedAccessException) — " +
                        "likely Controlled Folder Access or antivirus.");
            return new SaveReplayResult(SaveReplayOutcome.WriteBlocked);
        }
        catch
        {
        }

        if (originalError != null)
            Logger.Error("SaveReplay: failed, and write-probe found no CFA/AV signature", originalError);

        return new SaveReplayResult(SaveReplayOutcome.MergeFailed);
    }

    private static void DeleteOldSegments(string dir)
    {
        try
        {
            foreach (var file in Directory.EnumerateFiles(dir, SegmentBaseName + "*"))
                File.Delete(file);
        }
        catch (Exception ex)
        {
            Logger.Warn($"DeleteOldSegments failed: {ex.Message}");
        }
    }

    private static string MsToOffset(int ms)
    {
        bool neg = ms < 0;
        int absMs = Math.Abs(ms);
        int whole = absMs / 1000;
        int frac = absMs % 1000;
        return $"{(neg ? "-" : "")}{whole}.{frac:D3}";
    }

    public void Dispose() => Stop();
}

public static class StartupManager
{
    private const string RunKeyPath = @"Software\Microsoft\Windows\CurrentVersion\Run";
    private const string ValueName = "InstantReplay";

    public static bool IsEnabled()
    {
        try
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.OpenSubKey(RunKeyPath, writable: false);
            return key?.GetValue(ValueName) is string existing &&
                   existing.Equals(BuildCommand(), StringComparison.OrdinalIgnoreCase);
        }
        catch (Exception ex)
        {
            Logger.Warn($"StartupManager.IsEnabled check failed: {ex.Message}");
            return false;
        }
    }

    public static void SetEnabled(bool enabled)
    {
        try
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.OpenSubKey(RunKeyPath, writable: true)
                             ?? Microsoft.Win32.Registry.CurrentUser.CreateSubKey(RunKeyPath);
            if (enabled)
                key.SetValue(ValueName, BuildCommand());
            else
                key.DeleteValue(ValueName, throwOnMissingValue: false);
            Logger.Info($"Start with Windows {(enabled ? "enabled" : "disabled")}.");
        }
        catch (Exception ex)
        {
            Logger.Error("StartupManager.SetEnabled failed", ex);
        }
    }

    private static string BuildCommand()
    {
        string exe = Environment.ProcessPath ?? "InstantReplay.exe";
        return $"\"{exe}\" --tray";
    }
}
