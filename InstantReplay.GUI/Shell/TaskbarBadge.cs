using System.Runtime.InteropServices;
using InstantReplay.Services;

namespace InstantReplay.GUI.Shell;

public enum BadgeKind
{
    None,
    Recording,
    Paused,
    Error,
}

public sealed class TaskbarBadge : IDisposable
{
    [ComImport]
    [Guid("56FDF344-FD6D-11d0-958A-006097C9A090")]
    private class TaskbarInstance
    {
    }

    [ComImport]
    [Guid("ea1afb91-9e28-4b86-90e9-9e9f8a5eefaf")]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface ITaskbarList3
    {
        void HrInit();
        void AddTab(IntPtr hwnd);
        void DeleteTab(IntPtr hwnd);
        void ActivateTab(IntPtr hwnd);
        void SetActiveAlt(IntPtr hwnd);
        void MarkFullscreenWindow(IntPtr hwnd, [MarshalAs(UnmanagedType.Bool)] bool fullscreen);
        void SetProgressValue(IntPtr hwnd, ulong completed, ulong total);
        void SetProgressState(IntPtr hwnd, int flags);
        void RegisterTab(IntPtr hwnd, IntPtr tab);
        void UnregisterTab(IntPtr hwnd);
        void SetTabOrder(IntPtr hwndTab, IntPtr hwndInsertBefore);
        void SetTabActive(IntPtr hwndTab, IntPtr hwndMDI, uint dwReserved);
        void ThumbBarAddButtons(IntPtr hwnd, uint cButtons, IntPtr pButton);
        void ThumbBarUpdateButtons(IntPtr hwnd, uint cButtons, IntPtr pButton);
        void ThumbBarSetImageList(IntPtr hwnd, IntPtr himl);
        void SetOverlayIcon(IntPtr hwnd, IntPtr icon, [MarshalAs(UnmanagedType.LPWStr)] string? description);
        void SetThumbnailTooltip(IntPtr hwnd, [MarshalAs(UnmanagedType.LPWStr)] string? tip);
        void SetThumbnailClip(IntPtr hwnd, IntPtr rect);
    }

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool FlashWindowEx(IntPtr hwnd, ref FLASHWINFO pwfi);

    [StructLayout(LayoutKind.Sequential)]
    private struct FLASHWINFO
    {
        public int cbSize;
        public IntPtr hwnd;
        public uint dwFlags;
        public uint uCount;
        public uint dwTimeout;
    }

    private const uint FlashwTray = 0x00000002;
    private const uint FlashwTimer = 0x00000004;
    private const uint FlashwStop = 0x00000000;

    private readonly IntPtr _hwnd;
    private ITaskbarList3? _taskbar;
    private BadgeKind _kind = BadgeKind.None;
    private bool _taskbarVisible = true;

    public TaskbarBadge(IntPtr hwnd)
    {
        _hwnd = hwnd;
        try
        {
            var instance = (ITaskbarList3)new TaskbarInstance();
            instance.HrInit();
            _taskbar = instance;
        }
        catch (Exception ex)
        {
            Logger.Warn($"TaskbarList3 unavailable: {ex.Message}");
        }
    }

    public void SetTaskbarVisible(bool visible)
    {
        if (_taskbarVisible == visible) return;
        _taskbarVisible = visible;
        if (!visible) StopFlashing();
    }

    public void SetOverlay(IntPtr icon, BadgeKind kind)
    {
        if (_kind == kind) return;

        if (_taskbar == null || !_taskbarVisible) return;

        _kind = kind;

        try
        {
            _taskbar.SetOverlayIcon(_hwnd, icon, kind switch
            {
                BadgeKind.Recording => "Recording",
                BadgeKind.Paused => "Not recording",
                BadgeKind.Error => "Something went wrong",
                _ => null,
            });
        }
        catch (Exception ex)
        {
            Logger.Warn($"SetOverlayIcon failed: {ex.Message}");
        }
    }

    public void Clear()
    {
        SetOverlay(IntPtr.Zero, BadgeKind.None);
    }

    public void Reset()
    {
        _kind = BadgeKind.None;
    }


    public void Flash()
    {
        if (_taskbar == null || !_taskbarVisible) return;

        try
        {
            var info = new FLASHWINFO
            {
                cbSize = Marshal.SizeOf<FLASHWINFO>(),
                hwnd = _hwnd,
                dwFlags = FlashwTray | FlashwTimer,
                uCount = uint.MaxValue,
                dwTimeout = 0,
            };
            _ = FlashWindowEx(_hwnd, ref info);
        }
        catch (Exception ex)
        {
            Logger.Warn($"FlashWindowEx failed: {ex.Message}");
        }
    }

    public void StopFlashing()
    {
        if (_taskbar == null) return;
        try
        {
            var info = new FLASHWINFO
            {
                cbSize = Marshal.SizeOf<FLASHWINFO>(),
                hwnd = _hwnd,
                dwFlags = FlashwStop,
            };
            _ = FlashWindowEx(_hwnd, ref info);
        }
        catch (Exception ex)
        {
            Logger.Warn($"FlashWindowEx stop failed: {ex.Message}");
        }
    }

    public void Dispose()
    {
        try
        {
            _taskbar?.SetOverlayIcon(_hwnd, IntPtr.Zero, null);
        }
        catch (Exception ex)
        {
            Logger.Warn($"Taskbar overlay cleanup failed: {ex.Message}");
        }
        if (_taskbar != null && Marshal.IsComObject(_taskbar)) _ = Marshal.ReleaseComObject(_taskbar);
        _taskbar = null;
    }
}
