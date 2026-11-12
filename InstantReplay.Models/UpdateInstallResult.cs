namespace InstantReplay.Models;

internal sealed record UpdateInstallResult(bool Success, string Message, string Version, DateTimeOffset FinishedUtc);
