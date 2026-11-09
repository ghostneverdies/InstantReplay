using Microsoft.Win32.SafeHandles;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text.Json;
using System.Text.Json.Serialization;
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
    public bool NavigationPaneOpen { get; set; } = true;
    [JsonPropertyName("warning_dismissed")]
    public bool WarningDismissed { get; set; }
    [JsonPropertyName("dismissed_game_infobar_1")]
    public bool DismissedGameInfoBar1 { get; set; }
    [JsonPropertyName("dismissed_game_infobar_2")]
    public bool DismissedGameInfoBar2 { get; set; }

    public uint HotkeyModifiers { get; set; } = 0x0002;
    public uint HotkeyVk { get; set; } = 0x6A;

    public string QualityPreset { get; set; } = "Balanced";
    public int BitrateKbps { get; set; }

    public string EncoderChoice { get; set; } = "Auto";

    public string VideoCodec { get; set; } = "Auto";

    public string RateControl { get; set; } = "Cbr";

    public string EncoderPreset { get; set; } = "Balanced";

    public string Container { get; set; } = "Mp4";

    public static string ContainerExtension(string container) => container switch
    {
        "Mkv" => "mkv",
        "Mov" => "mov",
        "Ts" => "ts",
        "Flv" => "flv",
        _ => "mp4",
    };

    public string CaptureMethod { get; set; } = "Dxgi";
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
                var loaded = JsonSerializer.Deserialize<Settings>(File.ReadAllText(path),
                    new JsonSerializerOptions { PropertyNameCaseInsensitive = true });
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
        return new Settings();
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
                string jsonName = property.GetCustomAttribute<JsonPropertyNameAttribute>()?.Name ?? name;
                doc[jsonName] = JsonSerializer.SerializeToElement(property.GetValue(this), options);
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
    private const uint FileAppendData = 0x0004;
    private const uint ShareAll = 0x0001 | 0x0002 | 0x0004;
    private const uint OpenAlways = 4;
    private const uint FileAttributeNormal = 0x80;

    private static readonly object s_lock = new();
    private static SafeFileHandle? s_handle;
    private static bool s_debugMode;

    public static string LogPath { get; private set; } = "";

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern SafeFileHandle CreateFileW(
        string fileName,
        uint desiredAccess,
        uint shareMode,
        IntPtr securityAttributes,
        uint creationDisposition,
        uint flagsAndAttributes,
        IntPtr templateFile);

    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool WriteFile(
        SafeFileHandle file,
        byte[] buffer,
        uint bytesToWrite,
        out uint bytesWritten,
        IntPtr overlapped);

    public static void Init(bool debugMode)
    {
        lock (s_lock)
        {
            s_debugMode = debugMode;
            string fileName = $"debug_{DateTime.Now:yyyyMMdd_HHmmss}.txt";
            LogPath = Path.Combine(Path.GetTempPath(), fileName);

            s_handle?.Dispose();
            s_handle = CreateFileW(LogPath, FileAppendData, ShareAll, IntPtr.Zero,
                OpenAlways, FileAttributeNormal, IntPtr.Zero);
            if (s_handle.IsInvalid)
            {
                int error = Marshal.GetLastWin32Error();
                s_handle = null;
                throw new IOException($"Unable to open debug log '{LogPath}' (Win32 {error}).");
            }
        }
        Info("Application Started");
        Info($"Version: {typeof(Logger).Assembly.GetName().Version?.ToString(3)}");
    }

    public static bool DebugMode => s_debugMode;

    public static void SetDebugMode(bool enabled)
    {
        s_debugMode = enabled;
        OverlayBridge.Refresh();
    }

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
        byte[] line = Encoding.UTF8.GetBytes(
            $"[{DateTime.Now:HH:mm:ss.fff}] [{level}] {message}\r\n");
        lock (s_lock)
        {
            if (s_handle is null || s_handle.IsInvalid) return;
            WriteFile(s_handle, line, (uint)line.Length, out _, IntPtr.Zero);
        }
    }

    public static void Shutdown()
    {
        lock (s_lock)
        {
            s_handle?.Dispose();
            s_handle = null;
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
            // Hardware HEVC tiers. AVC backends scale these internally to
            // 40/28/18 Mbps because H.264 needs more bits at equal quality.
            "Quality" or "High" => ("High", 30000),
            "Balanced" => ("Balanced", 20000),
            _ => ("Low", 12000)
        };
        int rate = Math.Clamp(fps, 15, 240);
        return new VideoQualityProfile(name, at60Fps * rate / 60, 3840, 2160);
    }
    public string Label => Name;
}

