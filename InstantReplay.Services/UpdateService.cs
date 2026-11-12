using System.Diagnostics;
using System.Net;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;
using InstantReplay.Core;
using InstantReplay.Models;

namespace InstantReplay.Services;

internal sealed class UpdateService
{
    internal const string Repository = "ghostneverdies/InstantReplay";
    internal const string InstallerName = "InstantReplaySetup.exe";
    internal static readonly TimeSpan CheckInterval = TimeSpan.FromHours(24);
    private const long MaxInstallerSize = 512L * 1024 * 1024;
    private static readonly HttpClient SharedClient = new() { Timeout = Timeout.InfiniteTimeSpan };
    private readonly HttpClient _client;
    private readonly string _statePath;
    internal Version CurrentVersion { get; }
    internal UpdateCheck State { get; private set; } = new(DateTimeOffset.MinValue, null, null);
    internal bool HasUpdate => State.Release is { } release && TryParseVersion(release.Version, out var version) && version > CurrentVersion;
    internal bool IsDue(DateTimeOffset now) => State.LastAttemptUtc > now || now - State.LastAttemptUtc >= CheckInterval;
    internal bool IsReminderDue(DateTimeOffset now) => HasUpdate && State.Error is null
        && (State.LastReminderUtc > now || now - State.LastReminderUtc >= CheckInterval);

    internal void MarkReminderShown(DateTimeOffset now)
    {
        State = State with { LastReminderUtc = now };
        SaveState();
    }

    internal UpdateService() : this(SharedClient,
        Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "InstantReplay", "updates.json"),
        typeof(UpdateService).Assembly.GetName().Version ?? new Version(1, 5, 0))
    { }

    internal UpdateService(HttpClient client, string statePath, Version currentVersion)
    {
        _client = client;
        _statePath = statePath;
        CurrentVersion = Normalize(currentVersion);
        try
        {
            if (File.Exists(statePath))
                State = JsonSerializer.Deserialize<UpdateCheck>(File.ReadAllText(statePath)) ?? State;
        }
        catch (Exception ex) { Logger.Warn($"Update state could not be read: {ex.Message}"); }
    }

    private static Version Normalize(Version version) => new(version.Major, version.Minor, Math.Max(0, version.Build), Math.Max(0, version.Revision));

    internal static bool TryParseVersion(string? tag, out Version version)
    {
        version = new Version(0, 0, 0, 0);
        string text = (tag ?? "").Trim();
        if (text.StartsWith('v') || text.StartsWith('V')) text = text[1..];
        if (!Regex.IsMatch(text, @"^\d+\.\d+(?:\.\d+){0,2}$") || !Version.TryParse(text, out var parsed)) return false;
        version = Normalize(parsed);
        return true;
    }

    internal async Task<UpdateCheck> CheckAsync(CancellationToken cancellationToken)
    {
        State = State with { LastAttemptUtc = DateTimeOffset.UtcNow };
        SaveState();
        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeout.CancelAfter(TimeSpan.FromSeconds(25));
        try
        {
            using var request = new HttpRequestMessage(HttpMethod.Get, $"https://api.github.com/repos/{Repository}/releases/latest");
            request.Headers.UserAgent.ParseAdd($"InstantReplay/{CurrentVersion.ToString(3)}");
            request.Headers.Accept.ParseAdd("application/vnd.github+json");
            request.Headers.Add("X-GitHub-Api-Version", "2022-11-28");
            using var response = await _client.SendAsync(request, HttpCompletionOption.ResponseHeadersRead, timeout.Token).ConfigureAwait(false);
            if (response.StatusCode == HttpStatusCode.NotFound)
                State = State with { Release = null, Error = null };
            else
            {
                if (response.StatusCode is HttpStatusCode.Forbidden or HttpStatusCode.TooManyRequests)
                    throw new InvalidOperationException("GitHub temporarily limited update checks. Try again later.");
                response.EnsureSuccessStatusCode();
                if (response.Content.Headers.ContentLength > 1024 * 1024)
                    throw new InvalidDataException("The release response is too large.");
                await using var input = await response.Content.ReadAsStreamAsync(timeout.Token).ConfigureAwait(false);
                using var bytes = new MemoryStream();
                byte[] buffer = new byte[16384];
                int count;
                while ((count = await input.ReadAsync(buffer, timeout.Token).ConfigureAwait(false)) > 0)
                {
                    if (bytes.Length + count > 1024 * 1024) throw new InvalidDataException("The release response is too large.");
                    bytes.Write(buffer, 0, count);
                }
                State = State with { Release = ParseRelease(bytes.ToArray()), Error = null };
            }
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested) { throw; }
        catch (Exception ex)
        {
            string message = ex is OperationCanceledException ? "The update check timed out. Check your connection and try again." :
                ex is HttpRequestException ? "Could not contact GitHub. Check your connection and try again." : ex.Message;
            State = State with { Error = message };
            Logger.Warn($"Update check failed: {ex.Message}");
        }
        SaveState();
        return State;
    }

    internal async Task UpdateGameCatalogAsync(CancellationToken cancellationToken)
    {
        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeout.CancelAfter(TimeSpan.FromSeconds(25));
        const string url = "https://raw.githubusercontent.com/ghostneverdies/InstantReplay/main/assets/games/games.json";
        using var request = new HttpRequestMessage(HttpMethod.Get, url);
        request.Headers.UserAgent.ParseAdd($"InstantReplay/{CurrentVersion.ToString(3)}");
        using var response = await _client.SendAsync(request, HttpCompletionOption.ResponseHeadersRead, timeout.Token).ConfigureAwait(false);
        response.EnsureSuccessStatusCode();
        if (response.Content.Headers.ContentLength > 4 * 1024 * 1024)
            throw new InvalidDataException("The game database response is too large.");
        byte[] bytes = await response.Content.ReadAsByteArrayAsync(timeout.Token).ConfigureAwait(false);
        if (bytes.Length > 4 * 1024 * 1024)
            throw new InvalidDataException("The game database response is too large.");
        GameRule[] rules = JsonSerializer.Deserialize<GameRule[]>(bytes)
            ?? throw new InvalidDataException("The downloaded game database is empty.");
        if (rules.Length < 1000 || rules.Any(rule => string.IsNullOrWhiteSpace(rule.Name)
            || string.IsNullOrWhiteSpace(rule.Executable)))
            throw new InvalidDataException("The downloaded game database is invalid.");
        string path = GameDetector.UpdatedCatalogPath;
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        string temporary = path + ".tmp";
        await File.WriteAllBytesAsync(temporary, bytes, timeout.Token).ConfigureAwait(false);
        File.Move(temporary, path, true);
        GameDetector.ReloadCatalog();
    }

    internal static UpdateRelease ParseRelease(byte[] bytes)
    {
        using var json = JsonDocument.Parse(bytes);
        var root = json.RootElement;
        if (root.GetProperty("draft").GetBoolean() || root.GetProperty("prerelease").GetBoolean())
            throw new InvalidDataException("The release is not a stable published version.");
        string tag = root.GetProperty("tag_name").GetString() ?? "";
        if (!TryParseVersion(tag, out var version)) throw new InvalidDataException("The latest release has an unsupported version tag.");
        string notes = root.TryGetProperty("body", out var body) ? body.GetString() ?? "" : "";
        if (notes.Length > 12000) notes = notes[..12000] + "…";
        string? url = null, digest = null;
        long size = 0;
        foreach (var asset in root.GetProperty("assets").EnumerateArray())
        {
            if (!string.Equals(asset.GetProperty("name").GetString(), InstallerName, StringComparison.OrdinalIgnoreCase)) continue;
            string candidate = asset.GetProperty("browser_download_url").GetString() ?? "";
            if (!IsReleaseDownload(candidate)) throw new InvalidDataException("The installer URL does not belong to this project's GitHub releases.");
            url = candidate;
            size = asset.GetProperty("size").GetInt64();
            string? rawDigest = asset.TryGetProperty("digest", out var value) ? value.GetString() : null;
            if (rawDigest is not null && Regex.IsMatch(rawDigest, "^sha256:[0-9a-fA-F]{64}$")) digest = rawDigest[7..].ToLowerInvariant();
            break;
        }
        return new(tag, version.ToString(version.Revision == 0 ? 3 : 4), notes, url, size, digest);
    }

    private static bool IsReleaseDownload(string value) => Uri.TryCreate(value, UriKind.Absolute, out var uri)
        && uri.Scheme == "https" && uri.IsDefaultPort && uri.UserInfo.Length == 0 && uri.Host.Equals("github.com", StringComparison.OrdinalIgnoreCase)
        && uri.AbsolutePath.StartsWith($"/{Repository}/releases/download/", StringComparison.Ordinal);

    private static bool IsDownloadHost(Uri uri) => uri.Scheme == "https" && uri.IsDefaultPort && uri.UserInfo.Length == 0
        && (uri.Host.Equals("github.com", StringComparison.OrdinalIgnoreCase) || uri.Host.Equals("release-assets.githubusercontent.com", StringComparison.OrdinalIgnoreCase)
            || uri.Host.Equals("objects.githubusercontent.com", StringComparison.OrdinalIgnoreCase) || uri.Host.Equals("releases.githubusercontent.com", StringComparison.OrdinalIgnoreCase));

    internal async Task<string> DownloadAsync(UpdateRelease release, string directory, IProgress<double>? progress, CancellationToken cancellationToken)
    {
        if (!TryParseVersion(release.Version, out var version) || version <= CurrentVersion)
            throw new InvalidOperationException("This release is not newer than the installed app.");
        if (release.DownloadUrl is null || !IsReleaseDownload(release.DownloadUrl))
            throw new InvalidOperationException($"The release does not contain {InstallerName}.");
        if (release.Size <= 0 || release.Size > MaxInstallerSize || release.Sha256 is null || !Regex.IsMatch(release.Sha256, "^[0-9a-fA-F]{64}$"))
            throw new InvalidDataException("The release is missing a valid installer size or SHA-256 checksum.");
        Directory.CreateDirectory(directory);
        string path = Path.Combine(directory, InstallerName);
        string partial = path + ".partial";
        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeout.CancelAfter(TimeSpan.FromMinutes(10));
        try
        {
            using var request = new HttpRequestMessage(HttpMethod.Get, release.DownloadUrl);
            request.Headers.UserAgent.ParseAdd($"InstantReplay/{CurrentVersion.ToString(3)}");
            using var response = await _client.SendAsync(request, HttpCompletionOption.ResponseHeadersRead, timeout.Token).ConfigureAwait(false);
            response.EnsureSuccessStatusCode();
            if (response.RequestMessage?.RequestUri is not { } finalUri || !IsDownloadHost(finalUri))
                throw new InvalidDataException("The installer download redirected outside GitHub.");
            if (response.Content.Headers.ContentLength is { } length && length != release.Size)
                throw new InvalidDataException("The installer size does not match the release.");
            await using (var input = await response.Content.ReadAsStreamAsync(timeout.Token).ConfigureAwait(false))
            await using (var output = new FileStream(partial, FileMode.CreateNew, FileAccess.Write, FileShare.None, 131072, true))
            using (var hash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256))
            {
                byte[] buffer = new byte[131072];
                long downloaded = 0;
                int count;
                while ((count = await input.ReadAsync(buffer, timeout.Token).ConfigureAwait(false)) > 0)
                {
                    downloaded += count;
                    if (downloaded > release.Size) throw new InvalidDataException("The installer exceeds the expected size.");
                    hash.AppendData(buffer, 0, count);
                    await output.WriteAsync(buffer.AsMemory(0, count), timeout.Token).ConfigureAwait(false);
                    progress?.Report(downloaded * 100.0 / release.Size);
                }
                if (downloaded != release.Size) throw new InvalidDataException("The installer download was incomplete.");
                if (!CryptographicOperations.FixedTimeEquals(hash.GetHashAndReset(), Convert.FromHexString(release.Sha256)))
                    throw new InvalidDataException("The installer checksum does not match. Nothing was installed.");
            }
            File.Move(partial, path);
            return path;
        }
        finally { if (File.Exists(partial)) File.Delete(partial); }
    }

    private void SaveState()
    {
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(_statePath)!);
            string temp = _statePath + "." + Guid.NewGuid().ToString("N") + ".tmp";
            try
            {
                File.WriteAllText(temp, JsonSerializer.Serialize(State), Encoding.UTF8);
                File.Move(temp, _statePath, true);
            }
            finally { if (File.Exists(temp)) File.Delete(temp); }
        }
        catch (Exception ex) { Logger.Warn($"Update state could not be saved: {ex.Message}"); }
    }

    internal static string ResultPath => Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "InstantReplay", "update-result.json");

    internal static async Task StartInstallerAsync(string installer, UpdateRelease release, CancellationToken cancellationToken)
    {
        string directory = Path.GetDirectoryName(installer)!;
        string script = Path.Combine(directory, "install-update.ps1");
        string configuration = Path.Combine(directory, "install-update.json");
        string ready = Path.Combine(directory, "ready");
        string exe = Environment.ProcessPath ?? throw new InvalidOperationException("The application path is unavailable.");
        using var parent = Process.GetCurrentProcess();
        File.WriteAllText(configuration, JsonSerializer.Serialize(new
        {
            ParentId = Environment.ProcessId,
            ParentStartTicks = parent.StartTime.ToUniversalTime().Ticks,
            Installer = installer,
            Sha256 = release.Sha256,
            App = exe,
            InstallDirectory = Path.GetDirectoryName(exe),
            Version = release.Version,
            Result = ResultPath,
            Ready = ready
        }), Encoding.UTF8);
        File.WriteAllText(script, InstallerScript, new UTF8Encoding(true));
        var start = new ProcessStartInfo(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.System), @"WindowsPowerShell\v1.0\powershell.exe"))
        {
            UseShellExecute = false,
            CreateNoWindow = true,
            WindowStyle = ProcessWindowStyle.Hidden,
            WorkingDirectory = directory
        };
        foreach (string argument in new[] { "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", script, "-UpdateConfiguration", configuration })
            start.ArgumentList.Add(argument);
        using var helper = Process.Start(start) ?? throw new InvalidOperationException("The update helper could not start.");
        try
        {
            for (int i = 0; i < 100; i++)
            {
                cancellationToken.ThrowIfCancellationRequested();
                if (File.Exists(ready)) return;
                if (helper.HasExited) throw new InvalidOperationException("The update helper exited before it was ready.");
                await Task.Delay(100, cancellationToken).ConfigureAwait(false);
            }
            throw new TimeoutException("The update helper did not become ready. The app has stayed open.");
        }
        catch
        {
            if (!helper.HasExited) helper.Kill();
            throw;
        }
    }

    internal const string InstallerScript = """
        param([Parameter(Mandatory=$true)][string]$UpdateConfiguration)
        $ErrorActionPreference = 'Stop'
        $update = Get-Content -LiteralPath $UpdateConfiguration -Raw | ConvertFrom-Json
        $parentExited = $false
        $installerStarted = $false
        $installerFinished = $false
        $success = $false
        $message = ''
        try {
            $parentProcess = Get-Process -Id $update.ParentId -ErrorAction SilentlyContinue
            Set-Content -LiteralPath $update.Ready -Value 'ready' -Encoding UTF8
            if ($null -ne $parentProcess -and $parentProcess.StartTime.ToUniversalTime().Ticks -eq $update.ParentStartTicks) {
                if (-not $parentProcess.WaitForExit(60000)) { throw 'Instant Replay did not exit. Nothing was installed.' }
            }
            $parentExited = $true
            $hashStream = [System.IO.File]::OpenRead($update.Installer)
            $hasher = [System.Security.Cryptography.SHA256]::Create()
            try { $checksum = [BitConverter]::ToString($hasher.ComputeHash($hashStream)).Replace('-', '') }
            finally { $hasher.Dispose(); $hashStream.Dispose() }
            if ($checksum -ine $update.Sha256) { throw 'The installer checksum changed. Nothing was installed.' }
            $arguments = '/S -quiet /UPDATE=1 /D=' + $update.InstallDirectory
            $installerInfo = New-Object System.Diagnostics.ProcessStartInfo
            $installerInfo.FileName = $update.Installer
            $installerInfo.Arguments = $arguments
            $installerInfo.UseShellExecute = $true
            $installerInfo.Verb = 'runas'
            $installerProcess = [System.Diagnostics.Process]::Start($installerInfo)
            $installerStarted = $true
            $installerProcess.WaitForExit()
            $installerFinished = $true
            if ($installerProcess.ExitCode -ne 0) { throw ('The installer failed with exit code ' + $installerProcess.ExitCode + '.') }
            $success = $true
            $message = 'Update installed successfully.'
        } catch {
            $message = $_.Exception.Message
        } finally {
            try {
                @{Success=$success; Message=$message; Version=$update.Version; FinishedUtc=[DateTimeOffset]::UtcNow.ToString('o')} |
                    ConvertTo-Json | Set-Content -LiteralPath $update.Result -Encoding UTF8
            } catch { }
            if ($parentExited -and (Test-Path -LiteralPath $update.App)) {
                try {
                    $appInfo = New-Object System.Diagnostics.ProcessStartInfo
                    $appInfo.FileName = $update.App
                    $appInfo.WorkingDirectory = $update.InstallDirectory
                    $appInfo.UseShellExecute = $false
                    [System.Diagnostics.Process]::Start($appInfo) | Out-Null
                } catch { }
            }
            if (-not $installerStarted -or $installerFinished) {
                Remove-Item -LiteralPath $update.Installer,$UpdateConfiguration,$update.Ready,$PSCommandPath -Force -ErrorAction SilentlyContinue
            }
        }
        """;
}
