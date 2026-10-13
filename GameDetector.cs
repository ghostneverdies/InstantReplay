using System.Collections.Frozen;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;

namespace InstantReplay;

public sealed record GameRule(string Name, string Executable, string Keyword = "")
{
    public string Description => string.IsNullOrWhiteSpace(Keyword) ? Executable : $"{Executable} · title contains {Keyword}";
}

internal readonly record struct CaptureSurface(int Width, int Height, long Style, IntPtr Monitor, int DisplayWidth, int DisplayHeight);
internal sealed record CaptureWindow(IntPtr Handle, uint ProcessId, string Executable, string Title, string Game)
{
    public CaptureSurface Surface { get; init; }
}

internal sealed class GameDetector
{
    private sealed record Catalog(FrozenDictionary<string, GameRule[]> Rules, int Count);
    private static readonly Lazy<Catalog> SharedCatalog = new(LoadCatalog);
    private readonly FrozenDictionary<string, GameRule[]> _custom;
    private readonly Dictionary<uint, (string Executable, long Expiry)> _processCache = [];
    public int CatalogCount => SharedCatalog.Value.Count;
    private static readonly HashSet<string> Excluded = new(StringComparer.OrdinalIgnoreCase)
    {
        "explorer.exe", "dwm.exe", "sihost.exe", "ShellExperienceHost.exe", "StartMenuExperienceHost.exe",
        "SearchHost.exe", "SearchApp.exe", "TextInputHost.exe", "ApplicationFrameHost.exe", "InstantReplay.exe",
        "steam.exe", "EpicGamesLauncher.exe", "Battle.net.exe", "RiotClientServices.exe", "upc.exe",
        "msedgewebview2.exe"
    };
    private static readonly HashSet<string> Generic = new(StringComparer.OrdinalIgnoreCase)
    { "java.exe", "javaw.exe", "game.exe", "launcher.exe", "client.exe", "main.exe", "application.exe", "python.exe", "pythonw.exe" };
    private static readonly HashSet<string> MinecraftGameClasses = new(StringComparer.OrdinalIgnoreCase)
    { "LWJGL", "GLFW30", "SDL_app" };

    public GameDetector(IEnumerable<GameRule> custom)
    {
        _custom = custom.GroupBy(r => Path.GetFileName(r.Executable), StringComparer.OrdinalIgnoreCase)
            .ToFrozenDictionary(g => g.Key, g => g.ToArray(), StringComparer.OrdinalIgnoreCase);
        _ = SharedCatalog.Value;
    }
    private static Catalog LoadCatalog()
    {
        try
        {
            var rules = JsonSerializer.Deserialize<GameRule[]>(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "assets", "games.json"))) ?? [];
            var index = rules.GroupBy(r => Path.GetFileName(r.Executable), StringComparer.OrdinalIgnoreCase)
                .ToFrozenDictionary(g => g.Key, g => g.ToArray(), StringComparer.OrdinalIgnoreCase);
            return new Catalog(index, rules.Select(r => r.Name).Distinct().Count());
        }
        catch (Exception ex) { Logger.Warn($"Game catalog could not be loaded: {ex.Message}"); }
        return new Catalog(new Dictionary<string, GameRule[]>().ToFrozenDictionary(StringComparer.OrdinalIgnoreCase), 0);
    }
    public static bool IsAlive(CaptureWindow target)
    {
        if (!IsWindow(target.Handle)) return false;
        GetWindowThreadProcessId(target.Handle, out uint pid);
        return pid == target.ProcessId;
    }
    public static bool IsForeground(CaptureWindow target) =>
        GetAncestor(GetForegroundWindow(), 2) == target.Handle && IsAlive(target);
    private string? Executable(uint pid)
    {
        long now = Environment.TickCount64;
        if (_processCache.TryGetValue(pid, out var cached) && cached.Expiry > now) return cached.Executable;
        IntPtr process = OpenProcess(0x1000, false, pid);
        if (process == IntPtr.Zero) return null;
        try
        {
            var path = new StringBuilder(32768);
            uint length = (uint)path.Capacity;
            if (!QueryFullProcessImageNameW(process, 0, path, ref length)) return null;
            string executable = Path.GetFileName(path.ToString());
            if (_processCache.Count >= 64) _processCache.Clear();
            _processCache[pid] = (executable, now + 5000);
            return executable;
        }
        finally { CloseHandle(process); }
    }

    internal static bool Matches(GameRule rule, string executable, string title, bool builtIn)
    {
        if (!Path.GetFileName(rule.Executable).Equals(executable, StringComparison.OrdinalIgnoreCase)) return false;
        string keyword = rule.Keyword.Trim();
        if (builtIn && Generic.Contains(executable) && keyword.Length == 0) keyword = rule.Name;
        return keyword.Length == 0 || title.Contains(keyword, StringComparison.OrdinalIgnoreCase);
    }

    internal static bool MatchesGameSurface(GameRule rule, string executable, string windowClass) =>
        !((executable.Equals("java.exe", StringComparison.OrdinalIgnoreCase) || executable.Equals("javaw.exe", StringComparison.OrdinalIgnoreCase))
            && rule.Name.StartsWith("Minecraft", StringComparison.OrdinalIgnoreCase)) || MinecraftGameClasses.Contains(windowClass);

    internal GameRule? ListedRule(string executable, string title, string? windowClass = null)
    {
        var rule = _custom.TryGetValue(executable, out var custom) ? custom.FirstOrDefault(r => Matches(r, executable, title, false)) : null;
        if (rule == null && SharedCatalog.Value.Rules.TryGetValue(executable, out var candidates))
            rule = candidates.FirstOrDefault(r => Matches(r, executable, title, true)
                && (windowClass is null || MatchesGameSurface(r, executable, windowClass)));
        return rule;
    }

    public CaptureWindow? Foreground()
    {
        IntPtr previousDpi = SetThreadDpiAwarenessContext(new IntPtr(-4));
        try
        {
            IntPtr window = GetAncestor(GetForegroundWindow(), 2);
            if (window == IntPtr.Zero || !IsWindowVisible(window) || IsIconic(window)) return null;
            GetWindowThreadProcessId(window, out uint pid);
            if (pid == 0 || pid == Environment.ProcessId) return null;
            long style = GetWindowLongPtrW(window, -20).ToInt64();
            if ((style & 0x80) != 0) return null;
            if (DwmGetWindowAttribute(window, 14, out int cloaked, sizeof(int)) == 0 && cloaked != 0) return null;
            var text = new StringBuilder(512);
            GetWindowTextW(window, text, text.Capacity);
            string title = text.ToString().Trim();
            if (title.Length == 0 || title == "Program Manager" || title == "Windows Input Experience") return null;
            if (!GetClientRect(window, out var rect) || rect.Right - rect.Left < 2 || rect.Bottom - rect.Top < 2) return null;
            string? executable = Executable(pid);
            if (executable == null) return null;
            if (Excluded.Contains(executable)) return null;
            var className = new StringBuilder(256);
            GetClassNameW(window, className, className.Capacity);
            var rule = ListedRule(executable, title, className.ToString());
            if (rule == null) return null;
            IntPtr monitor = MonitorFromWindow(window, 2);
            var info = new MonitorInfo { Size = Marshal.SizeOf<MonitorInfo>() };
            if (!GetMonitorInfoW(monitor, ref info)) return null;
            var target = new CaptureWindow(window, pid, executable, title, rule.Name)
            {
                Surface = new CaptureSurface(rect.Right - rect.Left, rect.Bottom - rect.Top,
                    GetWindowLongPtrW(window, -16).ToInt64() & 0x80C40000L, monitor,
                    info.Monitor.Right - info.Monitor.Left, info.Monitor.Bottom - info.Monitor.Top)
            };
            return IsForeground(target) ? target : null;
        }
        catch (Exception ex) { Logger.Debug($"Foreground detection: {ex.Message}"); return null; }
        finally { if (previousDpi != IntPtr.Zero) SetThreadDpiAwarenessContext(previousDpi); }
    }

    [StructLayout(LayoutKind.Sequential)] private struct MonitorInfo { public int Size; public Rect Monitor, Work; public uint Flags; }
    [DllImport("user32.dll")] private static extern IntPtr MonitorFromWindow(IntPtr window, uint flags);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern bool GetMonitorInfoW(IntPtr monitor, ref MonitorInfo info);
    [StructLayout(LayoutKind.Sequential)] private struct Rect { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] private static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] private static extern bool IsWindow(IntPtr window);
    [DllImport("user32.dll")] private static extern IntPtr GetAncestor(IntPtr window, uint flags);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] private static extern bool IsIconic(IntPtr window);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern int GetWindowTextW(IntPtr window, StringBuilder text, int capacity);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern int GetClassNameW(IntPtr window, StringBuilder text, int capacity);
    [DllImport("user32.dll")] private static extern IntPtr GetWindowLongPtrW(IntPtr window, int index);
    [DllImport("user32.dll")] private static extern bool GetClientRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] private static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("dwmapi.dll")] private static extern int DwmGetWindowAttribute(IntPtr window, int attribute, out int value, int size);
    [DllImport("kernel32.dll")] private static extern IntPtr OpenProcess(uint access, bool inherit, uint pid);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] private static extern bool QueryFullProcessImageNameW(IntPtr process, uint flags, StringBuilder path, ref uint length);
    [DllImport("kernel32.dll")] private static extern bool CloseHandle(IntPtr handle);
}
