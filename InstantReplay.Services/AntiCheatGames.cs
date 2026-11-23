using System.Text.Json;

namespace InstantReplay.Services;

internal static class AntiCheatGames
{
    private sealed record Entry(string? Name, string? Executable, string? Keyword);
    private sealed record Catalog(Dictionary<string, string> Games, bool Failed);

    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNameCaseInsensitive = true,
        AllowTrailingCommas = true,
        ReadCommentHandling = JsonCommentHandling.Skip,
    };

    private static readonly object CatalogGate = new();
    private static Catalog? _shared;
    private static DateTime _retryAt;

    public static int Count => GetCatalog().Games.Count;

    public static bool IsProtected(string executable, out string gameName)
    {
        gameName = "";
        Catalog catalog = GetCatalog();
        if (catalog.Failed)
        {
            gameName = "anti-cheat game list unavailable";
            return true;
        }

        string name = Path.GetFileName(executable);
        if (name.Length > 0 && catalog.Games.TryGetValue(name, out string? matched))
        {
            gameName = matched;
            return true;
        }
        return false;
    }

    private static Catalog GetCatalog()
    {
        lock (CatalogGate)
        {
            if (_shared == null || (_shared.Failed && DateTime.UtcNow >= _retryAt))
            {
                _shared = Load();
                _retryAt = DateTime.UtcNow.AddSeconds(5);
            }
            return _shared;
        }
    }

    private static Catalog Load()
    {
        string path = Path.Combine(AppContext.BaseDirectory, "assets", "games", "anticheat_games.json");
        try
        {
            var entries = JsonSerializer.Deserialize<Entry[]>(File.ReadAllText(path), JsonOptions) ?? [];
            var games = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
            foreach (Entry entry in entries)
            {
                string exe = Path.GetFileName(entry.Executable?.Trim() ?? "");
                if (exe.Length == 0) continue;
                if (!games.ContainsKey(exe))
                    games[exe] = string.IsNullOrWhiteSpace(entry.Name) ? exe : entry.Name!.Trim();
            }
            Logger.Info($"Anti-cheat game list loaded: {games.Count} executables will never be injected.");
            return new Catalog(games, false);
        }
        catch (Exception ex)
        {
            Logger.Warn($"Anti-cheat game list could not be loaded from {path}: {ex.Message}. " +
                        "In-process capture is disabled until it is available.");
            return new Catalog(new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase), true);
        }
    }
}
