using System.Reflection;
using System.Runtime.InteropServices;
using System.Text.Json;
using System.Text;

namespace InstantReplay;

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
                _targetTimer = new System.Threading.Timer(_ => PollTarget(), null, 0, 200);
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
        int bitrateKbps = _captureSettings.BitrateKbps > 0
            ? _captureSettings.BitrateKbps
            : quality.BitrateKbps;
        // Windowed and borderless games use WGC on the window. Exclusive fullscreen
        // (and whole-display recording) uses DXGI desktop duplication on the display.
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

    /// <summary>
    /// Follows the game's window mode in real time. The encoder, ring and audio keep
    /// running; only the video source changes, so the output stays one seamless video.
    /// </summary>
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
        // Debounce: alt-tab and mode changes briefly look like other modes.
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

    /// <summary>
    /// The in-game card only exists in exclusive fullscreen, where normal windows cannot
    /// draw over the game. Anti-cheat protected games are never injected: there the
    /// clip notification silently stays the regular popup.
    /// </summary>
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
        // Enumerating the game's modules is relatively expensive; checking twice a
        // second is enough to notice the hook being unloaded, and it stops the
        // 200 ms poll from taking a module snapshot of the game every time.
        if (_hookedPid == game.ProcessId && DateTime.UtcNow >= _moduleCheckAt)
        {
            _moduleCheckAt = DateTime.UtcNow.AddSeconds(2);
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
                    _moduleCheckAt = DateTime.UtcNow.AddSeconds(2);
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

            // A game can destroy and recreate its window when it changes display mode.
            // Keep the replay while the process lives; only a gone process discards it.
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
            if (!protectedGame && HookInjector.HasAntiCheatModule(foreground.ProcessId, out string antiCheatModule))
            {
                protectedGame = true;
                antiCheatName = $"loaded anti-cheat module {antiCheatModule}";
            }

            WindowSnapshot? snap = WindowDetectionService.Analyze(foreground.Handle);
            if (snap == null)
            {
                // Minimized/cloaked/transient window recreation. Pause instead of
                // leaving the status on "SWITCHING DISPLAY MODE" forever, but keep
                // the short message while the mode change is likely still settling.
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
        finally { Monitor.Exit(_opLock); }
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

public static class StartupManager
{
    private const string RunKeyPath = @"Software\Microsoft\Windows\CurrentVersion\Run";

    private const string ValueName = "Instant Replay";

    private const string LegacyValueName = "InstantReplay";

    public static bool IsEnabled()
    {
        CleanupLegacyKey();
        try
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.OpenSubKey(RunKeyPath, writable: false);
            return key?.GetValue(ValueName) is string existing &&
                   existing.Equals(BuildCommand(), StringComparison.OrdinalIgnoreCase);
        }
        catch (Exception ex)
        {
            Logger.Warn($"StartupManager.IsEnabled check failed: {ex.Message}");
            return false;
        }
    }

    public static void SetEnabled(bool enabled)
    {
        CleanupLegacyKey();
        try
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.OpenSubKey(RunKeyPath, writable: true)
                             ?? Microsoft.Win32.Registry.CurrentUser.CreateSubKey(RunKeyPath);
            if (enabled)
                key.SetValue(ValueName, BuildCommand());
            else
                key.DeleteValue(ValueName, throwOnMissingValue: false);
            Logger.Info($"Start with Windows {(enabled ? "enabled" : "disabled")}.");
        }
        catch (Exception ex)
        {
            Logger.Error("StartupManager.SetEnabled failed", ex);
        }
    }

    private static void CleanupLegacyKey()
    {
        try
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.OpenSubKey(RunKeyPath, writable: true);
            if (key?.GetValue(LegacyValueName) != null)
            {
                key.DeleteValue(LegacyValueName, throwOnMissingValue: false);
                Logger.Info($"Removed legacy duplicate startup registry value \"{LegacyValueName}\".");
            }
        }
        catch (Exception ex)
        {
            Logger.Warn($"StartupManager.CleanupLegacyKey failed: {ex.Message}");
        }
    }

    private static string BuildCommand()
    {
        string exe = Environment.ProcessPath ?? "InstantReplay.exe";
        return $"\"{exe}\" --tray";
    }
}

internal static class IrNative
{
    internal const int OK = 0;
    internal const int Fail = -1;
    internal const int NotEnoughData = -30;

    private const string Dll = "engine";

    static IrNative()
    {
        try
        {
            NativeLibrary.SetDllImportResolver(typeof(IrNative).Assembly, ResolveEngine);
        }
        catch (InvalidOperationException)
        {
        }
    }

    private static IntPtr ResolveEngine(string name, Assembly assembly, DllImportSearchPath? path)
    {
        if (!string.Equals(name, Dll, StringComparison.OrdinalIgnoreCase)) return IntPtr.Zero;

        string baseDir = AppContext.BaseDirectory;
        foreach (string candidate in new[]
        {
            Path.Combine(baseDir, "engine", "bin", "engine.dll"),
            Path.Combine(baseDir, "engine.dll"),
        })
        {
            if (File.Exists(candidate)) return NativeLibrary.Load(candidate);
        }
        return IntPtr.Zero;
    }

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_Create(out IntPtr engine);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void IR_Destroy(IntPtr engine);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_Start(IntPtr engine, int monitorIndex, int ringSeconds,
        int fps, int bitrateKbps, int capW, int capH, int captureAudio, int captureMic,
        [MarshalAs(UnmanagedType.LPWStr)] string micDevice,
        int encoderPreference, int captureMethod, out long startQpc100ns);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_LastErrorMessage([Out] byte[] buffer, int capacity);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void IR_Stop(IntPtr engine);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_SetCaptureTarget(IntPtr engine, IntPtr window, uint options);
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_SetCaptureSource(IntPtr engine, int method, int monitorIndex,
        IntPtr window, uint options);
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_SetCanvasHint(IntPtr engine, int width, int height);
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_SetPaused(IntPtr engine, int paused);
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_GetWindowCount();
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void IR_GetWindowInfo(int index, out IntPtr window,
        [MarshalAs(UnmanagedType.LPWStr)] StringBuilder title, int titleCapacity, out int width, out int height);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_Save(IntPtr engine,
        [MarshalAs(UnmanagedType.LPWStr)] string outPath, int secondsBack);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_SaveProgress();

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void IR_GetStatus(IntPtr engine, out int isBuffering,
        out int ringSeconds, out long framesEncoded, out long framesDropped,
        out int hasError, out long audioFrames, out long audioEvents,
        out long audioGotFrames, out long healthHeartbeat);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_GetMonitorCount();

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void IR_GetMonitorInfo(int index,
        [MarshalAs(UnmanagedType.LPWStr)] StringBuilder name, int nameCap,
        out int width, out int height);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_IsDxgiSupported();

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_DxgiDuplicationSupported();

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_ActiveCaptureMethod(IntPtr engine);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_LastHresult(IntPtr engine);
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_LastFailStage(IntPtr engine);
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_LastFailHr(IntPtr engine);
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_LastCaptureHr(IntPtr engine);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_ActiveEncoder(IntPtr engine);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_HwInitHr(IntPtr engine);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_GetMicDeviceCount();

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void IR_GetMicDeviceName(int index,
        [MarshalAs(UnmanagedType.LPWStr)] StringBuilder name, int nameCap);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void IR_MicStatus(IntPtr engine, out long micFrames, out long micEvents,
        out long micGotFrames, out long micFailHr, out int sysPeak, out int micPeak,
        [MarshalAs(UnmanagedType.LPWStr)] StringBuilder activeName, int nameCap);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr IR_Version();

    internal static string Version()
    {
        try { return Marshal.PtrToStringAnsi(IR_Version()) ?? ""; }
        catch { return ""; }
    }
}

public enum EncoderPreference
{
    Auto = 0,
    Nvenc = 1,
    Amf = 2,
    Qsv = 3,
    X264 = 4,
    MediaFoundation = 5,
}

public enum CaptureMethod
{
    Wgc = 0,
    Dxgi = 1,
    External = 3,
}

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
        bool captureAudio, EncoderPreference preference, CaptureMethod method = CaptureMethod.Dxgi,
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
            int rate = Math.Clamp(fps, 15, 240);
            int bps = Math.Max(bitrateKbps, 1);
            int capW = Math.Max(0, outputWidthCap), capH = Math.Max(0, outputHeightCap);

            // The canvas is the highest resolution the game can show on its display.
            // Lower resolutions are letterboxed on it, so mode switches never resize the video.
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
                (int)preference | encoderOptions, (int)method, out origin);
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
                20 => "FFmpeg hevc_mf (Media Foundation)",
                19 => "FFmpeg libx265 (software)",
                18 => "FFmpeg hevc_qsv (Intel Quick Sync)",
                17 => "FFmpeg hevc_amf (AMD)",
                16 => "FFmpeg hevc_nvenc (NVIDIA)",
                30 => "FFmpeg h264_mf (Media Foundation fallback)",
                29 => "FFmpeg libx264 (CPU final fallback)",
                28 => "FFmpeg h264_qsv (Intel Quick Sync fallback)",
                27 => "FFmpeg h264_amf (AMD fallback)",
                26 => "FFmpeg h264_nvenc (NVIDIA fallback)",
                8 => "hardware (NVENC, GPU shaders, zero-copy)",
                4 => "hardware (Media Foundation, GPU shaders, zero-copy)",
                2 => "hardware (Media Foundation, CPU upload)",
                1 => "software (Microsoft H.264 MFT)",
                _ => "unknown",
            };
            Logger.Info($"[Engine] Encoder: {encName} (requested {preference}){(hwHr != 0 && active != 8 ? $", NVENC/GPU hardware path unavailable: code 0x{hwHr:X8}" : "")}");

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

    /// <summary>
    /// Re-targets video capture without touching the encoder, ring or audio, so a
    /// WGC &lt;-&gt; DXGI switch stays inside one continuous video.
    /// </summary>
    public bool SwitchSource(CaptureMethod method, int monitorIndex, IntPtr window, uint options)
    {
        lock (_opLock)
        {
            if (_engine == IntPtr.Zero) return false;
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

    /// <summary>
    /// Human-readable snapshot of the native failure state, used when the capture
    /// loop has to be torn down so the log tells us *why* instead of just looping.
    /// </summary>
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
                0 => "WGC", 1 => "DXGI", 3 => "graphics hook", _ => $"method {method}"
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

