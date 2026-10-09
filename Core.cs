using System.Reflection;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;

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

public enum BackdropKind
{
    Acrylic = 0,
    Mica = 1,
    None = 2,
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

    public BackdropKind Backdrop { get; set; } = BackdropKind.Mica;

    public bool DarkMode { get; set; } = true;

    public uint HotkeyModifiers { get; set; } = 0x0002;
    public uint HotkeyVk { get; set; } = 0x6A;

    public string QualityPreset { get; set; } = "Balanced";

    public string EncoderChoice { get; set; } = "Auto";

    public string CaptureMethod { get; set; } = "Wgc";

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
                    if (loaded.Backdrop == BackdropKind.None)
                        loaded.Backdrop = BackdropKind.Mica;
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
            "Manage ransomware protection → Controlled folder access, and allow Instant Replay.exe " +
            "through it, then try again.",
        SaveReplayOutcome.MergeFailed => "Failed to save replay (see log for details).",
        _ => "Failed to save replay (see log for details).",
    };
}

public sealed class RecorderEngine : IDisposable
{
    private readonly object _lock = new();
    private readonly object _opLock = new();
    private readonly Settings _settings;
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
                return buffering;
            }
        }
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

    private static int ResolveBitrate(string qualityPreset) => qualityPreset switch
    {
        "Quality" => 9000,
        "Balanced" => 6000,
        _ => 4000,
    };

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

            bool ok = _engine.StartBuffering(
                monitorIndex: -1,
                ringSeconds: Math.Max(_settings.ReplayDurationSeconds, 1),
                fps: _settings.FrameRate,
                bitrateKbps: ResolveBitrate(_settings.QualityPreset),
                captureAudio: _settings.RecordSystemAudio,
                preference: ResolveEncoder(_settings.EncoderChoice),
                method: ResolveCaptureMethod(_settings.CaptureMethod),
                captureMic: _settings.RecordMicrophone,
                micDevice: _settings.Microphone);

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

            Logger.Info($"Capture Started - Managed engine, FPS: {_settings.FrameRate}, " +
                        $"Ring: {_settings.ReplayDurationSeconds}s, " +
                        $"System audio: {(_settings.RecordSystemAudio ? "on" : "off")}, " +
                        $"Mic: {(_settings.RecordMicrophone ? (_settings.Microphone.Length > 0 ? _settings.Microphone : "default") : "off")}");
            return true;
        }
    }

    public void Stop()
    {
        lock (_opLock)
        {
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
        Stop();
        return Start(tempDir, wipeSegments);
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
        bool captureMic = false, string? micDevice = null)
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
            var (capW, capH) = ResolutionCap(preference);

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

    private static (int, int) ResolutionCap(EncoderPreference preference)
    {
        return preference switch
        {
            EncoderPreference.Hardware => (1920, 1080),
            EncoderPreference.Software => (1920, 1080),
            _ => (1600, 900),
        };
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