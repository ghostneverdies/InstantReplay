using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using System.Threading;
using InstantReplay.Models;
using InstantReplay.Native;
using InstantReplay.Services;

namespace InstantReplay.Core;

public static class EncoderDetector
{
    private static readonly SemaphoreSlim Gate = new(1, 1);
    private static Dictionary<string, bool>? _lastResult;

    public static Dictionary<string, bool>? LastResult => Volatile.Read(ref _lastResult);

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
                    Volatile.Write(ref _lastResult, map);
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

    public static bool HasResult => EncoderAvailabilityMap.IsComplete(LastResult);

    public static bool IsSelectable(EncoderPreference preference, IReadOnlyDictionary<string, bool>? detected)
    {
        if (preference == EncoderPreference.Auto || !EncoderAvailabilityMap.IsComplete(detected)) return true;
        bool On(string key) => detected!.TryGetValue(key, out bool v) && v;
        bool Has(string a, string b) => On(a) || On(b);
        return preference switch
        {
            EncoderPreference.MfQsv => Has("h264_mf_qsv", "hevc_mf_qsv"),
            EncoderPreference.MfVp12 => Has("h264_mf_vp12", "hevc_mf_vp12"),
            EncoderPreference.MfNvenc => Has("h264_mf_nvenc", "hevc_mf_nvenc"),
            EncoderPreference.MfAmf => Has("h264_mf_amf", "hevc_mf_amf"),
            EncoderPreference.MfSoftware => Has("h264_mf_software", "hevc_mf_software"),
            _ => true,
        };
    }
}
