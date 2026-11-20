namespace InstantReplay.Models;

public enum EncoderPreference
{
    Auto = 0,
    // Legacy ordinals 1..4 and 9..10 are retired with the old external encoder
    // stack. They are kept only so stored settings can be migrated to Auto.
    MfQsv = 5,
    MfVp12 = 6,
    MfNvenc = 7,
    MfAmf = 8,
    MfSoftware = 11,
}

/// <summary>
/// Bits returned by the engine's encoder probe. Keep in sync with the ENC_* constants in
/// engine/engine_core/src/core.h. All encoders are Media Foundation MFTs.
/// </summary>
[Flags]
public enum EncoderAvailability : uint
{
    None = 0,
    H264MfQsv = 1 << 8,
    HevcMfQsv = 1 << 9,
    H264MfVp12 = 1 << 10,
    HevcMfVp12 = 1 << 11,
    H264MfNvenc = 1 << 12,
    HevcMfNvenc = 1 << 13,
    H264MfAmf = 1 << 14,
    HevcMfAmf = 1 << 15,
    Av1MfQsv = 1 << 16,
    Av1MfVp12 = 1 << 17,
    Av1MfNvenc = 1 << 18,
    Av1MfAmf = 1 << 19,
    H264MfSoftware = 1 << 20,
    HevcMfSoftware = 1 << 21,
    Av1MfSoftware = 1 << 22,
    All = (1 << 23) - 1,
}

/// <summary>Compact per-codec availability packed into IR_Start pref bits 24..31.</summary>
public static class CompactAvailability
{
    public const uint MfQsv = 1u << 4;
    public const uint MfVp12 = 1u << 5;
    public const uint MfNvenc = 1u << 6;
    public const uint MfAmf = 1u << 7;
}

public static class EncoderAvailabilityMap
{
    public static readonly (string Key, EncoderAvailability Bit)[] Entries =
    [
        ("h264_mf_qsv", EncoderAvailability.H264MfQsv),
        ("hevc_mf_qsv", EncoderAvailability.HevcMfQsv),
        ("h264_mf_vp12", EncoderAvailability.H264MfVp12),
        ("hevc_mf_vp12", EncoderAvailability.HevcMfVp12),
        ("h264_mf_nvenc", EncoderAvailability.H264MfNvenc),
        ("hevc_mf_nvenc", EncoderAvailability.HevcMfNvenc),
        ("h264_mf_amf", EncoderAvailability.H264MfAmf),
        ("hevc_mf_amf", EncoderAvailability.HevcMfAmf),
        ("av1_mf_qsv", EncoderAvailability.Av1MfQsv),
        ("av1_mf_vp12", EncoderAvailability.Av1MfVp12),
        ("av1_mf_nvenc", EncoderAvailability.Av1MfNvenc),
        ("av1_mf_amf", EncoderAvailability.Av1MfAmf),
        ("h264_mf_software", EncoderAvailability.H264MfSoftware),
        ("hevc_mf_software", EncoderAvailability.HevcMfSoftware),
        ("av1_mf_software", EncoderAvailability.Av1MfSoftware),
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

    /// <summary>
    /// Packs the full probe mask into the single per-codec byte the native
    /// encoder sees in IR_Start's pref bits 24..31. Software MFT availability is
    /// inferred from these vendor bits by the native engine.
    /// </summary>
    public static uint Compact(EncoderAvailability mask, string codec)
    {
        uint r = 0;
        bool av1 = codec == "Av1";
        bool hevc = codec == "Hevc";
        if ((mask & (av1 ? EncoderAvailability.Av1MfQsv : hevc ? EncoderAvailability.HevcMfQsv : EncoderAvailability.H264MfQsv)) != 0)
            r |= CompactAvailability.MfQsv;
        if ((mask & (av1 ? EncoderAvailability.Av1MfVp12 : hevc ? EncoderAvailability.HevcMfVp12 : EncoderAvailability.H264MfVp12)) != 0)
            r |= CompactAvailability.MfVp12;
        if ((mask & (av1 ? EncoderAvailability.Av1MfNvenc : hevc ? EncoderAvailability.HevcMfNvenc : EncoderAvailability.H264MfNvenc)) != 0)
            r |= CompactAvailability.MfNvenc;
        if ((mask & (av1 ? EncoderAvailability.Av1MfAmf : hevc ? EncoderAvailability.HevcMfAmf : EncoderAvailability.H264MfAmf)) != 0)
            r |= CompactAvailability.MfAmf;
        return r;
    }
}

public readonly record struct VideoQualityProfile(string Name, int BitrateKbps, int MaxWidth, int MaxHeight)
{
    // Seven levels, ultra fast (0) -> slowest (6). The ordinal is the value
    // the native engine receives in IR_Start pref bits 12..14.
    public static readonly string[] PresetNames =
        ["UltraFast", "VeryFast", "Fast", "Balanced", "Slow", "Slower", "Slowest"];
    private static readonly string[] PresetLabels =
        ["Ultra fast", "Very fast", "Fast", "Balanced", "Slow", "Slower", "Slowest"];
    // Base 1080p/60 kbps. Higher quality (slower preset) gets a higher target;
    // TargetBitrateKbps scales this by pixel count and frame rate.
    private static readonly int[] BaseKbps60 = [12000, 14000, 16000, 20000, 24000, 27000, 30000];

    public static int PresetIndex(string preset) => preset switch
    {
        "UltraFast" => 0,
        "VeryFast" => 1,
        "Fast" => 2,
        "Balanced" => 3,
        "Slow" => 4,
        "Slower" => 5,
        "Slowest" => 6,
        // Legacy stored values. Old encoder "Speed"/"Quality" become
        // Fast/Slow; old quality "Low"/"High" become UltraFast/Slowest.
        "Low" => 0,
        "Speed" => 2,
        "Quality" => 4,
        "High" => 6,
        _ => 3,
    };

    public static VideoQualityProfile FromPreset(string preset, int fps)
    {
        int idx = PresetIndex(preset);
        int rate = Math.Clamp(fps, 15, 240);
        return new VideoQualityProfile(PresetNames[idx], BaseKbps60[idx] * rate / 60, 3840, 2160);
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

    public string Label => PresetLabels[PresetIndex(Name)];
}
