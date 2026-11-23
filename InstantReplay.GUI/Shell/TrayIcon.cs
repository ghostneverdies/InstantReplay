using System.Runtime.InteropServices;
using InstantReplay.Services;

namespace InstantReplay.GUI.Shell;

public sealed class TrayIcon : IDisposable
{
    public const int IconInfo = 1, IconWarning = 2, IconError = 3;

    public const uint WM_TRAYICON = 0x8001;

    private readonly IntPtr _hwnd;
    private readonly IntPtr _hIcon;
    private Func<List<TrayMenuItem>>? _itemsProvider;
    private bool _disposed;

    public event Action? DoubleClicked;
    public event Action? BalloonClicked;

    public Func<List<TrayMenuItem>>? MenuItemsProvider
    {
        get => _itemsProvider;
        set => _itemsProvider = value;
    }

    private enum PreferredAppMode { Default = 0, AllowDark = 1, ForceDark = 2, ForceLight = 3, Max = 4 }

    [DllImport("uxtheme.dll", EntryPoint = "#135", SetLastError = true, CharSet = CharSet.Unicode)]
    private static extern PreferredAppMode SetPreferredAppMode(PreferredAppMode appMode);

    [DllImport("uxtheme.dll", EntryPoint = "#136", SetLastError = true, CharSet = CharSet.Unicode)]
    private static extern void FlushMenuThemes();

    [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
    private static extern bool Shell_NotifyIcon(int dwMessage, ref NOTIFYICONDATA data);

    [DllImport("kernel32.dll")]
    private static extern uint GetLastError();

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern IntPtr LoadImageW(IntPtr hinst, string lpszName, uint uType, int cxDesired, int cyDesired, uint fuLoad);

    [DllImport("user32.dll")] private static extern bool GetCursorPos(out POINT lpPoint);
    [DllImport("user32.dll")] private static extern IntPtr CreatePopupMenu();
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern bool AppendMenuW(IntPtr hMenu, uint uFlags, uint uIDNewItem, string lpNewItem);
    [DllImport("user32.dll")] private static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] private static extern int TrackPopupMenuEx(IntPtr hMenu, uint uFlags, int x, int y, IntPtr hWnd, IntPtr lptpm);
    [DllImport("user32.dll")] private static extern bool DestroyMenu(IntPtr hMenu);
    [DllImport("user32.dll")] private static extern bool PostMessage(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);

    private const int NimAdd = 0, NimModify = 1, NimDelete = 2, NimSetVersion = 4;
    private const int NifMessage = 0x1, NifIcon = 0x2, NifTip = 0x4, NifInfo = 0x10;
    private const uint NotifyIconVersion4 = 4;

    private const uint MfString = 0x0, MfSeparator = 0x800, MfChecked = 0x08, MfGrayed = 0x01;
    private const uint TpmRightButton = 0x0002, TpmReturnCmd = 0x0100;

    private const int WM_LBUTTONDBLCLK = 0x0203;
    private const int WM_RBUTTONUP = 0x0205;
    private const int WM_CONTEXTMENU = 0x007B;
    private const int NIN_BALLOONUSERCLICK = 0x0405;

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

    [StructLayout(LayoutKind.Sequential)]
    private struct POINT { public int X; public int Y; }

    public TrayIcon(IntPtr hwnd, string iconPath, string tooltip)
    {
        _hwnd = hwnd;
        _itemsProvider = null;

        try
        {
            _ = SetPreferredAppMode(PreferredAppMode.AllowDark);
            FlushMenuThemes();
        }
        catch { }

        _hIcon = LoadImageW(IntPtr.Zero, iconPath, 2, 16, 16, 0x10);

        var data = new NOTIFYICONDATA
        {
            cbSize = Marshal.SizeOf<NOTIFYICONDATA>(),
            hWnd = hwnd,
            uID = 1,
            uFlags = NifMessage | NifIcon | NifTip,
            uCallbackMessage = (int)WM_TRAYICON,
            hIcon = _hIcon,
            szTip = tooltip,
        };
        if (!Shell_NotifyIcon(NimAdd, ref data))
            Logger.Warn($"Tray icon add failed (last error 0x{GetLastError():X8}).");
        else
            Logger.Info("Tray icon added.");

        var version = new NOTIFYICONDATA
        {
            cbSize = Marshal.SizeOf<NOTIFYICONDATA>(),
            hWnd = hwnd,
            uID = 1,
            uTimeoutOrVersion = (int)NotifyIconVersion4,
        };
        if (!Shell_NotifyIcon(NimSetVersion, ref version))
            Logger.Warn($"Tray icon version negotiation failed (last error 0x{GetLastError():X8}).");
    }

    public void SetIcon(IntPtr icon)
    {
        IntPtr handle = icon == IntPtr.Zero ? _hIcon : icon;
        if (handle == IntPtr.Zero) return;

        var data = new NOTIFYICONDATA
        {
            cbSize = Marshal.SizeOf<NOTIFYICONDATA>(),
            hWnd = _hwnd,
            uID = 1,
            uFlags = NifIcon,
            hIcon = handle,
        };
        if (!Shell_NotifyIcon(NimModify, ref data))
            Logger.Warn($"Tray icon update failed (last error 0x{GetLastError():X8}).");
    }

    public void ShowBalloonTip(string title, string message, int iconFlag)
    {
        var data = new NOTIFYICONDATA
        {
            cbSize = Marshal.SizeOf<NOTIFYICONDATA>(),
            hWnd = _hwnd,
            uID = 1,
            uFlags = NifInfo,
            szInfo = message,
            szInfoTitle = title,
            dwInfoFlags = iconFlag,
        };
        if (!Shell_NotifyIcon(NimModify, ref data))
            Logger.Warn($"Balloon modify failed (last error 0x{GetLastError():X8}).");
    }

    public void HandleMessage(IntPtr lParam)
    {
        int evt = (int)lParam & 0xFFFF;
        if (evt == WM_LBUTTONDBLCLK) DoubleClicked?.Invoke();
        else if (evt == NIN_BALLOONUSERCLICK) BalloonClicked?.Invoke();
        else if (evt == WM_RBUTTONUP || evt == WM_CONTEXTMENU) ShowContextMenu();
    }

    private void ShowContextMenu()
    {
        var items = MenuItemsProvider?.Invoke();
        if (items == null || items.Count == 0) return;

        IntPtr menu = CreatePopupMenu();
        var actions = new Dictionary<int, Action?>();
        int id = 1000;
        foreach (var item in items)
        {
            if (item.IsSeparator) { _ = AppendMenuW(menu, MfSeparator, 0, ""); continue; }
            uint flags = MfString | (item.IsChecked ? MfChecked : 0) | (item.IsEnabled ? 0u : MfGrayed);
            _ = AppendMenuW(menu, flags, (uint)id, item.Text);
            actions[id] = item.OnClick;
            id++;
        }

        _ = GetCursorPos(out var pt);
        _ = SetForegroundWindow(_hwnd);
        int cmd = TrackPopupMenuEx(menu, TpmRightButton | TpmReturnCmd, pt.X, pt.Y, _hwnd, IntPtr.Zero);
        _ = PostMessage(_hwnd, 0, IntPtr.Zero, IntPtr.Zero);
        _ = DestroyMenu(menu);

        if (cmd != 0 && actions.TryGetValue(cmd, out var action)) action?.Invoke();
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        var data = new NOTIFYICONDATA { cbSize = Marshal.SizeOf<NOTIFYICONDATA>(), hWnd = _hwnd, uID = 1 };
        _ = Shell_NotifyIcon(NimDelete, ref data);
    }
}

public sealed class TrayMenuItem
{
    public string Text { get; init; } = "";
    public string Glyph { get; init; } = "";
    public string Shortcut { get; init; } = "";
    public Action? OnClick { get; init; }
    public bool IsSeparator { get; init; }
    public bool IsCheckable { get; init; }
    public bool IsChecked { get; init; }
    public bool IsEnabled { get; init; } = true;

    public static TrayMenuItem Separator()
    {
        return new() { IsSeparator = true };
    }

}
