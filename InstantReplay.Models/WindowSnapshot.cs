namespace InstantReplay.Models;

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
