using System.Runtime.InteropServices;
using Microsoft.UI.Windowing;
using Windows.UI;
using InstantReplay.Services;

namespace InstantReplay.GUI.Helpers;

internal static class TitleBarButtons
{
    private delegate IntPtr SubclassProc(IntPtr hWnd, uint uMsg, IntPtr wParam, IntPtr lParam, IntPtr uIdSubclass, IntPtr dwRefData);

    [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW", CharSet = CharSet.Unicode)]
    private static extern IntPtr GetWindowLongPtr(IntPtr hWnd, int nIndex);

    [DllImport("user32.dll", EntryPoint = "SetWindowLongPtrW", CharSet = CharSet.Unicode)]
    private static extern IntPtr SetWindowLongPtr(IntPtr hWnd, int nIndex, IntPtr dwNewLong);

    [DllImport("user32.dll")]
    private static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter, int x, int y, int cx, int cy, uint uFlags);

    [DllImport("comctl32.dll")]
    private static extern bool SetWindowSubclass(IntPtr hWnd, SubclassProc pfnSubclass, uint uIdSubclass, IntPtr dwRefData);

    [DllImport("comctl32.dll")]
    private static extern IntPtr DefSubclassProc(IntPtr hWnd, uint uMsg, IntPtr wParam, IntPtr lParam);

    [DllImport("dwmapi.dll")]
    private static extern int DwmSetWindowAttribute(IntPtr hwnd, int dwAttribute, ref int pvAttribute, int cbAttribute);

    private const int GwlStyle = -16;
    private const long WsCaption = 0x00C00000L;
    private const long WsThickFrame = 0x00040000L;
    private const long WsMinimizeBox = 0x00020000L;
    private const long WsMaximizeBox = 0x00010000L;
    private const long WsPopup = 0x80000000L;
    private const uint WmNcCalcSize = 0x0083;
    private const uint SwpNoSize = 0x0001;
    private const uint SwpNoMove = 0x0002;
    private const uint SwpNoZOrder = 0x0004;
    private const uint SwpNoActivate = 0x0010;
    private const uint SwpFrameChanged = 0x0020;

    private const int DwmwaTransitionsForceDisabled = 3;
    private const long WsSysMenu = 0x00080000L;
    private const int DwmwaWindowCornerPreference = 33;
    private const int DwmwcpRound = 2;
    private const int DwmwcpDoNotRound = 1;

    private static SubclassProc? _subclassProc;

    private static IntPtr WindowSubclass(IntPtr hWnd, uint uMsg, IntPtr wParam, IntPtr lParam, IntPtr uIdSubclass, IntPtr dwRefData)
    {
        if (uMsg == WmNcCalcSize)
            return IntPtr.Zero;
        return DefSubclassProc(hWnd, uMsg, wParam, lParam);
    }

    public static void InstallNoCaptionFrame(IntPtr hwnd)
    {
        try
        {
            int allowTransitions = 0;
            DwmSetWindowAttribute(hwnd, DwmwaTransitionsForceDisabled, ref allowTransitions, sizeof(int));

            long style = GetWindowLongPtr(hwnd, GwlStyle).ToInt64();
            long want = WsCaption | WsThickFrame | WsSysMenu;
            if ((style & want) != want)
            {
                SetWindowLongPtr(hwnd, GwlStyle, new IntPtr(style | want));
                SetWindowPos(hwnd, IntPtr.Zero, 0, 0, 0, 0, SwpNoSize | SwpNoMove | SwpNoZOrder | SwpNoActivate | SwpFrameChanged);
            }

            if (_subclassProc == null)
            {
                _subclassProc = WindowSubclass;
                SetWindowSubclass(hwnd, _subclassProc, 0, IntPtr.Zero);
                SetWindowPos(hwnd, IntPtr.Zero, 0, 0, 0, 0, SwpNoSize | SwpNoMove | SwpNoZOrder | SwpNoActivate | SwpFrameChanged);
            }
        }
        catch (Exception ex)
        {
            Logger.Warn($"Could not install custom window frame: {ex.Message}");
        }
    }

    private const int DwmwaBorderColor = 34;

    public static void SetAccentBorder(IntPtr hwnd)
    {
        try
        {
            var accent = new Windows.UI.ViewManagement.UISettings()
                .GetColorValue(Windows.UI.ViewManagement.UIColorType.Accent);
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

    public static void Apply(AppWindowTitleBar? tb, bool dark)
    {
        if (tb == null) return;

        tb.ButtonBackgroundColor = Color.FromArgb(0, 0, 0, 0);

        if (dark)
        {
            tb.ButtonForegroundColor = Color.FromArgb(255, 255, 255, 255);
            tb.ButtonHoverBackgroundColor = Color.FromArgb(255, 76, 76, 76);
            tb.ButtonHoverForegroundColor = Color.FromArgb(255, 255, 255, 255);
            tb.ButtonPressedBackgroundColor = Color.FromArgb(255, 64, 64, 64);
            tb.ButtonPressedForegroundColor = Color.FromArgb(255, 255, 255, 255);
            tb.InactiveBackgroundColor = Color.FromArgb(0, 0, 0, 0);
            tb.InactiveForegroundColor = Color.FromArgb(140, 255, 255, 255);
        }
        else
        {
            tb.ButtonForegroundColor = Color.FromArgb(255, 24, 24, 24);
            tb.ButtonHoverBackgroundColor = Color.FromArgb(255, 228, 228, 228);
            tb.ButtonHoverForegroundColor = Color.FromArgb(255, 0, 0, 0);
            tb.ButtonPressedBackgroundColor = Color.FromArgb(255, 214, 214, 214);
            tb.ButtonPressedForegroundColor = Color.FromArgb(255, 0, 0, 0);
            tb.InactiveBackgroundColor = Color.FromArgb(0, 0, 0, 0);
            tb.InactiveForegroundColor = Color.FromArgb(130, 0, 0, 0);
        }
    }
}
