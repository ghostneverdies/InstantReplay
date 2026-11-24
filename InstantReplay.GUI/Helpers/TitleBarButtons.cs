using System;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;
using InstantReplay.Services;

namespace InstantReplay.GUI.Helpers;

internal static class TitleBarButtons
{
    [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW")]
    private static extern IntPtr GetWindowLongPtr(IntPtr hWnd, int nIndex);

    [DllImport("user32.dll", EntryPoint = "SetWindowLongPtrW")]
    private static extern IntPtr SetWindowLongPtr(IntPtr hWnd, int nIndex, IntPtr dwNewLong);

    [DllImport("user32.dll")]
    private static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter, int x, int y, int cx, int cy, uint uFlags);

    [DllImport("dwmapi.dll")]
    private static extern int DwmSetWindowAttribute(IntPtr hwnd, int dwAttribute, ref int pvAttribute, int cbAttribute);

    private const int GwlStyle = -16;
    private const long WsCaption = 0x00C00000L;
    private const long WsThickFrame = 0x00040000L;
    private const long WsMinimizeBox = 0x00020000L;
    private const long WsMaximizeBox = 0x00010000L;
    private const long WsPopup = 0x80000000L;
    private const long WsSysMenu = 0x00080000L;
    private const uint SwpNoSize = 0x0001;
    private const uint SwpNoMove = 0x0002;
    private const uint SwpNoZOrder = 0x0004;
    private const uint SwpNoActivate = 0x0010;
    private const uint SwpFrameChanged = 0x0020;

    private const int DwmwaWindowCornerPreference = 33;
    private const int DwmwaBorderColor = 34;
    private const int DwmwcpRound = 2;
    private const int DwmwcpDoNotRound = 1;

    public static void SetAccentBorder(IntPtr hwnd)
    {
        try
        {
            System.Windows.Media.Color accent = AppTheme.GetSystemAccent();
            int colorRef = accent.R | (accent.G << 8) | (accent.B << 16);
            DwmSetWindowAttribute(hwnd, DwmwaBorderColor, ref colorRef, sizeof(int));
        }
        catch (Exception ex)
        {
            Logger.Warn($"Could not set accent window border: {ex.Message}");
        }
    }

    public static void SetCornerPreference(IntPtr hwnd, bool round)
    {
        try
        {
            int preference = round ? DwmwcpRound : DwmwcpDoNotRound;
            DwmSetWindowAttribute(hwnd, DwmwaWindowCornerPreference, ref preference, sizeof(int));
        }
        catch (Exception ex)
        {
            Logger.Warn($"Could not set corner preference: {ex.Message}");
        }
    }

    public static void StripWindowFrame(IntPtr hwnd)
    {
        try
        {
            long style = GetWindowLongPtr(hwnd, GwlStyle).ToInt64();
            long next = (style & ~(WsCaption | WsThickFrame | WsSysMenu | WsMinimizeBox | WsMaximizeBox)) | WsPopup;
            SetWindowLongPtr(hwnd, GwlStyle, new IntPtr(next));

            int noBorder = unchecked((int)0xFFFFFFFE);
            DwmSetWindowAttribute(hwnd, DwmwaBorderColor, ref noBorder, sizeof(int));

            int corner = DwmwcpDoNotRound;
            DwmSetWindowAttribute(hwnd, DwmwaWindowCornerPreference, ref corner, sizeof(int));

            SetWindowPos(hwnd, IntPtr.Zero, 0, 0, 0, 0, SwpNoSize | SwpNoMove | SwpNoZOrder | SwpNoActivate | SwpFrameChanged);
        }
        catch (Exception ex)
        {
            Logger.Warn($"Could not strip window frame: {ex.Message}");
        }
    }

    public static void SetTopmost(IntPtr hwnd, bool topmost)
    {
        try
        {
            IntPtr insert = new IntPtr(topmost ? -1 : -2);
            SetWindowPos(hwnd, insert, 0, 0, 0, 0, SwpNoSize | SwpNoMove | SwpNoActivate);
        }
        catch (Exception ex)
        {
            Logger.Warn($"Could not set window z-order: {ex.Message}");
        }
    }
}
