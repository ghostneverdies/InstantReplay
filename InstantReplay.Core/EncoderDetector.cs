using InstantReplay.Models;
using InstantReplay.Native;
using InstantReplay.Services;

namespace InstantReplay.Core;

/// <summary>
/// Runs the engine's encoder probe. The probe opens every FFmpeg hardware/software encoder and the
/// manual Media Foundation hardware path and pushes real frames through each one, so an encoder is
/// only reported as available when it actually produced output on this machine.
/// </summary>
public static class EncoderDetector
{
    private static readonly SemaphoreSlim Gate = new(1, 1);

    /// <summary>Returns the per-encoder result keyed by FFmpeg name, or null if the probe could not run.</summary>
    public static async Task<Dictionary<string, bool>?> DetectAsync()
    {
        await Gate.WaitAsync().ConfigureAwait(false);
        try
        {
            return await Task.Run(() =>
            {
                try
                {
                    var started = DateTime.UtcNow;
                    var mask = (EncoderAvailability)IrNative.IR_ProbeEncoders() & EncoderAvailability.All;
                    var map = EncoderAvailabilityMap.ToDictionary(mask);
                    Logger.Info($"[Encoders] Detection finished in {(DateTime.UtcNow - started).TotalMilliseconds:F0} ms: " +
                                string.Join(", ", map.Select(kv => $"{kv.Key}={(kv.Value ? "yes" : "no")}")));
                    return map;
                }
                catch (Exception ex)
                {
                    Logger.Error("[Encoders] Detection failed", ex);
                    return null;
                }
            }).ConfigureAwait(false);
        }
        finally
        {
            Gate.Release();
        }
    }

    /// <summary>True when the stored result has an entry for every known encoder.</summary>
    public static bool HasResult(Settings settings) => EncoderAvailabilityMap.IsComplete(settings.DetectedEncoders);

    /// <summary>User-selectable encoders (Auto always is) that have at least one working codec variant.</summary>
    public static bool IsSelectable(EncoderPreference preference, IReadOnlyDictionary<string, bool>? detected)
    {
        if (preference == EncoderPreference.Auto || !EncoderAvailabilityMap.IsComplete(detected)) return true;
        bool Has(string a, string b) => (detected!.TryGetValue(a, out bool x) && x) || (detected.TryGetValue(b, out bool y) && y);
        return preference switch
        {
            EncoderPreference.Nvenc => Has("h264_nvenc", "hevc_nvenc"),
            EncoderPreference.Amf => Has("h264_amf", "hevc_amf"),
            EncoderPreference.Qsv => Has("h264_qsv", "hevc_qsv"),
            EncoderPreference.X264 => Has("libx264", "libx265"),
            _ => true,
        };
    }
}
