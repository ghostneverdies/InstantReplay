namespace InstantReplay.Models;

/// <summary>Hardware or software encoder the user wants the engine to prefer.</summary>
public enum EncoderPreference
{
    Auto = 0,
    Nvenc = 1,
    Amf = 2,
    Qsv = 3,
    X264 = 4,
    MediaFoundation = 5,
}

/// <summary>Bitrate and canvas derived from the quality preset and frame rate.</summary>
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
