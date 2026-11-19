namespace InstantReplay.Models;

public enum EncoderPreference
{
    Auto = 0,
    Nvenc = 1,
    Amf = 2,
    Qsv = 3,
    X264 = 4,
}

/// <summary>
/// Bits returned by the engine's encoder probe. Keep in sync with the ENC_* constants in
/// engine/engine_core/src/ir_common.h (C++ engine). The Media Foundation bits are internal: they
/// feed the Auto fallback chain but are never offered as a user-selectable encoder.
/// </summary>
[Flags]
public enum EncoderAvailability : uint
{
    None = 0,
    H264Nvenc = 1 << 0,
    HevcNvenc = 1 << 1,
    H264Amf = 1 << 2,
    HevcAmf = 1 << 3,
    H264Qsv = 1 << 4,
    HevcQsv = 1 << 5,
    H264X264 = 1 << 6,
    HevcX265 = 1 << 7,
    H264MediaFoundation = 1 << 8,
    HevcMediaFoundation = 1 << 9,
    All = (1 << 10) - 1,
}

public static class EncoderAvailabilityMap
{
    public static readonly (string Key, EncoderAvailability Bit)[] Entries =
    [
        ("h264_nvenc", EncoderAvailability.H264Nvenc),
        ("hevc_nvenc", EncoderAvailability.HevcNvenc),
        ("h264_amf", EncoderAvailability.H264Amf),
        ("hevc_amf", EncoderAvailability.HevcAmf),
        ("h264_qsv", EncoderAvailability.H264Qsv),
        ("hevc_qsv", EncoderAvailability.HevcQsv),
        ("libx264", EncoderAvailability.H264X264),
        ("libx265", EncoderAvailability.HevcX265),
        ("h264_mf", EncoderAvailability.H264MediaFoundation),
        ("hevc_mf", EncoderAvailability.HevcMediaFoundation),
    ];

    public static Dictionary<string, bool> ToDictionary(EncoderAvailability mask)
    {
        var map = new Dictionary<string, bool>(Entries.Length);
        foreach (var (key, bit) in Entries) map[key] = (mask & bit) != 0;
        return map;
    }

    /// <summary>Returns None when the map is missing or has no entry for any known encoder.</summary>
    public static EncoderAvailability FromDictionary(IReadOnlyDictionary<string, bool>? map)
    {
        if (map is null || map.Count == 0) return EncoderAvailability.None;
        var mask = EncoderAvailability.None;
        foreach (var (key, bit) in Entries)
            if (map.TryGetValue(key, out bool on) && on) mask |= bit;
        return mask;
    }

    public static bool IsComplete(IReadOnlyDictionary<string, bool>? map) =>
        map is not null && Entries.All(e => map.ContainsKey(e.Key));
}

public readonly record struct VideoQualityProfile(string Name, int BitrateKbps, int MaxWidth, int MaxHeight)
{
    public static VideoQualityProfile FromPreset(string preset, int fps)
    {
        var (name, at60Fps) = preset switch
        {
            "Quality" or "High" => ("High", 30000),
            "Balanced" => ("Balanced", 20000),
            _ => ("Low", 12000)
        };
        int rate = Math.Clamp(fps, 15, 240);
        return new VideoQualityProfile(name, at60Fps * rate / 60, 3840, 2160);
    }
    // Bitrate is derived from the quality preset, target FPS and the actual output resolution.
    // Values are calibrated so 1080p60 matches the documented Low/Balanced/High table; other
    // resolutions scale with pixel count, clamped so 4K does not demand an unreasonable rate.
    public static int TargetBitrateKbps(string preset, int fps, int width, int height)
    {
        int baseKbps = FromPreset(preset, fps).BitrateKbps;
        int w = Math.Max(16, width);
        int h = Math.Max(16, height);
        double pixels = (double)w * h;
        double scale = Math.Clamp(pixels / (1920.0 * 1080.0), 0.35, 2.0);
        return (int)Math.Round(baseKbps * scale / 1000.0) * 1000;
    }

    public string Label => Name;
}
