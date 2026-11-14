namespace InstantReplay.Models;

/// <summary>A release discovered by the updater.</summary>
internal sealed record UpdateRelease(string Tag, string Version, string Notes, string? DownloadUrl, long Size, string? Sha256);

/// <summary>Result of the most recent update check.</summary>
internal sealed record UpdateCheck(DateTimeOffset LastAttemptUtc, UpdateRelease? Release, string? Error, DateTimeOffset LastReminderUtc = default);

/// <summary>Outcome of downloading and staging an update.</summary>
internal sealed record UpdateInstallResult(bool Success, string Message, string Version, DateTimeOffset FinishedUtc);
