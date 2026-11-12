namespace InstantReplay.Models;

internal sealed record UpdateCheck(DateTimeOffset LastAttemptUtc, UpdateRelease? Release, string? Error, DateTimeOffset LastReminderUtc = default);
