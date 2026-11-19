using System.Diagnostics;
using System.IO.MemoryMappedFiles;

namespace InstantReplay.Services;

internal static class OverlayBridge
{
    private const string MapName = @"Local\InstantReplay.Control.v1";
    private const int MapSize = 2048;
    private const uint Magic = 0x49524652;
    private const uint Version = 2;

    private const int OffMagic = 0;
    private const int OffVersion = 4;
    private const int OffCaptureActive = 8;
    private const int OffCaptureFps = 12;
    private const int OffVisible = 16;
    private const int OffState = 20;
    private const int OffProgress = 24;
    private const int OffSeconds = 28;
    private const int OffSeq = 32;
    private const int OffDebugLog = 36;
    private const int OffTitle = 40;
    private const int OffDetail = 168;
    private const int OffIcon = 424;
    private const int OffLogPath = 944;

    private static readonly object Gate = new();
    private static MemoryMappedFile? _file;
    private static MemoryMappedViewAccessor? _view;
    private static Timer? _lifetime;
    private static uint _seq;
    private static bool _overlayOnly;
    private static int _overlayFps;

    public static bool InGame { get; private set; }

    public static void Prepare()
    {
        lock (Gate)
        {
            if (!EnsureOpenLocked()) return;
            RefreshDiagnosticsLocked();
        }
    }

    public static void Refresh()
    {
        lock (Gate)
        {
            if (_view != null) RefreshDiagnosticsLocked();
        }
    }

    public static void SetCapture(bool active, int fps, bool overlayAvailable = true)
    {
        lock (Gate)
        {
            if (!EnsureOpenLocked()) return;
            InGame = active && overlayAvailable;
            _view!.Write(OffCaptureActive, active ? 1u : 0u);
            _view.Write(OffCaptureFps, (uint)Math.Clamp(fps, 15, 240));
            RefreshDiagnosticsLocked();
            if (!active) ClearLocked();
            else
            {
                _view.Write(OffVisible, 0u);
                _view.Write(OffState, 0u);
                _view.Write(OffProgress, -1);
                _view.Write(OffSeconds, 0);
                _lifetime?.Dispose();
                _lifetime = null;
                BumpLocked();
            }
        }
    }

    public static void SetOverlayOnly(bool available, int fps)
    {
        lock (Gate)
        {
            int clamped = Math.Clamp(fps, 15, 240);
            if (available == _overlayOnly && clamped == _overlayFps && InGame == available) return;
            if (!EnsureOpenLocked()) return;
            _overlayOnly = available;
            _overlayFps = clamped;
            InGame = available;
            _view!.Write(OffCaptureActive, 0u);
            _view.Write(OffCaptureFps, (uint)clamped);
            RefreshDiagnosticsLocked();
            if (!available) ClearLocked();
        }
    }

    public static void BeginEncoding()
    {
        lock (Gate)
        {
            if (!EnsureOpenLocked()) return;
            _lifetime?.Dispose();
            _lifetime = null;
            _view!.Write(OffVisible, 1u);
            _view.Write(OffState, 1u);
            _view.Write(OffProgress, 0);
            _view.Write(OffSeconds, 0);
            WriteStringLocked(OffTitle, 64, "Clipping…");
            WriteStringLocked(OffDetail, 128, "Preparing clip…");
            BumpLocked();
        }
    }

    public static void ReportProgress(int percent)
    {
        lock (Gate)
        {
            if (_view == null || !InGame) return;
            int value = Math.Clamp(percent, 0, 100);
            _view.Write(OffProgress, value);
            WriteStringLocked(OffDetail, 128, value <= 0 ? "Preparing clip…" : $"Encoding {value}%");
            BumpLocked();
        }
    }

    public static void Complete(int seconds)
    {
        lock (Gate)
        {
            if (!EnsureOpenLocked()) return;
            int value = Math.Max(1, seconds);
            _view!.Write(OffVisible, 1u);
            _view.Write(OffState, 2u);
            _view.Write(OffProgress, 100);
            _view.Write(OffSeconds, value);
            WriteStringLocked(OffTitle, 64, $"Clipped Last {value} {(value == 1 ? "Second" : "Seconds")}");
            WriteStringLocked(OffDetail, 128, "Saved to your gallery");
            BumpLocked();
            StartLifetimeLocked();
        }
    }

    public static void Fail()
    {
        lock (Gate)
        {
            if (!EnsureOpenLocked()) return;
            _view!.Write(OffVisible, 1u);
            _view.Write(OffState, 3u);
            _view.Write(OffProgress, 100);
            _view.Write(OffSeconds, 0);
            WriteStringLocked(OffTitle, 64, "Failed To Clip");
            WriteStringLocked(OffDetail, 128, "See debug log for more details");
            BumpLocked();
            StartLifetimeLocked();
        }
    }

    public static void Hide()
    {
        lock (Gate)
        {
            _lifetime?.Dispose();
            _lifetime = null;
            if (_view == null) return;
            _view.Write(OffVisible, 0u);
            BumpLocked();
        }
    }

    public static void Reset()
    {
        lock (Gate)
        {
            _lifetime?.Dispose();
            _lifetime = null;
            InGame = false;
            _overlayOnly = false;
            _overlayFps = 0;
            if (_view == null) return;
            ClearLocked();
        }
    }

    private static void ClearLocked()
    {
        _view!.Write(OffVisible, 0u);
        _view.Write(OffState, 0u);
        _view.Write(OffProgress, -1);
        _view.Write(OffSeconds, 0);
        BumpLocked();
    }

    private static void StartLifetimeLocked()
    {
        _lifetime?.Dispose();
        _lifetime = new Timer(_ => Hide(), null, 2700, Timeout.Infinite);
    }

    private static void BumpLocked()
    {
        _seq++;
        _view!.Write(OffSeq, _seq);
    }

    private static void RefreshDiagnosticsLocked()
    {
        _view!.Write(OffDebugLog, Logger.DebugMode ? 1u : 0u);
        WriteStringLocked(OffLogPath, 260, Logger.LogPath);
    }

    private static bool EnsureOpenLocked()
    {
        if (_view != null) return true;
        try
        {
            _file = MemoryMappedFile.CreateOrOpen(MapName, MapSize);
            _view = _file.CreateViewAccessor();
            _view.Write(OffMagic, Magic);
            _view.Write(OffVersion, Version);
            _view.Write(OffCaptureActive, 0u);
            _view.Write(OffCaptureFps, 60u);
            _view.Write(OffVisible, 0u);
            _view.Write(OffState, 0u);
            _view.Write(OffProgress, -1);
            _view.Write(OffSeconds, 0);
            _view.Write(OffSeq, 0u);
            WriteStringLocked(OffTitle, 64, "");
            WriteStringLocked(OffDetail, 128, "");
            WriteStringLocked(OffIcon, 260, IconPath());
            RefreshDiagnosticsLocked();
            return true;
        }
        catch (Exception ex)
        {
            Logger.Warn($"In-game overlay shared memory unavailable: {ex.Message}");
            _view = null;
            _file = null;
            return false;
        }
    }

    private static string IconPath()
    {
        string root = AppContext.BaseDirectory;
        string[] candidates =
        [
            Path.Combine(root, "assets", "icons", "clapperboard.png"),
            Path.Combine(root, "assets", "icons", "icon.ico"),
            Path.Combine(root, "assets", "icon.ico"),
            Path.Combine(root, "icon.ico"),
        ];
        foreach (string candidate in candidates)
        {
            if (File.Exists(candidate)) return candidate;
        }
        return "";
    }

    private static void WriteStringLocked(int offset, int capacity, string value)
    {
        var buffer = new char[capacity];
        int count = Math.Min(value.Length, capacity - 1);
        value.CopyTo(0, buffer, 0, count);
        _view!.WriteArray(offset, buffer, 0, capacity);
    }
}
