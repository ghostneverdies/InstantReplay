namespace InstantReplay.Models;

internal sealed record UpdateRelease(string Tag, string Version, string Notes, string? DownloadUrl, long Size, string? Sha256);
