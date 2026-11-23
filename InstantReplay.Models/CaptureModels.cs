namespace InstantReplay.Models;

public enum CaptureMethod
{
    Wgc = 0,
    Dxgi = 1,
    External = 3,
}

internal enum CaptureSource
{
    Wgc,
    Dxgi,
}

internal enum WindowMode
{
    Windowed,
    BorderlessFullscreen,
    ExclusiveFullscreen,
}

internal readonly record struct CaptureSurface(int Width, int Height, long Style, IntPtr Monitor, int DisplayWidth, int DisplayHeight);

internal sealed record CaptureWindow(IntPtr Handle, uint ProcessId, string Executable, string Title, string Game)
{
    public CaptureSurface Surface { get; init; }
}

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
    public CaptureSource Source =>
        Mode == WindowMode.ExclusiveFullscreen ? CaptureSource.Dxgi : CaptureSource.Wgc;

    public bool MultiDisplay => MonitorCount > 1;

    public (int Width, int Height) CanvasSize => (
        Math.Max(Math.Max(NativeWidth, DisplayWidth), WindowWidth),
        Math.Max(Math.Max(NativeHeight, DisplayHeight), WindowHeight));

    public string SourceKey => Source == CaptureSource.Wgc
        ? $"wgc:{Handle.ToInt64()}"
        : $"dxgi:{MonitorIndex}";

    public override string ToString()
    {
        return $"{Mode} on display {MonitorIndex + 1}/{MonitorCount} ({DisplayWidth}x{DisplayHeight}, " +
        $"native {NativeWidth}x{NativeHeight}, window {WindowWidth}x{WindowHeight}; {Reason})";
    }

}
