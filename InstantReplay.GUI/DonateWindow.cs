using System.Runtime.InteropServices;
using Microsoft.UI;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.Graphics;
using Windows.System;
using WinRT.Interop;
using InstantReplay.GUI.Helpers;
using InstantReplay.Models;
using InstantReplay.Services;

namespace InstantReplay.GUI;

public sealed class DonateWindow : Window
{
    [StructLayout(LayoutKind.Sequential)]
    private struct Rect
    {
        public int Left, Top, Right, Bottom;
    }

    [DllImport("user32.dll")] private static extern bool GetWindowRect(IntPtr hWnd, out Rect rect);
    [DllImport("user32.dll")] private static extern uint GetDpiForWindow(IntPtr hWnd);
    [DllImport("user32.dll")] private static extern int GetSystemMetrics(int nIndex);

    private const int SmCyFullscreen = 17;
    private const double WindowWidthDips = 480;
    private const double WindowHeightDips = 540;

    private static DonateWindow? _instance;

    public static void Show(IntPtr parentHwnd, Settings settings)
    {
        if (_instance != null)
        {
            try { _instance.Activate(); } catch { }
            return;
        }

        _instance = new DonateWindow(parentHwnd, settings);
        _instance.Closed += (_, _) => _instance = null;
        _instance.DispatcherQueue.TryEnqueue(() =>
        {
            try { _instance?.Activate(); }
            catch { }
        });
    }

    private DonateWindow(IntPtr parentHwnd, Settings settings)
    {
        Title = "Support Instant Replay";

        var view = new SupportView
        {
            RequestedTheme = ElementTheme.Dark,
        };
        view.CloseRequested += (_, _) => CloseWithFade(view);
        Content = view;

        Chrome.ApplyAcrylicBackdrop(this);

        IntPtr hwnd = WindowNative.GetWindowHandle(this);
        double scale = GetDpiForWindow(hwnd) / 96.0;
        if (scale <= 0) scale = 1.0;

        var appWindow = AppWindow.GetFromWindowId(Win32Interop.GetWindowIdFromWindow(hwnd));

        try { appWindow.SetIcon(ResolveIconPath()); }
        catch (Exception ex) { Logger.Warn($"Could not set donate window icon: {ex.Message}"); }

        if (appWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.IsResizable = false;
            presenter.IsMaximizable = false;
            presenter.IsMinimizable = false;
        }

        PositionOverParent(appWindow, parentHwnd, scale);
        SetupCustomTitleBar(appWindow, hwnd);
        TitleBarButtons.Apply(appWindow.TitleBar, true);
    }

    private void CloseWithFade(SupportView view)
    {
        try
        {
            var appWindow = AppWindow.GetFromWindowId(
                Win32Interop.GetWindowIdFromWindow(WindowNative.GetWindowHandle(this)));
            appWindow.Hide();
        }
        catch (Exception ex)
        {
            Logger.Warn($"Could not hide donate window before closing: {ex.Message}");
        }

        DispatcherQueue.TryEnqueue(() =>
        {
            try { Close(); }
            catch (Exception ex) { Logger.Warn($"Could not close donate window: {ex.Message}"); }
        });
    }

    private static void SetupCustomTitleBar(AppWindow appWindow, IntPtr hwnd)
    {
        var tb = appWindow.TitleBar;
        if (tb == null) return;

        try { tb.ExtendsContentIntoTitleBar = true; }
        catch (Exception ex) { Logger.Warn($"Could not extend donate title bar: {ex.Message}"); return; }

        try { tb.PreferredHeightOption = TitleBarHeightOption.Collapsed; }
        catch (Exception ex) { Logger.Warn($"Could not collapse donate title bar: {ex.Message}"); }

        double scale = GetDpiForWindow(hwnd) / 96.0;
        if (scale <= 0) scale = 1.0;

        int stripPx = (int)Math.Round((tb.Height > 0 ? tb.Height : 32) * scale);
        int captionPx = (int)Math.Round(46.0 * scale);
        int dragW = appWindow.ClientSize.Width - captionPx;

        try
        {
            if (dragW > 0 && stripPx > 0)
                tb.SetDragRectangles(new[] { new RectInt32(0, 0, dragW, stripPx) });
        }
        catch (Exception ex)
        {
            Logger.Warn($"Could not set donate title bar drag region: {ex.Message}");
        }

        TitleBarButtons.InstallNoCaptionFrame(hwnd);
    }

    private static void PositionOverParent(AppWindow appWindow, IntPtr parentHwnd, double scale)
    {
        int width = (int)Math.Round(WindowWidthDips * scale);
        int height = (int)Math.Round(WindowHeightDips * scale);
        int workH = GetSystemMetrics(SmCyFullscreen);
        if (height > workH - 12) height = Math.Max(480, workH - 12);

        int x = 0, y = 0;
        if (GetWindowRect(parentHwnd, out Rect parent))
        {
            x = parent.Left + ((parent.Right - parent.Left) - width) / 2;
            y = parent.Top + ((parent.Bottom - parent.Top) - height) / 3;
        }
        appWindow.MoveAndResize(new RectInt32(x, y, width, height));
    }

    private static string ResolveIconPath()
    {
        string[] candidates =
        {
            Path.Combine(AppContext.BaseDirectory, "assets", "icons", "icon.ico"),
            Path.Combine(AppContext.BaseDirectory, "assets", "icon.ico"),
            Path.Combine(AppContext.BaseDirectory, "icon.ico"),
        };
        foreach (string candidate in candidates)
        {
            if (File.Exists(candidate)) return candidate;
        }
        return candidates[0];
    }
}
