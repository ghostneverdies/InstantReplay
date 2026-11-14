using System.Runtime.InteropServices;
using InstantReplay.Models;

namespace InstantReplay.Services;

internal static class WindowDetectionService
{
    private const int EnumCurrentSettings = -1;
    private const int EnumRegistrySettings = -2;
    private const int MonitorDefaultToNearest = 2;
    private const long WsCaption = 0xC00000;
    private const int GwlStyle = -16;
    private const int QunsRunningD3dFullScreen = 3;
    private const int CoverTolerancePx = 2;
    private const int DwmwaCloaked = 14;
    private static readonly IntPtr PerMonitorAwareV2 = new(-4);

    public static int MonitorCount => EnumerateMonitors().Count;

    public static WindowSnapshot? Analyze(IntPtr window)
    {
        if (window == IntPtr.Zero || !IsWindow(window) || IsIconic(window)) return null;
        if (!IsWindowVisible(window) || IsCloaked(window)) return null;
        IntPtr previousDpi = SetThreadDpiAwarenessContext(PerMonitorAwareV2);
        try
        {
            uint thread = GetWindowThreadProcessId(window, out uint pid);
            if (thread == 0 || pid == 0) return null;

            if (!TryGetFrame(window, out Rect frame)) return null;
            int windowW = frame.Right - frame.Left;
            int windowH = frame.Bottom - frame.Top;
            if (windowW < 2 || windowH < 2) return null;

            IntPtr monitor = MonitorFromWindow(window, MonitorDefaultToNearest);
            var info = new MonitorInfoEx
            {
                Size = Marshal.SizeOf<MonitorInfoEx>(),
                Device = string.Empty,
            };
            if (monitor == IntPtr.Zero || !GetMonitorInfoW(monitor, ref info)) return null;

            List<IntPtr> monitors = EnumerateMonitors();
            int monitorIndex = Math.Max(0, monitors.IndexOf(monitor));
            int displayW = info.Monitor.Right - info.Monitor.Left;
            int displayH = info.Monitor.Bottom - info.Monitor.Top;

            (int nativeW, int nativeH) = RegistrySize(info.Device, displayW, displayH);
            (WindowMode mode, string reason) = Classify(window, frame, info, displayW, displayH, nativeW, nativeH);

            return new WindowSnapshot(window, pid, thread, mode, monitor, monitorIndex, monitors.Count,
                displayW, displayH, nativeW, nativeH, windowW, windowH, reason);
        }
        catch (Exception ex)
        {
            Logger.Debug($"Window analysis failed: {ex.Message}");
            return null;
        }
        finally
        {
            if (previousDpi != IntPtr.Zero) SetThreadDpiAwarenessContext(previousDpi);
        }
    }

    private static (WindowMode Mode, string Reason) Classify(
        IntPtr window, Rect frame, MonitorInfoEx monitor,
        int displayW, int displayH, int nativeW, int nativeH)
    {
        long style = GetWindowLongPtrW(window, GwlStyle).ToInt64();
        bool framed = (style & WsCaption) == WsCaption;
        bool coversMonitor =
            frame.Left <= monitor.Monitor.Left + CoverTolerancePx &&
            frame.Top <= monitor.Monitor.Top + CoverTolerancePx &&
            frame.Right >= monitor.Monitor.Right - CoverTolerancePx &&
            frame.Bottom >= monitor.Monitor.Bottom - CoverTolerancePx;

        if (framed) return (WindowMode.Windowed, "has a title bar");
        if (!coversMonitor) return (WindowMode.Windowed, "does not cover its display");

        bool foreground = GetAncestor(GetForegroundWindow(), 2) == window;
        if (foreground && SHQueryUserNotificationState(out int state) == 0 &&
            state == QunsRunningD3dFullScreen)
            return (WindowMode.ExclusiveFullscreen, "Direct3D fullscreen notification state");

        bool modeChanged = nativeW > 0 && nativeH > 0 && (displayW != nativeW || displayH != nativeH);
        if (modeChanged)
            return (WindowMode.ExclusiveFullscreen, $"display mode {displayW}x{displayH} differs from desktop {nativeW}x{nativeH}");

        return (WindowMode.BorderlessFullscreen, "frameless window covering its display");
    }

    public static List<IntPtr> EnumerateMonitors()
    {
        var found = new List<(IntPtr Handle, bool Primary)>();
        EnumDisplayMonitors(IntPtr.Zero, IntPtr.Zero, (IntPtr handle, IntPtr hdc, ref Rect rect, IntPtr data) =>
        {
            var info = new MonitorInfoEx { Size = Marshal.SizeOf<MonitorInfoEx>(), Device = string.Empty };
            bool primary = GetMonitorInfoW(handle, ref info) && (info.Flags & 1) != 0;
            found.Add((handle, primary));
            return true;
        }, IntPtr.Zero);
        return found.OrderBy(m => m.Primary ? 0 : 1).Select(m => m.Handle).ToList();
    }

    private static bool TryGetFrame(IntPtr window, out Rect frame)
    {
        if (DwmGetWindowAttribute(window, 9, out frame, Marshal.SizeOf<Rect>()) == 0 &&
            frame.Right > frame.Left && frame.Bottom > frame.Top)
            return true;
        return GetWindowRect(window, out frame);
    }

    private static bool IsCloaked(IntPtr window)
    {
        return DwmGetWindowAttribute(window, DwmwaCloaked, out int cloaked, sizeof(int)) == 0 &&
               cloaked != 0;
    }

    private static (int Width, int Height) RegistrySize(string device, int fallbackW, int fallbackH)
    {
        try
        {
            var mode = new DevMode { Size = (ushort)Marshal.SizeOf<DevMode>(), DeviceName = string.Empty, FormName = string.Empty };
            if (EnumDisplaySettingsW(device, EnumRegistrySettings, ref mode) && mode.PelsWidth > 0 && mode.PelsHeight > 0)
                return ((int)mode.PelsWidth, (int)mode.PelsHeight);
        }
        catch (Exception ex)
        {
            Logger.Debug($"Registry display mode query failed: {ex.Message}");
        }
        return (fallbackW, fallbackH);
    }

    private delegate bool MonitorEnumProc(IntPtr monitor, IntPtr hdc, ref Rect rect, IntPtr data);

    [StructLayout(LayoutKind.Sequential)]
    private struct Rect { public int Left, Top, Right, Bottom; }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct MonitorInfoEx
    {
        public int Size;
        public Rect Monitor;
        public Rect Work;
        public uint Flags;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string Device;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct DevMode
    {
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string DeviceName;
        public ushort SpecVersion, DriverVersion, Size, DriverExtra;
        public uint Fields;
        public int PositionX, PositionY;
        public uint DisplayOrientation, DisplayFixedOutput;
        public short Color, Duplex, YResolution, TtOption, Collate;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string FormName;
        public ushort LogPixels;
        public uint BitsPerPel, PelsWidth, PelsHeight, DisplayFlags, DisplayFrequency;
        public uint IcmMethod, IcmIntent, MediaType, DitherType, Reserved1, Reserved2;
        public uint PanningWidth, PanningHeight;
    }

    [DllImport("user32.dll")] private static extern bool IsWindow(IntPtr window);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] private static extern bool IsIconic(IntPtr window);
    [DllImport("user32.dll")] private static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] private static extern IntPtr GetAncestor(IntPtr window, uint flags);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll")] private static extern IntPtr GetWindowLongPtrW(IntPtr window, int index);
    [DllImport("user32.dll")] private static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] private static extern IntPtr MonitorFromWindow(IntPtr window, int flags);
    [DllImport("user32.dll")] private static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern bool GetMonitorInfoW(IntPtr monitor, ref MonitorInfoEx info);
    [DllImport("user32.dll")]
    private static extern bool EnumDisplayMonitors(IntPtr hdc, IntPtr clip, MonitorEnumProc callback, IntPtr data);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern bool EnumDisplaySettingsW(string? device, int mode, ref DevMode devMode);
    [DllImport("dwmapi.dll")]
    private static extern int DwmGetWindowAttribute(IntPtr window, int attribute, out Rect value, int size);
    [DllImport("dwmapi.dll")]
    private static extern int DwmGetWindowAttribute(IntPtr window, int attribute, out int value, int size);
    [DllImport("shell32.dll")]
    private static extern int SHQueryUserNotificationState(out int state);
}
