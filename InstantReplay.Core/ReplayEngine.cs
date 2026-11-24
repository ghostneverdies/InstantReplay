using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using System.Text;
using InstantReplay.Models;
using InstantReplay.Native;
using InstantReplay.Services;

namespace InstantReplay.Core;

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
    public const int SaveLowMemory = IrNative.LowMemory;
    public const int SavePending = IrNative.SavePending;

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

    public static string LastStartError { get; private set; } = "";

    public static string GetLastErrorMessage()
    {
        try
        {
            var buffer = new byte[2048];
            int n = IrNative.IR_LastErrorMessage(buffer, buffer.Length);
            return n > 0 ? System.Text.Encoding.UTF8.GetString(buffer, 0, Math.Min(n, buffer.Length - 1)) : "";
        }
        catch { return ""; }
    }

    public bool StartBuffering(int monitorIndex, int ringSeconds, int fps, int bitrateKbps,
        bool captureAudio, int preference, CaptureMethod method = CaptureMethod.Dxgi,
        bool captureMic = false, string? micDevice = null, IntPtr targetWindow = default, uint captureOptions = 11,
        int outputWidthCap = 3840, int outputHeightCap = 2160, int encoderOptions = 0,
        int canvasWidth = 0, int canvasHeight = 0)
    {
        lock (_opLock)
        {
            GetStatus(out bool buffering, out _, out _, out _, out _, out _, out _, out _, out _, out _);
            if (buffering) return true;
            if (!Initialize()) return false;

            int mon = monitorIndex;
            int secs = Math.Max(ringSeconds, 1);
            int rate = Compat.Clamp(fps, 15, 240);
            int bps = Math.Max(bitrateKbps, 1);
            int capW = Math.Max(0, outputWidthCap), capH = Math.Max(0, outputHeightCap);

            if (canvasWidth > 0 && canvasHeight > 0)
                IrNative.IR_SetCanvasHint(_engine, canvasWidth, canvasHeight);
            if (IrNative.IR_SetCaptureTarget(_engine, targetWindow, captureOptions) != IrNative.OK)
            {
                _hasError = true;
                Logger.Error("[Engine] Capture target is unavailable. Select an open, non-minimized window.");
                return false;
            }
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
                preference | encoderOptions, (int)method, out origin);
            if (rc != IrNative.OK)
            {
                _hasError = true;
                Logger.Error($"[Engine] IR_Start failed (rc={rc}). Capture could not start.");
                LastStartError = GetLastErrorMessage();
                if (LastStartError.Length > 0) Logger.Error($"[Engine] {LastStartError}");
                return false;
            }

            if (captureMic) ReportMicStartup(wantedMic);

            _hasError = false;
            LastStartError = "";
            int active = IrNative.IR_ActiveEncoder(_engine);
            int hwHr = IrNative.IR_HwInitHr(_engine);
            string encName = active switch
            {
                20 => "MF HEVC (Intel Quick Sync)",
                21 => "MF HEVC (Intel Gen12 / Arc, VP12)",
                22 => "MF HEVC (NVIDIA NVENC)",
                23 => "MF HEVC (AMD AMF)",
                24 => "MF HEVC (software)",
                30 => "MF H.264 (Intel Quick Sync)",
                31 => "MF H.264 (Intel Gen12 / Arc, VP12)",
                32 => "MF H.264 (NVIDIA NVENC)",
                33 => "MF H.264 (AMD AMF)",
                34 => "MF H.264 (software)",
                40 => "MF AV1 (Intel Quick Sync)",
                41 => "MF AV1 (Intel Gen12 / Arc, VP12)",
                42 => "MF AV1 (NVIDIA NVENC)",
                43 => "MF AV1 (AMD AMF)",
                44 => "MF AV1 (software)",
                _ => "unknown",
            };
            Logger.Info($"[Engine] Encoder: {encName} (requested {preference}){(hwHr != 0 ? $", hardware path unavailable: code 0x{hwHr:X8}" : "")}");

            int activeCap = IrNative.IR_ActiveCaptureMethod(_engine);
            string capName = activeCap switch
            {
                0 => "WGC window capture",
                1 => "DXGI duplication",
                3 => "direct graphics hook",
                _ => "DXGI duplication",
            };
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
            OverlayBridge.SetCapture(false, 60);
            Logger.Info("[Engine] Buffering stopped");
        }
    }
    public void SetPaused(bool paused)
    {
        lock (_opLock)
        {
            if (_engine != IntPtr.Zero) IrNative.IR_SetPaused(_engine, paused ? 1 : 0);
        }
    }

    public bool SwitchSource(CaptureMethod method, int monitorIndex, IntPtr window, uint options,
                             int canvasWidth = 0, int canvasHeight = 0)
    {
        lock (_opLock)
        {
            if (_engine == IntPtr.Zero) return false;
            if (canvasWidth > 0 && canvasHeight > 0)
                IrNative.IR_SetCanvasHint(_engine, canvasWidth, canvasHeight);
            int rc = IrNative.IR_SetCaptureSource(_engine, (int)method, monitorIndex, window, options);
            if (rc != IrNative.OK)
            {
                LastStartError = GetLastErrorMessage();
                Logger.Warn($"[Engine] Switching capture to {method} failed (rc={rc}). {LastStartError}");
                return false;
            }
            Logger.Info($"[Engine] Capture switched to {method} (display {monitorIndex}, window 0x{window.ToInt64():X}).");
            return true;
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

    public int BeginSaveReplay(string outPath, int secondsBack, out ulong jobId)
    {
        jobId = 0;
        lock (_opLock)
        {
            if (_engine == IntPtr.Zero || string.IsNullOrWhiteSpace(outPath)) return SaveFailed;
            try
            {
                int rc = IrNative.IR_BeginSave(_engine, outPath, Math.Max(secondsBack, 1), out jobId);
                string detail = rc == SaveOk ? "" : $"; {GetLastErrorMessage()}";
                Logger.Info($"[Engine] IR_BeginSave('{outPath}', {secondsBack}s, job {jobId}) -> rc={rc}{detail}");
                return rc;
            }
            catch (Exception ex)
            {
                Logger.Error("[Engine] IR_BeginSave failed", ex);
                jobId = 0;
                return SaveFailed;
            }
        }
    }

    public bool TryPollSave(ulong jobId, out int progress, out int result)
    {
        progress = -1;
        result = SaveFailed;
        if (_engine == IntPtr.Zero || jobId == 0) return false;
        try
        {
            int pollRc = IrNative.IR_PollSave(_engine, jobId, out progress, out result);
            bool alive = pollRc == IrNative.OK;
            if (!alive)
            {
                progress = -1;
                result = SaveFailed;
            }
            return alive;
        }
        catch
        {
            progress = -1;
            result = SaveFailed;
            return false;
        }
    }

    public int SaveProgress
    {
        get
        {
            try { return _engine == IntPtr.Zero ? -1 : IrNative.IR_SaveProgress(); }
            catch { return -1; }
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

    public bool TryGetTelemetry(out long captured, out long paced, out long rejected,
        out long encoded, out long dropped, out int encoder, out long lastPts)
    {
        captured = paced = rejected = encoded = dropped = lastPts = 0;
        encoder = 0;
        if (_engine == IntPtr.Zero) return false;
        try
        {
            IrNative.IR_GetTelemetry(_engine, out captured, out paced, out rejected,
                out encoded, out dropped, out encoder, out lastPts);
            return true;
        }
        catch (Exception ex)
        {
            Logger.Warn($"[Engine] IR_GetTelemetry failed: {ex.Message}");
            return false;
        }
    }

    public bool TryGetTiming(out double mapMs, out double encodeMs, out double renderMs,
        out double submitMs, out double snapshotMs, out double unmapMs, out long frames)
    {
        mapMs = encodeMs = renderMs = submitMs = snapshotMs = unmapMs = 0;
        frames = 0;
        if (_engine == IntPtr.Zero) return false;
        try
        {
            IrNative.IR_GetTiming(_engine, out mapMs, out encodeMs, out renderMs,
                out submitMs, out snapshotMs, out unmapMs, out frames);
            return true;
        }
        catch (Exception ex)
        {
            Logger.Warn($"[Engine] IR_GetTiming failed: {ex.Message}");
            return false;
        }
    }

    public bool TryGetAudioDiagnostics(int which, out long activateHr, out long initHr,
        out long startHr, out int tag, out int rate, out int channels, out int bits,
        out int isFloat, out int convertFailures, out long lastFailHr)
    {
        activateHr = initHr = startHr = lastFailHr = 0;
        tag = rate = channels = bits = isFloat = convertFailures = 0;
        if (_engine == IntPtr.Zero) return false;
        try
        {
            IrNative.IR_AudioDiagnostics(_engine, which, out activateHr, out initHr,
                out startHr, out tag, out rate, out channels, out bits, out isFloat,
                out convertFailures, out lastFailHr);
            return true;
        }
        catch (Exception ex)
        {
            Logger.Warn($"[Engine] IR_AudioDiagnostics failed: {ex.Message}");
            return false;
        }
    }

    public string DescribeFailure()
    {
        if (_engine == IntPtr.Zero) return "engine not initialised";
        try
        {
            int last = IrNative.IR_LastHresult(_engine);
            int stage = IrNative.IR_LastFailStage(_engine);
            int failHr = IrNative.IR_LastFailHr(_engine);
            int capHr = IrNative.IR_LastCaptureHr(_engine);
            int method = IrNative.IR_ActiveCaptureMethod(_engine);
            string methodName = method switch
            {
                0 => "WGC",
                1 => "DXGI",
                3 => "graphics hook",
                _ => $"method {method}"
            };
            string err = GetLastErrorMessage();
            if (err.Length > 0) err = $"; {err}";
            return $"source={methodName}, capture=0x{capHr:X8}, last=0x{last:X8}, " +
                   $"stage={stage}, fail=0x{failHr:X8}{err}";
        }
        catch (Exception ex)
        {
            return $"diagnostics unavailable: {ex.Message}";
        }
    }

    public static List<(IntPtr Handle, string Title)> EnumerateWindows()
    {
        var result = new List<(IntPtr Handle, string Title)>();
        try
        {
            int count = IrNative.IR_GetWindowCount();
            for (int i = 0; i < count; i++)
            {
                var title = new StringBuilder(512);
                IrNative.IR_GetWindowInfo(i, out IntPtr handle, title, title.Capacity, out _, out _);
                if (handle != IntPtr.Zero && title.Length > 0) result.Add((handle, title.ToString()));
            }
        }
        catch (Exception error) { Logger.Warn($"[Engine] Window enumeration failed: {error.Message}"); }
        return result;
    }

    public static (int Width, int Height) MonitorSize(int index)
    {
        try
        {
            var sb = new StringBuilder(256);
            IrNative.IR_GetMonitorInfo(index, sb, sb.Capacity, out int width, out int height);
            return (width, height);
        }
        catch (Exception ex)
        {
            Logger.Warn($"[Engine] MonitorSize failed: {ex.Message}");
            return (1920, 1080);
        }
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
