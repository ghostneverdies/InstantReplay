namespace InstantReplay.Models;

public enum EncoderPreference
{
    Auto = 0,
    Nvenc = 1,
    Amf = 2,
    Qsv = 3,
    X264 = 4,
    MediaFoundation = 5,
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
    public string Label => Name;
}
