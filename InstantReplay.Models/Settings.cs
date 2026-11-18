using System.Reflection;
using System.Text.Json;
using System.Text.Json.Serialization;
using InstantReplay.Services;

namespace InstantReplay.Models;

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

    /// <summary>
    /// Result of the last encoder probe (frames were actually pushed through each encoder), keyed
    /// by FFmpeg encoder name, e.g. "h264_nvenc": true. Null or incomplete means detection has not
    /// run yet and the app runs it before recording starts.
    /// </summary>
    [JsonPropertyName("encoders")]
    public Dictionary<string, bool>? DetectedEncoders { get; set; }

    public string VideoCodec { get; set; } = "H264";

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
                    if (loaded.EncoderChoice is not ("Auto" or "Nvenc" or "Amf" or "Qsv" or "X264" or "X265" or "Software"))
                        loaded.EncoderChoice = "Auto";
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