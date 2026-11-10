using System.Diagnostics;
using System.IO.MemoryMappedFiles;
using System.Runtime.InteropServices;
using System.Text.Json;
using System.Text;

namespace InstantReplay;

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
                games.TryAdd(exe, string.IsNullOrWhiteSpace(entry.Name) ? exe : entry.Name.Trim());
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

internal static class HookInjector
{
    public const string CaptureDll = "capture_hook.dll";
    public const string OverlayDll = "overlay_hook.dll";

    private const string CaptureReadyEvent = @"Local\InstantReplay.CaptureHook.Ready.v1";
    private const string OverlayReadyEvent = @"Local\InstantReplay.OverlayHook.Ready.v1";

    private const uint ProcessCreateThread = 0x0002;
    private const uint ProcessVmOperation = 0x0008;
    private const uint ProcessVmRead = 0x0010;
    private const uint ProcessVmWrite = 0x0020;
    private const uint ProcessQueryInformation = 0x0400;
    private const uint MemCommit = 0x1000;
    private const uint MemReserve = 0x2000;
    private const uint MemRelease = 0x8000;
    private const uint PageReadWrite = 0x04;
    private const uint DontResolveDllReferences = 0x00000001;
    private const int WhGetMessage = 3;
    private const uint SafeInjectMessage = 0x0400 + 432; // WM_USER + 432, same as OBS
    private const uint Th32CsSnapshotModule = 0x00000008;
    private const uint Th32CsSnapshotModule32 = 0x00000010;

    public static string CaptureDllPath =>
        Path.Combine(AppContext.BaseDirectory, "engine", "hooks", CaptureDll);

    public static string OverlayDllPath =>
        Path.Combine(AppContext.BaseDirectory, "engine", "hooks", OverlayDll);

    public static bool HasModule(uint processId, string moduleName) =>
        IsModuleLoaded(processId, moduleName);

    public static bool RequestShutdown(uint processId, out string error)
    {
        error = "";
        // Only the overlay is injected now; the capture hook entry stays so a hook left
        // over from an older build is still unloaded cleanly.
        foreach (string moduleName in new[] { CaptureDll, OverlayDll })
        {
            if (!TryFindModule(processId, moduleName, out ModuleEntry32 remote))
            {
                Logger.Debug($"Hook shutdown pid {processId}: {moduleName} is not loaded; skipping.");
                continue;
            }
            if (!RequestModuleShutdown(processId, remote, out error))
                return false;
        }
        return true;
    }

    private static bool RequestModuleShutdown(uint processId, ModuleEntry32 remote, out string error)
    {
        error = "";
        IntPtr local = LoadLibraryExW(remote.Path, IntPtr.Zero, DontResolveDllReferences);
        if (local == IntPtr.Zero)
        {
            error = $"LoadLibraryEx failed for {remote.Name} (error {Marshal.GetLastWin32Error()})";
            return false;
        }
        IntPtr shutdown;
        try
        {
            IntPtr localExport = GetProcAddress(local, "IR_RequestShutdown");
            if (localExport == IntPtr.Zero)
            {
                error = $"{remote.Name} does not export IR_RequestShutdown; restart the game to replace the old hook";
                return false;
            }
            shutdown = new IntPtr(remote.BaseAddress.ToInt64() +
                (localExport.ToInt64() - local.ToInt64()));
        }
        finally { FreeLibrary(local); }

        IntPtr process = OpenProcess(ProcessCreateThread | ProcessVmOperation | ProcessVmRead |
            ProcessQueryInformation, false, (int)processId);
        if (process == IntPtr.Zero)
        {
            error = $"OpenProcess failed for hook shutdown (error {Marshal.GetLastWin32Error()})";
            return false;
        }
        try
        {
            IntPtr thread = CreateRemoteThread(process, IntPtr.Zero, UIntPtr.Zero, shutdown,
                IntPtr.Zero, 0, out _);
            if (thread == IntPtr.Zero)
            {
                error = $"CreateRemoteThread failed for {remote.Name} shutdown (error {Marshal.GetLastWin32Error()})";
                return false;
            }
            try
            {
                if (WaitForSingleObject(thread, 5000) != 0 ||
                    !GetExitCodeThread(thread, out uint accepted) || accepted == 0)
                {
                    error = $"{remote.Name} rejected or timed out requesting cooperative shutdown";
                    return false;
                }
            }
            finally { CloseHandle(thread); }
        }
        finally { CloseHandle(process); }

        long deadline = Environment.TickCount64 + 7000;
        while (Environment.TickCount64 < deadline)
        {
            if (!IsModuleLoaded(processId, remote.Name))
            {
                Logger.Info($"Hook shutdown pid {processId}: {remote.Name} unloaded cooperatively.");
                return true;
            }
            Thread.Sleep(50);
        }
        error = $"{remote.Name} accepted cooperative shutdown but remained loaded";
        return false;
    }

    public static bool UsesSupportedGraphicsApi(uint processId, out string api)
    {
        List<string> modules = EnumerateModules(processId, out string diagnostic);
        Logger.Debug($"Hook probe pid {processId}: {modules.Count} modules enumerated ({diagnostic}).");
        if (modules.Any(m => Path.GetFileName(m).Equals("vulkan-1.dll", StringComparison.OrdinalIgnoreCase)))
        {
            api = "Vulkan (no hook)";
            return false;
        }
        foreach ((string Module, string Name) candidate in new[]
        {
            ("d3d12.dll", "Direct3D 12"),
            ("d3d11.dll", "Direct3D 11"),
            ("d3d9.dll", "Direct3D 9"),
            ("opengl32.dll", "OpenGL"),
        })
        {
            if (modules.Any(m => Path.GetFileName(m).Equals(candidate.Module, StringComparison.OrdinalIgnoreCase)))
            {
                api = candidate.Name;
                return true;
            }
        }
        api = $"none detected; {diagnostic}";
        return false;
    }

    public static bool HasAntiCheatModule(uint processId, out string module)
    {
        string[] markers =
        [
            "easyanticheat", "easyanticheat_eos", "bedaisy", "battleye",
            "vgk", "vgc", "faceit", "equ8", "xhunter", "xigncode",
            "gameguard", "nprotect", "ricochet", "ace-base", "ace-trace",
        ];
        foreach (string loaded in EnumerateModules(processId))
        {
            string stem = Path.GetFileNameWithoutExtension(loaded);
            if (markers.Any(marker => stem.Contains(marker, StringComparison.OrdinalIgnoreCase)))
            {
                module = loaded;
                return true;
            }
        }
        module = "";
        return false;
    }

    /// <summary>
    /// Loads the overlay hook (and the capture hook DLL that drives its per-frame
    /// callbacks; video capture itself stays disabled) into the game using the
    /// anti-cheat compatibility scheme from OBS: a WH_GETMESSAGE hook on the game's
    /// window thread, no remote threads and no LoadLibrary call inside the game.
    /// </summary>
    public static bool EnsureInjected(uint processId, uint threadId, out bool overlayReady, out string error)
    {
        overlayReady = false;
        error = "";
        Logger.Info($"Overlay injection starting for pid {processId} ('{OverlayDllPath}').");

        // Only the overlay DLL is ever loaded into a game. It hooks the game's present
        // calls itself and never reads frames; video is recorded outside the process
        // (WGC / DXGI), so the game keeps its frame rate and cannot be crashed by
        // frame interception.
        if (!File.Exists(OverlayDllPath))
        {
            error = $"hook DLL missing: {OverlayDllPath}";
            return false;
        }

        try
        {
            using var process = Process.GetProcessById((int)processId);
            if (process.HasExited)
            {
                error = "process exited";
                return false;
            }
        }
        catch (Exception ex)
        {
            error = ex.Message;
            return false;
        }

        // Only a query handle: the compatibility hook needs no memory or thread access.
        IntPtr handle = OpenProcess(ProcessQueryInformation, false, (int)processId);
        if (handle == IntPtr.Zero)
        {
            error = $"OpenProcess failed (error {Marshal.GetLastWin32Error()})";
            return false;
        }

        try
        {
            if (IsWow64Process(handle, out bool wow64) && wow64)
            {
                error = "the game is 32-bit; the native overlay hook is x64 only";
                return false;
            }

            if (!IsModuleLoaded(processId, OverlayDll))
            {
                Logger.Info($"Overlay injection pid {processId}: loading {OverlayDll}.");
                if (!InjectLibrarySafe(threadId, processId, OverlayDllPath, out error)) return false;
            }
            else Logger.Info($"Overlay injection pid {processId}: {OverlayDll} is already loaded.");

            overlayReady = WaitForEvent(OverlayReadyEvent, 4000);
            if (!overlayReady)
            {
                error = "the overlay hook loaded but could not hook the game's present calls";
                return false;
            }
            Logger.Info($"Overlay injection pid {processId}: overlay hook signaled ready.");
        }
        finally
        {
            CloseHandle(handle);
        }

        return true;
    }

    private static bool InjectLibrarySafe(uint threadId, uint processId, string dll, out string error)
    {
        error = "";
        string name = Path.GetFileName(dll);
        if (threadId == 0)
        {
            error = "the game window thread is unknown";
            return false;
        }

        // Map the DLL without running its DllMain: only its export address and file
        // name are needed, and the hook must not initialise inside this process.
        IntPtr local = LoadLibraryExW(dll, IntPtr.Zero, DontResolveDllReferences);
        if (local == IntPtr.Zero)
        {
            error = $"LoadLibraryEx failed for {name} (error {Marshal.GetLastWin32Error()})";
            return false;
        }

        IntPtr hook = IntPtr.Zero;
        try
        {
            IntPtr proc = GetProcAddress(local, "dummy_debug_proc");
            if (proc == IntPtr.Zero)
            {
                error = $"{name} does not export dummy_debug_proc; rebuild the hooks";
                return false;
            }

            hook = SetWindowsHookExW(WhGetMessage, proc, local, threadId);
            if (hook == IntPtr.Zero)
            {
                error = $"SetWindowsHookEx failed for {name} (error {Marshal.GetLastWin32Error()})";
                return false;
            }

            // The hook only injects once the thread processes a message, so post one
            // that carries the hook handle. The DLL unhooks itself and pins itself.
            if (!PostThreadMessageW(threadId, SafeInjectMessage, UIntPtr.Zero, hook))
            {
                error = $"PostThreadMessage failed for {name} (error {Marshal.GetLastWin32Error()}); " +
                        "the game window thread has no message queue";
                return false;
            }

            long deadline = Environment.TickCount64 + 5000;
            while (Environment.TickCount64 < deadline)
            {
                if (IsModuleLoaded(processId, name))
                {
                    Thread.Sleep(150); // let the DLL pin itself before the hook goes away
                    Logger.Info($"Hook injection pid {processId}: {name} loaded through the compatibility hook.");
                    return true;
                }
                Thread.Sleep(25);
            }
            error = $"{name} was not loaded by the game within 5s (the window thread is not pumping messages)";
            return false;
        }
        finally
        {
            if (hook != IntPtr.Zero) UnhookWindowsHookEx(hook);
            FreeLibrary(local);
        }
    }

    private static bool WaitForEvent(string name, int timeoutMs)
    {
        long deadline = Environment.TickCount64 + timeoutMs;
        while (Environment.TickCount64 < deadline)
        {
            try
            {
                EventWaitHandle ev = EventWaitHandle.OpenExisting(name);
                if (ev.WaitOne(0))
                {
                    ev.Dispose();
                    return true;
                }
                ev.Dispose();
            }
            catch (WaitHandleCannotBeOpenedException)
            {
            }
            Thread.Sleep(50);
        }
        return false;
    }

    private static bool IsModuleLoaded(uint processId, string moduleName)
    {
        return EnumerateModules(processId, out _)
            .Any(path => Path.GetFileName(path).Equals(moduleName, StringComparison.OrdinalIgnoreCase));
    }

    private static bool TryFindModule(uint processId, string moduleName, out ModuleEntry32 found)
    {
        IntPtr snapshot = CreateToolhelp32Snapshot(Th32CsSnapshotModule | Th32CsSnapshotModule32,
            processId);
        if (snapshot != IntPtr.Zero && snapshot != new IntPtr(-1))
        {
            try
            {
                var entry = new ModuleEntry32 { Size = (uint)Marshal.SizeOf<ModuleEntry32>() };
                if (Module32FirstW(snapshot, ref entry))
                {
                    do
                    {
                        if (entry.Name.Equals(moduleName, StringComparison.OrdinalIgnoreCase))
                        {
                            found = entry;
                            return true;
                        }
                        entry.Size = (uint)Marshal.SizeOf<ModuleEntry32>();
                    } while (Module32NextW(snapshot, ref entry));
                }
            }
            finally { CloseHandle(snapshot); }
        }
        found = default;
        return false;
    }

    private static IEnumerable<string> EnumerateModules(uint processId) =>
        EnumerateModules(processId, out _);

    private static List<string> EnumerateModules(uint processId, out string diagnostic)
    {
        var modules = new List<string>();
        int snapshotError = 0;
        for (int attempt = 1; attempt <= 8; attempt++)
        {
            IntPtr snapshot = CreateToolhelp32Snapshot(Th32CsSnapshotModule | Th32CsSnapshotModule32,
                processId);
            if (snapshot == IntPtr.Zero || snapshot == new IntPtr(-1))
            {
                snapshotError = Marshal.GetLastWin32Error();
                if (snapshotError == 24) { Thread.Sleep(10); continue; }
                break;
            }
            try
            {
                var entry = new ModuleEntry32 { Size = (uint)Marshal.SizeOf<ModuleEntry32>() };
                if (Module32FirstW(snapshot, ref entry))
                {
                    do { modules.Add(entry.Path.Length > 0 ? entry.Path : entry.Name); }
                    while (Module32NextW(snapshot, ref entry));
                }
                else snapshotError = Marshal.GetLastWin32Error();
            }
            finally { CloseHandle(snapshot); }
            if (modules.Count > 0) break;
            if (snapshotError == 24) Thread.Sleep(10); else break;
        }
        if (modules.Count > 0)
        {
            diagnostic = $"Toolhelp succeeded with {modules.Count} modules";
            return modules;
        }
        try
        {
            using Process process = Process.GetProcessById((int)processId);
            foreach (ProcessModule module in process.Modules)
                modules.Add(module.FileName);
            diagnostic = $"Toolhelp failed with Win32 {snapshotError}; Process.Modules returned {modules.Count}";
        }
        catch (Exception ex)
        {
            diagnostic = $"Toolhelp failed with Win32 {snapshotError}; Process.Modules failed: {ex.Message}";
        }
        return modules;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct ModuleEntry32
    {
        public uint Size;
        public uint ModuleId;
        public uint ProcessId;
        public uint GlobalUsage;
        public uint ProcessUsage;
        public IntPtr BaseAddress;
        public IntPtr BaseSize;
        public IntPtr ModuleHandle;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)] public string Name;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 260)] public string Path;
    }

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern IntPtr OpenProcess(uint access, bool inherit, int processId);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern IntPtr SetWindowsHookExW(int hookId, IntPtr proc, IntPtr module, uint threadId);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool UnhookWindowsHookEx(IntPtr hook);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool PostThreadMessageW(uint threadId, uint message, UIntPtr wParam, IntPtr lParam);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern IntPtr VirtualAllocEx(IntPtr process, IntPtr address, UIntPtr size,
        uint allocationType, uint protect);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool VirtualFreeEx(IntPtr process, IntPtr address, UIntPtr size,
        uint freeType);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool WriteProcessMemory(IntPtr process, IntPtr address, byte[] buffer,
        UIntPtr size, out UIntPtr written);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern IntPtr CreateRemoteThread(IntPtr process, IntPtr attributes,
        UIntPtr stackSize, IntPtr start, IntPtr parameter, uint flags, out uint threadId);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool GetExitCodeThread(IntPtr thread, out uint exitCode);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool CloseHandle(IntPtr handle);

    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    private static extern IntPtr GetModuleHandleW(string name);

    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Ansi)]
    private static extern IntPtr GetProcAddress(IntPtr module, string name);

    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    private static extern IntPtr LoadLibraryExW(string path, IntPtr file, uint flags);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool FreeLibrary(IntPtr module);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool IsWow64Process(IntPtr process, out bool wow64);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern IntPtr CreateToolhelp32Snapshot(uint flags, uint processId);

    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    private static extern bool Module32FirstW(IntPtr snapshot, ref ModuleEntry32 entry);

    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    private static extern bool Module32NextW(IntPtr snapshot, ref ModuleEntry32 entry);
}

internal static class OverlayBridge
{
    private const string MapName = @"Local\InstantReplay.Control.v1";
    private const int MapSize = 2048;
    private const uint Magic = 0x49524652;
    private const uint Version = 2;

    private const int OffMagic = 0;
    private const int OffVersion = 4;
    private const int OffCaptureActive = 8;
    private const int OffCaptureFps = 12;
    private const int OffVisible = 16;
    private const int OffState = 20;
    private const int OffProgress = 24;
    private const int OffSeconds = 28;
    private const int OffSeq = 32;
    private const int OffDebugLog = 36;
    private const int OffTitle = 40;
    private const int OffDetail = 168;
    private const int OffIcon = 424;
    private const int OffLogPath = 944;
    private const int OffAnimation = 1464;

    private static readonly object Gate = new();
    private static MemoryMappedFile? _file;
    private static MemoryMappedViewAccessor? _view;
    private static Timer? _lifetime;
    private static uint _seq;
    private static string _animationPathWritten = "";
    private static bool _overlayOnly;
    private static int _overlayFps;

    public static bool InGame { get; private set; }

    public static void Prepare()
    {
        lock (Gate)
        {
            if (!EnsureOpenLocked()) return;
            RefreshDiagnosticsLocked();
        }
    }

    public static void Refresh()
    {
        lock (Gate)
        {
            if (_view != null) RefreshDiagnosticsLocked();
        }
    }

    public static void SetCapture(bool active, int fps, bool overlayAvailable = true)
    {
        lock (Gate)
        {
            if (!EnsureOpenLocked()) return;
            InGame = active && overlayAvailable;
            _view!.Write(OffCaptureActive, active ? 1u : 0u);
            _view.Write(OffCaptureFps, (uint)Math.Clamp(fps, 15, 240));
            RefreshDiagnosticsLocked();
            if (!active) ClearLocked();
            else
            {
                _view.Write(OffVisible, 0u);
                _view.Write(OffState, 0u);
                _view.Write(OffProgress, -1);
                _view.Write(OffSeconds, 0);
                _lifetime?.Dispose();
                _lifetime = null;
                BumpLocked();
            }
        }
    }

    /// <summary>
    /// Overlay without recording through the hook: the in-game card is available
    /// (exclusive fullscreen, hooks loaded) but the hook never captures frames.
    /// </summary>
    public static void SetOverlayOnly(bool available, int fps)
    {
        lock (Gate)
        {
            int clamped = Math.Clamp(fps, 15, 240);
            // UpdateOverlay polls this every 200 ms. Only touch shared memory when
            // the requested state actually changes, so the sequence is not bumped
            // continuously (which would restart the in-game card every poll).
            if (available == _overlayOnly && clamped == _overlayFps && InGame == available) return;
            if (!EnsureOpenLocked()) return;
            _overlayOnly = available;
            _overlayFps = clamped;
            InGame = available;
            _view!.Write(OffCaptureActive, 0u);
            _view.Write(OffCaptureFps, (uint)clamped);
            RefreshDiagnosticsLocked();
            if (!available) ClearLocked();
        }
    }

    public static void BeginEncoding()
    {
        lock (Gate)
        {
            if (!EnsureOpenLocked()) return;
            _lifetime?.Dispose();
            _lifetime = null;
            EnsureAnimationPathLocked();
            _view!.Write(OffVisible, 1u);
            _view.Write(OffState, 1u);
            _view.Write(OffProgress, 0);
            _view.Write(OffSeconds, 0);
            WriteStringLocked(OffTitle, 64, "Clipping…");
            WriteStringLocked(OffDetail, 128, "Preparing clip…");
            BumpLocked();
        }
    }

    public static void ReportProgress(int percent)
    {
        lock (Gate)
        {
            if (_view == null || !InGame) return;
            int value = Math.Clamp(percent, 0, 100);
            _view.Write(OffProgress, value);
            WriteStringLocked(OffDetail, 128, value <= 0 ? "Preparing clip…" : $"Encoding {value}%");
            BumpLocked();
        }
    }

    public static void Complete(int seconds)
    {
        lock (Gate)
        {
            if (!EnsureOpenLocked()) return;
            int value = Math.Max(1, seconds);
            EnsureAnimationPathLocked();
            _view!.Write(OffVisible, 1u);
            _view.Write(OffState, 2u);
            _view.Write(OffProgress, 100);
            _view.Write(OffSeconds, value);
            WriteStringLocked(OffTitle, 64, $"Clipped Last {value} {(value == 1 ? "Second" : "Seconds")}");
            WriteStringLocked(OffDetail, 128, "Saved to your gallery");
            BumpLocked();
            StartLifetimeLocked();
        }
    }

    public static void Fail()
    {
        lock (Gate)
        {
            if (!EnsureOpenLocked()) return;
            _view!.Write(OffVisible, 1u);
            _view.Write(OffState, 3u);
            _view.Write(OffProgress, 100);
            _view.Write(OffSeconds, 0);
            WriteStringLocked(OffTitle, 64, "Failed To Clip");
            WriteStringLocked(OffDetail, 128, "See debug log for more details");
            BumpLocked();
            StartLifetimeLocked();
        }
    }

    public static void Hide()
    {
        lock (Gate)
        {
            _lifetime?.Dispose();
            _lifetime = null;
            if (_view == null) return;
            _view.Write(OffVisible, 0u);
            BumpLocked();
        }
    }

    public static void Reset()
    {
        lock (Gate)
        {
            _lifetime?.Dispose();
            _lifetime = null;
            InGame = false;
            _overlayOnly = false;
            _overlayFps = 0;
            if (_view == null) return;
            ClearLocked();
        }
    }

    private static void ClearLocked()
    {
        _view!.Write(OffVisible, 0u);
        _view.Write(OffState, 0u);
        _view.Write(OffProgress, -1);
        _view.Write(OffSeconds, 0);
        BumpLocked();
    }

    private static void StartLifetimeLocked()
    {
        _lifetime?.Dispose();
        _lifetime = new Timer(_ => Hide(), null, 2700, Timeout.Infinite);
    }

    private static void BumpLocked()
    {
        _seq++;
        _view!.Write(OffSeq, _seq);
    }

    private static void RefreshDiagnosticsLocked()
    {
        _view!.Write(OffDebugLog, Logger.DebugMode ? 1u : 0u);
        WriteStringLocked(OffLogPath, 260, Logger.LogPath);
    }

    private static bool EnsureOpenLocked()
    {
        if (_view != null) return true;
        try
        {
            _file = MemoryMappedFile.CreateOrOpen(MapName, MapSize);
            _view = _file.CreateViewAccessor();
            _view.Write(OffMagic, Magic);
            _view.Write(OffVersion, Version);
            _view.Write(OffCaptureActive, 0u);
            _view.Write(OffCaptureFps, 60u);
            _view.Write(OffVisible, 0u);
            _view.Write(OffState, 0u);
            _view.Write(OffProgress, -1);
            _view.Write(OffSeconds, 0);
            _view.Write(OffSeq, 0u);
            WriteStringLocked(OffTitle, 64, "");
            WriteStringLocked(OffDetail, 128, "");
            WriteStringLocked(OffIcon, 260, IconPath());
            EnsureAnimationPathLocked();
            RefreshDiagnosticsLocked();
            return true;
        }
        catch (Exception ex)
        {
            Logger.Warn($"In-game overlay shared memory unavailable: {ex.Message}");
            _view = null;
            _file = null;
            return false;
        }
    }

    private static string IconPath()
    {
        string root = AppContext.BaseDirectory;
        string[] candidates =
        [
            Path.Combine(root, "assets", "icons", "clapperboard.png"),
            Path.Combine(root, "assets", "icons", "icon.ico"),
            Path.Combine(root, "assets", "icon.ico"),
            Path.Combine(root, "icon.ico"),
        ];
        foreach (string candidate in candidates)
        {
            if (File.Exists(candidate)) return candidate;
        }
        return "";
    }

    /// <summary>
    /// The completion animation bundled with the app. Both the historical
    /// "clapperboard" and "clapboard" spellings are accepted, and any other MP4 in
    /// the folder is used as a last resort, so a re-encoded asset keeps working
    /// without another code change.
    /// </summary>
    public static string AnimationPath()
    {
        string bundled = BundledAnimationPath();
        if (bundled.Length == 0) return "";
        // Mirror the asset into %TEMP% once and hand every consumer (WinUI popup
        // and the in-game hook) that stable local copy. Reading it from the app
        // folder can be slow on first use (antivirus/OneDrive), which made the
        // first notification stall for seconds before the animation appeared.
        try
        {
            string dir = Path.Combine(Path.GetTempPath(), "InstantReplay", "cache");
            string cached = Path.Combine(dir, "clapboard.mp4");
            var source = new FileInfo(bundled);
            var target = new FileInfo(cached);
            if (!target.Exists || target.Length != source.Length ||
                target.LastWriteTimeUtc < source.LastWriteTimeUtc)
            {
                Directory.CreateDirectory(dir);
                File.Copy(bundled, cached, overwrite: true);
                Logger.Debug("Completion animation cached under %TEMP%.");
            }
            return cached;
        }
        catch (Exception ex)
        {
            Logger.Debug($"Completion animation cache unavailable: {ex.Message}");
            return bundled;
        }
    }

    private static string BundledAnimationPath()
    {
        string folder = Path.Combine(AppContext.BaseDirectory, "assets", "animations");
        string[] candidates =
        [
            Path.Combine(folder, "clapperboard.mp4"),
            Path.Combine(folder, "clapboard.mp4"),
        ];
        foreach (string candidate in candidates)
        {
            if (File.Exists(candidate)) return candidate;
        }
        try
        {
            if (Directory.Exists(folder))
            {
                foreach (string candidate in Directory.EnumerateFiles(folder, "*.mp4"))
                {
                    if (File.Exists(candidate)) return candidate;
                }
            }
        }
        catch (Exception ex)
        {
            Logger.Debug($"Completion animation lookup failed: {ex.Message}");
        }
        return "";
    }

    private static void EnsureAnimationPathLocked()
    {
        string path = AnimationPath();
        if (path.Length == 0 || path == _animationPathWritten) return;
        WriteStringLocked(OffAnimation, 260, path);
        _animationPathWritten = path;
    }

    private static void WriteStringLocked(int offset, int capacity, string value)
    {
        var buffer = new char[capacity];
        int count = Math.Min(value.Length, capacity - 1);
        value.CopyTo(0, buffer, 0, count);
        _view!.WriteArray(offset, buffer, 0, capacity);
    }
}
