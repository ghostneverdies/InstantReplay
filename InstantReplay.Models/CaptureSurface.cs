namespace InstantReplay.Models;

internal readonly record struct CaptureSurface(int Width, int Height, long Style, IntPtr Monitor, int DisplayWidth, int DisplayHeight);
