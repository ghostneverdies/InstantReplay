using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using System.Text.Json;
using InstantReplay.Models;
using InstantReplay.Services;

namespace InstantReplay.Core;

public sealed class RecorderEngine : IDisposable
{
    private volatile bool _armed;
    private bool _specific;
    private bool _paused;
    private GameDetector? _detector;
    private System.Threading.Timer? _targetTimer;
    private CaptureWindow? _target;
    private IntPtr _candidate;
    private DateTime _candidateSince;
    private DateTime _startedAt;
    private DateTime _retryAt;
    private string _targetState = "STOPPED";
    public bool IsPaused { get { lock (_opLock) return _armed && _specific && _paused; } }
    public string TargetState { get { lock (_opLock) return _targetState; } }
    private readonly object _lock = new();
    private readonly object _opLock = new();
    private readonly Settings _settings;
    private Settings _captureSettings = new();
    private readonly ReplayEngine _engine = new();
    private int _saveInFlight;
    private long _activeSaveJobId;

    private long _lastHealthTicks = -1;
    private int _noProgressPolls;
    private uint _hookedPid;
    private bool _overlayReady;
    private DateTime _hookRetryAt;
    private uint _policyPid;
    private string _activeKey = "";
    private string? _pendingKey;
    private DateTime _pendingKeySince;
    private DateTime _switchRetryAt;
    private DateTime _targetLostSince;
    private DateTime _moduleCheckAt;
    private DateTime _analysisMissSince;

    private uint _antiCheatPid;
    private string _antiCheatModule = "";
    private int _antiCheatChecks;
    private DateTime _antiCheatNextCheck;
    private int _pollMs = 200;
    private WindowMode? _lastMode;
    private int _lastMonitor = -1;

    private volatile int _captureGeneration;
    public int CaptureGeneration => _captureGeneration;

    public RecorderEngine(Settings settings)
    {
        _settings = settings;
    }

    public bool IsRunning
    {
        get
        {
            lock (_lock)
            {
                _engine.GetStatus(out bool buffering, out _, out _, out _, out _, out _,
                    out _, out _, out _, out _);
                return buffering || _armed;
            }
        }
    }

    public void ReloadGameRules()
    {
        lock (_opLock) { if (_specific) _detector = new GameDetector(_settings.SpecificWindows); }
    }

    public bool IsBufferHealthy()
    {
        lock (_lock)
        {
            _engine.GetStatus(out bool buffering, out _, out _, out _, out _, out bool hasError,
                out _, out _, out _, out long healthTicks);
            if (!buffering)
            {
                _lastHealthTicks = -1;
                _noProgressPolls = 0;
                return false;
            }
            if (hasError) return false;

            if (_lastHealthTicks < 0)
            {
                _lastHealthTicks = healthTicks;
                return true;
            }

            if (healthTicks != _lastHealthTicks)
            {
                _lastHealthTicks = healthTicks;
                _noProgressPolls = 0;
                return true;
            }

            if (++_noProgressPolls >= 8)
            {
                _lastHealthTicks = -1;
                return false;
            }

            return true;
        }
    }

    public List<string> EnumerateMicrophones()
    {
        return ReplayEngine.EnumerateMicrophones();
    }

    public bool TryGetMicStatus(out long micFrames, out long micEvents, out long micGotFrames,
        out long micFailHr, out int sysPeak, out int micPeak, out string activeName)
    {
        return _engine.TryGetMicStatus(out micFrames, out micEvents, out micGotFrames,
            out micFailHr, out sysPeak, out micPeak, out activeName);
    }

    public void LogDebugDiagnostics()
    {
        if (!Logger.DebugMode) return;
        if (!_engine.TryGetTelemetry(out long captured, out long paced, out long rejected,
                out long encoded, out long dropped, out int encoder, out long lastPts))
            return;

        _engine.TryGetTiming(out double mapMs, out double encodeMs, out double renderMs,
            out double submitMs, out double snapshotMs, out double unmapMs, out long timedFrames);
        _engine.TryGetAudioDiagnostics(0, out long sysActivate, out long sysInit, out long sysStart,
            out int sysTag, out int sysRate, out int sysChannels, out int sysBits,
            out int sysFloat, out int sysConvertFailures, out long sysFail);
        _engine.TryGetAudioDiagnostics(1, out long micActivate, out long micInit, out long micStart,
            out int micTag, out int micRate, out int micChannels, out int micBits,
            out int micFloat, out int micConvertFailures, out long micFail);

        static string Hr(long value) => $"0x{(uint)value:X8}";
        Logger.Debug($"[Capture] captured={captured} paced={paced} rejected={rejected} " +
                     $"encoded={encoded} dropped={dropped} encoder={encoder} lastPts={lastPts} " +
                     $"timing(ms): submit={submitMs:F2} snapshot={snapshotMs:F2} render={renderMs:F2} " +
                     $"map={mapMs:F2} encode={encodeMs:F2} unmap={unmapMs:F2} n={timedFrames}");
        Logger.Debug($"[Audio] sys activate={Hr(sysActivate)} init={Hr(sysInit)} start={Hr(sysStart)} " +
                     $"fmt={sysTag}/{sysRate}Hz/{sysChannels}ch/{sysBits}bit float={sysFloat} " +
                     $"convFail={sysConvertFailures} last={Hr(sysFail)}; " +
                     $"mic activate={Hr(micActivate)} init={Hr(micInit)} start={Hr(micStart)} " +
                     $"fmt={micTag}/{micRate}Hz/{micChannels}ch/{micBits}bit float={micFloat} " +
                     $"convFail={micConvertFailures} last={Hr(micFail)}");
    }

    private static readonly int[] AudioBitrates = [96, 128, 160, 192];

    // Native pref layout (see engine.c):
    //   bits  0..7  encoder ordinal (Auto lets MF pick; 11 requests software)
    //   bit   8     rate control, always VBR now
    //   bit   9     segmented ring
    //   bit  10     GPU encoder enabled
    //   bit  11     GPU encoder prefers iGPU
    //   bits 12..14 quality/speed preset 0..6
    //   bits 15..16 codec (1 HEVC, 2 H264, 3 AV1)
    // No availability bits are sent: MFT_ENUM_FLAG_SORTANDFILTER chooses the MFT
    // attached to the selected adapter, exactly like wcap.
    private static int ResolveEncoderPreference(Settings s)
    {
        int codecBits = s.VideoCodec switch { "Hevc" => 1, "H264" => 2, "Av1" => 3, _ => 2 };
        int segmented = string.Equals(s.BufferMethod, "Segmented", StringComparison.OrdinalIgnoreCase)
            ? (1 << 9) : 0;
        int pref = (1 << 8)
                 | segmented
                 | (VideoQualityProfile.PresetIndex(s.QualityPreset) << 12)
                 | (codecBits << 15);
        if (s.GpuEncoder)
        {
            pref |= 1 << 10;
            if (s.GpuPreference == "Prefer iGPU") pref |= 1 << 11;
        }
        else
        {
            pref |= (int)EncoderPreference.MfSoftware;
        }
        return pref;
    }


    public bool Start(string tempDir, bool wipeSegments = true)
    {
        lock (_opLock)
        {
            lock (_lock)
            {
                if (IsRunning)
                {
                    Logger.Warn("Start() called while already running; ignoring.");
                    return true;
                }
            }

            _captureSettings = JsonSerializer.Deserialize<Settings>(JsonSerializer.Serialize(_settings))!;
            _specific = _captureSettings.CaptureTarget == "SpecificWindows";
            _armed = true;
            if (_specific)
            {
                _detector = new GameDetector(_settings.SpecificWindows);
                _paused = true;
                _targetState = "WAITING FOR A GAME";
                _pollMs = 200;
                _targetTimer = new System.Threading.Timer(_ => PollTarget(), null, 0, _pollMs);
                return true;
            }
            bool ok = StartNative(IntPtr.Zero);
            _armed = ok;
            _targetState = ok ? "RECORDING" : "ERROR";
            return ok;
        }
    }

    private bool StartNative(IntPtr window, WindowSnapshot? snap = null)
    {
        bool forcedDxgi = false;
        bool ok = StartEngine(window, snap, forceDxgi: false);
        if (!ok && snap?.Source == CaptureSource.Wgc)
        {
            _engine.StopBuffering();
            Logger.Warn("WGC window capture did not start; recording the game's display with DXGI instead.");
            forcedDxgi = true;
            ok = StartEngine(window, snap, forceDxgi: true);
        }

        if (!ok)
        {
            Logger.Error("Start() failed: managed engine could not start buffering.");
            return false;
        }
        _activeKey = snap == null ? "" : forcedDxgi ? $"dxgi:{snap.MonitorIndex}" : snap.SourceKey;
        _pendingKey = null;
        lock (_lock)
        {
            _lastHealthTicks = -1;
            _noProgressPolls = 0;
        }

        Logger.Info($"Capture Started - Managed engine, FPS: {_captureSettings.FrameRate}, " +
                    $"Ring: {_captureSettings.ReplayDurationSeconds}s, " +
                    $"System audio: {(_captureSettings.RecordSystemAudio ? "on" : "off")}, " +
                    $"Mic: {(_captureSettings.RecordMicrophone ? (_captureSettings.Microphone.Length > 0 ? _captureSettings.Microphone : "default") : "off")}");
        _captureGeneration++;
        return true;
    }

    private uint CaptureOptions()
    {
        int index = Array.IndexOf(AudioBitrates, _captureSettings.AudioBitrate);
        if (index < 0) index = Array.IndexOf(AudioBitrates, 160);
        // Rounded corners are hidden in the UI but stay enabled for WGC.
        // Include-secondary-windows is hidden and remains off for migration only.
        uint options = (uint)((_captureSettings.CaptureClientArea ? 1 : 0) | (_captureSettings.CaptureCursor ? 2 : 0) |
                              (_captureSettings.CaptureBorder ? 4 : 0) |
                              8 |
                              (_captureSettings.ApplicationLocalAudio ? 64 : 0) |
                              (_specific ? 128 : 0));
        options |= (uint)index << 8;
        if (_captureSettings.AudioCodec == "FLAC") options |= 1u << 10;
        return options;
    }

    private bool StartEngine(IntPtr window, WindowSnapshot? snap, bool forceDxgi)
    {
        var quality = VideoQualityProfile.FromPreset(_captureSettings.QualityPreset, _captureSettings.FrameRate);
        int monitor = snap?.MonitorIndex ?? _captureSettings.CaptureMonitorIndex;
        (int canvasW, int canvasH) = snap?.CanvasSize ?? (0, 0);
        (int srcW, int srcH) = canvasW > 0 && canvasH > 0
            ? (canvasW, canvasH)
            : ReplayEngine.MonitorSize(monitor);
        // Bitrate is always derived from the quality preset, frame rate and source size.
        int bitrateKbps = VideoQualityProfile.TargetBitrateKbps(
            _captureSettings.QualityPreset, _captureSettings.FrameRate, srcW, srcH);
        Logger.Debug($"[Encoder] GPU={(_captureSettings.GpuEncoder ? _captureSettings.GpuPreference : "off")}, " +
                     $"codec={_captureSettings.VideoCodec}, audio={_captureSettings.AudioCodec}/" +
                     $"{_captureSettings.AudioBitrate}kbps, target bitrate {bitrateKbps} kbps " +
                     $"from {_captureSettings.QualityPreset} at {_captureSettings.FrameRate} fps, source {srcW}x{srcH}.");
        // Game Capture is hybrid and follows the detected window mode. Whole Desktop
        // uses the capture method selected in settings (DXGI by default).
        CaptureMethod method;
        if (snap == null)
        {
            method = string.Equals(_captureSettings.CaptureMethod, "Wgc", StringComparison.OrdinalIgnoreCase)
                ? CaptureMethod.Wgc : CaptureMethod.Dxgi;
        }
        else
        {
            method = snap.Source == CaptureSource.Wgc && !forceDxgi
                ? CaptureMethod.Wgc : CaptureMethod.Dxgi;
        }
        return _engine.StartBuffering(
            monitorIndex: monitor,
            ringSeconds: Math.Max(_captureSettings.ReplayDurationSeconds, 1),
            fps: _captureSettings.FrameRate,
            bitrateKbps: bitrateKbps,
            captureAudio: _captureSettings.RecordSystemAudio,
            preference: ResolveEncoderPreference(_captureSettings),
            encoderOptions: 0,
            method: method,
            captureMic: _captureSettings.RecordMicrophone,
            micDevice: _captureSettings.Microphone,
            targetWindow: window,
            captureOptions: CaptureOptions(),
            outputWidthCap: quality.MaxWidth, outputHeightCap: quality.MaxHeight,
            canvasWidth: canvasW, canvasHeight: canvasH);
    }

    private static bool IsProcessAlive(uint processId)
    {
        try
        {
            using var process = System.Diagnostics.Process.GetProcessById((int)processId);
            return !process.HasExited;
        }
        catch { return false; }
    }

    private void DiscardTarget(string reason)
    {
        _engine.Shutdown();
        _target = null;
        _candidate = IntPtr.Zero;
        _pendingKey = null;
        _activeKey = "";
        _lastMode = null;
        _targetLostSince = default;
        _analysisMissSince = default;
        _moduleCheckAt = default;
        _paused = true;
        _retryAt = default;
        _targetState = "WAITING FOR A GAME";
        Logger.Info(reason);
    }

    private void UnloadHooks(string why)
    {
        if (_hookedPid == 0) return;
        uint previousPid = _hookedPid;
        if (!HookInjector.RequestShutdown(previousPid, out string shutdownError))
            Logger.Warn($"Could not gracefully unload hooks from pid {previousPid} ({why}): {shutdownError}");
        _hookedPid = 0;
        _overlayReady = false;
        _moduleCheckAt = default;
    }

    private void FollowWindowMode(WindowSnapshot snap)
    {
        string key = snap.SourceKey;
        if (key == _activeKey)
        {
            _pendingKey = null;
            return;
        }
        DateTime now = DateTime.UtcNow;
        if (_pendingKey != key)
        {
            _pendingKey = key;
            _pendingKeySince = now;
            return;
        }
        if (now - _pendingKeySince < TimeSpan.FromMilliseconds(350) || now < _switchRetryAt) return;

        uint options = CaptureOptions();
        var (canvasW, canvasH) = snap.CanvasSize;
        bool toWgc = snap.Source == CaptureSource.Wgc;
        bool ok = _engine.SwitchSource(toWgc ? CaptureMethod.Wgc : CaptureMethod.Dxgi,
            snap.MonitorIndex, snap.Handle, options, canvasW, canvasH);
        if (ok)
        {
            _activeKey = key;
            _pendingKey = null;
            Logger.Info($"Capture source follows window mode: {snap}.");
            return;
        }
        if (toWgc && _engine.SwitchSource(CaptureMethod.Dxgi, snap.MonitorIndex, snap.Handle,
                options, canvasW, canvasH))
        {
            _activeKey = $"dxgi:{snap.MonitorIndex}";
            _switchRetryAt = now.AddSeconds(5);
            Logger.Warn("WGC could not capture this window; recording its display with DXGI instead.");
            return;
        }
        _switchRetryAt = now.AddSeconds(2);
    }

    private void UpdateOverlay(CaptureWindow game, WindowSnapshot snap, bool protectedGame, string protectedName)
    {
        bool exclusive = snap.Mode == WindowMode.ExclusiveFullscreen;
        if (exclusive && protectedGame && _policyPid != game.ProcessId)
        {
            _policyPid = game.ProcessId;
            Logger.Info($"Exclusive fullscreen in {game.Executable}: {protectedName} protects against injection. " +
                        "No in-game overlay; the clip notification uses the normal popup.");
        }

        bool wantOverlay = exclusive && !protectedGame;
        if (_hookedPid == game.ProcessId && DateTime.UtcNow >= _moduleCheckAt)
        {
            _moduleCheckAt = DateTime.UtcNow.AddSeconds(8);
            if (!HookInjector.HasModule(game.ProcessId, HookInjector.OverlayDll))
            {
                _hookedPid = 0;
                _overlayReady = false;
            }
        }

        if (wantOverlay && _hookedPid != game.ProcessId && DateTime.UtcNow >= _hookRetryAt)
        {
            UnloadHooks("target changed");
            if (!HookInjector.UsesSupportedGraphicsApi(game.ProcessId, out string graphicsApi))
            {
                Logger.Info($"{game.Executable} has no hookable graphics API ({graphicsApi}); using the normal popup notification.");
                _hookRetryAt = DateTime.UtcNow.AddSeconds(15);
            }
            else
            {
                OverlayBridge.Prepare();
                if (HookInjector.EnsureInjected(game.ProcessId, snap.ThreadId, out bool overlayReady, out string hookError))
                {
                    _hookedPid = game.ProcessId;
                    _overlayReady = overlayReady;
                    _moduleCheckAt = DateTime.UtcNow.AddSeconds(8);
                    Logger.Info($"In-game {graphicsApi} overlay injected into {game.Executable} (pid {game.ProcessId}, overlay={(overlayReady ? "on" : "off")}).");
                }
                else
                {
                    _hookRetryAt = DateTime.UtcNow.AddSeconds(10);
                    Logger.Warn($"In-game overlay unavailable for {game.Executable}: {hookError}. Using the normal popup notification.");
                }
            }
        }

        OverlayBridge.SetOverlayOnly(wantOverlay && _hookedPid == game.ProcessId && _overlayReady,
            _captureSettings.FrameRate);
    }

    private void PollTarget()
    {
        if (!Monitor.TryEnter(_opLock)) return;
        try
        {
            if (!_armed || !_specific || _detector == null) return;

            if (_target != null && !GameDetector.IsAlive(_target))
            {
                if (_targetLostSince == default) _targetLostSince = DateTime.UtcNow;
                if (IsProcessAlive(_target.ProcessId) &&
                    DateTime.UtcNow - _targetLostSince < TimeSpan.FromSeconds(6))
                {
                    _targetState = "SWITCHING DISPLAY MODE";
                }
                else
                {
                    DiscardTarget("Captured window closed; replay discarded. Waiting for an eligible foreground game.");
                }
            }
            else _targetLostSince = default;

            var foreground = _detector.Foreground();
            if (foreground == null)
            {
                if (_target != null && GameDetector.IsForegroundProcess(_target.ProcessId))
                {
                    _targetState = "SWITCHING DISPLAY MODE";
                    return;
                }
                _candidate = IntPtr.Zero;
                if (!_paused) _engine.SetPaused(true);
                _paused = true;
                _targetState = _target == null ? "WAITING FOR A GAME" : "PAUSED";
                return;
            }

            bool protectedGame = AntiCheatGames.IsProtected(foreground.Executable, out string antiCheatName);
            if (!protectedGame && ProbeAntiCheatModule(foreground.ProcessId, out string antiCheatModule))
            {
                protectedGame = true;
                antiCheatName = $"loaded anti-cheat module {antiCheatModule}";
            }

            WindowSnapshot? snap = WindowDetectionService.Analyze(foreground.Handle);
            if (snap == null)
            {
                _analysisMissSince = _analysisMissSince == default ? DateTime.UtcNow : _analysisMissSince;
                bool transient = DateTime.UtcNow - _analysisMissSince < TimeSpan.FromSeconds(2);
                if (!_paused) _engine.SetPaused(true);
                _paused = true;
                _targetState = transient ? "SWITCHING DISPLAY MODE" : "WINDOW NOT AVAILABLE";
                return;
            }
            _analysisMissSince = default;
            if (_lastMode != snap.Mode || _lastMonitor != snap.MonitorIndex)
            {
                Logger.Info($"Window detection for {foreground.Executable}: {snap}" +
                            (snap.MultiDisplay ? $"; {snap.MonitorCount} displays attached." : "."));
                _lastMode = snap.Mode;
                _lastMonitor = snap.MonitorIndex;
            }

            bool changedProcess = _target?.ProcessId != foreground.ProcessId;
            if (changedProcess)
            {
                if (!_paused) _engine.SetPaused(true);
                _paused = true;
                if (_candidate != foreground.Handle)
                {
                    _candidate = foreground.Handle;
                    _candidateSince = DateTime.UtcNow;
                    return;
                }
                if (DateTime.UtcNow - _candidateSince < TimeSpan.FromMilliseconds(250) || DateTime.UtcNow < _retryAt) return;
                var verified = _detector.Foreground();
                if (verified?.Handle != foreground.Handle || verified.ProcessId != foreground.ProcessId) return;

                _engine.StopBuffering();
                UnloadHooks("new game");
                Logger.Info($"Capture restart committed: new game {foreground.Game} ({foreground.Executable}); previous buffer discarded.");
                _target = foreground;
                if (!StartNative(foreground.Handle, snap))
                {
                    _target = null;
                    _targetState = "CAPTURE UNAVAILABLE";
                    _retryAt = DateTime.UtcNow.AddSeconds(5);
                    return;
                }
                _startedAt = DateTime.UtcNow;
                _candidate = IntPtr.Zero;
            }
            else
            {
                _target = foreground;
                FollowWindowMode(snap);
            }

            UpdateOverlay(foreground, snap, protectedGame, antiCheatName);

            _engine.GetStatus(out bool running, out _, out _, out long frames, out _, out bool error, out _, out _, out _, out _);
            if (!running || error || (frames == 0 && DateTime.UtcNow - _startedAt > TimeSpan.FromSeconds(8)))
            {
                string diag = _engine.DescribeFailure();
                _engine.StopBuffering();
                UnloadHooks("capture stopped");
                _target = null;
                _activeKey = "";
                _paused = true;
                _targetState = "CAPTURE UNAVAILABLE";
                _retryAt = DateTime.UtcNow.AddSeconds(5);
                Logger.Warn($"Capture unavailable ({diag}); waiting before retrying. No system-audio fallback.");
                return;
            }
            if (_paused) _engine.SetPaused(false);
            _paused = false;
            _targetState = "RECORDING";
        }
        catch (Exception ex) { _engine.SetPaused(true); _paused = true; _targetState = "CAPTURE UNAVAILABLE"; Logger.Error("Target detection failed", ex); }
        finally
        {
            AdjustPollInterval();
            Monitor.Exit(_opLock);
        }
    }

    private void AdjustPollInterval()
    {
        int desired = _target != null || _candidate != IntPtr.Zero ? 250 : 500;
        if (desired == _pollMs) return;
        _pollMs = desired;
        _targetTimer?.Change(desired, desired);
    }

    private bool ProbeAntiCheatModule(uint processId, out string module)
    {
        DateTime now = DateTime.UtcNow;
        if (processId != _antiCheatPid)
        {
            _antiCheatPid = processId;
            _antiCheatModule = "";
            _antiCheatChecks = 0;
            _antiCheatNextCheck = default;
        }

        if (_antiCheatModule.Length == 0 && now >= _antiCheatNextCheck)
        {
            if (HookInjector.HasAntiCheatModule(processId, out string found)) _antiCheatModule = found;
            _antiCheatChecks++;
            _antiCheatNextCheck = now.AddSeconds(_antiCheatChecks switch { 1 => 5, 2 => 10, _ => 60 });
        }

        module = _antiCheatModule;
        return module.Length > 0;
    }

    public void Stop()
    {
        lock (_opLock)
        {
            _armed = false;
            _targetTimer?.Dispose();
            _targetTimer = null;
            _target = null;
            _candidate = IntPtr.Zero;
            _paused = false;
            _retryAt = default;
            _analysisMissSince = default;
            _moduleCheckAt = default;
            _targetState = "STOPPED";
            _engine.StopBuffering();
            lock (_lock)
            {
                _lastHealthTicks = -1;
                _noProgressPolls = 0;
            }
            Logger.Info("Recording stopped.");
        }
    }

    public bool Restart(string tempDir, bool wipeSegments = false)
    {
        lock (_opLock)
        {
            Stop();
            return Start(tempDir, wipeSegments);
        }
    }

    private sealed class PendingSaveJob
    {
        public ulong JobId { get; init; }
        public string DestDir { get; init; } = "";
        public string SaveFolder { get; init; } = "";
        public string VideoPath { get; init; } = "";
    }

    public SaveReplayResult SaveReplay(string tempDir, string destDir) =>
        Task.Run(() => SaveReplayAsync(tempDir, destDir)).GetAwaiter().GetResult();

    public async Task<SaveReplayResult> SaveReplayAsync(string tempDir, string destDir)
    {
        if (!TryBeginSave(destDir, out PendingSaveJob? job, out SaveReplayResult failure))
            return failure;

        try
        {
            int result = ReplayEngine.SavePending;
            bool alive = true;
            while (alive && result == ReplayEngine.SavePending)
            {
                await Task.Delay(100).ConfigureAwait(false);
                alive = _engine.TryPollSave(job!.JobId, out _, out result);
            }
            return FinishSave(job!, result);
        }
        catch (Exception ex)
        {
            Logger.Error("SaveReplay: async save failed", ex);
            CleanupFailedSaveFolder(job!.SaveFolder);
            Interlocked.Exchange(ref _activeSaveJobId, 0L);
            Interlocked.Exchange(ref _saveInFlight, 0);
            return ProbeWriteBlocked(job.DestDir, ex);
        }
    }

    public int SaveProgress
    {
        get
        {
            long id = Interlocked.Read(ref _activeSaveJobId);
            if (id != 0 && _engine.TryPollSave((ulong)id, out int progress, out _))
                return progress;
            return _engine.SaveProgress;
        }
    }

    private bool TryBeginSave(string destDir, out PendingSaveJob? job, out SaveReplayResult failure)
    {
        job = null;
        failure = default;
        lock (_lock)
        {
            if (!IsRunning)
            {
                Logger.Warn("SaveReplay requested but recorder is not running.");
                failure = new SaveReplayResult(SaveReplayOutcome.NotRecording);
                return false;
            }
        }
        if (Interlocked.CompareExchange(ref _saveInFlight, 1, 0) != 0)
        {
            Logger.Warn("SaveReplay requested while another save is still in progress.");
            failure = new SaveReplayResult(SaveReplayOutcome.MergeFailed);
            return false;
        }

        string saveFolder = Path.Combine(destDir, $"replay_{DateTime.Now:yyyyMMdd_HHmmss}");
        try
        {
            Directory.CreateDirectory(destDir);
            Directory.CreateDirectory(saveFolder);
            string destVideo = Path.Combine(saveFolder, "replay." + Settings.ContainerExtension(_settings.Container));
            int rc = _engine.BeginSaveReplay(destVideo, Math.Max(_settings.ReplayDurationSeconds, 1), out ulong jobId);
            if (rc != ReplayEngine.SaveOk)
            {
                Interlocked.Exchange(ref _saveInFlight, 0);
                CleanupFailedSaveFolder(saveFolder);
                failure = ClassifySaveStartFailure(rc, destDir);
                return false;
            }

            job = new PendingSaveJob
            {
                JobId = jobId,
                DestDir = destDir,
                SaveFolder = saveFolder,
                VideoPath = destVideo,
            };
            Interlocked.Exchange(ref _activeSaveJobId, (long)jobId);
            return true;
        }
        catch (Exception ex)
        {
            Logger.Error("SaveReplay: failed creating save folder", ex);
            Interlocked.Exchange(ref _saveInFlight, 0);
            CleanupFailedSaveFolder(saveFolder);
            failure = ProbeWriteBlocked(destDir, ex);
            return false;
        }
    }

    private static SaveReplayResult ClassifySaveStartFailure(int rc, string destDir)
    {
        Logger.Info($"SaveReplay: managed engine returned {rc}.");
        if (rc == ReplayEngine.SaveLowMemory)
        {
            Logger.Warn("Replay save refused because Windows has insufficient committed-memory headroom. No replay data was duplicated or written.");
            return new SaveReplayResult(SaveReplayOutcome.LowMemory);
        }
        if (rc == ReplayEngine.SaveFailed || rc == ReplayEngine.SaveNotEnoughData)
            return new SaveReplayResult(SaveReplayOutcome.NoSegments);
        return ProbeWriteBlocked(destDir, null);
    }

    private SaveReplayResult FinishSave(PendingSaveJob job, int rc)
    {
        Interlocked.Exchange(ref _activeSaveJobId, 0L);
        Interlocked.Exchange(ref _saveInFlight, 0);

        if (rc == ReplayEngine.SaveOk && File.Exists(job.VideoPath))
        {
            Logger.Info($"Replay Saved - Path: {job.VideoPath}");
            return new SaveReplayResult(SaveReplayOutcome.Success, job.VideoPath);
        }

        Logger.Info($"SaveReplay: job {job.JobId} finished with {rc}.");
        CleanupFailedSaveFolder(job.SaveFolder);
        if (rc == ReplayEngine.SaveLowMemory)
        {
            Logger.Warn("Replay save refused because Windows has insufficient committed-memory headroom. No replay data was duplicated or written.");
            return new SaveReplayResult(SaveReplayOutcome.LowMemory);
        }
        if (rc == ReplayEngine.SaveFailed || rc == ReplayEngine.SaveNotEnoughData)
            return new SaveReplayResult(SaveReplayOutcome.NoSegments);
        return ProbeWriteBlocked(job.DestDir, null);
    }

    private static void CleanupFailedSaveFolder(string saveFolder)
    {
        try
        {
            if (Directory.Exists(saveFolder))
                Directory.Delete(saveFolder, recursive: true);
        }
        catch (Exception ex)
        {
            Logger.Warn($"SaveReplay: could not clean up failed save folder '{saveFolder}': {ex.Message}");
        }
    }

    private static SaveReplayResult ProbeWriteBlocked(string dir, Exception? originalError)
    {
        try
        {
            string probePath = Path.Combine(dir, ".ir_write_test");
            File.WriteAllText(probePath, "instant replay write test");
            File.Delete(probePath);
        }
        catch (UnauthorizedAccessException)
        {
            Logger.Warn($"SaveReplay: write to '{dir}' blocked (UnauthorizedAccessException) — " +
                        "likely Controlled Folder Access or antivirus.");
            return new SaveReplayResult(SaveReplayOutcome.WriteBlocked);
        }
        catch
        {
        }

        if (originalError != null)
            Logger.Error("SaveReplay: failed, and write-probe found no CFA/AV signature", originalError);

        return new SaveReplayResult(SaveReplayOutcome.MergeFailed);
    }

    public void Dispose()
    {
        Stop();
        if (_hookedPid != 0)
        {
            uint pid = _hookedPid;
            if (!HookInjector.RequestShutdown(pid, out string error))
                Logger.Warn($"Could not gracefully unload hooks from pid {pid} during app shutdown: {error}");
            _hookedPid = 0;
            _overlayReady = false;
        }
        try { _engine.Shutdown(); }
        catch (Exception ex) { Logger.Warn($"Engine shutdown failed: {ex.Message}"); }
    }
}