namespace InstantReplay.Models;

/// <summary>Capture backend requested by the user.</summary>
public enum CaptureMethod
{
    Wgc = 0,
    Dxgi = 1,
    External = 3,
}

/// <summary>Capture backend that suits a <see cref="WindowMode"/>.</summary>
internal enum CaptureSource
{
    /// <summary>Windows Graphics Capture of the game window.</summary>
    Wgc,
    /// <summary>DXGI desktop duplication of the whole display the game is on.</summary>
    Dxgi,
}

/// <summary>How the game presents itself on screen.</summary>
internal enum WindowMode
{
    /// <summary>Normal window (title bar / frame), including maximized windows.</summary>
    Windowed,
    /// <summary>Frameless window that covers its monitor but is still composed by DWM.</summary>
    BorderlessFullscreen,
    /// <summary>Exclusive fullscreen: the game owns the display and bypasses normal composition.</summary>
    ExclusiveFullscreen,
}

/// <summary>Geometry and monitor information for a window that could be captured.</summary>
internal readonly record struct CaptureSurface(int Width, int Height, long Style, IntPtr Monitor, int DisplayWidth, int DisplayHeight);

/// <summary>A top-level window offered as a capture target.</summary>
internal sealed record CaptureWindow(IntPtr Handle, uint ProcessId, string Executable, string Title, string Game)
{
    public CaptureSurface Surface { get; init; }
}

/// <summary>Snapshot of the window and display state at the moment capture is (re)configured.</summary>
internal sealed record WindowSnapshot(
    IntPtr Handle,
    uint ProcessId,
    uint ThreadId,
    WindowMode Mode,
    IntPtr Monitor,
    int MonitorIndex,
    int MonitorCount,
    int DisplayWidth,
    int DisplayHeight,
    int NativeWidth,
    int NativeHeight,
    int WindowWidth,
    int WindowHeight,
    string Reason)
{
    /// <summary>Exclusive fullscreen is recorded through DXGI; everything else through WGC.</summary>
    public CaptureSource Source =>
        Mode == WindowMode.ExclusiveFullscreen ? CaptureSource.Dxgi : CaptureSource.Wgc;

    /// <summary>True when the machine has more than one display.</summary>
    public bool MultiDisplay => MonitorCount > 1;

    /// <summary>
    /// Highest resolution this game can produce on its display: the desktop's native
    /// size, the current mode, or the window itself. This is the video canvas size;
    /// smaller sources are centred on it with black bars.
    /// </summary>
    public (int Width, int Height) CanvasSize => (
        Math.Max(Math.Max(NativeWidth, DisplayWidth), WindowWidth),
        Math.Max(Math.Max(NativeHeight, DisplayHeight), WindowHeight));

    /// <summary>Identity of the capture target: changes when the backend, window or display must change.</summary>
    public string SourceKey => Source == CaptureSource.Wgc
        ? $"wgc:{Handle.ToInt64()}"
        : $"dxgi:{MonitorIndex}";

    public override string ToString() =>
        $"{Mode} on display {MonitorIndex + 1}/{MonitorCount} ({DisplayWidth}x{DisplayHeight}, " +
        $"native {NativeWidth}x{NativeHeight}, window {WindowWidth}x{WindowHeight}; {Reason})";
}
