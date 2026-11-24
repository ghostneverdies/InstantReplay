using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
namespace InstantReplay.Models;

internal sealed record UpdateRelease(string Tag, string Version, string Notes, string? DownloadUrl, long Size, string? Sha256);

internal sealed record UpdateCheck(DateTimeOffset LastAttemptUtc, UpdateRelease? Release, string? Error, DateTimeOffset LastReminderUtc = default);

internal sealed record UpdateInstallResult(bool Success, string Message, string Version, DateTimeOffset FinishedUtc);
