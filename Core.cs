using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml;
using System.Diagnostics;
using System.Net.Http;
using System.Net;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text.Json;
using System.Text.RegularExpressions;
using System.Text;

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
}

public sealed class Settings
{
    public string Microphone { get; set; } = "";
    public int ReplayDurationSeconds { get; set; } = 60;
    public int FrameRate { get; set; } = 60;
    public bool RecordSystemAudio { get; set; } = true;
    public bool RecordMicrophone { get; set; } = true;
    public bool DebugLogging { get; set; } = false;


    public bool DarkMode { get; set; } = true;

    public uint HotkeyModifiers { get; set; } = 0x0002;
    public uint HotkeyVk { get; set; } = 0x6A;

    public string QualityPreset { get; set; } = "Balanced";

    public string EncoderChoice { get; set; } = "Auto";

    public string CaptureMethod { get; set; } = "Wgc";
    public string CaptureTarget { get; set; } = "Display";
    public List<GameRule> SpecificWindows { get; set; } = [];
    public int CaptureMonitorIndex { get; set; } = -1;
    public bool CaptureClientArea { get; set; } = true;
    public bool CaptureCursor { get; set; } = true;
    public bool CaptureBorder { get; set; } = false;
    public bool CaptureRoundedCorners { get; set; } = true;
    public bool CaptureSecondaryWindows { get; set; } = false;

    public string SaveDestination { get; set; } = "";

    public bool AppMinimizedNotificationShown { get; set; } = false;

    public string BackgroundNotice { get; set; } = "Once";

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
                if (loaded != null)
                {
                    if (loaded.CaptureTarget is "Window" or "ActiveWindow") loaded.CaptureTarget = "SpecificWindows";
                    loaded.SpecificWindows ??= [];
                    if (loaded.FrameRate is not (30 or 60 or 120)) loaded.FrameRate = 60;
                    return loaded;
                }
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

    public void SaveFields(params string[] names)
    {
        if (names.Length == 0) return;
        try
        {
            string path = PathOnDisk;
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);

            Dictionary<string, JsonElement>? onDisk = null;
            if (File.Exists(path))
            {
                try
                {
                    onDisk = JsonSerializer.Deserialize<Dictionary<string, JsonElement>>(File.ReadAllText(path));
                }
                catch (Exception ex)
                {
                    Logger.Warn($"Settings.SaveFields: existing file is unreadable ({ex.Message}); rewriting it in full.");
                }
            }

            if (onDisk is null)
            {
                Save();
                return;
            }

            var options = new JsonSerializerOptions { WriteIndented = true };
            var doc = new Dictionary<string, JsonElement>(onDisk, StringComparer.Ordinal);
            foreach (string name in names)
            {
                PropertyInfo? property = typeof(Settings).GetProperty(name, BindingFlags.Public | BindingFlags.Instance);
                if (property is null || !property.CanRead)
                {
                    Logger.Warn($"Settings.SaveFields: '{name}' is not a readable property; ignored.");
                    continue;
                }
                doc[name] = JsonSerializer.SerializeToElement(property.GetValue(this), options);
            }

            File.WriteAllText(path, JsonSerializer.Serialize(doc, options));
        }
        catch (Exception ex)
        {
            Logger.Error("Settings.SaveFields failed", ex);
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
        Info($"Version: {typeof(Logger).Assembly.GetName().Version?.ToString(3)}");
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
    LowMemory,
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
            "Manage ransomware protection → Controlled folder access, and allow Instant Replay.exe " +
            "through it, then try again.",
        SaveReplayOutcome.MergeFailed => "Failed to save replay (see log for details).",
        SaveReplayOutcome.LowMemory => "Replay save skipped: Windows is running low on committed memory. Close other apps or enable a system-managed page file before trying again.",
        _ => "Failed to save replay (see log for details).",
    };
}

public readonly record struct VideoQualityProfile(string Name, int BitrateKbps, int MaxWidth, int MaxHeight)
{
    public static VideoQualityProfile FromPreset(string preset, int fps)
    {
        var (name, at60Fps) = preset switch
        {
            "Quality" or "High" => ("High", 9000),
            "Balanced" => ("Balanced", 6000),
            _ => ("Low", 4000)
        };
        int rate = Math.Clamp(fps, 15, 240);
        return new VideoQualityProfile(name, at60Fps * rate / 60, 3840, 2160);
    }
    public string Label => Name;
}

public sealed class RecorderEngine : IDisposable
{
    private volatile bool _armed;
    private bool _specific;
    private bool _paused;
    private GameDetector? _detector;
    private System.Threading.Timer? _targetTimer;
    private CaptureWindow? _target;
    private IntPtr _candidate;
    private DateTime _candidateSince;
    private CaptureSurface? _pendingSurface;
    private DateTime _surfaceSince;
    private DateTime _startedAt;
    private DateTime _retryAt;
    private string _targetState = "STOPPED";
    public bool IsPaused { get { lock (_opLock) return _armed && _specific && _paused; } }
    public string TargetState { get { lock (_opLock) return _targetState; } }
    private readonly object _lock = new();
    private readonly object _opLock = new();
    private readonly Settings _settings;
    private Settings _captureSettings = new();
    private readonly ReplayEngine _engine = new();

    private long _lastHealthTicks = -1;
    private int _noProgressPolls;

    public bool IsAvailable => true;

    public RecorderEngine(Settings settings)
    {
        _settings = settings;
    }

    public bool IsRunning
    {
        get
        {
            lock (_lock)
            {
                _engine.GetStatus(out bool buffering, out _, out _, out _, out _, out _,
                    out _, out _, out _, out _);
                return buffering || _armed;
            }
        }
    }

    public void ReloadGameRules()
    {
        lock (_opLock) { if (_specific) _detector = new GameDetector(_settings.SpecificWindows); }
    }

    public bool IsBufferHealthy()
    {
        lock (_lock)
        {
            _engine.GetStatus(out bool buffering, out _, out _, out _, out _, out bool hasError,
                out _, out _, out _, out long healthTicks);
            if (!buffering)
            {
                _lastHealthTicks = -1;
                _noProgressPolls = 0;
                return false;
            }
            if (hasError) return false;

            if (_lastHealthTicks < 0)
            {
                _lastHealthTicks = healthTicks;
                return true;
            }

            if (healthTicks != _lastHealthTicks)
            {
                _lastHealthTicks = healthTicks;
                _noProgressPolls = 0;
                return true;
            }

            if (++_noProgressPolls >= 8)
            {
                _lastHealthTicks = -1;
                return false;
            }

            return true;
        }
    }

    public List<string> EnumerateMicrophones()
    {
        return ReplayEngine.EnumerateMicrophones();
    }

    public bool TryGetMicStatus(out long micFrames, out long micEvents, out long micGotFrames,
        out long micFailHr, out int sysPeak, out int micPeak, out string activeName)
    {
        return _engine.TryGetMicStatus(out micFrames, out micEvents, out micGotFrames,
            out micFailHr, out sysPeak, out micPeak, out activeName);
    }

    private static EncoderPreference ResolveEncoder(string choice) => choice switch
    {
        "Hardware" => EncoderPreference.Hardware,
        "Software" => EncoderPreference.Software,
        _ => EncoderPreference.Auto,
    };

    private static CaptureMethod ResolveCaptureMethod(string choice) => choice switch
    {
        "Dxgi" => CaptureMethod.Dxgi,
        _ => CaptureMethod.Wgc,
    };

    public bool Start(string tempDir, bool wipeSegments = true)
    {
        lock (_opLock)
        {
            lock (_lock)
            {
                if (IsRunning)
                {
                    Logger.Warn("Start() called while already running; ignoring.");
                    return true;
                }
            }

            _captureSettings = JsonSerializer.Deserialize<Settings>(JsonSerializer.Serialize(_settings))!;
            _specific = _captureSettings.CaptureTarget == "SpecificWindows";
            _armed = true;
            if (_specific)
            {
                _detector = new GameDetector(_settings.SpecificWindows);
                _paused = true;
                _targetState = "WAITING FOR A GAME";
                _targetTimer = new System.Threading.Timer(_ => PollTarget(), null, 0, 200);
                return true;
            }
            bool ok = StartNative(IntPtr.Zero);
            _armed = ok;
            _targetState = ok ? "RECORDING" : "ERROR";
            return ok;
        }
    }

    private bool StartNative(IntPtr window)
    {
            var quality = VideoQualityProfile.FromPreset(_captureSettings.QualityPreset, _captureSettings.FrameRate);
            bool ok = _engine.StartBuffering(
                monitorIndex: _captureSettings.CaptureMonitorIndex,
                ringSeconds: Math.Max(_captureSettings.ReplayDurationSeconds, 1),
                fps: _captureSettings.FrameRate,
                bitrateKbps: quality.BitrateKbps,
                captureAudio: _captureSettings.RecordSystemAudio,
                preference: ResolveEncoder(_captureSettings.EncoderChoice),
                method: _specific ? CaptureMethod.Wgc : ResolveCaptureMethod(_captureSettings.CaptureMethod),
                captureMic: _captureSettings.RecordMicrophone,
                micDevice: _captureSettings.Microphone,
                targetWindow: window,
                captureOptions: (uint)((_captureSettings.CaptureClientArea ? 1 : 0) | (_captureSettings.CaptureCursor ? 2 : 0) | (_captureSettings.CaptureBorder ? 4 : 0) | (!_specific && _captureSettings.CaptureRoundedCorners ? 8 : 0) | (_captureSettings.CaptureSecondaryWindows ? 16 : 0) | (_specific ? 64 : 0)),
                outputWidthCap: quality.MaxWidth, outputHeightCap: quality.MaxHeight);

            if (!ok)
            {
                Logger.Error("Start() failed: managed engine could not start buffering.");
                return false;
            }

            lock (_lock)
            {
                _lastHealthTicks = -1;
                _noProgressPolls = 0;
            }

            Logger.Info($"Capture Started - Managed engine, FPS: {_captureSettings.FrameRate}, " +
                        $"Ring: {_captureSettings.ReplayDurationSeconds}s, " +
                        $"System audio: {(_captureSettings.RecordSystemAudio ? "on" : "off")}, " +
                        $"Mic: {(_captureSettings.RecordMicrophone ? (_captureSettings.Microphone.Length > 0 ? _captureSettings.Microphone : "default") : "off")}");
            return true;
    }

    private void PollTarget()
    {
        if (!Monitor.TryEnter(_opLock)) return;
        try
        {
            if (!_armed || !_specific || _detector == null) return;
            if (_target != null && !GameDetector.IsAlive(_target))
            {
                _engine.Shutdown();
                _target = null;
                _candidate = IntPtr.Zero;
                _pendingSurface = null;
                _paused = true;
                _retryAt = default;
                _targetState = "WAITING FOR A GAME";
                Logger.Info("Captured window closed; replay discarded. Waiting for an eligible foreground game.");
            }
            var foreground = _detector.Foreground();
            if (foreground == null)
            {
                _candidate = IntPtr.Zero;
                _pendingSurface = null;
                if (!_paused) _engine.SetPaused(true);
                _paused = true;
                _targetState = _target == null ? "WAITING FOR A GAME" : "PAUSED";
                return;
            }
            bool changedTarget = _target?.Handle != foreground.Handle || _target?.ProcessId != foreground.ProcessId;
            bool changedSurface = !changedTarget && _target!.Surface != foreground.Surface;
            if (changedSurface)
            {
                if (!_paused) _engine.SetPaused(true);
                _paused = true;
                _targetState = "RESTARTING CAPTURE";
                if (_pendingSurface != foreground.Surface)
                {
                    _pendingSurface = foreground.Surface;
                    _surfaceSince = DateTime.UtcNow;
                    return;
                }
                if (DateTime.UtcNow - _surfaceSince < TimeSpan.FromMilliseconds(600)) return;
            }
            else _pendingSurface = null;
            if (changedTarget || changedSurface)
            {
                if (!_paused) _engine.SetPaused(true);
                _paused = true;
                if (changedTarget && _candidate != foreground.Handle)
                {
                    _candidate = foreground.Handle;
                    _candidateSince = DateTime.UtcNow;
                    return;
                }
                if ((changedTarget && DateTime.UtcNow - _candidateSince < TimeSpan.FromMilliseconds(250)) || DateTime.UtcNow < _retryAt) return;
                var verified = _detector.Foreground();
                if (verified?.Handle != foreground.Handle || verified.ProcessId != foreground.ProcessId || verified.Surface != foreground.Surface) return;
                _engine.StopBuffering();
                _target = foreground;
                if (!StartNative(foreground.Handle))
                {
                    _target = null;
                    _targetState = "CAPTURE UNAVAILABLE";
                    _retryAt = DateTime.UtcNow.AddSeconds(5);
                    return;
                }
                _startedAt = DateTime.UtcNow;
                _pendingSurface = null;
                Logger.Info($"Capture surface: {foreground.Surface}; mode/size restart: {changedSurface}.");
                Logger.Info($"Replay target changed to {foreground.Game} ({foreground.Executable}); previous buffer discarded.");
            }
            _engine.GetStatus(out bool running, out _, out _, out long frames, out _, out bool error, out _, out _, out _, out _);
            if (!running || error || (frames == 0 && DateTime.UtcNow - _startedAt > TimeSpan.FromSeconds(8)))
            {
                _engine.StopBuffering();
                _target = null;
                _paused = true;
                _targetState = "CAPTURE UNAVAILABLE";
                _retryAt = DateTime.UtcNow.AddSeconds(5);
                Logger.Warn("Window capture unavailable; waiting before retrying. No desktop or system-audio fallback.");
                return;
            }
            var current = _detector.Foreground();
            if (current?.Handle != foreground.Handle || current.ProcessId != foreground.ProcessId || current.Surface != foreground.Surface)
            {
                _engine.SetPaused(true);
                _paused = true;
                _candidate = IntPtr.Zero;
                _pendingSurface = null;
                _targetState = "PAUSED";
                return;
            }
            if (_paused) _engine.SetPaused(false);
            _paused = false;
            _targetState = "RECORDING";
        }
        catch (Exception ex) { _engine.SetPaused(true); _paused = true; _targetState = "CAPTURE UNAVAILABLE"; Logger.Error("Target detection failed", ex); }
        finally { Monitor.Exit(_opLock); }
    }

    public void Stop()
    {
        lock (_opLock)
        {
            _armed = false;
            _targetTimer?.Dispose();
            _targetTimer = null;
            _target = null;
            _candidate = IntPtr.Zero;
            _pendingSurface = null;
            _paused = false;
            _retryAt = default;
            _targetState = "STOPPED";
            _engine.StopBuffering();
            lock (_lock)
            {
                _lastHealthTicks = -1;
                _noProgressPolls = 0;
            }
            Logger.Info("Recording stopped.");
        }
    }

    public bool Restart(string tempDir, bool wipeSegments = false)
    {
        lock (_opLock)
        {
            Stop();
            return Start(tempDir, wipeSegments);
        }
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
        lock (_lock)
        {
            if (!IsRunning)
            {
                Logger.Warn("SaveReplay requested but recorder is not running.");
                return new SaveReplayResult(SaveReplayOutcome.NotRecording);
            }
        }

        string saveFolder = Path.Combine(destDir, $"replay_{DateTime.Now:yyyyMMdd_HHmmss}");
        try
        {
            Directory.CreateDirectory(saveFolder);
        }
        catch (Exception ex)
        {
            Logger.Error("SaveReplay: failed creating save folder", ex);
            return ProbeWriteBlocked(destDir, ex);
        }

        string destMp4 = Path.Combine(saveFolder, "replay.mp4");

        int rc;
        try
        {
            rc = _engine.SaveReplay(destMp4, Math.Max(_settings.ReplayDurationSeconds, 1));
        }
        catch (Exception ex)
        {
            Logger.Error("SaveReplay: exception in managed engine", ex);
            CleanupFailedSaveFolder(saveFolder);
            return ProbeWriteBlocked(destDir, ex);
        }

        if (rc == 0 && File.Exists(destMp4))
        {
            Logger.Info($"Replay Saved - Path: {destMp4}");
            return new SaveReplayResult(SaveReplayOutcome.Success, destMp4);
        }

        Logger.Info($"SaveReplay: managed engine returned {rc}.");

        if (rc == -31)
        {
            CleanupFailedSaveFolder(saveFolder);
            Logger.Warn("Replay save refused because Windows has insufficient committed-memory headroom. No replay data was duplicated or written.");
            return new SaveReplayResult(SaveReplayOutcome.LowMemory);
        }

        if (rc == -1 || rc == -30)
        {
            CleanupFailedSaveFolder(saveFolder);
            return new SaveReplayResult(SaveReplayOutcome.NoSegments);
        }

        CleanupFailedSaveFolder(saveFolder);
        return ProbeWriteBlocked(destDir, null);
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

    public void Dispose()
    {
        Stop();
        try { _engine.Shutdown(); }
        catch (Exception ex) { Logger.Warn($"Engine shutdown failed: {ex.Message}"); }
    }
}

public static class StartupManager
{
    private const string RunKeyPath = @"Software\Microsoft\Windows\CurrentVersion\Run";

    private const string ValueName = "Instant Replay";

    private const string LegacyValueName = "InstantReplay";

    public static bool IsEnabled()
    {
        CleanupLegacyKey();
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
        CleanupLegacyKey();
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

    private static void CleanupLegacyKey()
    {
        try
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.OpenSubKey(RunKeyPath, writable: true);
            if (key?.GetValue(LegacyValueName) != null)
            {
                key.DeleteValue(LegacyValueName, throwOnMissingValue: false);
                Logger.Info($"Removed legacy duplicate startup registry value \"{LegacyValueName}\".");
            }
        }
        catch (Exception ex)
        {
            Logger.Warn($"StartupManager.CleanupLegacyKey failed: {ex.Message}");
        }
    }

    private static string BuildCommand()
    {
        string exe = Environment.ProcessPath ?? "InstantReplay.exe";
        return $"\"{exe}\" --tray";
    }
}

internal static class IrNative
{
    internal const int OK = 0;
    internal const int Fail = -1;
    internal const int NotEnoughData = -30;

    private const string Dll = "engine";

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_Create(out IntPtr engine);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void IR_Destroy(IntPtr engine);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_Start(IntPtr engine, int monitorIndex, int ringSeconds,
        int fps, int bitrateKbps, int capW, int capH, int captureAudio, int captureMic,
        [MarshalAs(UnmanagedType.LPWStr)] string micDevice,
        int encoderPreference, int captureMethod, out long startQpc100ns);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void IR_Stop(IntPtr engine);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_SetCaptureTarget(IntPtr engine, IntPtr window, uint options);
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_SetPaused(IntPtr engine, int paused);
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_GetWindowCount();
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void IR_GetWindowInfo(int index, out IntPtr window,
        [MarshalAs(UnmanagedType.LPWStr)] StringBuilder title, int titleCapacity, out int width, out int height);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_Save(IntPtr engine,
        [MarshalAs(UnmanagedType.LPWStr)] string outPath, int secondsBack);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void IR_GetStatus(IntPtr engine, out int isBuffering,
        out int ringSeconds, out long framesEncoded, out long framesDropped,
        out int hasError, out long audioFrames, out long audioEvents,
        out long audioGotFrames, out long healthHeartbeat);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_GetMonitorCount();

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void IR_GetMonitorInfo(int index,
        [MarshalAs(UnmanagedType.LPWStr)] StringBuilder name, int nameCap,
        out int width, out int height);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_IsDxgiSupported();

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_DxgiDuplicationSupported();

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_ActiveCaptureMethod(IntPtr engine);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_ActiveEncoder(IntPtr engine);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_HwInitHr(IntPtr engine);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_GetMicDeviceCount();

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void IR_GetMicDeviceName(int index,
        [MarshalAs(UnmanagedType.LPWStr)] StringBuilder name, int nameCap);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void IR_MicStatus(IntPtr engine, out long micFrames, out long micEvents,
        out long micGotFrames, out long micFailHr, out int sysPeak, out int micPeak,
        [MarshalAs(UnmanagedType.LPWStr)] StringBuilder activeName, int nameCap);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr IR_Version();

    internal static string Version()
    {
        try { return Marshal.PtrToStringAnsi(IR_Version()) ?? ""; }
        catch { return ""; }
    }
}

public enum EncoderPreference
{
    Auto = 0,
    Hardware = 1,
    Software = 2,
}

public enum CaptureMethod
{
    Wgc = 0,
    Dxgi = 1,
}

public sealed class ReplayEngine : IDisposable
{
    public static bool DxgiDuplicationSupported()
    {
        try { return IrNative.IR_DxgiDuplicationSupported() == 1; }
        catch { return false; }
    }
    public const int SaveOk = IrNative.OK;
    public const int SaveFailed = IrNative.Fail;
    public const int SaveNotEnoughData = IrNative.NotEnoughData;

    private readonly object _opLock = new();
    private IntPtr _engine;

    private volatile bool _hasError;
    private string _lastVersion = "";

    public ReplayEngine()
    {
        try { _lastVersion = IrNative.Version(); }
        catch (Exception ex)
        {
            Logger.Warn($"[Engine] Could not load engine.dll: {ex.Message}");
            _lastVersion = "";
        }
    }

    public bool Initialize()
    {
        lock (_opLock)
        {
            if (_engine != IntPtr.Zero) return true;
            int rc = IrNative.IR_Create(out IntPtr handle);
            if (rc != IrNative.OK || handle == IntPtr.Zero)
            {
                _hasError = true;
                Logger.Error("[Engine] IR_Create failed; native engine not available.");
                return false;
            }
            _engine = handle;
            Logger.Info($"[Engine] Native engine created (version {_lastVersion}).");
            return true;
        }
    }

    public bool StartBuffering(int monitorIndex, int ringSeconds, int fps, int bitrateKbps,
        bool captureAudio, EncoderPreference preference, CaptureMethod method = CaptureMethod.Wgc,
        bool captureMic = false, string? micDevice = null, IntPtr targetWindow = default, uint captureOptions = 11,
        int outputWidthCap = 3840, int outputHeightCap = 2160)
    {
        lock (_opLock)
        {
            GetStatus(out bool buffering, out _, out _, out _, out _, out _, out _, out _, out _, out _);
            if (buffering) return true;
            if (!Initialize()) return false;

            int mon = monitorIndex;
            int secs = Math.Max(ringSeconds, 1);
            int rate = Math.Clamp(fps, 15, 240);
            int bps = Math.Max(bitrateKbps, 1);
            int capW = Math.Max(0, outputWidthCap), capH = Math.Max(0, outputHeightCap);

            if (targetWindow != IntPtr.Zero || (captureOptions & 32) != 0) method = CaptureMethod.Wgc;
            if (IrNative.IR_SetCaptureTarget(_engine, targetWindow, captureOptions) != IrNative.OK)
            {
                _hasError = true;
                Logger.Error("[Engine] Capture target is unavailable. Select an open, non-minimized window.");
                return false;
            }
            if (method == CaptureMethod.Dxgi && IrNative.IR_DxgiDuplicationSupported() == 0)
            {
                _hasError = true;
                Logger.Error("[Engine] DXGI capture requested, but no desktop duplication could be opened. " +
                             "Another screen recorder or remote-desktop tool is probably already capturing this monitor.");
                return false;
            }

            long origin;
            string wantedMic = (micDevice ?? "").Trim();
            int rc = IrNative.IR_Start(_engine, mon, secs, rate, bps, capW, capH,
                captureAudio ? 1 : 0, captureMic ? 1 : 0,
                captureMic ? wantedMic : string.Empty,
                (int)preference, (int)method, out origin);
            if (rc != IrNative.OK)
            {
                _hasError = true;
                Logger.Error($"[Engine] IR_Start failed (rc={rc}). Capture could not start.");
                return false;
            }

            if (captureMic) ReportMicStartup(wantedMic);

            _hasError = false;
            int active = IrNative.IR_ActiveEncoder(_engine);
            int hwHr = IrNative.IR_HwInitHr(_engine);
            string encName = active switch
            {
                4 => "hardware (Media Foundation, GPU shaders, zero-copy)",
                2 => "hardware (Media Foundation, CPU upload)",
                1 => "software (Microsoft H.264 MFT)",
                _ => "unknown",
            };
            Logger.Info($"[Engine] Encoder: {encName} (requested {preference}){(hwHr != 0 && active != 4 ? $", GPU/hardware path unavailable: HRESULT 0x{hwHr:X8}" : "")}");

            int activeCap = IrNative.IR_ActiveCaptureMethod(_engine);
            string capName = activeCap == 1 ? "DXGI duplication" : "WGC";
            Logger.Info($"[Engine] Buffering started ({capName}): monitor {mon}, {rate} fps, {secs}s ring, bitrate {bps} kbps, cap {capW}x{capH}, audio={(captureAudio ? "on" : "off")}");
            return true;
        }
    }

    public void StopBuffering()
    {
        lock (_opLock)
        {
            if (_engine != IntPtr.Zero)
            {
                try { IrNative.IR_Stop(_engine); } catch { }
            }
            Logger.Info("[Engine] Buffering stopped");
        }
    }
    public void SetPaused(bool paused)
    {
        lock (_opLock)
        {
            if (_engine != IntPtr.Zero) IrNative.IR_SetPaused(_engine, paused ? 1 : 0);
        }
    }

    public int SaveReplay(string outPath, int secondsBack)
    {
        lock (_opLock)
        {
            if (_engine == IntPtr.Zero) return SaveFailed;
            if (string.IsNullOrWhiteSpace(outPath)) return SaveFailed;
            int rc = IrNative.IR_Save(_engine, outPath, Math.Max(secondsBack, 1));
            Logger.Info($"[Engine] IR_Save('{outPath}', {secondsBack}s) -> rc={rc}");
            return rc;
        }
    }

    public void GetStatus(out bool isBuffering, out int segmentCount, out int ringSeconds,
        out long framesEncoded, out long framesDropped, out bool hasError,
        out long audioFrames, out long audioEvents, out long audioGotFrames, out long healthHeartbeat)
    {
        if (_engine == IntPtr.Zero)
        {
            isBuffering = false;
            segmentCount = 0;
            ringSeconds = 0;
            framesEncoded = 0;
            framesDropped = 0;
            hasError = _hasError;
            audioFrames = 0;
            audioEvents = 0;
            audioGotFrames = 0;
            healthHeartbeat = 0;
            return;
        }

        int ib, rs, he;
        long fe, fd, af, ae, agf, hb;
        IrNative.IR_GetStatus(_engine, out ib, out rs, out fe, out fd, out he, out af, out ae, out agf, out hb);
        isBuffering = ib != 0;
        segmentCount = 0;
        ringSeconds = rs;
        framesEncoded = fe;
        framesDropped = fd;
        hasError = he != 0 || _hasError;
        audioFrames = af;
        audioEvents = ae;
        audioGotFrames = agf;
        healthHeartbeat = hb;
    }

    public static List<(IntPtr Handle, string Title)> EnumerateWindows()
    {
        var result = new List<(IntPtr Handle, string Title)>();
        try
        {
            int count = IrNative.IR_GetWindowCount();
            for (int i = 0; i < count; i++)
            {
                var title = new StringBuilder(512);
                IrNative.IR_GetWindowInfo(i, out IntPtr handle, title, title.Capacity, out _, out _);
                if (handle != IntPtr.Zero && title.Length > 0) result.Add((handle, title.ToString()));
            }
        }
        catch (Exception error) { Logger.Warn($"[Engine] Window enumeration failed: {error.Message}"); }
        return result;
    }

    public static List<(int Index, string Name)> EnumerateMonitors()
    {
        var result = new List<(int, string)>();
        try
        {
            int count = IrNative.IR_GetMonitorCount();
            for (int i = 0; i < count; ++i)
            {
                var sb = new StringBuilder(256);
                IrNative.IR_GetMonitorInfo(i, sb, sb.Capacity, out _, out _);
                result.Add((i, sb.ToString()));
            }
        }
        catch (Exception ex)
        {
            Logger.Warn($"[Engine] EnumerateMonitors failed: {ex.Message}");
        }
        return result;
    }

    public static List<string> EnumerateMicrophones()
    {
        var result = new List<string>();
        try
        {
            int count = IrNative.IR_GetMicDeviceCount();
            for (int i = 0; i < count; ++i)
            {
                var sb = new StringBuilder(512);
                IrNative.IR_GetMicDeviceName(i, sb, sb.Capacity);
                string name = sb.ToString().Trim();
                if (name.Length > 0 && !result.Contains(name)) result.Add(name);
            }
        }
        catch (Exception ex)
        {
            Logger.Warn($"[Engine] EnumerateMicrophones failed: {ex.Message}");
        }
        return result;
    }

    public bool TryGetMicStatus(out long micFrames, out long micEvents, out long micGotFrames,
        out long micFailHr, out int sysPeak, out int micPeak, out string activeName)
    {
        micFrames = 0;
        micEvents = 0;
        micGotFrames = 0;
        micFailHr = 0;
        sysPeak = 0;
        micPeak = 0;
        activeName = "";
        if (_engine == IntPtr.Zero) return false;
        try
        {
            var sb = new StringBuilder(512);
            IrNative.IR_MicStatus(_engine, out micFrames, out micEvents, out micGotFrames,
                out micFailHr, out sysPeak, out micPeak, sb, sb.Capacity);
            activeName = sb.ToString().Trim();
            return true;
        }
        catch (Exception ex)
        {
            Logger.Warn($"[Engine] IR_MicStatus failed: {ex.Message}");
            return false;
        }
    }

    private void ReportMicStartup(string requested)
    {
        if (!TryGetMicStatus(out _, out _, out long got, out long failHr, out int sysPeak,
            out int micPeak, out string active))
        {
            Logger.Warn("[Engine] Microphone capture requested but engine status is unavailable.");
            return;
        }

        if (failHr != 0)
        {
            string reason = failHr switch
            {
                -1 => "COM could not be initialised on the capture thread",
                -2 => "the capture stream stopped or could not be opened",
                -3 => "no capture device matched and there is no default",
                _ => $"internal error {failHr}",
            };
            Logger.Error($"[Engine] Microphone capture failed: {reason}.");
            return;
        }

        if (active.Length == 0)
        {
            Logger.Warn($"[Engine] Microphone capture requested ('{requested}') but no device is open.");
            return;
        }

        if (requested.Length > 0 && !string.Equals(active, requested, StringComparison.OrdinalIgnoreCase))
            Logger.Warn($"[Engine] Microphone '{requested}' is unavailable, using '{active}' instead.");
        else
            Logger.Info($"[Engine] Microphone capture: '{active}'.");

        if (got == 0)
            Logger.Warn($"[Engine] Microphone '{active}' has not delivered any audio yet; " +
                        "check the Windows privacy setting for microphone access and that no other app is holding it exclusively.");
        else if (micPeak == 0)
            Logger.Warn($"[Engine] Microphone '{active}' is delivering silence " +
                        $"(system audio peak={sysPeak}); it will be recorded but muted.");
        else
            Logger.Info($"[Engine] Microphone '{active}' peak={micPeak}, system audio peak={sysPeak}.");
    }

    public static bool IsDxgiSupported()
    {
        try { return IrNative.IR_IsDxgiSupported() != 0; }
        catch { return false; }
    }

    public void Shutdown()
    {
        lock (_opLock)
        {
            StopBuffering();
            if (_engine != IntPtr.Zero)
            {
                try { IrNative.IR_Destroy(_engine); } catch { }
                _engine = IntPtr.Zero;
            }
        }
    }

    public void Dispose() => Shutdown();
}

internal sealed record UpdateRelease(string Tag, string Version, string Notes, string? DownloadUrl, long Size, string? Sha256);
internal sealed record UpdateCheck(DateTimeOffset LastAttemptUtc, UpdateRelease? Release, string? Error, DateTimeOffset LastReminderUtc = default);
internal sealed record UpdateInstallResult(bool Success, string Message, string Version, DateTimeOffset FinishedUtc);

internal sealed class UpdateService
{
    internal const string Repository = "ghostneverdies/InstantReplay";
    internal const string InstallerName = "InstantReplaySetup.exe";
    internal static readonly TimeSpan CheckInterval = TimeSpan.FromHours(24);
    private const long MaxInstallerSize = 512L * 1024 * 1024;
    private static readonly HttpClient SharedClient = new() { Timeout = Timeout.InfiniteTimeSpan };
    private readonly HttpClient _client;
    private readonly string _statePath;
    internal Version CurrentVersion { get; }
    internal UpdateCheck State { get; private set; } = new(DateTimeOffset.MinValue, null, null);
    internal bool HasUpdate => State.Release is { } release && TryParseVersion(release.Version, out var version) && version > CurrentVersion;
    internal bool IsDue(DateTimeOffset now) => State.LastAttemptUtc > now || now - State.LastAttemptUtc >= CheckInterval;
    internal bool IsReminderDue(DateTimeOffset now) => HasUpdate && State.Error is null
        && (State.LastReminderUtc > now || now - State.LastReminderUtc >= CheckInterval);

    internal void MarkReminderShown(DateTimeOffset now)
    {
        State = State with { LastReminderUtc = now };
        SaveState();
    }

    internal UpdateService() : this(SharedClient,
        Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "InstantReplay", "updates.json"),
        typeof(UpdateService).Assembly.GetName().Version ?? new Version(1, 5, 0)) { }

    internal UpdateService(HttpClient client, string statePath, Version currentVersion)
    {
        _client = client;
        _statePath = statePath;
        CurrentVersion = Normalize(currentVersion);
        try
        {
            if (File.Exists(statePath))
                State = JsonSerializer.Deserialize<UpdateCheck>(File.ReadAllText(statePath)) ?? State;
        }
        catch (Exception ex) { Logger.Warn($"Update state could not be read: {ex.Message}"); }
    }

    private static Version Normalize(Version version) => new(version.Major, version.Minor, Math.Max(0, version.Build), Math.Max(0, version.Revision));

    internal static bool TryParseVersion(string? tag, out Version version)
    {
        version = new Version(0, 0, 0, 0);
        string text = (tag ?? "").Trim();
        if (text.StartsWith('v') || text.StartsWith('V')) text = text[1..];
        if (!Regex.IsMatch(text, @"^\d+\.\d+(?:\.\d+){0,2}$") || !Version.TryParse(text, out var parsed)) return false;
        version = Normalize(parsed);
        return true;
    }

    internal async Task<UpdateCheck> CheckAsync(CancellationToken cancellationToken)
    {
        State = State with { LastAttemptUtc = DateTimeOffset.UtcNow };
        SaveState();
        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeout.CancelAfter(TimeSpan.FromSeconds(25));
        try
        {
            using var request = new HttpRequestMessage(HttpMethod.Get, $"https://api.github.com/repos/{Repository}/releases/latest");
            request.Headers.UserAgent.ParseAdd($"InstantReplay/{CurrentVersion.ToString(3)}");
            request.Headers.Accept.ParseAdd("application/vnd.github+json");
            request.Headers.Add("X-GitHub-Api-Version", "2022-11-28");
            using var response = await _client.SendAsync(request, HttpCompletionOption.ResponseHeadersRead, timeout.Token).ConfigureAwait(false);
            if (response.StatusCode == HttpStatusCode.NotFound)
                State = State with { Release = null, Error = null };
            else
            {
                if (response.StatusCode is HttpStatusCode.Forbidden or HttpStatusCode.TooManyRequests)
                    throw new InvalidOperationException("GitHub temporarily limited update checks. Try again later.");
                response.EnsureSuccessStatusCode();
                if (response.Content.Headers.ContentLength > 1024 * 1024)
                    throw new InvalidDataException("The release response is too large.");
                await using var input = await response.Content.ReadAsStreamAsync(timeout.Token).ConfigureAwait(false);
                using var bytes = new MemoryStream();
                byte[] buffer = new byte[16384];
                int count;
                while ((count = await input.ReadAsync(buffer, timeout.Token).ConfigureAwait(false)) > 0)
                {
                    if (bytes.Length + count > 1024 * 1024) throw new InvalidDataException("The release response is too large.");
                    bytes.Write(buffer, 0, count);
                }
                State = State with { Release = ParseRelease(bytes.ToArray()), Error = null };
            }
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested) { throw; }
        catch (Exception ex)
        {
            string message = ex is OperationCanceledException ? "The update check timed out. Check your connection and try again." :
                ex is HttpRequestException ? "Could not contact GitHub. Check your connection and try again." : ex.Message;
            State = State with { Error = message };
            Logger.Warn($"Update check failed: {ex.Message}");
        }
        SaveState();
        return State;
    }

    internal static UpdateRelease ParseRelease(byte[] bytes)
    {
        using var json = JsonDocument.Parse(bytes);
        var root = json.RootElement;
        if (root.GetProperty("draft").GetBoolean() || root.GetProperty("prerelease").GetBoolean())
            throw new InvalidDataException("The release is not a stable published version.");
        string tag = root.GetProperty("tag_name").GetString() ?? "";
        if (!TryParseVersion(tag, out var version)) throw new InvalidDataException("The latest release has an unsupported version tag.");
        string notes = root.TryGetProperty("body", out var body) ? body.GetString() ?? "" : "";
        if (notes.Length > 12000) notes = notes[..12000] + "…";
        string? url = null, digest = null;
        long size = 0;
        foreach (var asset in root.GetProperty("assets").EnumerateArray())
        {
            if (!string.Equals(asset.GetProperty("name").GetString(), InstallerName, StringComparison.OrdinalIgnoreCase)) continue;
            string candidate = asset.GetProperty("browser_download_url").GetString() ?? "";
            if (!IsReleaseDownload(candidate)) throw new InvalidDataException("The installer URL does not belong to this project's GitHub releases.");
            url = candidate;
            size = asset.GetProperty("size").GetInt64();
            string? rawDigest = asset.TryGetProperty("digest", out var value) ? value.GetString() : null;
            if (rawDigest is not null && Regex.IsMatch(rawDigest, "^sha256:[0-9a-fA-F]{64}$")) digest = rawDigest[7..].ToLowerInvariant();
            break;
        }
        return new(tag, version.ToString(version.Revision == 0 ? 3 : 4), notes, url, size, digest);
    }

    private static bool IsReleaseDownload(string value) => Uri.TryCreate(value, UriKind.Absolute, out var uri)
        && uri.Scheme == "https" && uri.IsDefaultPort && uri.UserInfo.Length == 0 && uri.Host.Equals("github.com", StringComparison.OrdinalIgnoreCase)
        && uri.AbsolutePath.StartsWith($"/{Repository}/releases/download/", StringComparison.Ordinal);

    private static bool IsDownloadHost(Uri uri) => uri.Scheme == "https" && uri.IsDefaultPort && uri.UserInfo.Length == 0
        && (uri.Host.Equals("github.com", StringComparison.OrdinalIgnoreCase) || uri.Host.Equals("release-assets.githubusercontent.com", StringComparison.OrdinalIgnoreCase)
            || uri.Host.Equals("objects.githubusercontent.com", StringComparison.OrdinalIgnoreCase) || uri.Host.Equals("releases.githubusercontent.com", StringComparison.OrdinalIgnoreCase));

    internal async Task<string> DownloadAsync(UpdateRelease release, string directory, IProgress<double>? progress, CancellationToken cancellationToken)
    {
        if (!TryParseVersion(release.Version, out var version) || version <= CurrentVersion)
            throw new InvalidOperationException("This release is not newer than the installed app.");
        if (release.DownloadUrl is null || !IsReleaseDownload(release.DownloadUrl))
            throw new InvalidOperationException($"The release does not contain {InstallerName}.");
        if (release.Size <= 0 || release.Size > MaxInstallerSize || release.Sha256 is null || !Regex.IsMatch(release.Sha256, "^[0-9a-fA-F]{64}$"))
            throw new InvalidDataException("The release is missing a valid installer size or SHA-256 checksum.");
        Directory.CreateDirectory(directory);
        string path = Path.Combine(directory, InstallerName);
        string partial = path + ".partial";
        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeout.CancelAfter(TimeSpan.FromMinutes(10));
        try
        {
            using var request = new HttpRequestMessage(HttpMethod.Get, release.DownloadUrl);
            request.Headers.UserAgent.ParseAdd($"InstantReplay/{CurrentVersion.ToString(3)}");
            using var response = await _client.SendAsync(request, HttpCompletionOption.ResponseHeadersRead, timeout.Token).ConfigureAwait(false);
            response.EnsureSuccessStatusCode();
            if (response.RequestMessage?.RequestUri is not { } finalUri || !IsDownloadHost(finalUri))
                throw new InvalidDataException("The installer download redirected outside GitHub.");
            if (response.Content.Headers.ContentLength is { } length && length != release.Size)
                throw new InvalidDataException("The installer size does not match the release.");
            await using (var input = await response.Content.ReadAsStreamAsync(timeout.Token).ConfigureAwait(false))
            await using (var output = new FileStream(partial, FileMode.CreateNew, FileAccess.Write, FileShare.None, 131072, true))
            using (var hash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256))
            {
                byte[] buffer = new byte[131072];
                long downloaded = 0;
                int count;
                while ((count = await input.ReadAsync(buffer, timeout.Token).ConfigureAwait(false)) > 0)
                {
                    downloaded += count;
                    if (downloaded > release.Size) throw new InvalidDataException("The installer exceeds the expected size.");
                    hash.AppendData(buffer, 0, count);
                    await output.WriteAsync(buffer.AsMemory(0, count), timeout.Token).ConfigureAwait(false);
                    progress?.Report(downloaded * 100.0 / release.Size);
                }
                if (downloaded != release.Size) throw new InvalidDataException("The installer download was incomplete.");
                if (!CryptographicOperations.FixedTimeEquals(hash.GetHashAndReset(), Convert.FromHexString(release.Sha256)))
                    throw new InvalidDataException("The installer checksum does not match. Nothing was installed.");
            }
            File.Move(partial, path);
            return path;
        }
        finally { if (File.Exists(partial)) File.Delete(partial); }
    }

    private void SaveState()
    {
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(_statePath)!);
            string temp = _statePath + "." + Guid.NewGuid().ToString("N") + ".tmp";
            try
            {
                File.WriteAllText(temp, JsonSerializer.Serialize(State), Encoding.UTF8);
                File.Move(temp, _statePath, true);
            }
            finally { if (File.Exists(temp)) File.Delete(temp); }
        }
        catch (Exception ex) { Logger.Warn($"Update state could not be saved: {ex.Message}"); }
    }

    internal static string ResultPath => Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "InstantReplay", "update-result.json");

    internal static async Task StartInstallerAsync(string installer, UpdateRelease release, CancellationToken cancellationToken)
    {
        string directory = Path.GetDirectoryName(installer)!;
        string script = Path.Combine(directory, "install-update.ps1");
        string configuration = Path.Combine(directory, "install-update.json");
        string ready = Path.Combine(directory, "ready");
        string exe = Environment.ProcessPath ?? throw new InvalidOperationException("The application path is unavailable.");
        using var parent = Process.GetCurrentProcess();
        File.WriteAllText(configuration, JsonSerializer.Serialize(new
        {
            ParentId = Environment.ProcessId, ParentStartTicks = parent.StartTime.ToUniversalTime().Ticks,
            Installer = installer, Sha256 = release.Sha256, App = exe, InstallDirectory = Path.GetDirectoryName(exe),
            Version = release.Version, Result = ResultPath, Ready = ready
        }), Encoding.UTF8);
        File.WriteAllText(script, InstallerScript, new UTF8Encoding(true));
        var start = new ProcessStartInfo(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.System), @"WindowsPowerShell\v1.0\powershell.exe"))
        {
            UseShellExecute = false, CreateNoWindow = true, WindowStyle = ProcessWindowStyle.Hidden,
            WorkingDirectory = directory
        };
        foreach (string argument in new[] { "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", script, "-UpdateConfiguration", configuration })
            start.ArgumentList.Add(argument);
        using var helper = Process.Start(start) ?? throw new InvalidOperationException("The update helper could not start.");
        try
        {
            for (int i = 0; i < 100; i++)
            {
                cancellationToken.ThrowIfCancellationRequested();
                if (File.Exists(ready)) return;
                if (helper.HasExited) throw new InvalidOperationException("The update helper exited before it was ready.");
                await Task.Delay(100, cancellationToken).ConfigureAwait(false);
            }
            throw new TimeoutException("The update helper did not become ready. The app has stayed open.");
        }
        catch
        {
            if (!helper.HasExited) helper.Kill();
            throw;
        }
    }

    internal const string InstallerScript = """
        param([Parameter(Mandatory=$true)][string]$UpdateConfiguration)
        $ErrorActionPreference = 'Stop'
        $update = Get-Content -LiteralPath $UpdateConfiguration -Raw | ConvertFrom-Json
        $parentExited = $false
        $installerStarted = $false
        $installerFinished = $false
        $success = $false
        $message = ''
        try {
            $parentProcess = Get-Process -Id $update.ParentId -ErrorAction SilentlyContinue
            Set-Content -LiteralPath $update.Ready -Value 'ready' -Encoding UTF8
            if ($null -ne $parentProcess -and $parentProcess.StartTime.ToUniversalTime().Ticks -eq $update.ParentStartTicks) {
                if (-not $parentProcess.WaitForExit(60000)) { throw 'Instant Replay did not exit. Nothing was installed.' }
            }
            $parentExited = $true
            $hashStream = [System.IO.File]::OpenRead($update.Installer)
            $hasher = [System.Security.Cryptography.SHA256]::Create()
            try { $checksum = [BitConverter]::ToString($hasher.ComputeHash($hashStream)).Replace('-', '') }
            finally { $hasher.Dispose(); $hashStream.Dispose() }
            if ($checksum -ine $update.Sha256) { throw 'The installer checksum changed. Nothing was installed.' }
            $arguments = '/S -quiet /UPDATE=1 /D=' + $update.InstallDirectory
            $installerInfo = New-Object System.Diagnostics.ProcessStartInfo
            $installerInfo.FileName = $update.Installer
            $installerInfo.Arguments = $arguments
            $installerInfo.UseShellExecute = $true
            $installerInfo.Verb = 'runas'
            $installerProcess = [System.Diagnostics.Process]::Start($installerInfo)
            $installerStarted = $true
            $installerProcess.WaitForExit()
            $installerFinished = $true
            if ($installerProcess.ExitCode -ne 0) { throw ('The installer failed with exit code ' + $installerProcess.ExitCode + '.') }
            $success = $true
            $message = 'Update installed successfully.'
        } catch {
            $message = $_.Exception.Message
        } finally {
            try {
                @{Success=$success; Message=$message; Version=$update.Version; FinishedUtc=[DateTimeOffset]::UtcNow.ToString('o')} |
                    ConvertTo-Json | Set-Content -LiteralPath $update.Result -Encoding UTF8
            } catch { }
            if ($parentExited -and (Test-Path -LiteralPath $update.App)) {
                try {
                    $appInfo = New-Object System.Diagnostics.ProcessStartInfo
                    $appInfo.FileName = $update.App
                    $appInfo.WorkingDirectory = $update.InstallDirectory
                    $appInfo.UseShellExecute = $false
                    [System.Diagnostics.Process]::Start($appInfo) | Out-Null
                } catch { }
            }
            if (-not $installerStarted -or $installerFinished) {
                Remove-Item -LiteralPath $update.Installer,$UpdateConfiguration,$update.Ready,$PSCommandPath -Force -ErrorAction SilentlyContinue
            }
        }
        """;
}

public partial class MainWindow
{
    private UpdateService _updates = null!;
    private DispatcherTimer? _updateTimer;
    private readonly CancellationTokenSource _updateLifetime = new();
    private CancellationTokenSource? _downloadCancellation;
    private bool _updateBusy;
    private bool _updateReminderOpen;
    private string? _updateMessage;
    private bool _updateMessageIsError;

    private void InitializeUpdates()
    {
        _updates = new UpdateService();
        try
        {
            if (File.Exists(UpdateService.ResultPath))
            {
                var result = System.Text.Json.JsonSerializer.Deserialize<UpdateInstallResult>(File.ReadAllText(UpdateService.ResultPath));
                if (result is not null)
                {
                    _updateMessage = result.Success ? $"Updated to version {result.Version}." : $"Update was not completed: {result.Message}";
                    _updateMessageIsError = !result.Success;
                    if (!result.Success) Logger.Warn(_updateMessage);
                }
                File.Delete(UpdateService.ResultPath);
            }
        }
        catch (Exception ex) { Logger.Warn($"Update result could not be read: {ex.Message}"); }
        RefreshUpdatesPage();
        _updateTimer = new DispatcherTimer { Interval = TimeSpan.FromMinutes(1) };
        _updateTimer.Tick += async (_, _) => await CheckUpdatesAsync(false);
        _updateTimer.Start();
        RootGrid.Loaded += (_, _) => _dq.TryEnqueue(async () => await CheckUpdatesAsync(false));
    }

    private void RefreshUpdatesPage()
    {
        if (_updates is null) return;
        InstalledVersionLabel.Text = $"Version {_updates.CurrentVersion.ToString(3)}";
        UpdateCheck state = _updates.State;
        bool available = _updates.HasUpdate;
        UpdateHeading.Text = available ? "An update is available" : state.Release is null ? "Keep Instant Replay up to date" : "You're up to date";
        UpdateDescription.Text = available ? $"Version {state.Release!.Version} is ready to download." : state.Release is null
            ? "Check for the latest stable release on GitHub." : $"Latest release: {state.Release.Version}. Your version is current or newer.";
        CheckUpdatesButton.IsEnabled = !_updateBusy;
        InstallUpdateButton.Visibility = available ? Visibility.Visible : Visibility.Collapsed;
        InstallUpdateButton.IsEnabled = !_updateBusy && state.Error is null;
        ReleaseNotesCard.Visibility = state.Release is not null && available ? Visibility.Visible : Visibility.Collapsed;
        UpdateReleaseNotes.Text = string.IsNullOrWhiteSpace(state.Release?.Notes) ? "No release notes were provided." : state.Release.Notes;
        LastUpdateCheckLabel.Text = state.LastAttemptUtc == DateTimeOffset.MinValue ? "Not checked yet" : $"Last checked {state.LastAttemptUtc.ToLocalTime():g}";
        UpdateStatusBar.IsOpen = _updateMessage is not null || state.Error is not null;
        UpdateStatusBar.Severity = _updateMessage is not null ? (_updateMessageIsError ? InfoBarSeverity.Error : InfoBarSeverity.Informational) : InfoBarSeverity.Error;
        UpdateStatusBar.Message = _updateMessage ?? state.Error ?? "";
    }

    private async Task CheckUpdatesAsync(bool manual)
    {
        if (_updateBusy || _updateReminderOpen || _isClosingForReal) return;
        if (!manual && !_updates.IsDue(DateTimeOffset.UtcNow))
        {
            await ShowUpdateReminderAsync();
            return;
        }
        _updateBusy = true;
        _updateMessage = "Checking for updates…";
        _updateMessageIsError = false;
        UpdateProgress.Visibility = Visibility.Visible;
        UpdateProgress.IsIndeterminate = true;
        RefreshUpdatesPage();
        try
        {
            await _updates.CheckAsync(_updateLifetime.Token);
            _updateMessage = null;
            if (!manual && _updates.HasUpdate) NavUpdatesItem.InfoBadge = new InfoBadge { Value = 1 };
            else NavUpdatesItem.InfoBadge = null;
        }
        catch (OperationCanceledException) { }
        finally
        {
            _updateBusy = false;
            UpdateProgress.Visibility = Visibility.Collapsed;
            if (!_isClosingForReal) RefreshUpdatesPage();
        }
        await ShowUpdateReminderAsync();
    }

    private async void CheckUpdates_Click(object sender, RoutedEventArgs e) => await CheckUpdatesAsync(true);

    private async Task ShowUpdateReminderAsync()
    {
        if (_updates is null || _updateBusy || _updateReminderOpen || _isClosingForReal || _isHidden
            || RootGrid.XamlRoot is null || GetForegroundWindow() != _hwnd || !_updates.IsReminderDue(DateTimeOffset.UtcNow)) return;
        _updateReminderOpen = true;
        bool accepted = false;
        try
        {
            var dialog = new ContentDialog
            {
                Title = "An update is available",
                Content = $"Instant Replay {_updates.State.Release!.Version} is available. Would you like to update now?\n\nThe app will download the update, close, install it, and reopen automatically. Save any replay you want to keep first: restarting clears the current buffer. Windows may ask for administrator approval.",
                PrimaryButtonText = "Update now", CloseButtonText = "Not now",
                DefaultButton = ContentDialogButton.Close, XamlRoot = RootGrid.XamlRoot,
                RequestedTheme = RootGrid.RequestedTheme
            };
            dialog.Opened += (_, _) => _updates.MarkReminderShown(DateTimeOffset.UtcNow);
            accepted = await dialog.ShowAsync() == ContentDialogResult.Primary;
        }
        catch (Exception ex) { Logger.Debug($"Update reminder could not be shown: {ex.Message}"); }
        finally { _updateReminderOpen = false; }
        if (accepted && !_isClosingForReal)
        {
            NavView.SelectedItem = NavUpdatesItem;
            await InstallUpdateAsync(true);
        }
    }

    private async void InstallUpdate_Click(object sender, RoutedEventArgs e) => await InstallUpdateAsync(false);

    private async Task InstallUpdateAsync(bool confirmed)
    {
        if (_updateBusy || !_updates.HasUpdate || _isClosingForReal) return;
        _updateBusy = true;
        RefreshUpdatesPage();
        string? directory = null;
        bool handoffStarted = false;
        try
        {
            if (!confirmed)
            {
                var dialog = new ContentDialog
                {
                    Title = "Install update?",
                    Content = "Instant Replay will download the update, close, install it, and reopen automatically. The current replay buffer will be cleared. Save any replay you want to keep before continuing. Windows may ask for administrator approval.",
                    PrimaryButtonText = "Download and install", CloseButtonText = "Cancel",
                    DefaultButton = ContentDialogButton.Close, XamlRoot = RootGrid.XamlRoot,
                    RequestedTheme = RootGrid.RequestedTheme
                };
                if (await dialog.ShowAsync() != ContentDialogResult.Primary) return;
            }
            _downloadCancellation = CancellationTokenSource.CreateLinkedTokenSource(_updateLifetime.Token);
            _updateMessage = "Checking the release before downloading…";
            _updateMessageIsError = false;
            UpdateProgress.Visibility = Visibility.Visible;
            UpdateProgress.IsIndeterminate = true;
            CancelUpdateButton.Visibility = Visibility.Visible;
            RefreshUpdatesPage();
            await _updates.CheckAsync(_downloadCancellation.Token);
            if (_updates.State.Error is { } error) throw new InvalidOperationException(error);
            if (!_updates.HasUpdate) { _updateMessage = "You're already up to date."; return; }
            UpdateRelease release = _updates.State.Release!;
            directory = Path.Combine(Path.GetTempPath(), "InstantReplay-Updates", Guid.NewGuid().ToString("N"));
            UpdateProgress.IsIndeterminate = false;
            UpdateProgress.Value = 0;
            var progress = new Progress<double>(value =>
            {
                UpdateProgress.Value = value;
                UpdateStatusBar.Message = $"Downloading version {release.Version}… {value:0}%";
            });
            _updateMessage = $"Downloading version {release.Version}…";
            RefreshUpdatesPage();
            string installer = await _updates.DownloadAsync(release, directory, progress, _downloadCancellation.Token);
            CancelUpdateButton.Visibility = Visibility.Collapsed;
            _updateMessage = "Installer verified. Preparing to restart…";
            RefreshUpdatesPage();
            await UpdateService.StartInstallerAsync(installer, release, _downloadCancellation.Token);
            handoffStarted = true;
            ExitApplicationAsync();
        }
        catch (OperationCanceledException)
        {
            _updateMessage = "Update download cancelled. Nothing was installed.";
            _updateMessageIsError = false;
        }
        catch (Exception ex)
        {
            _updateMessage = $"Update could not be installed: {ex.Message}";
            _updateMessageIsError = true;
            Logger.Error("Update installation failed", ex);
        }
        finally
        {
            _downloadCancellation?.Dispose();
            _downloadCancellation = null;
            _updateBusy = false;
            CancelUpdateButton.Visibility = Visibility.Collapsed;
            UpdateProgress.Visibility = Visibility.Collapsed;
            if (!handoffStarted && directory is not null)
            {
                try
                {
                    foreach (string name in new[] { UpdateService.InstallerName, UpdateService.InstallerName + ".partial", "install-update.ps1", "install-update.json", "ready" })
                        File.Delete(Path.Combine(directory, name));
                    Directory.Delete(directory);
                }
                catch (Exception ex) { Logger.Debug($"Update cleanup: {ex.Message}"); }
            }
            if (!_isClosingForReal) RefreshUpdatesPage();
        }
    }

    private void CancelUpdate_Click(object sender, RoutedEventArgs e) => _downloadCancellation?.Cancel();
}
