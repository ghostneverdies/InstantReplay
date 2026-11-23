using System.Diagnostics;
using System.Runtime.InteropServices;

namespace InstantReplay.Services;

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
    private const uint SafeInjectMessage = 0x0400 + 432;
    private const uint Th32CsSnapshotModule = 0x00000008;
    private const uint Th32CsSnapshotModule32 = 0x00000010;

    public static string CaptureDllPath =>
        Path.Combine(AppContext.BaseDirectory, "engine", "hooks", "bin", CaptureDll);

    public static string OverlayDllPath =>
        Path.Combine(AppContext.BaseDirectory, "engine", "hooks", "bin", OverlayDll);

    public static bool HasModule(uint processId, string moduleName)
    {
        return IsModuleLoaded(processId, moduleName);
    }


    public static bool RequestShutdown(uint processId, out string error)
    {
        error = "";
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
        finally { _ = FreeLibrary(local); }

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
            finally { _ = CloseHandle(thread); }
        }
        finally { _ = CloseHandle(process); }

        long deadline = Environment.TickCount + 7000;
        while (Environment.TickCount < deadline)
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

    public static bool EnsureInjected(uint processId, uint threadId, out bool overlayReady, out string error)
    {
        overlayReady = false;
        error = "";
        Logger.Info($"Overlay injection starting for pid {processId} ('{OverlayDllPath}').");

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
            _ = CloseHandle(handle);
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

            if (!PostThreadMessageW(threadId, SafeInjectMessage, UIntPtr.Zero, hook))
            {
                error = $"PostThreadMessage failed for {name} (error {Marshal.GetLastWin32Error()}); " +
                        "the game window thread has no message queue";
                return false;
            }

            long deadline = Environment.TickCount + 5000;
            while (Environment.TickCount < deadline)
            {
                if (IsModuleLoaded(processId, name))
                {
                    Thread.Sleep(150);
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
            if (hook != IntPtr.Zero) _ = UnhookWindowsHookEx(hook);
            _ = FreeLibrary(local);
        }
    }

    private static bool WaitForEvent(string name, int timeoutMs)
    {
        long deadline = Environment.TickCount + timeoutMs;
        while (Environment.TickCount < deadline)
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
            finally { _ = CloseHandle(snapshot); }
        }
        found = default;
        return false;
    }

    private static IEnumerable<string> EnumerateModules(uint processId)
    {
        return EnumerateModules(processId, out _);
    }


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
            finally { _ = CloseHandle(snapshot); }
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
