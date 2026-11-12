namespace InstantReplay.Models;

internal sealed record CaptureWindow(IntPtr Handle, uint ProcessId, string Executable, string Title, string Game)
{
    public CaptureSurface Surface { get; init; }
}
