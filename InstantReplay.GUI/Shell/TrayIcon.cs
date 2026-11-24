using System;
using System.Runtime.InteropServices;
using System.Windows.Interop;
using InstantReplay.Services;

namespace InstantReplay.GUI.Shell;

/// <summary>
/// Shell notification adapter for the WPF-UI Tray NotifyIcon. WPF-UI owns the
/// tray icon and its animated Fluent context menu; this class only supplies the
/// pieces WPF-UI does not expose: balloon tips, balloon clicks and badge icons.
/// </summary>
public sealed class TrayIcon : IDisposable
{
    public const int IconInfo = 1, IconWarning = 2, IconError = 3;

    private const int NIM_MODIFY = 1;
    private const int NIF_ICON = 0x2;
    private const int NIF_INFO = 0x10;
    private const int NIN_BALLOONUSERCLICK = 0x0405;

    private readonly HwndSource? _source;
    private readonly int _id;
    private readonly IntPtr _defaultIcon;
    private readonly HwndSourceHook _hook;
    private bool _disposed;

    public event Action? BalloonClicked;

    [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
    private static extern bool Shell_NotifyIcon(int dwMessage, ref NOTIFYICONDATA data);

    [DllImport("kernel32.dll")]
    private static extern uint GetLastError();

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct NOTIFYICONDATA
    {
        public int cbSize;
        public IntPtr hWnd;
        public int uID;
        public int uFlags;
        public int uCallbackMessage;
        public IntPtr hIcon;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string szTip;
        public int dwState;
        public int dwStateMask;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)] public string szInfo;
        public int uTimeoutOrVersion;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 64)] public string szInfoTitle;
        public int dwInfoFlags;
        public IntPtr hBalloonIcon;
    }

    public TrayIcon(Wpf.Ui.Tray.Controls.NotifyIcon control, IntPtr defaultIcon)
    {
        _source = control.HookWindow;
        _id = control.Id;
        _defaultIcon = defaultIcon;
        _hook = OnHook;
        _source?.AddHook(_hook);
    }

    private IntPtr OnHook(IntPtr hwnd, int msg, IntPtr wParam, IntPtr lParam, ref bool handled)
    {
        if (((int)lParam & 0xFFFF) == NIN_BALLOONUSERCLICK)
            BalloonClicked?.Invoke();
        return IntPtr.Zero;
    }

    public void SetIcon(IntPtr icon)
    {
        IntPtr handle = icon == IntPtr.Zero ? _defaultIcon : icon;
        if (handle == IntPtr.Zero || _source is null) return;

        var data = new NOTIFYICONDATA
        {
            cbSize = Marshal.SizeOf<NOTIFYICONDATA>(),
            hWnd = _source.Handle,
            uID = _id,
            uFlags = NIF_ICON,
            hIcon = handle,
        };
        if (!Shell_NotifyIcon(NIM_MODIFY, ref data))
            Logger.Warn($"Tray icon update failed (last error 0x{GetLastError():X8}).");
    }

    public void ShowBalloonTip(string title, string message, int iconFlag)
    {
        if (_source is null) return;

        var data = new NOTIFYICONDATA
        {
            cbSize = Marshal.SizeOf<NOTIFYICONDATA>(),
            hWnd = _source.Handle,
            uID = _id,
            uFlags = NIF_INFO,
            szInfo = message,
            szInfoTitle = title,
            dwInfoFlags = iconFlag,
        };
        if (!Shell_NotifyIcon(NIM_MODIFY, ref data))
            Logger.Warn($"Balloon modify failed (last error 0x{GetLastError():X8}).");
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        _source?.RemoveHook(_hook);
    }
}
