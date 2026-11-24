using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
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

    // Hidden legacy field. Old files are migrated into GpuEncoder/GpuPreference on
    // load; the settings page no longer exposes individual encoders.
    public string EncoderChoice { get; set; } = "Auto";
    public bool GpuEncoder { get; set; } = true;
    public string GpuPreference { get; set; } = "Prefer dGPU";

    [JsonPropertyName("buffer_method")]
    public string BufferMethod { get; set; } = "Continuous";

    public string VideoCodec { get; set; } = "H264";

    public string RateControl { get; set; } = "Vbr";

    public string EncoderPreset { get; set; } = "Balanced";

    public string Container { get; set; } = "Mp4";

    public string AudioCodec { get; set; } = "AAC";
    public int AudioBitrate { get; set; } = 160;
    public bool ApplicationLocalAudio { get; set; } = true;

    // Media Foundation writes MP4 only. Legacy container choices map to .mp4.
    public static string ContainerExtension(string container) => "mp4";

    public string CaptureMethod { get; set; } = "Dxgi";
    public string CaptureTarget { get; set; } = "Display";
    public List<GameRule> SpecificWindows { get; set; } = new List<GameRule>();
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

    /// <summary>Maps a stored preset (including legacy names) to a valid seven-level name.</summary>
    private static string MigratePreset(string stored, string fallback, bool speedScale)
    {
        if (VideoQualityProfile.PresetNames.Contains(stored)) return stored;
        int idx = stored switch
        {
            "Low" => speedScale ? 2 : 0,
            "Speed" => 2,
            "Quality" => 4,
            "High" => speedScale ? 4 : 6,
            _ => VideoQualityProfile.PresetIndex(fallback),
        };
        return VideoQualityProfile.PresetNames[idx];
    }

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
                    if (loaded.CaptureTarget is not ("Display" or "SpecificWindows")) loaded.CaptureTarget = "Display";
                    // Whole Desktop can choose its capture API. Game Capture ignores this
                    // value and keeps switching between WGC and DXGI per detected window.
                    loaded.CaptureMethod = string.Equals(loaded.CaptureMethod, "Wgc", StringComparison.OrdinalIgnoreCase)
                        ? "Wgc" : "Dxgi";
                    loaded.SpecificWindows ??= new List<GameRule>();
                    // Rounded corners are always enabled for WGC now that the option is hidden.
                    loaded.CaptureRoundedCorners = true;

                    // A stored software encoder maps to GPU off. Every other legacy encoder
                    // maps to GPU on with the default dGPU preference.
                    string storedEncoder = loaded.EncoderChoice ?? "Auto";
                    loaded.GpuEncoder = !string.Equals(storedEncoder, "MfSoftware", StringComparison.OrdinalIgnoreCase);
                    loaded.GpuPreference = loaded.GpuPreference is "Prefer iGPU" or "Prefer dGPU"
                        ? loaded.GpuPreference : "Prefer dGPU";
                    loaded.EncoderChoice = "Auto";

                    loaded.VideoCodec = loaded.VideoCodec switch
                    {
                        "Hevc" or "H264" or "Av1" => loaded.VideoCodec,
                        _ => "H264",
                    };
                    loaded.AudioCodec = string.Equals(loaded.AudioCodec, "FLAC", StringComparison.OrdinalIgnoreCase)
                        ? "FLAC" : "AAC";
                    if (loaded.AudioBitrate is not (96 or 128 or 160 or 192)) loaded.AudioBitrate = 160;

                    // Hidden legacy values are pinned to safe equivalents: MP4 output,
                    // VBR and formula-driven quality.
                    loaded.Container = "Mp4";
                    loaded.RateControl = "Vbr";
                    loaded.BufferMethod = string.Equals(loaded.BufferMethod, "Segmented", StringComparison.OrdinalIgnoreCase)
                        ? "Segmented" : "Continuous";

                    // Migrate the old Speed/Balanced/Quality presets onto the
                    // seven-level ultra-fast -> slowest scale. The quality preset also
                    // drives the native encoder quality now, so keep both in sync.
                    loaded.QualityPreset = MigratePreset(loaded.QualityPreset ?? "Balanced", "Balanced", speedScale: false);
                    loaded.EncoderPreset = loaded.QualityPreset;

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
            CaptureRoundedCorners = true;
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