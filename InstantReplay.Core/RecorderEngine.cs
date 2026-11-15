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
    private CaptureSurface? _pendingSurface;
    private DateTime _surfaceSince;
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

    private long _lastHealthTicks = -1;
    private int _noProgressPolls;
    private uint _hookedPid;
    private bool _overlayReady;
    private DateTime _hookRetryAt;
    private uint _policyPid;
    private string _policyReason = "";
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

    private static EncoderPreference ResolveEncoder(string choice) => choice switch
    {
        "Nvenc" => EncoderPreference.Nvenc,
        "Amf" => EncoderPreference.Amf,
        "Qsv" => EncoderPreference.Qsv,
        "X265" or "X264" or "Software" => EncoderPreference.X264,
        "MediaFoundation" => EncoderPreference.MediaFoundation,
        _ => EncoderPreference.Auto,
    };

    private static int ResolveEncoderOptions(Settings s) =>
        (s.RateControl == "Vbr" ? 1 << 8 : 0)
        | ((s.EncoderPreset switch { "Speed" => 0, "Quality" => 2, _ => 1 }) << 10)
        | ((s.VideoCodec switch { "Hevc" => 1, "H264" => 2, _ => 0 }) << 12);


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

    private uint CaptureOptions() =>
        (uint)((_captureSettings.CaptureClientArea ? 1 : 0) | (_captureSettings.CaptureCursor ? 2 : 0) |
               (_captureSettings.CaptureBorder ? 4 : 0) |
               (!_specific && _captureSettings.CaptureRoundedCorners ? 8 : 0) |
               (_captureSettings.CaptureSecondaryWindows ? 16 : 0) | (_specific ? 64 | 128 : 0));

    private bool StartEngine(IntPtr window, WindowSnapshot? snap, bool forceDxgi)
    {
        var quality = VideoQualityProfile.FromPreset(_captureSettings.QualityPreset, _captureSettings.FrameRate);
        int bitrateKbps = _captureSettings.RateControl != "Vbr" && _captureSettings.BitrateKbps > 0
            ? _captureSettings.BitrateKbps
            : quality.BitrateKbps;
        CaptureMethod method = snap != null && snap.Source == CaptureSource.Wgc && !forceDxgi
            ? CaptureMethod.Wgc
            : CaptureMethod.Dxgi;
        int monitor = snap?.MonitorIndex ?? _captureSettings.CaptureMonitorIndex;
        (int canvasW, int canvasH) = snap?.CanvasSize ?? (0, 0);
        return _engine.StartBuffering(
            monitorIndex: monitor,
            ringSeconds: Math.Max(_captureSettings.ReplayDurationSeconds, 1),
            fps: _captureSettings.FrameRate,
            bitrateKbps: bitrateKbps,
            captureAudio: _captureSettings.RecordSystemAudio,
            preference: ResolveEncoder(_captureSettings.EncoderChoice),
            encoderOptions: ResolveEncoderOptions(_captureSettings),
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
        _pendingSurface = null;
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
        bool toWgc = snap.Source == CaptureSource.Wgc;
        bool ok = _engine.SwitchSource(toWgc ? CaptureMethod.Wgc : CaptureMethod.Dxgi,
            snap.MonitorIndex, snap.Handle, options);
        if (ok)
        {
            _activeKey = key;
            _pendingKey = null;
            Logger.Info($"Capture source follows window mode: {snap}.");
            return;
        }
        if (toWgc && _engine.SwitchSource(CaptureMethod.Dxgi, snap.MonitorIndex, snap.Handle, options))
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
            _policyReason = "protected";
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
                _pendingSurface = null;
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
            _pendingSurface = null;
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

    public SaveReplayResult SaveReplay(string tempDir, string destDir)
    {
        lock (_opLock)
        {
            return SaveReplayCore(tempDir, destDir);
        }
    }

    public int SaveProgress => _engine.SaveProgress;

    private SaveReplayResult SaveReplayCore(string tempDir, string destDir)
    {
        lock (_lock)
        {
            if (!IsRunning)
            {
                Logger.Warn("SaveReplay requested but recorder is not running.");
                return new SaveReplayResult(SaveReplayOutcome.NotRecording);
            }
        }

        string saveFolder = Path.Combine(destDir, $"replay_{DateTime.Now:yyyyMMdd_HHmmss}");
        try
        {
            Directory.CreateDirectory(saveFolder);
        }
        catch (Exception ex)
        {
            Logger.Error("SaveReplay: failed creating save folder", ex);
            return ProbeWriteBlocked(destDir, ex);
        }

        string destVideo = Path.Combine(saveFolder, "replay." + Settings.ContainerExtension(_settings.Container));

        int rc;
        try
        {
            rc = _engine.SaveReplay(destVideo, Math.Max(_settings.ReplayDurationSeconds, 1));
        }
        catch (Exception ex)
        {
            Logger.Error("SaveReplay: exception in managed engine", ex);
            CleanupFailedSaveFolder(saveFolder);
            return ProbeWriteBlocked(destDir, ex);
        }

        if (rc == 0 && File.Exists(destVideo))
        {
            Logger.Info($"Replay Saved - Path: {destVideo}");
            return new SaveReplayResult(SaveReplayOutcome.Success, destVideo);
        }

        Logger.Info($"SaveReplay: managed engine returned {rc}.");

        if (rc == -31)
        {
            CleanupFailedSaveFolder(saveFolder);
            Logger.Warn("Replay save refused because Windows has insufficient committed-memory headroom. No replay data was duplicated or written.");
            return new SaveReplayResult(SaveReplayOutcome.LowMemory);
        }

        if (rc == -1 || rc == -30)
        {
            CleanupFailedSaveFolder(saveFolder);
            return new SaveReplayResult(SaveReplayOutcome.NoSegments);
        }

        CleanupFailedSaveFolder(saveFolder);
        return ProbeWriteBlocked(destDir, null);
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