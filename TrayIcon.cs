using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;

namespace InstantReplay;

public sealed class TrayIcon : IDisposable
{
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
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct POINT { public int X; public int Y; }

    private const int NimAdd = 0, NimModify = 1, NimDelete = 2;
    private const int NifMessage = 0x1, NifIcon = 0x2, NifTip = 0x4, NifInfo = 0x10;

    public const int IconInfo = 0x1, IconWarning = 0x2, IconError = 0x3;

    public const uint WM_TRAYICON = 0x8001;
    private const int WM_LBUTTONDBLCLK = 0x0203;
    private const int WM_RBUTTONUP = 0x0205;
    private const int NIN_BALLOONUSERCLICK = 0x0405;

    private const uint MfString = 0x0, MfSeparator = 0x800, MfChecked = 0x08, MfGrayed = 0x01;
    private const uint TpmRightButton = 0x0002, TpmReturnCmd = 0x0100;

    [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
    private static extern bool Shell_NotifyIcon(int dwMessage, ref NOTIFYICONDATA data);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern IntPtr LoadImageW(IntPtr hinst, string lpszName, uint uType, int cxDesired, int cyDesired, uint fuLoad);

    [DllImport("user32.dll")] private static extern bool GetCursorPos(out POINT lpPoint);
    [DllImport("user32.dll")] private static extern IntPtr CreatePopupMenu();
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern bool AppendMenuW(IntPtr hMenu, uint uFlags, uint uIDNewItem, string lpNewItem);
    [DllImport("user32.dll")] private static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] private static extern int TrackPopupMenuEx(IntPtr hMenu, uint uFlags, int x, int y, IntPtr hWnd, IntPtr lptpm);
    [DllImport("user32.dll")] private static extern bool DestroyMenu(IntPtr hMenu);
    [DllImport("user32.dll")] private static extern bool PostMessage(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);

    private readonly IntPtr _hwnd;
    private const int Uid = 1;
    private readonly IntPtr _hIcon;

    public event Action? DoubleClicked;
    public event Action? BalloonClicked;

    public Func<List<TrayMenuItem>>? MenuItemsProvider { get; set; }

    public TrayIcon(IntPtr hwnd, string iconPath, string tooltip)
    {
        _hwnd = hwnd;
        _hIcon = LoadImageW(IntPtr.Zero, iconPath, 1, 16, 16, 0x10);

        var data = new NOTIFYICONDATA
        {
            cbSize = Marshal.SizeOf<NOTIFYICONDATA>(),
            hWnd = hwnd,
            uID = Uid,
            uFlags = NifMessage | NifIcon | NifTip,
            uCallbackMessage = (int)WM_TRAYICON,
            hIcon = _hIcon,
            szTip = tooltip,
        };
        Shell_NotifyIcon(NimAdd, ref data);
    }

    public void ShowBalloonTip(string title, string message, int iconFlag)
    {
        var data = new NOTIFYICONDATA
        {
            cbSize = Marshal.SizeOf<NOTIFYICONDATA>(),
            hWnd = _hwnd,
            uID = Uid,
            uFlags = NifInfo,
            szInfo = message,
            szInfoTitle = title,
            dwInfoFlags = iconFlag,
        };
        Shell_NotifyIcon(NimModify, ref data);
    }

    public void HandleMessage(IntPtr lParam)
    {
        int evt = (int)lParam;
        if (evt == WM_LBUTTONDBLCLK) DoubleClicked?.Invoke();
        else if (evt == NIN_BALLOONUSERCLICK) BalloonClicked?.Invoke();
        else if (evt == WM_RBUTTONUP) ShowContextMenu();
    }

    private void ShowContextMenu()
    {
        var items = MenuItemsProvider?.Invoke();
        if (items == null || items.Count == 0) return;

        IntPtr menu = CreatePopupMenu();
        var actions = new Dictionary<uint, Action?>();
        uint id = 1000;
        foreach (var item in items)
        {
            if (item.IsSeparator) { AppendMenuW(menu, MfSeparator, 0, ""); continue; }
            uint flags = MfString | (item.IsChecked ? MfChecked : 0) | (item.IsEnabled ? 0 : MfGrayed);
            AppendMenuW(menu, flags, id, item.Text);
            actions[id] = item.OnClick;
            id++;
        }

        GetCursorPos(out var pt);
        SetForegroundWindow(_hwnd);
        int cmd = TrackPopupMenuEx(menu, TpmRightButton | TpmReturnCmd, pt.X, pt.Y, _hwnd, IntPtr.Zero);
        PostMessage(_hwnd, 0, IntPtr.Zero, IntPtr.Zero);
        DestroyMenu(menu);

        if (cmd != 0 && actions.TryGetValue((uint)cmd, out var action)) action?.Invoke();
    }

    public void Dispose()
    {
        var data = new NOTIFYICONDATA { cbSize = Marshal.SizeOf<NOTIFYICONDATA>(), hWnd = _hwnd, uID = Uid };
        Shell_NotifyIcon(NimDelete, ref data);
    }
}

public sealed class TrayMenuItem
{
    public string Text { get; init; } = "";
    public Action? OnClick { get; init; }
    public bool IsSeparator { get; init; }
    public bool IsChecked { get; init; }
    public bool IsEnabled { get; init; } = true;

    public static TrayMenuItem Separator() => new() { IsSeparator = true };
}
